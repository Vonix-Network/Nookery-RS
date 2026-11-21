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
//!   slice (`typed_swap`), plus the exact raw Mail/original-design/diary keep
//!   block walkers in `composite`, matching the named `swap_*` helpers in
//!   `pc/src/pc_save_bswap.c`.
//!
//! # Explicit non-goals
//!
//! * No complete `Save_t` byte-order decoding, legacy-PC decoding, quest
//!   deadline relocation, or runtime card/travel integration. The upstream
//!   whole-save walker depends on the full decompiled struct layout
//!   (`pc/src/pc_save_bswap.c`). This crate validates big-endian bytes on disk,
//!   exposes them unmodified, and swaps only source-pinned composite regions.
//! * No host-struct `pc_gci_write` port. The Rust staged writer and manager
//!   cover synthetic, already-encoded records and recovery rotation only;
//!   construction from live `Save_t`/ARAM/game globals remains open.
//! * No `Save_t` field interpretation beyond the offsets pinned by upstream
//!   `_Static_assert`s (version, save code, land id, checksum).
//!
//! Every parser entry point bounds its input: no length field is trusted to
//! drive an allocation, and no input can panic.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod checksum;
pub mod composite;
pub mod dir;
pub mod error;
pub mod io;
pub mod layout;
pub mod manager;
pub mod path;
pub mod record;
pub mod typed_swap;

/// Exact remaining C symbols and behaviors intentionally outside this crate's
/// current bounded parity surface.
///
/// This is kept as data so downstream reports cannot accidentally imply full
/// save/card/travel parity after a bounded slice lands.
pub const NOT_PORTED: &[&str] = &[
    "pc_save_bswap: whole Save_t fields outside swap_save_t_verified, including homes, field, animals, shop, events, post office, island, fish records, and trailing save fields",
    "pc_save_bswap_from_legacy_pc: legacy-PC layout decode",
    "pc_save_relocate_quest_time_limits: all delivery/errand/animal quest deadline relocation",
    "pc_save_bswap_verify_roundtrip: whole Save_t BE->LE->BE oracle verification",
    "swap_mQst_errand legacy=TRUE chain-union behavior and contest-quest instances outside the private quest regions",
    "pc_gci_read_image: post-validation byte-order decode, legacy title conversion, and saved_auto_nwrite_time mutation",
    "pc_gci_write: live host Save_t/ARAM construction, byte swapping, comment/name generation, timestamped CARDDir creation, and durable filesystem transaction",
    "mCD_save_data_aram_malloc/mCD_set_aram_save_data/mCD_save_data_aram_to_main/mCD_save_data_main_to_aram: Dolphin ARAM/card DMA lifecycle (Rust only models explicit buffers)",
    "pc_save_pre_write_side_effects: live play-time, money-stone, reset-code, and global Save/Common state integration",
    "mCD_LoadLand, mCD_SaveStation_NextLand_bg, mCD_SaveStation_Passport_bg, mCD_toNextLand, and passport mCD_foreigner_c serialization: complete travel lifecycle",
];

pub use checksum::{checksum_be, verify_block};
pub use dir::{CardDir, FILE_NAME_MAX, HEADER_SIZE};
pub use error::{GciError, SaveResult};
pub use io::{read_town_record_from_slots, read_town_record_recovering, write_town_record};
pub use layout::{
    BLOCK_SIZE, FILE_DATA_SIZE, RECORD_SIZE, SAVE_BACK_OFFSET, SAVE_MAIN_OFFSET, SAVE_SIZE,
    TOWN_BLOCK_COUNT, TOWN_FILE_NAME,
};
pub use manager::{
    AramBlocks, BIRTHDAY_ACTIVE_FLAG, CARD_SLOT_A, CARD_SLOT_B, CardManager, CardManagerError,
    DIARY_ENTRY_COUNT, DIARY_ENTRY_SIZE, DIARY_PAGE_COUNT, EMPTY_ITEM, KEEP_MAIL_COUNT,
    KEEP_PAGE_COUNT, MAIL_SIZE, ORIGINAL_DESIGN_SIZE, PC_CARD_TOWN_MAX, PC_GCI_FILENAME,
    PC_GCI_PATH_MAX, PC_GCI_PREFIXED_FILENAME, POCKET_COUNT, PreWriteState, PrivateState,
    SavePaths, SaveScan, TownEntry, TownIdentity, WISP_FIRST, WISP_LAST, aram_offsets,
    mCD_save_data_aram_malloc, mCD_save_data_aram_to_main, mCD_save_data_main_to_aram,
    mCD_set_aram_save_data, manager_layout, pc_card_b_collect_town, pc_card_b_find_town,
    pc_card_b_is_legacy_town, pc_card_b_refresh_towns, pc_card_b_scan_town_files,
    pc_card_b_select_town, pc_card_b_town, pc_card_b_town_base, pc_card_b_town_count,
    pc_card_count_town, pc_card_find_town, pc_card_scan_towns, pc_card_take_first_town,
    pc_check_reset_code, pc_ensure_save_dirs, pc_fix_zeroed_original_entries,
    pc_get_land_copy_protect, pc_init_aram_block, pc_init_diary_entries, pc_init_mail_entries,
    pc_init_original_entries, pc_save_check_and_load, pc_save_find_and_load,
    pc_save_migrate_legacy, pc_save_pre_write_side_effects, pc_save_scan_gci_dir,
    pc_save_write_gci_to, pc_set_reset_code,
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
