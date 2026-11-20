//! Failure categories for the bounded swap helpers.
//!
//! The C oracle never validates its inputs: `pc_bswap16_array(void*, int)`
//! trusts the caller's `count`, `pc_load_be16` trusts `src`, and the
//! `repack_u8_*` helpers dereference a raw `u8*`. This crate replaces every
//! one of those unchecked accesses with an explicit error.

use std::fmt;

/// Reason a swap operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BswapError {
    /// A fixed-width load or store saw a shorter buffer than its width.
    ///
    /// Mirrors the out-of-bounds `memcpy` in `pc_load_be16`/`pc_store_be16`;
    /// C reads or writes past the end, this crate refuses.
    TooShort {
        /// Bytes the operation needs.
        needed: usize,
        /// Bytes actually available.
        available: usize,
    },
    /// A byte buffer is not a whole number of elements wide.
    ///
    /// `pc_bswap16_array`/`pc_bswap32_array` index `u16`/`u32` pointers, so a
    /// byte length that is not a multiple of the element width is malformed
    /// input rather than a partial tail.
    NotMultipleOf {
        /// Buffer length in bytes.
        len: usize,
        /// Element width in bytes.
        width: usize,
    },
    /// A count-style call asked for more elements than the buffer holds.
    ///
    /// The C array helpers loop `i < count` over a caller-supplied pointer;
    /// an oversized count silently walks off the allocation. Here it is an
    /// error.
    CountExceeds {
        /// Elements requested.
        count: usize,
        /// Elements available.
        available: usize,
    },
    /// A bitfield width list contains a zero or wider-than-byte field.
    ///
    /// The C repackers hardcode their layouts per struct; the generic engine
    /// takes them as data, so an unusable width is rejected instead of
    /// silently shifting bits off an end.
    BadFieldWidth {
        /// Index of the offending field.
        field_index: u8,
        /// The offending width, either zero or greater than eight.
        width: u8,
    },
    /// A bitfield width list does not describe exactly one byte.
    ///
    /// Repacking permutes the bits of a single byte, so the widths must sum to
    /// eight; anything else would drop or invent bits.
    FieldWidthSum {
        /// Sum of the supplied widths.
        total: u8,
    },
}

impl fmt::Display for BswapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BswapError::TooShort { needed, available } => {
                write!(f, "buffer too short: {available} of {needed} bytes")
            }
            BswapError::NotMultipleOf { len, width } => {
                write!(f, "buffer length {len} is not a multiple of {width}")
            }
            BswapError::CountExceeds { count, available } => {
                write!(f, "element count {count} exceeds {available} available")
            }
            BswapError::BadFieldWidth { field_index, width } => {
                write!(f, "bitfield {field_index} has unusable width {width}")
            }
            BswapError::FieldWidthSum { total } => {
                write!(f, "bitfield widths total {total} bits, expected 8")
            }
        }
    }
}

impl std::error::Error for BswapError {}

/// Convenience alias used by every fallible entry point.
pub type BswapResult<T> = Result<T, BswapError>;
