//! Bounded container inspection and file-backed logical-image verification.
//!
//! RVZ parsing stops at checked header metadata because the Aurora/NOD RVZ
//! decoder is not present in this worktree. GameCube/Wii CISO is self-contained:
//! its sparse block map is reconstructed into a logical read view and sent
//! through the normal header/hash engine. Raw ISO/GCM images are logical views
//! already.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::catalog::Catalog;
use crate::ciso::{self, CisoError, CisoImage};
use crate::engine;
use crate::header::{self, Header};
use crate::image::{DiscImage, ImageError};
use crate::metadata::{Cancellation, Progress, Result};

/// The four content formats exposed by the PC disc route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    /// Raw ISO image (the extension is a routing hint; the header is checked).
    Iso,
    /// Raw GameCube image, commonly using the `.gcm` extension.
    Gcm,
    /// RVZ compressed image.
    Rvz,
    /// CISO compressed image.
    Ciso,
}

/// Failure classes for bounded stream inspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamVerifyError {
    /// The source ended before the required header or index metadata.
    ShortRead,
    /// The source is not a valid instance of the detected format.
    InvalidHeader,
    /// The requested operation was cancelled.
    Cancelled,
    /// The format is valid, but its decoder is not available in this crate.
    DecoderUnavailable(ContainerKind),
    /// The source path is not a regular, non-symlink file.
    RejectedPath,
    /// An underlying I/O operation failed.
    Io,
}

impl fmt::Display for StreamVerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortRead => formatter.write_str("source ended before the required metadata"),
            Self::InvalidHeader => formatter.write_str("invalid disc-container header"),
            Self::Cancelled => formatter.write_str("disc inspection was canceled"),
            Self::DecoderUnavailable(kind) => {
                write!(formatter, "decoder unavailable for {kind:?} container")
            }
            Self::RejectedPath => formatter.write_str("disc path is not a regular file"),
            Self::Io => formatter.write_str("disc source I/O failed"),
        }
    }
}

impl std::error::Error for StreamVerifyError {}

/// RVZ magic from the RVZ/WIA container family.
pub const RVZ_MAGIC: [u8; 4] = *b"RVZ\0";
/// GameCube/Wii CISO magic.
pub const CISO_MAGIC: [u8; 4] = *b"CISO";
/// Fixed GameCube/Wii CISO header length.
pub const CISO_HEADER_SIZE: usize = ciso::HEADER_SIZE;
/// Number of GameCube/Wii CISO map bytes.
pub const CISO_MAP_SIZE: usize = ciso::MAP_SIZE;
/// Minimum CISO block size supported by the on-disk format.
pub const CISO_MIN_BLOCK_SIZE: u32 = ciso::GAMECUBE_BLOCK_SIZE;

/// Metadata obtained without decoding or loading a container into memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerMetadata {
    /// Detected format.
    pub kind: ContainerKind,
    /// Bytes in the source file.
    pub source_size: u64,
    /// Logical image size when the format exposes it without decoding.
    pub logical_size: Option<u64>,
    /// Parsed raw disc header, when the source itself is a logical image.
    pub header: Option<Header>,
    /// CISO block size, when applicable.
    pub block_size: Option<u32>,
    /// Number of CISO index entries implied by the declared logical size.
    pub index_entries: Option<u64>,
}

/// Inspect a seekable source using fixed-size reads and checked metadata bounds.
pub fn inspect_stream<R: Read + Seek>(
    reader: &mut R,
    source_size: u64,
    cancel: &dyn Cancellation,
    kind_hint: Option<ContainerKind>,
) -> std::result::Result<ContainerMetadata, StreamVerifyError> {
    if cancel.is_canceled() {
        return Err(StreamVerifyError::Cancelled);
    }
    if source_size < RVZ_MAGIC.len() as u64 {
        return Err(StreamVerifyError::ShortRead);
    }

    let mut prefix = [0u8; RVZ_MAGIC.len()];
    read_exact_cancelable(reader, &mut prefix, cancel)?;
    if prefix == RVZ_MAGIC {
        return Ok(ContainerMetadata {
            kind: ContainerKind::Rvz,
            source_size,
            logical_size: None,
            header: None,
            block_size: None,
            index_entries: None,
        });
    }
    if prefix == CISO_MAGIC {
        return inspect_ciso(reader, source_size, cancel);
    }

    let raw_kind = match kind_hint {
        Some(ContainerKind::Gcm) => ContainerKind::Gcm,
        _ => ContainerKind::Iso,
    };
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|_| StreamVerifyError::Io)?;
    let mut raw_header = [0u8; header::HEADER_SIZE];
    read_exact_cancelable(reader, &mut raw_header, cancel)?;
    let decoded = header::decode(&raw_header).map_err(|_| StreamVerifyError::InvalidHeader)?;
    if decoded.platform == crate::Platform::Unknown {
        return Err(StreamVerifyError::InvalidHeader);
    }
    Ok(ContainerMetadata {
        kind: raw_kind,
        source_size,
        logical_size: Some(source_size),
        header: Some(decoded),
        block_size: None,
        index_entries: None,
    })
}

/// Inspect a path after rejecting a symlink or non-regular final entry.
pub fn inspect_path(
    path: &Path,
    cancel: &dyn Cancellation,
) -> std::result::Result<ContainerMetadata, StreamVerifyError> {
    let mut file = open_regular_file(path)?;
    let source_size = file.metadata().map_err(|_| StreamVerifyError::Io)?.len();
    inspect_stream(&mut file, source_size, cancel, extension_kind(path))
}

/// Verify an ISO/GCM or GameCube/Wii CISO path with bounded reads and the
/// existing catalog. RVZ remains an explicit decoder-unavailable boundary;
/// compressed bytes are never hashed as if they were a logical image.
pub fn verify_path(
    path: &Path,
    catalog: &Catalog,
    cancel: &dyn Cancellation,
    on_progress: &mut dyn FnMut(Progress),
) -> Result {
    let mut file = match open_regular_file(path) {
        Ok(file) => file,
        Err(error) => return mapped_error(error),
    };
    let source_size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => return mapped_error(StreamVerifyError::Io),
    };
    let kind_hint = extension_kind(path);
    let metadata = match inspect_stream(&mut file, source_size, cancel, kind_hint) {
        Ok(metadata) => metadata,
        Err(error) => return mapped_error(error),
    };
    if metadata.kind == ContainerKind::Rvz {
        return mapped_error(StreamVerifyError::DecoderUnavailable(metadata.kind));
    }

    if metadata.kind == ContainerKind::Ciso {
        let mut image = match CisoImage::open(&mut file) {
            Ok(image) => image,
            Err(error) => return mapped_error(ciso_error(error)),
        };
        return engine::verify(&mut image, catalog, cancel, on_progress);
    }

    let mut image = FileImage {
        file,
        size: source_size,
    };
    engine::verify(&mut image, catalog, cancel, on_progress)
}

fn inspect_ciso<R: Read + Seek>(
    reader: &mut R,
    source_size: u64,
    cancel: &dyn Cancellation,
) -> std::result::Result<ContainerMetadata, StreamVerifyError> {
    let mut image = CisoImage::open(reader).map_err(ciso_error)?;
    if cancel.is_canceled() {
        return Err(StreamVerifyError::Cancelled);
    }
    let mut raw_header = [0u8; header::HEADER_SIZE];
    read_image_exact(&mut image, &mut raw_header)?;
    let decoded = header::decode(&raw_header).map_err(|_| StreamVerifyError::InvalidHeader)?;
    if decoded.platform == crate::Platform::Unknown {
        return Err(StreamVerifyError::InvalidHeader);
    }
    let ciso = image.metadata().clone();

    Ok(ContainerMetadata {
        kind: ContainerKind::Ciso,
        source_size,
        logical_size: Some(ciso.logical_size),
        header: Some(decoded),
        block_size: Some(ciso.block_size),
        index_entries: Some(ciso.logical_blocks),
    })
}

fn read_image_exact(
    image: &mut dyn DiscImage,
    output: &mut [u8],
) -> std::result::Result<(), StreamVerifyError> {
    let mut filled = 0;
    while filled < output.len() {
        let read = image
            .read_at(filled as u64, &mut output[filled..])
            .map_err(|_| StreamVerifyError::Io)?;
        if read == 0 {
            return Err(StreamVerifyError::ShortRead);
        }
        filled += read;
    }
    Ok(())
}

fn ciso_error(error: CisoError) -> StreamVerifyError {
    match error {
        CisoError::ShortHeader { .. } | CisoError::ShortData { .. } => StreamVerifyError::ShortRead,
        CisoError::Io => StreamVerifyError::Io,
        CisoError::WrongMagic
        | CisoError::InvalidBlockSize { .. }
        | CisoError::InvalidMapValue { .. }
        | CisoError::MissingHeaderBlock
        | CisoError::Overflow => StreamVerifyError::InvalidHeader,
    }
}

fn read_exact_cancelable<R: Read>(
    reader: &mut R,
    buffer: &mut [u8],
    cancel: &dyn Cancellation,
) -> std::result::Result<(), StreamVerifyError> {
    let mut filled = 0;
    while filled < buffer.len() {
        if cancel.is_canceled() {
            return Err(StreamVerifyError::Cancelled);
        }
        let read = reader
            .read(&mut buffer[filled..])
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => StreamVerifyError::ShortRead,
                _ => StreamVerifyError::Io,
            })?;
        if read == 0 {
            return Err(StreamVerifyError::ShortRead);
        }
        filled += read;
    }
    Ok(())
}

fn extension_kind(path: &Path) -> Option<ContainerKind> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("gcm") => Some(ContainerKind::Gcm),
        Some(extension) if extension.eq_ignore_ascii_case("iso") => Some(ContainerKind::Iso),
        _ => None,
    }
}

fn open_regular_file(path: &Path) -> std::result::Result<File, StreamVerifyError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StreamVerifyError::Io)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(StreamVerifyError::RejectedPath);
    }
    File::open(path).map_err(|_| StreamVerifyError::Io)
}

fn mapped_error(error: StreamVerifyError) -> Result {
    let status = match error {
        StreamVerifyError::ShortRead | StreamVerifyError::InvalidHeader => {
            crate::Status::InvalidImage
        }
        StreamVerifyError::Cancelled => crate::Status::Canceled,
        StreamVerifyError::DecoderUnavailable(_) => crate::Status::Failed,
        StreamVerifyError::RejectedPath | StreamVerifyError::Io => crate::Status::IOError,
    };
    Result {
        status,
        message: error.to_string(),
        ..Result::default()
    }
}

struct FileImage {
    file: File,
    size: u64,
}

impl DiscImage for FileImage {
    fn logical_size(&self) -> u64 {
        self.size
    }

    fn read_at(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
    ) -> std::result::Result<usize, ImageError> {
        if offset >= self.size || buffer.is_empty() {
            return Ok(0);
        }
        let remaining = self.size - offset;
        let requested = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| ImageError::Io)?;
        self.file
            .read(&mut buffer[..requested])
            .map_err(|_| ImageError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn ciso_bytes(block_size: usize, map: &[u8], blocks: &[&[u8]]) -> Vec<u8> {
        let mut bytes = vec![0u8; CISO_HEADER_SIZE];
        bytes[..4].copy_from_slice(&CISO_MAGIC);
        bytes[4..8].copy_from_slice(&(block_size as u32).to_le_bytes());
        bytes[8..8 + map.len()].copy_from_slice(map);
        for block in blocks {
            assert_eq!(block.len(), block_size);
            bytes.extend_from_slice(block);
        }
        bytes
    }

    #[test]
    fn rvz_short_header_is_a_short_read() {
        let mut source = Cursor::new(b"RVZ");
        assert_eq!(
            inspect_stream(&mut source, 3, &crate::Never, None),
            Err(StreamVerifyError::ShortRead)
        );
    }

    #[test]
    fn valid_rvz_metadata_is_bounded_and_does_not_claim_a_logical_size() {
        let mut source = Cursor::new(RVZ_MAGIC);
        let metadata = inspect_stream(&mut source, 4, &crate::Never, None).expect("RVZ metadata");
        assert_eq!(metadata.kind, ContainerKind::Rvz);
        assert_eq!(metadata.logical_size, None);
    }

    #[test]
    fn raw_gcm_reads_the_disc_header_and_reports_logical_size() {
        let mut bytes = vec![0u8; header::HEADER_SIZE];
        bytes[..6].copy_from_slice(b"GAFE01");
        bytes[28..32].copy_from_slice(&header::GCN_MAGIC);
        let title = b"synthetic stream fixture";
        bytes[32..32 + title.len()].copy_from_slice(title);
        let mut source = Cursor::new(bytes.clone());
        let metadata = inspect_stream(
            &mut source,
            bytes.len() as u64,
            &crate::Never,
            Some(ContainerKind::Gcm),
        )
        .expect("GCM metadata");
        assert_eq!(metadata.kind, ContainerKind::Gcm);
        assert_eq!(metadata.logical_size, Some(header::HEADER_SIZE as u64));
        assert_eq!(metadata.header.expect("raw header").game_id, *b"GAFE01");
    }

    #[test]
    fn cancellation_is_observed_before_any_header_read() {
        let flag = crate::CancelFlag::new();
        flag.cancel();
        let mut source = Cursor::new(RVZ_MAGIC);
        assert_eq!(
            inspect_stream(&mut source, 4, &flag, None),
            Err(StreamVerifyError::Cancelled)
        );
    }

    #[test]
    fn malformed_ciso_version_is_an_invalid_header() {
        let mut bytes = vec![0u8; CISO_HEADER_SIZE];
        bytes[..4].copy_from_slice(&CISO_MAGIC);
        bytes[4..8].copy_from_slice(&0x100u32.to_le_bytes());
        bytes[8] = 1;
        let mut source = Cursor::new(bytes);
        assert_eq!(
            inspect_stream(&mut source, CISO_HEADER_SIZE as u64, &crate::Never, None),
            Err(StreamVerifyError::InvalidHeader)
        );
    }

    #[test]
    fn verify_path_reports_decoder_unavailable_instead_of_hashing_rvz_bytes() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nookery-disc-rvz-test-{}-{nonce}.rvz",
            std::process::id()
        ));
        std::fs::write(&path, RVZ_MAGIC).expect("write synthetic RVZ header");
        let mut progress = Vec::new();
        let result = verify_path(&path, Catalog::upstream(), &crate::Never, &mut |sample| {
            progress.push(sample)
        });
        std::fs::remove_file(&path).expect("remove synthetic RVZ fixture");
        assert_eq!(result.status, crate::Status::Failed);
        assert!(result.message.contains("decoder unavailable"));
        assert!(
            progress.is_empty(),
            "decoder boundary precedes hash progress"
        );
    }

    #[test]
    fn ciso_metadata_is_checked_without_allocating_an_index() {
        let mut block0 = vec![0u8; ciso::GAMECUBE_BLOCK_SIZE as usize];
        block0[..6].copy_from_slice(b"GAFE01");
        block0[28..32].copy_from_slice(&crate::header::GCN_MAGIC);
        let block1 = vec![0u8; ciso::GAMECUBE_BLOCK_SIZE as usize];
        let bytes = ciso_bytes(
            ciso::GAMECUBE_BLOCK_SIZE as usize,
            &[1, 1],
            &[&block0, &block1],
        );
        let mut source = Cursor::new(bytes);
        assert_eq!(
            inspect_stream(
                &mut source,
                (CISO_HEADER_SIZE + 2 * ciso::GAMECUBE_BLOCK_SIZE as usize) as u64,
                &crate::Never,
                None,
            ),
            Ok(ContainerMetadata {
                kind: ContainerKind::Ciso,
                source_size: (CISO_HEADER_SIZE + 2 * ciso::GAMECUBE_BLOCK_SIZE as usize) as u64,
                logical_size: Some(ciso::GAMECUBE_LOGICAL_SIZE),
                header: Some(crate::Header {
                    game_id: *b"GAFE01",
                    disc_number: 0,
                    revision: 0,
                    title: Vec::new(),
                    platform: crate::Platform::GameCube,
                }),
                block_size: Some(ciso::GAMECUBE_BLOCK_SIZE),
                index_entries: Some(ciso::GAMECUBE_LOGICAL_BLOCKS as u64),
            })
        );
    }

    #[test]
    fn ciso_truncated_index_is_a_short_read() {
        let mut bytes = vec![0u8; CISO_HEADER_SIZE];
        bytes[..4].copy_from_slice(&CISO_MAGIC);
        bytes[4..8].copy_from_slice(&ciso::GAMECUBE_BLOCK_SIZE.to_le_bytes());
        bytes[8] = 1;
        bytes[9] = 1;
        bytes.extend_from_slice(&vec![0u8; ciso::GAMECUBE_BLOCK_SIZE as usize - 1]);
        let mut source = Cursor::new(bytes);
        assert_eq!(
            inspect_stream(
                &mut source,
                (CISO_HEADER_SIZE + ciso::GAMECUBE_BLOCK_SIZE as usize - 1) as u64,
                &crate::Never,
                None
            ),
            Err(StreamVerifyError::ShortRead)
        );
    }
}
