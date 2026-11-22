//! Source-located parity tests for the ASE vertical slice.

use nookery_effects::{
    EffectController, EffectId, EffectState, EffectVec3, SpawnInput, SpawnOutcome, UpdateError,
    ACTIVE_EFFECT_MAX, EFFECT_ASE, EFFECT_ASE_CH, EFFECT_NUM,
};

fn input(id: u16, priority: i32) -> SpawnInput {
    SpawnInput {
        position: EffectVec3 {
            x: 10.0,
            y: 20.0,
            z: 30.0,
        },
        priority,
        angle: 1234,
        arg0: 77,
        arg1: -88,
        item_name: id,
    }
}

#[test]
fn creation_initialization_and_ignored_public_arguments_match_c() {
    // C: ef_ase.c:21-31 and :33-40; ef_effect_control.c:516-545.
    let mut controller = EffectController::new();
    let handle = match controller.make(EFFECT_ASE, input(7, 3)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    let effect = controller.snapshot(handle).expect("priority 3 is active");

    assert_eq!(effect.id, EffectId::Ase);
    assert_eq!(effect.item_name, 7);
    assert_eq!(effect.priority, 3);
    assert_eq!(effect.timer, 26);
    assert_eq!(effect.lifetime, 26.0);
    assert_eq!(effect.state, EffectState::Normal);
    assert_eq!(effect.position, input(7, 3).position);
    assert_eq!(
        effect.offset,
        EffectVec3 {
            x: 0.0,
            y: 15.0,
            z: 0.0
        }
    );
    assert_eq!(effect.effect_specific0, 0);
    assert_eq!(
        effect.scale,
        EffectVec3 {
            x: 0.01,
            y: 0.01,
            z: 0.01
        }
    );
}

#[test]
fn update_order_rearms_then_spawns_child_and_records_audio() {
    // C: ef_ase.c:36-45; ef_effect_control.c:414-461.
    let mut controller = EffectController::new();
    let parent = match controller.make(EFFECT_ASE, input(9, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };

    controller.update(1.0).unwrap();
    let first = controller.snapshot(parent).unwrap();
    assert_eq!(first.timer, 25);
    assert_eq!(first.lifetime, 25.0);
    assert_eq!(first.effect_specific0, 1);
    assert_eq!(controller.drain_audio_events().len(), 1);

    // The second tick starts with timer 25, so eAS_mv creates ASE_CH before
    // the controller decrements the newly created child's timer in pass 2.
    controller.update(1.0).unwrap();
    let events = controller.drain_audio_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sound_id, 0x29);
    assert_eq!(controller.active_count(), 2);

    let child = controller
        .snapshot(nookery_effects::EffectHandle { slot: 1 })
        .expect("child is allocated immediately after its parent");
    assert_eq!(child.id, EffectId::AseChild);
    assert_eq!(child.timer, 19, "new child is timer-decremented in pass 2");
    assert_eq!(
        child.lifetime, 20.0,
        "pass 1 happened before child creation"
    );
    let horizontal_speed =
        (child.velocity.x * child.velocity.x + child.velocity.y * child.velocity.y).sqrt();
    assert!((horizontal_speed - 2.7).abs() < 0.0001);
    assert_eq!(child.acceleration.y, -0.057499997);
}

#[test]
fn child_expires_and_cleanup_removes_it_after_twenty_60hz_ticks() {
    // C: ef_ase_ch.c:41-58; ef_effect_control.c:453-460.
    let mut controller = EffectController::new();
    let child = match controller.make(EFFECT_ASE_CH, input(11, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };

    for _ in 0..19 {
        controller.update(1.0).unwrap();
        assert!(controller.snapshot(child).is_some());
    }
    let before_expiry = controller.snapshot(child).unwrap();
    assert_eq!(before_expiry.timer, 1);
    assert_eq!(before_expiry.lifetime, 1.0);

    controller.update(1.0).unwrap();
    assert!(controller.snapshot(child).is_none());
    assert_eq!(controller.active_count(), 0);
}

#[test]
fn immediate_kill_matches_item_and_id_and_cleans_active_visibility() {
    // C: ef_effect_control.c:195-219; both profiles use eEC_IMMEDIATE_DEATH.
    let mut controller = EffectController::new();
    let parent = match controller.make(EFFECT_ASE, input(12, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    assert_eq!(controller.kill(EFFECT_ASE_CH, 12), 0);
    assert_eq!(controller.kill(EFFECT_ASE, 99), 0);
    assert_eq!(controller.kill(EFFECT_ASE, 12), 1);
    assert!(controller.snapshot(parent).is_none());
    assert_eq!(controller.active_count(), 0);

    assert!(matches!(
        controller.make(EFFECT_ASE, input(12, 1)),
        SpawnOutcome::Created(_)
    ));
    assert!(matches!(
        controller.make(EFFECT_ASE_CH, input(12, 1)),
        SpawnOutcome::Created(_)
    ));
    assert_eq!(controller.kill_all(12), 2);
    assert_eq!(controller.active_count(), 0);
}

#[test]
fn duplicate_feature_flag_and_dispatch_bounds_are_preserved() {
    // C: ef_effect_control.c:174-191; include/ef_effect_control.h enum bound.
    let mut controller = EffectController::new();
    assert!(matches!(
        controller.make(EFFECT_ASE, input(13, 1)),
        SpawnOutcome::Created(_)
    ));
    assert_eq!(
        controller.make(EFFECT_ASE, input(13, 2)),
        SpawnOutcome::IgnoredDuplicate
    );
    assert_eq!(
        controller.make(-1, input(14, 1)),
        SpawnOutcome::IgnoredInvalidId
    );
    assert_eq!(
        controller.make(EFFECT_NUM, input(15, 1)),
        SpawnOutcome::IgnoredInvalidId
    );
    assert_eq!(
        controller.make(EFFECT_NUM - 1, input(16, 1)),
        SpawnOutcome::UnsupportedEffect
    );
}

#[test]
fn fixed_pool_priority_byte_and_excluded_last_slot_match_c() {
    // C: ef_effect_control.c:475-504 and :526-545.
    let mut full = EffectController::new();
    for item in 0..ACTIVE_EFFECT_MAX {
        assert!(matches!(
            full.make(EFFECT_ASE_CH, input(item as u16, 2)),
            SpawnOutcome::Created(_)
        ));
    }
    assert_eq!(full.active_effect_num(), ACTIVE_EFFECT_MAX);
    assert_eq!(
        full.make(EFFECT_ASE_CH, input(200, 1)),
        SpawnOutcome::PoolFull
    );

    // With flags of 1, C's final fallback evicts slot 0, then lowers the
    // mutable active_effect_num to use_idx + 1.
    let mut replacement = EffectController::new();
    for item in 0..ACTIVE_EFFECT_MAX {
        assert!(matches!(
            replacement.make(EFFECT_ASE_CH, input(item as u16, 1)),
            SpawnOutcome::Created(_)
        ));
    }
    let replaced = replacement.make(EFFECT_ASE_CH, input(201, 1));
    assert_eq!(
        replaced,
        SpawnOutcome::Created(nookery_effects::EffectHandle { slot: 0 })
    );
    assert_eq!(replacement.active_effect_num(), 1);
    assert_eq!(replacement.active_count(), ACTIVE_EFFECT_MAX);
    assert!(replacement
        .snapshot(nookery_effects::EffectHandle { slot: 99 })
        .is_some());
}

#[test]
fn fractional_lifetime_and_four_tick_catch_up_boundary_are_observable() {
    // C: eEC_AllEffectMove pass-1 lifetime decrement and four-step cap.
    let mut controller = EffectController::new();
    let child = match controller.make(EFFECT_ASE_CH, input(21, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    controller.update(0.5).unwrap();
    let half = controller.snapshot(child).unwrap();
    assert_eq!(half.lifetime, 19.5);
    assert_eq!(half.timer, 20);
    controller.update(0.5).unwrap();
    let one = controller.snapshot(child).unwrap();
    assert_eq!(one.lifetime, 19.0);
    assert_eq!(one.timer, 19);

    controller.update(10.0).unwrap();
    let capped = controller.snapshot(child).unwrap();
    assert_eq!(capped.timer, 15, "only four 60 Hz steps are processed");
    assert_eq!(capped.lifetime, 9.0, "lifetime still sees the full dt");
}

#[test]
fn invalid_deltas_do_not_mutate_the_controller() {
    let mut controller = EffectController::new();
    let child = match controller.make(EFFECT_ASE_CH, input(22, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    assert_eq!(controller.update(-1.0), Err(UpdateError::NegativeDelta));
    assert_eq!(
        controller.update(f32::NAN),
        Err(UpdateError::NonFiniteDelta)
    );
    let effect = controller.snapshot(child).unwrap();
    assert_eq!(effect.timer, 20);
    assert_eq!(effect.lifetime, 20.0);
}

#[test]
fn draw_state_and_zero_priority_follow_c_visibility_rules() {
    // C: ef_ase.c:47-48; ef_ase_ch.c:60-75; controller active_flags byte.
    let mut controller = EffectController::new();
    let invisible = match controller.make(EFFECT_ASE_CH, input(25, 0)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    assert!(controller.snapshot(invisible).is_none());
    assert_eq!(controller.active_count(), 0);

    let parent = match controller.make(EFFECT_ASE, input(26, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    let parent_draw = controller.draw_state(parent).unwrap();
    assert_eq!(parent_draw.model_index, None);
    assert_eq!(
        parent_draw.scale,
        EffectVec3 {
            x: 0.01,
            y: 0.01,
            z: 0.01
        }
    );

    let child = match controller.make(EFFECT_ASE_CH, input(27, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    let child_draw = controller.draw_state(child).unwrap();
    assert_eq!(child_draw.model_index, Some(0));
    assert_eq!(
        child_draw.scale,
        EffectVec3 {
            x: 0.00027000002,
            y: 0.00027000002,
            z: 0.00027000002,
        }
    );
}

#[test]
fn mutation_guard_requires_both_parent_order_and_child_expiry_conditions() {
    // Focused mutation sentinel: changing either `timer & 1` or the expiry
    // predicate in the implementation must make the corresponding parity
    // assertion fail. The external mutation run records PASS_DETECTED.
    let mut controller = EffectController::new();
    assert!(matches!(
        controller.make(EFFECT_ASE, input(23, 1)),
        SpawnOutcome::Created(_)
    ));
    controller.update(2.0).unwrap();
    assert_eq!(controller.active_count(), 2);

    let mut child_only = EffectController::new();
    let child = match child_only.make(EFFECT_ASE_CH, input(24, 1)) {
        SpawnOutcome::Created(handle) => handle,
        outcome => panic!("unexpected make outcome: {outcome:?}"),
    };
    for _ in 0..20 {
        child_only.update(1.0).unwrap();
    }
    assert!(child_only.snapshot(child).is_none());
}

#[test]
fn machine_readable_fixture_matches_the_candidate_scope() {
    let fixture = include_str!("fixtures/ase_slice.json");
    assert!(fixture.contains("\"effect\": \"ASE\""));
    assert!(fixture.contains("\"child\": \"ASE_CH\""));
    assert!(fixture.contains("\"active_slots\": 100"));
    assert!(fixture.contains("\"catch_up_cap\": 4"));
}
