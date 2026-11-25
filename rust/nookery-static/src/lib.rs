//! Host-independent ports of the small, stateful routines in `src/static`.
//!
//! The source units covered here are the MSL C character, random, memory,
//! string, and integer-conversion routines.  The APIs use bounded Rust slices
//! and return explicit errors where the C callers relied on a valid buffer;
//! valid calls retain the C byte ordering, comparison results, wraparound, and
//! parser state transitions.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod ctype;
pub mod memory;
pub mod random;
pub mod strings;
pub mod strto;

pub use ctype::{is_alpha, is_digit, is_space, is_upper, is_xdigit, to_lower, to_upper};
pub use memory::{memchr, memcmp, memmove, memmove_within, memrchr, MemoryError};
pub use random::{rand, srand};
pub use strings::{strcat, strchr, strcmp, strcpy, strlen, strncmp, strncpy, strstr, StringError};
pub use strto::{strtol, strtoul, IntegerParse};
