//! Bounded translations of `mem.c` and its `mem_funcs.c` copy helpers.

use std::ops::Range;

/// Invalid buffer geometry supplied to a memory primitive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryError {
    /// The two slices do not have the same length for a whole-slice operation.
    LengthMismatch {
        /// Length of the destination or left operand.
        left: usize,
        /// Length of the source or right operand.
        right: usize,
    },
    /// A range is outside the backing byte slice.
    OutOfBounds {
        /// Range that was requested.
        range: Range<usize>,
        /// Length of the backing slice.
        length: usize,
    },
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LengthMismatch { left, right } => {
                write!(f, "memory lengths differ: {left} and {right}")
            }
            Self::OutOfBounds { range, length } => {
                write!(f, "range {range:?} exceeds memory length {length}")
            }
        }
    }
}

impl std::error::Error for MemoryError {}

/// Returns the first byte equal to `needle`, using C's unsigned-byte result.
pub fn memchr(bytes: &[u8], needle: i32) -> Option<usize> {
    let needle = needle as u8;
    bytes.iter().position(|&byte| byte == needle)
}

/// Returns the last byte equal to `needle`.
pub fn memrchr(bytes: &[u8], needle: i32) -> Option<usize> {
    let needle = needle as u8;
    bytes.iter().rposition(|&byte| byte == needle)
}

/// Compares two equal-sized byte sequences with MSL's `-1/0/+1` result.
pub fn memcmp(left: &[u8], right: &[u8]) -> Result<i32, MemoryError> {
    if left.len() != right.len() {
        return Err(MemoryError::LengthMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    Ok(left
        .iter()
        .zip(right)
        .find_map(|(&a, &b)| (a != b).then_some(if a < b { -1 } else { 1 }))
        .unwrap_or(0))
}

/// Copies equal-sized non-overlapping slices.
pub fn memmove(destination: &mut [u8], source: &[u8]) -> Result<(), MemoryError> {
    if destination.len() != source.len() {
        return Err(MemoryError::LengthMismatch {
            left: destination.len(),
            right: source.len(),
        });
    }
    destination.copy_from_slice(source);
    Ok(())
}

/// Copies overlapping regions in the same backing slice, like `memmove`.
pub fn memmove_within(
    bytes: &mut [u8],
    destination: Range<usize>,
    source: Range<usize>,
) -> Result<(), MemoryError> {
    if destination.len() != source.len() {
        return Err(MemoryError::LengthMismatch {
            left: destination.len(),
            right: source.len(),
        });
    }
    if destination.end > bytes.len() {
        return Err(MemoryError::OutOfBounds {
            range: destination,
            length: bytes.len(),
        });
    }
    if source.end > bytes.len() {
        return Err(MemoryError::OutOfBounds {
            range: source,
            length: bytes.len(),
        });
    }
    bytes.copy_within(source, destination.start);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_compares_unsigned_bytes() {
        assert_eq!(memchr(&[0x80, 0, 0x80], -128), Some(0));
        assert_eq!(memrchr(&[0x80, 0, 0x80], 0x80), Some(2));
        assert_eq!(memcmp(b"a\xff", b"a\x00"), Ok(1));
        assert_eq!(memcmp(b"same", b"same"), Ok(0));
    }

    #[test]
    fn overlap_copy_preserves_the_source_bytes() {
        let mut bytes = *b"abcdef";
        memmove_within(&mut bytes, 2..6, 0..4).unwrap();
        assert_eq!(&bytes, b"ababcd");
        memmove_within(&mut bytes, 0..4, 2..6).unwrap();
        assert_eq!(&bytes, b"abcdcd");
    }
}
