//! State machine port of `src/actor/ac_tunahiki_control.c`.

/// NPC state flag `FLAG_NPC_DELETE0` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_DELETE0: u16 = 1 << 0;
/// NPC state flag `FLAG_NPC_DELETE1` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_DELETE1: u16 = 1 << 1;
/// NPC state flag `FLAG_NPC_DELETE2` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_DELETE2: u16 = 1 << 2;
/// NPC state flag `FLAG_NPC_DELETE3` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_DELETE3: u16 = 1 << 3;
/// NPC state flag `FLAG_NPC_DELETE4` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_DELETE4: u16 = 1 << 4;
/// NPC state flag `FLAG_NPC_TALK0` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_TALK0: u16 = 1 << 5;
/// NPC state flag `FLAG_NPC_TALK1` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_TALK1: u16 = 1 << 6;
/// NPC state flag `FLAG_NPC_TALK2` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_TALK2: u16 = 1 << 7;
/// NPC state flag `FLAG_NPC_TALK3` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_TALK3: u16 = 1 << 8;
/// NPC state flag `FLAG_NPC_TALK4` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_TALK4: u16 = 1 << 9;
/// NPC state flag `FLAG_NPC_TALK_ALL` from `ac_tunahiki_control.c`.
pub const FLAG_NPC_TALK_ALL: u16 =
    FLAG_NPC_TALK0 | FLAG_NPC_TALK1 | FLAG_NPC_TALK2 | FLAG_NPC_TALK3 | FLAG_NPC_TALK4;
/// `FLAG_SHAKE` from `ac_tunahiki_control.c`.
pub const FLAG_SHAKE: u16 = 1 << 15;

/// `NPC_STATE0` from `ac_tunahiki_control.c`.
pub const NPC_STATE0: i16 = 0;

/// The event save area `aEv_tunahiki_c`, in source order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
/// `TugOfWarEvent` from `ac_tunahiki_control.c`.
pub struct TugOfWarEvent {
    /// `rope` as defined by the C source.
    pub rope: f32,
    /// `rope_base` as defined by the C source.
    pub rope_base: f32,
    /// `speed` as defined by the C source.
    pub speed: f32,
    /// `next_speed` as defined by the C source.
    pub next_speed: f32,
    /// `npc_state` as defined by the C source.
    pub npc_state: i16,
    /// `flag` as defined by the C source.
    pub flag: u16,
    /// `unknown_14` as defined by the C source.
    pub unknown_14: i16,
    /// `counter` as defined by the C source.
    pub counter: u16,
}

/// The only action in the C control actor is `aTNC_wait`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// `TunahikiControl` from `ac_tunahiki_control.c`.
pub struct TunahikiControl {
    /// `action` as defined by the C source.
    pub action: u8,
}

impl TunahikiControl {
    /// `aTNC_actor_ct` creates the save area and selects the wait action.
    pub fn new(event: &mut Option<TugOfWarEvent>) -> Self {
        if event.is_none() {
            *event = Some(TugOfWarEvent {
                npc_state: NPC_STATE0,
                ..TugOfWarEvent::default()
            });
        }
        Self { action: 0 }
    }

    /// One game-frame invocation of `aTNC_wait`.
    pub fn tick(&mut self, event: &mut TugOfWarEvent, game_frame: u32) {
        let shake = if event.flag & FLAG_SHAKE != 0 {
            if game_frame & 1 != 0 { 0.3 } else { -0.3 }
        } else {
            0.0
        };

        if event.flag & FLAG_SHAKE != 0 && event.flag & FLAG_NPC_TALK_ALL == 0 {
            event.rope_base += event.speed * 0.025;
            if event.rope_base < -10.0 {
                event.rope_base = -10.0;
                event.speed = 0.0;
            } else if event.rope_base > 10.0 {
                event.rope_base = 10.0;
                event.speed = 0.0;
            }
        }

        event.rope = event.rope_base + shake;
        event.counter = event.counter.wrapping_add(1);
        if event.counter >= 65 {
            event.counter = 2;
        }
    }
}
