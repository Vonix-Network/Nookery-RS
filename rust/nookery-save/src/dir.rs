//! Memory-card directory header (`CARDDir`) parsing.
//!
//! Layout is taken from `libs/dolphin/include/dolphin/card.h`; the filename
//! match and file-data offset rules are taken from `pc_gci_dir_has_name` and
//! `pc_gci_data_offset` in `pc/src/pc_gci.c`.  The header is stored
//! big-endian on disk and in the GCI container.

use crate::error::GciError;

/// `sizeof(CARDDir)`.
pub const HEADER_SIZE: usize = 0x40;
/// `CARD_FILENAME_MAX`, the size of the `fileName` field.
pub const FILE_NAME_MAX: usize = 32;

const GAME_NAME_OFF: usize = 0x00;
const COMPANY_OFF: usize = 0x04;
const BANNER_FORMAT_OFF: usize = 0x07;
const FILE_NAME_OFF: usize = 0x08;
const TIME_OFF: usize = 0x28;
const ICON_ADDR_OFF: usize = 0x2C;
const ICON_FORMAT_OFF: usize = 0x30;
const ICON_SPEED_OFF: usize = 0x32;
const PERMISSION_OFF: usize = 0x34;
const COPY_TIMES_OFF: usize = 0x35;
const START_BLOCK_OFF: usize = 0x36;
const LENGTH_OFF: usize = 0x38;
const COMMENT_ADDR_OFF: usize = 0x3C;

/// `CARD_STAT_BANNER_MASK` / `CARD_STAT_ICON_MASK`.
const FORMAT_MASK: u8 = 0x03;
/// `CARD_STAT_BANNER_C8`.
const BANNER_C8: u8 = 1;
/// `CARD_STAT_BANNER_RGB5A3`.
const BANNER_RGB5A3: u8 = 2;
/// `CARD_STAT_ICON_C8`.
const ICON_C8: u16 = 1;
/// `CARD_STAT_ICON_RGB5A3`.
const ICON_RGB5A3: u16 = 2;
/// `CARD_ICON_MAX`.
const ICON_MAX: u32 = 8;
/// `CARD_ICON_WIDTH * CARD_ICON_HEIGHT`.
const ICON_PIXELS: u64 = 32 * 32;
/// `CARD_BANNER_WIDTH * CARD_BANNER_HEIGHT`.
const BANNER_PIXELS: u64 = 96 * 32;
/// Palette size in bytes (`256 * 2`).
const PALETTE_BYTES: u64 = 256 * 2;
/// Sentinel meaning "field not used".
const NO_ADDR: u32 = 0xFFFF_FFFF;

/// A decoded 0x40-byte memory-card directory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardDir {
    raw: [u8; HEADER_SIZE],
}

impl CardDir {
    /// Copy and decode a directory header from the first 0x40 bytes of `bytes`.
    ///
    /// Upstream calls this a successful `fread` of `sizeof(CARDDir)`; a short
    /// buffer is therefore malformed input, not a partial header.
    pub fn parse(bytes: &[u8]) -> Result<Self, GciError> {
        let header = bytes.get(..HEADER_SIZE).ok_or(GciError::TruncatedHeader {
            available: bytes.len(),
        })?;
        let mut raw = [0u8; HEADER_SIZE];
        raw.copy_from_slice(header);
        Ok(Self { raw })
    }

    /// Build a header from a full fixed-size array, e.g. when the header was
    /// read into a stack buffer.
    pub const fn from_bytes(raw: [u8; HEADER_SIZE]) -> Self {
        Self { raw }
    }

    /// The undecoded 0x40 header bytes, exactly as stored.
    pub const fn as_bytes(&self) -> &[u8; HEADER_SIZE] {
        &self.raw
    }

    /// The four-byte game code field.  Upstream only ever inspects `"GAF"`.
    pub fn game_name(&self) -> &[u8] {
        &self.raw[GAME_NAME_OFF..GAME_NAME_OFF + 4]
    }

    /// The two-byte publisher code field.
    pub fn company(&self) -> &[u8] {
        &self.raw[COMPANY_OFF..COMPANY_OFF + 2]
    }

    /// Raw banner format byte; the low two bits are the banner kind.
    pub fn banner_format(&self) -> u8 {
        self.raw[BANNER_FORMAT_OFF]
    }

    /// The whole 32-byte `fileName` field, including any NUL padding.
    pub fn file_name_field(&self) -> &[u8] {
        &self.raw[FILE_NAME_OFF..FILE_NAME_OFF + FILE_NAME_MAX]
    }

    /// The filename up to (but excluding) the first NUL.
    ///
    /// This is a header field only: upstream compares it byte-for-byte and
    /// never joins it into a filesystem path, so no path is ever derived from
    /// these bytes by this crate.
    pub fn file_name(&self) -> &[u8] {
        match self.file_name_field().iter().position(|&b| b == 0) {
            Some(end) => &self.raw[FILE_NAME_OFF..FILE_NAME_OFF + end],
            None => self.file_name_field(),
        }
    }

    /// Seconds since 2000-01-01 00:00:00, big-endian.
    pub fn time(&self) -> u32 {
        read_be_u32(&self.raw, TIME_OFF)
    }

    /// Big-endian icon address, or `0xFFFF_FFFF` when no icon is stored.
    pub fn icon_addr(&self) -> u32 {
        read_be_u32(&self.raw, ICON_ADDR_OFF)
    }

    /// Big-endian icon format word (two bits per icon slot).
    pub fn icon_format(&self) -> u16 {
        read_be_u16(&self.raw, ICON_FORMAT_OFF)
    }

    /// Big-endian icon animation speed.
    pub fn icon_speed(&self) -> u16 {
        read_be_u16(&self.raw, ICON_SPEED_OFF)
    }

    /// Raw permission/attribute byte.
    pub fn permission(&self) -> u8 {
        self.raw[PERMISSION_OFF]
    }

    /// Number of times the file has been copied.
    pub fn copy_times(&self) -> u8 {
        self.raw[COPY_TIMES_OFF]
    }

    /// Big-endian first block of the stored file.
    pub fn start_block(&self) -> u16 {
        read_be_u16(&self.raw, START_BLOCK_OFF)
    }

    /// Block count of the stored file, in 0x2000-byte blocks.
    pub fn block_count(&self) -> u16 {
        read_be_u16(&self.raw, LENGTH_OFF)
    }

    /// Big-endian comment address, or `0xFFFF_FFFF` when no comment is stored.
    pub fn comment_addr(&self) -> u32 {
        read_be_u32(&self.raw, COMMENT_ADDR_OFF)
    }

    /// Port of `pc_gci_dir_has_name`.
    ///
    /// The comparison is exact and NUL-terminated: the game name must start
    /// with `"GAF"`, `name` must be a non-empty C string shorter than 32
    /// bytes with no interior NUL, and the field must hold `name` followed by
    /// a NUL.  Trailing bytes after that NUL are ignored, exactly as
    /// `memcmp(dir->fileName, file_name, length + 1)` ignores them.
    pub fn matches_file_name(&self, name: &[u8]) -> bool {
        if name.is_empty() || name.len() >= FILE_NAME_MAX || name.contains(&0) {
            return false;
        }
        if !self.game_name().starts_with(b"GAF") {
            return false;
        }
        let field = &self.raw[FILE_NAME_OFF..FILE_NAME_OFF + FILE_NAME_MAX];
        &field[..name.len()] == name && field[name.len()] == 0
    }

    /// Port of `pc_gci_data_offset`.
    ///
    /// `None` mirrors the C `FALSE` return: either nothing is stored (zero
    /// offset with no comment block) or the computed offset does not fit in
    /// 32 bits.
    pub fn data_offset(&self) -> Option<u32> {
        let icon_addr = u64::from(self.icon_addr());
        let mut banner_format = self.banner_format() & FORMAT_MASK;
        let mut icon_format = self.icon_format();
        let mut position = icon_addr;
        let mut icon_tlut = false;

        if self.icon_addr() == NO_ADDR {
            position = 0;
            banner_format = 0;
            icon_format = 0;
        }

        position += match banner_format {
            BANNER_C8 => BANNER_PIXELS + PALETTE_BYTES,
            BANNER_RGB5A3 => 2 * BANNER_PIXELS,
            _ => 0,
        };

        for icon in 0..ICON_MAX {
            match (icon_format >> (icon * 2)) & u16::from(FORMAT_MASK) {
                ICON_C8 => {
                    position += ICON_PIXELS;
                    icon_tlut = true;
                }
                ICON_RGB5A3 => position += 2 * ICON_PIXELS,
                _ => {}
            }
        }

        if icon_tlut {
            position += PALETTE_BYTES;
        }

        if (position == 0 && self.comment_addr() == NO_ADDR) || position > u64::from(u32::MAX) {
            return None;
        }
        Some(position as u32)
    }
}

fn read_be_u16(raw: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([raw[offset], raw[offset + 1]])
}

fn read_be_u32(raw: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        raw[offset],
        raw[offset + 1],
        raw[offset + 2],
        raw[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_with(file_name: &[u8], game_name: &[u8; 4]) -> [u8; HEADER_SIZE] {
        let mut raw = [0u8; HEADER_SIZE];
        raw[0..4].copy_from_slice(game_name);
        raw[FILE_NAME_OFF..FILE_NAME_OFF + file_name.len()].copy_from_slice(file_name);
        raw[ICON_ADDR_OFF..ICON_ADDR_OFF + 4].copy_from_slice(&NO_ADDR.to_be_bytes());
        raw
    }

    #[test]
    fn header_size_matches_card_dir_layout() {
        assert_eq!(HEADER_SIZE, 0x40);
        assert_eq!(FILE_NAME_OFF + FILE_NAME_MAX, 0x28);
        assert_eq!(COMMENT_ADDR_OFF + 4, HEADER_SIZE);
    }

    #[test]
    fn decodes_big_endian_fields() {
        let mut raw = header_with(b"town", b"GAFE");
        raw[TIME_OFF..TIME_OFF + 4].copy_from_slice(&0x1234_5678u32.to_be_bytes());
        raw[ICON_FORMAT_OFF..ICON_FORMAT_OFF + 2].copy_from_slice(&0x0202u16.to_be_bytes());
        raw[START_BLOCK_OFF..START_BLOCK_OFF + 2].copy_from_slice(&5u16.to_be_bytes());
        raw[LENGTH_OFF..LENGTH_OFF + 2].copy_from_slice(&57u16.to_be_bytes());
        let dir = CardDir::from_bytes(raw);
        assert_eq!(dir.time(), 0x1234_5678);
        assert_eq!(dir.icon_format(), 0x0202);
        assert_eq!(dir.start_block(), 5);
        assert_eq!(dir.block_count(), 57);
        assert_eq!(dir.file_name(), b"town");
        assert_eq!(dir.company(), b"\0\0");
    }

    #[test]
    fn rejects_short_header() {
        let err = CardDir::parse(&[0u8; HEADER_SIZE - 1]).unwrap_err();
        assert!(matches!(err, GciError::TruncatedHeader { available: 0x3F }));
    }

    #[test]
    fn name_match_requires_game_prefix_and_terminator() {
        let dir = CardDir::from_bytes(header_with(b"DobutsunomoriP_MURA", b"GAFE"));
        assert!(dir.matches_file_name(b"DobutsunomoriP_MURA"));
        // Prefix of the stored name is not a match: the terminator differs.
        assert!(!dir.matches_file_name(b"DobutsunomoriP"));
        // Full-width field without a NUL cannot satisfy the length + 1 compare.
        let full = CardDir::from_bytes(header_with(&[b'A'; FILE_NAME_MAX], b"GAFE"));
        assert!(!full.matches_file_name(&[b'A'; FILE_NAME_MAX - 1]));
        // Wrong game name.
        let other = CardDir::from_bytes(header_with(b"DobutsunomoriP_MURA", b"GAME"));
        assert!(!other.matches_file_name(b"DobutsunomoriP_MURA"));
        // Interior NUL is impossible for a C string.
        assert!(!dir.matches_file_name(b"Dobut\0somoriP_MURA"));
    }

    #[test]
    fn data_offset_zero_when_icon_absent() {
        let mut raw = header_with(b"x", b"GAFE");
        raw[COMMENT_ADDR_OFF..COMMENT_ADDR_OFF + 4].copy_from_slice(&0u32.to_be_bytes());
        let dir = CardDir::from_bytes(raw);
        assert_eq!(dir.data_offset(), Some(0));

        raw[COMMENT_ADDR_OFF..COMMENT_ADDR_OFF + 4].copy_from_slice(&NO_ADDR.to_be_bytes());
        assert_eq!(CardDir::from_bytes(raw).data_offset(), None);
    }

    #[test]
    fn data_offset_accounts_for_banner_and_icons() {
        let mut raw = header_with(b"x", b"GAFE");
        raw[ICON_ADDR_OFF..ICON_ADDR_OFF + 4].copy_from_slice(&0x40u32.to_be_bytes());
        raw[BANNER_FORMAT_OFF] = BANNER_RGB5A3;
        // One RGB5A3 icon in slot 0.
        raw[ICON_FORMAT_OFF..ICON_FORMAT_OFF + 2].copy_from_slice(&2u16.to_be_bytes());
        raw[COMMENT_ADDR_OFF..COMMENT_ADDR_OFF + 4].copy_from_slice(&0x40u32.to_be_bytes());
        assert_eq!(
            CardDir::from_bytes(raw).data_offset(),
            Some(0x40 + 2 * 96 * 32 + 2 * 32 * 32)
        );

        // C8 icon additionally contributes the shared TLUT palette.
        raw[ICON_FORMAT_OFF..ICON_FORMAT_OFF + 2].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(
            CardDir::from_bytes(raw).data_offset(),
            Some(0x40 + 2 * 96 * 32 + 32 * 32 + 256 * 2)
        );
    }

    #[test]
    fn data_offset_rejects_overflowing_position() {
        let mut raw = header_with(b"x", b"GAFE");
        raw[ICON_ADDR_OFF..ICON_ADDR_OFF + 4].copy_from_slice(&0xFFFF_FFFEu32.to_be_bytes());
        raw[BANNER_FORMAT_OFF] = BANNER_RGB5A3;
        assert_eq!(CardDir::from_bytes(raw).data_offset(), None);
    }
}
