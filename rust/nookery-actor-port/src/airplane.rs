//! Pure state transitions from `src/actor/ac_airplane.c`.
//!
//! The renderer, collision clip, joystick, and effect clip stay outside this
//! module.  The state mutations performed by the source's frame-rate helpers,
//! fly selection, ground friction, and wall transition are represented here.

/// The source `aAp_STATUS_*` values, in dispatch-table order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum AirplaneStatus {
    /// Free flight.
    FreeFlyMove = 0,
    /// First falling-flight state.
    FallFlyMove = 1,
    /// Second falling-flight state.
    FallFlyMove2 = 2,
    /// Somersault flight.
    SomerFlyMove = 3,
    /// The player has started releasing the plane.
    StartFlyMove = 4,
    /// The plane is stopped on the ground.
    StopFlyMove = 5,
    /// The player is holding the plane.
    PlayerCatch = 6,
}

/// The source `aAp_TILT_*` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum TiltStatus {
    /// Nose-down phase.
    Down = 0,
    /// Nose-up phase.
    Up = 1,
}

/// The behavior-bearing fields used by the portable airplane transitions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirplaneState {
    /// Current movement status.
    pub status: AirplaneStatus,
    /// Current somersault phase.
    pub tilt_status: TiltStatus,
    /// Horizontal speed.
    pub speed: f32,
    /// X rotation in degrees.
    pub rot_x: f32,
    /// Z rotation in degrees.
    pub rot_z: f32,
    /// Ground dust timer.
    pub ground_timer: i16,
    /// Fractional 60-Hz tick accumulator for ground dust.
    pub ground_effect_accum: f32,
    /// Number of dust effects requested by the modeled ground step.
    pub dust_effects: u32,
}

impl Default for AirplaneState {
    fn default() -> Self {
        Self {
            status: AirplaneStatus::FreeFlyMove,
            tilt_status: TiltStatus::Down,
            speed: 0.0,
            rot_x: 0.0,
            rot_z: 0.0,
            ground_timer: 0,
            ground_effect_accum: 0.0,
            dust_effects: 0,
        }
    }
}

/// Source-equivalent `aAp_RubberMove`.
pub fn rubber_move(now: &mut f32, target: f32, step: f32, dt_num_60fps_frames: f32) {
    let dt_step = 1.0 - (1.0 - step).powf(dt_num_60fps_frames);
    *now += dt_step * (target - *now);
}

/// Source-equivalent `aAp_SelectFly` with the random draw supplied by the
/// caller.  The C helper always returns `TRUE`; the Rust result is therefore
/// `true` for every input while preserving both status mutations.
pub fn select_fly(state: &mut AirplaneState, stick_y: i32, random_draw: f32) -> bool {
    let mode = ((random_draw * 7.0) as i32) & 1;
    if stick_y > 67 {
        if mode != 0 {
            state.status = AirplaneStatus::FallFlyMove2;
        } else {
            state.status = AirplaneStatus::SomerFlyMove;
            state.tilt_status = TiltStatus::Down;
        }
    } else {
        state.status = AirplaneStatus::FreeFlyMove;
    }
    true
}

/// Source-equivalent `aAp_GetBgCheckOffsetY`.
pub const fn bg_check_offset_y(status: AirplaneStatus) -> f32 {
    match status {
        AirplaneStatus::FallFlyMove | AirplaneStatus::StopFlyMove => -5.5,
        _ => 0.0,
    }
}

/// Port the source's ground-friction branch and its observable state changes.
///
/// `grounded` and `wall_hit` are the results that the engine collision clip
/// supplies to the C actor.  Dust creation is represented by a counter rather
/// than a renderer callback; the source increments the same timer before each
/// effect request.
pub fn free_fly_move(
    state: &mut AirplaneState,
    dt_num_60fps_frames: f32,
    grounded: bool,
    wall_hit: bool,
) {
    rubber_move(&mut state.speed, 3.0, 0.01, dt_num_60fps_frames);
    rubber_move(&mut state.rot_x, 19.0, 0.045, dt_num_60fps_frames);
    rubber_move(&mut state.rot_z, 0.0, 0.05, dt_num_60fps_frames);

    if grounded {
        rubber_move(&mut state.speed, 0.0, 0.6, dt_num_60fps_frames);
        if state.speed < 1.0 {
            state.speed = 0.0;
            state.status = AirplaneStatus::StopFlyMove;

            let mut ticks_accum = state.ground_effect_accum + dt_num_60fps_frames;
            let ticks = ticks_accum as i32;
            ticks_accum -= ticks as f32;
            state.ground_effect_accum = ticks_accum;
            for _ in 0..ticks.clamp(0, 4) {
                if state.ground_timer >= 5 {
                    state.ground_timer = 0;
                    state.dust_effects += 1;
                }
                state.ground_timer += 1;
            }
        }
    }

    if wall_hit {
        state.status = AirplaneStatus::FallFlyMove;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_status_selection_preserves_threshold_and_random_branch() {
        let mut state = AirplaneState::default();
        assert!(select_fly(&mut state, 67, 0.99));
        assert_eq!(state.status, AirplaneStatus::FreeFlyMove);
        assert!(select_fly(&mut state, 68, 0.0));
        assert_eq!(state.status, AirplaneStatus::SomerFlyMove);
        assert!(select_fly(&mut state, 68, 0.5));
        assert_eq!(state.status, AirplaneStatus::FallFlyMove2);
    }

    #[test]
    fn ground_friction_and_wall_transition_follow_source_order() {
        let mut state = AirplaneState {
            speed: 0.5,
            ..AirplaneState::default()
        };
        free_fly_move(&mut state, 1.0, true, true);
        assert_eq!(state.status, AirplaneStatus::FallFlyMove);
        assert_eq!(state.speed, 0.0);
        assert_eq!(bg_check_offset_y(AirplaneStatus::FallFlyMove), -5.5);
    }
}
