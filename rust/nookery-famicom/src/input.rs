//! Controller button bits, copied from `include/types.h`.
//!
//! The exit combos in `famicom_emu_main` are matched with `==` against
//! `current_pad->now.button | current_pad->on.button`, so every bit in these
//! masks must match the upstream values exactly; a wrong bit silently turns a
//! working combo into a dead one.

/// `BUTTON_CRIGHT` — C-stick right.
pub const BUTTON_CRIGHT: u16 = 0x0001;
/// `BUTTON_CLEFT` — C-stick left.
pub const BUTTON_CLEFT: u16 = 0x0002;
/// `BUTTON_CDOWN` — C-stick down.
pub const BUTTON_CDOWN: u16 = 0x0004;
/// `BUTTON_CUP` — C-stick up.
pub const BUTTON_CUP: u16 = 0x0008;
/// `BUTTON_R` — R trigger.
pub const BUTTON_R: u16 = 0x0010;
/// `BUTTON_L` — L trigger.
pub const BUTTON_L: u16 = 0x0020;
/// `BUTTON_X` — X button.
pub const BUTTON_X: u16 = 0x0040;
/// `BUTTON_Y` — Y button.
pub const BUTTON_Y: u16 = 0x0080;
/// `BUTTON_DRIGHT` — D-pad right.
pub const BUTTON_DRIGHT: u16 = 0x0100;
/// `BUTTON_DLEFT` — D-pad left.
pub const BUTTON_DLEFT: u16 = 0x0200;
/// `BUTTON_DDOWN` — D-pad down.
pub const BUTTON_DDOWN: u16 = 0x0400;
/// `BUTTON_DUP` — D-pad up.
pub const BUTTON_DUP: u16 = 0x0800;
/// `BUTTON_START` — Start.
pub const BUTTON_START: u16 = 0x1000;
/// `BUTTON_Z` — Z button.
pub const BUTTON_Z: u16 = 0x2000;
/// `BUTTON_B` — B button.
pub const BUTTON_B: u16 = 0x4000;
/// `BUTTON_A` — A button.
pub const BUTTON_A: u16 = 0x8000;

/// First exit combo: `BUTTON_L | BUTTON_R | BUTTON_Z` (`0x2030`).
pub const EXIT_COMBO_L_R_Z: u16 = BUTTON_L | BUTTON_R | BUTTON_Z;

/// Second exit combo: `BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y` (`0x00F0`).
pub const EXIT_COMBO_L_R_X_Y: u16 = BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y;

/// Returns `true` when `combo` is one of the two exact exit combos.
///
/// Mirrors the two `==` comparisons in `famicom_emu_main`: the match is exact,
/// so any additional held button (Start, A, a D-pad direction, ...) makes the
/// value non-matching and does not exit the emulator.
#[must_use]
pub const fn is_exit_combo(combo: u16) -> bool {
    combo == EXIT_COMBO_L_R_Z || combo == EXIT_COMBO_L_R_X_Y
}
