//! A bounded Rust seam for the graph and executor lifecycle in the C sources.
//!
//! The graph clock and lifecycle state below are grounded in `src/graph.c` and
//! `include/graph.h`: construction initializes the source timing fields,
//! `graph_proc` updates them before entering the game frame, and `graph_dt`
//! marks cleanup with `GRAPH_DOING_DT`.
//!
//! Deliberate boundary: the C `GRAPH` also owns raw `Gfx*` display-list
//! pointers, `THA_GA` arenas, task/message queues, frame buffers, and renderer
//! callbacks. `graph_setup_double_buffer`, display-list macros, task
//! submission, and renderer conversion are not represented here. The C
//! executor's constructor/destructor arrays and `_unresolved` stack walk are
//! likewise runtime/ABI callbacks, so this crate models their lifecycle only.

use std::error::Error;
use std::fmt;

/// The source `GRAPH_DOING_POINT` values, retained so lifecycle observations
/// have the same vocabulary as `include/graph.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GraphDoingPoint {
    Zero = 0,
    Ct,
    GameCt,
    GameCtFinished,
    GameMain,
    GameTime,
    GameTimeFinished,
    GameExec,
    GameExecFinished,
    GameBgm,
    GameBgmFinished,
    GameMainFinished,
    TaskSet,
    WaitTask,
    WaitTaskFinished,
    TaskSetFinished,
    Audio,
    AudioFinished,
    Game18,
    GameDt,
    GameDtFinished,
    Dt,
    End,
}

/// Coarse lifecycle phase for the source graph thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphLifecycle {
    New,
    Running,
    Processing,
    CleanedUp,
}

/// Errors raised when graph lifecycle calls arrive out of source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphLifecycleError {
    BeginWhileRunning,
    BeginAfterCleanup,
    ProcessBeforeBegin,
    ProcessWhileProcessing,
    ProcessAfterCleanup,
    FinishBeforeProcess,
    FinishAfterCleanup,
    EndBeforeBegin,
    EndWhileProcessing,
    EndAlreadyComplete,
}

impl fmt::Display for GraphLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BeginWhileRunning => "cannot begin a graph that is already running",
            Self::BeginAfterCleanup => "cannot begin a graph after cleanup without reset",
            Self::ProcessBeforeBegin => "cannot process a graph before begin",
            Self::ProcessWhileProcessing => "cannot process while a frame is already processing",
            Self::ProcessAfterCleanup => "cannot process a graph after cleanup",
            Self::FinishBeforeProcess => "cannot finish a frame before process",
            Self::FinishAfterCleanup => "cannot finish a frame after cleanup",
            Self::EndBeforeBegin => "cannot end a graph before begin",
            Self::EndWhileProcessing => "cannot end while a frame is processing",
            Self::EndAlreadyComplete => "cannot end a graph after cleanup",
        };
        formatter.write_str(message)
    }
}

impl Error for GraphLifecycleError {}

/// Timing fields corresponding to `GRAPH::dt`, `dt_num_60fps_frames`, and
/// `dt_total_60fps_frames` in `include/graph.h`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphClock {
    /// Seconds since the prior execution frame.
    pub dt: f64,
    /// Number of source 60-Hz frame units in the current execution frame.
    pub dt_num_60fps_frames: f64,
    /// Accumulated 60-Hz frame units.
    pub dt_total_60fps_frames: f64,
}

impl Default for GraphClock {
    fn default() -> Self {
        // `graph_ct` assigns FRAMES_TO_SECONDS(1.0f), 1.0f, and 0.0.
        Self {
            dt: 1.0 / 60.0,
            dt_num_60fps_frames: 1.0,
            dt_total_60fps_frames: 0.0,
        }
    }
}

impl GraphClock {
    /// Apply the source `graph_proc` delta update.
    ///
    /// The caller supplies the post-speedhack delta when a platform speedhack
    /// is in use. The source clamps a stalled frame to four 60-Hz units and
    /// derives the clamped seconds value from that unit count.
    pub fn advance(&mut self, delta_seconds: f64) {
        let mut delta_60fps_frames = delta_seconds * 60.0;
        let mut delta_seconds = delta_seconds;

        if delta_60fps_frames > 4.0 {
            delta_60fps_frames = 4.0;
            delta_seconds = delta_60fps_frames / 60.0;
        }

        self.dt = delta_seconds;
        self.dt_num_60fps_frames = delta_60fps_frames;
        self.dt_total_60fps_frames += delta_60fps_frames;
    }

    /// Restore the values assigned by `graph_ct`.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Source-observable lifecycle state retained by the bounded graph seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphState {
    /// Corresponds to `GRAPH::frame_counter` (a C `int`).
    pub frame_counter: i32,
    /// Corresponds to `GRAPH::doing_point`.
    pub doing_point: GraphDoingPoint,
    lifecycle: GraphLifecycle,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            frame_counter: 0,
            doing_point: GraphDoingPoint::Zero,
            lifecycle: GraphLifecycle::New,
        }
    }
}

impl GraphState {
    pub fn lifecycle(&self) -> GraphLifecycle {
        self.lifecycle
    }

    /// Corresponds to `graph_ct`: clear the source frame counter and mark CT.
    pub fn begin(&mut self) -> Result<(), GraphLifecycleError> {
        match self.lifecycle {
            GraphLifecycle::New => {
                self.frame_counter = 0;
                self.doing_point = GraphDoingPoint::Ct;
                self.lifecycle = GraphLifecycle::Running;
                Ok(())
            }
            GraphLifecycle::Running => Err(GraphLifecycleError::BeginWhileRunning),
            GraphLifecycle::Processing => Err(GraphLifecycleError::BeginWhileRunning),
            GraphLifecycle::CleanedUp => Err(GraphLifecycleError::BeginAfterCleanup),
        }
    }

    /// Mark one source graph execution frame as entered.
    pub fn proc(&mut self) -> Result<(), GraphLifecycleError> {
        match self.lifecycle {
            GraphLifecycle::New => Err(GraphLifecycleError::ProcessBeforeBegin),
            GraphLifecycle::Running => {
                self.doing_point = GraphDoingPoint::GameMain;
                self.lifecycle = GraphLifecycle::Processing;
                Ok(())
            }
            GraphLifecycle::Processing => Err(GraphLifecycleError::ProcessWhileProcessing),
            GraphLifecycle::CleanedUp => Err(GraphLifecycleError::ProcessAfterCleanup),
        }
    }

    /// Finish the source frame and optionally commit its display/task path.
    ///
    /// `display_completed` stands for the source branch where
    /// `graph_draw_finish` succeeds and `graph_task_set00` completes. It does
    /// not submit or convert any display list; it only preserves the source
    /// ordering and counter boundary. The false branch represents a frame
    /// whose display work was disabled or skipped.
    pub fn finish_frame(&mut self, display_completed: bool) -> Result<(), GraphLifecycleError> {
        match self.lifecycle {
            GraphLifecycle::New | GraphLifecycle::Running => {
                Err(GraphLifecycleError::FinishBeforeProcess)
            }
            GraphLifecycle::Processing => {
                if display_completed {
                    self.doing_point = GraphDoingPoint::GameMainFinished;
                    self.doing_point = GraphDoingPoint::TaskSet;
                    self.doing_point = GraphDoingPoint::WaitTask;
                    self.doing_point = GraphDoingPoint::WaitTaskFinished;
                    self.doing_point = GraphDoingPoint::TaskSetFinished;
                    self.frame_counter = self.frame_counter.wrapping_add(1);
                } else {
                    self.doing_point = GraphDoingPoint::GameMainFinished;
                }
                self.lifecycle = GraphLifecycle::Running;
                Ok(())
            }
            GraphLifecycle::CleanedUp => Err(GraphLifecycleError::FinishAfterCleanup),
        }
    }

    /// Corresponds to `graph_dt`: mark DT and enter the cleaned-up state.
    pub fn end(&mut self) -> Result<(), GraphLifecycleError> {
        match self.lifecycle {
            GraphLifecycle::New => Err(GraphLifecycleError::EndBeforeBegin),
            GraphLifecycle::Running => {
                self.doing_point = GraphDoingPoint::Dt;
                self.lifecycle = GraphLifecycle::CleanedUp;
                Ok(())
            }
            GraphLifecycle::Processing => Err(GraphLifecycleError::EndWhileProcessing),
            GraphLifecycle::CleanedUp => Err(GraphLifecycleError::EndAlreadyComplete),
        }
    }

    /// Explicitly return to the pre-`graph_ct` state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Deterministic graph clock plus the source-observable lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Graph {
    pub clock: GraphClock,
    pub state: GraphState,
}

impl Graph {
    /// Begin a graph lifetime and reset its source construction fields.
    pub fn begin(&mut self) -> Result<(), GraphLifecycleError> {
        self.state.begin()?;
        self.clock.reset();
        Ok(())
    }

    /// Apply one graph execution delta, then commit the source frame state.
    pub fn proc(&mut self, delta_seconds: f64) -> Result<(), GraphLifecycleError> {
        if self.state.lifecycle() != GraphLifecycle::Running {
            return self.state.proc();
        }
        self.clock.advance(delta_seconds);
        self.state.proc()
    }

    pub fn finish_frame(&mut self, display_completed: bool) -> Result<(), GraphLifecycleError> {
        self.state.finish_frame(display_completed)
    }

    /// End the graph lifetime and perform the source cleanup transition.
    pub fn end(&mut self) -> Result<(), GraphLifecycleError> {
        self.state.end()
    }

    /// Reset clock and lifecycle state for a new construction boundary.
    pub fn reset(&mut self) {
        self.clock.reset();
        self.state.reset();
    }
}

/// Coarse lifecycle phase for the executor hooks in `src/executor.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorLifecycle {
    New,
    Running,
    Exited,
}

/// Errors for repeated or out-of-order executor hook calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorLifecycleError {
    PrologAlreadyCalled,
    EpilogBeforeProlog,
    EpilogAlreadyCalled,
}

impl fmt::Display for ExecutorLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::PrologAlreadyCalled => "cannot call executor prolog twice",
            Self::EpilogBeforeProlog => "cannot call executor epilog before prolog",
            Self::EpilogAlreadyCalled => "cannot call executor epilog twice",
        };
        formatter.write_str(message)
    }
}

impl Error for ExecutorLifecycleError {}

/// Lifecycle-only representation of `_prolog` and `_epilog`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutorState {
    /// Corresponds to the C `construct_skip` global.
    pub construct_skip: bool,
    pub constructors_called: bool,
    pub main_called: bool,
    pub destructors_called: bool,
    lifecycle: ExecutorLifecycle,
}

impl ExecutorState {
    pub fn new(construct_skip: bool) -> Self {
        Self {
            construct_skip,
            constructors_called: false,
            main_called: false,
            destructors_called: false,
            lifecycle: ExecutorLifecycle::New,
        }
    }

    pub fn lifecycle(&self) -> ExecutorLifecycle {
        self.lifecycle
    }

    /// Record `_prolog`'s constructor-then-main ordering.
    pub fn prolog(&mut self) -> Result<(), ExecutorLifecycleError> {
        if self.lifecycle != ExecutorLifecycle::New {
            return Err(ExecutorLifecycleError::PrologAlreadyCalled);
        }
        self.constructors_called = !self.construct_skip;
        self.main_called = true;
        self.lifecycle = ExecutorLifecycle::Running;
        Ok(())
    }

    /// Record `_epilog`'s destructor phase.
    pub fn epilog(&mut self) -> Result<(), ExecutorLifecycleError> {
        match self.lifecycle {
            ExecutorLifecycle::New => Err(ExecutorLifecycleError::EpilogBeforeProlog),
            ExecutorLifecycle::Running => {
                self.destructors_called = true;
                self.lifecycle = ExecutorLifecycle::Exited;
                Ok(())
            }
            ExecutorLifecycle::Exited => Err(ExecutorLifecycleError::EpilogAlreadyCalled),
        }
    }
}
