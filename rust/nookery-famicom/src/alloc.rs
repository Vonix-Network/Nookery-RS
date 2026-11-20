//! PC arena plan, ported from `famicom_emu_init` / `famicom_emu_cleanup` and
//! the `Famicom_MallocInfo` callbacks in `src/famicom_emu.c`.
//!
//! This module deliberately contains **no allocator**. The upstream PC path
//! borrows the game's zelda/Aurora arena (declared in `_mem.h`, implemented
//! behind game runtime state), so a working reimplementation cannot be proven
//! from these sources alone. What *is* source-observable is the contract:
//!
//! * A single host block of `0x400000` bytes (4 MiB) is allocated with
//!   `malloc` for the emulator; the GameCube build instead repurposes an XFB
//!   of `fbWidth * xfbHeight * sizeof(u16)` bytes.
//! * The arena is initialised at `ALIGN_NEXT(base, 16)` with
//!   `size - (aligned - base)` bytes, i.e. the leading slack up to 16-byte
//!   alignment is trimmed off the front and never given back.
//! * Every successful `my_zelda_malloc_align` result is filled with `0xFF`
//!   over the full requested size, so no emulator allocation sees stale heap
//!   content.
//! * Cleanup order is fixed: fade in, `famicom_cleanup`, arena teardown,
//!   free the host block and clear the bookkeeping, `sAdo_SubGameEnd`, then
//!   clear the PC active flag.
//!
//! `Famicom_MallocInfo`'s four callbacks are represented as
//! [`FamicomAllocator`]; the four-byte-alignment-trim arithmetic as
//! [`ArenaPlan`]; the teardown as [`PC_CLEANUP_SEQUENCE`].

use core::num::NonZeroUsize;
use core::ptr::NonNull;

/// Host block size on PC: `freeXfbSize = 0x400000`.
pub const PC_ARENA_REQUEST_BYTES: usize = 0x40_0000;

/// Arena alignment: `ALIGN_NEXT(alloc, 16)` and `THA_alloc16`.
pub const ARENA_ALIGNMENT: usize = 16;

/// Fill applied to every emulator allocation: `memset(aligned, 0xFF, size)`.
pub const ALLOCATION_FILL_BYTE: u8 = 0xFF;

/// Why the arena could not be laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArenaPlanError {
    /// Requested size was zero, which upstream never asks for but which
    /// cannot describe a usable arena.
    EmptyRequest,
    /// Alignment was not a power of two, so `ALIGN_NEXT` would be wrong.
    AlignmentNotPowerOfTwo {
        /// The rejected alignment.
        alignment: usize,
    },
    /// The leading trim consumed the entire request, leaving no arena.
    TrimConsumesArena {
        /// Bytes trimmed for alignment.
        trim: usize,
        /// Bytes requested.
        requested: usize,
    },
    /// `base + alignment - 1` overflowed, so `ALIGN_NEXT` cannot be evaluated
    /// at this address.
    UnrepresentableBase {
        /// The rejected base address.
        base: usize,
        /// The alignment that overflowed.
        alignment: usize,
    },
}

impl core::fmt::Display for ArenaPlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyRequest => f.write_str("famicom arena request is empty"),
            Self::AlignmentNotPowerOfTwo { alignment } => {
                write!(
                    f,
                    "famicom arena alignment {alignment} is not a power of two"
                )
            }
            Self::TrimConsumesArena { trim, requested } => write!(
                f,
                "famicom arena alignment trim {trim} consumes the whole {requested}-byte request"
            ),
            Self::UnrepresentableBase { base, alignment } => write!(
                f,
                "famicom arena base {base:#x} cannot be aligned to {alignment} without overflow"
            ),
        }
    }
}

impl core::error::Error for ArenaPlanError {}

/// Result of laying the emulator arena out inside a host block.
///
/// Mirrors the PC branch of `my_alloc_init`:
///
/// ```c
/// uintptr_t aligned = ALIGN_NEXT((uintptr_t)start, 16);
/// size_t tsize = aligned - (uintptr_t)start;
/// zelda_InitArena((void*)aligned, size - tsize);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaPlan {
    base: usize,
    trim: usize,
    arena_bytes: usize,
}

impl ArenaPlan {
    /// Lays out an arena for a host block at `base`.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaPlanError`] when `requested` is zero, when `alignment`
    /// is not a power of two, when the alignment trim leaves no bytes, or
    /// when `base` cannot be aligned without overflowing the address space.
    pub const fn new(
        base: usize,
        requested: usize,
        alignment: usize,
    ) -> Result<Self, ArenaPlanError> {
        if requested == 0 {
            return Err(ArenaPlanError::EmptyRequest);
        }
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(ArenaPlanError::AlignmentNotPowerOfTwo { alignment });
        }

        let trim = Self::trim_for(base, alignment);
        if trim >= requested {
            return Err(ArenaPlanError::TrimConsumesArena { trim, requested });
        }
        if base.checked_add(trim).is_none() {
            return Err(ArenaPlanError::UnrepresentableBase { base, alignment });
        }

        Ok(Self {
            base,
            trim,
            arena_bytes: requested - trim,
        })
    }

    /// `ALIGN_NEXT(u, align) - u` from `include/types.h`, evaluated without
    /// overflow.
    ///
    /// The macro form `(u + align - 1) & ~(align - 1)` wraps for an
    /// unaligned `u` near `usize::MAX`; the equivalent remainder form does
    /// not, and matches the macro everywhere the macro is defined.
    #[must_use]
    pub const fn trim_for(base: usize, alignment: usize) -> usize {
        let offset = base & (alignment - 1);
        (alignment - offset) & (alignment - 1)
    }

    /// Address the arena is initialised at: `ALIGN_NEXT(base, alignment)`.
    #[must_use]
    pub const fn arena_start(&self) -> usize {
        self.base + self.trim
    }

    /// Leading bytes trimmed for alignment (`tsize` upstream).
    #[must_use]
    pub const fn trim(&self) -> usize {
        self.trim
    }

    /// Bytes handed to `zelda_InitArena` (`size - tsize` upstream).
    #[must_use]
    pub const fn arena_bytes(&self) -> usize {
        self.arena_bytes
    }

    /// Host block address the plan was built from (`freeXfbBase` upstream).
    #[must_use]
    pub const fn host_base(&self) -> usize {
        self.base
    }

    /// Whether a request of `bytes` at this alignment succeeds.
    ///
    /// Conservative: reports `false` if the size does not fit the arena, but
    /// the zelda allocator may still fail earlier on fragmentation.
    #[must_use]
    pub const fn fits(&self, bytes: usize) -> bool {
        bytes <= self.arena_bytes
    }
}

/// The PC plan: 4 MiB, 16-byte alignment, `0xFF` fill.
#[must_use]
pub fn pc_arena_plan(base: usize) -> ArenaPlan {
    match ArenaPlan::new(base, PC_ARENA_REQUEST_BYTES, ARENA_ALIGNMENT) {
        Ok(plan) => plan,
        // Unreachable: 4 MiB with a 16-byte alignment trim is always usable.
        Err(_) => ArenaPlan {
            base,
            trim: 0,
            arena_bytes: PC_ARENA_REQUEST_BYTES,
        },
    }
}

/// Upgrade of the upstream `start != NULL && size != 0` gate.
///
/// On PC the arena is initialised unconditionally from the malloc'd block;
/// the `start`/`size` pair is only consulted by the GameCube branch's
/// `zelda_AddBlockArena`. Keeping the helper makes the gate itself testable.
#[must_use]
pub const fn accepts_extra_block(start: usize, size: usize) -> bool {
    start != 0 && size != 0
}

/// The four `Famicom_MallocInfo` callbacks from `include/Famicom/famicom.h`.
///
/// A host implements this against its own arena. This crate provides plans and
/// contract documentation only; it does not implement `zelda_malloc_align`,
/// `zelda_free`, `zelda_GetMemBlockSize` or `zelda_GetTotalFreeSize`.
pub trait FamicomAllocator {
    /// `my_zelda_malloc_align`: allocate `size` bytes at `align`, then fill
    /// the result with `0xFF`.
    ///
    /// `None` maps to upstream `NULL`, which the emulator treats as failure.
    fn malloc_align(&mut self, size: NonZeroUsize, align: NonZeroUsize) -> Option<NonNull<u8>>;

    /// `my_zelda_free`.
    fn free(&mut self, ptr: NonNull<u8>);

    /// `my_zelda_getmemblocksize`.
    fn mem_block_size(&self, ptr: NonNull<u8>) -> usize;

    /// `my_zelda_gettotalfreesize`.
    fn total_free_size(&self) -> usize;
}

/// Ordered steps of `famicom_emu_cleanup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CleanupStep {
    /// `JC_JFWDisplay_startFadeIn(manager, 1)`.
    FadeIn,
    /// `famicom_cleanup()`; a non-zero return sets message flag bit 2.
    EmulatorCleanup,
    /// `my_alloc_cleanup()` → `zelda_CleanupArena()`.
    ArenaCleanup,
    /// PC: `free(freeXfbBase)`, then base/size cleared. GameCube:
    /// `JC_JFWDisplay_changeToDoubleXfb(manager)`.
    ReleaseHostBlock,
    /// `sAdo_SubGameEnd()`.
    SubGameEnd,
    /// PC: `g_pc_nes_active = 0`.
    ClearActiveFlag,
}

/// `famicom_emu_cleanup` order, including the conditional host-block release.
pub const PC_CLEANUP_SEQUENCE: [CleanupStep; 6] = [
    CleanupStep::FadeIn,
    CleanupStep::EmulatorCleanup,
    CleanupStep::ArenaCleanup,
    CleanupStep::ReleaseHostBlock,
    CleanupStep::SubGameEnd,
    CleanupStep::ClearActiveFlag,
];

/// Message-control flag bits written by the entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageFlags(u8);

impl MessageFlags {
    /// `my_room_message_control_flags | 1`: set on ROM load failure.
    pub const LOAD_FAILED: Self = Self(1);
    /// `my_room_message_control_flags | 2`: set when `famicom_cleanup` fails.
    pub const CLEANUP_FAILED: Self = Self(2);

    /// Raw bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}
