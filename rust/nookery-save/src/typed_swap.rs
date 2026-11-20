//! Bounded typed byte-swap for five leaf structs from `pc/src/pc_save_bswap.c`.
//!
//! # Provenance
//!
//! A transcription of the leaf swap helpers at `pc/src/pc_save_bswap.c:181-201`
//! and the layouts they operate on:
//!
//! | Rust type          | C type              | Header                  | C lines |
//! |--------------------|---------------------|-------------------------|---------|
//! | [`RtcTime`]        | `lbRTC_time_c`      | `include/lb_rtc.h`      | 26-34   |
//! | [`RtcYmd`]         | `lbRTC_ymd_c`       | `include/lb_rtc.h`      | 46-50   |
//! | [`PersonalId`]     | `PersonalID_c`      | `include/m_personal_id.h` | 19-25 |
//! | [`AnimalPersonalId`] | `AnmPersonalID_c` | `include/m_npc_personal_id.h` | 24-31 |
//! | [`LandInfo`]       | `mLd_land_info_c`   | `include/m_land_h.h`    | 14-19   |
//!
//! The upstream helpers are `swap_lbRTC_time`, `swap_lbRTC_ymd`,
//! `swap_PersonalID`, `swap_AnmPersonalID`, and `swap_mLd_land_info`; each is a
//! sequence of `swap16` calls on named `u16` fields and nothing else.
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
//!   `AnimalPersonalId::{npc_id, land_id}`, and `LandInfo::id`.
//! * No allocation, no `unsafe`, no dependencies. Conversion to and from bytes
//!   is a field-by-field copy into fixed-size arrays.
//!
//! # Explicit non-goals
//!
//! No `Save_t`, `Mail_c`, quest, or ARAM coverage. This module ports only the
//! five leaf helpers named above; the walkers that compose them
//! (`swap_Mail`, `swap_Quest`, `swap_Save`, …) stay unported.

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

/// Shared surface for the five leaf structs.
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
