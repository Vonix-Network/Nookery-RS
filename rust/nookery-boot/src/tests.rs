use super::*;

#[derive(Default)]
struct Recorder {
    events: Vec<String>,
    fail_phase: Option<BootPhase>,
}

impl Recorder {
    fn record(&mut self, event: impl Into<String>) -> Result<(), String> {
        self.events.push(event.into());
        Ok(())
    }
}

impl BootCallbacks for Recorder {
    fn boot_phase(&mut self, phase: BootPhase) -> Result<(), String> {
        self.events.push(format!("phase:{phase:?}"));
        if self.fail_phase == Some(phase) {
            Err("injected failure".into())
        } else {
            Ok(())
        }
    }

    fn unlink_module(&mut self) -> Result<(), String> {
        self.record("unlink")
    }

    fn free_string_table(&mut self) -> Result<(), String> {
        self.record("free-string-table")
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.record("shutdown")
    }

    fn entry_pad_init(&mut self) -> Result<(), String> {
        self.record("pad-init")
    }

    fn entry_debug_mode(&mut self) -> Result<(), String> {
        self.record("debug-mode")
    }

    fn entry_reset_register(&mut self) -> Result<(), String> {
        self.record("reset-register")
    }

    fn mainproc(&mut self) -> Result<(), String> {
        self.record("mainproc")
    }

    fn frame(&mut self) -> Result<(), String> {
        self.record("frame")
    }
}

#[test]
fn boot_runs_source_order_once() {
    let mut callbacks = Recorder::default();
    BootPlan::source_order().run(&mut callbacks).unwrap();

    let expected = BootPhase::SOURCE_ORDER
        .iter()
        .map(|phase| format!("phase:{phase:?}"))
        .collect::<Vec<_>>();
    assert_eq!(callbacks.events, expected);
}

#[test]
fn failed_phase_enters_terminal_cleanup() {
    let mut callbacks = Recorder {
        fail_phase: Some(BootPhase::BootResources),
        ..Recorder::default()
    };

    let error = BootPlan::source_order().run(&mut callbacks).unwrap_err();
    assert!(matches!(
        error,
        BootError::Failed {
            cleanup: Ok(()),
            ..
        }
    ));
    assert_eq!(
        callbacks.events,
        vec![
            "phase:OsFoundation",
            "phase:ResetAndNmi",
            "phase:ConsoleMode",
            "phase:MemoryAndJsystem",
            "phase:FaultAndDebug",
            "phase:AudioAndInitialMenu",
            "phase:BootResources",
            "unlink",
            "free-string-table",
            "shutdown",
        ]
    );
}

#[test]
fn entry_loop_orders_frame_and_shutdown() {
    let mut loop_state = EntryLoop::new(Recorder::default());
    loop_state.boot().unwrap();
    loop_state.dispatch_frame().unwrap();
    loop_state.shutdown().unwrap();

    assert_eq!(
        loop_state.events(),
        &[
            EntryEvent::Boot,
            EntryEvent::Entry,
            EntryEvent::Reset,
            EntryEvent::Mainproc,
            EntryEvent::Frame,
            EntryEvent::Shutdown,
        ]
    );
    assert_eq!(loop_state.state(), &EntryState::Shutdown);
    assert_eq!(
        loop_state.callbacks().events,
        vec![
            "phase:OsFoundation",
            "phase:ResetAndNmi",
            "phase:ConsoleMode",
            "phase:MemoryAndJsystem",
            "phase:FaultAndDebug",
            "phase:AudioAndInitialMenu",
            "phase:BootResources",
            "phase:GameModule",
            "pad-init",
            "debug-mode",
            "reset-register",
            "mainproc",
            "frame",
            "unlink",
            "free-string-table",
            "shutdown",
        ]
    );
}
