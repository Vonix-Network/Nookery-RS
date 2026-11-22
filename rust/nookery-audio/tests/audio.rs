use std::sync::atomic::{AtomicU32, Ordering};

use nookery_audio::{
    AUDIO_BYTES_PER_SAMPLE, AUDIO_QUEUE_CAPACITY_BYTES, AUDIO_QUEUE_CAPACITY_S16,
    AUDIO_SAMPLE_RATE_HZ, AudioState, dispatch_callbacks,
};

#[test]
fn audio_contract_starts_at_32khz() {
    assert_eq!(AUDIO_SAMPLE_RATE_HZ, 32_000);
    assert_eq!(AUDIO_BYTES_PER_SAMPLE, 4);
}

#[test]
fn dma_accounting_rejects_zero_invalid_and_overfill() {
    let mut state = AudioState::new();
    assert!(!state.accept_dma(0, 0, 4));
    assert!(!state.accept_dma(-1, 1, 4));
    assert!(!state.accept_dma(0, 1, 0));
    assert!(state.accept_dma(0, 1, 10));
    assert_eq!(state.bytes_written(), 10);
    assert_eq!(state.stream_sample_count(), 2);

    let mut boundary = AudioState::new();
    assert!(boundary.accept_dma((AUDIO_QUEUE_CAPACITY_BYTES - 4) as i32, 1, 4));
    assert!(!boundary.accept_dma((AUDIO_QUEUE_CAPACITY_BYTES - 3) as i32, 1, 4));
    assert_eq!(boundary.bytes_written(), 4);
}

static CALLBACK_ORDER: AtomicU32 = AtomicU32::new(0);
static STREAM_COUNT: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn dma_callback() {
    assert_eq!(CALLBACK_ORDER.swap(1, Ordering::SeqCst), 0);
}

unsafe extern "C" fn replacement_dma_callback() {
    assert_eq!(CALLBACK_ORDER.swap(3, Ordering::SeqCst), 0);
}

unsafe extern "C" fn stream_callback(count: u32) {
    assert_eq!(CALLBACK_ORDER.load(Ordering::SeqCst), 3);
    STREAM_COUNT.store(count, Ordering::SeqCst);
}

#[test]
fn callback_replacement_returns_previous_and_dispatches_in_order() {
    let mut state = AudioState::new();
    assert!(state.replace_dma_callback(Some(dma_callback)).is_none());
    let previous = state.replace_dma_callback(Some(replacement_dma_callback));
    assert!(previous.is_some());
    // The returned pointer is the callback that was replaced.
    CALLBACK_ORDER.store(0, Ordering::SeqCst);
    // SAFETY: `previous` came directly from the registration slot above.
    unsafe { previous.expect("previous DMA callback")() };
    assert_eq!(CALLBACK_ORDER.load(Ordering::SeqCst), 1);
    assert!(
        state
            .replace_stream_callback(Some(stream_callback))
            .is_none()
    );
    CALLBACK_ORDER.store(0, Ordering::SeqCst);
    STREAM_COUNT.store(0, Ordering::SeqCst);
    // SAFETY: These are valid callbacks defined immediately above.
    unsafe {
        dispatch_callbacks(
            state.replace_dma_callback(None),
            state.replace_stream_callback(None),
            7,
        )
    };
    assert_eq!(CALLBACK_ORDER.load(Ordering::SeqCst), 3);
    assert_eq!(STREAM_COUNT.load(Ordering::SeqCst), 7);
}

#[test]
fn master_volume_and_host_lifecycle_are_bounded_and_stateful() {
    let mut state = AudioState::new();
    assert_eq!(state.set_master_volume(-1), 0);
    assert_eq!(state.set_master_volume(101), 100);
    assert_eq!(state.master_volume(), 100);
    state.host_initialized();
    assert!(state.host_active());
    assert!(state.host_paused());
    state.host_started();
    assert!(!state.host_paused());
    state.host_stopped();
    assert!(state.host_paused());
    state.host_shutdown();
    assert!(!state.host_active());
    assert!(!state.host_paused());
}

#[test]
fn ai_state_getters_setters_and_reset_semantics_match_oracle() {
    let mut state = AudioState::new();
    state.set_dsp_sample_rate(1);
    state.set_stream_sample_rate(1);
    state.set_stream_trigger(44);
    state.set_stream_play_state(1);
    state.set_stream_volume_left(12);
    state.set_stream_volume_right(34);
    assert_eq!(state.dsp_sample_rate(), 1);
    assert_eq!(state.stream_sample_rate(), 1);
    assert_eq!(state.stream_trigger(), 44);
    assert_eq!(state.stream_play_state(), 1);
    assert_eq!(state.stream_volume_left(), 0xff);
    assert_eq!(state.stream_volume_right(), 0xff);

    state.record_dma(8);
    state.reset_stream_sample_count();
    assert_eq!(state.stream_sample_count(), 0);
    state.host_initialized();
    state.host_shutdown();
    assert_eq!(state.dsp_sample_rate(), 1);
    assert_eq!(state.stream_trigger(), 44);
}

#[test]
fn refill_stops_on_no_progress_and_fill_is_bounded_in_s16_units() {
    assert_eq!(AudioState::refill_remaining(16, 10, 18), Some(8));
    assert_eq!(AudioState::refill_remaining(16, 10, 10), None);
    assert_eq!(AudioState::refill_remaining(0, 10, 18), None);
    assert_eq!(AUDIO_QUEUE_CAPACITY_S16, 128_000);
}
