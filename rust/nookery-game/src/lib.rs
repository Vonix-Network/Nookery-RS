//! Rust implementation of the nookery game frame and executor semantics.

mod controller;
mod lifecycle;

pub use controller::{ControllerInput, ControllerState, STICK_MAX, STICK_MIN};
pub use lifecycle::{
    CleanupCallback, FirstGameInitializer, GAME_FRAME, GAME_HYRAL_SIZE, GAME_SECOND_SIZE, Game,
    GameGlobals, GameLifecycleHooks, GameVersion, GraphState, LifecycleError, LifecyclePhase,
    NextGame, SaveChannelCallback,
};

use std::error::Error;
use std::fmt;

const MAX_CATCH_UP_TICKS: f64 = 4.0;

/// Errors returned by frame-clock operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameError {
    /// A value that must be strictly positive was zero or negative.
    NonPositiveInput(&'static str),
    /// A value was not finite, representable, or otherwise valid.
    InvalidInput(&'static str),
}

impl fmt::Display for GameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositiveInput(name) => write!(f, "{name} must be positive"),
            Self::InvalidInput(name) => write!(f, "invalid {name}"),
        }
    }
}

impl Error for GameError {}

/// Stateless frame-clock operations matching the C graph helpers.
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameClock;

impl FrameClock {
    /// Add a frame delta, return whole ticks, retain the fractional remainder,
    /// and cap returned catch-up work at four ticks.
    pub fn ticks_for_frame(dt_num_60fps_frames: f64, accum: &mut f64) -> Result<u32, GameError> {
        validate_accum(accum)?;
        validate_positive_finite(dt_num_60fps_frames, "dt_num_60fps_frames")?;

        let next = *accum + dt_num_60fps_frames;
        if !next.is_finite() {
            return Err(GameError::InvalidInput("frame accumulation"));
        }
        let ticks = next.trunc();
        *accum = next - ticks;
        Ok(ticks.min(MAX_CATCH_UP_TICKS) as u32)
    }

    /// Add a frame delta and report one elapsed period. One period is removed;
    /// an excess beyond four periods is discarded as in the C helper.
    pub fn period_elapsed(
        dt_num_60fps_frames: f64,
        accum: &mut f64,
        period_frames: f64,
    ) -> Result<bool, GameError> {
        validate_accum(accum)?;
        validate_positive_finite(dt_num_60fps_frames, "dt_num_60fps_frames")?;
        validate_positive_finite(period_frames, "period_frames")?;

        let next = *accum + dt_num_60fps_frames;
        if !next.is_finite() {
            return Err(GameError::InvalidInput("period accumulation"));
        }
        *accum = next;
        if *accum >= period_frames {
            *accum -= period_frames;
            if *accum > period_frames * MAX_CATCH_UP_TICKS {
                *accum = 0.0;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Return total elapsed time in 60fps frame units.
    pub fn frame_time(total_60fps_frames: f64) -> Result<f64, GameError> {
        if !total_60fps_frames.is_finite() || total_60fps_frames < 0.0 {
            return Err(GameError::InvalidInput("total_60fps_frames"));
        }
        Ok(total_60fps_frames)
    }

    /// Return the integer frame-time phase within a positive frame period.
    pub fn frame_phase(total_60fps_frames: f64, period_frames: i64) -> Result<i64, GameError> {
        if !total_60fps_frames.is_finite() || total_60fps_frames < 0.0 {
            return Err(GameError::InvalidInput("total_60fps_frames"));
        }
        if period_frames <= 0 {
            return Err(GameError::NonPositiveInput("period_frames"));
        }
        let whole_frames = total_60fps_frames.trunc();
        if whole_frames > i64::MAX as f64 {
            return Err(GameError::InvalidInput("total_60fps_frames"));
        }
        Ok((whole_frames as i64) % period_frames)
    }
}

fn validate_accum(accum: &f64) -> Result<(), GameError> {
    if !accum.is_finite() || *accum < 0.0 {
        Err(GameError::InvalidInput("accum"))
    } else {
        Ok(())
    }
}

fn validate_positive_finite(value: f64, name: &'static str) -> Result<(), GameError> {
    if !value.is_finite() {
        Err(GameError::InvalidInput(name))
    } else if value <= 0.0 {
        Err(GameError::NonPositiveInput(name))
    } else {
        Ok(())
    }
}

/// Ordered constructor/main/destructor execution plan.
pub struct ExecutorPlan<'a> {
    constructors: Vec<Box<dyn FnMut() + 'a>>,
    main: Box<dyn FnMut() + 'a>,
    destructors: Vec<Box<dyn FnMut() + 'a>>,
}

impl<'a> ExecutorPlan<'a> {
    pub fn new<M>(main: M) -> Self
    where
        M: FnMut() + 'a,
    {
        Self {
            constructors: Vec::new(),
            main: Box::new(main),
            destructors: Vec::new(),
        }
    }

    pub fn constructor<C>(mut self, constructor: C) -> Self
    where
        C: FnMut() + 'a,
    {
        self.constructors.push(Box::new(constructor));
        self
    }

    pub fn destructor<D>(mut self, destructor: D) -> Self
    where
        D: FnMut() + 'a,
    {
        self.destructors.push(Box::new(destructor));
        self
    }

    pub fn run(mut self, construct_skip: bool) {
        if !construct_skip {
            for constructor in &mut self.constructors {
                constructor();
            }
        }
        (self.main)();
        // src/executor.c walks `_dtors` from the first entry to the sentinel;
        // teardown is registration-order, not stack-order.
        for destructor in &mut self.destructors {
            destructor();
        }
    }
}
