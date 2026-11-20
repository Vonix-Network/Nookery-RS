//! Hand-authored fixtures for the town GCI record.
//!
//! Every byte is produced from the public format constants re-exported by the
//! crate (`HEADER_SIZE`, `TOWN_BLOCK_COUNT`, `SAVE_MAIN_OFFSET`, ...).  No
//! bytes are copied from a game save or any other protected asset.

#![allow(dead_code)]

use nookery_save::checksum::checksum_be;
use nookery_save::{FILE_DATA_SIZE, HEADER_SIZE, RECORD_SIZE, SAVE_SIZE, TOWN_BLOCK_COUNT};

/// The land id used by the fixtures; satisfies `mLd_CHECK_LAND_ID`.
pub const LAND_ID: u16 = 0x3001;

/// Auxiliary block geometry, recomputed here independently of the crate's
/// internal constants so the fixtures cannot silently follow a crate change.
pub const AUX_START: usize = 0x1460;
pub const MAIL_SIZE: usize = 0xBAC0;
pub const ORIGINAL_SIZE: usize = 0xCCA0;
pub const DIARY_SIZE: usize = 0xBA20;

/// Auxiliary block placement for one ordering.
pub struct Aux {
    pub mail: usize,
    pub original: usize,
    pub diary: usize,
}

pub fn aux_modern() -> Aux {
    let mail = AUX_START;
    let original = mail + MAIL_SIZE;
    Aux {
        mail,
        original,
        diary: original + ORIGINAL_SIZE,
    }
}

pub fn aux_legacy() -> Aux {
    let original = AUX_START;
    let mail = original + ORIGINAL_SIZE;
    Aux {
        mail,
        original,
        diary: mail + MAIL_SIZE,
    }
}

/// `offsetof(mFRm_chk_t, checksum)`: version 4, code 4, land_id 2, time 8.
pub const SAVE_CHECKSUM_OFFSET: usize = 0x12;

/// Set the big-endian 16-bit checksum at `block[at..at + 2]` so the block sums
/// to zero, the way `pc_gci_write` stores it.
pub fn seal_at(block: &mut [u8], at: usize) {
    block[at..at + 2].copy_from_slice(&[0, 0]);
    let value = checksum_be(block, 0);
    block[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

/// Seal a block whose checksum word is its first field.
pub fn seal(block: &mut [u8]) {
    seal_at(block, 0);
}

/// Write a `Save` block (0x26000 bytes) that passes `pc_gci_select_copy`.
pub fn write_save_block(block: &mut [u8], land_id: u16, version: i32, land_info_id: u16) {
    assert_eq!(block.len(), SAVE_SIZE);
    block.fill(0);
    block[0x00..0x04].copy_from_slice(&version.to_be_bytes());
    block[0x04..0x08].copy_from_slice(b"GAFE");
    block[0x08..0x0A].copy_from_slice(&land_id.to_be_bytes());
    block[0x912A..0x912C].copy_from_slice(&land_info_id.to_be_bytes());
    seal_at(block, SAVE_CHECKSUM_OFFSET);
}

/// Fill one auxiliary block with a deterministic pattern and seal it.
pub fn write_aux_block(block: &mut [u8], land_id: Option<u16>, seed: u8) {
    assert!(block.len() % 2 == 0);
    for (index, byte) in block.iter_mut().enumerate().skip(4) {
        *byte = seed.wrapping_add((index % 251) as u8);
    }
    block[2..4].copy_from_slice(&[0xFF, 0xEE]);
    if let Some(land_id) = land_id {
        block[2..4].copy_from_slice(&land_id.to_be_bytes());
    }
    seal(block);
}

/// A complete, valid town record.
pub fn valid_record() -> Vec<u8> {
    build(Options::default())
}

/// Knobs for exercising individual validation gates.
pub struct Options {
    pub file_name: Vec<u8>,
    pub block_count: u16,
    pub main: SaveSpec,
    pub backup: SaveSpec,
    pub aux: AuxOrder,
    pub mail_land_id: Option<u16>,
    pub corrupt_original: bool,
    pub payload_len: usize,
}

pub enum SaveSpec {
    Valid { version: i32 },
    Corrupt,
    Absent,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AuxOrder {
    Modern,
    Legacy,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            file_name: b"DobutsunomoriP_MURA".to_vec(),
            block_count: TOWN_BLOCK_COUNT,
            main: SaveSpec::Valid { version: 6 },
            backup: SaveSpec::Corrupt,
            aux: AuxOrder::Modern,
            mail_land_id: Some(LAND_ID),
            corrupt_original: false,
            payload_len: FILE_DATA_SIZE,
        }
    }
}

/// Assemble a record from `options`.
pub fn build(options: Options) -> Vec<u8> {
    // Always assemble a complete record, then truncate: this keeps the
    // short-payload fixtures from indexing past their own buffer.
    let mut record = vec![0u8; RECORD_SIZE];

    // Directory header.
    record[0x00..0x04].copy_from_slice(b"GAFE");
    record[0x04..0x06].copy_from_slice(b"01");
    record[0x06] = 0xFF;
    record[0x07] = 0x00;
    let name_len = options.file_name.len().min(32);
    record[0x08..0x08 + name_len].copy_from_slice(&options.file_name[..name_len]);
    record[0x28..0x2C].copy_from_slice(&0x0000_0100u32.to_be_bytes());
    record[0x2C..0x30].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    record[0x34] = 0x04;
    record[0x36..0x38].copy_from_slice(&5u16.to_be_bytes());
    record[0x38..0x3A].copy_from_slice(&options.block_count.to_be_bytes());
    record[0x3C..0x40].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());

    let aux = match options.aux {
        AuxOrder::Modern => aux_modern(),
        AuxOrder::Legacy => aux_legacy(),
    };

    let payload = &mut record[HEADER_SIZE..];

    // Auxiliary region.
    let mail = &mut payload[aux.mail..aux.mail + MAIL_SIZE];
    write_aux_block(mail, options.mail_land_id, 0x11);
    let original = &mut payload[aux.original..aux.original + ORIGINAL_SIZE];
    write_aux_block(original, None, 0x22);
    if options.corrupt_original {
        original[0x40] ^= 0xFF;
    }
    let diary = &mut payload[aux.diary..aux.diary + DIARY_SIZE];
    write_aux_block(diary, None, 0x33);

    // Main and embedded backup save copies.
    let main =
        &mut payload[nookery_save::SAVE_MAIN_OFFSET..nookery_save::SAVE_MAIN_OFFSET + SAVE_SIZE];
    match options.main {
        SaveSpec::Valid { version } => write_save_block(main, LAND_ID, version, LAND_ID),
        SaveSpec::Corrupt => {
            write_save_block(main, LAND_ID, 6, LAND_ID);
            main[0x2000] ^= 0xFF;
        }
        SaveSpec::Absent => main.fill(0),
    }
    let backup =
        &mut payload[nookery_save::SAVE_BACK_OFFSET..nookery_save::SAVE_BACK_OFFSET + SAVE_SIZE];
    match options.backup {
        SaveSpec::Valid { version } => write_save_block(backup, LAND_ID, version, LAND_ID),
        SaveSpec::Corrupt => {
            write_save_block(backup, LAND_ID, 6, LAND_ID);
            backup[0x3000] ^= 0xFF;
        }
        SaveSpec::Absent => backup.fill(0),
    }

    record.truncate(HEADER_SIZE + options.payload_len);
    record
}

/// A `SaveSpec::Valid` backup.  Handy as `..` base for option overrides.
pub const VALID_BACKUP: SaveSpec = SaveSpec::Valid { version: 6 };

/// Sanity check used by tests: the fixture really is record-sized.
pub fn assert_record_size(record: &[u8]) {
    assert_eq!(record.len(), RECORD_SIZE);
    assert_eq!(FILE_DATA_SIZE, 3 * SAVE_SIZE);
}
