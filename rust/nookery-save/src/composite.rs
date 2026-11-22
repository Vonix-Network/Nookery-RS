//! Verified wire-layout mirrors and bounded byte-swap walkers.
//!
//! The walkers below are byte-oriented so they can preserve unowned padding
//! and opaque union bytes exactly. Their offsets and field order are taken
//! from the C helpers and the struct comments in the named source headers.

use crate::checksum::checksum_be;
use crate::dir::HEADER_SIZE;
use crate::error::GciError;
use crate::layout::{AUX_START, RECORD_SIZE, SAVE_BACK_OFFSET, SAVE_MAIN_OFFSET};
use crate::record::{AuxLayout, TownRecord};

/// Exact `sizeof(lbRTC_time_c)`.
pub const RTC_TIME_SIZE: usize = 8;
/// Exact `sizeof(lbRTC_ymd_c)`.
pub const RTC_YMD_SIZE: usize = 4;
/// Exact `sizeof(PersonalID_c)`.
pub const PERSONAL_ID_SIZE: usize = 0x14;
/// Exact `sizeof(AnmPersonalID_c)`.
pub const ANIMAL_PERSONAL_ID_SIZE: usize = 0x0E;
/// Exact `sizeof(mLd_land_info_c)`.
pub const LAND_INFO_SIZE: usize = 0x0C;
/// Exact `sizeof(Mail_c)`.
pub const MAIL_SIZE: usize = 0x12A;
/// Exact `sizeof(mQst_base_c)`.
pub const QUEST_BASE_SIZE: usize = 0x0C;
/// Exact `sizeof(mQst_delivery_c)`.
pub const QUEST_DELIVERY_SIZE: usize = 0x28;
/// Exact `sizeof(mQst_errand_c)`.
pub const QUEST_ERRAND_SIZE: usize = 0x58;
/// Exact `sizeof(mQst_contest_c)`.
pub const QUEST_CONTEST_SIZE: usize = 0x28;
/// Exact `sizeof(Private_c)`.
pub const PRIVATE_SIZE: usize = 0x2440;
/// Exact `sizeof(Save_t)`.
pub const SAVE_T_SIZE: usize = 0x242A0;
/// Exact sector-rounded `sizeof(Save)`.
pub const SAVE_SIZE: usize = 0x26000;
/// Exact sector-rounded `mCD_KEEP_MAIL_SIZE`.
pub const KEEP_MAIL_SIZE: usize = 0xBAC0;
/// Exact sector-rounded `mCD_KEEP_ORIGINAL_SIZE`.
pub const KEEP_ORIGINAL_SIZE: usize = 0xCCA0;
/// Exact sector-rounded `mCD_KEEP_DIARY_SIZE`.
pub const KEEP_DIARY_SIZE: usize = 0xBA20;
/// Number of mail pages in `mCD_keep_mail_c`.
pub const KEEP_MAIL_PAGE_COUNT: usize = 8;
/// Number of mail entries per keep-mail page.
pub const KEEP_MAIL_COUNT: usize = 20;
/// The first `Mail_c` in `mCD_keep_mail_c`.
pub const KEEP_MAIL_ENTRIES_OFFSET: usize = 0x64;
/// The `_CC80` trailer field in `mCD_keep_original_c`.
///
/// The member name in `include/m_card.h` and the C swap helper's
/// `orig->_CC80` access pin this wire offset.  The intervening bytes are
/// alignment/padding and are intentionally left untouched.
pub const KEEP_ORIGINAL_TRAILER_OFFSET: usize = 0xCC80;
/// The first `Private_c` in `Save_t`.
pub const SAVE_PRIVATE_OFFSET: usize = 0x20;
/// The `Save_t.land_info` offset.
pub const SAVE_LAND_INFO_OFFSET: usize = 0x9120;
/// The `Save_t.save_check.checksum` offset.
pub const SAVE_CHECKSUM_OFFSET: usize = 0x12;
/// `UT_Z_NUM * UT_X_NUM` (`include/m_field_make.h`): the `u16` furniture slots
/// covered by one `mHm_lyr_c.items`, i.e. the `0x000..0x200` span.
pub const HOME_LAYER_ITEM_COUNT: usize = 256;

/// Byte-order direction used by the C `pc_bswap_dir_t` API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwapDirection {
    /// Convert host-order little-endian bytes to save-card big-endian bytes.
    ToBigEndian,
    /// Convert save-card big-endian bytes to host-order little-endian bytes.
    FromBigEndian,
}

/// `lbRTC_time_c`, including its 16-bit year at offset `0x06`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RtcTime {
    /// Seconds.
    pub sec: u8,
    /// Minutes.
    pub min: u8,
    /// Hour.
    pub hour: u8,
    /// Day of month.
    pub day: u8,
    /// Weekday.
    pub weekday: u8,
    /// Month.
    pub month: u8,
    /// Year.
    pub year: u16,
}

/// `lbRTC_ymd_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RtcYmd {
    /// Year.
    pub year: u16,
    /// Month.
    pub month: u8,
    /// Day.
    pub day: u8,
}

/// `PersonalID_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersonalId {
    /// Player name bytes.
    pub player_name: [u8; 8],
    /// Town name bytes.
    pub land_name: [u8; 8],
    /// Player identifier.
    pub player_id: u16,
    /// Town identifier.
    pub land_id: u16,
}

/// `AnmPersonalID_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnimalPersonalId {
    /// Animal identifier.
    pub npc_id: u16,
    /// Town identifier.
    pub land_id: u16,
    /// Town name bytes.
    pub land_name: [u8; 8],
    /// Low byte of the animal identifier.
    pub name_id: u8,
    /// Animal personality/look code.
    pub looks: u8,
}

/// `mLd_land_info_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LandInfo {
    /// Town name bytes.
    pub name: [u8; 8],
    /// Whether the town exists.
    pub exists: i8,
    /// Explicit C alignment padding before `id`.
    pub padding: u8,
    /// Town identifier.
    pub id: u16,
}

/// `mFRm_chk_t`, the fixed Save_t validation header.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaveCheck {
    /// Save format version.
    pub version: i32,
    /// Four-byte save code.
    pub code: u32,
    /// Town identifier copied into the validation header.
    pub land_id: u16,
    /// Save timestamp.
    pub time: RtcTime,
    /// Whole-block checksum.
    pub checksum: u16,
}

/// The fixed first `0x20` bytes of `Save_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SavePrefix {
    /// Validation header.
    pub save_check: SaveCheck,
    /// Current scene number.
    pub scene_no: i32,
    /// Current villager count.
    pub now_npc_max: u8,
    /// Pending removal index.
    pub remove_animal_idx: u8,
    /// Copy-protection value.
    pub copy_protect: u16,
    /// Uninterpreted bytes at `0x1C..0x20`.
    pub padding: [u8; 4],
}

/// `mQst_base_c`; the first two bytes are represented as raw bitfield storage.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QuestBase {
    /// Direction-dependent two-byte bitfield storage.
    pub flags: [u8; 2],
    /// Quest deadline.
    pub time_limit: RtcTime,
    /// C trailing padding.
    pub padding: [u8; 2],
}

/// `mQst_delivery_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QuestDelivery {
    /// Common quest fields.
    pub base: QuestBase,
    /// Recipient animal.
    pub recipient: AnimalPersonalId,
    /// Sender animal.
    pub sender: AnimalPersonalId,
}

/// `mQst_firstjob_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QuestFirstJob {
    /// Animals already used by the job.
    pub used_ids: [AnimalPersonalId; 2],
    /// Direction-dependent used-count/wrong-cloth bitfield.
    pub flags: u8,
    /// C trailing padding.
    pub padding: [u8; 3],
}

/// `mQst_errand_chain_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QuestErrandChain {
    /// Animals already used by the chain.
    pub used_ids: [AnimalPersonalId; 3],
    /// Number of used animals.
    pub used_num: u8,
    /// C trailing padding.
    pub padding: u8,
}

/// `mQst_errand_c`, with its union retained as the exact raw 0x2C region.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestErrand {
    /// Common quest fields.
    pub base: QuestBase,
    /// Recipient animal.
    pub recipient: AnimalPersonalId,
    /// Sender animal.
    pub sender: AnimalPersonalId,
    /// Errand item.
    pub item: u16,
    /// Direction-dependent pocket/type bitfield.
    pub flags: u8,
    /// C padding before the union.
    pub padding: u8,
    /// Chain/first-job union storage.
    pub info: [u8; 0x2C],
}

/// `mQst_contest_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QuestContest {
    /// Common quest fields.
    pub base: QuestBase,
    /// Requested item.
    pub requested_item: u16,
    /// Player identifier.
    pub player_id: PersonalId,
    /// Quest kind duplicate.
    pub quest_type: i8,
    /// C padding before the union.
    pub padding: u8,
    /// Contest union storage.
    pub info: [u8; 4],
}

/// `Mail_nm_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MailName {
    /// Named person.
    pub personal_id: PersonalId,
    /// Name source/type.
    pub name_type: u8,
}

/// `Mail_hdr_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MailHeader {
    /// Recipient.
    pub recipient: MailName,
    /// Sender.
    pub sender: MailName,
}

/// `Mail_ct_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MailContent {
    /// Font code.
    pub font: u8,
    /// Header insertion position.
    pub header_back_start: u8,
    /// Mail type.
    pub mail_type: u8,
    /// Paper type.
    pub paper_type: u8,
    /// Header bytes.
    pub header: [u8; 0x18],
    /// Body bytes.
    pub body: [u8; 0xC0],
    /// Footer bytes.
    pub footer: [u8; 0x20],
}

/// `Mail_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mail {
    /// Mail header.
    pub header: MailHeader,
    /// Attached present item.
    pub present: u16,
    /// Mail content.
    pub content: MailContent,
}

/// Opaque, aligned full Save_t storage for callers that need a C-sized value.
#[repr(C, align(4))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveTBytes {
    /// Exact Save_t bytes in host or card order.
    pub bytes: [u8; SAVE_T_SIZE],
}

/// Opaque, aligned keep-mail storage.
#[repr(C, align(32))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeepMailBytes {
    /// Exact mCD_keep_mail_c disk storage, including sector padding.
    pub bytes: [u8; KEEP_MAIL_SIZE],
}

/// The copy and auxiliary placement selected by the source GCI validator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompositeSelection {
    /// Index of the validated padded `Save` copy: 0 for main, 1 for backup.
    pub save_copy: usize,
    /// Validated auxiliary block placement and legacy-order decision.
    pub aux: AuxLayout,
}

/// Swap one bounded `Mail_c` byte representation.
pub fn swap_mail(bytes: &mut [u8], _direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, MAIL_SIZE)?;
    swap_personal(bytes, 0)?;
    swap_personal(bytes, 0x16)?;
    swap_u16(bytes, 0x2C)?;
    Ok(())
}

/// Swap one bounded delivery quest byte representation.
pub fn swap_quest_delivery(bytes: &mut [u8], direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, QUEST_DELIVERY_SIZE)?;
    swap_quest_base(bytes, 0, direction)?;
    swap_animal_personal(bytes, 0x0C)?;
    swap_animal_personal(bytes, 0x1A)?;
    Ok(())
}

/// Swap one bounded errand quest byte representation.
pub fn swap_quest_errand(bytes: &mut [u8], direction: SwapDirection) -> Result<(), GciError> {
    swap_quest_errand_impl(bytes, direction, false)
}

fn swap_quest_errand_impl(
    bytes: &mut [u8],
    direction: SwapDirection,
    legacy: bool,
) -> Result<(), GciError> {
    require(bytes, QUEST_ERRAND_SIZE)?;
    let errand_type = match direction {
        SwapDirection::FromBigEndian => bytes[0x2A] & 0x07,
        SwapDirection::ToBigEndian => (bytes[0x2A] >> 5) & 0x07,
    };
    swap_quest_base(bytes, 0, direction)?;
    swap_animal_personal(bytes, 0x0C)?;
    swap_animal_personal(bytes, 0x1A)?;
    swap_u16(bytes, 0x28)?;
    repack_5_3(&mut bytes[0x2A], direction);
    if legacy {
        for index in 0..3 {
            swap_animal_personal(bytes, 0x2C + index * ANIMAL_PERSONAL_ID_SIZE)?;
        }
        return Ok(());
    }
    match errand_type {
        1 => {
            for index in 0..3 {
                swap_animal_personal(bytes, 0x2C + index * ANIMAL_PERSONAL_ID_SIZE)?;
            }
        }
        2 => {
            for index in 0..2 {
                swap_animal_personal(bytes, 0x2C + index * ANIMAL_PERSONAL_ID_SIZE)?;
            }
            repack_7_1(&mut bytes[0x48], direction);
        }
        _ => {}
    }
    Ok(())
}

/// Swap one bounded contest quest byte representation.
pub fn swap_quest_contest(bytes: &mut [u8], direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, QUEST_CONTEST_SIZE)?;
    swap_quest_base(bytes, 0, direction)?;
    swap_u16(bytes, 0x0C)?;
    swap_personal(bytes, 0x0E)?;
    swap_u16(bytes, 0x26)?;
    Ok(())
}

/// Swap the complete source-pinned `Save_t` walker in place.
///
/// This is the normal GameCube layout.  The legacy PC title has two layout
/// differences in the C oracle and is exposed by [`swap_save_t_legacy`].
pub fn swap_save_t_verified(bytes: &mut [u8], direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, SAVE_T_SIZE)?;
    swap_save_t_impl(bytes, direction, false)
}

/// Swap the source's legacy-PC `Save_t` layout in place.
pub fn swap_save_t_legacy(bytes: &mut [u8], direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, SAVE_T_SIZE)?;
    swap_save_t_impl(bytes, direction, true)
}

fn swap_save_t_impl(
    bytes: &mut [u8],
    direction: SwapDirection,
    legacy: bool,
) -> Result<(), GciError> {
    swap_u32(bytes, 0x00)?;
    swap_u32(bytes, 0x04)?;
    swap_u16(bytes, 0x08)?;
    swap_time(bytes, 0x0A)?;
    swap_u16(bytes, SAVE_CHECKSUM_OFFSET)?;
    swap_u32(bytes, 0x14)?;
    swap_u16(bytes, 0x1A)?;

    for player in 0..4 {
        let base = SAVE_PRIVATE_OFFSET + player * PRIVATE_SIZE;
        swap_private_verified(bytes, base, direction, legacy)?;
    }
    swap_u16(bytes, SAVE_LAND_INFO_OFFSET + 0x0A)?;

    // mNtc_board_post_c[15]
    for index in 0..15 {
        swap_time(bytes, 0x912C + index * 0xC8 + 0xC0)?;
    }
    for index in 0..4 {
        swap_home(bytes, 0x9CE8 + index * 0x26B0, direction)?;
    }

    // mFM_fg_c[FG_BLOCK_Z_NUM * FG_BLOCK_X_NUM], 16x16 u16 items each.
    for index in 0..30 * 256 {
        swap_u16(bytes, 0x137A8 + index * 2)?;
    }
    for index in 0..72 {
        swap_combination(bytes, 0x173A8 + index * 2, direction)?;
    }
    for index in 0..15 {
        swap_animal(bytes, 0x17438 + index * 0x988, direction, false)?;
    }
    swap_animal_personal(bytes, 0x20330)?;
    swap_shop(bytes, 0x20340, direction)?;
    for index in 0..7 {
        swap_u16(bytes, 0x20480 + index * 2)?;
    }
    swap_u16(bytes, 0x2048E)?;
    swap_time(bytes, 0x20490)?;
    swap_event_save(bytes, 0x20498, direction)?;
    swap_event_common(bytes, 0x20554, direction, legacy)?;
    swap_u16(bytes, 0x20688)?;
    swap_time(bytes, 0x2068C)?;
    swap_post_office(bytes, 0x20694, direction, legacy)?;
    for index in 0..20 {
        swap_u16(bytes, 0x20ED0 + index * 2)?;
    }
    swap_u64(bytes, 0x20F08)?;
    swap_ymd(bytes, 0x20F14)?;
    for index in 0..(5 * 6 * 16) {
        swap_u16(bytes, 0x20F1C + index * 2)?;
    }
    swap_time(bytes, 0x212DC)?;
    for index in 0..4 {
        swap_ymd(bytes, 0x212E4 + index * 0x0E)?;
    }
    swap_mushroom_time(bytes, 0x2131C, direction)?;
    swap_ymd(bytes, 0x21322)?;
    for index in 0..20 {
        swap_u16(bytes, 0x21326 + index * 2)?;
    }
    swap_time(bytes, 0x2136E)?;
    swap_time(bytes, 0x2137E)?;
    swap_time(bytes, 0x21386)?;
    swap_u16(bytes, 0x21398)?;
    swap_ymd(bytes, 0x213A4)?;
    swap_bridge(bytes, 0x213F0, direction)?;
    swap_u64(bytes, 0x22528)?;
    swap_island(bytes, 0x22540, direction)?;
    for index in 0..5 {
        swap_fish_record(bytes, 0x23E68 + index * 0x24)?;
    }
    swap_mask_cat(bytes, 0x23F20)?;
    swap_anm_return(bytes, 0x24160)?;
    swap_ymd(bytes, 0x2416C)?;
    swap_time(bytes, 0x24178)?;
    swap_u32(bytes, 0x24180)?;
    swap_u64(bytes, 0x24198)?;
    swap_time(bytes, 0x241A0)?;
    Ok(())
}

/// Swap one complete GCI composite in place after the caller has selected its
/// save copy and auxiliary order.
///
/// The directory header remains card-format data.  The selected padded
/// `Save` copy and all three auxiliary/ARAM blocks are swapped using the
/// source-pinned offsets.  `legacy_aux_order` selects the C reader's legacy
/// original/mail ordering; the modern order is mail/original.  The function
/// The complete record extent is checked before mutating it, so short input
/// has no partial-swap side effect.  Use [`decode_gci_composite`] for the
/// source's checksum and selection gates.
pub fn swap_gci_composite(
    bytes: &mut [u8],
    save_copy: usize,
    legacy_aux_order: bool,
    direction: SwapDirection,
) -> Result<(), GciError> {
    require(bytes, RECORD_SIZE)?;
    let save_offset = match save_copy {
        0 => SAVE_MAIN_OFFSET,
        1 => SAVE_BACK_OFFSET,
        _ => {
            return Err(GciError::InvalidSaveCopy { found: save_copy });
        }
    };
    let (original_offset, mail_offset, diary_offset) = if legacy_aux_order {
        (
            AUX_START,
            AUX_START + KEEP_ORIGINAL_SIZE,
            AUX_START + KEEP_ORIGINAL_SIZE + KEEP_MAIL_SIZE,
        )
    } else {
        (
            AUX_START + KEEP_MAIL_SIZE,
            AUX_START,
            AUX_START + KEEP_MAIL_SIZE + KEEP_ORIGINAL_SIZE,
        )
    };

    swap_save_t_verified(&mut bytes[HEADER_SIZE + save_offset..], direction)?;
    swap_keep_original(&mut bytes[HEADER_SIZE + original_offset..], direction)?;
    swap_keep_mail(&mut bytes[HEADER_SIZE + mail_offset..], direction)?;
    swap_keep_diary(&mut bytes[HEADER_SIZE + diary_offset..], direction)?;
    Ok(())
}

/// Validate and decode a GCI composite in place.
///
/// This is the bounded production seam for the source's read order:
/// directory/name and block-count checks, main/backup Save selection, auxiliary
/// checksum/order selection, then BE-to-host conversion of the selected Save
/// fields and all three ARAM blocks.  The directory bytes and opaque/padding
/// fields remain untouched.  A failed validation returns before mutation.
///
/// Like `pc_gci_read_image`, trailing bytes are ignored after one complete
/// record; callers requiring an exact file use [`TownRecord::parse`] first.
pub fn decode_gci_composite(bytes: &mut [u8]) -> Result<CompositeSelection, GciError> {
    let record = TownRecord::parse_prefix(bytes)?;
    let selection = CompositeSelection {
        save_copy: record.save_copy(),
        aux: record.aux_layout(),
    };
    let save_offset = match selection.save_copy {
        0 => SAVE_MAIN_OFFSET,
        1 => SAVE_BACK_OFFSET,
        _ => unreachable!(),
    };
    let save_start = HEADER_SIZE + save_offset;
    let legacy = bytes
        .get(HEADER_SIZE..HEADER_SIZE + 28)
        .is_some_and(|comment| comment == b"DobutsunomoriP (AC PC Port)\0");
    relocate_quest_time_limits(&mut bytes[save_start..save_start + SAVE_SIZE])?;
    if legacy {
        swap_save_t_legacy(
            &mut bytes[save_start..save_start + SAVE_T_SIZE],
            SwapDirection::FromBigEndian,
        )?;
    } else {
        swap_save_t_verified(
            &mut bytes[save_start..save_start + SAVE_T_SIZE],
            SwapDirection::FromBigEndian,
        )?;
    }
    let aux_start = |block: crate::record::AuxBlock| HEADER_SIZE + selection.aux.offset(block);
    swap_keep_original(
        &mut bytes[aux_start(crate::record::AuxBlock::Original)..],
        SwapDirection::FromBigEndian,
    )?;
    swap_keep_mail(
        &mut bytes[aux_start(crate::record::AuxBlock::Mail)..],
        SwapDirection::FromBigEndian,
    )?;
    swap_keep_diary(
        &mut bytes[aux_start(crate::record::AuxBlock::Diary)..],
        SwapDirection::FromBigEndian,
    )?;
    // pc_gci_read_image copies the v5 validation timestamp after the normal
    // byte-order walk.  The marker selects the legacy union/bitfield rules.
    let version = i32::from_le_bytes(bytes[save_start..save_start + 4].try_into().unwrap());
    if version == 5 {
        let time = bytes[save_start + 0x0A..save_start + 0x12].to_owned();
        bytes[save_start + 0x241A0..save_start + 0x241A8].copy_from_slice(&time);
    }
    Ok(selection)
}

/// Encode a previously decoded composite back to card order and reseal it.
///
/// This mirrors the source writer's `pc_save_bswap_*` followed by
/// `pc_checksum_be`: the selected Save checksum covers `Save_t` bytes and each
/// ARAM block checksum covers its padded block.  The directory header and all
/// other bytes are preserved.
pub fn encode_gci_composite(
    bytes: &mut [u8],
    selection: CompositeSelection,
) -> Result<(), GciError> {
    let legacy = bytes
        .get(HEADER_SIZE..HEADER_SIZE + 28)
        .is_some_and(|comment| comment == b"DobutsunomoriP (AC PC Port)\0");
    if legacy {
        // The composite walker below uses the modern private quest rules, so
        // use the legacy Save_t walker directly for the selected save copy.
        let save_offset = match selection.save_copy {
            0 => SAVE_MAIN_OFFSET,
            1 => SAVE_BACK_OFFSET,
            _ => {
                return Err(GciError::InvalidSaveCopy {
                    found: selection.save_copy,
                });
            }
        };
        let start = HEADER_SIZE + save_offset;
        swap_save_t_legacy(
            &mut bytes[start..start + SAVE_T_SIZE],
            SwapDirection::ToBigEndian,
        )?;
        let save_offset = HEADER_SIZE + save_offset;
        seal_checksum(bytes, save_offset, SAVE_T_SIZE, SAVE_CHECKSUM_OFFSET)?;
        for block in [
            crate::record::AuxBlock::Original,
            crate::record::AuxBlock::Mail,
            crate::record::AuxBlock::Diary,
        ] {
            seal_checksum(
                bytes,
                HEADER_SIZE + selection.aux.offset(block),
                AuxLayout::size(block),
                0,
            )?;
        }
        return Ok(());
    }
    swap_gci_composite(
        bytes,
        selection.save_copy,
        selection.aux.legacy_order,
        SwapDirection::ToBigEndian,
    )?;

    let save_offset = match selection.save_copy {
        0 => SAVE_MAIN_OFFSET,
        1 => SAVE_BACK_OFFSET,
        _ => unreachable!("swap_gci_composite validates save_copy"),
    };
    seal_checksum(
        bytes,
        HEADER_SIZE + save_offset,
        SAVE_T_SIZE,
        SAVE_CHECKSUM_OFFSET,
    )?;
    for block in [
        crate::record::AuxBlock::Original,
        crate::record::AuxBlock::Mail,
        crate::record::AuxBlock::Diary,
    ] {
        seal_checksum(
            bytes,
            HEADER_SIZE + selection.aux.offset(block),
            AuxLayout::size(block),
            0,
        )?;
    }
    Ok(())
}

/// Apply the C reader's pre-swap quest deadline relocation to a BE `Save`
/// buffer.  It is intentionally separate because the source performs it
/// before `pc_save_bswap`, and it is a no-op for already-correct deadlines.
pub fn relocate_quest_time_limits(bytes: &mut [u8]) -> Result<(), GciError> {
    require(bytes, SAVE_SIZE)?;
    for player in 0..4 {
        let base = SAVE_PRIVATE_OFFSET + player * PRIVATE_SIZE;
        for index in 0..15 {
            relocate_quest_time_limit(bytes, base + 0x94 + index * QUEST_DELIVERY_SIZE)?;
        }
        for index in 0..5 {
            relocate_quest_time_limit(bytes, base + 0x2EC + index * QUEST_ERRAND_SIZE)?;
        }
    }
    for index in 0..15 {
        relocate_quest_time_limit(bytes, 0x17438 + index * 0x988 + 0x8A8)?;
    }
    relocate_quest_time_limit(bytes, 0x22540 + 0xF00 + 0x8A8)
}

fn relocate_quest_time_limit(bytes: &mut [u8], base: usize) -> Result<(), GciError> {
    require_from(bytes, base, QUEST_BASE_SIZE)?;
    if be_time_state(&bytes[base + 2..base + 10]) < 0
        && be_time_state(&bytes[base + 4..base + 12]) > 0
    {
        let time = bytes[base + 4..base + 12].to_owned();
        bytes[base + 2..base + 10].copy_from_slice(&time);
        bytes[base + 10..base + 12].fill(0);
    }
    Ok(())
}

fn be_time_state(time: &[u8]) -> i8 {
    if time.len() < 8 {
        return -1;
    }
    if time.iter().all(|&byte| byte == 0) {
        return 0;
    }
    let year = u16::from_be_bytes([time[6], time[7]]);
    if time[0] < 60
        && time[1] < 60
        && time[2] < 24
        && (1..=31).contains(&time[3])
        && time[4] < 7
        && (1..=12).contains(&time[5])
        && (1901..=2100).contains(&year)
    {
        1
    } else {
        -1
    }
}

/// Swap a full padded keep-mail block, including all 160 Mail_c entries.
pub fn swap_keep_mail(bytes: &mut [u8], direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, KEEP_MAIL_SIZE)?;
    swap_u16(bytes, 0)?;
    swap_u16(bytes, 2)?;
    for index in 0..(KEEP_MAIL_PAGE_COUNT * KEEP_MAIL_COUNT) {
        swap_mail(
            &mut bytes[KEEP_MAIL_ENTRIES_OFFSET + index * MAIL_SIZE..],
            direction,
        )?;
    }
    Ok(())
}

/// Swap the exact scalar fields covered by `pc_save_bswap_keep_original`.
///
/// The folder names and all 96 `mNW_original_design_c` values are byte data;
/// the C oracle swaps only the checksum, land id, and `_CC80` trailer.  The
/// direction is accepted for API symmetry, but each operation is a
/// self-inverse 16/32-bit byte swap.
pub fn swap_keep_original(bytes: &mut [u8], _direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, KEEP_ORIGINAL_SIZE)?;
    swap_u16(bytes, 0)?;
    swap_u16(bytes, 2)?;
    swap_u32(bytes, KEEP_ORIGINAL_TRAILER_OFFSET)?;
    Ok(())
}

/// Swap the checksum covered by `pc_save_bswap_keep_diary`.
///
/// Diary entries are fixed-size byte text (`mDi_entry_c`), so the C oracle
/// deliberately leaves the rest of the `mCD_keep_diary_c` block unchanged.
pub fn swap_keep_diary(bytes: &mut [u8], _direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, KEEP_DIARY_SIZE)?;
    swap_u16(bytes, 0)?;
    Ok(())
}

fn swap_time(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    swap_u16(bytes, offset + 6)
}

fn swap_ymd(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    swap_u16(bytes, offset)
}

fn swap_u64(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    require_from(bytes, offset, 8)?;
    bytes[offset..offset + 8].reverse();
    Ok(())
}

fn swap_notice_free_layer(
    bytes: &mut [u8],
    offset: usize,
    direction: SwapDirection,
) -> Result<(), GciError> {
    for index in 0..HOME_LAYER_ITEM_COUNT {
        swap_u16(bytes, offset + index * 2)?;
    }
    swap_u64(bytes, offset + 0x200)?;
    for index in 0..8 {
        swap_u32(bytes, offset + 0x208 + index * 4)?;
    }
    let _ = direction;
    Ok(())
}

fn swap_floor(bytes: &mut [u8], offset: usize, direction: SwapDirection) -> Result<(), GciError> {
    for layer in 0..4 {
        swap_notice_free_layer(bytes, offset + layer * 0x228, direction)?;
    }
    repack_1_1_6(&mut bytes[offset + 0x8A4], direction);
    Ok(())
}

fn swap_home(bytes: &mut [u8], offset: usize, direction: SwapDirection) -> Result<(), GciError> {
    swap_personal(bytes, offset)?;
    swap_ymd(bytes, offset + 0x1C)?;
    swap_u32(bytes, offset + 0x20)?;
    bytes[offset + 0x24] = bytes[offset + 0x24].reverse_bits();
    swap_ymd(bytes, offset + 0x26 + 2)?;
    let size = offset + 0x26 + 4;
    let (left, right) = bytes[size..size + 2].split_at_mut(1);
    repack_home_size(&mut left[0], &mut right[0], direction);
    for floor in 0..3 {
        swap_floor(bytes, offset + 0x38 + floor * 0x8A8, direction)?;
    }
    for index in 0..10 {
        swap_mail(&mut bytes[offset + 0x1A30 + index * MAIL_SIZE..], direction)?;
    }
    for index in 0..4 {
        let item = offset + 0x25D4 + index * 8;
        swap_u16(bytes, item)?;
        swap_u16(bytes, item + 2)?;
        swap_u32(bytes, item + 4)?;
    }
    swap_u32(bytes, offset + 0x2674)?;
    swap_time(bytes, offset + 0x2678)?;
    for index in 0..2 {
        swap_u32(bytes, offset + 0x2684 + index * 4)?;
    }
    Ok(())
}

fn swap_animal(
    bytes: &mut [u8],
    offset: usize,
    direction: SwapDirection,
    island: bool,
) -> Result<(), GciError> {
    swap_animal_personal(bytes, offset)?;
    for memory in 0..7 {
        let base = offset + 0x10 + memory * 0x138;
        swap_personal(bytes, base)?;
        swap_time(bytes, base + 0x14)?;
        if island {
            swap_u32(bytes, base + 0x1C)?;
            swap_u16(bytes, base + 0x20)?;
        } else {
            swap_u16(bytes, base + 0x1C + 8)?;
        }
        swap_u64(bytes, base + 0x28)?;
        repack_11111_3(&mut bytes[base + 0x31], direction);
        swap_u16(bytes, base + 0x34)?;
        swap_ymd(bytes, base + 0x32 + 0xFE)?;
    }
    swap_quest_contest(&mut bytes[offset + 0x8A8..], direction)?;
    if island {
        for index in 0..4 {
            swap_u16(bytes, offset + 0x8D8 + index * 2)?;
        }
    }
    swap_u16(bytes, offset + 0x8E0)?;
    swap_u16(bytes, offset + 0x8E4)?;
    swap_u16(bytes, offset + 0x8E6)?;
    swap_u16(bytes, offset + 0x8EE)?;
    for index in 0..4 {
        swap_time(bytes, offset + 0x900 + index * 0x1C)?;
    }
    Ok(())
}

fn swap_island(bytes: &mut [u8], offset: usize, direction: SwapDirection) -> Result<(), GciError> {
    swap_u16(bytes, offset + 0x08 + 0x0A)?;
    for index in 0..(2 * 256) {
        swap_u16(bytes, offset + 0x14 + index * 2)?;
    }
    let cottage = offset + 0x418;
    swap_floor(bytes, cottage + 0x08, direction)?;
    swap_time(bytes, cottage + 0x8B0)?;
    for index in 0..2 {
        swap_u32(bytes, cottage + 0x8BC + index * 4)?;
    }
    swap_animal(bytes, offset + 0xF00, direction, true)?;
    for index in 0..(2 * 16) {
        swap_u16(bytes, offset + 0x1888 + index * 2)?;
    }
    swap_time(bytes, offset + 0x18CA)
}

fn swap_combination(
    bytes: &mut [u8],
    offset: usize,
    direction: SwapDirection,
) -> Result<(), GciError> {
    let raw = read_u16(bytes, offset, direction)?;
    let (kind, height) = match direction {
        SwapDirection::FromBigEndian => ((raw >> 2) & 0x3FFF, raw & 3),
        SwapDirection::ToBigEndian => (raw & 0x3FFF, (raw >> 14) & 3),
    };
    let packed = match direction {
        SwapDirection::FromBigEndian => kind | (height << 14),
        SwapDirection::ToBigEndian => (kind << 2) | height,
    };
    write_u16(bytes, offset, packed, direction)
}

fn swap_shop(bytes: &mut [u8], offset: usize, direction: SwapDirection) -> Result<(), GciError> {
    for index in 0..5 {
        repack_2222(&mut bytes[offset + index], direction);
    }
    for index in 0..10 {
        swap_personal(bytes, offset + 0x06 + index * PERSONAL_ID_SIZE)?;
    }
    for index in 0..(39 + 1 + 3) {
        swap_u16(bytes, offset + 0xCE + index * 2)?;
    }
    let raw = read_u16(bytes, offset + 0x126, direction)?;
    let fields = match direction {
        SwapDirection::FromBigEndian => (
            (raw >> 14) & 3,
            (raw >> 13) & 1,
            (raw >> 12) & 1,
            (raw >> 11) & 1,
            (raw >> 7) & 0xF,
            raw & 0x7F,
        ),
        SwapDirection::ToBigEndian => (
            raw & 3,
            (raw >> 2) & 1,
            (raw >> 3) & 1,
            (raw >> 4) & 1,
            (raw >> 5) & 0xF,
            (raw >> 9) & 0x7F,
        ),
    };
    let packed = match direction {
        SwapDirection::FromBigEndian => {
            fields.0
                | (fields.1 << 2)
                | (fields.2 << 3)
                | (fields.3 << 4)
                | (fields.4 << 5)
                | (fields.5 << 9)
        }
        SwapDirection::ToBigEndian => {
            (fields.0 << 14)
                | (fields.1 << 13)
                | (fields.2 << 12)
                | (fields.3 << 11)
                | (fields.4 << 7)
                | fields.5
        }
    };
    write_u16(bytes, offset + 0x126, packed, direction)?;
    swap_u32(bytes, offset + 0x128)?;
    swap_time(bytes, offset + 0x12C)?;
    swap_time(bytes, offset + 0x134)?;
    swap_u32(bytes, offset + 0x13C)
}

fn swap_post_office(
    bytes: &mut [u8],
    offset: usize,
    direction: SwapDirection,
    legacy: bool,
) -> Result<(), GciError> {
    for field in [0, 2, 4, 6] {
        swap_u16(bytes, offset + field)?;
    }
    for index in 0..5 {
        swap_mail(&mut bytes[offset + 8 + index * MAIL_SIZE..], direction)?;
    }
    swap_mail(&mut bytes[offset + 0x5DA..], direction)?;
    swap_mail(&mut bytes[offset + 0x704..], direction)?;
    if legacy {
        swap_u32(bytes, offset + 0x830)?;
    } else {
        swap_u16(bytes, offset + 0x830)?;
        swap_u16(bytes, offset + 0x832)?;
    }
    swap_time(bytes, offset + 0x834)
}

fn swap_event_save(
    bytes: &mut [u8],
    offset: usize,
    direction: SwapDirection,
) -> Result<(), GciError> {
    swap_time(bytes, offset)?;
    let kind = read_u32(bytes, offset + 8, direction)?;
    swap_u32(bytes, offset + 8)?;
    let event = offset + 0x0C;
    match kind {
        0 => {
            swap_time(bytes, event)?;
            swap_time(bytes, event + 8)?;
            for index in 0..5 {
                swap_u16(bytes, event + 0x10 + index * 2)?;
            }
            swap_u32(bytes, event + 0x1C)?;
        }
        1 => {
            for index in 0..3 {
                swap_personal(bytes, event + index * PERSONAL_ID_SIZE)?;
            }
            swap_u32(bytes, event + 0x3C)?;
            for index in 0..3 {
                swap_u16(bytes, event + 0x40 + index * 2)?;
            }
        }
        2 => {
            for index in 0..2 {
                swap_personal(bytes, event + index * PERSONAL_ID_SIZE)?;
            }
            swap_time(bytes, event + 0x28)?;
            swap_u32(bytes, event + 0x30)?;
            for index in 0..2 {
                swap_u16(bytes, event + 0x34 + index * 2)?;
            }
            for index in 0..3 {
                swap_u16(bytes, event + 0x38 + index * 2)?;
            }
        }
        3 => {
            for index in 0..2 {
                swap_personal(bytes, event + index * PERSONAL_ID_SIZE)?;
            }
            swap_u32(bytes, event + 0x28)?;
            for index in 0..2 {
                swap_u16(bytes, event + 0x2C + index * 2)?;
            }
        }
        4 => {
            swap_u32(bytes, event)?;
            swap_u16(bytes, event + 4)?;
        }
        5 => {
            for index in 0..5 {
                swap_u32(bytes, event + index * 4)?;
            }
        }
        _ => {}
    }
    for index in 0..5 {
        swap_personal(bytes, offset + 0x50 + index * PERSONAL_ID_SIZE)?;
    }
    swap_u32(bytes, offset + 0xB4)
}

fn swap_event_common(
    bytes: &mut [u8],
    offset: usize,
    _direction: SwapDirection,
    legacy: bool,
) -> Result<(), GciError> {
    for index in 0..8 {
        swap_u16(bytes, offset + 4 + index * 2)?;
    }
    swap_u32(bytes, offset + 0x14)?;
    for index in 0..5 {
        let area = offset + 0x18 + index * 0x34;
        swap_u16(bytes, area + 2)?;
        swap_u16(bytes, area + 4)?;
        swap_u16(bytes, area + 6)?;
        if legacy {
            for data in 0..11 {
                swap_u32(bytes, area + 8 + data * 4)?;
            }
            continue;
        }
        let event_type = bytes[area];
        let event_id = bytes[area + 1];
        let data = area + 8;
        match (event_type, event_id) {
            (_, 117) => {
                for field in [0, 4, 8, 12] {
                    swap_u32(bytes, data + field)?;
                }
                swap_u16(bytes, data + 16)?;
                swap_u16(bytes, data + 18)?;
            }
            (1, 7) => {
                for field in 0..6 {
                    swap_u16(bytes, data + field * 2)?;
                }
            }
            (_, 8) if event_type == 15 => {
                swap_u16(bytes, data)?;
                for field in 2..8 {
                    swap_u16(bytes, data + field * 2)?;
                }
            }
            (_, 9) if event_type == 14 => {
                for field in 0..4 {
                    swap_u32(bytes, data + field * 4)?;
                }
                for field in [16, 18, 20, 22] {
                    swap_u16(bytes, data + field)?;
                }
            }
            (_, 10) if event_type == 69 => {
                swap_u16(bytes, data)?;
                swap_u16(bytes, data + 2)?;
            }
            (_, 54) if event_type == 112 => {
                swap_u16(bytes, data)?;
                swap_u16(bytes, data + 2)?;
                swap_ymd(bytes, data + 4)?;
            }
            (24, 0) | (66, 13) | (111, 34) => swap_u16(bytes, data)?,
            (_, 0) if event_type == 17 => {
                for player in 0..4 {
                    swap_u16(bytes, data + player * 2)?;
                }
            }
            (_, 0) if event_type == 29 || event_type == 100 => {
                swap_u32(bytes, data)?;
                swap_personal(bytes, data + 4)?;
                swap_u16(bytes, data + 0x18)?;
                swap_u16(bytes, data + 0x1A)?;
            }
            (_, 0) if event_type == 32 => {
                for item in 0..5 {
                    swap_u16(bytes, data + item * 2)?;
                }
                swap_u16(bytes, data + 10)?;
                swap_u16(bytes, data + 12)?;
            }
            (_, 0) if event_type == 106 => {
                swap_personal(bytes, data)?;
                swap_u16(bytes, data + 0x14)?;
            }
            (_, 0) if event_type == 59 => {
                swap_personal(bytes, data)?;
                for cloth in 0..10 {
                    swap_u16(bytes, data + 0x16 + cloth * 2)?;
                }
            }
            _ => {}
        }
    }
    for field in [0x11C, 0x120, 0x124, 0x128] {
        swap_u32(bytes, offset + field)?;
    }
    swap_u16(bytes, offset + 0x12C)?;
    swap_u16(bytes, offset + 0x12E)
}

fn swap_bridge(bytes: &mut [u8], offset: usize, direction: SwapDirection) -> Result<(), GciError> {
    repack_1_1_6(&mut bytes[offset + 2], direction);
    swap_ymd(bytes, offset + 4)
}

fn swap_mushroom_time(
    bytes: &mut [u8],
    offset: usize,
    direction: SwapDirection,
) -> Result<(), GciError> {
    let raw = read_u16(bytes, offset, direction)?;
    let (year, month) = match direction {
        SwapDirection::FromBigEndian => ((raw >> 4) & 0xFFF, raw & 0xF),
        SwapDirection::ToBigEndian => (raw & 0xFFF, (raw >> 12) & 0xF),
    };
    let packed = match direction {
        SwapDirection::FromBigEndian => year | (month << 12),
        SwapDirection::ToBigEndian => (year << 4) | month,
    };
    write_u16(bytes, offset, packed, direction)?;
    repack_5_3(&mut bytes[offset + 2], direction);
    repack_5_3(&mut bytes[offset + 3], direction);
    repack_4_1_3(&mut bytes[offset + 4], direction);
    Ok(())
}

fn swap_fish_record(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    swap_personal(bytes, offset)?;
    swap_time(bytes, offset + 0x14)?;
    swap_u32(bytes, offset + 0x1C)
}

fn swap_mask_cat(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    swap_personal(bytes, offset)?;
    // mMC_design is aligned at 0x20 and occupies 0x200 bytes; the three
    // one-byte state fields then precede the RTC at 0x223.
    swap_time(bytes, offset + 0x223)
}

fn swap_anm_return(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    swap_u16(bytes, offset)?;
    swap_time(bytes, offset + 4)
}

fn repack_home_size(first: &mut u8, second: &mut u8, direction: SwapDirection) {
    let a = *first;
    let b = *second;
    match direction {
        SwapDirection::FromBigEndian => {
            *first = ((a & 3) << 6) | (((a >> 2) & 7) << 3) | ((a >> 5) & 7);
            *second = ((b & 0xF) << 4)
                | (((b >> 4) & 1) << 3)
                | (((b >> 5) & 1) << 2)
                | (((b >> 6) & 1) << 1)
                | (b >> 7);
        }
        SwapDirection::ToBigEndian => {
            *first = ((a & 7) << 5) | (((a >> 3) & 7) << 2) | (a >> 6);
            *second = ((b & 1) << 7)
                | (((b >> 1) & 1) << 6)
                | (((b >> 2) & 1) << 5)
                | (((b >> 3) & 1) << 4)
                | (b >> 4);
        }
    }
}

fn swap_private_verified(
    bytes: &mut [u8],
    base: usize,
    direction: SwapDirection,
    legacy: bool,
) -> Result<(), GciError> {
    require_from(bytes, base, PRIVATE_SIZE)?;
    swap_personal(bytes, base)?;
    repack_111_5(&mut bytes[base + 0x17], direction);
    for index in 0..30 {
        swap_u16(bytes, base + 0x27 + index * 2)?;
    }
    for index in 0..15 {
        swap_u16(bytes, base + 0x68 + index * 2)?;
        swap_quest_delivery(
            &mut bytes[base + 0x94 + index * QUEST_DELIVERY_SIZE..],
            direction,
        )?;
    }
    for index in 0..5 {
        swap_quest_errand_impl(
            &mut bytes[base + 0x2EC + index * QUEST_ERRAND_SIZE..],
            direction,
            legacy,
        )?;
    }
    swap_u32(bytes, base + 0x88)?;
    swap_u32(bytes, base + 0x8C)?;
    swap_u32(bytes, base + 0x90)?;
    swap_u16(bytes, base + 0x4A4)?;
    for index in 0..10 {
        swap_mail(&mut bytes[base + 0x4E0 + index * MAIL_SIZE..], direction)?;
    }
    swap_u16(bytes, base + 0x1084)?;
    swap_u16(bytes, base + 0x1088)?;
    swap_u16(bytes, base + 0x108A)?;
    swap_animal_personal(bytes, base + 0x108C)?;
    swap_u16(bytes, base + 0x109A + 6)?;
    swap_u16(bytes, base + 0x10A4)?;
    for index in 0..5 {
        swap_u16(bytes, base + 0x10A8 + index * 4)?;
    }
    for index in 0..2 {
        swap_u32(bytes, base + 0x10D4 + index * 4)?;
    }
    swap_u16(bytes, base + 0x10DC)?;
    if !legacy {
        repack_1_7(&mut bytes[base + 0x10DC + 0x15], direction);
    }
    swap_u32(bytes, base + 0x10F4)?;
    swap_u16(bytes, base + 0x10F8)?;
    for index in 0..43 {
        swap_u32(bytes, base + 0x1108 + index * 4)?;
    }
    for index in 0..3 {
        swap_u32(bytes, base + 0x11B4 + index * 4)?;
        swap_u32(bytes, base + 0x11C0 + index * 4)?;
        swap_u32(bytes, base + 0x11CC + index * 4)?;
    }
    for index in 0..2 {
        swap_u32(bytes, base + 0x11D4 + index * 4)?;
    }
    for index in 0..8 {
        swap_u16(bytes, base + 0x11DC + index * 0x0A + 8)?;
    }
    swap_u32(bytes, base + 0x122C)?;
    swap_u32(bytes, base + 0x2348)?;
    for index in 0..12 {
        swap_u32(bytes, base + 0x234C + index * 4)?;
        swap_u32(bytes, base + 0x237C + index * 4)?;
    }
    swap_u16(bytes, base + 0x23AC)?;
    swap_u16(bytes, base + 0x23B0)?;
    swap_u16(bytes, base + 0x23B4 + 0x04)?;
    swap_u16(bytes, base + 0x23BE)?;
    swap_u16(bytes, base + 0x23C4)?;
    swap_u16(bytes, base + 0x23D8)?;
    swap_u32(bytes, base + 0x23DC)?;
    swap_u16(bytes, base + 0x23E0)?;
    Ok(())
}

fn swap_quest_base(
    bytes: &mut [u8],
    offset: usize,
    direction: SwapDirection,
) -> Result<(), GciError> {
    require_from(bytes, offset, QUEST_BASE_SIZE)?;
    let raw = read_u16(bytes, offset, direction)?;
    let (quest_type, quest_kind, enabled, progress, reward, unused) = match direction {
        SwapDirection::FromBigEndian => (
            (raw >> 14) & 0x3,
            (raw >> 8) & 0x3F,
            (raw >> 7) & 1,
            (raw >> 3) & 0xF,
            (raw >> 2) & 1,
            raw & 0x3,
        ),
        SwapDirection::ToBigEndian => (
            raw & 0x3,
            (raw >> 2) & 0x3F,
            (raw >> 8) & 1,
            (raw >> 9) & 0xF,
            (raw >> 13) & 1,
            (raw >> 14) & 0x3,
        ),
    };
    let packed = match direction {
        SwapDirection::FromBigEndian => {
            quest_type
                | (quest_kind << 2)
                | (enabled << 8)
                | (progress << 9)
                | (reward << 13)
                | (unused << 14)
        }
        SwapDirection::ToBigEndian => {
            (quest_type << 14)
                | (quest_kind << 8)
                | (enabled << 7)
                | (progress << 3)
                | (reward << 2)
                | unused
        }
    };
    write_u16(bytes, offset, packed, direction)?;
    swap_u16(bytes, offset + 2 + 6)?;
    Ok(())
}

fn swap_personal(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    require_from(bytes, offset, PERSONAL_ID_SIZE)?;
    swap_u16(bytes, offset + 0x10)?;
    swap_u16(bytes, offset + 0x12)?;
    Ok(())
}

fn swap_animal_personal(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    require_from(bytes, offset, ANIMAL_PERSONAL_ID_SIZE)?;
    swap_u16(bytes, offset)?;
    swap_u16(bytes, offset + 2)?;
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize, direction: SwapDirection) -> Result<u16, GciError> {
    require_from(bytes, offset, 2)?;
    let pair = [bytes[offset], bytes[offset + 1]];
    Ok(match direction {
        SwapDirection::FromBigEndian => u16::from_be_bytes(pair),
        SwapDirection::ToBigEndian => u16::from_le_bytes(pair),
    })
}

fn read_u32(bytes: &[u8], offset: usize, direction: SwapDirection) -> Result<u32, GciError> {
    require_from(bytes, offset, 4)?;
    let word = [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ];
    Ok(match direction {
        SwapDirection::FromBigEndian => u32::from_be_bytes(word),
        SwapDirection::ToBigEndian => u32::from_le_bytes(word),
    })
}

fn write_u16(
    bytes: &mut [u8],
    offset: usize,
    value: u16,
    direction: SwapDirection,
) -> Result<(), GciError> {
    require_from(bytes, offset, 2)?;
    let encoded = match direction {
        SwapDirection::FromBigEndian => value.to_le_bytes(),
        SwapDirection::ToBigEndian => value.to_be_bytes(),
    };
    bytes[offset..offset + 2].copy_from_slice(&encoded);
    Ok(())
}

fn swap_u16(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    require_from(bytes, offset, 2)?;
    bytes[offset..offset + 2].reverse();
    Ok(())
}

fn swap_u32(bytes: &mut [u8], offset: usize) -> Result<(), GciError> {
    require_from(bytes, offset, 4)?;
    bytes[offset..offset + 4].reverse();
    Ok(())
}

fn seal_checksum(
    bytes: &mut [u8],
    start: usize,
    size: usize,
    checksum_offset: usize,
) -> Result<(), GciError> {
    require_from(bytes, start, size)?;
    let checksum = start + checksum_offset;
    require_from(bytes, checksum, 2)?;
    bytes[checksum..checksum + 2].fill(0);
    let value = checksum_be(&bytes[start..start + size], 0);
    bytes[checksum..checksum + 2].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn require(bytes: &[u8], expected: usize) -> Result<(), GciError> {
    require_from(bytes, 0, expected)
}

fn require_from(bytes: &[u8], offset: usize, size: usize) -> Result<(), GciError> {
    let found = bytes.len().saturating_sub(offset);
    if found < size {
        return Err(GciError::Truncated {
            found,
            expected: size,
        });
    }
    Ok(())
}

fn repack_5_3(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    *value = match direction {
        SwapDirection::FromBigEndian => ((raw & 0x07) << 5) | ((raw >> 3) & 0x1F),
        SwapDirection::ToBigEndian => ((raw & 0x1F) << 3) | ((raw >> 5) & 0x07),
    };
}

fn repack_7_1(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    *value = match direction {
        SwapDirection::FromBigEndian => raw.rotate_right(1),
        SwapDirection::ToBigEndian => raw.rotate_left(1),
    };
}

fn repack_1_7(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    *value = match direction {
        SwapDirection::FromBigEndian => raw.rotate_right(1),
        SwapDirection::ToBigEndian => raw.rotate_left(1),
    };
}

fn repack_111_5(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    let a = match direction {
        SwapDirection::FromBigEndian => (raw >> 7) & 1,
        SwapDirection::ToBigEndian => raw & 1,
    };
    let b = match direction {
        SwapDirection::FromBigEndian => (raw >> 6) & 1,
        SwapDirection::ToBigEndian => (raw >> 1) & 1,
    };
    let c = match direction {
        SwapDirection::FromBigEndian => (raw >> 5) & 1,
        SwapDirection::ToBigEndian => (raw >> 2) & 1,
    };
    let d = match direction {
        SwapDirection::FromBigEndian => (raw >> 4) & 1,
        SwapDirection::ToBigEndian => (raw >> 3) & 1,
    };
    let e = match direction {
        SwapDirection::FromBigEndian => (raw >> 3) & 1,
        SwapDirection::ToBigEndian => (raw >> 4) & 1,
    };
    let f = match direction {
        SwapDirection::FromBigEndian => raw & 0x07,
        SwapDirection::ToBigEndian => (raw >> 5) & 0x07,
    };
    *value = match direction {
        SwapDirection::FromBigEndian => (f << 5) | (e << 4) | (d << 3) | (c << 2) | (b << 1) | a,
        SwapDirection::ToBigEndian => (a << 7) | (b << 6) | (c << 5) | (d << 4) | (e << 3) | f,
    };
}

fn repack_1_1_6(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    let (a, b, rest) = match direction {
        SwapDirection::FromBigEndian => ((raw >> 7) & 1, (raw >> 6) & 1, raw & 0x3F),
        SwapDirection::ToBigEndian => (raw & 1, (raw >> 1) & 1, (raw >> 2) & 0x3F),
    };
    *value = match direction {
        SwapDirection::FromBigEndian => (rest << 2) | (b << 1) | a,
        SwapDirection::ToBigEndian => (a << 7) | (b << 6) | rest,
    };
}

fn repack_4_1_3(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    let (a, b, c) = match direction {
        SwapDirection::FromBigEndian => ((raw >> 4) & 0xF, (raw >> 3) & 1, raw & 7),
        SwapDirection::ToBigEndian => (raw & 0xF, (raw >> 4) & 1, (raw >> 5) & 7),
    };
    *value = match direction {
        SwapDirection::FromBigEndian => (c << 5) | (b << 4) | a,
        SwapDirection::ToBigEndian => (a << 4) | (b << 3) | c,
    };
}

fn repack_11111_3(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    let bits = match direction {
        SwapDirection::FromBigEndian => [
            (raw >> 7) & 1,
            (raw >> 6) & 1,
            (raw >> 5) & 1,
            (raw >> 4) & 1,
            (raw >> 3) & 1,
            raw & 7,
        ],
        SwapDirection::ToBigEndian => [
            raw & 1,
            (raw >> 1) & 1,
            (raw >> 2) & 1,
            (raw >> 3) & 1,
            (raw >> 4) & 1,
            (raw >> 5) & 7,
        ],
    };
    *value = match direction {
        SwapDirection::FromBigEndian => {
            (bits[5] << 5)
                | (bits[4] << 4)
                | (bits[3] << 3)
                | (bits[2] << 2)
                | (bits[1] << 1)
                | bits[0]
        }
        SwapDirection::ToBigEndian => {
            (bits[0] << 7)
                | (bits[1] << 6)
                | (bits[2] << 5)
                | (bits[3] << 4)
                | (bits[4] << 3)
                | bits[5]
        }
    };
}

fn repack_2222(value: &mut u8, direction: SwapDirection) {
    let raw = *value;
    let bits = match direction {
        SwapDirection::FromBigEndian => [(raw >> 6) & 3, (raw >> 4) & 3, (raw >> 2) & 3, raw & 3],
        SwapDirection::ToBigEndian => [raw & 3, (raw >> 2) & 3, (raw >> 4) & 3, (raw >> 6) & 3],
    };
    *value = match direction {
        SwapDirection::FromBigEndian => (bits[3] << 6) | (bits[2] << 4) | (bits[1] << 2) | bits[0],
        SwapDirection::ToBigEndian => (bits[0] << 6) | (bits[1] << 4) | (bits[2] << 2) | bits[3],
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn c_layout_sizes_and_offsets_are_pinned() {
        assert_eq!(size_of::<RtcTime>(), 0x08);
        assert_eq!(offset_of!(RtcTime, year), 0x06);
        assert_eq!(size_of::<RtcYmd>(), 0x04);
        assert_eq!(size_of::<PersonalId>(), 0x14);
        assert_eq!(offset_of!(PersonalId, land_id), 0x12);
        assert_eq!(size_of::<AnimalPersonalId>(), 0x0E);
        assert_eq!(size_of::<LandInfo>(), 0x0C);
        assert_eq!(offset_of!(LandInfo, id), 0x0A);
        assert_eq!(size_of::<SaveCheck>(), 0x14);
        assert_eq!(offset_of!(SaveCheck, checksum), 0x12);
        assert_eq!(size_of::<SavePrefix>(), 0x20);
        assert_eq!(offset_of!(SavePrefix, copy_protect), 0x1A);
        assert_eq!(size_of::<QuestBase>(), QUEST_BASE_SIZE);
        assert_eq!(size_of::<QuestDelivery>(), QUEST_DELIVERY_SIZE);
        assert_eq!(size_of::<QuestErrand>(), QUEST_ERRAND_SIZE);
        assert_eq!(size_of::<QuestContest>(), QUEST_CONTEST_SIZE);
        assert_eq!(size_of::<MailName>(), 0x16);
        assert_eq!(size_of::<MailHeader>(), 0x2C);
        assert_eq!(size_of::<MailContent>(), 0xFC);
        assert_eq!(size_of::<Mail>(), MAIL_SIZE);
        assert_eq!(size_of::<SaveTBytes>(), SAVE_T_SIZE);
        assert_eq!(align_of::<SaveTBytes>(), 4);
        assert_eq!(size_of::<KeepMailBytes>(), KEEP_MAIL_SIZE);
        assert_eq!(align_of::<KeepMailBytes>(), 32);
        assert_eq!(KEEP_ORIGINAL_SIZE, 0xCCA0);
        assert_eq!(KEEP_ORIGINAL_TRAILER_OFFSET, 0xCC80);
        assert_eq!(KEEP_DIARY_SIZE, 0xBA20);
    }

    #[test]
    fn save_t_verified_walker_round_trips_pinned_fields() {
        let mut bytes = vec![0u8; SAVE_T_SIZE];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
        }
        let original = bytes.clone();
        swap_save_t_verified(&mut bytes, SwapDirection::FromBigEndian).expect("full Save_t");
        swap_save_t_verified(&mut bytes, SwapDirection::ToBigEndian).expect("full Save_t");
        let first = bytes
            .iter()
            .zip(original.iter())
            .position(|(actual, expected)| actual != expected);
        assert_eq!(first, None, "first mismatch: {first:?}");
    }

    #[test]
    fn gci_composite_round_trips_save_and_aram_regions() {
        let mut bytes = vec![0u8; RECORD_SIZE];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(13).wrapping_add(3);
        }
        let original = bytes.clone();
        swap_gci_composite(&mut bytes, 1, false, SwapDirection::FromBigEndian)
            .expect("modern composite");
        swap_gci_composite(&mut bytes, 1, false, SwapDirection::ToBigEndian)
            .expect("modern composite reverse");
        assert_eq!(bytes, original);
    }

    #[test]
    fn truncated_composites_are_typed_failures() {
        assert_eq!(
            swap_save_t_verified(&mut [0u8; SAVE_T_SIZE - 1], SwapDirection::FromBigEndian),
            Err(GciError::Truncated {
                found: SAVE_T_SIZE - 1,
                expected: SAVE_T_SIZE
            })
        );
        assert_eq!(
            crate::error::SaveResult::Invalid,
            GciError::Truncated {
                found: 0,
                expected: 1
            }
            .as_save_result()
        );
        assert_eq!(
            swap_gci_composite(
                &mut vec![0u8; RECORD_SIZE],
                2,
                false,
                SwapDirection::FromBigEndian
            ),
            Err(GciError::InvalidSaveCopy { found: 2 })
        );
    }
}
