//! Deterministic Rust model of the C boot/entry ordering contract.
//!
//! This crate is intentionally a vertical slice. It owns ordering and failure
//! boundaries, while platform work remains behind typed callbacks. In
//! particular, display, audio, and disc services are not emulated here.
//!
//! The C-boundary gaps are deliberate: `JW_*` display setup, `Na_*` audio,
//! DVD/COPYDATE/string-table/module loading, OS threads/message queues, and
//! `graph_proc` remain callback-owned. On `TARGET_PC`, C skips graph-thread
//! creation/start and the IRQ message loop, calls `graph_proc` directly, skips
//! the REL load/unlink, and suppresses COPYDATE's fatal error; those target
//! decisions are not guessed by this crate.

use std::fmt;

/// Ordered boot phases observed in `src/static/boot.c`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BootPhase {
    /// `ReconfigBATs`, the re-entry guard, clock capture, and OS primitives.
    OsFoundation,
    /// Reset-code/NMI handling and common OS initialization.
    ResetAndNmi,
    /// Console-mode checks and boot-mode setup.
    ConsoleMode,
    /// Arena sizing/clearing and the JSystem foundation.
    MemoryAndJsystem,
    /// Fault clients, debug console, and exception map setup.
    FaultAndDebug,
    /// `sound_initial`, `initial_menu_init`, `dvderr_init`, `sound_initial2`.
    AudioAndInitialMenu,
    /// COPYDATE and `/static.str` loading.
    BootResources,
    /// REL/static module setup, heap group change, and PC data conversion.
    GameModule,
}

impl BootPhase {
    pub const SOURCE_ORDER: [Self; 8] = [
        Self::OsFoundation,
        Self::ResetAndNmi,
        Self::ConsoleMode,
        Self::MemoryAndJsystem,
        Self::FaultAndDebug,
        Self::AudioAndInitialMenu,
        Self::BootResources,
        Self::GameModule,
    ];
}

/// Typed platform operations that remain at the C boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DisplayHook {
    BeginFrame,
    EndFrame,
    Init,
    Init3,
    Init2,
    Cleanup,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AudioHook {
    Initial,
    Initial2,
    MessageAramInit,
    GameFrame,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiscHook {
    CardInit,
    MountArchive,
    CopyDate,
    StringTable,
    GameModule,
}

/// Optional typed hook surface for a callback registry implementation.
/// Implementations decide whether a target-specific operation is completed or
/// skipped; this crate supplies no display, audio, disc, or OS behavior.
pub trait PlatformHooks {
    fn display(&mut self, hook: DisplayHook) -> Result<(), String>;
    fn audio(&mut self, hook: AudioHook) -> Result<(), String>;
    fn disc(&mut self, hook: DiscHook) -> Result<(), String>;
}

/// The terminal action after the hot-start loop exits.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CleanupPhase {
    /// Unlink the loaded module and release the string table.
    ReleaseBootResources,
    /// `JW_Cleanup` and the boot-loader return.
    Shutdown,
}

/// A callback result that preserves the C distinction between continuing and
/// entering terminal cleanup after a failed phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootFailure {
    pub phase: BootPhase,
    pub reason: String,
}

impl BootFailure {
    pub fn new(phase: BootPhase, reason: impl Into<String>) -> Self {
        Self {
            phase,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for BootFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "boot phase {:?} failed: {}", self.phase, self.reason)
    }
}

impl std::error::Error for BootFailure {}

/// Callbacks for the C-owned platform operations.
pub trait BootCallbacks {
    /// Run one source-ordered boot phase.
    fn boot_phase(&mut self, phase: BootPhase) -> Result<(), String>;

    /// Release a loaded REL/static module, if one exists.
    fn unlink_module(&mut self) -> Result<(), String>;

    /// Release `/static.str`, if it was loaded.
    fn free_string_table(&mut self) -> Result<(), String>;

    /// Perform `JW_Cleanup`.
    fn shutdown(&mut self) -> Result<(), String>;

    /// `padmgr_Init(NULL)` from `entry`.
    fn entry_pad_init(&mut self) -> Result<(), String>;

    /// `new_Debug_mode()` from `entry`.
    fn entry_debug_mode(&mut self) -> Result<(), String>;

    /// `SETREG(SREG, 0, 0)` from `entry`.
    fn entry_reset_register(&mut self) -> Result<(), String>;

    /// The `mainproc` setup and its first frame boundary.
    fn mainproc(&mut self) -> Result<(), String>;

    /// One graph/game frame. This is the PC direct-call boundary in C.
    fn frame(&mut self) -> Result<(), String>;
}

/// A phase plan, kept separate from callback storage so callers can inspect
/// and test the contract without hidden globals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootPlan {
    phases: Vec<BootPhase>,
}

impl Default for BootPlan {
    fn default() -> Self {
        Self::source_order()
    }
}

impl BootPlan {
    pub fn source_order() -> Self {
        Self {
            phases: BootPhase::SOURCE_ORDER.to_vec(),
        }
    }

    pub fn phases(&self) -> &[BootPhase] {
        &self.phases
    }

    /// Execute all phases exactly once. A failure stops phase execution and
    /// immediately enters terminal cleanup, matching a fatal boot boundary.
    pub fn run<C: BootCallbacks>(&self, callbacks: &mut C) -> Result<BootOutcome, BootError> {
        for &phase in &self.phases {
            if let Err(reason) = callbacks.boot_phase(phase) {
                let failure = BootFailure::new(phase, reason);
                let cleanup = cleanup(callbacks);
                return Err(BootError::Failed { failure, cleanup });
            }
        }

        Ok(BootOutcome::ReadyForEntryLoop)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootOutcome {
    ReadyForEntryLoop,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CleanupError {
    UnlinkModule(String),
    FreeStringTable(String),
    Shutdown(String),
}

#[derive(Debug, Eq, PartialEq)]
pub enum BootError {
    Failed {
        failure: BootFailure,
        cleanup: Result<(), CleanupError>,
    },
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed { failure, cleanup } => write!(
                f,
                "{failure}; terminal cleanup {}",
                if cleanup.is_ok() {
                    "completed"
                } else {
                    "failed"
                }
            ),
        }
    }
}

impl std::error::Error for BootError {}

fn cleanup<C: BootCallbacks>(callbacks: &mut C) -> Result<(), CleanupError> {
    let mut first_error = None;
    if let Err(error) = callbacks.unlink_module() {
        first_error = Some(CleanupError::UnlinkModule(error));
    }
    if let Err(error) = callbacks.free_string_table() {
        if first_error.is_none() {
            first_error = Some(CleanupError::FreeStringTable(error));
        }
    }
    if let Err(error) = callbacks.shutdown() {
        if first_error.is_none() {
            first_error = Some(CleanupError::Shutdown(error));
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Events emitted by [`EntryLoop`] at the same boundaries as the C entry path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryEvent {
    Boot,
    Entry,
    Mainproc,
    Frame,
    Reset,
    Shutdown,
}

/// Explicit state machine for the boot loader's hot-start loop.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum EntryState {
    #[default]
    NotStarted,
    Ready,
    Running,
    Resetting,
    Shutdown,
    Failed,
}

#[derive(Debug, Eq, PartialEq)]
pub enum EntryError {
    Boot(BootError),
    Callback {
        event: EntryEvent,
        reason: String,
    },
    InvalidState {
        event: EntryEvent,
        state: EntryState,
    },
}

impl fmt::Display for EntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boot(error) => error.fmt(f),
            Self::Callback { event, reason } => write!(f, "{event:?} callback failed: {reason}"),
            Self::InvalidState { event, state } => {
                write!(f, "cannot dispatch {event:?} while in {state:?}")
            }
        }
    }
}

impl std::error::Error for EntryError {}

/// Drives boot once, then dispatches entry/frame/reset/shutdown boundaries.
pub struct EntryLoop<C> {
    callbacks: C,
    plan: BootPlan,
    state: EntryState,
    events: Vec<EntryEvent>,
}

impl<C> EntryLoop<C> {
    pub fn new(callbacks: C) -> Self {
        Self::with_plan(callbacks, BootPlan::source_order())
    }

    pub fn with_plan(callbacks: C, plan: BootPlan) -> Self {
        Self {
            callbacks,
            plan,
            state: EntryState::NotStarted,
            events: Vec::new(),
        }
    }

    pub fn state(&self) -> &EntryState {
        &self.state
    }

    pub fn events(&self) -> &[EntryEvent] {
        &self.events
    }

    pub fn callbacks(&self) -> &C {
        &self.callbacks
    }

    pub fn callbacks_mut(&mut self) -> &mut C {
        &mut self.callbacks
    }
}

impl<C: BootCallbacks> EntryLoop<C> {
    /// Run the one-time C boot `main`. A failed boot is terminal and cannot
    /// reach entry, frames, reset, or shutdown a second time.
    pub fn boot(&mut self) -> Result<(), EntryError> {
        if self.state != EntryState::NotStarted {
            return Err(EntryError::InvalidState {
                event: EntryEvent::Boot,
                state: self.state.clone(),
            });
        }
        self.events.push(EntryEvent::Boot);
        match self.plan.run(&mut self.callbacks) {
            Ok(_) => {
                self.state = EntryState::Ready;
                Ok(())
            }
            Err(error) => {
                self.state = EntryState::Failed;
                Err(EntryError::Boot(error))
            }
        }
    }

    /// Dispatch one hot-start callback: `entry` → `mainproc` → one frame.
    pub fn dispatch_frame(&mut self) -> Result<(), EntryError> {
        if self.state != EntryState::Ready && self.state != EntryState::Running {
            return Err(EntryError::InvalidState {
                event: EntryEvent::Frame,
                state: self.state.clone(),
            });
        }
        self.state = EntryState::Running;
        let result = self.dispatch_frame_inner();
        if result.is_err() {
            self.state = EntryState::Failed;
        }
        result
    }

    fn dispatch_frame_inner(&mut self) -> Result<(), EntryError> {
        self.events.push(EntryEvent::Entry);
        callback(&mut self.callbacks, EntryEvent::Entry, |c| {
            c.entry_pad_init()
        })?;
        callback(&mut self.callbacks, EntryEvent::Entry, |c| {
            c.entry_debug_mode()
        })?;
        self.events.push(EntryEvent::Reset);
        callback(&mut self.callbacks, EntryEvent::Reset, |c| {
            c.entry_reset_register()
        })?;
        self.events.push(EntryEvent::Mainproc);
        callback(&mut self.callbacks, EntryEvent::Mainproc, |c| c.mainproc())?;
        self.events.push(EntryEvent::Frame);
        callback(&mut self.callbacks, EntryEvent::Frame, |c| c.frame())
    }

    /// End the loop and perform boot-resource cleanup exactly once.
    pub fn shutdown(&mut self) -> Result<(), EntryError> {
        if self.state != EntryState::Ready && self.state != EntryState::Running {
            return Err(EntryError::InvalidState {
                event: EntryEvent::Shutdown,
                state: self.state.clone(),
            });
        }
        self.events.push(EntryEvent::Shutdown);
        cleanup(&mut self.callbacks).map_err(|error| EntryError::Callback {
            event: EntryEvent::Shutdown,
            reason: format!("{error:?}"),
        })?;
        self.state = EntryState::Shutdown;
        Ok(())
    }
}

fn callback<C, F>(callbacks: &mut C, event: EntryEvent, operation: F) -> Result<(), EntryError>
where
    F: FnOnce(&mut C) -> Result<(), String>,
{
    operation(callbacks).map_err(|reason| EntryError::Callback { event, reason })
}

// These hooks intentionally do not provide platform defaults. The real C
// boundary still owns display/audio/disc/OS behavior; callers must register
// callbacks for those operations instead of relying on hidden globals.

#[cfg(test)]
mod tests;
