//! Bounded typed byte-swap for source-grounded save structs from
//! `pc/src/pc_save_bswap.c`.
//!
//! # Provenance
//!
//! A transcription of the leaf and Mail swap helpers at
//! `pc/src/pc_save_bswap.c:181-220`
//! and the layouts they operate on:
//!
//! | Rust type          | C type              | Header                  | C lines |
//! |--------------------|---------------------|-------------------------|---------|
//! | [`RtcTime`]        | `lbRTC_time_c`      | `include/lb_rtc.h`      | 26-34   |
//! | [`RtcYmd`]         | `lbRTC_ymd_c`       | `include/lb_rtc.h`      | 46-50   |
//! | [`PersonalId`]     | `PersonalID_c`      | `include/m_personal_id.h` | 19-25 |
//! | [`AnimalPersonalId`] | `AnmPersonalID_c` | `include/m_npc_personal_id.h` | 24-31 |
//! | [`LandInfo`]       | `mLd_land_info_c`   | `include/m_land_h.h`    | 14-19   |
//! | [`MailName`]       | `Mail_nm_c`         | `include/m_mail.h`      | 68-72   |
//! | [`MailHeader`]     | `Mail_hdr_c`        | `include/m_mail.h`      | 82-86   |
//! | [`MailContent`]    | `Mail_ct_c`         | `include/m_mail.h`      | 88-97   |
//! | [`Mail`]           | `Mail_c`            | `include/m_mail.h`      | 99-104  |
//!
//! The upstream helpers are `swap_lbRTC_time`, `swap_lbRTC_ymd`,
//! `swap_PersonalID`, `swap_AnmPersonalID`, `swap_mLd_land_info`,
//! `swap_Mail_nm`, `swap_Mail_hdr`, and `swap_Mail`. The Mail helpers recurse
//! into the embedded PersonalIDs and swap `Mail_c::present`; Mail content is
//! byte-only and is untouched.
//!
//! # Semantics
//!
//! These are *memory-layout mirrors*, not decoded domain types. A record read
//! off a big-endian image is copied into memory verbatim, so on a
//! little-endian host each `u16` field holds a byte-reversed value until it is
//! swapped. [`TypedSwap::swap_from_be`] performs that correction in place;
//! [`TypedSwap::swap_to_be`] performs the inverse on the way back to disk.
//! Both are byte-identical: `swap16` is its own inverse.
//!
//! * Every non-`u16` byte — names, ids below `0x100`, and the explicit
//!   [`LandInfo::pad`] alignment byte at offset `0x09` — is preserved verbatim.
//! * The only fields swapped are the ones upstream swaps: `RtcTime::year`,
//!   `RtcYmd::year`, `PersonalId::{player_id, land_id}`,
//!   `AnimalPersonalId::{npc_id, land_id}`, `LandInfo::id`, the embedded
//!   PersonalIDs in `MailName`/`MailHeader`, and `Mail::present`.
//! * Mail name arrays, name type, the explicit name pad, all MailContent bytes,
//!   and all other alignment bytes are preserved verbatim.
//! * No allocation, no `unsafe`, no dependencies. Conversion to and from bytes
//!   is a field-by-field copy into fixed-size arrays.
//!
//! # Explicit non-goals
//!
//! No whole-`Save_t` or Mail-array coverage, quest walkers, ARAM, card I/O, or
//! runtime integration. The composite slice stops at one `Mail_c`; the larger
//! walkers (`swap_Mail_array`, `swap_Quest`, `swap_Save`, …) stay unported.

#![allow(clippy::module_name_repetitions)]

/// Direction of a byte swap, mirroring `pc_bswap_dir_t`.
///
/// `swap16` is its own inverse, so both directions are the same operation; the
/// enum exists so call sites state their intent and stay readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// `PC_BSWAP_FROM_BE`: big-endian disk image to native in-memory layout.
    FromBe,
    /// `PC_BSWAP_TO_BE`: native in-memory layout back to the big-endian image.
    ToBe,
}

impl Direction {
    /// The opposite direction.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Direction::FromBe => Direction::ToBe,
            Direction::ToBe => Direction::FromBe,
        }
    }
}

/// Shared surface for the leaf structs and bounded Mail mirrors.
///
/// Every implementor has a single, fixed layout and swaps exactly the `u16`
/// fields upstream swaps. `swap_from_be` and `swap_to_be` are the same
/// operation; applying either twice restores the original bytes.
pub trait TypedSwap {
    /// `sizeof` of the corresponding C struct, in bytes.
    const SIZE: usize;

    /// Swap every `u16` field in place, big-endian image to native layout.
    ///
    /// Equivalent to `swap_<name>()` in `pc/src/pc_save_bswap.c`.
    fn swap_from_be(&mut self);

    /// Swap every `u16` field in place, native layout to big-endian image.
    ///
    /// Identical bytes to [`TypedSwap::swap_from_be`]; `swap16` is an
    /// involution, so the two directions coincide.
    fn swap_to_be(&mut self) {
        self.swap_from_be();
    }

    /// Apply the swap named by `dir`.
    fn swap(&mut self, dir: Direction) {
        match dir {
            Direction::FromBe => self.swap_from_be(),
            Direction::ToBe => self.swap_to_be(),
        }
    }
}

/// `lbRTC_time_c` from `include/lb_rtc.h:26-34`: `u8` fields through offset
/// `0x05`, then a big-endian `year` at `0x06`. Size `0x08`, align 2.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RtcTime {
    /// Seconds (`lbRTC_sec_t`).
    pub sec: u8,
    /// Minutes (`lbRTC_min_t`).
    pub min: u8,
    /// Hours (`lbRTC_hour_t`).
    pub hour: u8,
    /// Day of month (`lbRTC_day_t`).
    pub day: u8,
    /// Day of week (`lbRTC_weekday_t`).
    pub weekday: u8,
    /// Month (`lbRTC_month_t`).
    pub month: u8,
    /// Year (`lbRTC_year_t`); the only swapped field.
    pub year: u16,
}

impl RtcTime {
    /// `sizeof(lbRTC_time_c)`.
    pub const SIZE: usize = 0x08;

    /// Copy the native memory image into a fixed-size byte array.
    ///
    /// Multi-byte fields use native byte order, matching what a C `memcpy` of
    /// the struct would produce on this host.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x08] {
        let year = self.year.to_ne_bytes();
        [
            self.sec,
            self.min,
            self.hour,
            self.day,
            self.weekday,
            self.month,
            year[0],
            year[1],
        ]
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x08]) -> Self {
        Self {
            sec: bytes[0],
            min: bytes[1],
            hour: bytes[2],
            day: bytes[3],
            weekday: bytes[4],
            month: bytes[5],
            year: u16::from_ne_bytes([bytes[6], bytes[7]]),
        }
    }
}

impl TypedSwap for RtcTime {
    const SIZE: usize = RtcTime::SIZE;

    fn swap_from_be(&mut self) {
        self.year = self.year.swap_bytes();
    }
}

/// `lbRTC_ymd_c` from `include/lb_rtc.h:46-50`: big-endian `year` at `0x00`,
/// then `month` and `day`. Size `0x04`, align 2.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RtcYmd {
    /// Year (`lbRTC_year_t`); the only swapped field.
    pub year: u16,
    /// Month (`lbRTC_month_t`).
    pub month: u8,
    /// Day (`lbRTC_day_t`).
    pub day: u8,
}

impl RtcYmd {
    /// `sizeof(lbRTC_ymd_c)`.
    pub const SIZE: usize = 0x04;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x04] {
        let year = self.year.to_ne_bytes();
        [year[0], year[1], self.month, self.day]
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x04]) -> Self {
        Self {
            year: u16::from_ne_bytes([bytes[0], bytes[1]]),
            month: bytes[2],
            day: bytes[3],
        }
    }
}

impl TypedSwap for RtcYmd {
    const SIZE: usize = RtcYmd::SIZE;

    fn swap_from_be(&mut self) {
        self.year = self.year.swap_bytes();
    }
}

/// `PersonalID_c` from `include/m_personal_id.h:19-25`: two 8-byte names, then
/// big-endian `player_id` and `land_id`. Size `0x14`, align 2.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PersonalId {
    /// `player_name`: eight raw name bytes, never swapped.
    pub player_name: [u8; 8],
    /// `land_name`: eight raw name bytes, never swapped.
    pub land_name: [u8; 8],
    /// `player_id` at `0x10`; swapped.
    pub player_id: u16,
    /// `land_id` at `0x12`; swapped.
    pub land_id: u16,
}

impl PersonalId {
    /// `sizeof(PersonalID_c)`.
    pub const SIZE: usize = 0x14;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x14] {
        let mut out = [0u8; 0x14];
        out[0x00..0x08].copy_from_slice(&self.player_name);
        out[0x08..0x10].copy_from_slice(&self.land_name);
        out[0x10..0x12].copy_from_slice(&self.player_id.to_ne_bytes());
        out[0x12..0x14].copy_from_slice(&self.land_id.to_ne_bytes());
        out
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x14]) -> Self {
        let mut player_name = [0u8; 8];
        let mut land_name = [0u8; 8];
        player_name.copy_from_slice(&bytes[0x00..0x08]);
        land_name.copy_from_slice(&bytes[0x08..0x10]);
        Self {
            player_name,
            land_name,
            player_id: u16::from_ne_bytes([bytes[0x10], bytes[0x11]]),
            land_id: u16::from_ne_bytes([bytes[0x12], bytes[0x13]]),
        }
    }
}

impl TypedSwap for PersonalId {
    const SIZE: usize = PersonalId::SIZE;

    fn swap_from_be(&mut self) {
        self.player_id = self.player_id.swap_bytes();
        self.land_id = self.land_id.swap_bytes();
    }
}

/// `Mail_nm_c` from `include/m_mail.h:68-72`: a `PersonalID_c`, one-byte
/// name type, and one implicit alignment byte at `0x15`. Size `0x16`, align 2.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct MailName {
    /// Embedded player/land identity at `0x00`; its two IDs are swapped.
    pub personal_id: PersonalId,
    /// Mail name kind (`Mail_nm_c::type`) at `0x14`; never swapped.
    pub type_: u8,
    /// The C struct's implicit alignment padding at `0x15`, preserved verbatim.
    pub pad: u8,
}

impl MailName {
    /// `sizeof(Mail_nm_c)`.
    pub const SIZE: usize = 0x16;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x16] {
        let mut out = [0u8; 0x16];
        out[0x00..0x14].copy_from_slice(&self.personal_id.to_bytes());
        out[0x14] = self.type_;
        out[0x15] = self.pad;
        out
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x16]) -> Self {
        let mut personal_id = [0u8; 0x14];
        personal_id.copy_from_slice(&bytes[0x00..0x14]);
        Self {
            personal_id: PersonalId::from_bytes(&personal_id),
            type_: bytes[0x14],
            pad: bytes[0x15],
        }
    }
}

impl TypedSwap for MailName {
    const SIZE: usize = MailName::SIZE;

    fn swap_from_be(&mut self) {
        self.personal_id.swap_from_be();
    }
}

/// `Mail_hdr_c` from `include/m_mail.h:82-86`: recipient at `0x00` and
/// sender at `0x16`. Size `0x2C`, align 2.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct MailHeader {
    /// Recipient name at `0x00`.
    pub recipient: MailName,
    /// Sender name at `0x16`.
    pub sender: MailName,
}

impl MailHeader {
    /// `sizeof(Mail_hdr_c)`.
    pub const SIZE: usize = 0x2C;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x2C] {
        let mut out = [0u8; 0x2C];
        out[0x00..0x16].copy_from_slice(&self.recipient.to_bytes());
        out[0x16..0x2C].copy_from_slice(&self.sender.to_bytes());
        out
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x2C]) -> Self {
        let mut recipient = [0u8; 0x16];
        let mut sender = [0u8; 0x16];
        recipient.copy_from_slice(&bytes[0x00..0x16]);
        sender.copy_from_slice(&bytes[0x16..0x2C]);
        Self {
            recipient: MailName::from_bytes(&recipient),
            sender: MailName::from_bytes(&sender),
        }
    }
}

impl TypedSwap for MailHeader {
    const SIZE: usize = MailHeader::SIZE;

    fn swap_from_be(&mut self) {
        self.recipient.swap_from_be();
        self.sender.swap_from_be();
    }
}

/// `Mail_ct_c` from `include/m_mail.h:88-97`: four byte fields followed by
/// `header[0x18]`, `body[0xC0]`, and `footer[0x20]`. Size `0xFC`, align 1;
/// this record contains no multi-byte fields and is never changed by the
/// upstream Mail swap helper.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MailContent {
    /// Font byte at `0x00`, never swapped.
    pub font: u8,
    /// Header background start byte at `0x01`, never swapped.
    pub header_back_start: u8,
    /// Mail type byte at `0x02`, never swapped.
    pub mail_type: u8,
    /// Paper type byte at `0x03`, never swapped.
    pub paper_type: u8,
    /// Header bytes at `0x04..0x1C`, never swapped.
    pub header: [u8; 0x18],
    /// Body bytes at `0x1C..0xDC`, never swapped.
    pub body: [u8; 0xC0],
    /// Footer bytes at `0xDC..0xFC`, never swapped.
    pub footer: [u8; 0x20],
}

impl Default for MailContent {
    fn default() -> Self {
        Self {
            font: 0,
            header_back_start: 0,
            mail_type: 0,
            paper_type: 0,
            header: [0; 0x18],
            body: [0; 0xC0],
            footer: [0; 0x20],
        }
    }
}

impl MailContent {
    /// `sizeof(Mail_ct_c)`.
    pub const SIZE: usize = 0xFC;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0xFC] {
        let mut out = [0u8; 0xFC];
        out[0x00] = self.font;
        out[0x01] = self.header_back_start;
        out[0x02] = self.mail_type;
        out[0x03] = self.paper_type;
        out[0x04..0x1C].copy_from_slice(&self.header);
        out[0x1C..0xDC].copy_from_slice(&self.body);
        out[0xDC..0xFC].copy_from_slice(&self.footer);
        out
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0xFC]) -> Self {
        let mut header = [0u8; 0x18];
        let mut body = [0u8; 0xC0];
        let mut footer = [0u8; 0x20];
        header.copy_from_slice(&bytes[0x04..0x1C]);
        body.copy_from_slice(&bytes[0x1C..0xDC]);
        footer.copy_from_slice(&bytes[0xDC..0xFC]);
        Self {
            font: bytes[0x00],
            header_back_start: bytes[0x01],
            mail_type: bytes[0x02],
            paper_type: bytes[0x03],
            header,
            body,
            footer,
        }
    }
}

impl TypedSwap for MailContent {
    const SIZE: usize = MailContent::SIZE;

    fn swap_from_be(&mut self) {}
}

/// `Mail_c` from `include/m_mail.h:99-104`: header at `0x000`, present at
/// `0x02C`, and byte-only content at `0x02E`. Size `0x12A`, align 2.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Mail {
    /// Recipient/sender header at `0x000`; its embedded PersonalIDs are swapped.
    pub header: MailHeader,
    /// Present actor name at `0x02C`; swapped as `mActor_name_t` (`u16`).
    pub present: u16,
    /// Byte-only content at `0x02E`; preserved verbatim.
    pub content: MailContent,
}

impl Mail {
    /// `sizeof(Mail_c)`.
    pub const SIZE: usize = 0x12A;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x12A] {
        let mut out = [0u8; 0x12A];
        out[0x000..0x02C].copy_from_slice(&self.header.to_bytes());
        out[0x02C..0x02E].copy_from_slice(&self.present.to_ne_bytes());
        out[0x02E..0x12A].copy_from_slice(&self.content.to_bytes());
        out
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x12A]) -> Self {
        let mut header = [0u8; 0x2C];
        let mut content = [0u8; 0xFC];
        header.copy_from_slice(&bytes[0x000..0x02C]);
        content.copy_from_slice(&bytes[0x02E..0x12A]);
        Self {
            header: MailHeader::from_bytes(&header),
            present: u16::from_ne_bytes([bytes[0x02C], bytes[0x02D]]),
            content: MailContent::from_bytes(&content),
        }
    }
}

impl TypedSwap for Mail {
    const SIZE: usize = Mail::SIZE;

    fn swap_from_be(&mut self) {
        self.header.swap_from_be();
        self.present = self.present.swap_bytes();
    }
}

/// `AnmPersonalID_c` from `include/m_npc_personal_id.h:24-31`: big-endian
/// `npc_id` and `land_id`, an 8-byte town name, then `name_id` and `looks`.
/// Size `0x0E`, align 2, no padding.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct AnimalPersonalId {
    /// `npc_id` (`mActor_name_t`, `u16`) at `0x00`; swapped.
    pub npc_id: u16,
    /// `land_id` at `0x02`; swapped.
    pub land_id: u16,
    /// `land_name`: eight raw name bytes, never swapped.
    pub land_name: [u8; 8],
    /// `name_id` at `0x0C`: lower byte of the id, never swapped.
    pub name_id: u8,
    /// `looks` at `0x0D`: personality enum, never swapped.
    pub looks: u8,
}

impl AnimalPersonalId {
    /// `sizeof(AnmPersonalID_c)`.
    pub const SIZE: usize = 0x0E;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x0E] {
        let mut out = [0u8; 0x0E];
        out[0x00..0x02].copy_from_slice(&self.npc_id.to_ne_bytes());
        out[0x02..0x04].copy_from_slice(&self.land_id.to_ne_bytes());
        out[0x04..0x0C].copy_from_slice(&self.land_name);
        out[0x0C] = self.name_id;
        out[0x0D] = self.looks;
        out
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x0E]) -> Self {
        let mut land_name = [0u8; 8];
        land_name.copy_from_slice(&bytes[0x04..0x0C]);
        Self {
            npc_id: u16::from_ne_bytes([bytes[0x00], bytes[0x01]]),
            land_id: u16::from_ne_bytes([bytes[0x02], bytes[0x03]]),
            land_name,
            name_id: bytes[0x0C],
            looks: bytes[0x0D],
        }
    }
}

impl TypedSwap for AnimalPersonalId {
    const SIZE: usize = AnimalPersonalId::SIZE;

    fn swap_from_be(&mut self) {
        self.npc_id = self.npc_id.swap_bytes();
        self.land_id = self.land_id.swap_bytes();
    }
}

/// `mLd_land_info_c` from `include/m_land_h.h:14-19`: `name[8]`, `s8 exists`,
/// one alignment byte at `0x09`, then big-endian `id` at `0x0A`.
///
/// The C struct's pad byte is implicit; it is modelled explicitly here so the
/// byte image round-trips exactly and the swap provably leaves it alone.
/// Size `0x0C`, align 2.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LandInfo {
    /// `name`: eight raw name bytes, never swapped.
    pub name: [u8; 8],
    /// `exists` (`s8`) at `0x08`, never swapped.
    pub exists: i8,
    /// Alignment padding at `0x09`, never swapped, preserved verbatim.
    pub pad: u8,
    /// `id` at `0x0A`; the only swapped field.
    pub id: u16,
}

impl LandInfo {
    /// `sizeof(mLd_land_info_c)`.
    pub const SIZE: usize = 0x0C;

    /// Copy the native memory image into a fixed-size byte array.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 0x0C] {
        let mut out = [0u8; 0x0C];
        out[0x00..0x08].copy_from_slice(&self.name);
        out[0x08] = self.exists as u8;
        out[0x09] = self.pad;
        out[0x0A..0x0C].copy_from_slice(&self.id.to_ne_bytes());
        out
    }

    /// Rebuild the struct from a native memory image.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 0x0C]) -> Self {
        let mut name = [0u8; 8];
        name.copy_from_slice(&bytes[0x00..0x08]);
        Self {
            name,
            exists: bytes[0x08] as i8,
            pad: bytes[0x09],
            id: u16::from_ne_bytes([bytes[0x0A], bytes[0x0B]]),
        }
    }
}

impl TypedSwap for LandInfo {
    const SIZE: usize = LandInfo::SIZE;

    fn swap_from_be(&mut self) {
        self.id = self.id.swap_bytes();
    }
}

// Layout is the contract with the C headers; pin it at compile time.
const _: () = {
    assert!(core::mem::size_of::<RtcTime>() == 0x08);
    assert!(core::mem::align_of::<RtcTime>() == 2);
    assert!(core::mem::size_of::<RtcYmd>() == 0x04);
    assert!(core::mem::align_of::<RtcYmd>() == 2);
    assert!(core::mem::size_of::<PersonalId>() == 0x14);
    assert!(core::mem::align_of::<PersonalId>() == 2);
    assert!(core::mem::size_of::<AnimalPersonalId>() == 0x0E);
    assert!(core::mem::align_of::<AnimalPersonalId>() == 2);
    assert!(core::mem::size_of::<LandInfo>() == 0x0C);
    assert!(core::mem::align_of::<LandInfo>() == 2);
    assert!(core::mem::size_of::<MailName>() == 0x16);
    assert!(core::mem::align_of::<MailName>() == 2);
    assert!(core::mem::size_of::<MailHeader>() == 0x2C);
    assert!(core::mem::align_of::<MailHeader>() == 2);
    assert!(core::mem::size_of::<MailContent>() == 0xFC);
    assert!(core::mem::align_of::<MailContent>() == 1);
    assert!(core::mem::size_of::<Mail>() == 0x12A);
    assert!(core::mem::align_of::<Mail>() == 2);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn land_info_pad_byte_sits_at_offset_nine() {
        // Guard the explicit pad field against a reordering that would silently
        // shift `id` to 0x09 and break the byte image.
        let li = LandInfo {
            name: [0; 8],
            exists: 1,
            pad: 0x5A,
            id: 0x1234,
        };
        let bytes = li.to_bytes();
        assert_eq!(bytes[0x08], 1);
        assert_eq!(bytes[0x09], 0x5A);
        assert_eq!(&bytes[0x0A..], &0x1234u16.to_ne_bytes());
        assert_eq!(LandInfo::from_bytes(&bytes), li);
    }
}
