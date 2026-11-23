//! Stable, no-unwind host boundary for the parent application.
//!
//! The production PC graph entry allocates the callback table and calls these
//! exports. Each operation names its source seam in the adjacent comment and
//! is kept instance-owned rather than process-global.
//!
//! The C source performs reset detection at the end of a graph frame and then
//! immediately schedules the PRENMI game transition. This boundary exposes
//! those two source seams as explicit `reset` and `prenmi` calls so a parent
//! can place the transition in its own scheduler; it never synthesizes a
//! hidden extra frame or invokes callbacks after the reset request.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;

use crate::{DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH, Lifecycle, RuntimeError};

/// Deterministic result codes for C callers. Values are ABI-stable.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NookeryRuntimeStatus {
    Ok = 0,
    Null = 1,
    InvalidState = 2,
    InvalidInput = 3,
    HostError = 4,
    CallbackError = 5,
    Panic = 6,
    Overflow = 7,
    /// Another operation already owns this handle, including same-thread
    /// callback re-entry. The guard is nonblocking so callbacks cannot deadlock.
    Busy = 8,
}

/// Callback failure values are bounded before they cross the C boundary.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NookeryRuntimeCallbackStatus {
    Ok = 0,
    Failed = 1,
    Unavailable = 2,
    InvalidInput = 3,
}

/// Source seams that this callback-only boundary cannot represent by itself.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NookeryRuntimeGap {
    /// `graph_main` performs GAME_GOTO_NEXT(PRENMI) in the same source frame
    /// after reset detection; the parent scheduler must make that transition.
    AutomaticSourceResetFrameTransition = 0,
}

/// Explicit ledger for the intentionally unrepresented source transition.
pub const RUNTIME_GAPS: &[NookeryRuntimeGap] =
    &[NookeryRuntimeGap::AutomaticSourceResetFrameTransition];

pub type NookeryRuntimeVoidCallback =
    unsafe extern "C-unwind" fn(*mut std::ffi::c_void) -> NookeryRuntimeCallbackStatus;
pub type NookeryRuntimeScreenCallback =
    unsafe extern "C-unwind" fn(*mut std::ffi::c_void, u32, u32) -> NookeryRuntimeCallbackStatus;
pub type NookeryRuntimeFrameCallback =
    unsafe extern "C-unwind" fn(*mut std::ffi::c_void, f64, u64) -> NookeryRuntimeCallbackStatus;
pub type NookeryRuntimeResizeCallback = unsafe extern "C-unwind" fn(
    *mut std::ffi::c_void,
    i64,
    *mut usize,
)
    -> NookeryRuntimeCallbackStatus;

// `C-unwind` has the same call layout as the C ABI for C callers. It permits a
// Rust callback implementation to unwind into the boundary's catch_unwind;
// native C callbacks do not unwind and remain ordinary C function pointers.

/// Explicit C callback table. The parent owns `user_data` and every callback.
/// A null callback means that source seam is an intentional no-op.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NookeryRuntimeCallbacks {
    pub user_data: *mut std::ffi::c_void,
    // src/main.c: platform/card/IRQ/input/display/messages/archive/ARAM boot.
    pub set_screen_size: Option<NookeryRuntimeScreenCallback>,
    pub initial_frame_begin: Option<NookeryRuntimeVoidCallback>,
    pub initial_frame_end: Option<NookeryRuntimeVoidCallback>,
    pub initialize_card: Option<NookeryRuntimeVoidCallback>,
    pub create_irq_manager: Option<NookeryRuntimeVoidCallback>,
    pub create_input_manager: Option<NookeryRuntimeVoidCallback>,
    pub runtime_frame_begin: Option<NookeryRuntimeVoidCallback>,
    pub runtime_frame_end: Option<NookeryRuntimeVoidCallback>,
    pub initialize_display: Option<NookeryRuntimeVoidCallback>,
    pub initialize_messages: Option<NookeryRuntimeVoidCallback>,
    pub enable_land_start: Option<NookeryRuntimeVoidCallback>,
    pub mount_archive: Option<NookeryRuntimeVoidCallback>,
    pub dump_aram_heap: Option<NookeryRuntimeVoidCallback>,
    // src/graph.c: first frame and graph frame callbacks.
    pub frame_begin: Option<NookeryRuntimeVoidCallback>,
    pub input: Option<NookeryRuntimeVoidCallback>,
    pub draw_first: Option<NookeryRuntimeFrameCallback>,
    pub update: Option<NookeryRuntimeFrameCallback>,
    pub execute: Option<NookeryRuntimeFrameCallback>,
    pub bgm: Option<NookeryRuntimeFrameCallback>,
    pub audio_tick: Option<NookeryRuntimeVoidCallback>,
    pub frame_end: Option<NookeryRuntimeVoidCallback>,
    pub submit_frame: Option<NookeryRuntimeVoidCallback>,
    pub game_init: Option<NookeryRuntimeVoidCallback>,
    pub audio_initialize: Option<NookeryRuntimeVoidCallback>,
    // src/game.c: reset/pre-NMI, resize, cleanup.
    pub audio_reset: Option<NookeryRuntimeVoidCallback>,
    pub reset: Option<NookeryRuntimeVoidCallback>,
    pub prenmi: Option<NookeryRuntimeVoidCallback>,
    pub resize_game_memory: Option<NookeryRuntimeResizeCallback>,
    pub game_cleanup: Option<NookeryRuntimeVoidCallback>,
    pub audio_cleanup: Option<NookeryRuntimeVoidCallback>,
    pub renderer_shutdown: Option<NookeryRuntimeVoidCallback>,
    pub platform_shutdown: Option<NookeryRuntimeVoidCallback>,
}

impl Default for NookeryRuntimeCallbacks {
    fn default() -> Self {
        Self {
            user_data: std::ptr::null_mut(),
            set_screen_size: None,
            initial_frame_begin: None,
            initial_frame_end: None,
            initialize_card: None,
            create_irq_manager: None,
            create_input_manager: None,
            runtime_frame_begin: None,
            runtime_frame_end: None,
            initialize_display: None,
            initialize_messages: None,
            enable_land_start: None,
            mount_archive: None,
            dump_aram_heap: None,
            frame_begin: None,
            input: None,
            draw_first: None,
            update: None,
            execute: None,
            bgm: None,
            audio_tick: None,
            frame_end: None,
            submit_frame: None,
            game_init: None,
            audio_initialize: None,
            audio_reset: None,
            reset: None,
            prenmi: None,
            resize_game_memory: None,
            game_cleanup: None,
            audio_cleanup: None,
            renderer_shutdown: None,
            platform_shutdown: None,
        }
    }
}

/// Runtime tuning passed by value, with no hidden defaults in the C caller.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NookeryRuntimeConfig {
    pub width: u32,
    pub height: u32,
    pub audio_enabled: u8,
}

impl Default for NookeryRuntimeConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_SCREEN_WIDTH,
            height: DEFAULT_SCREEN_HEIGHT,
            audio_enabled: 1,
        }
    }
}

/// Opaque ownership handle returned by `nookery_runtime_create`.
#[repr(C)]
pub struct NookeryRuntimeHandle {
    runtime: CAbiRuntime,
    busy: std::sync::atomic::AtomicBool,
}

struct CAbiRuntime {
    callbacks: NookeryRuntimeCallbacks,
    config: NookeryRuntimeConfig,
    lifecycle: Lifecycle,
    graph: nookery_graph::Graph,
    frame_index: u64,
    total_60fps_frames: f64,
    audio_accumulator: f64,
    reset_requested: bool,
    reset_seen: bool,
    stopped: bool,
}

impl CAbiRuntime {
    fn new(
        callbacks: NookeryRuntimeCallbacks,
        config: NookeryRuntimeConfig,
    ) -> Result<Self, NookeryRuntimeStatus> {
        if config.width == 0 || config.height == 0 {
            return Err(NookeryRuntimeStatus::InvalidInput);
        }
        Ok(Self {
            callbacks,
            config,
            lifecycle: Lifecycle::Cold,
            graph: nookery_graph::Graph::default(),
            frame_index: 0,
            total_60fps_frames: 0.0,
            audio_accumulator: 0.0,
            reset_requested: false,
            reset_seen: false,
            stopped: false,
        })
    }

    fn call(
        &self,
        callback: Option<NookeryRuntimeVoidCallback>,
    ) -> Result<(), NookeryRuntimeStatus> {
        let Some(callback) = callback else {
            return Ok(());
        };
        match unsafe { callback(self.callbacks.user_data) } {
            NookeryRuntimeCallbackStatus::Ok => Ok(()),
            NookeryRuntimeCallbackStatus::InvalidInput => Err(NookeryRuntimeStatus::InvalidInput),
            NookeryRuntimeCallbackStatus::Failed | NookeryRuntimeCallbackStatus::Unavailable => {
                Err(NookeryRuntimeStatus::CallbackError)
            }
        }
    }

    fn call_resize(
        &self,
        callback: Option<NookeryRuntimeResizeCallback>,
        requested_size: i64,
        actual_size: *mut usize,
    ) -> Result<(), NookeryRuntimeStatus> {
        let Some(callback) = callback else {
            return Ok(());
        };
        match unsafe { callback(self.callbacks.user_data, requested_size, actual_size) } {
            NookeryRuntimeCallbackStatus::Ok => Ok(()),
            NookeryRuntimeCallbackStatus::InvalidInput => Err(NookeryRuntimeStatus::InvalidInput),
            NookeryRuntimeCallbackStatus::Failed | NookeryRuntimeCallbackStatus::Unavailable => {
                Err(NookeryRuntimeStatus::CallbackError)
            }
        }
    }

    fn call_screen(&self) -> Result<(), NookeryRuntimeStatus> {
        let Some(callback) = self.callbacks.set_screen_size else {
            return Ok(());
        };
        match unsafe {
            callback(
                self.callbacks.user_data,
                self.config.width,
                self.config.height,
            )
        } {
            NookeryRuntimeCallbackStatus::Ok => Ok(()),
            NookeryRuntimeCallbackStatus::InvalidInput => Err(NookeryRuntimeStatus::InvalidInput),
            NookeryRuntimeCallbackStatus::Failed | NookeryRuntimeCallbackStatus::Unavailable => {
                Err(NookeryRuntimeStatus::CallbackError)
            }
        }
    }

    fn call_frame(
        &self,
        callback: Option<NookeryRuntimeFrameCallback>,
        delta_seconds: f64,
    ) -> Result<(), NookeryRuntimeStatus> {
        let Some(callback) = callback else {
            return Ok(());
        };
        match unsafe { callback(self.callbacks.user_data, delta_seconds, self.frame_index) } {
            NookeryRuntimeCallbackStatus::Ok => Ok(()),
            NookeryRuntimeCallbackStatus::InvalidInput => Err(NookeryRuntimeStatus::InvalidInput),
            NookeryRuntimeCallbackStatus::Failed | NookeryRuntimeCallbackStatus::Unavailable => {
                Err(NookeryRuntimeStatus::CallbackError)
            }
        }
    }

    fn boot(&mut self) -> Result<(), NookeryRuntimeStatus> {
        if self.lifecycle != Lifecycle::Cold {
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        self.lifecycle = Lifecycle::Booting;
        if self.graph.begin().is_err() {
            self.lifecycle = Lifecycle::Failed;
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        let result = self.boot_inner();
        if result.is_err() {
            self.lifecycle = Lifecycle::Failed;
        }
        result
    }

    fn boot_inner(&mut self) -> Result<(), NookeryRuntimeStatus> {
        // src/main.c order: screen/initial frame, card, IRQ, input, display,
        // messages, land-start, archive, ARAM.
        self.call_screen()?;
        self.call(self.callbacks.initial_frame_begin)?;
        self.call(self.callbacks.initial_frame_end)?;
        for callback in [
            self.callbacks.initialize_card,
            self.callbacks.create_irq_manager,
            self.callbacks.create_input_manager,
        ] {
            if let Err(status) = self.call(callback) {
                self.lifecycle = Lifecycle::Failed;
                return Err(status);
            }
        }
        // src/main.c's second JW_BeginFrame/JW_EndFrame brackets platform
        // thread/input setup before display initialization.
        for callback in [
            self.callbacks.runtime_frame_begin,
            self.callbacks.runtime_frame_end,
            self.callbacks.initialize_display,
            self.callbacks.initialize_messages,
            self.callbacks.enable_land_start,
            self.callbacks.mount_archive,
            self.callbacks.dump_aram_heap,
        ] {
            if let Err(status) = self.call(callback) {
                self.lifecycle = Lifecycle::Failed;
                return Err(status);
            }
        }
        self.call(self.callbacks.game_init)?;
        self.call(self.callbacks.audio_initialize)?;
        self.lifecycle = Lifecycle::Booted;
        Ok(())
    }

    fn frame(&mut self, delta_seconds: f64) -> Result<(), NookeryRuntimeStatus> {
        if self.lifecycle != Lifecycle::Booted || self.stopped || self.reset_requested {
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(NookeryRuntimeStatus::InvalidInput);
        }
        let effective_delta_seconds = (delta_seconds * 60.0).min(4.0) / 60.0;
        self.lifecycle = Lifecycle::Frame;
        if self.graph.proc(effective_delta_seconds).is_err() {
            self.lifecycle = Lifecycle::Failed;
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        let result = self.frame_inner(effective_delta_seconds);
        if result.is_err() {
            let _ = self.graph.finish_frame(false);
            self.lifecycle = Lifecycle::Failed;
        } else {
            if self.graph.finish_frame(true).is_err() {
                self.lifecycle = Lifecycle::Failed;
                return Err(NookeryRuntimeStatus::InvalidState);
            }
            self.lifecycle = Lifecycle::Booted;
        }
        result
    }

    fn frame_inner(&mut self, delta_seconds: f64) -> Result<(), NookeryRuntimeStatus> {
        // src/graph.c: graph setup/controller, game draw-first/time/execute/BGM,
        // audio schedule, draw finish and task submission.
        self.call(self.callbacks.frame_begin)?;
        self.call(self.callbacks.input)?;
        self.call_frame(self.callbacks.draw_first, delta_seconds)?;
        self.call_frame(self.callbacks.update, delta_seconds)?;
        self.call_frame(self.callbacks.execute, delta_seconds)?;
        self.call_frame(self.callbacks.bgm, delta_seconds)?;
        self.call(self.callbacks.frame_end)?;
        self.call(self.callbacks.submit_frame)?;
        // src/graph.c graph_audio_gameframe follows task submission and is
        // capped at four whole 60 Hz ticks after consuming the accumulator.
        if self.config.audio_enabled != 0 {
            let delta_frames = (delta_seconds * 60.0).min(4.0);
            self.audio_accumulator += delta_frames;
            let whole_ticks = self.audio_accumulator.floor();
            self.audio_accumulator -= whole_ticks;
            let ticks = whole_ticks.min(4.0) as u32;
            for _ in 0..ticks {
                self.call(self.callbacks.audio_tick)?;
            }
        }
        self.frame_index = self
            .frame_index
            .checked_add(1)
            .ok_or(NookeryRuntimeStatus::Overflow)?;
        self.total_60fps_frames += (delta_seconds * 60.0).min(4.0);
        if !self.total_60fps_frames.is_finite() {
            return Err(NookeryRuntimeStatus::Overflow);
        }
        Ok(())
    }

    fn reset(&mut self) -> Result<(), NookeryRuntimeStatus> {
        if self.lifecycle != Lifecycle::Booted || self.stopped || self.reset_requested {
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        let result = self.reset_inner();
        if result.is_err() {
            self.lifecycle = Lifecycle::Failed;
        }
        result
    }

    fn reset_inner(&mut self) -> Result<(), NookeryRuntimeStatus> {
        // src/graph.c do_soft_reset and src/game.c GAME_GOTO_NEXT reset transition.
        self.lifecycle = Lifecycle::Resetting;
        self.call(self.callbacks.audio_reset)?;
        self.call(self.callbacks.reset)?;
        self.reset_requested = true;
        self.lifecycle = Lifecycle::Booted;
        Ok(())
    }

    fn prenmi(&mut self) -> Result<(), NookeryRuntimeStatus> {
        if self.lifecycle != Lifecycle::Booted {
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        // src/game.c pre-NMI transition is explicit and idempotent per handle.
        if !self.reset_requested || self.reset_seen {
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        let result = self.call(self.callbacks.prenmi);
        if result.is_err() {
            self.lifecycle = Lifecycle::Failed;
        }
        result?;
        self.reset_seen = true;
        self.stopped = true;
        Ok(())
    }

    fn resize(
        &mut self,
        requested_size: i64,
        actual_size: *mut usize,
    ) -> Result<(), NookeryRuntimeStatus> {
        if self.lifecycle != Lifecycle::Booted {
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        // `game_resize_hyral` treats non-positive values as a request relative
        // to the available arena, so negative requests are source-valid and
        // must reach the parent allocator unchanged.
        if actual_size.is_null() {
            return Err(NookeryRuntimeStatus::InvalidInput);
        }
        // src/game.c game_resize_hyral: preserve signed request and report size.
        self.call_resize(
            self.callbacks.resize_game_memory,
            requested_size,
            actual_size,
        )
    }

    fn shutdown(&mut self) -> Result<(), NookeryRuntimeStatus> {
        if !matches!(
            self.lifecycle,
            Lifecycle::Booted | Lifecycle::Failed | Lifecycle::Resetting
        ) {
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        self.lifecycle = Lifecycle::ShuttingDown;
        // src/game.c game_dt performs BGM/audio cleanup before the game
        // destructor; graph/main then release renderer and platform state.
        for callback in [
            self.callbacks.audio_cleanup,
            self.callbacks.game_cleanup,
            self.callbacks.renderer_shutdown,
            self.callbacks.platform_shutdown,
        ] {
            if let Err(status) = self.call(callback) {
                self.lifecycle = Lifecycle::Failed;
                return Err(status);
            }
        }
        if self.graph.end().is_err() {
            self.lifecycle = Lifecycle::Failed;
            return Err(NookeryRuntimeStatus::InvalidState);
        }
        self.lifecycle = Lifecycle::Shutdown;
        Ok(())
    }
}

fn ffi_call<F>(operation: F) -> NookeryRuntimeStatus
where
    F: FnOnce() -> NookeryRuntimeStatus,
{
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or(NookeryRuntimeStatus::Panic)
}

#[unsafe(no_mangle)]
/// Creates one owned runtime handle without mutating process-global state.
///
/// # Safety
/// `callbacks`, `config`, and `out_handle` must be valid readable/writable
/// pointers for this call. The callback table's `user_data` must remain valid
/// until the handle is destroyed, and callbacks must obey their declared ABI.
pub unsafe extern "C" fn nookery_runtime_create(
    callbacks: *const NookeryRuntimeCallbacks,
    config: *const NookeryRuntimeConfig,
    out_handle: *mut *mut NookeryRuntimeHandle,
) -> NookeryRuntimeStatus {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_handle.is_null() {
            return NookeryRuntimeStatus::Null;
        }
        unsafe { *out_handle = std::ptr::null_mut() };
        if callbacks.is_null() || config.is_null() {
            return NookeryRuntimeStatus::Null;
        }
        let callbacks = unsafe { *callbacks };
        let config = unsafe { *config };
        let Ok(runtime) = CAbiRuntime::new(callbacks, config) else {
            return NookeryRuntimeStatus::InvalidInput;
        };
        let handle = Box::new(NookeryRuntimeHandle {
            runtime,
            busy: std::sync::atomic::AtomicBool::new(false),
        });
        unsafe { *out_handle = Box::into_raw(handle) };
        NookeryRuntimeStatus::Ok
    }));
    result.unwrap_or(NookeryRuntimeStatus::Panic)
}

fn with_handle<F>(handle: *mut NookeryRuntimeHandle, operation: F) -> NookeryRuntimeStatus
where
    F: FnOnce(&mut CAbiRuntime) -> NookeryRuntimeStatus,
{
    let Some(handle) = NonNull::new(handle) else {
        return NookeryRuntimeStatus::Null;
    };
    // The caller owns the opaque allocation and must provide a live handle.
    // Acquire the guard before manufacturing `&mut CAbiRuntime`: this is the
    // only point where the raw C pointer becomes a Rust mutable reference.
    let handle_ref = unsafe { handle.as_ref() };
    let guard = match HandleGuard::try_acquire(&handle_ref.busy) {
        Ok(guard) => guard,
        Err(status) => return status,
    };
    let status = ffi_call(|| operation(unsafe { &mut (*handle.as_ptr()).runtime }));
    drop(guard);
    status
}

struct HandleGuard<'a> {
    busy: &'a std::sync::atomic::AtomicBool,
}

impl<'a> HandleGuard<'a> {
    fn try_acquire(busy: &'a std::sync::atomic::AtomicBool) -> Result<Self, NookeryRuntimeStatus> {
        use std::sync::atomic::Ordering;
        busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| Self { busy })
            .map_err(|_| NookeryRuntimeStatus::Busy)
    }
}

impl Drop for HandleGuard<'_> {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self.busy.store(false, Ordering::Release);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nookery_runtime_boot(handle: *mut NookeryRuntimeHandle) -> NookeryRuntimeStatus {
    with_handle(handle, |runtime| match runtime.boot() {
        Ok(()) => NookeryRuntimeStatus::Ok,
        Err(status) => status,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn nookery_runtime_frame(
    handle: *mut NookeryRuntimeHandle,
    delta_seconds: f64,
) -> NookeryRuntimeStatus {
    with_handle(handle, |runtime| match runtime.frame(delta_seconds) {
        Ok(()) => NookeryRuntimeStatus::Ok,
        Err(status) => status,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn nookery_runtime_reset(handle: *mut NookeryRuntimeHandle) -> NookeryRuntimeStatus {
    with_handle(handle, |runtime| match runtime.reset() {
        Ok(()) => NookeryRuntimeStatus::Ok,
        Err(status) => status,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn nookery_runtime_prenmi(
    handle: *mut NookeryRuntimeHandle,
) -> NookeryRuntimeStatus {
    with_handle(handle, |runtime| match runtime.prenmi() {
        Ok(()) => NookeryRuntimeStatus::Ok,
        Err(status) => status,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn nookery_runtime_resize(
    handle: *mut NookeryRuntimeHandle,
    requested_size: i64,
    actual_size: *mut usize,
) -> NookeryRuntimeStatus {
    with_handle(handle, |runtime| {
        match runtime.resize(requested_size, actual_size) {
            Ok(()) => NookeryRuntimeStatus::Ok,
            Err(status) => status,
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn nookery_runtime_shutdown(
    handle: *mut NookeryRuntimeHandle,
) -> NookeryRuntimeStatus {
    with_handle(handle, |runtime| match runtime.shutdown() {
        Ok(()) => NookeryRuntimeStatus::Ok,
        Err(status) => status,
    })
}

#[unsafe(no_mangle)]
/// Releases a handle and reports whether the source shutdown transition ran.
///
/// # Safety
/// `handle` must be null or a live pointer previously returned through
/// `nookery_runtime_create`; it must not be used again after this call.
pub unsafe extern "C" fn nookery_runtime_destroy(
    handle: *mut NookeryRuntimeHandle,
) -> NookeryRuntimeStatus {
    if handle.is_null() {
        return NookeryRuntimeStatus::Null;
    }
    ffi_call(|| {
        let handle = unsafe { NonNull::new_unchecked(handle) };
        let handle_ref = unsafe { handle.as_ref() };
        let guard = match HandleGuard::try_acquire(&handle_ref.busy) {
            Ok(guard) => guard,
            Err(status) => return status,
        };
        // Destruction is a separate explicit operation so C can observe an
        // invalid lifecycle rather than silently running cleanup twice.
        let status = if unsafe { handle.as_ref() }.runtime.lifecycle == Lifecycle::Shutdown {
            NookeryRuntimeStatus::Ok
        } else {
            NookeryRuntimeStatus::InvalidState
        };
        // The allocation is about to disappear, so running HandleGuard::drop
        // after Box::from_raw would access its freed AtomicBool. No caller may
        // reuse this handle after destroy; intentionally leak only the guard,
        // not the handle allocation.
        std::mem::forget(guard);
        drop(unsafe { Box::from_raw(handle.as_ptr()) });
        status
    })
}

impl From<RuntimeError> for NookeryRuntimeStatus {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::InvalidInput => Self::InvalidInput,
            RuntimeError::ArithmeticOverflow => Self::Overflow,
            RuntimeError::InvalidState { .. } | RuntimeError::GameStopped => Self::InvalidState,
            RuntimeError::Host { .. } => Self::HostError,
            RuntimeError::Callback(_) => Self::CallbackError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Trace {
        events: Vec<&'static str>,
        fail: Option<&'static str>,
        resize_requests: Vec<i64>,
        frame_deltas: Vec<f64>,
        screen: Option<(u32, u32)>,
        reentrant_handle: *mut NookeryRuntimeHandle,
        reentrant_status: Option<NookeryRuntimeStatus>,
    }

    unsafe fn trace_mut(user_data: *mut std::ffi::c_void) -> &'static mut Trace {
        // Tests model a parent-owned callback context; the runtime never owns
        // or globally stores this pointer.
        unsafe { &mut *(user_data.cast::<Trace>()) }
    }

    macro_rules! void_callback {
        ($name:ident, $event:literal) => {
            unsafe extern "C-unwind" fn $name(
                user_data: *mut std::ffi::c_void,
            ) -> NookeryRuntimeCallbackStatus {
                let trace = unsafe { trace_mut(user_data) };
                trace.events.push($event);
                if trace.fail == Some($event) {
                    NookeryRuntimeCallbackStatus::Failed
                } else {
                    NookeryRuntimeCallbackStatus::Ok
                }
            }
        };
    }

    macro_rules! frame_callback {
        ($name:ident, $event:literal) => {
            unsafe extern "C-unwind" fn $name(
                user_data: *mut std::ffi::c_void,
                delta_seconds: f64,
                _frame_index: u64,
            ) -> NookeryRuntimeCallbackStatus {
                let trace = unsafe { trace_mut(user_data) };
                trace.events.push($event);
                trace.frame_deltas.push(delta_seconds);
                if trace.fail == Some($event) {
                    NookeryRuntimeCallbackStatus::Failed
                } else {
                    NookeryRuntimeCallbackStatus::Ok
                }
            }
        };
    }

    void_callback!(initial_begin, "initial_begin");
    void_callback!(initial_end, "initial_end");
    void_callback!(card, "card");
    void_callback!(irq, "irq");
    void_callback!(input_manager, "input_manager");
    void_callback!(runtime_begin, "runtime_begin");
    void_callback!(runtime_end, "runtime_end");
    void_callback!(display, "display");
    void_callback!(messages, "messages");
    void_callback!(land_start, "land_start");
    void_callback!(archive, "archive");
    void_callback!(aram_dump, "aram_dump");
    void_callback!(game_init, "game_init");
    void_callback!(audio_init, "audio_init");
    void_callback!(frame_begin, "frame_begin");
    void_callback!(input, "input");
    frame_callback!(draw_first, "draw_first");
    frame_callback!(update, "update");
    frame_callback!(execute, "execute");
    frame_callback!(bgm, "bgm");
    void_callback!(audio_tick, "audio_tick");
    void_callback!(frame_end, "frame_end");
    void_callback!(submit, "submit");
    void_callback!(audio_reset, "audio_reset");
    void_callback!(reset, "reset");
    void_callback!(prenmi, "prenmi");
    void_callback!(game_cleanup, "game_cleanup");
    void_callback!(audio_cleanup, "audio_cleanup");
    void_callback!(renderer_shutdown, "renderer_shutdown");
    void_callback!(platform_shutdown, "platform_shutdown");

    unsafe extern "C-unwind" fn screen(
        user_data: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> NookeryRuntimeCallbackStatus {
        let trace = unsafe { trace_mut(user_data) };
        trace.events.push("screen");
        trace.screen = Some((width, height));
        NookeryRuntimeCallbackStatus::Ok
    }

    unsafe extern "C-unwind" fn resize(
        user_data: *mut std::ffi::c_void,
        requested_size: i64,
        actual_size: *mut usize,
    ) -> NookeryRuntimeCallbackStatus {
        let trace = unsafe { trace_mut(user_data) };
        trace.events.push("resize");
        trace.resize_requests.push(requested_size);
        unsafe { *actual_size = requested_size.max(0) as usize + 1 };
        NookeryRuntimeCallbackStatus::Ok
    }

    unsafe extern "C-unwind" fn reentrant_boot(
        user_data: *mut std::ffi::c_void,
    ) -> NookeryRuntimeCallbackStatus {
        let handle = unsafe { trace_mut(user_data) }.reentrant_handle;
        let status = nookery_runtime_boot(handle);
        unsafe { trace_mut(user_data) }.reentrant_status = Some(status);
        NookeryRuntimeCallbackStatus::Ok
    }

    #[cfg(not(debug_assertions))]
    unsafe extern "C-unwind" fn panic_callback(
        _user_data: *mut std::ffi::c_void,
    ) -> NookeryRuntimeCallbackStatus {
        panic!("release exported-boundary panic probe")
    }

    fn callbacks(trace: &mut Trace) -> NookeryRuntimeCallbacks {
        NookeryRuntimeCallbacks {
            user_data: (trace as *mut Trace).cast(),
            set_screen_size: Some(screen),
            initial_frame_begin: Some(initial_begin),
            initial_frame_end: Some(initial_end),
            initialize_card: Some(card),
            create_irq_manager: Some(irq),
            create_input_manager: Some(input_manager),
            runtime_frame_begin: Some(runtime_begin),
            runtime_frame_end: Some(runtime_end),
            initialize_display: Some(display),
            initialize_messages: Some(messages),
            enable_land_start: Some(land_start),
            mount_archive: Some(archive),
            dump_aram_heap: Some(aram_dump),
            frame_begin: Some(frame_begin),
            input: Some(input),
            draw_first: Some(draw_first),
            update: Some(update),
            execute: Some(execute),
            bgm: Some(bgm),
            audio_tick: Some(audio_tick),
            frame_end: Some(frame_end),
            submit_frame: Some(submit),
            game_init: Some(game_init),
            audio_initialize: Some(audio_init),
            audio_reset: Some(audio_reset),
            reset: Some(reset),
            prenmi: Some(prenmi),
            resize_game_memory: Some(resize),
            game_cleanup: Some(game_cleanup),
            audio_cleanup: Some(audio_cleanup),
            renderer_shutdown: Some(renderer_shutdown),
            platform_shutdown: Some(platform_shutdown),
        }
    }

    fn create(trace: &mut Trace) -> *mut NookeryRuntimeHandle {
        let config = NookeryRuntimeConfig::default();
        let mut handle = std::ptr::null_mut();
        assert_eq!(
            unsafe { nookery_runtime_create(&callbacks(trace), &config, &mut handle) },
            NookeryRuntimeStatus::Ok
        );

        handle
    }

    #[test]
    fn c_abi_source_order_is_mutation_sensitive() {
        let mut trace = Trace::default();
        let handle = create(&mut trace);
        assert!(!handle.is_null());
        assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            trace.events,
            vec![
                "screen",
                "initial_begin",
                "initial_end",
                "card",
                "irq",
                "input_manager",
                "runtime_begin",
                "runtime_end",
                "display",
                "messages",
                "land_start",
                "archive",
                "aram_dump",
                "game_init",
                "audio_init",
            ]
        );
        assert_eq!(
            nookery_runtime_frame(handle, 1.0 / 60.0),
            NookeryRuntimeStatus::Ok
        );
        assert_eq!(
            &trace.events[15..],
            &[
                "frame_begin",
                "input",
                "draw_first",
                "update",
                "execute",
                "bgm",
                "frame_end",
                "submit",
                "audio_tick",
            ]
        );
        assert_eq!(trace.frame_deltas, vec![1.0 / 60.0; 4]);
        assert_eq!(nookery_runtime_shutdown(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            &trace.events[24..],
            &[
                "audio_cleanup",
                "game_cleanup",
                "renderer_shutdown",
                "platform_shutdown",
            ]
        );
        assert_eq!(
            unsafe { nookery_runtime_destroy(handle) },
            NookeryRuntimeStatus::Ok
        );
    }

    #[test]
    fn c_abi_reset_and_prenmi_are_ordered_and_terminal() {
        assert_eq!(
            RUNTIME_GAPS,
            &[NookeryRuntimeGap::AutomaticSourceResetFrameTransition]
        );
        let mut trace = Trace::default();
        let handle = create(&mut trace);
        assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            trace.screen,
            Some((DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT))
        );
        assert_eq!(nookery_runtime_reset(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            nookery_runtime_frame(handle, 0.0),
            NookeryRuntimeStatus::InvalidState
        );
        assert_eq!(&trace.events[15..], &["audio_reset", "reset"]);
        assert_eq!(nookery_runtime_prenmi(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(&trace.events[15..], &["audio_reset", "reset", "prenmi"]);
        assert_eq!(
            nookery_runtime_frame(handle, 0.0),
            NookeryRuntimeStatus::InvalidState
        );
        assert_eq!(
            nookery_runtime_prenmi(handle),
            NookeryRuntimeStatus::InvalidState
        );
        assert_eq!(nookery_runtime_shutdown(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            unsafe { nookery_runtime_destroy(handle) },
            NookeryRuntimeStatus::Ok
        );
    }

    #[test]
    fn c_abi_callback_error_is_bounded_and_shutdown_remains_available() {
        let mut trace = Trace {
            fail: Some("execute"),
            ..Trace::default()
        };
        let handle = create(&mut trace);
        assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            nookery_runtime_frame(handle, 1.0 / 60.0),
            NookeryRuntimeStatus::CallbackError
        );
        assert_eq!(
            nookery_runtime_frame(handle, 0.0),
            NookeryRuntimeStatus::InvalidState
        );
        assert_eq!(nookery_runtime_shutdown(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            unsafe { nookery_runtime_destroy(handle) },
            NookeryRuntimeStatus::Ok
        );
    }

    #[test]
    fn c_abi_invalid_lifecycle_and_null_inputs_are_deterministic() {
        assert_eq!(
            nookery_runtime_boot(std::ptr::null_mut()),
            NookeryRuntimeStatus::Null
        );
        assert_eq!(
            unsafe { nookery_runtime_destroy(std::ptr::null_mut()) },
            NookeryRuntimeStatus::Null
        );
        let mut handle = std::ptr::null_mut();
        assert_eq!(
            unsafe { nookery_runtime_create(std::ptr::null(), std::ptr::null(), &mut handle) },
            NookeryRuntimeStatus::Null
        );
        assert!(handle.is_null());

        let mut trace = Trace::default();
        let handle = create(&mut trace);
        assert_eq!(
            nookery_runtime_frame(handle, 0.0),
            NookeryRuntimeStatus::InvalidState
        );
        assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            nookery_runtime_resize(handle, 10, std::ptr::null_mut()),
            NookeryRuntimeStatus::InvalidInput
        );
        assert_eq!(
            nookery_runtime_boot(handle),
            NookeryRuntimeStatus::InvalidState
        );
        assert_eq!(
            nookery_runtime_frame(handle, f64::NAN),
            NookeryRuntimeStatus::InvalidInput
        );
        assert_eq!(nookery_runtime_shutdown(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            unsafe { nookery_runtime_destroy(handle) },
            NookeryRuntimeStatus::Ok
        );
    }

    #[test]
    fn c_abi_resize_edge_cases_preserve_signed_source_request() {
        let mut trace = Trace::default();
        let handle = create(&mut trace);
        let mut actual = 0_usize;
        assert_eq!(
            nookery_runtime_resize(handle, 10, &mut actual),
            NookeryRuntimeStatus::InvalidState
        );
        assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            nookery_runtime_resize(handle, -1, &mut actual),
            NookeryRuntimeStatus::Ok
        );
        assert_eq!(actual, 1);
        assert_eq!(
            nookery_runtime_resize(handle, 0, &mut actual),
            NookeryRuntimeStatus::Ok
        );
        assert_eq!(actual, 1);
        assert_eq!(
            nookery_runtime_resize(handle, 4096, &mut actual),
            NookeryRuntimeStatus::Ok
        );
        assert_eq!(actual, 4097);
        assert_eq!(trace.resize_requests, vec![-1, 0, 4096]);
        assert_eq!(nookery_runtime_shutdown(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            unsafe { nookery_runtime_destroy(handle) },
            NookeryRuntimeStatus::Ok
        );
    }

    #[test]
    fn c_abi_panic_wrapper_returns_bounded_status() {
        assert_eq!(
            ffi_call(|| panic!("test panic")),
            NookeryRuntimeStatus::Panic
        );
    }

    #[test]
    fn c_abi_reentry_is_rejected_before_a_second_mutable_reference() {
        let mut trace = Trace::default();
        let callbacks = NookeryRuntimeCallbacks {
            user_data: (&mut trace as *mut Trace).cast(),
            initial_frame_begin: Some(reentrant_boot),
            ..NookeryRuntimeCallbacks::default()
        };
        let config = NookeryRuntimeConfig::default();
        let mut handle = std::ptr::null_mut();
        assert_eq!(
            unsafe { nookery_runtime_create(&callbacks, &config, &mut handle) },
            NookeryRuntimeStatus::Ok
        );
        trace.reentrant_handle = handle;
        assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(trace.reentrant_status, Some(NookeryRuntimeStatus::Busy));
        assert_eq!(nookery_runtime_shutdown(handle), NookeryRuntimeStatus::Ok);
        assert_eq!(
            unsafe { nookery_runtime_destroy(handle) },
            NookeryRuntimeStatus::Ok
        );
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_exported_boundary_converts_callback_panic_to_status() {
        let callbacks = NookeryRuntimeCallbacks {
            game_init: Some(panic_callback),
            ..NookeryRuntimeCallbacks::default()
        };
        let config = NookeryRuntimeConfig::default();
        let mut handle = std::ptr::null_mut();
        assert_eq!(
            unsafe { nookery_runtime_create(&callbacks, &config, &mut handle) },
            NookeryRuntimeStatus::Ok
        );
        assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Panic);
        assert_eq!(
            unsafe { nookery_runtime_destroy(handle) },
            NookeryRuntimeStatus::InvalidState
        );
    }
}
