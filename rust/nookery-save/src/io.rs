//! Bounded filesystem recovery and staged synthetic-record writing.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;

use crate::error::{GciError, SaveResult};
use crate::layout::RECORD_SIZE;
use crate::path::{PENDING_SUFFIXES, PRESERVED_PENDING_SUFFIX, ROTATION_SUFFIXES, sibling_path};
use crate::record::TownRecord;

/// Read the base file, then pending and rotated companions in C order.
pub fn read_town_record_recovering(base: impl AsRef<Path>) -> Result<TownRecord, GciError> {
    let base = base.as_ref();
    let mut result = read_record_prefix(base);
    if result.is_ok() {
        return result;
    }

    for suffix in PENDING_SUFFIXES {
        let path = recovery_path(base, suffix)?;
        let candidate = read_record_prefix(&path);
        if candidate.is_ok() {
            return candidate;
        }
        result = merge_failure(result, candidate);
    }
    for suffix in &ROTATION_SUFFIXES[1..] {
        let path = recovery_path(base, suffix)?;
        let candidate = read_record_prefix(&path);
        if candidate.is_ok() {
            return candidate;
        }
        result = merge_failure(result, candidate);
    }
    result
}

fn recovery_path(base: &Path, suffix: &str) -> Result<std::path::PathBuf, GciError> {
    sibling_path(base, suffix).map_err(|_| GciError::Io(ErrorKind::InvalidInput))
}

/// Try card A first and card B second, preserving C's failure precedence.
pub fn read_town_record_from_slots(
    card_a: impl AsRef<Path>,
    card_b: impl AsRef<Path>,
) -> Result<TownRecord, GciError> {
    let first = read_town_record_recovering(card_a);
    if first.is_ok() {
        return first;
    }
    let second = read_town_record_recovering(card_b);
    if second.is_ok() {
        return second;
    }
    merge_failure(first, second)
}

/// Stage, sync, rotate, and install an already-validated synthetic GCI record.
///
/// This is the byte-level equivalent of the C writer's pending/backup path;
/// callers supply all bytes, so this function does not create game data.
pub fn write_town_record(path: impl AsRef<Path>, record: &[u8]) -> Result<(), GciError> {
    TownRecord::parse(record)?;
    let base = path.as_ref();
    let pending = sibling_path(base, ".write")?;
    let preserved = sibling_path(base, PRESERVED_PENDING_SUFFIX)?;
    let rotation = sibling_path(base, ".temp")?;

    preserve_pending(base, &pending, &preserved)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&pending)
        .map_err(map_io)?;
    file.write_all(record).map_err(map_io)?;
    file.sync_all().map_err(map_io)?;
    drop(file);

    remove_if_present(&rotation)?;
    for destination_index in (1..ROTATION_SUFFIXES.len()).rev() {
        let source = sibling_path(base, ROTATION_SUFFIXES[destination_index - 1])?;
        let destination = sibling_path(base, ROTATION_SUFFIXES[destination_index])?;
        move_if_present(&source, &destination)?;
    }
    fs::rename(&pending, base).map_err(map_io)?;
    remove_if_present(&rotation)?;
    remove_if_present(&preserved)?;
    Ok(())
}

fn read_record_prefix(path: &Path) -> Result<TownRecord, GciError> {
    let file = File::open(path).map_err(map_io)?;
    let mut buffer = Vec::with_capacity(RECORD_SIZE);
    file.take(RECORD_SIZE as u64)
        .read_to_end(&mut buffer)
        .map_err(map_io)?;
    TownRecord::parse_prefix(&buffer)
}

fn merge_failure(
    previous: Result<TownRecord, GciError>,
    next: Result<TownRecord, GciError>,
) -> Result<TownRecord, GciError> {
    let previous_error = match previous {
        Ok(record) => return Ok(record),
        Err(error) => error,
    };
    let next_error = match next {
        Ok(record) => return Ok(record),
        Err(error) => error,
    };
    if matches!(previous_error.as_save_result(), SaveResult::IoError)
        || matches!(next_error.as_save_result(), SaveResult::IoError)
    {
        Err(
            if matches!(previous_error.as_save_result(), SaveResult::IoError) {
                previous_error
            } else {
                next_error
            },
        )
    } else if previous_error.is_invalid() || next_error.is_invalid() {
        Err(if previous_error.is_invalid() {
            previous_error
        } else {
            next_error
        })
    } else {
        Err(GciError::Missing)
    }
}

fn preserve_pending(base: &Path, pending: &Path, preserved: &Path) -> Result<(), GciError> {
    match read_record_prefix(pending) {
        Ok(_) => {
            remove_if_present(preserved)?;
            move_if_present(pending, preserved)
        }
        Err(GciError::Missing) => Ok(()),
        Err(error) if error.is_invalid() => match read_town_record_recovering(base) {
            Ok(_) => remove_if_present(pending),
            Err(_) => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn remove_if_present(path: &Path) -> Result<(), GciError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(map_io(error)),
    }
}

fn move_if_present(source: &Path, destination: &Path) -> Result<(), GciError> {
    match fs::metadata(source) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(GciError::Io(ErrorKind::Other)),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(map_io(error)),
    }
    if fs::metadata(destination).is_ok() {
        return Err(GciError::Io(ErrorKind::AlreadyExists));
    }
    fs::rename(source, destination).map_err(map_io)
}

fn map_io(error: std::io::Error) -> GciError {
    if error.kind() == ErrorKind::NotFound {
        GciError::Missing
    } else {
        GciError::Io(error.kind())
    }
}
