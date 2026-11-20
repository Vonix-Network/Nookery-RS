//! Behavior-preserving Rust port of Nookery's bounded allocation primitives.
//!
//! Upstream sources at pin `a81c1aaf0d169a49d1044eb5bce9bbf699da64d9`:
//!
//! | Upstream | Here |
//! |---|---|
//! | `include/gamealloc.h`, `src/gamealloc.c` | [`GameAlloc`] |
//! | `include/TwoHeadArena.h`, `src/TwoHeadArena.c` | [`TwoHeadArena`] |
//! | `malloc` / `free` from `src/static/libc64/malloc.c` | [`BlockStore`] / [`SlabStore`] / [`BumpStore`] |
//!
//! Both primitives are modeled as *address bookkeeping over caller-supplied
//! storage*.  No Rust code dereferences a raw pointer derived from an address:
//! the ported functions only compute addresses, and the adapters ([`Arena`],
//! [`SlabStore`]) map an address back to a real buffer through checked offset
//! arithmetic.  Consequently this crate contains no `unsafe` blocks at all.
//!
//! # What is ported
//!
//! * `gamealloc_init`, `gamealloc_malloc`, `gamealloc_free`, `gamealloc_cleanup`
//! * `THA_ct`, `THA_dt`, `THA_alloc16`, `THA_allocAlign`, `THA_getFreeBytes`,
//!   `THA_getFreeBytes16`, `THA_getFreeBytesAlign`, `THA_isCrash`, `THA_init`
//!
//! The `/* @fabricated */` functions in `TwoHeadArena.c` (`THA_alloc`,
//! `THA_getHeadPtr`, `THA_setHeadPtr`, `THA_getTailPtr`, `THA_nextPtrN`,
//! `THA_nextPtr1`) are commented out upstream and are *not* ported as active
//! behavior.
//!
//! # Semantic boundaries (deliberate, documented)
//!
//! 1. **Address width.**  The source computes on `uintptr_t`, which is 32-bit on
//!    the shipped console targets and 64-bit on the `TARGET_PC` build.  This
//!    port fixes the model at 32 bits ([`storage::Addr`] is `u32`), which is the
//!    width the header's own layout assertions (`sizeof(GameAllocList) == 0x10`,
//!    `sizeof(GameAlloc) == 0x14`, `tha` at `0x78` in `include/game.h`) describe.
//!    It reproduces the source's explicit narrowing exactly: the `(u32)~mask`
//!    term in `THA_getFreeBytesAlign` and the `(int)` return conversion.  For
//!    any buffer entirely below 4 GiB the 32-bit model and a 64-bit build agree
//!    bit-for-bit; the narrow `(int)` result is signed-truncated in both.
//!    Addresses at or above 4 GiB are outside this model by construction, which
//!    [`TwoHeadArena`] and [`Arena`] enforce with [`ArenaError::AddressOverflow`].
//!
//! 2. **Pointer stability.**  The C code hands out raw pointers and requires
//!    them to stay valid and unmoved for the node's lifetime, so no port here
//!    relocates a live block.  This crate is an *owned byte-storage model*: node
//!    and payload addresses are synthetic numbers resolved by offset arithmetic
//!    into an owned `Vec<u8>`; they are never host pointers, and nothing in this
//!    crate can be handed to a C caller or dereferenced as one.  [`GameAlloc`]
//!    documents exactly which part of the C pointer contract survives (a live
//!    block's address never moves or aliases) and which does not (it is not a
//!    host pointer, and the backing buffer's lifetime is the crate's, not C's).
//!
//! 3. **Wrapping vs. checked.**  C wraps silently on address/size overflow and
//!    reports arena exhaustion only through [`TwoHeadArena::is_crash`].
//!    [`TwoHeadArena`] exposes the exact wrapping arithmetic, and the `try_*`
//!    methods reject overflow, out-of-range results and collisions with
//!    [`ArenaError`] instead of mutating state to an invalid value.
//! 4. **Invalid free.**  `gamealloc_free` assumes the pointer came from the
//!    same `GameAlloc`; a foreign pointer is undefined behavior upstream.
//!    [`GameAlloc::free`] returns [`AllocError::NotOwned`] for it.
//! 5. **Size overflow at the header add.**  `gamealloc_malloc` computes
//!    `size + sizeof(GameAllocList)` without a check; on a 32-bit target a huge
//!    `size` wraps and hands out a small block.  [`GameAlloc::malloc`] reports
//!    [`AllocError::SizeOverflow`] instead.
//! 6. **Failure to allocate.**  A failing `malloc` returns `NULL` and leaves the
//!    list untouched.  That is preserved as `Ok(None)`; it is not an error.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod gamealloc;
pub mod storage;
pub mod two_head_arena;

pub use gamealloc::{AllocError, GameAlloc, Handle, NODE_HEADER_SIZE};
pub use storage::{BlockStore, BumpStore, SlabStore};
pub use two_head_arena::{Arena, ArenaError, TwoHeadArena};
