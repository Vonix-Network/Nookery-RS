use crate::adapters::{AudioAdapter, PlatformAdapter, RendererAdapter, host_error};
use crate::error::{CallbackKind, Operation, RuntimeError};
use crate::schedule::{AudioSchedule, Timing, period_elapsed};
use crate::{GAME_FRAME, GAME_HYRAL_SIZE};

/// Runtime lifecycle phases. Failed is terminal until the owning process
/// discards the runtime, preventing partial boot from being reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Cold,
    Booting,
    Booted,
    Frame,
    Resetting,
    ShuttingDown,
    Shutdown,
    Failed,
}

/// Small checked state machine kept public for integration tests and hosts
/// that need to mirror lifecycle state without owning adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleState {
    phase: Lifecycle,
}

impl LifecycleState {
    pub const fn new() -> Self {
        Self {
            phase: Lifecycle::Cold,
        }
    }

    pub const fn phase(self) -> Lifecycle {
        self.phase
    }

    pub fn boot(&mut self) -> Result<(), RuntimeError> {
        self.expect(Lifecycle::Cold)?;
        self.phase = Lifecycle::Booted;
        Ok(())
    }

    pub fn begin_frame(&mut self) -> Result<(), RuntimeError> {
        self.expect(Lifecycle::Booted)?;
        self.phase = Lifecycle::Frame;
        Ok(())
    }

    pub fn finish_frame(&mut self) -> Result<(), RuntimeError> {
        self.expect(Lifecycle::Frame)?;
        self.phase = Lifecycle::Booted;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), RuntimeError> {
        match self.phase {
            Lifecycle::Booted
            | Lifecycle::Frame
            | Lifecycle::Resetting
            | Lifecycle::ShuttingDown => {
                self.phase = Lifecycle::Shutdown;
                Ok(())
            }
            actual => Err(RuntimeError::invalid_state(Lifecycle::Booted, actual)),
        }
    }

    fn expect(&self, expected: Lifecycle) -> Result<(), RuntimeError> {
        if self.phase == expected {
            Ok(())
        } else {
            Err(RuntimeError::invalid_state(expected, self.phase))
        }
    }
}

impl Default for LifecycleState {
    fn default() -> Self {
        Self::new()
    }
}

/// Reset status values from irqmgr.h.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetStatus {
    Normal,
    PreNmi,
    Delay,
}

/// Game transition represented by the C GAME_GOTO_NEXT contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameTransition {
    Prenmi,
}

/// State owned by the game boundary; no game data or social state is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    pub frame_counter: u32,
    pub game_frame: u8,
    pub disable_display: bool,
    pub disable_prenmi: bool,
    pub doing: bool,
    pub next_transition: Option<GameTransition>,
    pub hyral_size: usize,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            frame_counter: 0,
            game_frame: GAME_FRAME,
            disable_display: false,
            disable_prenmi: false,
            doing: true,
            next_transition: None,
            hyral_size: GAME_HYRAL_SIZE,
        }
    }
}

/// Graph state corresponding to the modeled fields in GRAPH.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphState {
    pub frame_counter: u64,
    pub dt_seconds: f64,
    pub dt_num_60fps_frames: f64,
    pub dt_total_60fps_frames: f64,
    pub cfb_bank: u8,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            frame_counter: 0,
            dt_seconds: 1.0 / 60.0,
            dt_num_60fps_frames: 1.0,
            dt_total_60fps_frames: 0.0,
            cfb_bank: 0,
        }
    }
}

/// Host-independent runtime tuning. The default is the PC single-thread path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeConfig {
    pub speed_multiplier: f64,
    pub audio_enabled: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            speed_multiplier: 1.0,
            audio_enabled: true,
        }
    }
}

/// Immutable view passed to callbacks and typed adapters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameContext {
    pub frame_index: u64,
    pub dt_seconds: f64,
    pub dt_60fps_frames: f64,
    pub total_60fps_frames: f64,
}

/// Result of one public run_frame or tick call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameReport {
    pub audio_ticks: u32,
    pub display_submitted: bool,
    pub reset_triggered: bool,
    pub stopped: bool,
}

/// Public callback registry. Every callback is reached from boot,
/// run_frame/tick, resize_game_memory, or shutdown.
pub type GameCallback = Box<dyn FnMut(&mut GameState) -> Result<(), RuntimeError>>;
pub type FrameCallback = Box<dyn FnMut(&mut GameState, &FrameContext) -> Result<(), RuntimeError>>;
pub type ResizeCallback = Box<dyn FnMut(&mut GameState, i64, usize) -> Result<(), RuntimeError>>;

#[derive(Default)]
pub struct CallbackRegistry {
    pub game_init: Option<GameCallback>,
    pub input: Option<GameCallback>,
    pub update: Option<FrameCallback>,
    pub execute: Option<FrameCallback>,
    pub bgm: Option<FrameCallback>,
    pub game_cleanup: Option<GameCallback>,
    pub reset: Option<GameCallback>,
    pub prenmi: Option<GameCallback>,
    pub resize: Option<ResizeCallback>,
}

impl CallbackRegistry {
    pub fn register_input<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState) -> Result<(), RuntimeError> + 'static,
    {
        self.input = Some(Box::new(callback));
    }

    pub fn register_execute<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState, &FrameContext) -> Result<(), RuntimeError> + 'static,
    {
        self.execute = Some(Box::new(callback));
    }

    pub fn register_bgm<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState, &FrameContext) -> Result<(), RuntimeError> + 'static,
    {
        self.bgm = Some(Box::new(callback));
    }

    pub fn register_update<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState, &FrameContext) -> Result<(), RuntimeError> + 'static,
    {
        self.update = Some(Box::new(callback));
    }

    /// Alias for hosts naming the C mTM_time seam directly.
    pub fn register_time<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState, &FrameContext) -> Result<(), RuntimeError> + 'static,
    {
        self.register_update(callback);
    }

    pub fn register_game_init<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState) -> Result<(), RuntimeError> + 'static,
    {
        self.game_init = Some(Box::new(callback));
    }

    pub fn register_game_cleanup<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState) -> Result<(), RuntimeError> + 'static,
    {
        self.game_cleanup = Some(Box::new(callback));
    }

    pub fn register_reset<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState) -> Result<(), RuntimeError> + 'static,
    {
        self.reset = Some(Box::new(callback));
    }

    pub fn register_prenmi<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState) -> Result<(), RuntimeError> + 'static,
    {
        self.prenmi = Some(Box::new(callback));
    }

    pub fn register_resize<F>(&mut self, callback: F)
    where
        F: FnMut(&mut GameState, i64, usize) -> Result<(), RuntimeError> + 'static,
    {
        self.resize = Some(Box::new(callback));
    }
}

/// Small read-only runtime snapshot for host integration and focused tests.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSnapshot {
    pub lifecycle: Lifecycle,
    pub graph: GraphState,
    pub game: GameState,
    pub reset_status: ResetStatus,
    pub audio_accumulator: f64,
}

/// Deterministic runtime boundary. Adapter ownership is explicit and there is
/// no hidden global mutable state, matching the PC single-thread route.
pub struct Runtime<P, R, A>
where
    P: PlatformAdapter,
    R: RendererAdapter,
    A: AudioAdapter,
{
    platform: P,
    renderer: R,
    audio: A,
    callbacks: CallbackRegistry,
    lifecycle: LifecycleState,
    config: RuntimeConfig,
    graph: GraphState,
    game: GameState,
    audio_schedule: AudioSchedule,
    reset_status: ResetStatus,
    soft_reset_enabled: bool,
    last_time_seconds: Option<f64>,
}

impl<P, R, A> Runtime<P, R, A>
where
    P: PlatformAdapter,
    R: RendererAdapter,
    A: AudioAdapter,
{
    pub fn new(platform: P, renderer: R, audio: A) -> Self {
        Self::with_config(platform, renderer, audio, RuntimeConfig::default())
    }

    pub fn with_config(platform: P, renderer: R, audio: A, config: RuntimeConfig) -> Self {
        Self {
            platform,
            renderer,
            audio,
            callbacks: CallbackRegistry::default(),
            lifecycle: LifecycleState::new(),
            config,
            graph: GraphState::default(),
            game: GameState::default(),
            audio_schedule: AudioSchedule::new(),
            reset_status: ResetStatus::Normal,
            soft_reset_enabled: false,
            last_time_seconds: None,
        }
    }

    pub fn callbacks_mut(&mut self) -> &mut CallbackRegistry {
        &mut self.callbacks
    }

    pub fn set_soft_reset_enabled(&mut self, enabled: bool) {
        self.soft_reset_enabled = enabled;
    }

    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle.phase()
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            lifecycle: self.lifecycle(),
            graph: self.graph.clone(),
            game: self.game.clone(),
            reset_status: self.reset_status,
            audio_accumulator: self.audio_schedule.accumulator(),
        }
    }

    /// Execute the ordered mainproc/PC boot boundary and initial game ctor.
    pub fn boot(&mut self) -> Result<(), RuntimeError> {
        self.lifecycle.booting()?;
        let result = self.boot_inner();
        if result.is_err() {
            self.lifecycle.fail();
        }
        result
    }

    fn boot_inner(&mut self) -> Result<(), RuntimeError> {
        self.call_host(Operation::SetScreenSize, |p| {
            p.set_screen_size(crate::DEFAULT_SCREEN_WIDTH, crate::DEFAULT_SCREEN_HEIGHT)
        })?;
        self.call_host(Operation::InitialFrame, |p| p.frame_begin())?;
        self.call_host(Operation::InitialFrame, |p| p.frame_end())?;
        self.call_host(Operation::InitializeCard, |p| p.initialize_card())?;
        self.call_host(Operation::CreateIrqManager, |p| p.create_irq_manager())?;
        self.call_host(Operation::CreateInputManager, |p| p.create_input_manager())?;
        self.call_host(Operation::RuntimeFrame, |p| p.frame_begin())?;
        self.call_host(Operation::RuntimeFrame, |p| p.frame_end())?;
        self.call_host(Operation::InitializeDisplay, |p| p.initialize_display())?;
        self.call_host(Operation::InitializeMessages, |p| p.initialize_messages())?;
        self.call_host(Operation::EnableLandStart, |p| p.enable_land_start())?;
        self.call_host(Operation::MountArchive, |p| p.mount_archive())?;
        self.call_host(Operation::DumpAramHeap, |p| p.dump_aram_heap())?;

        self.graph = GraphState::default();
        if let Some(callback) = self.callbacks.game_init.as_mut() {
            callback(&mut self.game).map_err(|_| RuntimeError::Callback(CallbackKind::GameInit))?;
        }
        self.call_audio(Operation::AudioInit, |a| a.initialize())?;
        self.last_time_seconds = Some(
            self.platform
                .monotonic_seconds()
                .map_err(|code| host_error(Operation::ReadClock, code))?,
        );
        self.lifecycle.booted();
        Ok(())
    }

    /// Read host time and reach the same public frame path as run_frame.
    pub fn tick(&mut self) -> Result<FrameReport, RuntimeError> {
        self.require(Lifecycle::Booted)?;
        let now = self
            .platform
            .monotonic_seconds()
            .map_err(|code| host_error(Operation::ReadClock, code))?;
        let previous = self.last_time_seconds.replace(now).unwrap_or(now);
        let delta = now - previous;
        self.run_frame(delta)
    }

    /// Run one deterministic frame. This is the reachable host/test path for
    /// input, game update/exec/BGM, rendering, audio, and reset handling.
    pub fn run_frame(&mut self, raw_delta_seconds: f64) -> Result<FrameReport, RuntimeError> {
        self.require(Lifecycle::Booted)?;
        let timing = Timing::from_delta(
            raw_delta_seconds,
            self.graph.dt_total_60fps_frames,
            self.config.speed_multiplier,
        )
        .ok_or(RuntimeError::InvalidInput)?;
        self.lifecycle.begin_frame()?;
        let result = self.run_frame_inner(timing);
        if result.is_err() {
            self.lifecycle.fail();
        } else {
            self.lifecycle.finish_frame()?;
        }
        result
    }

    fn run_frame_inner(&mut self, timing: Timing) -> Result<FrameReport, RuntimeError> {
        self.graph.dt_seconds = timing.delta_seconds;
        self.graph.dt_num_60fps_frames = timing.delta_60fps_frames;
        self.graph.dt_total_60fps_frames = timing.total_60fps_frames;
        let context = FrameContext {
            frame_index: self.game.frame_counter as u64,
            dt_seconds: timing.delta_seconds,
            dt_60fps_frames: timing.delta_60fps_frames,
            total_60fps_frames: timing.total_60fps_frames,
        };

        self.call_renderer(Operation::PrepareFrame, |r| r.prepare_frame(&context))?;
        self.call_callback(CallbackKind::Input, |callbacks, game| {
            callbacks.input.as_mut().map(|callback| callback(game))
        })?;
        self.call_renderer(Operation::DrawFirst, |r| r.draw_first(&context))?;
        self.call_callback(CallbackKind::Update, |callbacks, game| {
            callbacks
                .update
                .as_mut()
                .map(|callback| callback(game, &context))
        })?;
        self.call_callback(CallbackKind::Execute, |callbacks, game| {
            callbacks
                .execute
                .as_mut()
                .map(|callback| callback(game, &context))
        })?;
        self.call_callback(CallbackKind::Bgm, |callbacks, game| {
            callbacks
                .bgm
                .as_mut()
                .map(|callback| callback(game, &context))
        })?;
        self.call_audio(Operation::Bgm, |a| a.bgm_update())?;
        self.game.frame_counter = self.game.frame_counter.saturating_add(1);

        let mut display_submitted = false;
        if !self.game.disable_display && self.reset_status != ResetStatus::Delay {
            self.call_renderer(Operation::FinishFrame, |r| r.finish_frame(&context))?;
            self.call_renderer(Operation::SubmitFrame, |r| r.submit_frame(&context))?;
            self.graph.frame_counter = self.graph.frame_counter.saturating_add(1);
            display_submitted = true;
        }

        let audio_ticks = if self.config.audio_enabled && self.reset_status != ResetStatus::Delay {
            let ticks = self.audio_schedule.ticks(timing.delta_60fps_frames);
            for _ in 0..ticks {
                self.call_audio(Operation::AudioTick, |a| a.game_tick())?;
            }
            ticks
        } else {
            0
        };

        let reset_triggered = self.check_reset()?;
        Ok(FrameReport {
            audio_ticks,
            display_submitted,
            reset_triggered,
            stopped: !self.game.doing,
        })
    }

    fn check_reset(&mut self) -> Result<bool, RuntimeError> {
        if !self.soft_reset_enabled
            || !self
                .platform
                .reset_requested()
                .map_err(|code| host_error(Operation::Reset, code))?
        {
            return Ok(false);
        }
        self.reset_status = ResetStatus::PreNmi;
        self.call_audio(Operation::Reset, |a| a.reset())?;
        self.call_callback(CallbackKind::Reset, |callbacks, game| {
            callbacks.reset.as_mut().map(|callback| callback(game))
        })?;
        if !self.game.disable_prenmi {
            self.call_callback(CallbackKind::Prenmi, |callbacks, game| {
                callbacks.prenmi.as_mut().map(|callback| callback(game))
            })?;
            self.game.doing = false;
            self.game.next_transition = Some(GameTransition::Prenmi);
        }
        Ok(true)
    }

    /// Reach the C game_resize_hyral allocation boundary without inventing an
    /// allocator or game memory. The host returns the actual resulting size.
    pub fn resize_game_memory(&mut self, requested_size: i64) -> Result<usize, RuntimeError> {
        self.require(Lifecycle::Booted)?;
        let actual = self
            .platform
            .resize_game_memory(requested_size)
            .map_err(|code| host_error(Operation::ResizeGameMemory, code))?;
        self.game.hyral_size = actual;
        if let Some(callback) = self.callbacks.resize.as_mut() {
            callback(&mut self.game, requested_size, actual)
                .map_err(|_| RuntimeError::Callback(CallbackKind::Resize))?;
        }
        Ok(actual)
    }

    /// Perform C game teardown followed by graph/host shutdown.
    pub fn shutdown(&mut self) -> Result<(), RuntimeError> {
        self.require(Lifecycle::Booted)?;
        self.lifecycle.shutting_down();
        let result = self.shutdown_inner();
        if result.is_err() {
            self.lifecycle.fail();
        } else {
            self.lifecycle.shutdown()?;
        }
        result
    }

    fn shutdown_inner(&mut self) -> Result<(), RuntimeError> {
        self.call_audio(Operation::AudioCleanup, |a| a.cleanup())?;
        if let Some(callback) = self.callbacks.game_cleanup.as_mut() {
            callback(&mut self.game)
                .map_err(|_| RuntimeError::Callback(CallbackKind::GameCleanup))?;
        }
        self.call_audio(Operation::AudioCleanup, |a| a.shutdown())?;
        self.call_renderer(Operation::RendererShutdown, |r| r.shutdown())?;
        self.call_host(Operation::PlatformShutdown, |p| p.shutdown())?;
        Ok(())
    }

    fn require(&self, expected: Lifecycle) -> Result<(), RuntimeError> {
        if self.lifecycle() == expected {
            Ok(())
        } else {
            Err(RuntimeError::invalid_state(expected, self.lifecycle()))
        }
    }

    fn call_host<F>(&mut self, operation: Operation, call: F) -> Result<(), RuntimeError>
    where
        F: FnOnce(&mut P) -> Result<(), crate::HostErrorCode>,
    {
        call(&mut self.platform).map_err(|code| host_error(operation, code))
    }

    fn call_renderer<F>(&mut self, operation: Operation, call: F) -> Result<(), RuntimeError>
    where
        F: FnOnce(&mut R) -> Result<(), crate::HostErrorCode>,
    {
        call(&mut self.renderer).map_err(|code| host_error(operation, code))
    }

    fn call_audio<F>(&mut self, operation: Operation, call: F) -> Result<(), RuntimeError>
    where
        F: FnOnce(&mut A) -> Result<(), crate::HostErrorCode>,
    {
        call(&mut self.audio).map_err(|code| host_error(operation, code))
    }

    fn call_callback<F>(&mut self, kind: CallbackKind, call: F) -> Result<(), RuntimeError>
    where
        F: FnOnce(&mut CallbackRegistry, &mut GameState) -> Option<Result<(), RuntimeError>>,
    {
        if let Some(result) = call(&mut self.callbacks, &mut self.game) {
            result.map_err(|_| RuntimeError::Callback(kind))?;
        }
        Ok(())
    }

    /// Test/integration helper for period-based game logic sites modeled by
    /// graph_dt_period_elapsed.
    pub fn period_elapsed(&self, accumulator: &mut f64, period_frames: f64) -> bool {
        period_elapsed(accumulator, self.graph.dt_num_60fps_frames, period_frames)
    }
}

impl LifecycleState {
    fn booting(&mut self) -> Result<(), RuntimeError> {
        if self.phase() != Lifecycle::Cold {
            return Err(RuntimeError::invalid_state(Lifecycle::Cold, self.phase()));
        }
        self.phase = Lifecycle::Booting;
        Ok(())
    }

    fn booted(&mut self) {
        self.phase = Lifecycle::Booted;
    }

    fn shutting_down(&mut self) {
        self.phase = Lifecycle::ShuttingDown;
    }

    fn fail(&mut self) {
        self.phase = Lifecycle::Failed;
    }
}
