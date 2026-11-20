//! Bounded exit state machine and per-frame decision, ported from
//! `famicom_emu_main` in `src/famicom_emu.c`.
//!
//! Upstream control flow, annotated (the source of every rule below):
//!
//! ```c
//! if (!famicom_done) {
//!     if (famicom_rom_load_check() < 0) {          /* (A) load gate */
//!         Common_Set(my_room_message_control_flags, ... | 1);
//!         famicom_done = TRUE;
//!         famicom_done_countdown = 0;              /* no fade, return now */
//!     } else {
//!         for (padid = 0; padid < 4; padid++) {    /* (B) four pads */
//!             combo = pad->now.button | pad->on.button;
//!             if (combo == (L|R|Z) || combo == (L|R|X|Y)) {
//!                 famicom_done = TRUE;
//!                 famicom_done_countdown = 60;
//!                 JC_JFWDisplay_startFadeOut(manager, 60);
//!                 break;
//!             }
//!         }
//!     }
//! }
//! if (famicom_done) {                              /* (C) countdown */
//!     if (famicom_done_countdown == 0) return_emu_game(famicom);
//!     else famicom_done_countdown--;
//! }
//! JW_BeginFrame();
//! famicom->disable_display = 1;
//! if (!famicom_done) {
//!     sAdo_GameFrame();        /* (D) TARGET_PC only */
//!     famicom_1frame();
//! } else {
//!     JC_JFWDisplay_clearEfb(manager, black_color);
//! }
//! JW_EndFrame();
//! ```
//!
//! Consequences encoded here, each with a test:
//!
//! * The load gate runs before the combo scan, so a failing load never
//!   evaluates a combo and never asks for a fade (`observe_rom_load`
//!   precedence).
//! * The combo test is exact equality, not a superset test, so a held Start
//!   or D-pad cancels the exit (`input::is_exit_combo`).
//! * The combo scan only runs while `!famicom_done`, so repeated presses
//!   during the fade are ignored and cannot restart the countdown.
//! * `60` is both the countdown and the fade length passed to
//!   `startFadeOut`; the triggering frame already decrements to `59` and
//!   draws black, and the return happens on the first frame whose countdown
//!   reads `0`.
//! * The frame that returns to the game still falls through to the black EFB
//!   clear, because `return_emu_game` does not `return` from the loop body.

use crate::input::is_exit_combo;

/// Frames requested from both the countdown and `JC_JFWDisplay_startFadeOut`.
pub const FADEOUT_FRAMES: u16 = 60;

/// Controller slots scanned by the upstream combo loop (`padid < 4`).
pub const PAD_COUNT: usize = 4;

/// Outcome of `famicom_rom_load_check()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadResult {
    /// `>= 0`: the ROM is loaded, the combo scan runs.
    Loaded,
    /// `< 0`: sets message flag bit 1, latches done, and returns immediately.
    Failed,
}

/// Host platform, which selects the PC-only audio pump in the frame body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// `TARGET_PC`: `sAdo_GameFrame()` runs before every NES frame.
    Pc,
    /// GameCube/Wii: no host audio pump inside the emulator loop.
    GameCube,
}

impl Platform {
    /// Whether the upstream frame body pumps host audio before the frame.
    #[must_use]
    pub const fn pumps_audio_before_frame(self) -> bool {
        matches!(self, Self::Pc)
    }
}

/// Observable state of the pair `famicom_done` / `famicom_done_countdown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitPhase {
    /// `!famicom_done`: the emulator is playing a frame per tick.
    Running,
    /// Load failed: done latched with countdown `0`, return on the next tick,
    /// no fade-out was requested.
    LoadFailed,
    /// `famicom_done && famicom_done_countdown > 0`: fading, presenting black.
    FadeOut {
        /// Frames still to elapse before the return. `1` is the last
        /// black frame; the next tick returns.
        remaining: u16,
    },
    /// Countdown drained (`0`) or the return was already issued.
    ReadyToReturn,
}

/// What the current frame body must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameAction {
    /// `!famicom_done`: optionally pump host audio, then run one NES frame.
    AdvanceFrame {
        /// `TARGET_PC` audio pump precedes the frame (`sAdo_GameFrame`).
        pump_audio: bool,
    },
    /// `famicom_done` with frames left: `JC_JFWDisplay_clearEfb(black)`.
    PresentBlack,
    /// Countdown at `0`: `return_emu_game`. Upstream also falls through to the
    /// black EFB clear in the same frame.
    ReturnToGame,
}

impl FrameAction {
    /// Whether this frame issues the black EFB clear.
    ///
    /// `ReturnToGame` reports `true` because upstream has no early return.
    #[must_use]
    pub const fn clears_efb(self) -> bool {
        !matches!(self, Self::AdvanceFrame { .. })
    }

    /// Whether host audio is pumped before the frame.
    #[must_use]
    pub const fn pumps_audio(self) -> bool {
        matches!(self, Self::AdvanceFrame { pump_audio: true })
    }
}

/// The full `famicom_done` / `famicom_done_countdown` state plus platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmulatorExit {
    phase: ExitPhase,
    platform: Platform,
}

impl EmulatorExit {
    /// Fresh entry: `famicom_done = FALSE`, `famicom_done_countdown = 0`.
    #[must_use]
    pub const fn new(platform: Platform) -> Self {
        Self {
            phase: ExitPhase::Running,
            platform,
        }
    }

    /// Current phase.
    #[must_use]
    pub const fn phase(&self) -> ExitPhase {
        self.phase
    }

    /// Platform this instance was created for.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Current `famicom_done_countdown` value: `0` outside the fade.
    #[must_use]
    pub const fn countdown(&self) -> u16 {
        match self.phase {
            ExitPhase::FadeOut { remaining } => remaining,
            ExitPhase::Running | ExitPhase::LoadFailed | ExitPhase::ReadyToReturn => 0,
        }
    }

    /// Whether `famicom_done` is latched.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        !matches!(self.phase, ExitPhase::Running)
    }

    /// Applies `famicom_rom_load_check()`.
    ///
    /// Returns `true` when this call latched the failure. On failure it sets
    /// [`ExitPhase::LoadFailed`] with countdown `0` and requests message flag
    /// bit 1 from the caller.
    ///
    /// Ignored once done, matching the `if (!famicom_done)` guard: a failed
    /// check cannot re-trigger, and a late successful check cannot resurrect
    /// a finished session.
    pub fn observe_rom_load(&mut self, result: LoadResult) -> bool {
        if self.is_done() {
            return false;
        }

        match result {
            LoadResult::Loaded => false,
            LoadResult::Failed => {
                self.phase = ExitPhase::LoadFailed;
                true
            }
        }
    }

    /// Applies one pad's `now.button | on.button` value to the combo scan.
    ///
    /// Returns `true` when this call latched the exit and requested the
    /// 60-frame fade. Ignored once done, and ignored when the load gate has
    /// already failed, because upstream evaluates the combo scan only in the
    /// `else` branch of the load check.
    pub fn observe_exit_combo(&mut self, buttons: u16) -> bool {
        if self.phase != ExitPhase::Running {
            return false;
        }
        if !is_exit_combo(buttons) {
            return false;
        }

        self.phase = ExitPhase::FadeOut {
            remaining: FADEOUT_FRAMES,
        };
        true
    }

    /// Applies the combo scan to all four pads in upstream order, stopping at
    /// the first match (`break`).
    ///
    /// Returns the matching pad index, if any.
    pub fn observe_pads(&mut self, pads: &[u16; PAD_COUNT]) -> Option<usize> {
        for (padid, &buttons) in pads.iter().enumerate() {
            if self.observe_exit_combo(buttons) {
                return Some(padid);
            }
        }
        None
    }

    /// Advances one frame and reports what the frame body must do.
    ///
    /// * Running: [`FrameAction::AdvanceFrame`].
    /// * Fading with frames left: decrement, [`FrameAction::PresentBlack`].
    ///   The decrement that reaches `0` still presents black (upstream clears
    ///   the EFB on that frame and returns on the next).
    /// * Countdown at `0` (load failure, drained fade, or already returned):
    ///   [`FrameAction::ReturnToGame`].
    pub fn tick(&mut self) -> FrameAction {
        match self.phase {
            ExitPhase::Running => FrameAction::AdvanceFrame {
                pump_audio: self.platform.pumps_audio_before_frame(),
            },
            ExitPhase::FadeOut { remaining } => {
                let next = remaining.saturating_sub(1);
                self.phase = if next == 0 {
                    ExitPhase::ReadyToReturn
                } else {
                    ExitPhase::FadeOut { remaining: next }
                };
                FrameAction::PresentBlack
            }
            ExitPhase::LoadFailed => {
                self.phase = ExitPhase::ReadyToReturn;
                FrameAction::ReturnToGame
            }
            ExitPhase::ReadyToReturn => FrameAction::ReturnToGame,
        }
    }
}

/// Host presentation boundaries of the loop body.
///
/// Every method is an adapter point with no emulator, GX, JFWDisplay or audio
/// implementation behind it. The defaults are deliberate no-ops so a host can
/// opt into only the boundaries it owns; `advance_famicom_frame` has no
/// default because a silent no-op would look like a working emulator.
pub trait HostSeam {
    /// `JW_BeginFrame()` plus `famicom->disable_display = 1`.
    fn begin_frame(&mut self) {}

    /// `sAdo_GameFrame()` on PC: pump the host audio engine. Called before
    /// [`HostSeam::advance_famicom_frame`] on the same frame.
    fn pump_audio(&mut self) {}

    /// `famicom_1frame()`: run exactly one NES frame. Implemented by the host
    /// emulator binding; this crate never emulates.
    fn advance_famicom_frame(&mut self);

    /// `JC_JFWDisplay_clearEfb(manager, black)`: present the black frame.
    fn present_black(&mut self) {}

    /// `return_emu_game(game)`: leave the emulator scene. The host also
    /// performs the black clear on this frame (see
    /// [`FrameAction::clears_efb`]).
    fn return_to_game(&mut self) {}

    /// `JW_EndFrame()`.
    fn end_frame(&mut self) {}

    /// `JC_JFWDisplay_startFadeOut(manager, frames)`, requested at the moment
    /// the exit combo latches.
    fn request_fade_out(&mut self, frames: u16) {
        let _ = frames;
    }

    /// `Common_Set(my_room_message_control_flags, flags | 1)` on load failure.
    fn report_load_failure(&mut self) {}
}

/// Runs one full upstream loop iteration against a host seam.
///
/// Order: optional load check, optional pad scan, then the
/// begin/dispatch/end frame triplet exactly as `famicom_emu_main` arranges
/// it. Returns the [`FrameAction`] that was executed.
pub fn dispatch(
    exit: &mut EmulatorExit,
    host: &mut dyn HostSeam,
    load: Option<LoadResult>,
    pads: Option<&[u16; PAD_COUNT]>,
) -> FrameAction {
    if let Some(result) = load {
        if exit.observe_rom_load(result) {
            host.report_load_failure();
        }
    }

    if let Some(pads) = pads {
        if exit.observe_pads(pads).is_some() {
            host.request_fade_out(FADEOUT_FRAMES);
        }
    }

    host.begin_frame();

    let action = exit.tick();
    match action {
        FrameAction::AdvanceFrame { pump_audio } => {
            if pump_audio {
                host.pump_audio();
            }
            host.advance_famicom_frame();
        }
        FrameAction::PresentBlack => host.present_black(),
        FrameAction::ReturnToGame => {
            host.return_to_game();
            // Upstream does not return from the loop body, so the black EFB
            // clear still runs on this frame.
            host.present_black();
        }
    }

    host.end_frame();
    action
}
