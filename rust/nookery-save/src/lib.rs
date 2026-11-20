//! Bounded, allocation-capped parsing of Animal Crossing (GAFE) town GCI
//! records, ported from the PC port layer in `pc/src/pc_gci.c`.
//!
//! # Scope
//!
//! This crate ports only the parts of the save-card surface that are pure,
//! source-verifiable, and independent of proprietary game data:
//!
//! * `CARDDir` header decoding and filename matching (`pc_gci_dir_has_name`).
//! * File-data offset computation (`pc_gci_data_offset`).
//! * The big-endian 16-bit block checksum (`pc_checksum_be`).
//! * Town-record validation: required block count, main/backup `Save` copy
//!   selection with version/ID/land-id/checksum gates (`pc_gci_select_copy`),
//!   and auxiliary block ordering plus checksum validation
//!   (`pc_gci_aux_offsets`).
//! * Recovery-companion path derivation (`pc_gci_sibling_path` and the
//!   suffix tables).
//! * Typed byte-swap for five leaf structs and the bounded Mail composite
//!   slice (`typed_swap`): `lbRTC_time_c`, `lbRTC_ymd_c`, `PersonalID_c`,
//!   `AnmPersonalID_c`, `mLd_land_info_c`, `Mail_nm_c`, `Mail_hdr_c`,
//!   `Mail_ct_c`, and `Mail_c`, matching the `swap_*` helpers at
//!   `pc/src/pc_save_bswap.c:181-220`.
//!
//! # Explicit non-goals
//!
//! * No whole-`Save_t` or Mail-array byte-order decoding, quest walkers, ARAM,
//!   card I/O, or runtime integration. The upstream walkers depend on the full
//!   decompiled struct layout (`pc/src/pc_save_bswap.c`); this crate validates
//!   the big-endian bytes on disk, hands them back unmodified, and swaps only
//!   the five leaves plus one source-grounded `Mail_c` in [`typed_swap`].
//! * No writing, backup rotation, or import.  `pc_gci_write` mutates the
//!   filesystem and constructs game data, so it is out of scope for a
//!   validation slice.
//! * No `Save_t` field interpretation beyond the offsets pinned by upstream
//!   `_Static_assert`s (version, save code, land id, checksum).
//!
//! Every parser entry point bounds its input: no length field is trusted to
//! drive an allocation, and no input can panic.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod checksum;
pub mod dir;
pub mod error;
pub mod layout;
pub mod path;
pub mod record;
pub mod typed_swap;

pub use checksum::{checksum_be, verify_block};
pub use dir::{CardDir, FILE_NAME_MAX, HEADER_SIZE};
pub use error::{GciError, SaveResult};
pub use layout::{
    BLOCK_SIZE, FILE_DATA_SIZE, RECORD_SIZE, SAVE_BACK_OFFSET, SAVE_MAIN_OFFSET, SAVE_SIZE,
    TOWN_BLOCK_COUNT, TOWN_FILE_NAME,
};
pub use path::{
    DEFAULT_FILE_NAME, PATH_MAX, PENDING_SUFFIXES, ROTATION_SUFFIXES, recovery_candidates,
    sibling_path, validate_record_name,
};
pub use record::{AuxBlock, AuxLayout, TownRecord};
pub use typed_swap::{
    AnimalPersonalId, Direction, LandInfo, Mail, MailContent, MailHeader, MailName, PersonalId,
    RtcTime, RtcYmd, TypedSwap,
};

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

/// Read and validate one town record from `path`.
///
/// The read is bounded by [`RECORD_SIZE`]: the file is stat-ed first and any
/// larger file is rejected as [`GciError::Oversized`] instead of being read.
/// Missing files map to [`GciError::Missing`] (`PC_SAVE_MISSING`); other open
/// failures map to [`GciError::Io`] (`PC_SAVE_IO_ERROR`).
///
/// This is stricter than `pc_gci_read_image`, which `fread`s exactly
/// `RECORD_SIZE` bytes and ignores trailing data; [`TownRecord::parse_prefix`]
/// reproduces that leniency for callers that need it.
pub fn read_town_record(path: impl AsRef<Path>) -> Result<TownRecord, GciError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|err| match err.kind() {
        ErrorKind::NotFound => GciError::Missing,
        kind => GciError::Io(kind),
    })?;
    let size = file
        .metadata()
        .map_err(|err| GciError::Io(err.kind()))?
        .len();
    if size > RECORD_SIZE as u64 {
        return Err(GciError::Oversized {
            found: usize::try_from(size).unwrap_or(usize::MAX),
            expected: RECORD_SIZE,
        });
    }
    let mut buffer = Vec::with_capacity(RECORD_SIZE.min(size as usize).max(1));
    let mut reader = file.take(RECORD_SIZE as u64);
    reader
        .read_to_end(&mut buffer)
        .map_err(|err| GciError::Io(err.kind()))?;
    TownRecord::parse(&buffer)
}

/// Read only the directory header of `path`, as `pc_gci_read_dir` does.
pub fn read_card_dir(path: impl AsRef<Path>) -> Result<CardDir, GciError> {
    let file = File::open(path).map_err(|err| match err.kind() {
        ErrorKind::NotFound => GciError::Missing,
        kind => GciError::Io(kind),
    })?;
    let mut header = Vec::with_capacity(HEADER_SIZE);
    file.take(HEADER_SIZE as u64)
        .read_to_end(&mut header)
        .map_err(|err| GciError::Io(err.kind()))?;
    CardDir::parse(&header)
}

/// Port of `pc_gci_is_town_file`: the header names the town record.
///
/// Reads at most one header; a missing or short file simply answers `false`,
/// matching upstream's silent failure.
pub fn is_town_file(path: impl AsRef<Path>) -> bool {
    read_card_dir(path)
        .map(|dir| dir.matches_file_name(TOWN_FILE_NAME))
        .unwrap_or(false)
}

/// Port of `pc_gci_is_gci_file`: at least header + payload, and the game name
/// starts with `"GAF"`.
///
/// Always `false` when the file is not a regular file or is too small.
pub fn is_gci_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() < (HEADER_SIZE + FILE_DATA_SIZE) as u64 {
        return false;
    }
    read_card_dir(path)
        .map(|dir| dir.game_name().starts_with(b"GAF"))
        .unwrap_or(false)
}
