//! Bounded, pure Famicom/NES host seam ported from the PC port layer
//! (`src/famicom_emu.c`, `include/famicom_emu.h`).
//!
//! # Scope
//!
//! This crate ports only the parts of the Famicom entry point that are pure,
//! source-verifiable, and independent of game assets and of the emulator core:
//!
//! * The ROM resource catalog of `famicom_gba_getImage`: the static 19-slot
//!   name table, the `rom_id == 0` alias onto slot 1, the `rom_id > 19`
//!   rejection, the empty slots, and the exact bundled resource path
//!   `/FAMICOM/GBA/jb_<name>.bin.szs`.
//! * The exit state machine of `famicom_emu_main`: the `famicom_done` /
//!   `famicom_done_countdown` pair, the two exact-match pad combos, the
//!   60-frame fade-out request, and the load-failure short circuit.
//! * The per-frame decision of that same loop body: pump audio then run one
//!   NES frame, or clear/present black, or return to the game scene.
//! * A mapper-0 CPU-driven, frame-sampled PPU bus and indexed 256x240 picture.
//! * The PC allocator plan of `famicom_emu_init` / `famicom_emu_cleanup`:
//!   the 4 MiB host block, the 16-byte arena alignment trim, the `0xFF`
//!   fill applied to every emulator allocation, and the ordered cleanup.
//!
//! # Explicit non-goals
//!
//! * No mapper beyond mapper 0, APU synthesis, DMC DMA, unofficial opcodes,
//!   or exact mid-scanline PPU timing/NMI sequencing. OAM DMA copies the
//!   requested page through the PPU path and accounts for its 513/514-cycle
//!   CPU stall, while its per-cycle read/write/PPU-dot interleaving remains a
//!   typed gap.
//! * No audio, framebuffer, GX, JFWDisplay or window calls. `JW_BeginFrame`,
//!   `famicom->disable_display = 1`, `JC_JFWDisplay_startFadeOut` and the
//!   black EFB clear are host presentation boundaries; only their ordering
//!   relative to the frame action is encoded.
//! * No game ROMs, no `.szs` assets, no resource heap. Paths are produced as
//!   strings; nothing is loaded.
//! * No zelda/Aurora arena implementation. The observed `Famicom_MallocInfo`
//!   contract is a trait plus a layout plan, not a working heap.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod alloc;
pub mod core;
pub mod exit;
pub mod input;
pub mod ppu;
pub mod rom;
pub mod runtime;

pub use alloc::{
    ALLOCATION_FILL_BYTE, ARENA_ALIGNMENT, ArenaPlan, ArenaPlanError, CleanupStep,
    FamicomAllocator, MessageFlags, PC_ARENA_REQUEST_BYTES, PC_CLEANUP_SEQUENCE,
    accepts_extra_block, pc_arena_plan,
};
pub use core::NOT_PORTED;
pub use exit::{
    EmulatorExit, ExitPhase, FADEOUT_FRAMES, FrameAction, HostSeam, LoadResult, PAD_COUNT,
    Platform, dispatch,
};
pub use input::{
    BUTTON_A, BUTTON_B, BUTTON_CDOWN, BUTTON_CLEFT, BUTTON_CRIGHT, BUTTON_CUP, BUTTON_DDOWN,
    BUTTON_DLEFT, BUTTON_DRIGHT, BUTTON_DUP, BUTTON_L, BUTTON_R, BUTTON_START, BUTTON_X, BUTTON_Y,
    BUTTON_Z, EXIT_COMBO_L_R_X_Y, EXIT_COMBO_L_R_Z, is_exit_combo,
};
pub use ppu::{FRAME_HEIGHT, FRAME_WIDTH, Frame, Mirroring, Ppu};
pub use rom::{
    MAX_ROM_ID, RESOURCE_PATH_CAPACITY, ROM_NAMES, ROM_SLOT_COUNT, ResourcePath, RomId, RomIdError,
};
pub use runtime::{FamicomRuntime, FrameReport, RuntimeError, RuntimeState, controller_byte};
