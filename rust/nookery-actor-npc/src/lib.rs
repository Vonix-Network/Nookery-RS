//! A bounded Rust model of one ordinary-NPC actor behavior family.
//!
//! The port follows the request arbitration and lifecycle dispatch in:
//!
//! * `src/actor/npc/ac_npc_action.c_inc:445-498`
//! * `src/actor/npc/ac_normal_npc.c:28-64`
//! * `include/ac_npc.h:693-722` and `include/ac_npc.h:971-1002`
//!
//! The request records use the shipped actor ABI's 32-bit handle width. A
//! handle is deliberately an integer token rather than a host pointer: this
//! crate is a behavior model, not a production FFI surface.
//!
//! # Scope and boundary
//!
//! [`NpcAction`] and [`NpcRequest`] are exact `repr(C)` mirrors of the
//! behavior-bearing records in `ac_npc.h`. [`NpcBehavior`] contains those
//! records as a small standalone state carrier; it is not a claim that the
//! entire, much larger `NPC_ACTOR` has been ported. Rendering, movement,
//! scheduling, dialogue, cloth, and the global NPC clip remain dependencies.
//!
//! The C source indexes `aNPC_act_proc` without checking the requested action
//! index. Rust returns [`NpcError::InvalidActionIndex`] before dispatching an
//! invalid index, preserving memory safety at this boundary. Valid indices
//! follow the source dispatch order exactly.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

use core::fmt;

/// Width of a target actor/function handle in the decompiled actor ABI.
pub type Handle = u32;

/// Number of ordinary NPC action procedures in the source dispatch table.
pub const ACTION_COUNT: usize = 23;

/// Sentinel used by the source for an action that has no active procedure.
pub const ACTION_NONE: u8 = 0xff;

/// Sentinel used by the source when an action procedure has finished.
pub const ACTION_END_STEP: u8 = 0xff;

/// The demo flag that suppresses head look-at requests.
pub const COND_DEMO_SKIP_HEAD_LOOKAT: u32 = 1 << 8;

/// The ordinary NPC schedule selector installed by `ac_normal_npc.c`.
pub const SCHEDULE_TYPE_NORMAL: i32 = 1;

/// The phase passed to an action procedure when its data changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionPhase {
    /// The action receives a request's type and may update its object fields.
    ChangeData,
    /// The action is entering a new action index.
    Init,
    /// The action performs its per-frame work.
    Main,
}

/// Errors raised by the safe Rust boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NpcError {
    /// The C implementation would index outside its `aNPC_act_proc` table.
    InvalidActionIndex {
        /// The rejected raw action index.
        index: u8,
    },
}

impl fmt::Display for NpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidActionIndex { index } => {
                write!(f, "NPC action index {index} is outside the dispatch table")
            }
        }
    }
}

impl std::error::Error for NpcError {}

/// The exact target-layout mirror of `aNPC_action_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcAction {
    /// Current action priority.
    pub priority: u8,
    /// Current action index.
    pub idx: u8,
    /// Current action step.
    pub step: u8,
    /// Current action type.
    pub action_type: u8,
    /// Previous action index.
    pub prev_idx: u8,
    /// Previous action step.
    pub prev_step: u8,
    /// Action timer.
    pub act_timer: f32,
    /// Feeling selector.
    pub feel: u8,
    /// Action object selector.
    pub act_obj: u8,
    /// Action object name/identifier.
    pub act_obj_id: u16,
    /// X movement argument.
    pub move_x: i16,
    /// Z movement argument.
    pub move_z: i16,
    /// Target-width action procedure handle.
    pub act_proc: Handle,
}

impl Default for NpcAction {
    fn default() -> Self {
        Self {
            priority: 0,
            idx: ACTION_NONE,
            step: ACTION_END_STEP,
            action_type: 0,
            prev_idx: ACTION_NONE,
            prev_step: ACTION_END_STEP,
            act_timer: 0.0,
            feel: 0,
            act_obj: 0,
            act_obj_id: 0,
            move_x: 0,
            move_z: 0,
            act_proc: 0,
        }
    }
}

/// The exact target-layout mirror of `aNPC_request_c`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcRequest {
    /// Pending action priority.
    pub act_priority: u8,
    /// Pending action index.
    pub act_idx: u8,
    /// Pending action type.
    pub act_type: u8,
    /// Six 16-bit action arguments.
    pub act_args: [u16; 6],
    /// Umbrella request flag.
    pub umb_flag: u8,
    /// Unnamed source byte at offset `0x11`.
    pub reserved: u8,
    /// Pending head request priority.
    pub head_priority: u8,
    /// Pending head request type.
    pub head_type: u8,
    /// Target-width actor handle for a head request.
    pub head_target: Handle,
    /// Target position for a head request.
    pub head_pos: [f32; 3],
}

impl Default for NpcRequest {
    fn default() -> Self {
        Self {
            act_priority: 0,
            act_idx: ACTION_NONE,
            act_type: 0,
            act_args: [0; 6],
            umb_flag: 0,
            reserved: 0,
            head_priority: 0,
            head_type: 0,
            head_target: 0,
            head_pos: [0.0; 3],
        }
    }
}

/// The behavior-bearing subset of an NPC actor state.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NpcBehavior {
    /// Current action state.
    pub action: NpcAction,
    /// Pending requests.
    pub request: NpcRequest,
}

/// Receives the source action-procedure calls in their exact order.
pub trait ActionDispatcher {
    /// Dispatch one action procedure. The callback may update `action` during
    /// [`ActionPhase::ChangeData`], just as the C procedure can.
    fn dispatch(&mut self, index: u8, phase: ActionPhase, action: &mut NpcAction);

    /// Resolve a changed action object after `ChangeData` returns.
    fn resolve_object(&mut self, action: &NpcAction);
}

/// Admits an action request using the source's inclusive priority threshold.
///
/// If `args` is `None`, the existing six arguments are left untouched, which
/// matches the C null-pointer branch. Equal priorities replace the existing
/// request; lower priorities are rejected.
pub fn set_request_act(
    state: &mut NpcBehavior,
    priority: u8,
    action_index: u8,
    action_type: u8,
    args: Option<&[u16; 6]>,
) -> bool {
    if priority >= state.request.act_priority {
        state.request.act_priority = priority;
        state.request.act_idx = action_index;
        state.request.act_type = action_type;
        if let Some(args) = args {
            state.request.act_args = *args;
        }
        true
    } else {
        false
    }
}

/// Runs one source-equivalent action-processing pass.
pub fn action_proc<D: ActionDispatcher>(
    state: &mut NpcBehavior,
    dispatcher: &mut D,
) -> Result<(), NpcError> {
    if state.request.act_priority >= state.action.priority && state.request.act_priority != 0 {
        let requested_index = state.request.act_idx;
        validate_action_index(requested_index)?;

        let previous_index = state.action.idx;
        let previous_object = state.action.act_obj;
        let previous_object_id = state.action.act_obj_id;

        state.action.prev_idx = state.action.idx;
        state.action.prev_step = state.action.step;
        state.action.priority = state.request.act_priority;
        state.action.idx = requested_index;
        state.action.action_type = state.request.act_type;

        dispatcher.dispatch(requested_index, ActionPhase::ChangeData, &mut state.action);
        if state.action.act_obj != previous_object || state.action.act_obj_id != previous_object_id
        {
            dispatcher.resolve_object(&state.action);
        }

        if requested_index != previous_index {
            dispatcher.dispatch(requested_index, ActionPhase::Init, &mut state.action);
        }
    }

    state.request.act_priority = 0;

    if state.action.step != ACTION_END_STEP && state.action.idx != ACTION_NONE {
        validate_action_index(state.action.idx)?;
        dispatcher.dispatch(state.action.idx, ActionPhase::Main, &mut state.action);
    }

    if state.action.step == ACTION_END_STEP {
        state.action.priority = 0;
    }
    Ok(())
}

fn validate_action_index(index: u8) -> Result<(), NpcError> {
    if usize::from(index) < ACTION_COUNT {
        Ok(())
    } else {
        Err(NpcError::InvalidActionIndex { index })
    }
}

/// The exact target-layout mirror of the `aNPC_ct_data_c` initializer used by
/// the normal NPC wrapper.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalNpcCtData {
    /// Move-procedure handle.
    pub move_proc: Handle,
    /// Draw-procedure handle.
    pub draw_proc: Handle,
    /// Schedule selector.
    pub schedule_type: i32,
    /// Optional talk-request procedure handle.
    pub talk_request_proc: Handle,
    /// Optional talk-init procedure handle.
    pub talk_init_proc: Handle,
    /// Optional talk-end procedure handle.
    pub talk_end_check_proc: Handle,
    /// Source padding/data field at offset `0x18`.
    pub reserved: i32,
}

/// Static data passed by `aNOR_actor_ct` to the NPC clip.
pub const NORMAL_NPC_CT_DATA: NormalNpcCtData = NormalNpcCtData {
    move_proc: 1,
    draw_proc: 2,
    schedule_type: SCHEDULE_TYPE_NORMAL,
    talk_request_proc: 0,
    talk_init_proc: 0,
    talk_end_check_proc: 0,
    reserved: 0,
};

/// The callback surface needed to model the normal-NPC wrapper in
/// `ac_normal_npc.c`.
pub trait NormalNpcClip {
    /// Run the birth check; `true` permits construction.
    fn birth_check(&mut self, actor: Handle, game: Handle) -> bool;
    /// Run the common NPC construction procedure.
    fn construct(&mut self, actor: Handle, game: Handle, data: &NormalNpcCtData);
    /// Run common destruction.
    fn destroy(&mut self, actor: Handle, game: Handle);
    /// Run common save.
    fn save(&mut self, actor: Handle, game: Handle);
    /// Run common initialization.
    fn init(&mut self, actor: Handle, game: Handle);
    /// Run common movement.
    fn move_actor(&mut self, actor: Handle, game: Handle);
    /// Run common drawing.
    fn draw(&mut self, actor: Handle, game: Handle);
}

/// Executes the normal-NPC construction gate and common construction call.
pub fn normal_npc_ct<C: NormalNpcClip>(clip: &mut C, actor: Handle, game: Handle) {
    if clip.birth_check(actor, game) {
        clip.construct(actor, game, &NORMAL_NPC_CT_DATA);
    }
}

/// Forwards normal-NPC destruction to the common NPC clip.
pub fn normal_npc_dt<C: NormalNpcClip>(clip: &mut C, actor: Handle, game: Handle) {
    clip.destroy(actor, game);
}

/// Forwards normal-NPC save to the common NPC clip.
pub fn normal_npc_save<C: NormalNpcClip>(clip: &mut C, actor: Handle, game: Handle) {
    clip.save(actor, game);
}

/// Forwards normal-NPC initialization to the common NPC clip.
pub fn normal_npc_init<C: NormalNpcClip>(clip: &mut C, actor: Handle, game: Handle) {
    clip.init(actor, game);
}

/// Forwards normal-NPC movement to the common NPC clip.
pub fn normal_npc_move<C: NormalNpcClip>(clip: &mut C, actor: Handle, game: Handle) {
    clip.move_actor(actor, game);
}

/// Forwards normal-NPC drawing to the common NPC clip.
pub fn normal_npc_draw<C: NormalNpcClip>(clip: &mut C, actor: Handle, game: Handle) {
    clip.draw(actor, game);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn target_layouts_match_the_c_records() {
        assert_eq!(size_of::<NpcAction>(), 0x18);
        assert_eq!(offset_of!(NpcAction, act_timer), 0x08);
        assert_eq!(offset_of!(NpcAction, act_proc), 0x14);
        assert_eq!(size_of::<NpcRequest>(), 0x24);
        assert_eq!(offset_of!(NpcRequest, act_args), 0x04);
        assert_eq!(offset_of!(NpcRequest, head_target), 0x14);
        assert_eq!(size_of::<NormalNpcCtData>(), 0x1c);
        assert_eq!(offset_of!(NormalNpcCtData, schedule_type), 0x08);
    }

    struct Recorder(Vec<(u8, ActionPhase)>);

    impl ActionDispatcher for Recorder {
        fn dispatch(&mut self, index: u8, phase: ActionPhase, action: &mut NpcAction) {
            self.0.push((index, phase));
            if phase == ActionPhase::ChangeData {
                action.act_obj = 7;
                action.act_obj_id = 0x1234;
            }
        }

        fn resolve_object(&mut self, _action: &NpcAction) {
            self.0.push((0, ActionPhase::Main));
        }
    }

    #[test]
    fn request_equal_priority_and_dispatch_order_match_source() {
        let mut state = NpcBehavior::default();
        state.action.priority = 2;
        state.action.idx = 0;
        state.action.step = 0;
        let args = [1, 2, 3, 4, 5, 6];
        assert!(set_request_act(&mut state, 2, 1, 3, Some(&args)));

        let mut recorder = Recorder(Vec::new());
        action_proc(&mut state, &mut recorder).unwrap();

        assert_eq!(state.request.act_priority, 0);
        assert_eq!(state.action.prev_idx, 0);
        assert_eq!(state.action.idx, 1);
        assert_eq!(
            recorder.0,
            vec![
                (1, ActionPhase::ChangeData),
                (0, ActionPhase::Main),
                (1, ActionPhase::Init),
                (1, ActionPhase::Main),
            ]
        );
    }

    #[test]
    fn end_step_resets_priority_after_main_is_skipped() {
        let mut state = NpcBehavior::default();
        state.action.priority = 4;
        state.action.idx = 1;
        state.action.step = ACTION_END_STEP;
        let mut recorder = Recorder(Vec::new());
        action_proc(&mut state, &mut recorder).unwrap();
        assert_eq!(state.action.priority, 0);
        assert!(recorder.0.is_empty());
    }

    #[test]
    fn invalid_action_index_is_a_safe_boundary() {
        let mut state = NpcBehavior::default();
        assert!(set_request_act(&mut state, 1, ACTION_COUNT as u8, 0, None));
        let mut recorder = Recorder(Vec::new());
        assert_eq!(
            action_proc(&mut state, &mut recorder),
            Err(NpcError::InvalidActionIndex {
                index: ACTION_COUNT as u8
            })
        );
        assert!(recorder.0.is_empty());
    }

    #[test]
    fn priority_threshold_mutation_is_detectable() {
        let mut source = NpcBehavior::default();
        let mut mutated = source;
        assert!(set_request_act(&mut source, 1, 0, 0, None));
        let accepted_by_mutation = if 1 > mutated.request.act_priority {
            mutated.request.act_priority = 1;
            true
        } else {
            false
        };
        assert!(accepted_by_mutation);

        source.request.act_priority = 1;
        mutated.request.act_priority = 1;
        let source_equal = set_request_act(&mut source, 1, 2, 0, None);
        let mutation_equal = if 1 > mutated.request.act_priority {
            mutated.request.act_idx = 2;
            true
        } else {
            false
        };
        assert!(source_equal);
        assert!(!mutation_equal);
        assert_ne!(source.request.act_idx, mutated.request.act_idx);
    }

    struct LifecycleRecorder {
        events: Vec<&'static str>,
        construct_data: Option<NormalNpcCtData>,
        birth: bool,
    }

    impl NormalNpcClip for LifecycleRecorder {
        fn birth_check(&mut self, _actor: Handle, _game: Handle) -> bool {
            self.events.push("birth");
            self.birth
        }

        fn construct(&mut self, _actor: Handle, _game: Handle, data: &NormalNpcCtData) {
            self.events.push("construct");
            self.construct_data = Some(*data);
        }

        fn destroy(&mut self, _actor: Handle, _game: Handle) {
            self.events.push("destroy");
        }

        fn save(&mut self, _actor: Handle, _game: Handle) {
            self.events.push("save");
        }

        fn init(&mut self, _actor: Handle, _game: Handle) {
            self.events.push("init");
        }

        fn move_actor(&mut self, _actor: Handle, _game: Handle) {
            self.events.push("move");
        }

        fn draw(&mut self, _actor: Handle, _game: Handle) {
            self.events.push("draw");
        }
    }

    #[test]
    fn normal_lifecycle_preserves_birth_gate_and_forwarding_order() {
        let mut clip = LifecycleRecorder {
            events: Vec::new(),
            construct_data: None,
            birth: true,
        };
        normal_npc_ct(&mut clip, 1, 2);
        normal_npc_dt(&mut clip, 1, 2);
        normal_npc_save(&mut clip, 1, 2);
        normal_npc_init(&mut clip, 1, 2);
        normal_npc_move(&mut clip, 1, 2);
        normal_npc_draw(&mut clip, 1, 2);
        assert_eq!(
            clip.events,
            [
                "birth",
                "construct",
                "destroy",
                "save",
                "init",
                "move",
                "draw"
            ]
        );
        assert_eq!(clip.construct_data, Some(NORMAL_NPC_CT_DATA));

        clip.events.clear();
        clip.construct_data = None;
        clip.birth = false;
        normal_npc_ct(&mut clip, 1, 2);
        assert_eq!(clip.events, ["birth"]);
        assert_eq!(clip.construct_data, None);
    }
}
