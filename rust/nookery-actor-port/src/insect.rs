//! Shared insect lifetime and fade behavior from `ac_insect_move.c_inc`.

/// The source insect type used by the firefly-specific alpha branch.
pub const INSECT_TYPE_FIREFLY: i32 = 27;
/// `aINS_PATIENCE_STEP` from `include/ac_insect_h.h`.
pub const PATIENCE_STEP: f32 = 0.5;

const DESTRUCT: u8 = 1 << 0;
const LIFE_EXPIRED: u8 = 1 << 3;

/// Behavior-bearing subset of `aINS_INSECT_ACTOR` used by the common move
/// code.  The remaining fields are owned by each species actor and are not
/// silently fabricated here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InsectState {
    /// Species selector.
    pub insect_type: i32,
    /// Current stress-adjusted patience.
    pub patience: f32,
    /// Remaining visible lifetime.
    pub life_time: f32,
    /// Remaining alpha-fade time after lifetime expiry.
    pub alpha_time: f32,
    /// Firefly/default alpha channels.
    pub alpha0: i32,
    /// Firefly second alpha channel.
    pub alpha1: i32,
    /// Firefly third alpha channel.
    pub alpha2: i32,
    /// Packed source flags, including `destruct` and `bit_3`.
    pub flags: u8,
}

impl Default for InsectState {
    fn default() -> Self {
        Self {
            insect_type: 0,
            patience: 0.0,
            life_time: 0.0,
            alpha_time: 0.0,
            alpha0: 0,
            alpha1: 0,
            alpha2: 0,
            flags: 0,
        }
    }
}

/// Whether the source's `destruct` bit is set.
pub const fn is_destructed(state: &InsectState) -> bool {
    state.flags & DESTRUCT != 0
}

/// Whether the source's lifetime-expired `bit_3` is set.
pub const fn is_lifetime_expired(state: &InsectState) -> bool {
    state.flags & LIFE_EXPIRED != 0
}

/// Port `aINS_calc_patience` exactly, including the source's 0.008 zero band.
pub fn calc_patience(state: &mut InsectState, stress: f32, dt_num_60fps_frames: f32) {
    if stress.abs() < 0.008 {
        state.patience -= PATIENCE_STEP * dt_num_60fps_frames;
        if state.patience < 0.0 {
            state.patience = 0.0;
        }
    } else {
        state.patience += stress * PATIENCE_STEP;
        if state.patience > 100.0 {
            state.patience = 100.0;
        }
    }
}

/// Port `aINS_calc_life_time`, preserving the failure/expiry mutation.
pub fn calc_life_time(state: &mut InsectState, dt_num_60fps_frames: f32) {
    state.life_time -= dt_num_60fps_frames;
    if state.life_time <= 0.0 {
        state.flags |= LIFE_EXPIRED;
        state.life_time = 0.0;
    }
}

/// Port `aINS_calc_alpha_time`, including firefly's separate alpha channel.
pub fn calc_alpha_time(state: &mut InsectState, dt_num_60fps_frames: f32) {
    if state.life_time != 0.0 || state.alpha_time <= 0.0 {
        return;
    }

    state.alpha_time -= dt_num_60fps_frames;
    let fade_frames = 24.0 - state.alpha_time;
    if fade_frames <= 0.0 {
        return;
    }

    if state.insect_type == INSECT_TYPE_FIREFLY {
        let mut alpha = (fade_frames * 11.0) as i32;
        if alpha >= 255 {
            alpha = 255;
            state.alpha0 = 0;
            state.alpha1 = 0;
            state.flags |= DESTRUCT;
        }
        state.alpha2 = alpha;
    } else {
        let mut alpha = 255 - (fade_frames * 11.0) as i32;
        if alpha <= 0 {
            alpha = 0;
            state.flags |= DESTRUCT;
        }
        state.alpha0 = alpha;
    }
}

/// Apply the three common per-frame transitions in source order.
pub fn tick(state: &mut InsectState, stress: f32, dt_num_60fps_frames: f32) {
    calc_patience(state, stress, dt_num_60fps_frames);
    calc_life_time(state, dt_num_60fps_frames);
    calc_alpha_time(state, dt_num_60fps_frames);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_lifetime_sets_bit_and_clamps_before_fade() {
        let mut state = InsectState {
            life_time: 0.5,
            alpha_time: 24.0,
            alpha0: 255,
            ..InsectState::default()
        };
        tick(&mut state, 0.0, 1.0);
        assert_eq!(state.life_time, 0.0);
        assert!(is_lifetime_expired(&state));
        assert_eq!(state.alpha0, 244);
    }

    #[test]
    fn firefly_fade_uses_alpha2_and_destroys_at_the_end() {
        let mut state = InsectState {
            insect_type: INSECT_TYPE_FIREFLY,
            alpha_time: 1.0,
            ..InsectState::default()
        };
        calc_alpha_time(&mut state, 1.0);
        assert_eq!(state.alpha2, 255);
        assert!(is_destructed(&state));
        assert_eq!(state.alpha0, 0);
        assert_eq!(state.alpha1, 0);
    }
}
