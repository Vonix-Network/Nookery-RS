//! Production Rust state and C ABI for the PC jaudio/SDL audio seam.
//!
//! The SDL calls remain in `pc/src/pc_audio.c`; this crate owns every
//! observable AI/DSP value and all decisions about DMA acceptance, callback
//! ordering, refill progress, and reset behavior.

#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

/// `pc/src/pc_audio.c`: PC_AUDIO_SAMPLE_RATE and the SDL_AudioSpec contract.
pub const AUDIO_SAMPLE_RATE_HZ: u32 = 32_000;
/// S16 stereo has two 16-bit channels per sample frame.
pub const AUDIO_BYTES_PER_SAMPLE: u32 = 4;
/// Bounded SDL queue policy: at most two seconds of 32 kHz S16 stereo.
pub const AUDIO_QUEUE_CAPACITY_BYTES: u32 = AUDIO_SAMPLE_RATE_HZ * AUDIO_BYTES_PER_SAMPLE * 2;
/// `pc/src/pc_audio.c`: pc_audio_get_buffer_fill reports 16-bit units.
pub const AUDIO_QUEUE_CAPACITY_S16: i32 = (AUDIO_QUEUE_CAPACITY_BYTES / 2) as i32;

/// Dolphin's `AIDCallback` from `libs/dolphin/include/dolphin/ai.h`.
pub type AIDCallback = Option<unsafe extern "C" fn()>;
/// Dolphin's `AISCallback` from `libs/dolphin/include/dolphin/ai.h`.
pub type AISCallback = Option<unsafe extern "C" fn(u32)>;

/// Rust-owned, host-independent representation of the observable PC AI state.
///
/// The callback slots are kept here rather than in the SDL adapter so replacing
/// a callback has the same previous-value behavior in tests and production.
pub struct AudioState {
    dma_callback: AIDCallback,
    stream_callback: AISCallback,
    dsp_sample_rate: u32,
    stream_sample_rate: u32,
    stream_trigger: u32,
    stream_play_state: u32,
    stream_sample_count: u32,
    bytes_written: u32,
    master_volume: i32,
    host_active: bool,
    host_paused: bool,
}

impl AudioState {
    /// The defaults match the zero-initialized C statics in
    /// `pc/src/pc_audio.c`: DSP and stream rate 32 kHz, volume 100.
    pub const fn new() -> Self {
        Self {
            dma_callback: None,
            stream_callback: None,
            dsp_sample_rate: 0,
            stream_sample_rate: 0,
            stream_trigger: 0,
            stream_play_state: 0,
            stream_sample_count: 0,
            bytes_written: 0,
            master_volume: 100,
            host_active: false,
            host_paused: false,
        }
    }

    /// Apply the bounded DMA admission policy and update byte/sample counters.
    /// `available_bytes` is the current SDL stream fill. Zero address and zero
    /// length retain `pc/src/pc_audio.c:AIInitDMA`'s no-op behavior.
    pub fn accept_dma(&mut self, available_bytes: i32, address: usize, size: u32) -> bool {
        if !Self::dma_admissible(available_bytes, address, size) {
            return false;
        }
        self.record_dma(size);
        true
    }

    pub fn dma_admissible(available_bytes: i32, address: usize, size: u32) -> bool {
        if address == 0 || size == 0 || available_bytes < 0 {
            return false;
        }
        let available = available_bytes as u32;
        available <= AUDIO_QUEUE_CAPACITY_BYTES && size <= AUDIO_QUEUE_CAPACITY_BYTES - available
    }

    pub fn record_dma(&mut self, size: u32) {
        self.bytes_written = self.bytes_written.wrapping_add(size);
        self.stream_sample_count = self
            .stream_sample_count
            .wrapping_add(size / AUDIO_BYTES_PER_SAMPLE);
    }

    /// Return the amount still requested by one SDL refill callback.
    /// `pc/src/pc_audio.c:pc_audio_callback` stops exactly when no bytes were
    /// produced, preventing an infinite refill loop.
    pub fn refill_remaining(additional_amount: i32, before: u32, after: u32) -> Option<i32> {
        if additional_amount <= 0 {
            return None;
        }
        let produced = after.wrapping_sub(before);
        if produced == 0 {
            return None;
        }
        Some(additional_amount.saturating_sub(produced.min(i32::MAX as u32) as i32))
    }

    pub fn bytes_written(&self) -> u32 {
        self.bytes_written
    }

    pub fn stream_sample_count(&self) -> u32 {
        self.stream_sample_count
    }

    pub fn reset_stream_sample_count(&mut self) {
        self.stream_sample_count = 0;
    }

    pub fn set_stream_trigger(&mut self, trigger: u32) {
        self.stream_trigger = trigger;
    }

    pub fn stream_trigger(&self) -> u32 {
        self.stream_trigger
    }

    pub fn set_stream_play_state(&mut self, state: u32) {
        self.stream_play_state = state;
    }

    pub fn stream_play_state(&self) -> u32 {
        self.stream_play_state
    }

    pub fn set_dsp_sample_rate(&mut self, rate: u32) {
        self.dsp_sample_rate = rate;
    }

    pub fn dsp_sample_rate(&self) -> u32 {
        self.dsp_sample_rate
    }

    pub fn set_stream_sample_rate(&mut self, rate: u32) {
        self.stream_sample_rate = rate;
    }

    pub fn stream_sample_rate(&self) -> u32 {
        self.stream_sample_rate
    }

    /// `pc/src/pc_audio.c:AISetStreamVolLeft` is an intentional stub.
    pub fn set_stream_volume_left(&mut self, _volume: u8) {}

    /// `pc/src/pc_audio.c:AIGetStreamVolLeft` is an intentional stub.
    pub const fn stream_volume_left(&self) -> u8 {
        0xff
    }

    /// `pc/src/pc_audio.c:AISetStreamVolRight` is an intentional stub.
    pub fn set_stream_volume_right(&mut self, _volume: u8) {}

    /// `pc/src/pc_audio.c:AIGetStreamVolRight` is an intentional stub.
    pub const fn stream_volume_right(&self) -> u8 {
        0xff
    }

    pub fn set_master_volume(&mut self, volume: i32) -> i32 {
        self.master_volume = volume.clamp(0, 100);
        self.master_volume
    }

    pub fn master_volume(&self) -> i32 {
        self.master_volume
    }

    pub fn host_initialized(&mut self) {
        self.host_active = true;
        self.host_paused = true;
    }

    pub fn host_started(&mut self) {
        if self.host_active {
            self.host_paused = false;
        }
    }

    pub fn host_stopped(&mut self) {
        if self.host_active {
            self.host_paused = true;
        }
    }

    pub fn host_shutdown(&mut self) {
        self.host_active = false;
        self.host_paused = false;
    }

    pub fn host_active(&self) -> bool {
        self.host_active
    }

    pub fn host_paused(&self) -> bool {
        self.host_paused
    }

    pub fn replace_dma_callback(&mut self, callback: AIDCallback) -> AIDCallback {
        std::mem::replace(&mut self.dma_callback, callback)
    }

    pub fn replace_stream_callback(&mut self, callback: AISCallback) -> AISCallback {
        std::mem::replace(&mut self.stream_callback, callback)
    }
}

/// Invoke callbacks in the exact DMA-then-stream order from
/// `pc/src/pc_audio.c:AIInitDMA`.
///
/// # Safety
/// The function pointers must be valid C callbacks for this call.
pub unsafe fn dispatch_callbacks(
    dma_callback: AIDCallback,
    stream_callback: AISCallback,
    sample_count: u32,
) {
    if let Some(callback) = dma_callback {
        // SAFETY: Guaranteed by this function's contract.
        unsafe { callback() };
    }
    if let Some(callback) = stream_callback {
        // SAFETY: Guaranteed by this function's contract.
        unsafe { callback(sample_count) };
    }
}

impl Default for AudioState {
    fn default() -> Self {
        Self::new()
    }
}

static AUDIO_STATE: OnceLock<Mutex<AudioState>> = OnceLock::new();

fn audio_state() -> &'static Mutex<AudioState> {
    AUDIO_STATE.get_or_init(|| Mutex::new(AudioState::new()))
}

fn lock_audio_state() -> std::sync::MutexGuard<'static, AudioState> {
    audio_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

unsafe extern "C" {
    // Narrow host adapter implemented by pc/src/pc_audio.c.
    fn nookery_audio_host_init(callback: Option<unsafe extern "C" fn(i32)>) -> bool;
    fn nookery_audio_host_set_gain(gain: f32);
    fn nookery_audio_host_put(data: *const c_void, size: u32) -> bool;
    fn nookery_audio_host_resume() -> bool;
    fn nookery_audio_host_pause() -> bool;
    fn nookery_audio_host_available() -> i32;
    fn nookery_audio_host_shutdown();

    // src/static/jaudio_NES/internal/audiothread.c owns the mixer frame.
    fn pc_audio_process_frame();
}

fn host_available() -> i32 {
    // SAFETY: The C adapter has process lifetime and returns a plain status.
    unsafe { nookery_audio_host_available() }
}

fn set_host_gain(volume: i32) {
    // SAFETY: The adapter accepts a finite gain and ignores it when closed.
    unsafe { nookery_audio_host_set_gain(volume as f32 / 100.0) }
}

fn audio_is_active() -> bool {
    lock_audio_state().host_active()
}

/// SDL calls this through `pc/src/pc_audio.c:pc_audio_callback`.
///
/// # Safety
/// Called only by the C SDL callback trampoline with a live mixer callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_audio_refill(additional_amount: i32) {
    let mut remaining = additional_amount;
    while remaining > 0 {
        let before = lock_audio_state().bytes_written();
        // SAFETY: SDL invokes this callback only while the mixer is alive, and
        // pc_audio_process_frame is the existing synchronous game mixer seam.
        unsafe { pc_audio_process_frame() };
        let after = lock_audio_state().bytes_written();
        let Some(next) = AudioState::refill_remaining(remaining, before, after) else {
            break;
        };
        remaining = next;
    }
}

/// `pc/src/pc_audio.c:AIInit` — initialize once; a failed host open remains
/// inactive and is intentionally non-fatal to match the C oracle.
///
/// # Safety
/// `_stack` is ignored, matching the C oracle; the caller must use the C ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn AIInit(_stack: *mut u8) {
    if audio_is_active() {
        return;
    }
    // SAFETY: The callback is a stable Rust export and the adapter stores only
    // the function pointer until host shutdown.
    if unsafe { nookery_audio_host_init(Some(nookery_audio_refill)) } {
        let mut state = lock_audio_state();
        state.host_initialized();
        let volume = state.master_volume();
        drop(state);
        set_host_gain(volume);
    }
}

/// `pc/src/pc_audio.c:pc_audio_set_master_volume`.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_set_master_volume(volume: i32) {
    let clamped = lock_audio_state().set_master_volume(volume);
    if audio_is_active() {
        set_host_gain(clamped);
    }
}

/// Diagnostic accessor for host/settings callers; the SDL adapter receives the
/// same clamped value through `nookery_audio_host_set_gain`.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_get_master_volume() -> i32 {
    lock_audio_state().master_volume()
}

/// `pc/src/pc_audio.c:AIInitDMA`.
///
/// # Safety
/// `address` must reference `size` readable bytes for the duration of the
/// synchronous host-adapter copy, as required by the original C API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn AIInitDMA(address: usize, size: u32) {
    if !audio_is_active() || address == 0 || size == 0 {
        return;
    }

    let available = host_available();
    if available < 0 || available as u32 > AUDIO_QUEUE_CAPACITY_BYTES {
        return;
    }
    let (dma_callback, stream_callback, sample_count) = {
        let mut state = lock_audio_state();
        if !AudioState::dma_admissible(available, address, size) {
            return;
        }
        // SAFETY: This is the same caller-owned sample pointer passed to the C
        // SDL adapter by the original AIInitDMA implementation. The adapter
        // copies it synchronously into SDL before counters change.
        if !unsafe { nookery_audio_host_put(address as *const c_void, size) } {
            return;
        }
        state.record_dma(size);
        (
            state.dma_callback,
            state.stream_callback,
            state.stream_sample_count(),
        )
    };

    // The source invokes the DMA callback first, then the stream callback.
    // SAFETY: The callbacks came from the C ABI registration slots.
    unsafe { dispatch_callbacks(dma_callback, stream_callback, sample_count) };
}

/// `pc/src/pc_audio.c:AIStartDMA`.
#[unsafe(no_mangle)]
pub extern "C" fn AIStartDMA() {
    // SAFETY: The C adapter owns the SDL stream and reports operation failure.
    if unsafe { nookery_audio_host_resume() } {
        lock_audio_state().host_started();
    }
}

/// `pc/src/pc_audio.c:AIStopDMA`.
#[unsafe(no_mangle)]
pub extern "C" fn AIStopDMA() {
    // SAFETY: The C adapter owns the SDL stream and reports operation failure.
    if unsafe { nookery_audio_host_pause() } {
        lock_audio_state().host_stopped();
    }
}

/// `pc/src/pc_audio.c:pc_audio_set_paused`.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_set_paused(paused: i32) {
    if paused != 0 {
        AIStopDMA();
    } else {
        AIStartDMA();
    }
}

/// `pc/src/pc_audio.c:pc_audio_start_producer_thread` is an SDL callback
/// registration seam, not a thread creator.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_start_producer_thread() {}

/// `pc/src/pc_audio.c:pc_audio_get_buffer_fill`, bounded to the queue policy.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_get_buffer_fill() -> i32 {
    let available = if audio_is_active() {
        host_available()
    } else {
        0
    };
    if available <= 0 {
        return 0;
    }
    (available / 2).min(AUDIO_QUEUE_CAPACITY_S16)
}

/// `pc/src/pc_audio.c:pc_audio_is_active`.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_is_active() -> i32 {
    i32::from(audio_is_active())
}

/// Host-visible pause state; DMA enable remains true while paused, matching
/// the source's `audio_stream != NULL` definition.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_is_paused() -> i32 {
    i32::from(lock_audio_state().host_paused())
}

/// `pc/src/pc_audio.c:pc_audio_shutdown`.
#[unsafe(no_mangle)]
pub extern "C" fn pc_audio_shutdown() {
    // SAFETY: Shutdown is idempotent in the C adapter.
    unsafe { nookery_audio_host_shutdown() };
    lock_audio_state().host_shutdown();
}

/// `pc/src/pc_audio.c:AIRegisterDMACallback`.
#[unsafe(no_mangle)]
pub extern "C" fn AIRegisterDMACallback(callback: AIDCallback) -> AIDCallback {
    lock_audio_state().replace_dma_callback(callback)
}

/// `pc/src/pc_audio.c:AIRegisterStreamCallback`.
#[unsafe(no_mangle)]
pub extern "C" fn AIRegisterStreamCallback(callback: AISCallback) -> AISCallback {
    lock_audio_state().replace_stream_callback(callback)
}

/// C `BOOL` is an integer in the Dolphin headers, not Rust's one-byte bool.
#[unsafe(no_mangle)]
pub extern "C" fn AIGetDMAEnableFlag() -> i32 {
    i32::from(audio_is_active())
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetDMABytesLeft() -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetDMAStartAddr() -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetDMALength() -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn AICheckInit() -> i32 {
    i32::from(audio_is_active())
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetStreamSampleCount() -> u32 {
    lock_audio_state().stream_sample_count()
}

#[unsafe(no_mangle)]
pub extern "C" fn AIResetStreamSampleCount() {
    lock_audio_state().reset_stream_sample_count();
}

#[unsafe(no_mangle)]
pub extern "C" fn AISetStreamTrigger(trigger: u32) {
    lock_audio_state().set_stream_trigger(trigger);
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetStreamTrigger() -> u32 {
    lock_audio_state().stream_trigger()
}

#[unsafe(no_mangle)]
pub extern "C" fn AISetStreamPlayState(state: u32) {
    lock_audio_state().set_stream_play_state(state);
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetStreamPlayState() -> u32 {
    lock_audio_state().stream_play_state()
}

#[unsafe(no_mangle)]
pub extern "C" fn AISetDSPSampleRate(rate: u32) {
    lock_audio_state().set_dsp_sample_rate(rate);
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetDSPSampleRate() -> u32 {
    lock_audio_state().dsp_sample_rate()
}

#[unsafe(no_mangle)]
pub extern "C" fn AISetStreamSampleRate(rate: u32) {
    lock_audio_state().set_stream_sample_rate(rate);
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetStreamSampleRate() -> u32 {
    lock_audio_state().stream_sample_rate()
}

#[unsafe(no_mangle)]
pub extern "C" fn AISetStreamVolLeft(volume: u8) {
    lock_audio_state().set_stream_volume_left(volume);
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetStreamVolLeft() -> u8 {
    lock_audio_state().stream_volume_left()
}

#[unsafe(no_mangle)]
pub extern "C" fn AISetStreamVolRight(volume: u8) {
    lock_audio_state().set_stream_volume_right(volume);
}

#[unsafe(no_mangle)]
pub extern "C" fn AIGetStreamVolRight() -> u8 {
    lock_audio_state().stream_volume_right()
}

/// `pc/src/pc_audio.c:AIReset` only shuts down the host stream; all static AI
/// values intentionally persist exactly as they do in the C oracle.
#[unsafe(no_mangle)]
pub extern "C" fn AIReset() {
    pc_audio_shutdown();
}

/// `pc/src/pc_audio.c:DSPCheckMailToDSP` stub.
#[unsafe(no_mangle)]
pub extern "C" fn DSPCheckMailToDSP() -> i32 {
    0
}

/// `pc/src/pc_audio.c:DSPCheckMailFromDSP` stub.
#[unsafe(no_mangle)]
pub extern "C" fn DSPCheckMailFromDSP() -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn DSPReadMailFromDSP() -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn DSPSendMailToDSP(_mail: u32) {}

#[unsafe(no_mangle)]
pub extern "C" fn DSPAssertInt() {}

#[unsafe(no_mangle)]
pub extern "C" fn DSPAddTask(task: *mut c_void) -> *mut c_void {
    task
}
