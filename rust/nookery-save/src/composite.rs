//! Verified wire-layout mirrors and bounded byte-swap walkers.
//!
//! The C implementation has a much larger dependency graph than this crate.
//! The types below cover the layouts pinned directly by the named save
//! headers, and [`swap_save_t_verified`] walks the Save_t header, every
//! Private_c quest/mail region, and land identity fields. Bytes belonging to
//! other Save_t substructures are preserved until their source layout is
//! ported; the function is therefore deliberately named `verified`, not
//! `complete`.

use crate::error::GciError;

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

/// Swap every source-pinned Save_t field in the header, Private_c quest/mail
/// regions, and land identity region.
pub fn swap_save_t_verified(bytes: &mut [u8], direction: SwapDirection) -> Result<(), GciError> {
    require(bytes, SAVE_T_SIZE)?;
    swap_u32(bytes, 0x00)?;
    swap_u32(bytes, 0x04)?;
    swap_u16(bytes, 0x08)?;
    swap_u16(bytes, 0x0A + 6)?;
    swap_u16(bytes, SAVE_CHECKSUM_OFFSET)?;
    swap_u32(bytes, 0x14)?;
    swap_u16(bytes, 0x1A)?;

    for player in 0..4 {
        let base = SAVE_PRIVATE_OFFSET + player * PRIVATE_SIZE;
        swap_private_verified(bytes, base, direction)?;
    }
    swap_u16(bytes, SAVE_LAND_INFO_OFFSET + 0x0A)?;
    Ok(())
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

fn swap_private_verified(
    bytes: &mut [u8],
    base: usize,
    direction: SwapDirection,
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
        swap_quest_errand(
            &mut bytes[base + 0x2EC + index * QUEST_ERRAND_SIZE..],
            direction,
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
    repack_1_7(&mut bytes[base + 0x10DC + 0x15], direction);
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
    }
}
