#![allow(non_snake_case)]

use std::mem::size_of;

use nookery_game::{ControllerInput, ControllerState};

// Independent oracle fixture from include/m_controller.h, not values read
// from the Rust implementation under test.
const ORACLE_STICK_MIN: f32 = 9.899_495;
const ORACLE_STICK_MAX: f32 = 61.0;

fn oracle_percent(x: f32, shift: bool) -> f32 {
    if x.abs() <= ORACLE_STICK_MIN {
        0.0
    } else if x >= ORACLE_STICK_MAX {
        1.0
    } else if x <= -ORACLE_STICK_MAX {
        -1.0
    } else if shift {
        if x > 0.0 {
            (x - ORACLE_STICK_MIN) / (ORACLE_STICK_MAX - ORACLE_STICK_MIN)
        } else {
            (x + ORACLE_STICK_MIN) / (ORACLE_STICK_MAX - ORACLE_STICK_MIN)
        }
    } else {
        x / ORACLE_STICK_MAX
    }
}

#[test]
fn c_mCon_calc_deadzone_clears_outputs_but_preserves_angle_history() {
    let mut state = ControllerState {
        move_angle: 0x1234,
        ..ControllerState::default()
    };
    state.mcon_calc(4.0, -4.0);

    assert_eq!(state.move_px, 0.0);
    assert_eq!(state.move_py, 0.0);
    assert_eq!(state.move_pr, 0.0);
    assert_eq!(state.adjusted_pr, 0.0);
    assert_eq!(state.last_move_angle, 0x1234);
}

#[test]
fn c_mCon_calc_matches_independent_oracle_at_thresholds_and_signed_axes() {
    let mut state = ControllerState {
        move_angle: 0x1234,
        ..ControllerState::default()
    };
    state.mcon_calc(ORACLE_STICK_MIN, 0.0);
    assert_eq!(state.move_px, 0.0);
    assert_eq!(state.adjusted_px, 0.0);

    state.mcon_calc(-30.5, 0.0);
    assert!((state.move_px - oracle_percent(-30.5, false)).abs() < 1e-6);
    assert!((state.adjusted_px - oracle_percent(-30.5, true)).abs() < 1e-6);
    assert!((state.move_pr - oracle_percent(30.5, false)).abs() < 1e-6);
    assert!((state.adjusted_pr - oracle_percent(30.5, true)).abs() < 1e-6);
    assert_eq!(state.move_angle, i16::MIN);
}

#[test]
fn c_mCon_calc_clamps_before_scaling_and_snapshots_in_update_order() {
    let mut state = ControllerState::default();
    state.mcon_calc(20.0, 20.0);
    let previous = state;
    state.mcon_calc(100.0, 0.0);

    assert_eq!(state.last_move_px, previous.move_px);
    assert_eq!(state.last_move_py, previous.move_py);
    assert_eq!(state.last_adjusted_pr, previous.adjusted_pr);
    assert_eq!(state.move_px, 1.0);
    assert_eq!(state.move_pr, 1.0);
    assert_eq!(state.adjusted_px, 1.0);
}

#[test]
fn c_mCon_input_mapping_masks_buttons_and_zeroes_title_demo() {
    let input = ControllerInput {
        now_buttons: 0x8000 | 0x4000,
        triggered_buttons: 0x8000,
        stick_x: 30,
        stick_y: -20,
        ..ControllerInput::default()
    };
    assert!(input.chk_button(0x8000 | 0x4000));
    assert!(!input.chk_button(0x8000 | 0x0100));
    assert!(input.chk_trigger(0x8000));
    assert_eq!(input.get_joystick_x(), 30);
    assert_eq!(input.get_joystick_y(), -20);

    let demo = ControllerInput {
        title_demo: true,
        ..input
    };
    assert_eq!(demo.get_button(), 0);
    assert_eq!(demo.get_trigger(), 0);
    assert_eq!(demo.get_joystick_x(), 0);
    assert!(!demo.chk_button(0));
}

#[test]
fn c_mCon_main_reads_the_mapped_joystick_and_title_demo_zeroes_it() {
    let input = ControllerInput {
        stick_x: 30,
        stick_y: -20,
        ..ControllerInput::default()
    };
    let mut state = ControllerState::default();
    input.mcon_main(&mut state);
    assert!(state.move_pr > 0.0);
    assert!(state.move_px > 0.0);
    assert!(state.move_py < 0.0);
    assert!(state.adjusted_py < 0.0);

    let demo = ControllerInput {
        title_demo: true,
        ..input
    };
    demo.mcon_main(&mut state);
    assert_eq!(state.move_pr, 0.0);
}

#[test]
fn mutation_source_boundary_and_ordering_checks_are_live() {
    // PASS_DETECTED: weakening the `<= STICK_MIN` source boundary, or moving
    // the snapshot after the calculation, makes these assertions fail.
    let mut state = ControllerState {
        move_angle: 0x1234,
        ..ControllerState::default()
    };
    state.mcon_calc(20.0, 0.0);
    let prior = state;
    state.mcon_calc(0.0, ORACLE_STICK_MIN);
    assert_eq!(state.move_px, 0.0);
    assert_eq!(state.move_pr, 0.0);
    assert_eq!(state.move_angle, prior.move_angle);
    assert_eq!(state.last_move_pr, prior.move_pr);
    assert_eq!(state.last_adjusted_pr, prior.adjusted_pr);
}

#[test]
fn c_MCON_layout_preserves_persistent_bytes() {
    assert_eq!(size_of::<ControllerState>(), 0x38);
}
