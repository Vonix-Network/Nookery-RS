//! Bounded path derivation for a save file and its recovery companions.
//!
//! Ported from `pc_gci_sibling_path` plus the suffix tables in
//! `pc/src/pc_gci.c`.  Upstream appends a fixed suffix to a 1024-byte buffer
//! and treats `snprintf` truncation as failure; this module keeps that bound
//! and additionally refuses suffixes that could redirect the path.

use std::path::{Path, PathBuf};

use crate::error::GciError;

/// `PC_GCI_PATH_MAX`: the buffer bound used by every upstream path helper.
pub const PATH_MAX: usize = 1024;

/// `PC_GCI_FILENAME`: the default town save file name.
pub const DEFAULT_FILE_NAME: &str = "DobutsunomoriP_MURA.gci";

/// `pc_gci_pending_suffixes`, in read order.
pub const PENDING_SUFFIXES: [&str; 3] = [".write", ".write.temp", ".tmp"];

/// `pc_gci_rotation_suffixes`, in read order (index 0 is the base file).
pub const ROTATION_SUFFIXES: [&str; 5] = ["", ".bak1", ".bak2", ".bak3", ".temp"];

/// `PC_SAVE_PENDING_SUFFIX`.
pub const PENDING_SUFFIX: &str = ".write";
/// `PC_SAVE_PRESERVED_PENDING_SUFFIX`.
pub const PRESERVED_PENDING_SUFFIX: &str = ".write.temp";
/// `PC_SAVE_ROTATION_SUFFIX`.
pub const ROTATION_SUFFIX: &str = ".temp";
/// `PC_SAVE_LEGACY_PENDING_SUFFIX`.
pub const LEGACY_PENDING_SUFFIX: &str = ".tmp";

/// Append `suffix` to `base` under the upstream length bound.
///
/// Rejects suffixes that are absolute or contain a separator, a parent
/// component, or a NUL byte: upstream suffixes are compile-time constants, so
/// such a suffix is caller error rather than data.  The result is a plain
/// concatenation — no canonicalisation, no filesystem access.
pub fn sibling_path(base: &Path, suffix: &str) -> Result<PathBuf, GciError> {
    if suffix.contains('/')
        || suffix.contains('\\')
        || suffix.contains('\0')
        || suffix.starts_with("..")
    {
        return Err(GciError::InvalidSuffix);
    }
    let base_len = base.as_os_str().len();
    if base_len + suffix.len() >= PATH_MAX {
        return Err(GciError::PathTooLong);
    }
    let mut joined = base.as_os_str().to_os_string();
    joined.push(suffix);
    Ok(PathBuf::from(joined))
}

/// Candidate paths tried by `pc_gci_read`, in order: the base file, the three
/// pending suffixes, then the four rotation backups.
pub fn recovery_candidates(base: &Path) -> Result<Vec<PathBuf>, GciError> {
    let mut paths = Vec::with_capacity(1 + PENDING_SUFFIXES.len() + ROTATION_SUFFIXES.len() - 1);
    paths.push(sibling_path(base, "")?);
    for suffix in PENDING_SUFFIXES {
        paths.push(sibling_path(base, suffix)?);
    }
    for suffix in &ROTATION_SUFFIXES[1..] {
        paths.push(sibling_path(base, suffix)?);
    }
    Ok(paths)
}

/// Validate a `CARDDir.fileName` value as a name rather than a path.
///
/// Upstream only ever compares this field to a constant and never joins it
/// into a path, so a value with separators, `..`, or an interior NUL is a
/// rejected record, not a rejected path lookup.  This guard exists for
/// consumers that expose the field.
pub fn validate_record_name(name: &[u8]) -> Result<&[u8], GciError> {
    if name.is_empty()
        || name.len() >= crate::dir::FILE_NAME_MAX
        || name.contains(&0)
        || name.contains(&b'/')
        || name.contains(&b'\\')
        || name == b"."
        || name == b".."
    {
        return Err(GciError::InvalidRecordName);
    }
    Ok(name)
}
