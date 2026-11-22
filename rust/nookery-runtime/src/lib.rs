//! Deterministic application-runtime boundary for the Nookery game.
//!
//! The crate models the lifecycle contracts in src/main.c, src/graph.c,
//! src/game.c, and src/executor.c without owning a renderer, audio device,
//! platform, game data, or process-global state. The parent application must
//! still wire this crate into the real CMake/application path before it can
//! replace the C main/graph/game path.

mod adapters;
mod error;
mod executor;
mod ffi;
mod runtime;
mod schedule;

pub use adapters::{AudioAdapter, HostErrorCode, PlatformAdapter, RendererAdapter};
pub use error::{CallbackKind, Operation, RuntimeError};
pub use executor::{Executor, ExecutorError, ExecutorReport, StackFrame, UnresolvedReport};
pub use ffi::{
    NookeryRuntimeCallbackStatus, NookeryRuntimeCallbacks, NookeryRuntimeConfig,
    NookeryRuntimeFrameCallback, NookeryRuntimeGap, NookeryRuntimeHandle,
    NookeryRuntimeResizeCallback, NookeryRuntimeScreenCallback, NookeryRuntimeStatus,
    NookeryRuntimeVoidCallback, RUNTIME_GAPS, nookery_runtime_boot, nookery_runtime_create,
    nookery_runtime_destroy, nookery_runtime_frame, nookery_runtime_prenmi, nookery_runtime_reset,
    nookery_runtime_resize, nookery_runtime_shutdown,
};
pub use runtime::{
    CallbackRegistry, FrameCallback, FrameContext, FrameReport, GameCallback, GameState,
    GameTransition, GraphState, Lifecycle, LifecycleState, ResetStatus, ResizeCallback, Runtime,
    RuntimeConfig, RuntimeSnapshot,
};
pub use schedule::{AudioSchedule, Timing};

/// Source constants preserved by the runtime boundary.
pub const BASE_FRAME_RATE: f64 = 60.0;
pub const MAX_DT_60FPS_FRAMES: f64 = 4.0;
pub const MAX_AUDIO_TICKS: u32 = 4;
pub const DEFAULT_SCREEN_WIDTH: u32 = 320;
pub const DEFAULT_SCREEN_HEIGHT: u32 = 240;
pub const GAME_FRAME: u8 = 1;
pub const GAME_HYRAL_SIZE: usize = 0x100000;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::{
        AudioAdapter, AudioSchedule, Executor, HostErrorCode, Lifecycle, LifecycleState,
        PlatformAdapter, RendererAdapter, Runtime, RuntimeError,
    };
    use nookery_game::FrameClock;

    #[derive(Clone)]
    struct TestPlatform {
        events: Rc<RefCell<Vec<&'static str>>>,
        reset: bool,
    }

    impl PlatformAdapter for TestPlatform {
        fn set_screen_size(&mut self, _: u32, _: u32) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("screen");
            Ok(())
        }
        fn frame_begin(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("platform_begin");
            Ok(())
        }
        fn frame_end(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("platform_end");
            Ok(())
        }
        fn initialize_card(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("card");
            Ok(())
        }
        fn create_irq_manager(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("irq");
            Ok(())
        }
        fn create_input_manager(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("input_manager");
            Ok(())
        }
        fn initialize_display(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("display_init");
            Ok(())
        }
        fn initialize_messages(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("messages");
            Ok(())
        }
        fn enable_land_start(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("land_start");
            Ok(())
        }
        fn mount_archive(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("archive");
            Ok(())
        }
        fn dump_aram_heap(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("aram_dump");
            Ok(())
        }
        fn monotonic_seconds(&mut self) -> Result<f64, HostErrorCode> {
            Ok(0.0)
        }
        fn reset_requested(&mut self) -> Result<bool, HostErrorCode> {
            Ok(self.reset)
        }
        fn shutdown(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("platform_shutdown");
            Ok(())
        }
    }

    struct TestRenderer {
        events: Rc<RefCell<Vec<&'static str>>>,
        fail_finish: bool,
    }

    impl RendererAdapter for TestRenderer {
        fn prepare_frame(&mut self, _: &super::FrameContext) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("prepare");
            Ok(())
        }
        fn draw_first(&mut self, _: &super::FrameContext) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("draw_first");
            Ok(())
        }
        fn finish_frame(&mut self, _: &super::FrameContext) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("finish");
            if self.fail_finish {
                Err(HostErrorCode::Failed)
            } else {
                Ok(())
            }
        }
        fn submit_frame(&mut self, _: &super::FrameContext) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("submit");
            Ok(())
        }
        fn shutdown(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("renderer_shutdown");
            Ok(())
        }
    }

    struct TestAudio {
        events: Rc<RefCell<Vec<&'static str>>>,
    }

    impl AudioAdapter for TestAudio {
        fn initialize(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("audio_init");
            Ok(())
        }
        fn bgm_update(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("bgm_adapter");
            Ok(())
        }
        fn game_tick(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("audio_tick");
            Ok(())
        }
        fn reset(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("audio_reset");
            Ok(())
        }
        fn cleanup(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("audio_cleanup");
            Ok(())
        }
        fn shutdown(&mut self) -> Result<(), HostErrorCode> {
            self.events.borrow_mut().push("audio_shutdown");
            Ok(())
        }
    }

    type TestRuntime = Runtime<TestPlatform, TestRenderer, TestAudio>;

    fn runtime(reset: bool, fail_finish: bool) -> (TestRuntime, Rc<RefCell<Vec<&'static str>>>) {
        let events = Rc::new(RefCell::new(Vec::new()));
        let platform = TestPlatform {
            events: events.clone(),
            reset,
        };
        let renderer = TestRenderer {
            events: events.clone(),
            fail_finish,
        };
        let audio = TestAudio {
            events: events.clone(),
        };
        (Runtime::new(platform, renderer, audio), events)
    }

    #[test]
    fn source_order_is_boot_frame_shutdown() {
        let mut state = LifecycleState::new();
        assert_eq!(state.phase(), Lifecycle::Cold);
        state.boot().expect("cold state can boot");
        assert_eq!(state.phase(), Lifecycle::Booted);
        state.begin_frame().expect("booted state can frame");
        state.finish_frame().expect("frame can finish");
        assert_eq!(state.phase(), Lifecycle::Booted);
        state.shutdown().expect("booted state can shutdown");
        assert_eq!(state.phase(), Lifecycle::Shutdown);
    }

    #[test]
    fn public_frame_path_reaches_callbacks_in_source_order() {
        let (mut runtime, events) = runtime(false, false);
        let callback_events = events.clone();
        runtime.callbacks_mut().register_input(move |_| {
            callback_events.borrow_mut().push("input");
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_update(move |_, _| {
            callback_events.borrow_mut().push("update");
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_execute(move |_, _| {
            callback_events.borrow_mut().push("execute");
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_bgm(move |_, _| {
            callback_events.borrow_mut().push("bgm");
            Ok(())
        });
        runtime.boot().unwrap();
        runtime.run_frame(1.0 / 60.0).unwrap();
        let events = events.borrow();
        let frame = &events[events.iter().position(|event| *event == "prepare").unwrap()..];
        assert_eq!(
            &frame[..10],
            &[
                "prepare",
                "input",
                "draw_first",
                "update",
                "execute",
                "bgm",
                "bgm_adapter",
                "finish",
                "submit",
                "audio_tick",
            ]
        );
    }

    #[test]
    fn dt_is_capped_and_audio_ticks_are_bounded() {
        let (mut runtime, _) = runtime(false, false);
        runtime.boot().unwrap();
        let report = runtime.run_frame(1.0).unwrap();
        assert_eq!(report.audio_ticks, 4);
        assert_eq!(runtime.snapshot().graph.dt_num_60fps_frames, 4.0);
        assert_eq!(runtime.snapshot().audio_accumulator, 0.0);
    }

    #[test]
    fn runtime_uses_game_frame_clock_and_executor_order() {
        let (mut runtime, events) = runtime(false, false);
        let callback_events = events.clone();
        runtime.callbacks_mut().register_game_init(move |_| {
            callback_events.borrow_mut().push("game_init");
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_update(move |_, _| {
            callback_events.borrow_mut().push("update");
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_game_cleanup(move |_| {
            callback_events.borrow_mut().push("game_cleanup");
            Ok(())
        });

        runtime.boot().unwrap();
        let first = runtime.run_frame(0.5 / 60.0).unwrap();
        let second = runtime.run_frame(0.5 / 60.0).unwrap();
        runtime.shutdown().unwrap();

        let mut expected_accumulator = 0.0;
        assert_eq!(
            first.audio_ticks,
            FrameClock::ticks_for_frame(0.5, &mut expected_accumulator).unwrap()
        );
        assert_eq!(
            second.audio_ticks,
            FrameClock::ticks_for_frame(0.5, &mut expected_accumulator).unwrap()
        );
        assert_eq!(runtime.snapshot().audio_accumulator, 0.0);
        let events = events.borrow();
        let game_init = events
            .iter()
            .position(|event| *event == "game_init")
            .unwrap();
        let update = events.iter().position(|event| *event == "update").unwrap();
        let cleanup = events
            .iter()
            .position(|event| *event == "game_cleanup")
            .unwrap();
        assert!(game_init < update);
        assert!(update < cleanup);

        let executor_events = Rc::new(RefCell::new(Vec::new()));
        let mut executor = Executor::new(false);
        let event = executor_events.clone();
        executor.register_constructor(move || {
            event.borrow_mut().push("constructor-a");
            Ok(())
        });
        let event = executor_events.clone();
        executor.register_constructor(move || {
            event.borrow_mut().push("constructor-b");
            Ok(())
        });
        let event = executor_events.clone();
        executor.register_main(move || {
            event.borrow_mut().push("main");
            Ok(())
        });
        let event = executor_events.clone();
        executor.register_destructor(move || {
            event.borrow_mut().push("destructor-a");
            Ok(())
        });
        let event = executor_events.clone();
        executor.register_destructor(move || {
            event.borrow_mut().push("destructor-b");
            Ok(())
        });
        executor.prolog().unwrap();
        executor.epilog().unwrap();
        assert_eq!(
            executor_events.borrow().as_slice(),
            [
                "constructor-a",
                "constructor-b",
                "main",
                "destructor-b",
                "destructor-a"
            ]
        );
    }

    #[test]
    fn reset_is_reachable_and_shutdown_is_clean() {
        let (mut runtime, events) = runtime(true, false);
        let callback_events = events.clone();
        runtime.callbacks_mut().register_reset(move |_| {
            callback_events.borrow_mut().push("reset_callback");
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_prenmi(move |_| {
            callback_events.borrow_mut().push("prenmi_callback");
            Ok(())
        });
        runtime
            .callbacks_mut()
            .register_game_cleanup(move |_| Ok(()));
        runtime.set_soft_reset_enabled(true);
        runtime.boot().unwrap();
        let report = runtime.run_frame(1.0 / 60.0).unwrap();
        assert!(report.reset_triggered);
        assert!(report.stopped);
        assert_eq!(runtime.snapshot().reset_status, super::ResetStatus::PreNmi);
        runtime.shutdown().unwrap();
        assert_eq!(runtime.lifecycle(), Lifecycle::Shutdown);
        assert!(events.borrow().contains(&"audio_cleanup"));
        assert!(events.borrow().contains(&"renderer_shutdown"));
        assert!(events.borrow().contains(&"platform_shutdown"));
    }

    #[test]
    fn reset_transition_fires_prenmi_once_and_stops_the_frame_loop() {
        let (mut runtime, events) = runtime(true, false);
        let callback_events = events.clone();
        runtime.callbacks_mut().register_reset(move |_| {
            callback_events.borrow_mut().push("reset_callback");
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_prenmi(move |_| {
            callback_events.borrow_mut().push("prenmi_callback");
            Ok(())
        });
        runtime.set_soft_reset_enabled(true);
        runtime.boot().unwrap();

        let report = runtime.run_frame(1.0 / 60.0).unwrap();
        assert!(report.reset_triggered);
        assert!(report.stopped);
        assert!(!runtime.snapshot().game.disable_prenmi);
        assert_eq!(
            runtime.snapshot().game.next_transition,
            Some(super::GameTransition::Prenmi)
        );

        // src/graph.c graph_proc enters graph_main only while game_is_doing, so
        // a stopped game must not re-enter the frame path or replay callbacks.
        assert_eq!(
            runtime.run_frame(1.0 / 60.0),
            Err(RuntimeError::GameStopped)
        );
        assert_eq!(runtime.lifecycle(), Lifecycle::Booted);
        let count = |event: &str| events.borrow().iter().filter(|e| **e == event).count();
        assert_eq!(count("reset_callback"), 1);
        assert_eq!(count("prenmi_callback"), 1);
    }

    #[test]
    fn pending_prenmi_transition_defers_while_disable_prenmi_is_set() {
        let (mut runtime, events) = runtime(true, false);
        let first_update = Rc::new(RefCell::new(true));
        let flag = first_update.clone();
        runtime.callbacks_mut().register_update(move |game, _| {
            if *flag.borrow() {
                *flag.borrow_mut() = false;
                game.disable_prenmi = true;
            }
            Ok(())
        });
        let callback_events = events.clone();
        runtime.callbacks_mut().register_prenmi(move |_| {
            callback_events.borrow_mut().push("prenmi_callback");
            Ok(())
        });
        runtime.set_soft_reset_enabled(true);
        runtime.boot().unwrap();

        // The game raises disable_prenmi, so graph_main defers the queued
        // IRQ_RESET_PRENMI transition on the frame the reset was detected.
        let first = runtime.run_frame(1.0 / 60.0).unwrap();
        assert!(first.reset_triggered);
        assert!(!first.stopped);
        assert_eq!(runtime.snapshot().reset_status, super::ResetStatus::PreNmi);

        // graph_main re-arms disable_prenmi each frame, so the pending
        // transition completes next frame without a second reset.
        let second = runtime.run_frame(1.0 / 60.0).unwrap();
        assert!(!second.reset_triggered);
        assert!(second.stopped);
        assert_eq!(
            runtime.snapshot().game.next_transition,
            Some(super::GameTransition::Prenmi)
        );
        let count = |event: &str| events.borrow().iter().filter(|e| **e == event).count();
        assert_eq!(count("prenmi_callback"), 1);
        assert_eq!(count("reset_callback"), 0);
    }

    #[test]
    fn adapter_error_is_bounded_and_runtime_becomes_failed() {
        let (mut runtime, _) = runtime(false, true);
        runtime.boot().unwrap();
        assert_eq!(
            runtime.run_frame(1.0 / 60.0),
            Err(RuntimeError::Host {
                operation: super::Operation::FinishFrame,
                code: HostErrorCode::Failed,
            })
        );
        assert_eq!(runtime.lifecycle(), Lifecycle::Failed);
    }

    #[test]
    fn executor_hooks_and_unresolved_report_are_reachable() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut executor = Executor::new(false);
        let c = calls.clone();
        executor.register_constructor(move || {
            c.borrow_mut().push("constructor");
            Ok(())
        });
        let c = calls.clone();
        executor.register_main(move || {
            c.borrow_mut().push("main");
            Ok(())
        });
        let c = calls.clone();
        executor.register_destructor(move || {
            c.borrow_mut().push("destructor");
            Ok(())
        });
        let c = calls.clone();
        executor.register_unresolved(move |report| {
            assert_eq!(report.frames.len(), 16);
            c.borrow_mut().push("unresolved");
            Ok(())
        });
        executor.prolog().unwrap();
        executor
            .unresolved(
                &[super::StackFrame {
                    address: 1,
                    back_chain: 2,
                    link_register: 3,
                }; 17],
            )
            .unwrap();
        executor.epilog().unwrap();
        assert_eq!(
            &*calls.borrow(),
            &["constructor", "main", "unresolved", "destructor"]
        );
    }

    #[test]
    fn audio_schedule_preserves_fractional_accumulation() {
        let mut schedule = AudioSchedule::new();
        assert_eq!(schedule.ticks(0.5), 0);
        assert_eq!(schedule.ticks(0.75), 1);
        assert!((schedule.accumulator() - 0.25).abs() < f64::EPSILON);
    }
}
