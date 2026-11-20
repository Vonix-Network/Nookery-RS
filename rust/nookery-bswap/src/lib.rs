//! Bounded endian and `u8` bitfield primitives for the Nookery port.
//!
//! # Provenance
//!
//! A transcription of the reusable primitive layer in Nookery's PC port:
//! `pc/include/pc_bswap.h` (byte swaps, big-endian load/store, array swaps) and
//! the leaf/directional helpers at the head of `pc/src/pc_save_bswap.c`
//! (`swap16/32/64`, `swap16_array`, `swap32_array`, `reverse_bits8`, and the
//! `repack_u8_*` family). Licensing follows the repository's dual terms: the
//! PC port layer is MIT (© 2026 FlyingMeta) and the decompilation is CC0 1.0;
//! see the root `LICENSE`. Nothing here is derived from game assets.
//!
//! # Scope
//!
//! * [`endian`] — `u16`/`u32`/`u64` byte swaps, `pc_load_be*`/`pc_store_be*`,
//!   and the bounded `u16`/`u32` array swaps.
//! * [`bitfield`] — `reverse_bits8`, the eight named directional repackers
//!   (`1_7`, `7_1`, `5_3`, `2222`, `1_1_6`, `11111_3`, `111_5`, `4_1_3`), and a
//!   width-driven [`bitfield::repack`] engine for layouts supplied as data.
//!
//! # Explicit non-goals
//!
//! * No typed `Save_t` / ARAM struct traversal. The `swap_*` walker functions in
//!   `pc/src/pc_save_bswap.c` depend on the full decompiled struct layout and
//!   are a separate, layout-driven phase.
//! * No GCI container, checksum, or card I/O. Those live in `nookery-save`.
//! * No FFI surface. Nothing is wired into the CMake build.
//!
//! # Invariants
//!
//! * Byte swaps are involutions; the directional repackers are not, so their
//!   [`bitfield::Direction`] argument is mandatory.
//! * No caller-controlled length, count, or width is trusted: every entry point
//!   returns [`BswapError`] instead of panicking or indexing out of bounds.
//! * Zero-length and zero-count operations are no-ops where a no-op is
//!   meaningful, and are tested as such.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bitfield;
pub mod endian;
pub mod error;

pub use bitfield::{
    Direction, check_field_layout, repack, repack_1_1_6, repack_1_7, repack_4_1_3, repack_5_3,
    repack_7_1, repack_111_5, repack_2222, repack_11111_3, reverse_bits8,
};
pub use endian::{
    load_be16, load_be32, store_be16, store_be32, swap_u16, swap_u16_array, swap_u32,
    swap_u32_array, swap_u64,
};
pub use error::{BswapError, BswapResult};
