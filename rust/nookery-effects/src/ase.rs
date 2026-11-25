//! The portable effect-controller and profile implementation.
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

// The effect controller's public IDs are part of the C interface.  Keeping
// the complete table here is important: callers may pass an effect ID to the
// controller without first constructing a Rust enum.
pub const EFFECT_SHOCK: i16 = 0;
pub const EFFECT_DUST: i16 = 1;
pub const EFFECT_MUKA: i16 = 2;
pub const EFFECT_WARAU: i16 = 3;
pub const EFFECT_HA: i16 = 4;
pub const EFFECT_SIBUKI: i16 = 5;
pub const EFFECT_GIMONHU: i16 = 6;
pub const EFFECT_KANTANHU: i16 = 7;
pub const EFFECT_KANGAERU: i16 = 10;
pub const EFFECT_OTIKOMI: i16 = 11;
pub const EFFECT_BURUBURU: i16 = 12;
pub const EFFECT_SIAWASE_HIKARI: i16 = 13;
pub const EFFECT_SIAWASE_HANA: i16 = 14;
pub const EFFECT_SIAWASE_HANA_CH: i16 = 15;
pub const EFFECT_TAKURAMI: i16 = 16;
pub const EFFECT_TAKURAMI_KIRA: i16 = 17;
pub const EFFECT_KAZE: i16 = 18;
pub const EFFECT_KAZE_HAPPA: i16 = 19;
pub const EFFECT_LOVELOVE: i16 = 20;
pub const EFFECT_LOVELOVE_HEART: i16 = 21;
pub const EFFECT_PUN: i16 = 22;
pub const EFFECT_PUN_YUGE: i16 = 23;
pub const EFFECT_PUN_SEKIMEN: i16 = 24;
pub const EFFECT_NAKU: i16 = 25;
pub const EFFECT_NAMIDA: i16 = 26;
pub const EFFECT_HIRAMEKI_DEN: i16 = 27;
pub const EFFECT_HIRAMEKI_HIKARI: i16 = 28;
pub const EFFECT_SITUREN: i16 = 29;
pub const EFFECT_ASE2: i16 = 30;
pub const EFFECT_DASH_ASIMOTO: i16 = 31;
pub const EFFECT_KONPU: i16 = 32;
pub const EFFECT_LOVELOVE2: i16 = 33;
pub const EFFECT_KPUN: i16 = 34;
pub const EFFECT_KISHA_KEMURI: i16 = 35;
pub const EFFECT_NEBOKE: i16 = 36;
pub const EFFECT_NEBOKE_AWA: i16 = 37;
pub const EFFECT_HANABI_YANAGI: i16 = 38;
pub const EFFECT_HANABI_BOTAN1: i16 = 39;
pub const EFFECT_HANABI_BOTAN2: i16 = 40;
pub const EFFECT_HANABI_HOSHI: i16 = 41;
pub const EFFECT_HANABI_SET: i16 = 42;
pub const EFFECT_HANABI_SWITCH: i16 = 43;
pub const EFFECT_FOOTPRINT: i16 = 44;
pub const EFFECT_TURN_FOOTPRINT: i16 = 45;
pub const EFFECT_BUBU: i16 = 46;
pub const EFFECT_TURN_ASIMOTO: i16 = 47;
pub const EFFECT_STRING: i16 = 48;
pub const EFFECT_YUKIHANE: i16 = 49;
pub const EFFECT_ANAHIKARI: i16 = 50;
pub const EFFECT_BUSH_HAPPA: i16 = 51;
pub const EFFECT_BUSH_YUKI: i16 = 52;
pub const EFFECT_TUMBLE: i16 = 53;
pub const EFFECT_TUMBLE_BODYPRINT: i16 = 54;
pub const EFFECT_SLIP: i16 = 55;
pub const EFFECT_TUMBLE_DUST: i16 = 56;
pub const EFFECT_SWING_NET: i16 = 57;
pub const EFFECT_AMI_MIZU: i16 = 58;
pub const EFFECT_MIZUTAMA: i16 = 59;
pub const EFFECT_YUKIDAMA: i16 = 60;
pub const EFFECT_KIKUZU: i16 = 61;
pub const EFFECT_SWING_AXE: i16 = 62;
pub const EFFECT_STEAM: i16 = 63;
pub const EFFECT_NIGHT15_MOON: i16 = 64;
pub const EFFECT_NIGHT13_MOON: i16 = 65;
pub const EFFECT_NEBOKE_AKUBI: i16 = 66;
pub const EFFECT_SLIP_FOOTPRINT: i16 = 67;
pub const EFFECT_WALK_ASIMOTO: i16 = 68;
pub const EFFECT_TURI_HAMON: i16 = 69;
pub const EFFECT_TURI_MIZU: i16 = 70;
pub const EFFECT_TURI_HANE0: i16 = 71;
pub const EFFECT_TURI_HANE1: i16 = 72;
pub const EFFECT_TURI_SUITEKI: i16 = 73;
pub const EFFECT_ONGEN: i16 = 74;
pub const EFFECT_IMPACT_STAR: i16 = 75;
pub const EFFECT_SANDSPLASH: i16 = 76;
pub const EFFECT_YOUNG_TREE: i16 = 77;
pub const EFFECT_SWING_ROD: i16 = 78;
pub const EFFECT_DOYON: i16 = 79;
pub const EFFECT_KAGU_HAPPA: i16 = 80;
pub const EFFECT_HANABI_DUMMY: i16 = 81;
pub const EFFECT_DIG_HOLE: i16 = 82;
pub const EFFECT_DIG_SCOOP: i16 = 83;
pub const EFFECT_DIG_MUD: i16 = 84;
pub const EFFECT_KIGAE: i16 = 85;
pub const EFFECT_KIGAE_LIGHT: i16 = 86;
pub const EFFECT_TAPE: i16 = 87;
pub const EFFECT_CLACKER: i16 = 88;
pub const EFFECT_KAMIFUBUKI: i16 = 89;
pub const EFFECT_CAR_BLIGHT: i16 = 90;
pub const EFFECT_TAMAIRE: i16 = 91;
pub const EFFECT_CAR_LIGHT: i16 = 92;
pub const EFFECT_RESET_HOLE: i16 = 93;
pub const EFFECT_GOKI: i16 = 94;
pub const EFFECT_HANABIRA: i16 = 95;
pub const EFFECT_WAIT_ASIMOTO: i16 = 96;
pub const EFFECT_OTOSIANA: i16 = 97;
pub const EFFECT_IKIGIRE: i16 = 98;
pub const EFFECT_YUKIDARUMA: i16 = 99;
pub const EFFECT_KYOUSOU_ONPU: i16 = 100;
pub const EFFECT_DOUZOU_LIGHT: i16 = 101;
pub const EFFECT_HALLOWEEN_SMOKE: i16 = 102;
pub const EFFECT_HALLOWEEN: i16 = 103;
pub const EFFECT_MOTIYUGE: i16 = 104;
pub const EFFECT_TABERU: i16 = 105;
pub const EFFECT_KASAMIZU: i16 = 106;
pub const EFFECT_KASAMIZUTAMA: i16 = 107;
pub const EFFECT_HANATIRI: i16 = 108;
pub const EFFECT_URANAI: i16 = 109;
pub const EFFECT_SUISOU_AWA: i16 = 110;
pub const EFFECT_BREAK_AXE: i16 = 111;
pub const EFFECT_YAJIRUSHI: i16 = 112;
pub const EFFECT_SOBA_YUGE: i16 = 113;
pub const EFFECT_FURO_YUGE: i16 = 114;
pub const EFFECT_SHOOTING: i16 = 115;
pub const EFFECT_SHOOTING_SET: i16 = 116;
pub const EFFECT_SHOOTING_KIRA: i16 = 117;
pub const EFFECT_COIN: i16 = 118;
pub const EFFECT_TENT_LAMP: i16 = 119;
pub const EFFECT_FLASH: i16 = 120;
pub const EFFECT_FLASHC: i16 = 121;
pub const EFFECT_KILLER: i16 = 122;
pub const EFFECT_MAKE_HEM: i16 = 123;
pub const EFFECT_MAKE_HEM_KIRA: i16 = 124;
pub const EFFECT_MAKE_HEM_LIGHT: i16 = 125;

/// Effect IDs from `include/ef_effect_control.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectId {
    /// `eEC_EFFECT_ASE`.
    Ase,
    /// `eEC_EFFECT_ASE_CH`.
    AseChild,
    /// Any of the remaining IDs in `include/ef_effect_control.h`.
    Other(u8),
}

impl EffectId {
    fn from_c_id(id: i16) -> Option<Self> {
        match id {
            EFFECT_ASE => Some(Self::Ase),
            EFFECT_ASE_CH => Some(Self::AseChild),
            0..EFFECT_NUM => Some(Self::Other(id as u8)),
            _ => None,
        }
    }

    fn c_id(self) -> i16 {
        match self {
            Self::Ase => EFFECT_ASE,
            Self::AseChild => EFFECT_ASE_CH,
            Self::Other(id) => i16::from(id),
        }
    }

    fn is_smooth(self) -> bool {
        // C: eEC_smooth_motion[eEC_EFFECT_ASE_CH] = 1; ASE is absent.
        matches!(
            self.c_id(),
            EFFECT_DUST
                | EFFECT_BUSH_HAPPA
                | EFFECT_BUSH_YUKI
                | EFFECT_KAZE_HAPPA
                | EFFECT_NAMIDA
                | EFFECT_IMPACT_STAR
                | EFFECT_KIGAE_LIGHT
                | EFFECT_KONPU
                | EFFECT_NEBOKE_AKUBI
                | EFFECT_NEBOKE_AWA
                | EFFECT_LOVELOVE2
                | EFFECT_LOVELOVE_HEART
                | EFFECT_KIKUZU
                | EFFECT_ASE_CH
                | EFFECT_KASAMIZUTAMA
                | EFFECT_HALLOWEEN_SMOKE
                | EFFECT_YUKIHANE
                | EFFECT_STEAM
                | EFFECT_FURO_YUGE
                | EFFECT_SOBA_YUGE
                | EFFECT_TUMBLE_DUST
                | EFFECT_DIG_MUD
                | EFFECT_KPUN
                | EFFECT_PUN_YUGE
                | EFFECT_SANDSPLASH
                | EFFECT_KISHA_KEMURI
                | EFFECT_DIG_HOLE
                | EFFECT_GOKI
                | EFFECT_RESET_HOLE
                | EFFECT_TAMAIRE
                | EFFECT_BREAK_AXE
                | EFFECT_HANABIRA
                | EFFECT_KAMIFUBUKI
                | EFFECT_TURI_SUITEKI
                | EFFECT_SHOOTING
                | EFFECT_SHOOTING_SET
                | EFFECT_MAKE_HEM
                | EFFECT_MAKE_HEM_KIRA
                | EFFECT_MAKE_HEM_LIGHT
                | EFFECT_KAGU_HAPPA
                | EFFECT_YOUNG_TREE
                | EFFECT_NIGHT13_MOON
                | EFFECT_NIGHT15_MOON
                | EFFECT_TURI_MIZU
                | EFFECT_TUMBLE_BODYPRINT
                | EFFECT_SUISOU_AWA
                | EFFECT_TURI_HAMON
                | EFFECT_DOYON
                | EFFECT_CAR_LIGHT
                | EFFECT_DOUZOU_LIGHT
                | EFFECT_AMI_MIZU
        )
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
    /// The ID is valid in the full game but could not be dispatched.
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
    /// The public angle and two effect arguments retained by the C slot.
    pub angle: i16,
    /// First profile argument.
    pub arg0: i16,
    /// Second profile argument.
    pub arg1: i16,
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
    effect_specific: [i16; 6],
    angle: i16,
    arg0: i16,
    arg1: i16,
    program_index: i16,
    zero_word_0a: i16,
}

/// Fixed-pool controller for the C effect library.
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

        // C suppresses exact `(effect name, item_name)` duplicates for the
        // feature-enabled profiles before their init callback runs.
        if feature_enabled(id) && self.has_active_match(id, input.item_name) {
            return SpawnOutcome::IgnoredDuplicate;
        }

        // C: eAS_init/eASC_init call make_effect_proc with an explicit offset
        // and force arg0/arg1 to zero. The incoming angle is also ignored.
        self.allocate(id, input)
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
                if effect.id == EffectId::AseChild {
                    move_child(effect, dt_num_60fps_frames);
                } else {
                    move_effect_motion(effect, dt_num_60fps_frames);
                    update_profile_animation(effect);
                }
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
                    if id == EffectId::Ase {
                        self.move_parent(slot);
                    } else {
                        self.move_profile(slot);
                    }
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
                match death_policy(id) {
                    DeathPolicy::Immediate => self.active_flags[slot] = 0,
                    DeathPolicy::Finite(frames) => {
                        let effect = self.effects[slot]
                            .as_mut()
                            .expect("an active C flag always has an initialized effect");
                        effect.timer = frames;
                        effect.lifetime = f32::from(frames);
                        effect.state = EffectState::Finished;
                    }
                    DeathPolicy::Ignore => {}
                }
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
            angle: effect.angle,
            arg0: effect.arg0,
            arg1: effect.arg1,
        })
    }

    /// Return the source draw data for an active slot.
    pub fn draw_state(&self, handle: EffectHandle) -> Option<DrawState> {
        let snapshot = self.snapshot(handle)?;
        let model_index = if snapshot.id == EffectId::Ase {
            None
        } else {
            Some(0)
        };
        let scale = if snapshot.id == EffectId::Ase {
            snapshot.scale
        } else if snapshot.id == EffectId::AseChild {
            child_draw_scale(snapshot.lifetime)
        } else {
            snapshot.scale
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

    fn allocate(&mut self, id: EffectId, input: SpawnInput) -> Result<EffectHandle, SpawnOutcome> {
        let stored_priority = input.priority as u8;
        let Some(slot) = self.find_effect_slot(stored_priority) else {
            return Err(SpawnOutcome::PoolFull);
        };

        let mut effect = Effect {
            id,
            item_name: input.item_name,
            priority: stored_priority,
            state: EffectState::Normal,
            timer: 10,
            lifetime: 10.0,
            position: input.position,
            velocity: EffectVec3::default(),
            acceleration: EffectVec3::default(),
            scale: EffectVec3 {
                x: 0.01,
                y: 0.01,
                z: 0.01,
            },
            offset: EffectVec3::default(),
            effect_specific0: 0,
            effect_specific: [0; 6],
            angle: input.angle,
            arg0: input.arg0,
            arg1: input.arg1,
            // C static work starts at zero and this slice never changes it.
            program_index: 0,
            zero_word_0a: 0,
        };

        initialize_profile(&mut effect, id, &mut self.random_state);
        effect.lifetime = f32::from(effect.timer);
        self.active_flags[slot] = stored_priority;
        self.effects[slot] = Some(effect);
        // C assigns active_effect_num = use_idx + 1 even when it overwrites a
        // lower slot; this intentionally is not a max/high-water operation.
        self.active_effect_num = slot + 1;
        Ok(EffectHandle { slot })
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
                SpawnInput {
                    position,
                    priority: i32::from(priority),
                    angle: 0,
                    arg0: 0,
                    arg1: 0,
                    item_name,
                },
            );
        }
    }

    fn move_profile(&mut self, slot: usize) {
        let effect = self.effects[slot]
            .as_mut()
            .expect("an active C flag always has an initialized effect");
        move_effect_motion(effect, 1.0);
        update_profile_animation(effect);
    }
}

#[derive(Clone, Copy)]
enum DeathPolicy {
    Immediate,
    Ignore,
    Finite(i16),
}

fn death_policy(id: EffectId) -> DeathPolicy {
    match id.c_id() {
        EFFECT_ASE
        | EFFECT_ASE_CH
        | EFFECT_ASE2
        | EFFECT_BURUBURU
        | EFFECT_HANABI_SWITCH
        | EFFECT_KPUN
        | EFFECT_LOVELOVE
        | EFFECT_LOVELOVE_HEART
        | EFFECT_NAKU
        | EFFECT_NEBOKE
        | EFFECT_PUN
        | EFFECT_SIAWASE_HANA
        | EFFECT_SIAWASE_HANA_CH
        | EFFECT_SHOOTING_KIRA
        | EFFECT_SHOOTING_SET
        | EFFECT_TAKURAMI_KIRA
        | EFFECT_WARAU => DeathPolicy::Immediate,
        EFFECT_ANAHIKARI => DeathPolicy::Finite(60),
        EFFECT_KANGAERU => DeathPolicy::Finite(7),
        EFFECT_NIGHT13_MOON => DeathPolicy::Finite(80),
        EFFECT_NIGHT15_MOON => DeathPolicy::Finite(60),
        EFFECT_OTIKOMI => DeathPolicy::Finite(10),
        EFFECT_RESET_HOLE => DeathPolicy::Finite(24),
        EFFECT_SIAWASE_HIKARI => DeathPolicy::Finite(72),
        EFFECT_YAJIRUSHI => DeathPolicy::Finite(6),
        _ => DeathPolicy::Ignore,
    }
}

fn feature_enabled(id: EffectId) -> bool {
    matches!(
        id.c_id(),
        3 | 8
            | 10
            | 11
            | 12
            | 13
            | 14
            | 16
            | 18
            | 19
            | 22
            | 25
            | 30
            | 36
            | 43
            | 46
            | 64
            | 65
            | 105
            | 109
            | 116
            | 119
            | 121
            | 122
            | 123
            | 125
    )
}

fn profile_timer(id: EffectId, arg0: i16, arg1: i16) -> i16 {
    match id.c_id() {
        EFFECT_SHOCK => 14,
        EFFECT_DUST => 18,
        EFFECT_MUKA => 40,
        EFFECT_WARAU => 24,
        EFFECT_HA => 56,
        EFFECT_SIBUKI => 12,
        EFFECT_GIMONHU => 72,
        EFFECT_KANTANHU => 72,
        EFFECT_ASE => 26,
        EFFECT_ASE_CH => 20,
        EFFECT_KANGAERU => 36,
        EFFECT_OTIKOMI => 22,
        EFFECT_BURUBURU => 8,
        EFFECT_SIAWASE_HIKARI => 22,
        EFFECT_SIAWASE_HANA => 22,
        EFFECT_SIAWASE_HANA_CH => 92,
        EFFECT_TAKURAMI => 54,
        EFFECT_TAKURAMI_KIRA => 30,
        EFFECT_KAZE => 100,
        EFFECT_KAZE_HAPPA => 60,
        EFFECT_LOVELOVE => 52,
        EFFECT_LOVELOVE_HEART => 120,
        EFFECT_PUN => 24,
        EFFECT_PUN_YUGE => 26,
        EFFECT_PUN_SEKIMEN => 16,
        EFFECT_NAKU => 32,
        EFFECT_NAMIDA => 30,
        EFFECT_HIRAMEKI_DEN => 72,
        EFFECT_HIRAMEKI_HIKARI => 12,
        EFFECT_SITUREN => 128,
        EFFECT_ASE2 => 26,
        EFFECT_DASH_ASIMOTO => 0,
        EFFECT_KONPU => 72,
        EFFECT_LOVELOVE2 => 112,
        EFFECT_KPUN => 26,
        EFFECT_KISHA_KEMURI => 80,
        EFFECT_NEBOKE => 112,
        EFFECT_NEBOKE_AWA => 64,
        EFFECT_HANABI_YANAGI | EFFECT_HANABI_BOTAN1 | EFFECT_HANABI_BOTAN2 => 110,
        EFFECT_HANABI_HOSHI => 80,
        EFFECT_HANABI_SET => 200,
        EFFECT_HANABI_SWITCH => 300,
        EFFECT_FOOTPRINT | EFFECT_TURN_FOOTPRINT => 160,
        EFFECT_BUBU => 10,
        EFFECT_TURN_ASIMOTO => 0,
        EFFECT_STRING => 20,
        EFFECT_YUKIHANE => 16,
        EFFECT_ANAHIKARI => 60,
        EFFECT_BUSH_HAPPA => {
            if arg1 & i16::MIN != 0 {
                20
            } else {
                60
            }
        }
        EFFECT_BUSH_YUKI => 60,
        EFFECT_TUMBLE => 0,
        EFFECT_TUMBLE_BODYPRINT => 600,
        EFFECT_SLIP => 0,
        EFFECT_TUMBLE_DUST => 32,
        EFFECT_SWING_NET | EFFECT_SWING_AXE | EFFECT_SWING_ROD => 0,
        EFFECT_AMI_MIZU => 10,
        EFFECT_MIZUTAMA => 50,
        EFFECT_YUKIDAMA => 50,
        EFFECT_KIKUZU => 70,
        EFFECT_STEAM => 30,
        EFFECT_NIGHT15_MOON | EFFECT_NIGHT13_MOON => 20_000,
        EFFECT_NEBOKE_AKUBI => 80,
        EFFECT_SLIP_FOOTPRINT => 160,
        EFFECT_WALK_ASIMOTO => 0,
        EFFECT_TURI_HAMON => {
            if arg0 == 7 {
                3
            } else {
                32
            }
        }
        EFFECT_TURI_MIZU => {
            if arg0 == 0 || arg0 == 2 {
                16
            } else {
                20
            }
        }
        EFFECT_TURI_HANE0 | EFFECT_TURI_HANE1 => 16,
        EFFECT_TURI_SUITEKI => 10,
        EFFECT_ONGEN => 10,
        EFFECT_IMPACT_STAR => 40,
        EFFECT_SANDSPLASH => 16,
        EFFECT_YOUNG_TREE => match arg0 {
            1 | 2 => 40,
            3 => 14,
            _ => 82,
        },
        EFFECT_DOYON => 60,
        EFFECT_KAGU_HAPPA => 36,
        EFFECT_HANABI_DUMMY => 80,
        EFFECT_DIG_HOLE => 10,
        EFFECT_DIG_SCOOP => 0,
        EFFECT_DIG_MUD => 10,
        EFFECT_KIGAE => 60,
        EFFECT_KIGAE_LIGHT => 30,
        EFFECT_TAPE => 80,
        EFFECT_CLACKER => 80,
        EFFECT_KAMIFUBUKI => 88,
        EFFECT_CAR_BLIGHT => 42,
        EFFECT_TAMAIRE => 200,
        EFFECT_CAR_LIGHT => 24,
        EFFECT_RESET_HOLE => 16,
        EFFECT_GOKI => 190,
        EFFECT_HANABIRA => 60,
        EFFECT_WAIT_ASIMOTO => 0,
        EFFECT_OTOSIANA => 16,
        EFFECT_IKIGIRE => 0,
        EFFECT_YUKIDARUMA => 20,
        EFFECT_KYOUSOU_ONPU => 0,
        EFFECT_DOUZOU_LIGHT => 24,
        EFFECT_HALLOWEEN_SMOKE => 18,
        EFFECT_HALLOWEEN => 40,
        EFFECT_MOTIYUGE => 30,
        EFFECT_TABERU => 44,
        EFFECT_KASAMIZU => 24,
        EFFECT_KASAMIZUTAMA => 20,
        EFFECT_HANATIRI => 0,
        EFFECT_URANAI => 80,
        EFFECT_SUISOU_AWA => 80,
        EFFECT_BREAK_AXE => 70,
        EFFECT_YAJIRUSHI => 4,
        EFFECT_SOBA_YUGE => 44,
        EFFECT_FURO_YUGE => 44,
        EFFECT_SHOOTING => 320,
        EFFECT_SHOOTING_SET => 100,
        EFFECT_SHOOTING_KIRA => 42,
        EFFECT_COIN => 10,
        EFFECT_TENT_LAMP => 100,
        EFFECT_FLASH => 5,
        EFFECT_FLASHC => 10,
        EFFECT_KILLER => 360,
        EFFECT_MAKE_HEM => 2000,
        EFFECT_MAKE_HEM_KIRA => 500,
        EFFECT_MAKE_HEM_LIGHT => 195,
        _ => 10,
    }
}

fn initialize_profile(effect: &mut Effect, id: EffectId, random_state: &mut u32) {
    effect.timer = profile_timer(id, effect.arg0, effect.arg1);
    match id.c_id() {
        EFFECT_ASE => effect.offset.y = 15.0,
        EFFECT_ASE_CH => {
            let angle = next_fqrand(random_state) * PI;
            effect.velocity.x = -2.7 * angle.sin();
            effect.velocity.y = 2.7 * angle.cos();
            effect.acceleration.y = -0.057499997;
            effect.scale = EffectVec3 {
                x: 0.0027,
                y: 0.0027,
                z: 0.0027,
            };
        }
        EFFECT_ASE2 => {
            effect.scale = EffectVec3 {
                x: 0.006,
                y: 0.006,
                z: 0.006,
            }
        }
        EFFECT_ANAHIKARI => {
            let scale = if effect.arg0 == 0 { 0.03 } else { 0.037 };
            effect.scale = EffectVec3 {
                x: scale,
                y: scale,
                z: scale,
            };
            effect.position.z += 4.0;
        }
        EFFECT_BUBU => {
            effect.scale = EffectVec3 {
                x: 0.015,
                y: 0.015,
                z: 0.015,
            };
            effect.position.y += 70.0;
        }
        EFFECT_HA => {
            effect.scale = EffectVec3 {
                x: 0.0067,
                y: 0.0067,
                z: 0.0067,
            }
        }
        EFFECT_HIRAMEKI_DEN => {
            effect.scale = EffectVec3 {
                x: 0.007,
                y: 0.007,
                z: 0.007,
            }
        }
        EFFECT_PUN_SEKIMEN => {
            effect.scale = EffectVec3 {
                x: 0.0105,
                y: 0.0105,
                z: 0.0105,
            }
        }
        EFFECT_BURUBURU => {
            effect.scale = EffectVec3 {
                x: 0.01,
                y: 0.01,
                z: 0.01,
            }
        }
        EFFECT_YUKIHANE => {
            effect.position.y += 0.5;
            effect.scale = EffectVec3 {
                x: 0.008,
                y: 0.008,
                z: 0.008,
            };
            effect.velocity.y = 0.5;
            effect.acceleration.y = -0.02;
        }
        EFFECT_NAMIDA => effect.acceleration.y = -0.0675,
        EFFECT_NEBOKE_AKUBI => effect.acceleration.y = 0.003425,
        EFFECT_NEBOKE_AWA => {
            effect.velocity.y = 0.18;
            effect.acceleration.y = 0.008;
            effect.offset = effect.position;
        }
        EFFECT_KASAMIZUTAMA => effect.acceleration.y = -0.105,
        EFFECT_STEAM => effect.acceleration.y = 0.125,
        EFFECT_SUISOU_AWA => effect.acceleration.y = 0.075,
        EFFECT_YOUNG_TREE if effect.arg0 == 1 || effect.arg0 == 2 => {
            effect.velocity.y = 0.75;
            effect.acceleration.y = -0.05;
        }
        EFFECT_URANAI => {
            effect.scale = EffectVec3 {
                x: 0.01,
                y: 0.01,
                z: 0.01,
            }
        }
        EFFECT_TURI_HANE0 => {
            let scale = match effect.arg0 {
                1 => 0.013,
                2 => 0.014,
                3 => 0.009,
                4 => 0.007,
                _ => 0.01,
            };
            effect.scale = EffectVec3 {
                x: scale,
                y: scale,
                z: scale,
            };
        }
        EFFECT_TURI_HANE1 => {
            effect.scale = EffectVec3 {
                x: 0.01,
                y: 0.01,
                z: 0.01,
            }
        }
        EFFECT_YUKIDARUMA => {
            let scale = (f32::from(effect.arg0).clamp(0.0, 100.0) / 100.0) * 0.02 + 0.01;
            effect.offset.x = scale;
            effect.scale = EffectVec3 {
                x: scale,
                y: scale,
                z: scale,
            };
        }
        _ => {}
    }
    effect.effect_specific = [0; 6];
    effect.effect_specific0 = 0;
}

fn next_fqrand(random_state: &mut u32) -> f32 {
    *random_state = random_state
        .wrapping_mul(0x0019_660d)
        .wrapping_add(0x3c6e_f35f);
    f32::from_bits((*random_state >> 9) | 0x3f80_0000) - 1.0
}

fn update_profile_animation(effect: &mut Effect) {
    match effect.id.c_id() {
        EFFECT_AMI_MIZU => {
            let value = if effect.lifetime >= 10.0 {
                effect.offset.x
            } else {
                calc_adjust(
                    effect.lifetime,
                    0.0,
                    8.0,
                    1.3 * effect.offset.x,
                    1.1 * effect.offset.x,
                )
            };
            effect.scale = EffectVec3 {
                x: value,
                y: value,
                z: value,
            };
        }
        EFFECT_URANAI if effect.lifetime < 70.0 => {
            effect.scale.x = calc_adjust(effect.lifetime, 0.0, 70.0, 0.014, 0.01);
            effect.scale.y = calc_adjust(effect.lifetime, 0.0, 70.0, 0.012, 0.01);
        }
        _ => {}
    }
}

fn move_effect_motion(effect: &mut Effect, dt: f32) {
    effect.velocity.x += effect.acceleration.x * dt;
    effect.velocity.y += effect.acceleration.y * dt;
    effect.velocity.z += effect.acceleration.z * dt;
    effect.position.x += effect.velocity.x * dt;
    effect.position.y += effect.velocity.y * dt;
    effect.position.z += effect.velocity.z * dt;
}

fn calc_adjust(now: f32, start: f32, end: f32, start_value: f32, end_value: f32) -> f32 {
    if start == end || now <= start {
        start_value
    } else if now >= end {
        end_value
    } else {
        start_value + (now - start) * ((end_value - start_value) / (end - start))
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
