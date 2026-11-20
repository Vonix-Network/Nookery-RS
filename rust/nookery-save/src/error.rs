//! Failure categories for GCI parsing.
//!
//! Upstream collapses every parse failure into `PC_SAVE_INVALID` (or
//! `PC_SAVE_MISSING` / `PC_SAVE_IO_ERROR` when a file cannot be opened).
//! [`GciError::is_invalid`] and [`GciError::as_save_result`] expose that
//! mapping; the finer variants only describe *why* a record was rejected.

use std::fmt;

/// Upstream `pc_save_result` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveResult {
    /// `PC_SAVE_LOADED`.
    Loaded,
    /// `PC_SAVE_MISSING`: no file at the path.
    Missing,
    /// `PC_SAVE_INVALID`: the file exists but fails validation.
    Invalid,
    /// `PC_SAVE_IO_ERROR`: the file could not be opened or read.
    IoError,
}

/// Reason a memory-card image was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GciError {
    /// No file at the requested path (`PC_SAVE_MISSING`).
    Missing,
    /// Opening, stat-ing, or reading failed (`PC_SAVE_IO_ERROR`).
    Io(std::io::ErrorKind),
    /// Fewer than `CARDDir` bytes were available.
    TruncatedHeader {
        /// Bytes actually available.
        available: usize,
    },
    /// The header name does not match the expected file name.
    FileNameMismatch,
    /// The stored block count is not the one the source requires.
    BadBlockCount {
        /// Block count found in the header.
        found: u16,
        /// Block count upstream requires.
        expected: u16,
    },
    /// The record ends before its declared payload.
    TruncatedPayload {
        /// Payload bytes available.
        found: usize,
        /// Payload bytes required.
        expected: usize,
    },
    /// The record extends past its declared payload.
    Oversized {
        /// Bytes found.
        found: usize,
        /// Bytes expected.
        expected: usize,
    },
    /// Neither the main save block nor its embedded backup validates.
    NoValidSaveCopy,
    /// No auxiliary block layout passed checksum validation.
    BadAuxChecksums,
    /// A caller-supplied path suffix would redirect the derived path.
    InvalidSuffix,
    /// Concatenating base and suffix would exceed `PC_GCI_PATH_MAX`.
    PathTooLong,
    /// A `fileName` value is unusable as a record name.
    InvalidRecordName,
}

impl GciError {
    /// True when upstream would report `PC_SAVE_INVALID`.
    pub fn is_invalid(&self) -> bool {
        matches!(self.as_save_result(), SaveResult::Invalid)
    }

    /// Upstream result category for this failure.
    pub fn as_save_result(&self) -> SaveResult {
        match self {
            GciError::Missing => SaveResult::Missing,
            GciError::Io(_) => SaveResult::IoError,
            _ => SaveResult::Invalid,
        }
    }
}

impl fmt::Display for GciError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GciError::Missing => f.write_str("save file is missing"),
            GciError::Io(kind) => write!(f, "save I/O error: {kind}"),
            GciError::TruncatedHeader { available } => {
                write!(f, "directory header truncated: {available} bytes available")
            }
            GciError::FileNameMismatch => f.write_str("directory header names a different file"),
            GciError::BadBlockCount { found, expected } => {
                write!(f, "directory block count {found} != {expected}")
            }
            GciError::TruncatedPayload { found, expected } => {
                write!(f, "record truncated: {found} of {expected} payload bytes")
            }
            GciError::Oversized { found, expected } => {
                write!(f, "record oversized: {found} bytes, expected {expected}")
            }
            GciError::NoValidSaveCopy => {
                f.write_str("neither main save block nor embedded backup validates")
            }
            GciError::BadAuxChecksums => f.write_str("auxiliary block checksums are invalid"),
            GciError::InvalidSuffix => f.write_str("path suffix is not a plain suffix"),
            GciError::PathTooLong => f.write_str("derived save path exceeds PC_GCI_PATH_MAX"),
            GciError::InvalidRecordName => f.write_str("record name is not a usable file name"),
        }
    }
}

impl std::error::Error for GciError {}
