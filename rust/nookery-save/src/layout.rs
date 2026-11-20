//! Fixed on-disk geometry of a town GCI record.
//!
//! Every constant here is derived from `pc/src/pc_gci.c` (whose `_Static_assert`
//! block pins the same values), `include/m_card.h`, and
//! `libs/dolphin/include/dolphin/card.h`.

use crate::dir;

/// `PC_GCI_BLOCK_SIZE` / `mCD_MEMCARD_SECTORSIZE`: one memory-card block.
pub const BLOCK_SIZE: usize = 0x2000;

/// `PC_GCI_TOWN_FILE_NAME`, the `fileName` field of a town record.
pub const TOWN_FILE_NAME: &[u8] = b"DobutsunomoriP_MURA";

/// `OTHERS_SIZE`: the auxiliary block region at the start of the payload.
pub const OTHERS_SIZE: usize = 0x26000;

/// `GCI_SAVE_MAIN_OFFSET`: `Save[]` element 0.
pub const SAVE_MAIN_OFFSET: usize = OTHERS_SIZE;

/// `GCI_SAVE_BACK_OFFSET`: `Save[]` element 1 (embedded backup).
pub const SAVE_BACK_OFFSET: usize = OTHERS_SIZE + SAVE_SIZE;

/// `sizeof(Save)`: `Save_t` rounded up to one memory-card sector.
pub const SAVE_SIZE: usize = 0x26000;

/// `GCI_FILE_DATA_SIZE` / `mCD_LAND_SAVE_SIZE`: bytes after the header.
pub const FILE_DATA_SIZE: usize = 3 * SAVE_SIZE;

/// Total bytes of a town record, header included.
pub const RECORD_SIZE: usize = dir::HEADER_SIZE + FILE_DATA_SIZE;

/// `GCI_FILE_DATA_SIZE / GCI_SECTOR_SIZE`, the required `CARDDir.length`.
pub const TOWN_BLOCK_COUNT: u16 = (FILE_DATA_SIZE / BLOCK_SIZE) as u16;

/// `ALIGN_NEXT(sizeof(MemcardHeader_c) + 32, 32)`: first auxiliary block.
pub const AUX_START: usize = 0x1460;

/// `mCD_KEEP_ORIGINAL_SIZE`.
pub const KEEP_ORIGINAL_SIZE: usize = 0xCCA0;
/// `mCD_KEEP_MAIL_SIZE`.
pub const KEEP_MAIL_SIZE: usize = 0xBAC0;
/// `mCD_KEEP_DIARY_SIZE`.
pub const KEEP_DIARY_SIZE: usize = 0xBA20;

/// `mFRm_SAVE_ID` (`'GAFE'`).
pub const SAVE_ID: u32 = u32::from_be_bytes(*b"GAFE");

/// `mFRm_VERSION`: the current save layout version.
pub const SAVE_VERSION_CURRENT: i32 = 6;
/// The oldest version `pc_gci_select_copy` accepts.
pub const SAVE_VERSION_LEGACY: i32 = 5;

/// `mFRm_chk_t.version`.
pub const SAVE_CHECK_VERSION_OFFSET: usize = 0x00;
/// `mFRm_chk_t.code`.
pub const SAVE_CHECK_CODE_OFFSET: usize = 0x04;
/// `mFRm_chk_t.land_id`.
pub const SAVE_CHECK_LAND_ID_OFFSET: usize = 0x08;
/// `offsetof(Save_t, land_info)`.
pub const LAND_INFO_OFFSET: usize = 0x9120;
/// `offsetof(mLd_land_info_c, id)`.
pub const LAND_INFO_ID_OFFSET: usize = LAND_INFO_OFFSET + 0x0A;

/// `mLd_BITMASK` high byte test: `mLd_CHECK_LAND_ID` uses `0xFF00`.
pub const LAND_ID_MASK: u16 = 0xFF00;
/// `mLd_BITMASK`.
pub const LAND_ID_BITMASK: u16 = 0x3000;

/// `offsetof(mCD_keep_mail_c, landid)`, the auxiliary order probe.
pub const KEEP_MAIL_LANDID_OFFSET: usize = AUX_START + 0x02;
