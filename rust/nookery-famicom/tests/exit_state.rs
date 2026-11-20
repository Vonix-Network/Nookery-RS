//! Exit state machine: load-failure precedence, both combos, exact-match
//! strictness, countdown `60/59/1/0`, repeated input while exiting, and the
//! per-frame decisions including the PC audio pump.

use nookery_famicom::{
    BUTTON_A, BUTTON_DDOWN, BUTTON_DLEFT, BUTTON_DRIGHT, BUTTON_DUP, BUTTON_L, BUTTON_R,
    BUTTON_START, BUTTON_X, BUTTON_Y, BUTTON_Z, EmulatorExit, ExitPhase, FADEOUT_FRAMES,
    FrameAction, LoadResult, PAD_COUNT, Platform, dispatch, is_exit_combo,
};

fn running(platform: Platform) -> EmulatorExit {
    EmulatorExit::new(platform)
}

fn advance(pump_audio: bool) -> FrameAction {
    FrameAction::AdvanceFrame { pump_audio }
}

#[test]
fn fresh_state_is_running_with_zero_countdown() {
    let exit = running(Platform::Pc);
    assert_eq!(exit.phase(), ExitPhase::Running);
    assert!(!exit.is_done());
    assert_eq!(exit.countdown(), 0);
    assert_eq!(exit.platform(), Platform::Pc);
}

#[test]
fn combo_masks_match_the_upstream_bit_values() {
    assert_eq!(BUTTON_L | BUTTON_R | BUTTON_Z, 0x2030);
    assert_eq!(BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y, 0x00F0);
    assert!(is_exit_combo(0x2030));
    assert!(is_exit_combo(0x00F0));
}

#[test]
fn both_exit_combos_latch_a_sixty_frame_fade() {
    for combo in [
        BUTTON_L | BUTTON_R | BUTTON_Z,
        BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y,
    ] {
        let mut exit = running(Platform::Pc);
        assert!(exit.observe_exit_combo(combo), "combo {combo:#06x}");
        assert!(exit.is_done());
        assert_eq!(
            exit.phase(),
            ExitPhase::FadeOut {
                remaining: FADEOUT_FRAMES
            }
        );
        assert_eq!(exit.countdown(), 60);
    }
}

#[test]
fn non_combos_do_not_latch() {
    let cases = [
        0,
        BUTTON_L,
        BUTTON_R,
        BUTTON_Z,
        BUTTON_X,
        BUTTON_Y,
        BUTTON_L | BUTTON_R,
        BUTTON_L | BUTTON_Z,
        BUTTON_R | BUTTON_Z,
        BUTTON_L | BUTTON_R | BUTTON_X,
        BUTTON_L | BUTTON_R | BUTTON_Y,
        BUTTON_X | BUTTON_Y,
    ];
    for combo in cases {
        let mut exit = running(Platform::Pc);
        assert!(!exit.observe_exit_combo(combo), "combo {combo:#06x}");
        assert_eq!(exit.phase(), ExitPhase::Running, "combo {combo:#06x}");
    }
}

#[test]
fn combo_match_is_exact_so_extra_buttons_cancel_the_exit() {
    // `combo == (L|R|Z)` is equality, not a superset test: holding any extra
    // button while pressing the combo must not exit.
    let extras = [
        BUTTON_A,
        BUTTON_START,
        BUTTON_DUP,
        BUTTON_DDOWN,
        BUTTON_DLEFT,
        BUTTON_DRIGHT,
        0x0001,
        0x0002,
        0x0004,
        0x0008,
    ];
    for extra in extras {
        let mut exit = running(Platform::Pc);
        assert!(
            !exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z | extra),
            "extra {extra:#06x}"
        );
        assert_eq!(exit.phase(), ExitPhase::Running, "extra {extra:#06x}");
    }
}

#[test]
fn load_failure_returns_immediately_without_a_fade() {
    let mut exit = running(Platform::Pc);
    assert!(exit.observe_rom_load(LoadResult::Failed));
    assert_eq!(exit.phase(), ExitPhase::LoadFailed);
    assert!(exit.is_done());
    assert_eq!(exit.countdown(), 0);
    assert_eq!(exit.tick(), FrameAction::ReturnToGame);
    assert_eq!(exit.phase(), ExitPhase::ReadyToReturn);
    assert_eq!(exit.tick(), FrameAction::ReturnToGame);
}

#[test]
fn load_failure_precedes_the_combo_scan() {
    let mut exit = running(Platform::Pc);
    assert!(exit.observe_rom_load(LoadResult::Failed));
    // A combo arriving afterwards must not start a fade.
    assert!(!exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z));
    assert_eq!(exit.phase(), ExitPhase::LoadFailed);
    assert_eq!(exit.countdown(), 0);
    assert_eq!(exit.tick(), FrameAction::ReturnToGame);
}

#[test]
fn successful_load_keeps_running_and_clears_nothing() {
    let mut exit = running(Platform::Pc);
    assert!(!exit.observe_rom_load(LoadResult::Loaded));
    assert_eq!(exit.phase(), ExitPhase::Running);
    assert_eq!(exit.tick(), advance(true));
}

#[test]
fn repeated_load_results_after_done_are_ignored() {
    let mut exit = running(Platform::Pc);
    assert!(exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z));
    assert!(!exit.observe_rom_load(LoadResult::Failed));
    assert_eq!(exit.phase(), ExitPhase::FadeOut { remaining: 60 });

    let mut failed = running(Platform::Pc);
    assert!(failed.observe_rom_load(LoadResult::Failed));
    assert!(!failed.observe_rom_load(LoadResult::Loaded));
    assert_eq!(failed.phase(), ExitPhase::LoadFailed);
}

#[test]
fn repeated_combo_while_exiting_does_not_restart_the_countdown() {
    let mut exit = running(Platform::Pc);
    assert!(exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z));
    for _ in 0..10 {
        let _ = exit.tick();
    }
    // 60 → 50 after ten black frames.
    assert_eq!(exit.countdown(), 50);
    for _ in 0..5 {
        assert!(!exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y));
    }
    assert_eq!(exit.countdown(), 50);
}

#[test]
fn countdown_goes_sixty_fifty_nine_then_one_then_zero() {
    let mut exit = running(Platform::GameCube);
    assert!(exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z));

    // The triggering frame is the first tick: 60 → 59, still black.
    assert_eq!(exit.tick(), FrameAction::PresentBlack);
    assert_eq!(exit.countdown(), 59);
    assert_eq!(exit.phase(), ExitPhase::FadeOut { remaining: 59 });

    // Drain to 1: frames 2..59 of the fade.
    for _ in 0..58 {
        assert_eq!(exit.tick(), FrameAction::PresentBlack);
    }
    assert_eq!(exit.countdown(), 1);
    assert_eq!(exit.phase(), ExitPhase::FadeOut { remaining: 1 });

    // The 60th black frame reaches 0 but still presents black.
    assert_eq!(exit.tick(), FrameAction::PresentBlack);
    assert_eq!(exit.countdown(), 0);
    assert_eq!(exit.phase(), ExitPhase::ReadyToReturn);

    // The next frame is the return, and it stays returned.
    assert_eq!(exit.tick(), FrameAction::ReturnToGame);
    assert_eq!(exit.tick(), FrameAction::ReturnToGame);
}

#[test]
fn fade_out_lasts_exactly_sixty_black_frames() {
    let mut exit = running(Platform::Pc);
    assert!(exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z));

    let mut black = 0usize;
    for _ in 0..FADEOUT_FRAMES {
        assert_eq!(exit.tick(), FrameAction::PresentBlack);
        black += 1;
    }
    assert_eq!(black, 60);
    assert_eq!(exit.countdown(), 0);
    assert_eq!(exit.tick(), FrameAction::ReturnToGame);
}

#[test]
fn first_matching_pad_wins_and_scan_stops() {
    let mut exit = running(Platform::Pc);
    let pads = [
        0,
        0,
        BUTTON_L | BUTTON_R | BUTTON_Z,
        BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y,
    ];
    assert_eq!(exit.observe_pads(&pads), Some(2));
    assert_eq!(exit.countdown(), 60);

    let mut none = running(Platform::Pc);
    let pads = [BUTTON_A, BUTTON_L, BUTTON_R, BUTTON_Z];
    assert_eq!(none.observe_pads(&pads), None);
    assert_eq!(none.phase(), ExitPhase::Running);
}

#[test]
fn every_pad_slot_can_trigger_the_exit() {
    for padid in 0..PAD_COUNT {
        let mut exit = running(Platform::Pc);
        let mut pads = [0u16; PAD_COUNT];
        pads[padid] = BUTTON_L | BUTTON_R | BUTTON_Y | BUTTON_X;
        assert_eq!(exit.observe_pads(&pads), Some(padid));
        assert_eq!(exit.countdown(), 60);
    }
}

#[test]
fn pc_pumps_audio_before_each_frame_and_gamecube_does_not() {
    let mut pc = running(Platform::Pc);
    assert_eq!(pc.tick(), advance(true));

    let mut gc = running(Platform::GameCube);
    assert_eq!(gc.tick(), advance(false));

    // The pump only applies while running; the fade presents black instead.
    let mut pc_fading = running(Platform::Pc);
    assert!(pc_fading.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z));
    assert_eq!(pc_fading.tick(), FrameAction::PresentBlack);
}

#[test]
fn frame_action_predicates_match_the_upstream_body() {
    assert!(!advance(true).clears_efb());
    assert!(!advance(false).clears_efb());
    assert!(advance(true).pumps_audio());
    assert!(!advance(false).pumps_audio());
    assert!(FrameAction::PresentBlack.clears_efb());
    assert!(!FrameAction::PresentBlack.pumps_audio());
    // Upstream has no early return, so the return frame still clears the EFB.
    assert!(FrameAction::ReturnToGame.clears_efb());
    assert!(!FrameAction::ReturnToGame.pumps_audio());
}

#[test]
fn platform_reports_the_audio_pump_it_selects() {
    assert!(Platform::Pc.pumps_audio_before_frame());
    assert!(!Platform::GameCube.pumps_audio_before_frame());
}

/// Records seam calls in order so frame ordering is verifiable.
#[derive(Default)]
struct Recorder {
    calls: Vec<&'static str>,
    fades: Vec<u16>,
}

impl nookery_famicom::HostSeam for Recorder {
    fn begin_frame(&mut self) {
        self.calls.push("begin");
    }
    fn pump_audio(&mut self) {
        self.calls.push("audio");
    }
    fn advance_famicom_frame(&mut self) {
        self.calls.push("frame");
    }
    fn present_black(&mut self) {
        self.calls.push("black");
    }
    fn return_to_game(&mut self) {
        self.calls.push("return");
    }
    fn end_frame(&mut self) {
        self.calls.push("end");
    }
    fn request_fade_out(&mut self, frames: u16) {
        self.fades.push(frames);
    }
    fn report_load_failure(&mut self) {
        self.calls.push("load-failed");
    }
}

#[test]
fn dispatch_runs_audio_then_one_frame_between_begin_and_end() {
    let mut exit = running(Platform::Pc);
    let mut host = Recorder::default();
    let action = dispatch(&mut exit, &mut host, Some(LoadResult::Loaded), None);
    assert_eq!(action, advance(true));
    assert_eq!(host.calls, vec!["begin", "audio", "frame", "end"]);
    assert!(host.fades.is_empty());
}

#[test]
fn dispatch_on_gamecube_skips_the_audio_pump() {
    let mut exit = running(Platform::GameCube);
    let mut host = Recorder::default();
    assert_eq!(dispatch(&mut exit, &mut host, None, None), advance(false));
    assert_eq!(host.calls, vec!["begin", "frame", "end"]);
}

#[test]
fn dispatch_requests_a_sixty_frame_fade_on_the_triggering_frame() {
    let mut exit = running(Platform::Pc);
    let mut host = Recorder::default();
    let pads = [0, BUTTON_L | BUTTON_R | BUTTON_Z, 0, 0];
    let action = dispatch(&mut exit, &mut host, Some(LoadResult::Loaded), Some(&pads));
    assert_eq!(host.fades, vec![60]);
    assert_eq!(action, FrameAction::PresentBlack);
    // Fade request happens before the frame body runs.
    assert_eq!(host.calls, vec!["begin", "black", "end"]);
    assert_eq!(exit.countdown(), 59);
}

#[test]
fn dispatch_reports_load_failure_and_returns_the_same_frame() {
    let mut exit = running(Platform::Pc);
    let mut host = Recorder::default();
    let action = dispatch(&mut exit, &mut host, Some(LoadResult::Failed), None);
    assert_eq!(action, FrameAction::ReturnToGame);
    // No fade was requested and no emulator frame ran.
    assert!(host.fades.is_empty());
    assert_eq!(
        host.calls,
        vec!["load-failed", "begin", "return", "black", "end"]
    );
}

#[test]
fn dispatch_returns_to_game_after_the_full_fade() {
    let mut exit = running(Platform::Pc);
    let mut host = Recorder::default();
    let pads = [BUTTON_L | BUTTON_R | BUTTON_Z, 0, 0, 0];
    let _ = dispatch(&mut exit, &mut host, Some(LoadResult::Loaded), Some(&pads));
    host.calls.clear();

    for _ in 0..FADEOUT_FRAMES - 1 {
        assert_eq!(
            dispatch(&mut exit, &mut host, None, None),
            FrameAction::PresentBlack
        );
    }
    assert_eq!(exit.countdown(), 0);
    host.calls.clear();

    assert_eq!(
        dispatch(&mut exit, &mut host, None, None),
        FrameAction::ReturnToGame
    );
    assert_eq!(host.calls, vec!["begin", "return", "black", "end"]);
    // Still returning on later frames, no emulator frame ever runs again.
    host.calls.clear();
    assert_eq!(
        dispatch(&mut exit, &mut host, None, None),
        FrameAction::ReturnToGame
    );
    assert!(!host.calls.contains(&"frame"));
    assert!(!host.calls.contains(&"audio"));
}

#[test]
fn dispatch_ignores_late_combos_once_returning() {
    let mut exit = running(Platform::Pc);
    let mut host = Recorder::default();
    let _ = dispatch(&mut exit, &mut host, Some(LoadResult::Failed), None);
    let pads = [BUTTON_L | BUTTON_R | BUTTON_Z, 0, 0, 0];
    let _ = dispatch(&mut exit, &mut host, Some(LoadResult::Loaded), Some(&pads));
    // Done already: the combo cannot re-latch, so no fade is ever requested.
    assert!(host.fades.is_empty());
    assert_eq!(
        host.calls,
        vec![
            "load-failed",
            "begin",
            "return",
            "black",
            "end", // failing frame
            "begin",
            "return",
            "black",
            "end", // late-combo frame, still returning
        ]
    );
    assert_eq!(exit.phase(), ExitPhase::ReadyToReturn);
    assert_eq!(exit.countdown(), 0);
}

#[test]
fn duplicate_bits_do_not_change_the_combo_value() {
    // `combo` is a bitwise OR, so pressing Z twice is indistinguishable from
    // pressing it once — an exact-value match still fires.
    let mut exit = running(Platform::Pc);
    assert!(exit.observe_exit_combo(BUTTON_L | BUTTON_R | BUTTON_Z | BUTTON_Z));
    assert_eq!(exit.countdown(), 60);
}
