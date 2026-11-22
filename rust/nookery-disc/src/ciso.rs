//! Sparse GameCube/Wii CISO logical-image mapping.
//!
//! This is the CISO variant used by the Dolphin/NOD disc path, not the
//! unrelated PSP CSO container. Its 0x8000-byte header contains a little-endian
//! block size followed by a 0x7ff8-byte presence map. Present logical blocks
//! are packed after the header; absent blocks read as zeroes. NOD exposes the
//! GameCube logical size as the fixed `0x57058000`-byte disc extent, whose last
//! block is partial.

use std::fmt;
use std::io::{Read, Seek};

use crate::image::{DiscImage, DiscStream, ImageError, StreamError};

/// CISO header and map length in bytes.
pub const HEADER_SIZE: usize = 0x8000;
/// Number of one-byte map entries in a CISO header.
pub const MAP_SIZE: usize = HEADER_SIZE - 8;
/// CISO block size field offset.
pub const BLOCK_SIZE_OFFSET: usize = 4;
/// First packed data block offset.
pub const DATA_OFFSET: u64 = HEADER_SIZE as u64;
/// GameCube CISO block size used by NOD.
pub const GAMECUBE_BLOCK_SIZE: u32 = 0x20_0000;
/// Fixed GameCube logical disc size returned by `nod_disc_size` for CISO.
pub const GAMECUBE_LOGICAL_SIZE: u64 = 0x5705_8000;
/// Number of CISO map blocks covering [`GAMECUBE_LOGICAL_SIZE`] at the
/// default [`GAMECUBE_BLOCK_SIZE`].
pub const GAMECUBE_LOGICAL_BLOCKS: usize = 697;
/// Smallest valid CISO block alignment, matching NOD's sector-size check.
pub const CISO_MIN_BLOCK_SIZE: u32 = 0x800;

/// Why a CISO header or block read was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CisoError {
    /// The source ended before the fixed CISO header.
    ShortHeader {
        /// Bytes available in the source.
        actual: u64,
    },
    /// The first four bytes were not `CISO`.
    WrongMagic,
    /// The block size is zero, not sector-aligned, or cannot cover the logical
    /// GameCube extent within the CISO map.
    InvalidBlockSize {
        /// Block size read from the header.
        block_size: u32,
    },
    /// A map byte other than zero or one was encountered.
    InvalidMapValue {
        /// Map index containing the invalid value.
        index: usize,
        /// Invalid byte value.
        value: u8,
    },
    /// The first logical block is absent, so a disc header cannot exist.
    MissingHeaderBlock,
    /// The logical or packed size overflowed a checked calculation.
    Overflow,
    /// The source ended before all present blocks.
    ShortData {
        /// Source length required by the packed blocks.
        needed: u64,
        /// Source length available.
        actual: u64,
    },
    /// The underlying reader failed.
    Io,
}

impl fmt::Display for CisoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortHeader { actual } => {
                write!(formatter, "CISO header truncated: {actual} bytes available")
            }
            Self::WrongMagic => formatter.write_str("CISO magic is missing"),
            Self::InvalidBlockSize { block_size } => {
                write!(formatter, "invalid CISO block size {block_size:#x}")
            }
            Self::InvalidMapValue { index, value } => {
                write!(formatter, "invalid CISO map byte {value:#x} at {index}")
            }
            Self::MissingHeaderBlock => formatter.write_str("CISO logical block zero is absent"),
            Self::Overflow => formatter.write_str("CISO size calculation overflows"),
            Self::ShortData { needed, actual } => {
                write!(
                    formatter,
                    "CISO data truncated: need {needed}, have {actual}"
                )
            }
            Self::Io => formatter.write_str("CISO source I/O failed"),
        }
    }
}

impl std::error::Error for CisoError {}

/// Checked metadata for a GameCube/Wii CISO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CisoMetadata {
    /// Little-endian block size from offset four.
    pub block_size: u32,
    /// Logical block count covering the fixed GameCube extent at this block size.
    pub logical_blocks: u64,
    /// Number of present blocks packed in the source.
    pub present_blocks: u64,
    /// Logical image size in bytes.
    pub logical_size: u64,
}

/// A checked, random-access logical view over a sparse CISO source.
pub struct CisoImage<'a, R> {
    source: DiscStream<&'a mut R>,
    map: Vec<u8>,
    metadata: CisoMetadata,
}

impl<'a, R: Read + Seek> CisoImage<'a, R> {
    /// Opens and validates a CISO source without materializing its logical
    /// image. All map bytes and all packed-block bounds are checked up front.
    pub fn open(reader: &'a mut R) -> Result<Self, CisoError> {
        let mut source = DiscStream::open(reader).map_err(stream_error)?;
        let source_size = source.len();
        if source_size < HEADER_SIZE as u64 {
            return Err(CisoError::ShortHeader {
                actual: source_size,
            });
        }

        let mut header = [0u8; HEADER_SIZE];
        read_exact_at(&mut source, 0, &mut header)?;
        if &header[..4] != b"CISO" {
            return Err(CisoError::WrongMagic);
        }
        let block_size = u32::from_le_bytes(
            header[BLOCK_SIZE_OFFSET..BLOCK_SIZE_OFFSET + 4]
                .try_into()
                .expect("fixed CISO block-size field"),
        );
        if block_size == 0 || block_size % CISO_MIN_BLOCK_SIZE != 0 {
            return Err(CisoError::InvalidBlockSize { block_size });
        }

        let logical_blocks = GAMECUBE_LOGICAL_SIZE
            .checked_add(u64::from(block_size) - 1)
            .ok_or(CisoError::Overflow)?
            / u64::from(block_size);
        let logical_blocks_usize =
            usize::try_from(logical_blocks).map_err(|_| CisoError::Overflow)?;
        if logical_blocks_usize > MAP_SIZE {
            return Err(CisoError::InvalidBlockSize { block_size });
        }

        let map = header[8..].to_vec();
        let mut present_blocks = 0u64;
        for (index, &value) in map[..logical_blocks_usize].iter().enumerate() {
            match value {
                0 => {}
                1 => {
                    present_blocks = present_blocks.checked_add(1).ok_or(CisoError::Overflow)?;
                }
                value => return Err(CisoError::InvalidMapValue { index, value }),
            }
        }
        if map[0] == 0 {
            return Err(CisoError::MissingHeaderBlock);
        }
        let logical_size = GAMECUBE_LOGICAL_SIZE;
        let packed_size = present_blocks
            .checked_mul(u64::from(block_size))
            .ok_or(CisoError::Overflow)?;
        let needed = DATA_OFFSET
            .checked_add(packed_size)
            .ok_or(CisoError::Overflow)?;
        if needed > source_size {
            return Err(CisoError::ShortData {
                needed,
                actual: source_size,
            });
        }

        Ok(Self {
            source,
            map,
            metadata: CisoMetadata {
                block_size,
                logical_blocks,
                present_blocks,
                logical_size,
            },
        })
    }

    /// Returns checked CISO metadata.
    pub const fn metadata(&self) -> &CisoMetadata {
        &self.metadata
    }

    /// Returns the map byte for a logical block.
    pub fn map_entry(&self, block: u64) -> Option<u8> {
        self.map.get(usize::try_from(block).ok()?).copied()
    }

    fn packed_block_index(&self, block: usize) -> Result<u64, ImageError> {
        let present_before = self.map[..block]
            .iter()
            .filter(|&&value| value == 1)
            .count();
        u64::try_from(present_before)
            .ok()
            .and_then(|index| index.checked_mul(u64::from(self.metadata.block_size)))
            .and_then(|offset| DATA_OFFSET.checked_add(offset))
            .ok_or(ImageError::Failed)
    }
}

impl<R: Read + Seek> DiscImage for CisoImage<'_, R> {
    fn logical_size(&self) -> u64 {
        self.metadata.logical_size
    }

    fn read_at(&mut self, offset: u64, output: &mut [u8]) -> Result<usize, ImageError> {
        if output.is_empty() || offset >= self.metadata.logical_size {
            return Ok(0);
        }
        let available = self.metadata.logical_size - offset;
        let mut remaining = usize::try_from(available)
            .unwrap_or(usize::MAX)
            .min(output.len());
        let mut logical_offset = offset;
        let mut written = 0usize;
        let block_size = u64::from(self.metadata.block_size);
        while remaining != 0 {
            let block =
                usize::try_from(logical_offset / block_size).map_err(|_| ImageError::Failed)?;
            let within =
                usize::try_from(logical_offset % block_size).map_err(|_| ImageError::Failed)?;
            let count = remaining.min(self.metadata.block_size as usize - within);
            if self.map[block] == 0 {
                output[written..written + count].fill(0);
            } else {
                let physical = self.packed_block_index(block)?;
                let read = self
                    .source
                    .read_at(
                        physical + within as u64,
                        &mut output[written..written + count],
                    )
                    .map_err(|error| match error {
                        StreamError::Closed => ImageError::Failed,
                        StreamError::Io(_) | StreamError::PositionOverflow => ImageError::Io,
                    })?;
                if read != count {
                    return Err(ImageError::Io);
                }
            }
            written += count;
            remaining -= count;
            logical_offset += count as u64;
        }
        Ok(written)
    }
}

fn read_exact_at<R: Read + Seek>(
    source: &mut DiscStream<R>,
    offset: u64,
    output: &mut [u8],
) -> Result<(), CisoError> {
    let mut filled = 0;
    while filled < output.len() {
        let read = source
            .read_at(offset + filled as u64, &mut output[filled..])
            .map_err(stream_error)?;
        if read == 0 {
            return Err(CisoError::ShortData {
                needed: offset + output.len() as u64,
                actual: source.len(),
            });
        }
        filled += read;
    }
    Ok(())
}

fn stream_error(error: StreamError) -> CisoError {
    match error {
        StreamError::Closed | StreamError::Io(_) | StreamError::PositionOverflow => CisoError::Io,
    }
}
