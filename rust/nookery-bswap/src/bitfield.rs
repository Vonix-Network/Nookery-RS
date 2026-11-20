//! `u8` bitfield repackers: the direction-dependent part of the save swap.
//!
//! Ported from the `repack_u8_*` helpers and `reverse_bits8` at the head of
//! `pc/src/pc_save_bswap.c` (lines 45–178). The problem they solve: GCC packs
//! `u8` bitfields MSB-first on big-endian PowerPC and LSB-first on the
//! little-endian PC host, but a single byte has no byte order to swap, so the
//! bits must be permuted explicitly.
//!
//! The oracle's rule, restated (verified against every function in the C
//! oracle): in declaration order the fields occupy the byte from the most
//! significant bit down on BE, and from the least significant bit up on LE.
//! So `[a:5, b:3]` is `a=[7:3], b=[2:0]` on disk and `b=[7:5], a=[4:0]` in
//! memory — the field *order* is reversed, not just the bits.
//!
//! These repackers are **not** self-inverse. They take a [`Direction`]
//! mirroring the C `pc_bswap_dir_t` parameter; using the wrong one silently
//! corrupts a byte. `FROM_BE` followed by `TO_BE` is the identity, and
//! [`repack`] rejects a field-width layout that does not fill exactly 8 bits.
//!
//! All helpers operate on the first byte of the slice, matching the C
//! `repack_u8_*(u8* p, …)` pointer contract; trailing bytes are untouched.

use crate::error::{BswapError, BswapResult};

/// Direction of a bitfield repack, mirroring the C `pc_bswap_dir_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// `PC_BSWAP_TO_BE`: little-endian in-memory layout to big-endian disk.
    ToBe,
    /// `PC_BSWAP_FROM_BE`: big-endian disk to little-endian in-memory layout.
    FromBe,
}

impl Direction {
    /// The opposite direction. `FROM_BE` then `TO_BE` is the identity.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Direction::ToBe => Direction::FromBe,
            Direction::FromBe => Direction::ToBe,
        }
    }
}

/// `reverse_bits8`: reverse all eight bits of a byte.
///
/// Only correct for structs whose fields are all one bit wide (the C comment
/// says exactly that); for wider fields use the matching [`repack`] layout.
/// Self-inverse, so it takes no [`Direction`]. Upstream applies it to
/// `mHm_hs_c::flags` (8 × 1-bit fields).
#[must_use]
pub const fn reverse_bits8(value: u8) -> u8 {
    value.reverse_bits()
}

/// Validate that `widths` are one-bit-or-wider fields filling exactly 8 bits.
///
/// # Errors
///
/// [`BswapError::BadFieldWidth`] when a width is zero or exceeds 8, and
/// [`BswapError::FieldWidthSum`] when the widths do not sum to 8.
pub const fn check_field_layout(widths: &[u8]) -> BswapResult<()> {
    let mut total: u32 = 0;
    let mut i = 0;
    while i < widths.len() {
        let width = widths[i];
        if width == 0 || width > 8 {
            return Err(BswapError::BadFieldWidth {
                field_index: i as u8,
                width,
            });
        }
        total += width as u32;
        i += 1;
    }
    if total != 8 {
        return Err(BswapError::FieldWidthSum { total: total as u8 });
    }
    Ok(())
}

/// Repack the first byte of `buf` under the declaration-order widths `widths`.
///
/// `widths` is the layout as the C struct declares it, most significant field
/// first; for `[a:5, b:3]` pass `&[5, 3]`. This is the generic engine behind
/// the named per-struct helpers, so callers that know only the widths (for
/// example when driving the swap from a generated layout table) do not need a
/// bespoke function.
///
/// # Errors
///
/// * [`BswapError::TooShort`] when `buf` is empty.
/// * [`BswapError::BadFieldWidth`] / [`BswapError::FieldWidthSum`] when
///   `widths` does not describe exactly one byte.
pub fn repack(buf: &mut [u8], widths: &[u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, widths, dir)
}

/// Check the buffer and layout, then permute the first byte in place.
fn repack_checked(buf: &mut [u8], widths: &[u8], dir: Direction) -> BswapResult<()> {
    let Some(slot) = buf.first_mut() else {
        return Err(BswapError::TooShort {
            needed: 1,
            available: 0,
        });
    };
    check_field_layout(widths)?;
    *slot = pack(*slot, widths, dir);
    Ok(())
}

/// Pure permutation of one byte between the two bitfield layouts.
///
/// Field `i` occupies bits `be_offset(i) .. be_offset(i) + width(i)` counting
/// down from bit 7 on disk, and `le_offset(i) .. le_offset(i) + width(i)`
/// counting up from bit 0 in memory, where `le_offset(i)` is the sum of the
/// widths declared before field `i`. `FROM_BE` copies each field from its disk
/// position to its memory position; `TO_BE` is the mirror. Fields are *moved*,
/// not reordered, so the two directions are exact inverses.
fn pack(value: u8, widths: &[u8], dir: Direction) -> u8 {
    let mut out: u8 = 0;
    let mut be_shift: u32 = 8;
    let mut le_shift: u32 = 0;
    for width in widths {
        let width = u32::from(*width);
        be_shift -= width;
        let mask = u8::MAX >> (8 - width);
        let field_from_be = (value >> be_shift) & mask;
        let field_from_le = (value >> le_shift) & mask;
        match dir {
            Direction::FromBe => out |= field_from_be << le_shift,
            Direction::ToBe => out |= field_from_le << be_shift,
        }
        le_shift += width;
    }
    out
}

/// `repack_u8_1_7`: `[a:1, b:7]` — `a=[7], b=[6:0]` on disk, `b=[7:1], a=[0]`
/// in memory.
///
/// A pure 8-bit rotation: `FROM_BE` is a rotate-left, `TO_BE` a rotate-right.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_1_7(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[1, 7], dir)
}

/// `repack_u8_7_1`: `[a:7, b:1]` — `a=[7:1], b=[0]` on disk, `b=[7], a=[6:0]`
/// in memory. Applied to `mQst_errand_c::info.first_job` (`used_num:7,
/// wrong_cloth:1`), so in a given direction this is the inverse of
/// [`repack_1_7`]: `1_7` is one 8-bit rotation, this is the opposite one.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_7_1(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[7, 1], dir)
}

/// `repack_u8_5_3`: `[a:5, b:3]` — `a=[7:3], b=[2:0]` on disk, `b=[7:5],
/// a=[4:0]` in memory. Used for the quest `pockets_idx:5, errand_type:3` byte
/// and the `birthday` byte pair.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_5_3(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[5, 3], dir)
}

/// `repack_u8_2222`: `[a:2, b:2, c:2, d:2]` — on disk `a=[7:6], b=[5:4],
/// c=[3:2], d=[1:0]`; in memory `d=[7:6], c=[5:4], b=[3:2], a=[1:0]`. Used for
/// `mMsm_shop_c::priority_lists`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_2222(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[2, 2, 2, 2], dir)
}

/// `repack_u8_1_1_6`: `[a:1, b:1, c:6]` — on disk `a=[7], b=[6], c=[5:0]`; in
/// memory `c=[7:2], b=[1], a=[0]`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_1_1_6(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[1, 1, 6], dir)
}

/// `repack_u8_11111_3`: `[a:1, b:1, c:1, d:1, e:1, f:3]` — on disk
/// `a=[7] … e=[3], f=[2:0]`; in memory `f=[7:5], e=[4], d=[3], c=[2], b=[1],
/// a=[0]`. Used for `mMsm_memory_c::letter_info`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_11111_3(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[1, 1, 1, 1, 1, 3], dir)
}

/// `repack_u8_111_5`: `[a:1, b:1, c:1, d:5]` — on disk `a=[7], b=[6], c=[5],
/// d=[4:0]`; in memory `d=[7:3], c=[2], b=[1], a=[0]`. Used for the first byte
/// of `mMsm_record_c`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_111_5(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[1, 1, 1, 5], dir)
}

/// `repack_u8_4_1_3`: `[a:4, b:1, c:3]` — on disk `a=[7:4], b=[3], c=[2:0]`;
/// in memory `c=[7:5], b=[4], a=[3:0]`. Used for the fifth byte of the
/// `birthday` record.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `buf` is empty.
pub fn repack_4_1_3(buf: &mut [u8], dir: Direction) -> BswapResult<()> {
    repack_checked(buf, &[4, 1, 3], dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One named repacker with its C declaration-order layout.
    type RepackerFn = fn(&mut [u8], Direction) -> BswapResult<()>;
    struct Repacker(&'static str, RepackerFn, &'static [u8]);

    /// Every named repacker with its layout, so the same exhaustive and
    /// property tests cover all eight.
    const REPACKERS: &[Repacker] = &[
        Repacker("1_7", repack_1_7, &[1, 7]),
        Repacker("7_1", repack_7_1, &[7, 1]),
        Repacker("5_3", repack_5_3, &[5, 3]),
        Repacker("2222", repack_2222, &[2, 2, 2, 2]),
        Repacker("1_1_6", repack_1_1_6, &[1, 1, 6]),
        Repacker("11111_3", repack_11111_3, &[1, 1, 1, 1, 1, 3]),
        Repacker("111_5", repack_111_5, &[1, 1, 1, 5]),
        Repacker("4_1_3", repack_4_1_3, &[4, 1, 3]),
    ];

    /// Independent reference: map every source bit to its destination bit one
    /// bit at a time. Positions are derived by cumulative sum from the declared
    /// layout, and placement is per-bit rather than the production code's
    /// per-field shift-and-mask, so this cross-checks the engine rather than
    /// restating it.
    fn reference(value: u8, widths: &[u8], dir: Direction) -> u8 {
        let mut out = 0u8;
        let mut before: u32 = 0;
        for width in widths {
            let width = u32::from(*width);
            let be = 8 - before - width;
            let le = before;
            for bit in 0..width {
                let (src, dst) = match dir {
                    Direction::FromBe => (be + bit, le + bit),
                    Direction::ToBe => (le + bit, be + bit),
                };
                if (value >> src) & 1 == 1 {
                    out |= 1 << dst;
                }
            }
            before += width;
        }
        out
    }

    #[test]
    fn production_matches_reference_for_every_byte() {
        for Repacker(name, repack_fn, widths) in REPACKERS {
            for byte in 0..=u8::MAX {
                for dir in [Direction::FromBe, Direction::ToBe] {
                    let mut buf = [byte];
                    repack_fn(&mut buf, dir).unwrap();
                    assert_eq!(
                        buf[0],
                        reference(byte, widths, dir),
                        "{name} {dir:?} {byte:#04X}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_repacker_round_trips_every_byte() {
        for Repacker(name, repack_fn, _) in REPACKERS {
            for byte in 0..=u8::MAX {
                let mut buf = [byte];
                repack_fn(&mut buf, Direction::FromBe).unwrap();
                repack_fn(&mut buf, Direction::ToBe).unwrap();
                assert_eq!(buf[0], byte, "{name}: FROM_BE then TO_BE on {byte:#04X}");

                let mut buf = [byte];
                repack_fn(&mut buf, Direction::ToBe).unwrap();
                repack_fn(&mut buf, Direction::FromBe).unwrap();
                assert_eq!(buf[0], byte, "{name}: TO_BE then FROM_BE on {byte:#04X}");
            }
        }
    }

    #[test]
    fn repackers_are_permutations() {
        // A byte repacker must map 256 inputs onto 256 distinct outputs; a
        // collapsing implementation would still round-trip on its image.
        for Repacker(name, repack_fn, _) in REPACKERS {
            let mut seen = [false; 256];
            for byte in 0..=u8::MAX {
                let mut buf = [byte];
                repack_fn(&mut buf, Direction::FromBe).unwrap();
                assert!(!seen[buf[0] as usize], "{name}: {byte:#04X} collides");
                seen[buf[0] as usize] = true;
            }
            assert!(seen.iter().all(|hit| *hit), "{name}: not surjective");
        }
    }

    #[test]
    fn hand_computed_vectors() {
        // FROM_BE vectors computed by hand from the C comments: extract each
        // field at its BE bit position, place it at its LE position.
        struct Case(&'static str, RepackerFn, u8, u8);
        let cases = &[
            // 1_7: 0xAA = a=1 b=0b0101010 -> (b<<1)|a = 0b01010101.
            Case("1_7", repack_1_7, 0xAA, 0x55),
            // 7_1: 0x81 = a=0b1000000 b=1 -> (b<<7)|a = 0b11000000.
            Case("7_1", repack_7_1, 0x81, 0xC0),
            // 5_3: 0xB3 = a=0b10110 b=0b011 -> (b<<5)|a = 0b01110110.
            Case("5_3", repack_5_3, 0xB3, 0x76),
            // 2222: 0x6C = a=1 b=2 c=3 d=0 -> (d<<6)|(c<<4)|(b<<2)|a = 0x39.
            Case("2222", repack_2222, 0x6C, 0x39),
            // 1_1_6: 0xAA = a=1 b=0 c=42 -> (c<<2)|(b<<1)|a = 0b10101001.
            Case("1_1_6", repack_1_1_6, 0xAA, 0xA9),
            // 11111_3: 0xB5 = a=1 b=0 c=1 d=1 e=0 f=5
            //   -> (f<<5)|(e<<4)|(d<<3)|(c<<2)|(b<<1)|a = 0b10101101.
            Case("11111_3", repack_11111_3, 0xB5, 0xAD),
            // 111_5: 0xAA = a=1 b=0 c=1 d=10 -> (d<<3)|(c<<2)|(b<<1)|a = 0x55.
            Case("111_5", repack_111_5, 0xAA, 0x55),
            // 4_1_3: 0xB5 = a=11 b=0 c=5 -> (c<<5)|(b<<4)|a = 0b10101011.
            Case("4_1_3", repack_4_1_3, 0xB5, 0xAB),
        ];
        for Case(name, repack_fn, input, expected) in cases {
            let mut buf = [*input];
            repack_fn(&mut buf, Direction::FromBe).unwrap();
            assert_eq!(buf[0], *expected, "{name} FROM_BE {input:#04X}");
            let mut buf = [*expected];
            repack_fn(&mut buf, Direction::ToBe).unwrap();
            assert_eq!(buf[0], *input, "{name} TO_BE {expected:#04X}");
        }
    }

    #[test]
    fn one_seven_and_seven_one_are_mutual_inverses_in_the_same_direction() {
        // 1_7 is a rotate and 7_1 is the opposite rotate, so applying them
        // back-to-back restores the byte. The pair is *not* inverse across
        // directions: FROM_BE then TO_BE of a single repacker is the identity,
        // while 1_7 then 7_1 is the identity only when both use one direction.
        for dir in [Direction::FromBe, Direction::ToBe] {
            for byte in 0..=u8::MAX {
                let mut a = [byte];
                repack_1_7(&mut a, dir).unwrap();
                repack_7_1(&mut a, dir).unwrap();
                assert_eq!(a[0], byte, "1_7 then 7_1 {dir:?} on {byte:#04X}");

                let mut b = [byte];
                repack_7_1(&mut b, dir).unwrap();
                repack_1_7(&mut b, dir).unwrap();
                assert_eq!(b[0], byte, "7_1 then 1_7 {dir:?} on {byte:#04X}");
            }
        }
    }

    #[test]
    fn one_bit_field_repackers_equal_bit_reversal_pattern() {
        // With every field one bit wide the permutation is a pure reversal, so
        // the oracle's reverse_bits8 agrees with the width-driven engine.
        for byte in 0..=u8::MAX {
            let mut buf = [byte];
            repack(&mut buf, &[1; 8], Direction::FromBe).unwrap();
            assert_eq!(buf[0], reverse_bits8(byte));
        }
    }

    #[test]
    fn reverse_bits8_vectors_and_involution() {
        assert_eq!(reverse_bits8(0x00), 0x00);
        assert_eq!(reverse_bits8(0x01), 0x80);
        assert_eq!(reverse_bits8(0xAA), 0x55);
        assert_eq!(reverse_bits8(0xB5), 0xAD);
        assert_eq!(reverse_bits8(0xFF), 0xFF);
        for byte in 0..=u8::MAX {
            assert_eq!(reverse_bits8(reverse_bits8(byte)), byte);
        }
    }

    #[test]
    fn generic_repack_matches_named_helpers() {
        for Repacker(name, repack_fn, widths) in REPACKERS {
            for byte in 0..=u8::MAX {
                let mut generic = [byte];
                repack(&mut generic, widths, Direction::FromBe).unwrap();
                let mut named = [byte];
                repack_fn(&mut named, Direction::FromBe).unwrap();
                assert_eq!(generic[0], named[0], "{name} FROM_BE {byte:#04X}");
            }
        }
    }

    #[test]
    fn empty_buffer_is_rejected_and_extra_bytes_untouched() {
        assert_eq!(
            repack_1_7(&mut [], Direction::FromBe),
            Err(BswapError::TooShort {
                needed: 1,
                available: 0
            })
        );
        assert_eq!(
            repack(&mut [], &[4, 4], Direction::ToBe),
            Err(BswapError::TooShort {
                needed: 1,
                available: 0
            })
        );
        // The C helpers only write *p; a longer slice must keep its tail.
        let mut buf = [0xB3, 0x11, 0x22];
        repack_5_3(&mut buf, Direction::FromBe).unwrap();
        assert_eq!(buf, [0x76, 0x11, 0x22]);
    }

    #[test]
    fn field_layout_is_validated() {
        assert!(check_field_layout(&[8]).is_ok());
        assert!(check_field_layout(&[1, 7]).is_ok());
        assert!(check_field_layout(&[1, 1, 1, 1, 1, 3]).is_ok());
        // 7 bits total: a bit would be dropped.
        assert_eq!(
            check_field_layout(&[3, 4]),
            Err(BswapError::FieldWidthSum { total: 7 })
        );
        // 9 bits total: a bit would be invented.
        assert_eq!(
            check_field_layout(&[5, 4]),
            Err(BswapError::FieldWidthSum { total: 9 })
        );
        // Zero-width field at index 1.
        assert_eq!(
            check_field_layout(&[8, 0]),
            Err(BswapError::BadFieldWidth {
                field_index: 1,
                width: 0
            })
        );
        // Oversized field at index 0.
        assert_eq!(
            check_field_layout(&[9]),
            Err(BswapError::BadFieldWidth {
                field_index: 0,
                width: 9
            })
        );
        // An empty layout cannot describe a byte.
        assert_eq!(
            check_field_layout(&[]),
            Err(BswapError::FieldWidthSum { total: 0 })
        );
        // An invalid layout is refused before the byte is touched.
        let mut buf = [0xB3];
        assert!(repack(&mut buf, &[5, 4], Direction::FromBe).is_err());
        assert_eq!(buf, [0xB3]);
    }

    #[test]
    fn direction_flip_is_an_involution() {
        assert_eq!(Direction::FromBe.flipped(), Direction::ToBe);
        assert_eq!(Direction::ToBe.flipped(), Direction::FromBe);
        assert_eq!(Direction::ToBe.flipped().flipped(), Direction::ToBe);
    }
}
