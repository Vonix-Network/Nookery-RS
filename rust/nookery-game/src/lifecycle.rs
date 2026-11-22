//! The initialization/transition/destruction slice from `src/game.c` and
//! `src/first_game.c`.
//!
//! The C functions call several subsystems that are outside this crate.  They
//! are therefore represented by [`GameLifecycleHooks`], whose methods are all
//! required: a production integration cannot accidentally turn one of those
//! calls into a silent no-op.

use std::error::Error;
use std::fmt;

/// `GAME_HYRAL_SIZE` from `include/game.h`.
pub const GAME_HYRAL_SIZE: usize = 0x100000;
/// `GAME_FRAME` from `include/game.h`.
pub const GAME_FRAME: i32 = 1;
/// `sizeof(GAME_SECOND)` from `include/second_game.h`.
pub const GAME_SECOND_SIZE: usize = 0xE0;

/// The two version branches used by `#if VERSION < VER_GAFU01_00` in
/// `src/game.c`.  The checkout's default C configuration is the first branch.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GameVersion {
    #[default]
    Gafe01_00,
    Gafu01_00,
}

/// The callback identity installed by `first_game.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveChannelCallback {
    McdGetThisLandSlotNo,
}

/// The destination written by `GAME_GOTO_NEXT(game, second_game, SECOND)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextGame {
    Second,
}

/// Which cleanup function is installed in the C `GAME` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupCallback {
    First,
}

/// Lifecycle state used to reject unsafe repeated `game_ct`/`game_dt` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    Cold,
    Active,
    AllocationFailed,
    Cleaned,
}

/// Failure returned by the checked Rust boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    /// The source path reached `gamealloc_malloc` but returned `NULL`.
    HyralAllocationFailed { requested: usize },
    /// The caller attempted a lifecycle operation in the wrong phase.
    InvalidPhase {
        operation: &'static str,
        actual: LifecyclePhase,
    },
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HyralAllocationFailed { requested } => {
                write!(f, "hyral allocation failed for {requested:#x} bytes")
            }
            Self::InvalidPhase { operation, actual } => {
                write!(f, "cannot {operation} while game is {actual:?}")
            }
        }
    }
}

impl Error for LifecycleError {}

/// The process-global values written by `SetGameFrame` in `src/game.c`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameGlobals {
    pub game_frame: u8,
    pub game_frame_f: f32,
    pub game_frame_2f: f32,
    pub game_frame_inverse: f32,
    pub sreg_30: u16,
    pub sreg_29: u16,
}

impl Default for GameGlobals {
    fn default() -> Self {
        Self {
            game_frame: 0,
            game_frame_f: 0.0,
            game_frame_2f: 0.0,
            game_frame_inverse: 0.0,
            sreg_30: 0,
            sreg_29: 0,
        }
    }
}

impl GameGlobals {
    fn set_game_frame<H: GameLifecycleHooks>(&mut self, frame: i32, hooks: &mut H) {
        // This mirrors the C comparison before the `(u8)frame` conversion.
        if self.game_frame as i32 == frame {
            return;
        }

        self.game_frame = frame as u8;
        self.game_frame_f = (self.game_frame as i32) as f32;
        self.game_frame_2f = self.game_frame_f * 0.5;
        self.game_frame_inverse = 1.0 / self.game_frame_f;
        self.sreg_30 = frame as u16;

        let display_frame = if self.sreg_29 > 0 {
            self.sreg_29 as i32
        } else {
            frame
        };
        hooks.display_set_frame_rate(display_frame);
    }
}

/// The externally observable fields of the C `GAME` object for this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    phase: LifecyclePhase,
    pub graph_attached: bool,
    pub graph_need_viupdate: bool,
    pub frame_counter: u32,
    pub exec_installed: bool,
    pub cleanup: Option<CleanupCallback>,
    pub next_game: Option<NextGame>,
    pub next_game_class_size: usize,
    pub pad_initialized: bool,
    pub doing: bool,
    pub disable_display: bool,
    pub disable_prenmi: bool,
    pub gamealloc_initialized: bool,
    pub tha_size: usize,
    pub hyral_allocated: bool,
    pub current_game: bool,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            phase: LifecyclePhase::Cold,
            graph_attached: false,
            graph_need_viupdate: false,
            frame_counter: 0,
            exec_installed: false,
            cleanup: None,
            next_game: None,
            next_game_class_size: 0,
            pad_initialized: false,
            doing: false,
            disable_display: false,
            disable_prenmi: false,
            gamealloc_initialized: false,
            tha_size: 0,
            hyral_allocated: false,
            current_game: false,
        }
    }
}

impl Game {
    pub const fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Port `game_ct` for one caller-provided game initializer.
    pub fn ct<H: GameLifecycleHooks, I: GameInitializer<H>>(
        &mut self,
        graph: &mut GraphState,
        globals: &mut GameGlobals,
        version: GameVersion,
        initializer: I,
        hooks: &mut H,
    ) -> Result<(), LifecycleError> {
        self.require_phase("construct", LifecyclePhase::Cold)?;
        self.phase = LifecyclePhase::Active;
        self.current_game = true;

        // The order is the order in `game_ct`: vi update, controller ctor,
        // graph assignment, scalar reset, next-game clear, allocator init,
        // hyral setup, frame setup, high-res flag, init callback, audio init.
        graph.need_viupdate = true;
        self.graph_need_viupdate = graph.need_viupdate;
        hooks.mcon_ct();
        self.graph_attached = true;
        self.frame_counter = 0;
        self.exec_installed = false;
        self.cleanup = None;
        self.doing = true;
        self.pad_initialized = true;
        self.next_game = None;
        self.next_game_class_size = 0;
        self.gamealloc_initialized = true;
        hooks.gamealloc_init();

        let allocated = hooks.gamealloc_malloc(GAME_HYRAL_SIZE);
        self.tha_size = if allocated { GAME_HYRAL_SIZE } else { 0 };
        self.hyral_allocated = allocated;
        hooks.tha_ct(self.tha_size);
        if !allocated {
            self.phase = LifecyclePhase::AllocationFailed;
            return Err(LifecycleError::HyralAllocationFailed {
                requested: GAME_HYRAL_SIZE,
            });
        }

        globals.set_game_frame(GAME_FRAME, hooks);
        hooks.gfxprint_enable_highres();
        initializer.init(self, hooks);
        hooks.mbgm_init();
        if version == GameVersion::Gafe01_00 {
            hooks.gba_init();
        }
        hooks.mvibctl_init();
        Ok(())
    }

    /// Port `game_dt` in its exact cleanup order.
    pub fn dt<H: GameLifecycleHooks>(&mut self, hooks: &mut H) -> Result<(), LifecycleError> {
        self.require_phase("destruct", LifecyclePhase::Active)?;

        hooks.mcon_dt();
        hooks.mbgm_cleanup();
        if self.cleanup == Some(CleanupCallback::First) {
            // The C function body is empty, but the installed callback is
            // still invoked and remains observable at this boundary.
            hooks.first_game_cleanup();
        }
        hooks.mvibctl_cleanup();
        hooks.tha_dt();
        hooks.gamealloc_cleanup();
        self.current_game = false;
        self.phase = LifecyclePhase::Cleaned;
        Ok(())
    }

    fn require_phase(
        &self,
        operation: &'static str,
        expected: LifecyclePhase,
    ) -> Result<(), LifecycleError> {
        if self.phase == expected {
            Ok(())
        } else {
            Err(LifecycleError::InvalidPhase {
                operation,
                actual: self.phase,
            })
        }
    }
}

/// Minimal graph state corresponding to the field written by `game_ct`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GraphState {
    pub need_viupdate: bool,
}

/// The `first_game_init` callback used by the lifecycle slice.
#[derive(Debug, Default, Clone, Copy)]
pub struct FirstGameInitializer;

/// A game-specific initializer invoked from `game_ct`.
pub trait GameInitializer<H: GameLifecycleHooks> {
    fn init(&self, game: &mut Game, hooks: &mut H);
}

impl<H: GameLifecycleHooks> GameInitializer<H> for FirstGameInitializer {
    fn init(&self, game: &mut Game, hooks: &mut H) {
        // first_game_init
        game.cleanup = Some(CleanupCallback::First);
        hooks.sys_romcheck();
        hooks.mcd_save_data_aram_malloc();

        // exit_game
        hooks.famicom_set_save_channel(SaveChannelCallback::McdGetThisLandSlotNo);
        hooks.mbgm_ct();
        hooks.mvibctl_ct();
        hooks.common_data_init();
        game.doing = false;
        game.next_game = Some(NextGame::Second);
        game.next_game_class_size = GAME_SECOND_SIZE;
    }
}

/// Required subsystem calls made by the ported C functions.
pub trait GameLifecycleHooks {
    fn mcon_ct(&mut self);
    fn mcon_dt(&mut self);
    fn gamealloc_init(&mut self);
    fn gamealloc_malloc(&mut self, size: usize) -> bool;
    fn gamealloc_cleanup(&mut self);
    fn tha_ct(&mut self, size: usize);
    fn tha_dt(&mut self);
    fn display_set_frame_rate(&mut self, frame: i32);
    fn gfxprint_enable_highres(&mut self);
    fn mbgm_init(&mut self);
    fn mbgm_cleanup(&mut self);
    fn mbgm_ct(&mut self);
    fn mvibctl_init(&mut self);
    fn mvibctl_cleanup(&mut self);
    fn mvibctl_ct(&mut self);
    fn gba_init(&mut self);
    fn sys_romcheck(&mut self);
    fn mcd_save_data_aram_malloc(&mut self);
    fn famicom_set_save_channel(&mut self, callback: SaveChannelCallback);
    fn common_data_init(&mut self);
    fn first_game_cleanup(&mut self);
}
