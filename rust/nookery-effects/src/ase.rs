//! The `ASE`/`ASE_CH` vertical slice.
//!
//! C locations used as the oracle:
//!
//! * `src/effect/ef_ase.c:21-45`: parent profile, constructor, continuous
//!   re-arm, and odd-timer child creation.
//! * `src/effect/ef_ase_ch.c:9-75`: child profile, gravity, motion, and
//!   lifetime-driven draw scale.
//! * `src/effect/ef_effect_control.c:172-193,195-237`: public make/kill
//!   dispatch and immediate-death cleanup.
//! * `src/effect/ef_effect_control.c:411-463`: lifetime pass, smooth/non-smooth
//!   update order, four-step catch-up cap, timer decrement, and expiry cleanup.
//! * `src/effect/ef_effect_control.c:475-549`: the fixed 100-slot allocation
//!   search, priority-byte behavior, initialization order, and high-water mark.

use std::f32::consts::PI;

/// The C controller's fixed effect-slot capacity.
pub const ACTIVE_EFFECT_MAX: usize = 100;

/// Number of effect IDs in `include/ef_effect_control.h`.
pub const EFFECT_NUM: i16 = 126;

/// The public C effect ID for `iam_ef_ase`.
pub const EFFECT_ASE: i16 = 8;

/// The public C effect ID for `iam_ef_ase_ch`.
pub const EFFECT_ASE_CH: i16 = 9;

/// Effect IDs supported by this candidate crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectId {
    /// `eEC_EFFECT_ASE`.
    Ase,
    /// `eEC_EFFECT_ASE_CH`.
    AseChild,
}

impl EffectId {
    fn from_c_id(id: i16) -> Option<Self> {
        match id {
            EFFECT_ASE => Some(Self::Ase),
            EFFECT_ASE_CH => Some(Self::AseChild),
            _ => None,
        }
    }

    fn is_smooth(self) -> bool {
        // C: eEC_smooth_motion[eEC_EFFECT_ASE_CH] = 1; ASE is absent.
        matches!(self, Self::AseChild)
    }
}

/// Position, velocity, acceleration, scale, or offset in C's `xyz_t` shape.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EffectVec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

/// State values matching `eEC_STATE_*` in `include/ef_effect_control.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectState {
    /// Normal playback.
    Normal = 0,
    /// The effect has re-armed its continuous lifetime.
    Continuous = 1,
    /// Finished playback; retained for shape parity with the C controller.
    Finished = 2,
}

/// Exact public inputs accepted by `effect_make_proc`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnInput {
    /// Initial world position.
    pub position: EffectVec3,
    /// C's `int prio`; the controller stores its low eight bits.
    pub priority: i32,
    /// Public angle. Both ported C init functions ignore it.
    pub angle: i16,
    /// Public argument 0. Both ported C init functions replace it with zero.
    pub arg0: i16,
    /// Public argument 1. Both ported C init functions replace it with zero.
    pub arg1: i16,
    /// Item identity used for duplicate and kill matching.
    pub item_name: u16,
}

/// A stable-in-this-controller slot identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectHandle {
    /// Fixed pool index. Values outside the pool are rejected by snapshots.
    pub slot: usize,
}

impl EffectHandle {
    /// Return the controller slot, in `[0, ACTIVE_EFFECT_MAX)` for valid handles.
    pub const fn slot(self) -> usize {
        self.slot
    }
}

/// Result of the C-style make dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnOutcome {
    /// The effect was initialized in the reported slot.
    Created(EffectHandle),
    /// The C controller rejected a negative or out-of-range ID without work.
    IgnoredInvalidId,
    /// The ID is valid in the full game but is outside this candidate slice.
    UnsupportedEffect,
    /// The C allocation search returned `-1`.
    PoolFull,
    /// `ASE` has the C feature flag that suppresses an exact active duplicate.
    IgnoredDuplicate,
}

/// Errors for the frame delta boundary at the Rust API edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    /// C graph deltas are non-negative; negative deltas are rejected here.
    NegativeDelta,
    /// NaN and infinities cannot participate in lifetime arithmetic.
    NonFiniteDelta,
    /// The fractional accumulator overflowed.
    AccumulatorOverflow,
}

/// A source-level snapshot of one active slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectSnapshot {
    /// The fixed controller slot.
    pub slot: usize,
    /// The effect kind.
    pub id: EffectId,
    /// Item identity.
    pub item_name: u16,
    /// Stored low-byte priority.
    pub priority: u8,
    /// Current lifecycle state.
    pub state: EffectState,
    /// Integer timer decremented in the 60 Hz pass.
    pub timer: i16,
    /// Real-time lifetime decremented every update call.
    pub lifetime: f32,
    /// Current position.
    pub position: EffectVec3,
    /// Current velocity.
    pub velocity: EffectVec3,
    /// Current acceleration.
    pub acceleration: EffectVec3,
    /// Current scale.
    pub scale: EffectVec3,
    /// Initial/render offset.
    pub offset: EffectVec3,
    /// The effect's first `effect_specific` word.
    pub effect_specific0: i16,
}

/// The observable sound request made by `eAS_mv`/`sAdo_OngenPos`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioEvent {
    /// C sound ID `0x29`.
    pub sound_id: u16,
    /// Effect slot that issued the request in this model.
    pub slot: usize,
    /// Position passed to the audio helper.
    pub position: EffectVec3,
}

/// Draw data emitted by the two ported draw procedures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawState {
    /// Position supplied to the matrix helper.
    pub position: EffectVec3,
    /// Offset supplied to the matrix helper.
    pub offset: EffectVec3,
    /// Scale supplied to the matrix helper.
    pub scale: EffectVec3,
    /// Child model index is always zero; the parent draw procedure is empty.
    pub model_index: Option<u8>,
}

#[derive(Debug, Clone, Copy)]
struct Effect {
    id: EffectId,
    item_name: u16,
    priority: u8,
    state: EffectState,
    timer: i16,
    lifetime: f32,
    position: EffectVec3,
    velocity: EffectVec3,
    acceleration: EffectVec3,
    scale: EffectVec3,
    offset: EffectVec3,
    effect_specific0: i16,
    program_index: i16,
    zero_word_0a: i16,
}

/// Fixed-pool controller for the `ASE`/`ASE_CH` slice.
pub struct EffectController {
    effects: [Option<Effect>; ACTIVE_EFFECT_MAX],
    active_flags: [u8; ACTIVE_EFFECT_MAX],
    active_effect_num: usize,
    dt_accumulator: f32,
    audio_events: Vec<AudioEvent>,
    random_state: u32,
}

impl Default for EffectController {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectController {
    /// Construct the zeroed C-equivalent effect work area.
    pub fn new() -> Self {
        Self {
            effects: [None; ACTIVE_EFFECT_MAX],
            active_flags: [0; ACTIVE_EFFECT_MAX],
            active_effect_num: 0,
            dt_accumulator: 0.0,
            audio_events: Vec::new(),
            // C: src/static/libc64/qrand.c:3, static __qrand_idum = 1.
            random_state: 1,
        }
    }

    /// Construct a controller with an explicit C `sqrand`-compatible seed.
    pub fn with_random_seed(seed: u32) -> Self {
        let mut controller = Self::new();
        controller.random_state = seed;
        controller
    }

    /// Dispatch the exact `effect_make_proc` input shape for this slice.
    pub fn make(&mut self, effect_id: i16, input: SpawnInput) -> SpawnOutcome {
        if !(0..EFFECT_NUM).contains(&effect_id) {
            return SpawnOutcome::IgnoredInvalidId;
        }

        let Some(id) = EffectId::from_c_id(effect_id) else {
            return SpawnOutcome::UnsupportedEffect;
        };

        // C: eEC_effect_feature[eEC_EFFECT_ASE] is TRUE and the exact
        // (effect name, item_name) pair is suppressed before eAS_init runs.
        if matches!(id, EffectId::Ase) && self.has_active_match(id, input.item_name) {
            return SpawnOutcome::IgnoredDuplicate;
        }

        // C: eAS_init/eASC_init call make_effect_proc with an explicit offset
        // and force arg0/arg1 to zero. The incoming angle is also ignored.
        let offset = match id {
            EffectId::Ase => EffectVec3 {
                x: 0.0,
                y: 15.0,
                z: 0.0,
            },
            EffectId::AseChild => EffectVec3::default(),
        };
        let _ = (input.angle, input.arg0, input.arg1);

        self.allocate(id, input.position, input.priority, input.item_name, offset)
            .map_or_else(|outcome| outcome, SpawnOutcome::Created)
    }

    /// Advance the source's two ordered update passes.
    pub fn update(&mut self, dt_num_60fps_frames: f32) -> Result<(), UpdateError> {
        if !dt_num_60fps_frames.is_finite() {
            return Err(UpdateError::NonFiniteDelta);
        }
        if dt_num_60fps_frames < 0.0 {
            return Err(UpdateError::NegativeDelta);
        }

        let next_accumulator = self.dt_accumulator + dt_num_60fps_frames;
        if !next_accumulator.is_finite() {
            return Err(UpdateError::AccumulatorOverflow);
        }

        // C: eEC_AllEffectMove pass 1, lines 414-428.
        for slot in 0..ACTIVE_EFFECT_MAX {
            if self.active_flags[slot] == 0 {
                continue;
            }
            let effect = self.effects[slot]
                .as_mut()
                .expect("an active C flag always has an initialized effect");
            effect.lifetime -= dt_num_60fps_frames;
            if effect.id.is_smooth() {
                move_child(effect, dt_num_60fps_frames);
            }
        }

        let whole_ticks = next_accumulator.trunc();
        self.dt_accumulator = next_accumulator - whole_ticks;
        // C: steps = (int)eEC_dt_accum; if (steps > 4) steps = 4.
        let steps = whole_ticks.min(4.0) as usize;

        // C: eEC_AllEffectMove pass 2, lines 440-463. A child created by the
        // parent can be visited later in this same slot scan, but its smooth
        // movement has already missed pass 1 for this frame.
        for _ in 0..steps {
            for slot in 0..ACTIVE_EFFECT_MAX {
                if self.active_flags[slot] == 0 {
                    continue;
                }

                let id = self.effects[slot]
                    .as_ref()
                    .expect("an active C flag always has an initialized effect")
                    .id;
                if !id.is_smooth() {
                    self.move_parent(slot);
                }

                let should_die = {
                    let effect = self.effects[slot]
                        .as_mut()
                        .expect("an active C flag always has an initialized effect");
                    effect.timer = effect.timer.wrapping_sub(1);
                    effect.lifetime <= 0.0 || effect.timer <= 0
                };
                if should_die {
                    // C cleanup clears the active flag, timer, lifetime, and
                    // `_0A`, while leaving stale slot storage in place.
                    self.active_flags[slot] = 0;
                    let effect = self.effects[slot]
                        .as_mut()
                        .expect("an active C flag always has an initialized effect");
                    effect.timer = 0;
                    effect.lifetime = 0.0;
                    effect.zero_word_0a = 0;
                }
            }
        }

        Ok(())
    }

    /// Immediately kill all matching effects, preserving C's item matching.
    pub fn kill(&mut self, effect_id: i16, item_name: u16) -> usize {
        let Some(id) = EffectId::from_c_id(effect_id) else {
            return 0;
        };
        let mut killed = 0;
        for slot in 0..ACTIVE_EFFECT_MAX {
            if self.active_flags[slot] != 0
                && self.effects[slot].as_ref().is_some_and(|effect| {
                    effect.id == id && effect.item_name == item_name && effect.program_index != -1
                })
            {
                // Both profiles are `eEC_IMMEDIATE_DEATH`, so C only clears
                // the active flag here; it does not rewrite the stale struct.
                self.active_flags[slot] = 0;
                killed += 1;
            }
        }
        killed
    }

    /// Immediately kill every active effect owned by an item.
    pub fn kill_all(&mut self, item_name: u16) -> usize {
        let mut killed = 0;
        for slot in 0..ACTIVE_EFFECT_MAX {
            if self.active_flags[slot] != 0
                && self.effects[slot].as_ref().is_some_and(|effect| {
                    effect.item_name == item_name && effect.program_index != -1
                })
            {
                self.active_flags[slot] = 0;
                killed += 1;
            }
        }
        killed
    }

    /// Return an active slot's observable state, or `None` for an inactive slot.
    pub fn snapshot(&self, handle: EffectHandle) -> Option<EffectSnapshot> {
        if handle.slot >= ACTIVE_EFFECT_MAX || self.active_flags[handle.slot] == 0 {
            return None;
        }
        self.effects[handle.slot].map(|effect| EffectSnapshot {
            slot: handle.slot,
            id: effect.id,
            item_name: effect.item_name,
            priority: effect.priority,
            state: effect.state,
            timer: effect.timer,
            lifetime: effect.lifetime,
            position: effect.position,
            velocity: effect.velocity,
            acceleration: effect.acceleration,
            scale: effect.scale,
            offset: effect.offset,
            effect_specific0: effect.effect_specific0,
        })
    }

    /// Return the source draw data for an active slot.
    pub fn draw_state(&self, handle: EffectHandle) -> Option<DrawState> {
        let snapshot = self.snapshot(handle)?;
        let model_index = match snapshot.id {
            EffectId::Ase => None,
            EffectId::AseChild => Some(0),
        };
        let scale = match snapshot.id {
            EffectId::Ase => snapshot.scale,
            EffectId::AseChild => child_draw_scale(snapshot.lifetime),
        };
        Some(DrawState {
            position: snapshot.position,
            offset: snapshot.offset,
            scale,
            model_index,
        })
    }

    /// Number of nonzero C active flags.
    pub fn active_count(&self) -> usize {
        self.active_flags.iter().filter(|flag| **flag != 0).count()
    }

    /// C's mutable high-water allocation index.
    pub const fn active_effect_num(&self) -> usize {
        self.active_effect_num
    }

    /// Read and clear queued audio requests from the ported parent move.
    pub fn drain_audio_events(&mut self) -> Vec<AudioEvent> {
        std::mem::take(&mut self.audio_events)
    }

    fn has_active_match(&self, id: EffectId, item_name: u16) -> bool {
        self.active_flags.iter().enumerate().any(|(slot, flag)| {
            *flag != 0
                && self.effects[slot]
                    .as_ref()
                    .is_some_and(|effect| effect.id == id && effect.item_name == item_name)
        })
    }

    fn allocate(
        &mut self,
        id: EffectId,
        position: EffectVec3,
        priority: i32,
        item_name: u16,
        offset: EffectVec3,
    ) -> Result<EffectHandle, SpawnOutcome> {
        let stored_priority = priority as u8;
        let Some(slot) = self.find_effect_slot(stored_priority) else {
            return Err(SpawnOutcome::PoolFull);
        };

        let mut effect = Effect {
            id,
            item_name,
            priority: stored_priority,
            state: EffectState::Normal,
            timer: 10,
            lifetime: 10.0,
            position,
            velocity: EffectVec3::default(),
            acceleration: EffectVec3::default(),
            scale: EffectVec3 {
                x: 0.01,
                y: 0.01,
                z: 0.01,
            },
            offset,
            effect_specific0: 0,
            // C static work starts at zero and this slice never changes it.
            program_index: 0,
            zero_word_0a: 0,
        };

        match id {
            // C eAS_ct: only timer is changed; offset came from eAS_init.
            EffectId::Ase => effect.timer = 26,
            // C eASC_ct: random_first_speed, gravity, scale, timer.
            EffectId::AseChild => {
                effect.timer = 20;
                effect.velocity = EffectVec3 {
                    x: 0.0,
                    y: 2.7,
                    z: 0.0,
                };
                // C: eASC_DecideASCeDirect -> eEL_RandomFirstSpeed
                // -> sMath_RotateZ(vec, DEG2RAD(RANDOM_F(180))).
                let random_angle = self.next_fqrand() * PI;
                effect.velocity.x = -2.7 * random_angle.sin();
                effect.velocity.y = 2.7 * random_angle.cos();
                effect.acceleration.y = -0.057499997;
                effect.scale = EffectVec3 {
                    x: 0.0027,
                    y: 0.0027,
                    z: 0.0027,
                };
            }
        }
        effect.lifetime = f32::from(effect.timer);
        self.active_flags[slot] = stored_priority;
        self.effects[slot] = Some(effect);
        // C assigns active_effect_num = use_idx + 1 even when it overwrites a
        // lower slot; this intentionally is not a max/high-water operation.
        self.active_effect_num = slot + 1;
        Ok(EffectHandle { slot })
    }

    fn next_fqrand(&mut self) -> f32 {
        // C: src/static/libc64/qrand.c:11-16. `from_bits` keeps the source's
        // float construction without unsafe pointer punning.
        self.random_state = self
            .random_state
            .wrapping_mul(0x0019_660d)
            .wrapping_add(0x3c6e_f35f);
        let bits = (self.random_state >> 9) | 0x3f80_0000;
        f32::from_bits(bits) - 1.0
    }

    fn find_effect_slot(&self, priority: u8) -> Option<usize> {
        // C eEC_GetEffectBuff, lines 475-504. The `-1` loop bound excludes
        // `now_idx - 1`; retaining that quirk is part of this candidate's
        // boundary behavior.
        for slot in self.active_effect_num..ACTIVE_EFFECT_MAX {
            if self.active_flags[slot] == 0 {
                return Some(slot);
            }
        }
        for slot in 0..self.active_effect_num.saturating_sub(1) {
            if self.active_flags[slot] == 0 {
                return Some(slot);
            }
        }
        for slot in self.active_effect_num..ACTIVE_EFFECT_MAX {
            if self.active_flags[slot] < priority {
                return Some(slot);
            }
        }
        (0..self.active_effect_num.saturating_sub(1)).find(|&slot| self.active_flags[slot] != 2)
    }

    fn move_parent(&mut self, slot: usize) {
        let (position, priority, item_name, spawn_child) = {
            let effect = self.effects[slot]
                .as_mut()
                .expect("an active C flag always has an initialized effect");

            // C eEL_SetContiniousEnv(effect, 26, 52), before effect_specific++
            // and before the odd-timer child decision.
            if matches!(effect.state, EffectState::Normal | EffectState::Continuous)
                && (effect.timer <= 1 || effect.lifetime <= 1.0)
            {
                effect.state = EffectState::Continuous;
                effect.timer = 52;
                effect.lifetime = 52.0;
            }
            effect.effect_specific0 = effect.effect_specific0.wrapping_add(1);
            self.audio_events.push(AudioEvent {
                sound_id: 0x29,
                slot,
                position: effect.position,
            });
            (
                effect.position,
                effect.priority,
                effect.item_name,
                effect.timer & 1 != 0,
            )
        };

        if spawn_child {
            // C eAS_mv invokes the public name dispatcher with ASE_CH. The
            // child is therefore subject to the same fixed-pool failure path.
            let _ = self.allocate(
                EffectId::AseChild,
                position,
                i32::from(priority),
                item_name,
                EffectVec3::default(),
            );
        }
    }
}

fn move_child(effect: &mut Effect, dt: f32) {
    // C eASC_mv: acceleration and position are both scaled by graph dt.
    effect.velocity.x += effect.acceleration.x * dt;
    effect.velocity.y += effect.acceleration.y * dt;
    effect.velocity.z += effect.acceleration.z * dt;
    effect.position.x += effect.velocity.x * dt;
    effect.position.y += effect.velocity.y * dt;
    effect.position.z += effect.velocity.z * dt;
}

fn child_draw_scale(lifetime: f32) -> EffectVec3 {
    // C eASC_dw's eASC_scale_table and lifetime interpolation.
    const SCALE_TABLE: [f32; 10] = [
        0.00027000002,
        0.00027000002,
        0.00027000002,
        0.00054000004,
        0.0010800001,
        0.0016200001,
        0.0021600001,
        0.0027,
        0.00135,
        0.00054000004,
    ];
    let k = ((20.0 - lifetime) * 0.5).clamp(0.0, 9.0);
    let i = k as usize;
    let j = (i + 1).min(9);
    let fraction = k - i as f32;
    let scale = SCALE_TABLE[i] + (SCALE_TABLE[j] - SCALE_TABLE[i]) * fraction;
    EffectVec3 {
        x: scale,
        y: scale,
        z: scale,
    }
}
