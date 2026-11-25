//! Fixed-width integer parsing from `MSL_C.PPCEABI.bare.H/strtoul.c`.

use crate::ctype::{is_alpha, is_digit, is_space, to_upper};

/// The result and side effects of one MSL string-to-integer conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegerParse {
    /// The returned 32-bit value, after unsigned negation when applicable.
    pub value: u32,
    /// The byte offset corresponding to the C `end` pointer.
    pub end: usize,
    /// Whether a leading minus sign was consumed.
    pub negative: bool,
    /// Whether the unsigned accumulator overflowed.
    pub overflow: bool,
    /// MSL `errno` after the call: `0x22` means `ERANGE`.
    pub errno: Option<i32>,
}

fn digit(byte: u8) -> Option<u32> {
    let c = byte as i32;
    if is_digit(c) {
        Some((byte - b'0') as u32)
    } else if is_alpha(c) {
        Some((to_upper(c) as u8 - b'A' + 10) as u32)
    } else {
        None
    }
}

fn parse_unsigned(input: &[u8], base: u32) -> (u32, usize, bool, bool) {
    if !(base == 0 || (2..=36).contains(&base)) {
        return (0, 0, false, false);
    }
    let end = input
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(input.len());
    let input = &input[..end];
    let mut index = 0;
    while index < input.len() && is_space(input[index] as i32) {
        index += 1;
    }
    let negative = input.get(index) == Some(&b'-');
    if matches!(input.get(index), Some(b'+') | Some(b'-')) {
        index += 1;
    }
    let start = index;
    let mut radix = base;
    if (radix == 0 || radix == 16)
        && input.get(index) == Some(&b'0')
        && matches!(input.get(index + 1), Some(b'x' | b'X'))
    {
        radix = 16;
        index += 2;
    } else if radix == 0 && input.get(index) == Some(&b'0') {
        radix = 8;
    } else if radix == 0 {
        radix = 10;
    }

    let digits_start = index;
    let mut value = 0u32;
    let mut overflow = false;
    while let Some(&byte) = input.get(index) {
        let Some(next) = digit(byte) else { break };
        if next >= radix {
            break;
        }
        if value > u32::MAX / radix || (value == u32::MAX / radix && next > u32::MAX % radix) {
            overflow = true;
        }
        value = value.wrapping_mul(radix).wrapping_add(next);
        index += 1;
    }
    if index == digits_start {
        // A lone zero is still a successful leading-zero conversion in MSL.
        if start < input.len() && input[start] == b'0' {
            return (0, start + 1, negative, false);
        }
        return (0, 0, negative, false);
    }
    (value, index, negative, overflow)
}

/// Parses an MSL 32-bit unsigned integer and returns its C `end` offset.
pub fn strtoul(input: &[u8], base: u32) -> IntegerParse {
    let (mut value, end, negative, overflow) = parse_unsigned(input, base);
    let errno = if overflow {
        value = u32::MAX;
        Some(0x22)
    } else {
        if negative {
            value = value.wrapping_neg();
        }
        None
    };
    IntegerParse {
        value,
        end,
        negative,
        overflow,
        errno,
    }
}

/// Parses an MSL 32-bit signed integer with saturation at the C limits.
pub fn strtol(input: &[u8], base: u32) -> IntegerParse {
    let parsed = strtoul(input, base);
    let magnitude = if parsed.negative {
        parsed.value.wrapping_neg()
    } else {
        parsed.value
    };
    let signed_overflow = parsed.overflow
        || (!parsed.negative && magnitude > i32::MAX as u32)
        || (parsed.negative && magnitude > 0x8000_0000);
    IntegerParse {
        value: if signed_overflow {
            if parsed.negative {
                0x8000_0000
            } else {
                i32::MAX as u32
            }
        } else if parsed.negative {
            magnitude.wrapping_neg()
        } else {
            magnitude
        },
        errno: signed_overflow.then_some(0x22),
        ..parsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_base_detection_end_offsets_and_wrap() {
        assert_eq!(
            strtoul(b"  -0x2a tail\0", 0),
            IntegerParse {
                value: 0xffff_ffd6,
                end: 7,
                negative: true,
                overflow: false,
                errno: None,
            }
        );
        assert_eq!(strtoul(b"077\0", 0).value, 63);
        assert_eq!(strtoul(b"0x100000000\0", 0).errno, Some(0x22));
        assert_eq!(strtol(b"2147483648\0", 10).value, i32::MAX as u32);
        assert_eq!(strtol(b"-2147483648\0", 10).value, 0x8000_0000);
    }
}
