use nookery_actor_port::tunahiki::{
    FLAG_NPC_TALK_ALL, FLAG_SHAKE, NPC_STATE0, TugOfWarEvent, TunahikiControl,
};

#[test]
fn construction_reserves_the_same_zeroed_event_state() {
    let mut event = None;
    let control = TunahikiControl::new(&mut event);
    assert_eq!(control.action, 0);
    assert_eq!(
        event.unwrap(),
        TugOfWarEvent {
            npc_state: NPC_STATE0,
            ..TugOfWarEvent::default()
        }
    );
}

#[test]
fn shake_updates_rope_before_the_counter_wrap() {
    let mut event = TugOfWarEvent {
        flag: FLAG_SHAKE,
        rope_base: 9.99,
        speed: 1.0,
        counter: 64,
        ..TugOfWarEvent::default()
    };
    let mut control = TunahikiControl::default();
    control.tick(&mut event, 3);
    assert!((event.rope_base - 10.0).abs() < f32::EPSILON);
    assert!((event.rope - 10.3).abs() < f32::EPSILON);
    assert_eq!(event.speed, 0.0);
    assert_eq!(event.counter, 2);
}

#[test]
fn all_talking_freezes_base_but_keeps_the_visual_shake() {
    let mut event = TugOfWarEvent {
        flag: FLAG_SHAKE | FLAG_NPC_TALK_ALL,
        rope_base: 2.0,
        speed: 8.0,
        ..TugOfWarEvent::default()
    };
    let mut control = TunahikiControl::default();
    control.tick(&mut event, 4);
    assert_eq!(event.rope_base, 2.0);
    assert!((event.rope - 1.7).abs() < f32::EPSILON);
    assert_eq!(event.speed, 8.0);
}
