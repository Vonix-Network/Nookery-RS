//! XXH3-128 text parsing and streaming, matching `borealis::disc::parse_xxh3_128`
//! and `XXH3_128bits_*`.
//!
//! The canonical text form is 32 hexadecimal characters with the high 64 bits
//! first, exactly as `parse_xxh3_128` consumes it.

use core::fmt;

use xxhash_rust::xxh3::Xxh3;

/// Number of hexadecimal characters in the canonical text form.
pub const HEX_LEN: usize = 32;

/// A 128-bit XXH3 digest, split exactly like `XXH128_hash_t`.
///
/// `high` holds the first 16 canonical characters and `low` the last 16, which
/// is how `parse_xxh3_128` builds the struct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Digest128 {
    /// High 64 bits (`XXH128_hash_t::high64`).
    pub high: u64,
    /// Low 64 bits (`XXH128_hash_t::low64`).
    pub low: u64,
}

/// Failure while parsing a canonical digest string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseDigestError {
    /// The input length is not [`HEX_LEN`].
    Length {
        /// Actual byte length of the input.
        len: usize,
    },
    /// A byte is not an ASCII hexadecimal digit.
    Digit {
        /// Byte offset of the offending character.
        index: usize,
    },
}

impl fmt::Display for ParseDigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Length { len } => {
                write!(f, "expected {HEX_LEN} hexadecimal characters, got {len}")
            }
            Self::Digit { index } => write!(f, "invalid hexadecimal character at byte {index}"),
        }
    }
}

impl std::error::Error for ParseDigestError {}

const fn nibble(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        b'A'..=b'F' => Some(character - b'A' + 10),
        _ => None,
    }
}

impl Digest128 {
    /// Parses a canonical high-bits-first 32-character digest.
    ///
    /// Mirrors `borealis::disc::parse_xxh3_128`, including accepting uppercase
    /// digits.  The C++ helper throws `std::invalid_argument`; this returns
    /// [`ParseDigestError`] instead of panicking.
    pub fn parse(text: &str) -> Result<Self, ParseDigestError> {
        Self::parse_bytes(text.as_bytes())
    }

    /// Byte-slice form of [`Digest128::parse`].
    pub fn parse_bytes(value: &[u8]) -> Result<Self, ParseDigestError> {
        if value.len() != HEX_LEN {
            return Err(ParseDigestError::Length { len: value.len() });
        }
        // `parse_xxh3_128` builds `.low64` from `substr(16, 16)` and `.high64`
        // from `substr(0, 16)`; both halves shift left four bits per digit.
        let mut halves = [0u64; 2];
        for (half_index, half) in halves.iter_mut().enumerate() {
            let mut acc = 0u64;
            for offset in 0..16 {
                let index = half_index * 16 + offset;
                let Some(digit) = nibble(value[index]) else {
                    return Err(ParseDigestError::Digit { index });
                };
                acc = (acc << 4) | u64::from(digit);
            }
            *half = acc;
        }
        Ok(Self {
            high: halves[0],
            low: halves[1],
        })
    }
}

impl fmt::Display for Digest128 {
    /// Writes the 32-character half-first hexadecimal form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}{:016x}", self.high, self.low)
    }
}

/// Streaming XXH3-128 hasher with a fixed, caller-supplied buffer discipline.
///
/// Wraps `xxhash-rust`'s streaming state so the verification loop can hash an
/// image in bounded chunks, as `XXH3_128bits_update` does upstream.
#[derive(Clone, Default)]
pub struct Hasher {
    state: Xxh3,
}

impl Hasher {
    /// Creates an empty hasher.
    pub fn new() -> Self {
        Self { state: Xxh3::new() }
    }

    /// Feeds one bounded chunk.
    pub fn update(&mut self, chunk: &[u8]) {
        self.state.update(chunk);
    }

    /// Finishes the digest.  Upstream compares `XXH128_hash_t` fields, so the
    /// split matches [`Digest128`].
    pub fn finish(self) -> Digest128 {
        let digest = self.state.digest128();
        Digest128 {
            high: (digest >> 64) as u64,
            low: digest as u64,
        }
    }
}
