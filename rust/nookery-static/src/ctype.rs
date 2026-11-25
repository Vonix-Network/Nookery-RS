//! The fixed C locale tables from `MSL_C.PPCEABI.bare.H/ctype.c`.

const CONTROL: u8 = 0x01;
const MOTION: u8 = 0x02;
const SPACE: u8 = 0x04;
const PUNCT: u8 = 0x08;
const DIGIT: u8 = 0x10;
const HEX: u8 = 0x20;
const LOWER: u8 = 0x40;
const UPPER: u8 = 0x80;
const LETTER: u8 = LOWER | UPPER;
const WHITESPACE: u8 = MOTION | SPACE;

const fn classify(byte: u8) -> u8 {
    match byte {
        0..=8 | 14..=31 | 127 => CONTROL,
        9..=13 => MOTION,
        32 => SPACE,
        b'0'..=b'9' => DIGIT | HEX,
        b'A'..=b'F' => UPPER | HEX,
        b'G'..=b'Z' => UPPER,
        b'a'..=b'f' => LOWER | HEX,
        b'g'..=b'z' => LOWER,
        33..=47 | 58..=64 | 91..=96 | 123..=126 => PUNCT,
        _ => CONTROL,
    }
}

/// Returns the MSL classification flags for one byte.
pub const fn classification(c: i32) -> u8 {
    classify(c as u8)
}

/// Returns non-zero when `c` is an ASCII letter, including the C `EOF` rule.
pub const fn is_alpha(c: i32) -> bool {
    c != -1 && classification(c) & LETTER != 0
}

/// Returns non-zero when `c` is an ASCII decimal digit.
pub const fn is_digit(c: i32) -> bool {
    c != -1 && classification(c) & DIGIT != 0
}

/// Returns non-zero for the fixed MSL whitespace set.
pub const fn is_space(c: i32) -> bool {
    c != -1 && classification(c) & WHITESPACE != 0
}

/// Returns non-zero when `c` is an uppercase ASCII letter.
pub const fn is_upper(c: i32) -> bool {
    c != -1 && classification(c) & UPPER != 0
}

/// Returns non-zero when `c` is an ASCII hexadecimal digit.
pub const fn is_xdigit(c: i32) -> bool {
    c != -1 && classification(c) & HEX != 0
}

/// MSL `tolower`: `EOF` remains `EOF`, and all other inputs are byte-masked.
pub const fn to_lower(c: i32) -> i32 {
    if c == -1 {
        -1
    } else if c >= b'A' as i32 && c <= b'Z' as i32 {
        c + (b'a' as i32 - b'A' as i32)
    } else {
        (c as u8) as i32
    }
}

/// The matching fixed-locale uppercase conversion.
pub const fn to_upper(c: i32) -> i32 {
    if c == -1 {
        -1
    } else if c >= b'a' as i32 && c <= b'z' as i32 {
        c - (b'a' as i32 - b'A' as i32)
    } else {
        (c as u8) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_msl_ascii_table_boundaries() {
        assert!(is_space(b'\t' as i32));
        assert!(is_space(b' ' as i32));
        assert!(!is_space(b'\x1c' as i32));
        assert!(is_xdigit(b'F' as i32));
        assert!(is_xdigit(b'f' as i32));
        assert!(!is_xdigit(b'G' as i32));
        assert!(!is_alpha(0x80 + b'A' as i32));
        assert_eq!(to_lower(-1), -1);
        assert_eq!(to_lower(0x141), b'A' as i32);
        assert_eq!(to_upper(b'z' as i32), b'Z' as i32);
    }
}
