//! Endian primitives: byte swaps, big-endian loads/stores, and array swaps.
//!
//! Ported 1:1 from `pc/include/pc_bswap.h` and the leaf helpers at the head of
//! `pc/src/pc_save_bswap.c`:
//!
//! | Rust | C oracle |
//! |---|---|
//! | [`swap_u16`] | `swap16` / `pc_bswap16` |
//! | [`swap_u32`] | `swap32` / `pc_bswap32` |
//! | [`swap_u64`] | `swap64` / `pc_bswap64` |
//! | [`load_be16`] | `pc_load_be16` |
//! | [`load_be32`] | `pc_load_be32` |
//! | [`store_be16`] | `pc_store_be16` |
//! | [`store_be32`] | `pc_store_be32` |
//! | [`swap_u16_array`] | `swap16_array` / `pc_bswap16_array` |
//! | [`swap_u32_array`] | `swap32_array` / `pc_bswap32_array` |
//!
//! Every scalar swap is an involution: applying it twice restores the input.
//! The array helpers keep the C pointer+count shape but derive bounds from the
//! slice instead of trusting the count, so an oversized count is an error and
//! a trailing partial element is rejected rather than swapped.

use crate::error::{BswapError, BswapResult};

/// `pc_bswap16`: swap the two bytes of a 16-bit value.
///
/// Self-inverse.
#[must_use]
pub const fn swap_u16(value: u16) -> u16 {
    value.swap_bytes()
}

/// `pc_bswap32`: swap the four bytes of a 32-bit value.
///
/// Self-inverse.
#[must_use]
pub const fn swap_u32(value: u32) -> u32 {
    value.swap_bytes()
}

/// `pc_bswap64`: swap the eight bytes of a 64-bit value.
///
/// Self-inverse.
#[must_use]
pub const fn swap_u64(value: u64) -> u64 {
    value.swap_bytes()
}

/// `pc_load_be16`: read one big-endian `u16` from the first two bytes of `src`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when fewer than two bytes are available; the C
/// version would `memcpy` past the end.
pub fn load_be16(src: &[u8]) -> BswapResult<u16> {
    let bytes = src.first_chunk::<2>().ok_or(BswapError::TooShort {
        needed: 2,
        available: src.len(),
    })?;
    Ok(u16::from_be_bytes(*bytes))
}

/// `pc_load_be32`: read one big-endian `u32` from the first four bytes of `src`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when fewer than four bytes are available.
pub fn load_be32(src: &[u8]) -> BswapResult<u32> {
    let bytes = src.first_chunk::<4>().ok_or(BswapError::TooShort {
        needed: 4,
        available: src.len(),
    })?;
    Ok(u32::from_be_bytes(*bytes))
}

/// `pc_store_be16`: write `value` big-endian into the first two bytes of `dst`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `dst` has fewer than two bytes.
pub fn store_be16(dst: &mut [u8], value: u16) -> BswapResult<()> {
    let available = dst.len();
    let slot = dst.first_chunk_mut::<2>().ok_or(BswapError::TooShort {
        needed: 2,
        available,
    })?;
    *slot = value.to_be_bytes();
    Ok(())
}

/// `pc_store_be32`: write `value` big-endian into the first four bytes of `dst`.
///
/// # Errors
///
/// [`BswapError::TooShort`] when `dst` has fewer than four bytes.
pub fn store_be32(dst: &mut [u8], value: u32) -> BswapResult<()> {
    let available = dst.len();
    let slot = dst.first_chunk_mut::<4>().ok_or(BswapError::TooShort {
        needed: 4,
        available,
    })?;
    *slot = value.to_be_bytes();
    Ok(())
}

/// `pc_bswap16_array`: swap `count` consecutive `u16` elements in place.
///
/// `count` is an element count, exactly as in C, but it is validated against
/// the slice: `count == 0` is a no-op, and any count past the buffer is
/// refused instead of walking off the allocation.
///
/// # Errors
///
/// * [`BswapError::NotMultipleOf`] when `buf.len()` is odd — the C helper
///   indexes through `unsigned short*`, so a partial trailing byte means the
///   caller passed a length it never intended.
/// * [`BswapError::CountExceeds`] when `count > buf.len() / 2`.
pub fn swap_u16_array(buf: &mut [u8], count: usize) -> BswapResult<()> {
    check_element_count(buf.len(), 2, count)?;
    for elem in buf.chunks_exact_mut(2).take(count) {
        let value = u16::from_ne_bytes([elem[0], elem[1]]);
        elem.copy_from_slice(&swap_u16(value).to_ne_bytes());
    }
    Ok(())
}

/// `pc_bswap32_array`: swap `count` consecutive `u32` elements in place.
///
/// # Errors
///
/// * [`BswapError::NotMultipleOf`] when `buf.len()` is not a multiple of four.
/// * [`BswapError::CountExceeds`] when `count > buf.len() / 4`.
pub fn swap_u32_array(buf: &mut [u8], count: usize) -> BswapResult<()> {
    check_element_count(buf.len(), 4, count)?;
    for elem in buf.chunks_exact_mut(4).take(count) {
        let value = u32::from_ne_bytes([elem[0], elem[1], elem[2], elem[3]]);
        elem.copy_from_slice(&swap_u32(value).to_ne_bytes());
    }
    Ok(())
}

/// Shared validation for the array helpers: whole elements only, and a count
/// that fits.
fn check_element_count(len: usize, width: usize, count: usize) -> BswapResult<()> {
    if len % width != 0 {
        return Err(BswapError::NotMultipleOf { len, width });
    }
    let available = len / width;
    if count > available {
        return Err(BswapError::CountExceeds { count, available });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_swaps_match_hand_vectors() {
        assert_eq!(swap_u16(0x1234), 0x3412);
        assert_eq!(swap_u32(0x1234_5678), 0x7856_3412);
        assert_eq!(swap_u64(0x0102_0304_0506_0708), 0x0807_0605_0403_0201);
        assert_eq!(swap_u16(0x00FF), 0xFF00);
        assert_eq!(swap_u32(0), 0);
    }

    #[test]
    fn scalar_swaps_are_self_inverse() {
        for value in 0..=u16::MAX {
            assert_eq!(swap_u16(swap_u16(value)), value);
        }
        let u32_probes = [
            0,
            1,
            0x8000_0000,
            0x1234_5678,
            0xFFFF_FFFF,
            0x00FF_00FF,
            0xDEAD_BEEF,
        ];
        for value in u32_probes {
            assert_eq!(swap_u32(swap_u32(value)), value);
        }
        let u64_probes = [0, 1, u64::MAX, 0x0123_4567_89AB_CDEF, 0xFFFF_0000_FFFF_0000];
        for value in u64_probes {
            assert_eq!(swap_u64(swap_u64(value)), value);
        }
    }

    #[test]
    fn scalar_swaps_reverse_the_byte_sequence() {
        // swap_uXX must be byte reversal of the value's own byte order, which
        // is what the C `pc_bswap*` builtins do.
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_ne_bytes());
        bytes.reverse();
        assert_eq!(u64::from_ne_bytes(bytes), swap_u64(0x0123_4567_89AB_CDEF));
    }

    #[test]
    fn big_endian_load_store_vectors() {
        assert_eq!(load_be16(&[0x12, 0x34]).unwrap(), 0x1234);
        assert_eq!(load_be32(&[0x12, 0x34, 0x56, 0x78]).unwrap(), 0x1234_5678);
        // Extra trailing bytes are ignored, exactly like the C memcpy of a
        // fixed width.
        assert_eq!(
            load_be32(&[0x12, 0x34, 0x56, 0x78, 0xAA]).unwrap(),
            0x1234_5678
        );

        let mut buf = [0u8; 4];
        store_be16(&mut buf, 0x1234).unwrap();
        assert_eq!(buf, [0x12, 0x34, 0x00, 0x00]);
        store_be32(&mut buf, 0x1234_5678).unwrap();
        assert_eq!(buf, [0x12, 0x34, 0x56, 0x78]);
        assert_eq!(load_be32(&buf).unwrap(), 0x1234_5678);
    }

    #[test]
    fn big_endian_load_store_reject_short_buffers() {
        assert_eq!(
            load_be16(&[0x12]),
            Err(BswapError::TooShort {
                needed: 2,
                available: 1
            })
        );
        assert_eq!(
            load_be32(&[]),
            Err(BswapError::TooShort {
                needed: 4,
                available: 0
            })
        );
        assert_eq!(
            store_be16(&mut [0u8], 1),
            Err(BswapError::TooShort {
                needed: 2,
                available: 1
            })
        );
        assert_eq!(
            store_be32(&mut [0u8; 3], 1),
            Err(BswapError::TooShort {
                needed: 4,
                available: 3
            })
        );
    }

    #[test]
    fn store_then_load_round_trips() {
        for value in [0x0000u16, 0x00FF, 0xFF00, 0x1234, 0xFFFF] {
            let mut buf = [0u8; 2];
            store_be16(&mut buf, value).unwrap();
            assert_eq!(load_be16(&buf).unwrap(), value);
        }
        for value in [0u32, 1, 0x8000_0000, 0xDEAD_BEEF, u32::MAX] {
            let mut buf = [0u8; 4];
            store_be32(&mut buf, value).unwrap();
            assert_eq!(load_be32(&buf).unwrap(), value);
        }
    }

    #[test]
    fn u16_array_swaps_each_element_and_is_self_inverse() {
        let mut buf = [0x12, 0x34, 0xAB, 0xCD];
        swap_u16_array(&mut buf, 2).unwrap();
        assert_eq!(buf, [0x34, 0x12, 0xCD, 0xAB]);
        swap_u16_array(&mut buf, 2).unwrap();
        assert_eq!(buf, [0x12, 0x34, 0xAB, 0xCD]);

        // Swapping a prefix must not disturb the tail (C loops `i < count`).
        let mut buf = [0x12, 0x34, 0xAB, 0xCD];
        swap_u16_array(&mut buf, 1).unwrap();
        assert_eq!(buf, [0x34, 0x12, 0xAB, 0xCD]);
    }

    #[test]
    fn u32_array_swaps_each_element_and_is_self_inverse() {
        let mut buf = [0x12, 0x34, 0x56, 0x78, 0xDE, 0xAD, 0xBE, 0xEF];
        swap_u32_array(&mut buf, 2).unwrap();
        assert_eq!(buf, [0x78, 0x56, 0x34, 0x12, 0xEF, 0xBE, 0xAD, 0xDE]);
        swap_u32_array(&mut buf, 2).unwrap();
        assert_eq!(buf, [0x12, 0x34, 0x56, 0x78, 0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn zero_count_is_a_noop() {
        let mut buf = [0x12, 0x34];
        swap_u16_array(&mut buf, 0).unwrap();
        assert_eq!(buf, [0x12, 0x34]);

        let mut empty: [u8; 0] = [];
        swap_u16_array(&mut empty, 0).unwrap();
        swap_u32_array(&mut empty, 0).unwrap();
    }

    #[test]
    fn array_helpers_reject_bad_lengths_and_counts() {
        assert_eq!(
            swap_u16_array(&mut [0u8; 3], 1),
            Err(BswapError::NotMultipleOf { len: 3, width: 2 })
        );
        // Length is validated before the count, so an odd buffer is refused
        // even for a zero count: it never describes whole `u16` elements.
        assert_eq!(
            swap_u16_array(&mut [0u8; 3], 0),
            Err(BswapError::NotMultipleOf { len: 3, width: 2 })
        );
        assert_eq!(
            swap_u16_array(&mut [0u8; 4], 3),
            Err(BswapError::CountExceeds {
                count: 3,
                available: 2
            })
        );
        assert_eq!(
            swap_u32_array(&mut [0u8; 6], 2),
            Err(BswapError::NotMultipleOf { len: 6, width: 4 })
        );
        assert_eq!(
            swap_u32_array(&mut [0u8; 8], 3),
            Err(BswapError::CountExceeds {
                count: 3,
                available: 2
            })
        );
    }
}
