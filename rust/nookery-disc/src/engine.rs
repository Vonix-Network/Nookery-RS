//! Inspection and verification, ported from
//! `extern/borealis/src/disc.cpp::open_and_inspect` and `verify_owned_stream`.

use crate::catalog::Catalog;
use crate::error::{ValidationError, validation_error};
use crate::hash::Hasher;
use crate::header::{self, HEADER_OFFSET};
use crate::image::{DiscImage, ImageError};
use crate::metadata::{Cancellation, Metadata, Never, Progress, Result};
use crate::status::Status;

/// Verification chunk size (`detail::HashBufferSize`).
pub const HASH_BUFFER_SIZE: usize = 1024 * 1024;

/// Maps a backend read failure the way `status_from_nod_result` does.
const fn status_from_image_error(error: ImageError) -> Status {
    match error {
        ImageError::Io => Status::IOError,
        ImageError::Format => Status::InvalidImage,
        ImageError::Failed => Status::Failed,
    }
}

fn failed(error: ImageError, message: &str) -> Result {
    Result {
        status: status_from_image_error(error),
        message: message.to_owned(),
        ..Result::default()
    }
}

/// Reads the fixed-size header into `buf`, returning the byte count read.
///
/// Bounded by construction: exactly `HEADER_SIZE` bytes at offset 0, so no
/// length field can drive the read.  A backend that reports more bytes than the
/// slice it was given is rejected as [`ImageError::Failed`] rather than clamped:
/// clamping would silently hash or decode bytes the adapter never wrote.
fn read_header(
    image: &mut dyn DiscImage,
    buf: &mut [u8; header::HEADER_SIZE],
) -> std::result::Result<usize, ImageError> {
    let mut filled = 0;
    while filled < buf.len() {
        let offset = HEADER_OFFSET + filled as u64;
        let read = image.read_at(offset, &mut buf[filled..])?;
        if read == 0 {
            break;
        }
        let remaining = buf.len() - filled;
        if read > remaining {
            return Err(ImageError::Failed);
        }
        filled += read;
    }
    Ok(filled)
}

/// Opens and classifies an image, mirroring `open_and_inspect`.
///
/// A truncated header is reported as [`Status::InvalidImage`]: `decode` requires
/// the full 0x400-byte header, and no accepted record can be matched without it.
fn open_and_inspect(image: &mut dyn DiscImage, catalog: &Catalog) -> Result {
    let logical_size = image.logical_size();
    let mut buf = [0u8; header::HEADER_SIZE];
    let read = match read_header(image, &mut buf) {
        Ok(read) => read,
        Err(error) => return failed(error, "Failed to read disc header"),
    };
    let decoded = match header::decode(&buf[..read]) {
        Ok(decoded) => decoded,
        Err(_) => return failed(ImageError::Format, "Failed to read disc header"),
    };

    let metadata = Metadata::from_header(&decoded, logical_size);
    let mut result = Result {
        status: Status::Failed,
        metadata,
        accepted_disc_index: None,
        message: String::new(),
    };

    if result.metadata.platform == header::Platform::Unknown {
        result.status = Status::InvalidImage;
        result.message = "Disc header contains no recognized platform magic".to_owned();
        return result;
    }

    result.accepted_disc_index = catalog.accepted_index(&decoded);
    result.status = if result.accepted_disc_index.is_some() {
        Status::Success
    } else if catalog.is_recognized_game(&decoded) {
        Status::UnsupportedVersion
    } else {
        Status::UnknownGame
    };
    result
}

fn canceled() -> Result {
    Result {
        status: Status::Canceled,
        ..Result::default()
    }
}

/// Inspects an image header without hashing it.
///
/// Mirrors `borealis::disc::inspect`.  `progress` is not touched because
/// upstream resets and reports progress only in `verify`.
pub fn inspect(image: &mut dyn DiscImage, catalog: &Catalog) -> Result {
    open_and_inspect(image, catalog)
}

/// Fully verifies an image against the catalog, mirroring
/// `borealis::disc::verify`.
///
/// `on_progress` receives a sample after every chunk and once with the total
/// size before hashing; cancellation is polled once before opening and before
/// every chunk, exactly where `verify_owned_stream` polls it.
pub fn verify(
    image: &mut dyn DiscImage,
    catalog: &Catalog,
    cancel: &dyn Cancellation,
    on_progress: &mut dyn FnMut(Progress),
) -> Result {
    on_progress(Progress::default());
    if cancel.is_canceled() {
        return canceled();
    }

    let mut opened = open_and_inspect(image, catalog);
    if opened.status != Status::Success {
        return opened;
    }

    let total = opened.metadata.logical_size;
    on_progress(Progress {
        bytes_read: 0,
        bytes_total: total,
    });

    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; HASH_BUFFER_SIZE];
    let mut offset = 0u64;
    loop {
        if cancel.is_canceled() {
            return canceled();
        }
        let read = match image.read_at(offset, &mut buffer) {
            Ok(read) => read,
            Err(error) => {
                opened.status = status_from_image_error(error);
                opened.message = "Failed while reading disc image".to_owned();
                return opened;
            }
        };
        if read == 0 {
            break;
        }
        // Mirrors `verify_owned_stream`'s `read > HashBufferSize` arm: a backend
        // that over-reports is a backend bug, and upstream reports `Failed`
        // rather than silently hashing a clamped byte count.
        if read > buffer.len() {
            opened.status = Status::Failed;
            opened.message = "Disc backend returned an oversized read".to_owned();
            return opened;
        }
        hasher.update(&buffer[..read]);
        offset += read as u64;
        on_progress(Progress {
            bytes_read: offset,
            bytes_total: total,
        });
    }

    if offset != total {
        opened.status = Status::IOError;
        opened.message = "Disc image ended before its reported logical size".to_owned();
        return opened;
    }

    let Some(index) = opened.accepted_disc_index else {
        // `open_and_inspect` only reports `Success` with an accepted index, so
        // this is unreachable; keep it total instead of indexing unchecked.
        opened.status = Status::Failed;
        opened.message = "Accepted disc record missing for verified image".to_owned();
        return opened;
    };
    let expected = catalog.accepted_discs[index].expected_hash;
    opened.status = if hasher.finish() == expected {
        Status::Success
    } else {
        Status::HashMismatch
    };
    opened
}

/// [`verify`] without progress reporting.
pub fn verify_simple(image: &mut dyn DiscImage, catalog: &Catalog) -> Result {
    verify(image, catalog, &Never, &mut |_| {})
}

/// Convenience wrapper: inspect then map to the PC port's category.
pub fn inspect_error(image: &mut dyn DiscImage, catalog: &Catalog) -> ValidationError {
    validation_error(inspect(image, catalog).status)
}
