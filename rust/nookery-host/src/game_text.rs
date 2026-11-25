//! Animal Crossing game-font codec from `pc/src/acgc_game_text.cpp`.

const TILDE: i32 = 42;
const FORWARD_SLASH: i32 = 174;
const PLUS: i32 = 180;
const SEMICOLON: i32 = 208;
const HASHTAG: i32 = 209;
const BACKSLASH: i32 = 222;

const UPPERCASE: [i32; 32] = [
    3, 4, 5, 6, 2, 7, 162, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, -1, 24,
    25, 26, 27, 28, 150, 30, 29,
];

const LOWERCASE: [i32; 32] = [
    31, 35, 36, 91, 93, 94, 163, 96, 123, 124, 125, 126, 129, 130, 131, 132, 134, 135, 136, 137,
    138, 139, 140, -1, 141, 142, 143, 145, 146, 147, 149, 148,
];

/// Decode one game-font character from the beginning of `text`.
///
/// The returned tuple is `(game_code, consumed_bytes)`. An invalid or empty
/// input returns `-1`; the consumed count intentionally follows the C++
/// function: empty input consumes zero, a non-empty invalid lead consumes one,
/// and a valid two-byte UTF-8 lead/continuation consumes two even when its code
/// point is not represented by the game font.
#[must_use]
pub fn decode(text: &[u8]) -> (i32, usize) {
    let Some(&lead) = text.first() else {
        return (-1, 0);
    };

    if lead < 0x80 {
        return (decode_ascii(lead), 1);
    }
    if !(0xC2..=0xDF).contains(&lead) || text.len() < 2 {
        return (-1, 1);
    }
    let continuation = text[1];
    if continuation & 0xC0 != 0x80 {
        return (-1, 1);
    }
    let codepoint = (i32::from(lead & 0x1F) << 6) | i32::from(continuation & 0x3F);
    let code = match codepoint {
        0xA1 => 0,
        0xBF => 1,
        0xC0..=0xDF => UPPERCASE[(codepoint - 0xC0) as usize],
        0xE0..=0xFF => LOWERCASE[(codepoint - 0xE0) as usize],
        _ => -1,
    };
    (code, 2)
}

fn decode_ascii(value: u8) -> i32 {
    match value {
        0x20 => b' ' as i32,
        0x21 => b'!' as i32,
        0x22 => b'"' as i32,
        0x23 => HASHTAG,
        0x25 => b'%' as i32,
        0x26 => b'&' as i32,
        0x27 => b'\'' as i32,
        0x28 => b'(' as i32,
        0x29 => b')' as i32,
        0x2B => PLUS,
        0x2C => b',' as i32,
        0x2D => b'-' as i32,
        0x2E => b'.' as i32,
        0x2F => FORWARD_SLASH,
        0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A => i32::from(value),
        0x3A => b':' as i32,
        0x3B => SEMICOLON,
        0x3C => b'<' as i32,
        0x3D => b'=' as i32,
        0x3E => b'>' as i32,
        0x3F => b'?' as i32,
        0x40 => b'@' as i32,
        0x5C => BACKSLASH,
        0x5F => b'_' as i32,
        0x7E => TILDE,
        _ => -1,
    }
}

/// The encoded representation of one game-font code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encoded {
    bytes: [u8; 2],
    length: u8,
}

impl Encoded {
    /// Return the UTF-8 bytes used by the original `std::string_view`.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length as usize]
    }

    /// Return the bytes as a small owned array and its valid length.
    #[must_use]
    pub const fn to_array(self) -> ([u8; 2], usize) {
        (self.bytes, self.length as usize)
    }

    /// Whether the game code has an encoding.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    /// Return the encoding as a UTF-8 `String`.
    #[must_use]
    pub fn as_string(self) -> String {
        String::from_utf8(self.bytes[..self.length as usize].to_vec())
            .expect("game-font encoder only emits valid UTF-8")
    }
}

/// Encode one game-font code, matching the descending overwrite construction
/// in the C++ static table.
#[must_use]
pub fn encode(code: u8) -> Encoded {
    for codepoint in (0x20u16..=0xFF).rev() {
        if (0x7F..=0xA0).contains(&codepoint) {
            continue;
        }
        let encoded = if codepoint < 0x80 {
            [codepoint as u8, 0]
        } else {
            [
                0xC0 | (codepoint >> 6) as u8,
                0x80 | (codepoint & 0x3F) as u8,
            ]
        };
        let (decoded, consumed) = decode(&encoded);
        if decoded == i32::from(code) && consumed > 0 {
            return Encoded {
                bytes: encoded,
                length: if codepoint < 0x80 { 1 } else { 2 },
            };
        }
    }
    Encoded {
        bytes: [0, 0],
        length: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_and_special_codes_match_source_table() {
        assert_eq!(decode(b"A"), (i32::from(b'A'), 1));
        assert_eq!(decode(b"#"), (HASHTAG, 1));
        assert_eq!(decode(b"\\"), (BACKSLASH, 1));
        assert_eq!(decode(b"~"), (TILDE, 1));
        assert_eq!(decode(b"\x00"), (-1, 1));
    }

    #[test]
    fn utf8_consumption_and_mapping_match_source() {
        assert_eq!(decode("¡".as_bytes()), (0, 2));
        assert_eq!(decode("À".as_bytes()), (3, 2));
        assert_eq!(decode(&[0xC2, 0x80]), (-1, 2));
        assert_eq!(decode(&[0xC2, 0x20]), (-1, 1));
        assert_eq!(decode(&[0xC2]), (-1, 1));
    }

    #[test]
    fn encode_round_trips_every_representable_code() {
        for code in 0..=u8::MAX {
            let encoded = encode(code);
            if !encoded.is_empty() {
                let (bytes, length) = encoded.to_array();
                assert_eq!(decode(&bytes[..length]).0, i32::from(code));
            }
        }
    }
}
