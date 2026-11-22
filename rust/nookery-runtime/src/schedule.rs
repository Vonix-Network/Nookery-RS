use crate::{BASE_FRAME_RATE, MAX_DT_60FPS_FRAMES};
use nookery_game::FrameClock;

/// Per-frame timing after the C graph clamp and optional PC speed multiplier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub delta_seconds: f64,
    pub delta_60fps_frames: f64,
    pub total_60fps_frames: f64,
}

impl Timing {
    pub(crate) fn from_delta(
        raw_delta_seconds: f64,
        total_60fps_frames: f64,
        speed_multiplier: f64,
    ) -> Option<Self> {
        if !raw_delta_seconds.is_finite()
            || raw_delta_seconds < 0.0
            || !speed_multiplier.is_finite()
            || speed_multiplier <= 0.0
        {
            return None;
        }
        let mut frames = raw_delta_seconds * BASE_FRAME_RATE;
        if !frames.is_finite() {
            return None;
        }
        if frames > MAX_DT_60FPS_FRAMES {
            frames = MAX_DT_60FPS_FRAMES;
        }
        frames *= speed_multiplier;
        let total_60fps_frames = total_60fps_frames + frames;
        if !frames.is_finite() || FrameClock::frame_time(total_60fps_frames).is_err() {
            return None;
        }
        Some(Self {
            delta_seconds: frames / BASE_FRAME_RATE,
            delta_60fps_frames: frames,
            total_60fps_frames,
        })
    }
}

/// Independent accumulator used for audio game ticks and other 60 Hz sites.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioSchedule {
    accumulator: f64,
}

impl AudioSchedule {
    pub const fn new() -> Self {
        Self { accumulator: 0.0 }
    }

    /// Match graph_dt_60hz_ticks: subtract all whole ticks, then cap the
    /// returned work to four ticks to avoid a spiral after a stall.
    pub fn ticks(&mut self, delta_60fps_frames: f64) -> u32 {
        if delta_60fps_frames == 0.0 {
            return 0;
        }
        FrameClock::ticks_for_frame(delta_60fps_frames, &mut self.accumulator).unwrap_or_default()
    }

    pub const fn accumulator(self) -> f64 {
        self.accumulator
    }
}

impl Default for AudioSchedule {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn period_elapsed(accumulator: &mut f64, delta: f64, period: f64) -> bool {
    if !period.is_finite() || period <= 0.0 {
        return false;
    }
    *accumulator += delta;
    if *accumulator >= period {
        *accumulator -= period;
        if *accumulator > period * 4.0 {
            *accumulator = 0.0;
        }
        true
    } else {
        false
    }
}
