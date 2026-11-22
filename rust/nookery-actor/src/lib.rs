//! Pure Rust candidate for the player-owned balloon actor.
//!
//! The oracle is [`src/actor/ac_balloon.c`](../../src/actor/ac_balloon.c) and
//! [`include/ac_balloon.h`](../../include/ac_balloon.h). This crate deliberately
//! has no C or CMake linkage: callers can exercise the lifecycle and its
//! observable asset/animation/render callbacks while the original actor remains
//! the active game implementation.
//!
//! The byte layout below mirrors `BALLOON_ACTOR` exactly as laid out by the
//! 32-bit game ABI. The first `0x174` bytes are the shared `ACTOR`; opaque
//! keyframe/matrix bytes retain their source allocation sizes, and the named
//! fields follow the header's order. Gameplay accessors operate on those bytes
//! so unknown state is not silently discarded.

use std::fmt;

/// `mAc_PROFILE_BALLOON`, counted from `mAc_PROFILE_PLAYER` in
/// `include/m_actor.h:790-1021`.
pub const PROFILE_ID: u16 = 230;
/// `ACTOR_PART_BG` from `include/m_actor.h:779-785`.
pub const PART_BG: u8 = 3;
/// `ACTOR_STATE_NO_DRAW_WHILE_CULLED | ACTOR_STATE_NO_MOVE_WHILE_CULLED`.
pub const INITIAL_FLAGS: u32 = (1 << 5) | (1 << 4);
/// `ITM_BALLOON_START`, the profile's world identifier.
pub const NPC_ID: u16 = 0x2244;
/// `ACTOR_OBJ_BANK_KEEP` from `include/m_actor.h:42-47`.
pub const OBJECT_BANK_KEEP: i16 = 3;
pub const BALLOON_TYPE_COUNT: i32 = 8;
pub const JOINT_COUNT: usize = 8;
pub const DATA_BALLOON1: u16 = 23;
pub const DATA_BALLOON_WAIT: u16 = 31;
pub const DATA_BALLOON_GYAZA: u16 = 32;

const MODE_HIDE: i32 = 0;
const MODE_FLY: i32 = 1;
const MODE_NUM: i32 = 2;

const ACTOR_BYTES: usize = 0x174;
const KEYFRAME_BYTES: usize = 0x70;
const MATRIX_BYTES: usize = 0x40;

// ACTOR offsets from include/m_actor.h:1104-1152. These are intentionally
// explicit: the actor's C base contains many pointer/function fields that are
// not safe to reinterpret as host Rust pointers.
const ACTOR_WORLD_POSITION: usize = 0x28;
const ACTOR_WORLD_ANGLE_Y: usize = 0x36;
const ACTOR_SHAPE_ROTATION: usize = 0xdc;
const ACTOR_SCALE: usize = 0x5c;
const ACTOR_POSITION_SPEED: usize = 0x68;
const ACTOR_GRAVITY: usize = 0x78;
const ACTOR_MAX_VELOCITY_Y: usize = 0x7c;

/// Byte offsets within [`BalloonActorLayout`] for the named tail fields.
pub const MAIN_MODE_OFFSET: usize =
    ACTOR_BYTES + KEYFRAME_BYTES + 0x30 + 0x30 + 4 + MATRIX_BYTES * 8;
pub const SAVED_TYPE_OFFSET: usize = MAIN_MODE_OFFSET + 4;
pub const SETUP_MODE_OFFSET: usize = SAVED_TYPE_OFFSET + 4;
pub const BALLOON_TYPE_OFFSET: usize = SETUP_MODE_OFFSET + 4;
pub const ANGLE_OFFSET: usize = BALLOON_TYPE_OFFSET + 4;
pub const LEAN_OFFSET: usize = ANGLE_OFFSET + 6;
pub const START_FRAME_OFFSET: usize = LEAN_OFFSET + 2;
pub const SPEED_OFFSET: usize = START_FRAME_OFFSET + 4;
pub const POS_OFFSET: usize = SPEED_OFFSET + 4;

/// C `s_xyz` mirror.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortAngles {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

/// C `xyz_t` mirror.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// The source allocation and field order of `BALLOON_ACTOR` in
/// `include/ac_balloon.h:29-47`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BalloonActorLayout {
    pub actor: [u8; ACTOR_BYTES],
    pub keyframe: [u8; KEYFRAME_BYTES],
    pub work: [[i16; 3]; JOINT_COUNT],
    pub morph: [[i16; 3]; JOINT_COUNT],
    pub unknown_244: i32,
    pub matrices: [[u8; MATRIX_BYTES]; 8],
    pub main_mode: i32,
    pub saved_type: i32,
    pub setup_mode: i32,
    pub balloon_type: i32,
    pub angle: ShortAngles,
    pub lean: i16,
    pub start_frame: f32,
    pub speed: f32,
    pub pos: Position,
    pub unknown_474: i32,
}

impl Default for BalloonActorLayout {
    fn default() -> Self {
        Self {
            actor: [0; ACTOR_BYTES],
            keyframe: [0; KEYFRAME_BYTES],
            work: [[0; 3]; JOINT_COUNT],
            morph: [[0; 3]; JOINT_COUNT],
            unknown_244: 0,
            matrices: [[0; MATRIX_BYTES]; 8],
            main_mode: 0,
            saved_type: 0,
            setup_mode: 0,
            balloon_type: 0,
            angle: ShortAngles::default(),
            lean: 0,
            start_frame: 0.0,
            speed: 0.0,
            pos: Position::default(),
            unknown_474: 0,
        }
    }
}

/// Lifecycle mode from `ac_balloon.c:8-12`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Hide,
    Fly,
}

/// The setup-time animation initialization observable at the game boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationInit {
    pub animation_id: u16,
    pub start_frame: f32,
    pub speed: f32,
    pub morph: f32,
}

/// Render callback state corresponding to `Ac_Balloon_draw_Before/After`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JointRenderCallback {
    pub joint: usize,
    pub tex_edge_alpha: u8,
}

/// The callbacks that replace the actor's observable engine calls.
///
/// They are intentionally narrow: asset lookup/animation, player anchoring,
/// and the three-joint render hooks are the calls made by the oracle. No C ABI
/// or fake renderer is introduced.
pub trait BalloonCallbacks {
    fn player_position(&mut self) -> Option<Position>;
    fn keyframe_construct(&mut self, skeleton_id: u16, joint_count: usize);
    fn keyframe_init(&mut self, init: AnimationInit);
    fn keyframe_play(&mut self);
    fn joint_render_callback(&mut self, callback: JointRenderCallback);
}

/// A pure Rust actor instance whose backing storage has the C actor's exact
/// field order and size.
#[derive(Clone, Copy, Default)]
pub struct BalloonActor {
    layout: BalloonActorLayout,
}

impl fmt::Debug for BalloonActor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BalloonActor")
            .field("mode", &self.mode())
            .field("setup_mode", &self.setup_mode_raw())
            .field("balloon_type", &self.balloon_type())
            .field("position", &self.world_position())
            .finish()
    }
}

impl BalloonActor {
    /// Construct the actor as `Actor_info_make_actor` + `Ac_Balloon_ct` do.
    /// The first `tick` performs the pending hide setup and follows it with
    /// hidden-mode movement, matching `Ac_Balloon_main`'s two-table ordering.
    pub fn new(home_position: Position, home_angle: ShortAngles) -> Self {
        let mut actor = Self::default();
        actor.set_i16(ACTOR_SHAPE_ROTATION, home_angle.x);
        actor.set_i16(ACTOR_SHAPE_ROTATION + 2, home_angle.y);
        actor.set_i16(ACTOR_SHAPE_ROTATION + 4, home_angle.z);
        actor.set_position(ACTOR_WORLD_POSITION, home_position);
        actor.set_i16(ACTOR_WORLD_ANGLE_Y, home_angle.y);
        actor.set_position(
            ACTOR_SCALE,
            Position {
                x: 0.01,
                y: 0.01,
                z: 0.01,
            },
        );
        actor.set_f32(ACTOR_MAX_VELOCITY_Y, -20.0);
        actor.layout.setup_mode = MODE_HIDE;
        actor
    }

    pub fn layout(&self) -> &BalloonActorLayout {
        &self.layout
    }

    pub fn mode(&self) -> Option<Mode> {
        match self.layout.main_mode {
            MODE_HIDE => Some(Mode::Hide),
            MODE_FLY => Some(Mode::Fly),
            _ => None,
        }
    }

    pub fn setup_mode_raw(&self) -> i32 {
        self.layout.setup_mode
    }

    pub fn balloon_type(&self) -> i32 {
        self.layout.balloon_type
    }

    pub fn saved_type(&self) -> i32 {
        self.layout.saved_type
    }

    pub fn world_position(&self) -> Position {
        self.position_at(ACTOR_WORLD_POSITION)
    }

    pub fn shape_rotation(&self) -> ShortAngles {
        ShortAngles {
            x: self.i16_at(ACTOR_SHAPE_ROTATION),
            y: self.i16_at(ACTOR_SHAPE_ROTATION + 2),
            z: self.i16_at(ACTOR_SHAPE_ROTATION + 4),
        }
    }

    pub fn position_speed(&self) -> Position {
        self.position_at(ACTOR_POSITION_SPEED)
    }

    pub fn request_hide(&mut self) {
        self.layout.setup_mode = MODE_HIDE;
    }

    /// Port of `Ac_Balloon_request_fly` (`ac_balloon.c:44-54`). The C function
    /// accepts out-of-range types and stores them; fallback happens only during
    /// setup, so this method deliberately does not validate `balloon_type`.
    pub fn request_fly(
        &mut self,
        balloon_type: i32,
        angle: ShortAngles,
        lean: i16,
        pos: Position,
        start_frame: f32,
        speed: f32,
    ) {
        self.layout.setup_mode = MODE_FLY;
        self.layout.balloon_type = balloon_type;
        self.layout.angle = angle;
        self.layout.lean = lean;
        self.layout.start_frame = start_frame;
        self.layout.speed = speed;
        self.layout.pos = pos;
    }

    /// Port of `Ac_Balloon_main` (`ac_balloon.c:125-143`). Setup runs first,
    /// then the current main mode runs in the same tick. Invalid raw modes are
    /// ignored exactly as the C bounds checks ignore them.
    pub fn tick(&mut self, dt_num_60fps_frames: f32, callbacks: &mut impl BalloonCallbacks) {
        if (0..MODE_NUM).contains(&self.layout.setup_mode) {
            match self.layout.setup_mode {
                MODE_HIDE => self.setup_hide(),
                MODE_FLY => self.setup_fly(callbacks),
                _ => unreachable!("MODE_NUM bounds preserve the two C table entries"),
            }
        }

        if (0..MODE_NUM).contains(&self.layout.main_mode) {
            match self.layout.main_mode {
                MODE_HIDE => self.main_hide(callbacks),
                MODE_FLY => self.main_fly(dt_num_60fps_frames, callbacks),
                _ => unreachable!("MODE_NUM bounds preserve the two C table entries"),
            }
        }
    }

    /// Port of `Ac_Balloon_draw` (`ac_balloon.c:217-223`). Hidden actors do
    /// not invoke any render callback; flying actors invoke before/after for
    /// joints 1, 2, and 3 in that order.
    pub fn draw(&self, callbacks: &mut impl BalloonCallbacks) {
        if self.layout.main_mode != MODE_HIDE {
            for joint in 1..=3 {
                callbacks.joint_render_callback(JointRenderCallback {
                    joint,
                    tex_edge_alpha: 80,
                });
            }
            for joint in 1..=3 {
                callbacks.joint_render_callback(JointRenderCallback {
                    joint,
                    tex_edge_alpha: 144,
                });
            }
        }
    }

    fn setup_hide(&mut self) {
        self.layout.main_mode = MODE_HIDE;
        self.layout.setup_mode = -1;
    }

    fn main_hide(&mut self, callbacks: &mut impl BalloonCallbacks) {
        if let Some(player) = callbacks.player_position() {
            self.set_position(ACTOR_WORLD_POSITION, player);
        }
    }

    fn setup_fly(&mut self, callbacks: &mut impl BalloonCallbacks) {
        let balloon_type = self.layout.balloon_type;
        let skeleton_id = if (0..BALLOON_TYPE_COUNT).contains(&balloon_type) {
            DATA_BALLOON1 + balloon_type as u16
        } else {
            DATA_BALLOON1
        };
        let start_frame = self.layout.start_frame;
        let initial_animation = if start_frame < 1.0 {
            DATA_BALLOON_WAIT
        } else {
            DATA_BALLOON_GYAZA
        };

        self.layout.main_mode = MODE_FLY;
        self.layout.saved_type = balloon_type;
        callbacks.keyframe_construct(skeleton_id, JOINT_COUNT);
        callbacks.keyframe_init(AnimationInit {
            animation_id: initial_animation,
            start_frame,
            speed: 0.0,
            morph: 0.0,
        });
        callbacks.keyframe_play();
        callbacks.keyframe_init(AnimationInit {
            animation_id: DATA_BALLOON_WAIT,
            start_frame: 1.0,
            speed: 0.5,
            morph: -5.0,
        });
        self.set_short_angles(ACTOR_SHAPE_ROTATION, self.layout.angle);
        self.set_f32(ACTOR_POSITION_SPEED + 4, 0.0);
        self.set_f32(ACTOR_MAX_VELOCITY_Y, self.layout.speed);
        self.set_f32(ACTOR_GRAVITY, 0.2);
        self.set_position(ACTOR_WORLD_POSITION, self.layout.pos);
        self.layout.setup_mode = -1;
    }

    fn main_fly(&mut self, dt: f32, callbacks: &mut impl BalloonCallbacks) {
        callbacks.keyframe_play();
        self.move_fly(dt);
        if let Some(player) = callbacks.player_position() {
            if self.world_position().y - player.y > 200.0 {
                self.request_hide();
            }
        }
    }

    fn move_fly(&mut self, dt: f32) {
        let fraction = 1.0 - 0.5_f32.sqrt();
        let mut rotation = self.shape_rotation();
        rotation.x = calc_short_angle(rotation.x, 0, fraction, 50, 5, dt);
        rotation.z = calc_short_angle(rotation.z, 0, fraction, 50, 5, dt);
        let lean = calc_short_angle(self.layout.lean, 0, fraction, 50, 5, dt);
        self.set_short_angles(ACTOR_SHAPE_ROTATION, rotation);
        self.layout.lean = lean;

        let speed = self.layout.speed;
        let world_angle_y = self.i16_at(ACTOR_WORLD_ANGLE_Y);
        let mut velocity = self.position_speed();
        velocity.x = speed * sin_short(world_angle_y);
        velocity.z = speed * cos_short(world_angle_y);
        velocity.y = chase_float(
            velocity.y,
            self.f32_at(ACTOR_MAX_VELOCITY_Y),
            0.5 * self.f32_at(ACTOR_GRAVITY) * dt,
        );
        self.set_position(ACTOR_POSITION_SPEED, velocity);
        let mut position = self.world_position();
        position.x += 0.5 * dt * velocity.x;
        position.y += 0.5 * dt * velocity.y;
        position.z += 0.5 * dt * velocity.z;
        self.set_position(ACTOR_WORLD_POSITION, position);
    }

    fn i16_at(&self, offset: usize) -> i16 {
        i16::from_ne_bytes(self.layout.actor[offset..offset + 2].try_into().unwrap())
    }

    fn f32_at(&self, offset: usize) -> f32 {
        f32::from_ne_bytes(self.layout.actor[offset..offset + 4].try_into().unwrap())
    }

    fn set_i16(&mut self, offset: usize, value: i16) {
        self.layout.actor[offset..offset + 2].copy_from_slice(&value.to_ne_bytes());
    }

    fn set_f32(&mut self, offset: usize, value: f32) {
        self.layout.actor[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }

    fn position_at(&self, offset: usize) -> Position {
        Position {
            x: self.f32_at(offset),
            y: self.f32_at(offset + 4),
            z: self.f32_at(offset + 8),
        }
    }

    fn set_position(&mut self, offset: usize, value: Position) {
        self.set_f32(offset, value.x);
        self.set_f32(offset + 4, value.y);
        self.set_f32(offset + 8, value.z);
    }

    fn set_short_angles(&mut self, offset: usize, value: ShortAngles) {
        self.set_i16(offset, value.x);
        self.set_i16(offset + 2, value.y);
        self.set_i16(offset + 4, value.z);
    }
}

fn sin_short(angle: i16) -> f32 {
    (f32::from(angle) * std::f32::consts::PI / 32768.0).sin()
}

fn cos_short(angle: i16) -> f32 {
    (f32::from(angle) * std::f32::consts::PI / 32768.0).cos()
}

fn chase_float(mut value: f32, target: f32, step: f32) -> f32 {
    if step != 0.0 {
        let signed_step = if value > target { -step } else { step };
        value += signed_step;
        if signed_step * (value - target) >= 0.0 {
            target
        } else {
            value
        }
    } else {
        value
    }
}

fn calc_short_angle(
    value: i16,
    target: i16,
    fraction: f32,
    max_step: i16,
    min_step: i16,
    dt: f32,
) -> i16 {
    if value == target {
        return value;
    }
    // `ac_balloon.c` delegates to `add_calc_short_angle2`, whose `s16 diff`
    // assignment wraps the subtraction before the float calculation.
    let diff = i32::from(target.wrapping_sub(value));
    let dt_fraction = 1.0 - (1.0 - fraction).powf(dt);
    let dt_max_step = (f32::from(max_step) * dt).max(1.0);
    let dt_min_step = (f32::from(min_step) * dt).max(1.0);
    let mut step = (diff as f32) * dt_fraction;
    if !(step > dt_min_step || step < -dt_min_step) {
        step = if diff >= 0 { dt_min_step } else { -dt_min_step };
    } else if step > dt_max_step {
        step = dt_max_step;
    } else if step < -dt_max_step {
        step = -dt_max_step;
    }
    let step = step as i32;
    let step = if step == 0 && diff != 0 {
        if diff > 0 { 1 } else { -1 }
    } else {
        step
    };
    let next = value.wrapping_add(step as i16);
    let remaining = target.wrapping_sub(next);
    if (step > 0 && remaining < 0) || (step <= 0 && remaining > 0) {
        target
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Trace {
        player: Option<Position>,
        events: Vec<String>,
    }

    impl BalloonCallbacks for Trace {
        fn player_position(&mut self) -> Option<Position> {
            self.player
        }

        fn keyframe_construct(&mut self, skeleton_id: u16, joint_count: usize) {
            self.events
                .push(format!("construct:{skeleton_id}:{joint_count}"));
        }

        fn keyframe_init(&mut self, init: AnimationInit) {
            self.events.push(format!(
                "init:{}:{:.1}:{:.1}:{:.1}",
                init.animation_id, init.start_frame, init.speed, init.morph
            ));
        }

        fn keyframe_play(&mut self) {
            self.events.push("play".into());
        }

        fn joint_render_callback(&mut self, callback: JointRenderCallback) {
            self.events.push(format!(
                "draw:{}:{}",
                callback.joint, callback.tex_edge_alpha
            ));
        }
    }

    #[test]
    fn c_header_layout_is_preserved() {
        assert_eq!(std::mem::size_of::<BalloonActorLayout>(), 0x478);
        assert_eq!(std::mem::offset_of!(BalloonActorLayout, main_mode), 0x448);
        assert_eq!(
            std::mem::offset_of!(BalloonActorLayout, balloon_type),
            0x454
        );
        assert_eq!(std::mem::offset_of!(BalloonActorLayout, pos), 0x468);
        assert_eq!(std::mem::offset_of!(BalloonActorLayout, unknown_474), 0x474);
    }

    #[test]
    fn invalid_type_is_stored_then_falls_back_only_during_setup() {
        let mut actor = BalloonActor::new(Position::default(), ShortAngles::default());
        actor.request_fly(-1, ShortAngles::default(), 0, Position::default(), 0.0, 7.0);
        let mut trace = Trace::default();
        actor.tick(1.0, &mut trace);
        assert_eq!(actor.balloon_type(), -1);
        assert_eq!(actor.saved_type(), -1);
        assert_eq!(trace.events[0], "construct:23:8");
    }

    #[test]
    fn invalid_modes_are_ignored_by_both_dispatch_tables() {
        let mut actor = BalloonActor::new(Position::default(), ShortAngles::default());
        actor.layout.setup_mode = MODE_NUM;
        actor.layout.main_mode = MODE_NUM;
        let mut trace = Trace::default();
        actor.tick(1.0, &mut trace);
        assert!(trace.events.is_empty());
        assert_eq!(actor.setup_mode_raw(), MODE_NUM);
    }

    #[test]
    fn short_angle_subtraction_preserves_c_s16_wrap_at_boundaries() {
        assert_eq!(calc_short_angle(i16::MAX, -1, 1.0, 50, 5, 1.0), 32717);
        assert_eq!(calc_short_angle(i16::MIN, 0, 1.0, 50, 5, 1.0), 32718);
    }
}
