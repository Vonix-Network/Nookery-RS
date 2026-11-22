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

use std::ffi::{c_char, c_void};
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

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

/// Maximum number of constructor/destructor callbacks accepted by the C ABI.
/// The fixed bound keeps the table layout stable and makes malformed counts
/// rejectable before any callback is entered.
pub const NOOKERY_BOOT_MAX_CALLBACKS: usize = 16;

/// Successful callback return value.
pub const NOOKERY_BOOT_OK: i32 = 0;
/// Reserved callback result. `construct_skip` itself is the table flag and is
/// never inferred from a callback result.
pub const NOOKERY_BOOT_SKIP: i32 = 1;
/// The callback table or its context is not usable.
pub const NOOKERY_BOOT_INVALID_ARGUMENT: i32 = -1;
/// A required callback is absent or a callback count exceeds the fixed bound.
pub const NOOKERY_BOOT_INVALID_TABLE: i32 = -2;
/// A callback returned a non-success value.
pub const NOOKERY_BOOT_CALLBACK_FAILED: i32 = -3;
/// The operation is not valid in the current repeated-call state.
pub const NOOKERY_BOOT_INVALID_STATE: i32 = -4;
/// A callback panicked; panics never cross this ABI.
pub const NOOKERY_BOOT_PANIC: i32 = -5;
/// No unresolved callback was registered for an unresolved-function report.
pub const NOOKERY_BOOT_UNRESOLVED_UNAVAILABLE: i32 = -6;
/// The report callback failed while reporting an operation failure.
pub const NOOKERY_BOOT_REPORT_FAILED: i32 = -7;
/// The source `_unresolved` report is bounded to sixteen frames.
pub const NOOKERY_BOOT_MAX_UNRESOLVED_FRAMES: usize = 16;

/// A callback invoked by the production C++ adapter.
pub type NookeryBootCallback = unsafe extern "C-unwind" fn(*mut c_void) -> i32;

/// Reports an ABI failure. The message is a borrowed, NUL-terminated string
/// valid only for the duration of this call. A null reporter is permitted;
/// the result code remains the authoritative failure boundary.
pub type NookeryBootReport = unsafe extern "C-unwind" fn(*mut c_void, i32, *const c_char) -> i32;

/// One bounded stack frame supplied by the host-side `_unresolved` adapter.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NookeryBootStackFrame {
    pub address: u32,
    pub back_chain: u32,
    pub link_register: u32,
}

/// Callback for the unresolved-function report boundary. Rust does not walk
/// a host stack: the C++ adapter supplies at most sixteen already-captured
/// frames from the platform stack API.
pub type NookeryBootUnresolvedCallback =
    unsafe extern "C-unwind" fn(*mut c_void, *const NookeryBootStackFrame, u32) -> i32;

/// Stable C ABI callback table for the boot constructor/main/destructor seam.
///
/// `construct_skip` is a data flag matching the C entry decision: when it is
/// non-zero, constructors are not called, while `main` and epilog cleanup
/// remain available. Slots are executed in ascending order for constructors
/// and descending order for destructors.
#[repr(C)]
pub struct NookeryBootCallbackTable {
    pub context: *mut c_void,
    pub construct_skip: u8,
    pub constructor_count: u32,
    pub constructors: [Option<NookeryBootCallback>; NOOKERY_BOOT_MAX_CALLBACKS],
    pub main: Option<NookeryBootCallback>,
    pub destructor_count: u32,
    pub destructors: [Option<NookeryBootCallback>; NOOKERY_BOOT_MAX_CALLBACKS],
    pub unresolved: Option<NookeryBootUnresolvedCallback>,
    pub report: Option<NookeryBootReport>,
    pub entry_pad_init: Option<NookeryBootCallback>,
    pub entry_debug_mode: Option<NookeryBootCallback>,
    pub entry_reset_register: Option<NookeryBootCallback>,
    pub entry_mainproc: Option<NookeryBootCallback>,
}

impl NookeryBootCallbackTable {
    /// A table with no callbacks registered. It is useful as a base for a C
    /// or C++ adapter that fills the fields explicitly.
    pub const fn empty(context: *mut c_void) -> Self {
        Self {
            context,
            construct_skip: 0,
            constructor_count: 0,
            constructors: [None; NOOKERY_BOOT_MAX_CALLBACKS],
            main: None,
            destructor_count: 0,
            destructors: [None; NOOKERY_BOOT_MAX_CALLBACKS],
            unresolved: None,
            report: None,
            entry_pad_init: None,
            entry_debug_mode: None,
            entry_reset_register: None,
            entry_mainproc: None,
        }
    }
}

impl fmt::Debug for NookeryBootCallbackTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NookeryBootCallbackTable")
            .field("context", &self.context)
            .field("construct_skip", &self.construct_skip)
            .field("constructor_count", &self.constructor_count)
            .field("main", &self.main.map(|_| "registered"))
            .field("destructor_count", &self.destructor_count)
            .field("unresolved", &self.unresolved.map(|_| "registered"))
            .field("report", &self.report.map(|_| "registered"))
            .field("entry_pad_init", &self.entry_pad_init.map(|_| "registered"))
            .field(
                "entry_debug_mode",
                &self.entry_debug_mode.map(|_| "registered"),
            )
            .field(
                "entry_reset_register",
                &self.entry_reset_register.map(|_| "registered"),
            )
            .field("entry_mainproc", &self.entry_mainproc.map(|_| "registered"))
            .finish()
    }
}

#[derive(Clone, Copy)]
struct RegisteredBootCallbacks {
    table: usize,
    lifecycle: AbiLifecycle,
    entry_called: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AbiLifecycle {
    Registered,
    Running,
    Exited,
    Failed,
}

static REGISTERED_CALLBACKS: std::sync::Mutex<Option<RegisteredBootCallbacks>> =
    std::sync::Mutex::new(None);

fn report(table: &NookeryBootCallbackTable, code: i32, message: &str) -> i32 {
    if table.context.is_null() {
        return NOOKERY_BOOT_INVALID_ARGUMENT;
    }
    let Some(reporter) = table.report else {
        return NOOKERY_BOOT_OK;
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut bytes = message
            .as_bytes()
            .iter()
            .copied()
            .filter(|byte| *byte != 0)
            .collect::<Vec<_>>();
        bytes.push(0);
        // SAFETY: `reporter` is supplied by the registered C adapter and
        // receives the same opaque context that was validated at registration.
        unsafe { reporter(table.context, code, bytes.as_ptr().cast()) }
    }));
    match result {
        Ok(NOOKERY_BOOT_OK) => NOOKERY_BOOT_OK,
        Ok(_) => NOOKERY_BOOT_REPORT_FAILED,
        Err(_) => NOOKERY_BOOT_PANIC,
    }
}

fn reported_error(table: &NookeryBootCallbackTable, code: i32, message: &str) -> i32 {
    let report_code = report(table, code, message);
    if report_code == NOOKERY_BOOT_OK {
        code
    } else {
        report_code
    }
}

fn validate_table(table: &NookeryBootCallbackTable) -> Result<(), i32> {
    if table.context.is_null() {
        return Err(NOOKERY_BOOT_INVALID_ARGUMENT);
    }
    if table.constructor_count as usize > NOOKERY_BOOT_MAX_CALLBACKS
        || table.destructor_count as usize > NOOKERY_BOOT_MAX_CALLBACKS
    {
        return Err(NOOKERY_BOOT_INVALID_TABLE);
    }
    if table.main.is_none() {
        return Err(NOOKERY_BOOT_INVALID_TABLE);
    }
    if table
        .constructors
        .iter()
        .take(table.constructor_count as usize)
        .any(Option::is_none)
        || table
            .destructors
            .iter()
            .take(table.destructor_count as usize)
            .any(Option::is_none)
    {
        return Err(NOOKERY_BOOT_INVALID_TABLE);
    }
    Ok(())
}

fn invoke(table: &NookeryBootCallbackTable, callback: NookeryBootCallback) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| unsafe { callback(table.context) })) {
        Ok(NOOKERY_BOOT_OK) => NOOKERY_BOOT_OK,
        Ok(
            code @ (NOOKERY_BOOT_INVALID_ARGUMENT
            | NOOKERY_BOOT_INVALID_TABLE
            | NOOKERY_BOOT_INVALID_STATE
            | NOOKERY_BOOT_CALLBACK_FAILED
            | NOOKERY_BOOT_UNRESOLVED_UNAVAILABLE),
        ) => code,
        Ok(_) => NOOKERY_BOOT_CALLBACK_FAILED,
        Err(_) => NOOKERY_BOOT_PANIC,
    }
}

fn ffi_guard<F>(operation: F) -> i32
where
    F: FnOnce() -> i32,
{
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or(NOOKERY_BOOT_PANIC)
}

fn with_registered_table<F>(operation: F) -> i32
where
    F: FnOnce(&NookeryBootCallbackTable) -> i32,
{
    let registered = match REGISTERED_CALLBACKS.lock() {
        Ok(guard) => *guard,
        Err(_) => return NOOKERY_BOOT_INVALID_STATE,
    };
    let Some(registered) = registered else {
        return NOOKERY_BOOT_INVALID_ARGUMENT;
    };
    // Registration only stores a pointer owned by the adapter. The adapter
    // must keep its table alive until it replaces the registration.
    let Some(table) = (unsafe { (registered.table as *const NookeryBootCallbackTable).as_ref() })
    else {
        return NOOKERY_BOOT_INVALID_ARGUMENT;
    };
    operation(table)
}

/// Register the callback table used by the exported prolog/epilog functions.
/// Registration replaces the prior table and resets repeated-call state.
///
/// # Safety
/// `table` must be null or point to a fully initialized table that remains
/// alive and unchanged until it is unregistered or replaced. Its context and
/// callback pointers must remain valid for every exported operation.
#[no_mangle]
pub unsafe extern "C" fn nookery_boot_register_callbacks(
    table: *const NookeryBootCallbackTable,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let Some(table_ref) = (unsafe { table.as_ref() }) else {
            return NOOKERY_BOOT_INVALID_ARGUMENT;
        };
        if let Err(code) = validate_table(table_ref) {
            return reported_error(table_ref, code, "invalid nookery boot callback table");
        }
        let Ok(mut registered) = REGISTERED_CALLBACKS.lock() else {
            return NOOKERY_BOOT_INVALID_STATE;
        };
        *registered = Some(RegisteredBootCallbacks {
            table: table as usize,
            lifecycle: AbiLifecycle::Registered,
            entry_called: false,
        });
        NOOKERY_BOOT_OK
    }));
    result.unwrap_or(NOOKERY_BOOT_PANIC)
}

/// Clear the registered table. This is also the explicit repeated-call reset
/// point for an adapter that has completed epilog and wants a fresh prolog.
#[no_mangle]
pub extern "C" fn nookery_boot_unregister_callbacks() -> i32 {
    match catch_unwind(AssertUnwindSafe(|| {
        let Ok(mut registered) = REGISTERED_CALLBACKS.lock() else {
            return NOOKERY_BOOT_INVALID_STATE;
        };
        *registered = None;
        NOOKERY_BOOT_OK
    })) {
        Ok(code) => code,
        Err(_) => NOOKERY_BOOT_PANIC,
    }
}

/// Run constructors in source order, unless `construct_skip` is set, then
/// invoke the registered main callback exactly once.
#[no_mangle]
pub extern "C" fn nookery_boot_prolog() -> i32 {
    ffi_guard(|| {
        with_registered_table(|table| {
            if let Err(code) = validate_table(table) {
                return reported_error(table, code, "invalid nookery boot callback table");
            }
            let state = match REGISTERED_CALLBACKS.lock() {
                Ok(mut registered) => {
                    let Some(state) = registered.as_mut() else {
                        return NOOKERY_BOOT_INVALID_ARGUMENT;
                    };
                    if state.lifecycle != AbiLifecycle::Registered {
                        return NOOKERY_BOOT_INVALID_STATE;
                    }
                    state.lifecycle = AbiLifecycle::Running;
                    state.lifecycle
                }
                Err(_) => return NOOKERY_BOOT_INVALID_STATE,
            };
            debug_assert_eq!(state, AbiLifecycle::Running);
            if table.construct_skip == 0 {
                for callback in table
                    .constructors
                    .iter()
                    .take(table.constructor_count as usize)
                    .flatten()
                {
                    let code = invoke(table, *callback);
                    if code != NOOKERY_BOOT_OK {
                        let code = reported_error(table, code, "constructor callback failed");
                        if let Ok(mut registered) = REGISTERED_CALLBACKS.lock() {
                            if let Some(state) = registered.as_mut() {
                                state.lifecycle = AbiLifecycle::Failed;
                            }
                        }
                        return code;
                    }
                }
            }
            let Some(main) = table.main else {
                if let Ok(mut registered) = REGISTERED_CALLBACKS.lock() {
                    if let Some(state) = registered.as_mut() {
                        state.lifecycle = AbiLifecycle::Failed;
                    }
                }
                return reported_error(table, NOOKERY_BOOT_INVALID_TABLE, "main callback missing");
            };
            let code = invoke(table, main);
            if code != NOOKERY_BOOT_OK {
                let code = reported_error(table, code, "main callback failed");
                if let Ok(mut registered) = REGISTERED_CALLBACKS.lock() {
                    if let Some(state) = registered.as_mut() {
                        state.lifecycle = AbiLifecycle::Failed;
                    }
                }
                return code;
            }
            code
        })
    })
}

/// Run destructors in reverse source order. A failure is reported and stops
/// the sequence; already-run destructors are not repeated by this call.
#[no_mangle]
pub extern "C" fn nookery_boot_epilog() -> i32 {
    ffi_guard(|| {
        with_registered_table(|table| {
            if let Err(code) = validate_table(table) {
                return reported_error(table, code, "invalid nookery boot callback table");
            }
            match REGISTERED_CALLBACKS.lock() {
                Ok(mut registered) => {
                    let Some(state) = registered.as_mut() else {
                        return NOOKERY_BOOT_INVALID_ARGUMENT;
                    };
                    if state.lifecycle != AbiLifecycle::Running {
                        return NOOKERY_BOOT_INVALID_STATE;
                    }
                    state.lifecycle = AbiLifecycle::Exited;
                }
                Err(_) => return NOOKERY_BOOT_INVALID_STATE,
            }
            for callback in table
                .destructors
                .iter()
                .take(table.destructor_count as usize)
                .rev()
                .flatten()
            {
                let code = invoke(table, *callback);
                if code != NOOKERY_BOOT_OK {
                    let code = reported_error(table, code, "destructor callback failed");
                    if let Ok(mut registered) = REGISTERED_CALLBACKS.lock() {
                        if let Some(state) = registered.as_mut() {
                            state.lifecycle = AbiLifecycle::Failed;
                        }
                    }
                    return code;
                }
            }
            NOOKERY_BOOT_OK
        })
    })
}

/// Invoke the `src/main.c` `entry` sequence after `_prolog` has completed:
/// `padmgr_Init(NULL)`, `new_Debug_mode()`, `SETREG(SREG, 0, 0)`, then
/// `mainproc(NULL)`.
#[no_mangle]
pub extern "C" fn nookery_boot_entry() -> i32 {
    ffi_guard(|| {
        with_registered_table(|table| {
            if let Err(code) = validate_table(table) {
                return reported_error(table, code, "invalid nookery boot callback table");
            }
            let (pad_init, debug_mode, reset_register, mainproc) = match (
                table.entry_pad_init,
                table.entry_debug_mode,
                table.entry_reset_register,
                table.entry_mainproc,
            ) {
                (Some(pad_init), Some(debug_mode), Some(reset_register), Some(mainproc)) => {
                    (pad_init, debug_mode, reset_register, mainproc)
                }
                _ => {
                    return reported_error(
                        table,
                        NOOKERY_BOOT_INVALID_TABLE,
                        "entry callback missing",
                    )
                }
            };
            match REGISTERED_CALLBACKS.lock() {
                Ok(mut registered) => {
                    let Some(state) = registered.as_mut() else {
                        return NOOKERY_BOOT_INVALID_ARGUMENT;
                    };
                    if state.lifecycle != AbiLifecycle::Running || state.entry_called {
                        return NOOKERY_BOOT_INVALID_STATE;
                    }
                    state.entry_called = true;
                }
                Err(_) => return NOOKERY_BOOT_INVALID_STATE,
            }
            for (callback, message) in [
                (pad_init, "entry pad-init callback failed"),
                (debug_mode, "entry debug-mode callback failed"),
                (reset_register, "entry reset-register callback failed"),
                (mainproc, "entry mainproc callback failed"),
            ] {
                let code = invoke(table, callback);
                if code != NOOKERY_BOOT_OK {
                    let code = reported_error(table, code, message);
                    if let Ok(mut registered) = REGISTERED_CALLBACKS.lock() {
                        if let Some(state) = registered.as_mut() {
                            state.lifecycle = AbiLifecycle::Failed;
                        }
                    }
                    return code;
                }
            }
            NOOKERY_BOOT_OK
        })
    })
}

/// Deliver a bounded `_unresolved` report. Stack walking remains a host
/// responsibility; a missing unresolved callback is an explicit error rather
/// than silently pretending the diagnostic was handled.
///
/// # Safety
/// `frames` must be null when `frame_count` is zero, otherwise it must point
/// to `frame_count` readable frames that remain valid for this call.
#[no_mangle]
pub unsafe extern "C" fn nookery_boot_unresolved(
    frames: *const NookeryBootStackFrame,
    frame_count: u32,
) -> i32 {
    ffi_guard(|| {
        with_registered_table(|table| {
            if frame_count as usize > NOOKERY_BOOT_MAX_UNRESOLVED_FRAMES {
                return reported_error(
                    table,
                    NOOKERY_BOOT_INVALID_ARGUMENT,
                    "too many unresolved frames",
                );
            }
            if frame_count != 0 && frames.is_null() {
                return reported_error(
                    table,
                    NOOKERY_BOOT_INVALID_ARGUMENT,
                    "null unresolved frames",
                );
            }
            let Some(unresolved) = table.unresolved else {
                return reported_error(
                    table,
                    NOOKERY_BOOT_UNRESOLVED_UNAVAILABLE,
                    "unresolved callback unavailable",
                );
            };
            let code = match catch_unwind(AssertUnwindSafe(|| unsafe {
                unresolved(table.context, frames, frame_count)
            })) {
                Ok(NOOKERY_BOOT_OK) => NOOKERY_BOOT_OK,
                Ok(
                    code @ (NOOKERY_BOOT_INVALID_ARGUMENT
                    | NOOKERY_BOOT_INVALID_TABLE
                    | NOOKERY_BOOT_INVALID_STATE
                    | NOOKERY_BOOT_CALLBACK_FAILED
                    | NOOKERY_BOOT_UNRESOLVED_UNAVAILABLE),
                ) => code,
                Ok(_) => NOOKERY_BOOT_CALLBACK_FAILED,
                Err(_) => NOOKERY_BOOT_PANIC,
            };
            if code == NOOKERY_BOOT_OK {
                code
            } else {
                reported_error(table, code, "unresolved callback failed")
            }
        })
    })
}

// These hooks intentionally do not provide platform defaults. The real C
// boundary still owns display/audio/disc/OS behavior; callers must register
// callbacks for those operations instead of relying on hidden globals.

#[cfg(test)]
mod tests;
