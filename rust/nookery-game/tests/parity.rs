use std::cell::RefCell;
use std::rc::Rc;

use nookery_game::{ExecutorPlan, FrameClock, GameError};

#[test]
fn ticks_carries_fractional_frames_and_caps_whole_ticks() {
    let mut accum = 0.0;
    assert_eq!(FrameClock::ticks_for_frame(0.75, &mut accum).unwrap(), 0);
    assert_eq!(accum, 0.75);
    assert_eq!(FrameClock::ticks_for_frame(0.5, &mut accum).unwrap(), 1);
    assert_eq!(accum, 0.25);
    assert_eq!(FrameClock::ticks_for_frame(9.9, &mut accum).unwrap(), 4);
    assert!((accum - 0.15).abs() < 1e-12);
}

#[test]
fn period_elapsed_subtracts_one_period_and_discards_excess() {
    let mut accum = 1.5;
    assert!(FrameClock::period_elapsed(0.5, &mut accum, 2.0).unwrap());
    assert_eq!(accum, 0.0);

    let mut excess = 10.5;
    assert!(FrameClock::period_elapsed(0.5, &mut excess, 2.0).unwrap());
    assert_eq!(excess, 0.0);
}

#[test]
fn period_elapsed_boundary_does_not_fire_before_period() {
    let mut before = 1.999;
    assert!(!FrameClock::period_elapsed(0.0009, &mut before, 2.0).unwrap());
    assert!(FrameClock::period_elapsed(0.0001, &mut before, 2.0).unwrap());
}

#[test]
fn frame_time_and_phase_match_graph_frame_units() {
    assert_eq!(FrameClock::frame_time(120.5).unwrap(), 120.5);
    assert_eq!(FrameClock::frame_phase(9.99, 4).unwrap(), 1);
    assert_eq!(FrameClock::frame_phase(8.0, 4).unwrap(), 0);
}

#[test]
fn invalid_and_non_positive_inputs_are_typed_errors() {
    let mut accum = 0.0;
    assert_eq!(
        FrameClock::ticks_for_frame(0.0, &mut accum),
        Err(GameError::NonPositiveInput("dt_num_60fps_frames"))
    );
    assert_eq!(
        FrameClock::period_elapsed(1.0, &mut accum, -1.0),
        Err(GameError::NonPositiveInput("period_frames"))
    );
    assert_eq!(
        FrameClock::frame_phase(1.0, 0),
        Err(GameError::NonPositiveInput("period_frames"))
    );
    assert_eq!(
        FrameClock::frame_time(f64::NAN),
        Err(GameError::InvalidInput("total_60fps_frames"))
    );
    let mut invalid_accum = f64::INFINITY;
    assert_eq!(
        FrameClock::ticks_for_frame(1.0, &mut invalid_accum),
        Err(GameError::InvalidInput("accum"))
    );
}

#[test]
fn executor_skips_constructors_but_runs_main_and_reverse_destructors() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let main_events = Rc::clone(&events);
    let construct_a_events = Rc::clone(&events);
    let construct_b_events = Rc::clone(&events);
    let destruct_a_events = Rc::clone(&events);
    let destruct_b_events = Rc::clone(&events);
    ExecutorPlan::new(move || main_events.borrow_mut().push("main"))
        .constructor(move || construct_a_events.borrow_mut().push("construct-a"))
        .constructor(move || construct_b_events.borrow_mut().push("construct-b"))
        .destructor(move || destruct_a_events.borrow_mut().push("destruct-a"))
        .destructor(move || destruct_b_events.borrow_mut().push("destruct-b"))
        .run(true);
    assert_eq!(
        events.borrow().as_slice(),
        ["main", "destruct-b", "destruct-a"]
    );
}

#[test]
fn executor_runs_constructors_in_registration_order() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let main_events = Rc::clone(&events);
    let construct_a_events = Rc::clone(&events);
    let construct_b_events = Rc::clone(&events);
    let destruct_a_events = Rc::clone(&events);
    let destruct_b_events = Rc::clone(&events);
    ExecutorPlan::new(move || main_events.borrow_mut().push("main"))
        .constructor(move || construct_a_events.borrow_mut().push("construct-a"))
        .constructor(move || construct_b_events.borrow_mut().push("construct-b"))
        .destructor(move || destruct_a_events.borrow_mut().push("destruct-a"))
        .destructor(move || destruct_b_events.borrow_mut().push("destruct-b"))
        .run(false);
    assert_eq!(
        events.borrow().as_slice(),
        [
            "construct-a",
            "construct-b",
            "main",
            "destruct-b",
            "destruct-a"
        ]
    );
}

#[test]
fn mutation_checks_fail_for_changed_cap_and_destructor_order() {
    let mut accum = 0.0;
    assert_eq!(FrameClock::ticks_for_frame(8.0, &mut accum).unwrap(), 4);

    let events = Rc::new(RefCell::new(Vec::new()));
    let main_events = Rc::clone(&events);
    let first_events = Rc::clone(&events);
    let second_events = Rc::clone(&events);
    ExecutorPlan::new(move || main_events.borrow_mut().push("main"))
        .destructor(move || first_events.borrow_mut().push("first"))
        .destructor(move || second_events.borrow_mut().push("second"))
        .run(false);
    assert_eq!(events.borrow().as_slice(), ["main", "second", "first"]);
}
