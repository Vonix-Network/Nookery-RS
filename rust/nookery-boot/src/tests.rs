use super::*;

static ABI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn abi_test_guard() -> std::sync::MutexGuard<'static, ()> {
    ABI_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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

#[test]
fn abi_source_locations_preserve_entry_contract() {
    let boot_c = include_str!("../../../src/static/boot.c");
    let main_c = include_str!("../../../src/main.c");
    let main_h = include_str!("../../../include/main.h");
    let executor_h = include_str!("../../../include/executor.h");
    let executor_c = include_str!("../../../src/executor.c");
    let abi_h = include_str!("../include/nookery_boot.h");

    for source in [boot_c, main_c, main_h, executor_h, executor_c, abi_h] {
        assert!(!source.trim().is_empty());
    }
    assert!(boot_c.contains("while (HotStartEntry != nullptr)"));
    assert!(main_c.contains("mainproc"));
    assert!(main_c.find("padmgr_Init").unwrap() < main_c.find("mainproc(NULL)").unwrap());
    assert!(main_h.contains("extern void mainproc(void* val)"));
    assert!(executor_h.contains("void _prolog();"));
    assert!(executor_h.contains("void _epilog();"));
    assert!(executor_h.contains("void _unresolved();"));
    assert!(executor_c.contains("if (!construct_skip)"));
    assert!(executor_c.contains("for (constructor = _ctors; *constructor; constructor++)"));
    assert!(executor_c.contains("for (destructor = _dtors; *destructor; destructor++)"));
    for symbol in [
        "nookery_boot_register_callbacks",
        "nookery_boot_prolog",
        "nookery_boot_entry",
        "nookery_boot_epilog",
        "nookery_boot_unresolved",
    ] {
        assert!(
            abi_h.contains(symbol),
            "ABI header does not declare {symbol}"
        );
    }
    assert!(abi_h.contains("NOOKERY_BOOT_MAX_CALLBACKS 16u"));
}

#[test]
fn abi_c_layout_is_explicitly_bounded() {
    use std::mem::{offset_of, size_of};

    assert_eq!(size_of::<NookeryBootStackFrame>(), 12);
    assert_eq!(offset_of!(NookeryBootCallbackTable, context), 0);
    assert_eq!(
        offset_of!(NookeryBootCallbackTable, construct_skip),
        size_of::<*mut std::ffi::c_void>()
    );
    assert_eq!(
        offset_of!(NookeryBootCallbackTable, constructors),
        size_of::<*mut std::ffi::c_void>() + 8
    );
}

#[test]
fn abi_callback_table_runs_main_and_reverse_destructors() {
    let _guard = abi_test_guard();
    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let table = abi_table(context);
    assert_eq!(register(&table), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_prolog(), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_epilog(), NOOKERY_BOOT_OK);
    assert_eq!(
        trace.events,
        vec!["ctor-0", "ctor-1", "main", "drop-1", "drop-0"]
    );
    assert_eq!(nookery_boot_unregister_callbacks(), NOOKERY_BOOT_OK);
}

#[test]
fn abi_construct_skip_skips_only_constructors() {
    let _guard = abi_test_guard();
    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let mut table = abi_table(context);
    table.construct_skip = 1;
    assert_eq!(register(&table), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_prolog(), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_epilog(), NOOKERY_BOOT_OK);
    assert_eq!(trace.events, vec!["main", "drop-1", "drop-0"]);
    nookery_boot_unregister_callbacks();
}

#[test]
fn abi_negative_inputs_have_safe_codes() {
    let _guard = abi_test_guard();
    let null_context = NookeryBootCallbackTable::empty(std::ptr::null_mut());
    assert_eq!(
        unsafe { nookery_boot_register_callbacks(std::ptr::null()) },
        NOOKERY_BOOT_INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { nookery_boot_register_callbacks(&null_context) },
        NOOKERY_BOOT_INVALID_ARGUMENT
    );
    assert_eq!(nookery_boot_prolog(), NOOKERY_BOOT_INVALID_ARGUMENT);
    assert_eq!(nookery_boot_epilog(), NOOKERY_BOOT_INVALID_ARGUMENT);

    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let mut table = abi_table(context);
    table.main = None;
    assert_eq!(
        unsafe { nookery_boot_register_callbacks(&table) },
        NOOKERY_BOOT_INVALID_TABLE
    );
    assert_eq!(nookery_boot_unregister_callbacks(), NOOKERY_BOOT_OK);

    let mut table = abi_table(context);
    table.constructor_count = (NOOKERY_BOOT_MAX_CALLBACKS + 1) as u32;
    assert_eq!(register(&table), NOOKERY_BOOT_INVALID_TABLE);
    nookery_boot_unregister_callbacks();
}

#[test]
fn abi_repeated_calls_require_explicit_unregister() {
    let _guard = abi_test_guard();
    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let table = abi_table(context);
    assert_eq!(register(&table), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_prolog(), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_prolog(), NOOKERY_BOOT_INVALID_STATE);
    assert_eq!(nookery_boot_epilog(), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_epilog(), NOOKERY_BOOT_INVALID_STATE);
    assert_eq!(
        trace.events,
        vec!["ctor-0", "ctor-1", "main", "drop-1", "drop-0"]
    );
    assert_eq!(nookery_boot_unregister_callbacks(), NOOKERY_BOOT_OK);
}

#[test]
fn abi_entry_orders_main_c_callbacks_after_prolog() {
    let _guard = abi_test_guard();
    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let mut table = abi_table(context);
    table.entry_pad_init = Some(entry_pad_init);
    table.entry_debug_mode = Some(entry_debug_mode);
    table.entry_reset_register = Some(entry_reset_register);
    table.entry_mainproc = Some(entry_mainproc);
    assert_eq!(register(&table), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_prolog(), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_entry(), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_epilog(), NOOKERY_BOOT_OK);
    assert_eq!(
        trace.events,
        vec![
            "ctor-0",
            "ctor-1",
            "main",
            "entry-pad-init",
            "entry-debug-mode",
            "entry-reset-register",
            "entry-mainproc",
            "drop-1",
            "drop-0",
        ]
    );
    nookery_boot_unregister_callbacks();
}

#[test]
fn abi_callback_failure_is_reported_and_stops_the_trace() {
    let _guard = abi_test_guard();
    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let mut table = abi_table(context);
    table.constructors[0] = Some(failing_callback);
    table.report = Some(report_callback);
    assert_eq!(register(&table), NOOKERY_BOOT_OK);
    assert_eq!(
        nookery_boot_prolog(),
        NOOKERY_BOOT_CALLBACK_FAILED,
        "callback failure must cross the ABI"
    );
    assert_eq!(trace.events, vec!["fail", "report"]);
    nookery_boot_unregister_callbacks();
}

#[test]
fn abi_callback_panic_becomes_a_status_code() {
    let _guard = abi_test_guard();
    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let mut table = abi_table(context);
    table.main = Some(panic_callback);
    assert_eq!(register(&table), NOOKERY_BOOT_OK);
    assert_eq!(nookery_boot_prolog(), NOOKERY_BOOT_PANIC);
    assert_eq!(nookery_boot_epilog(), NOOKERY_BOOT_INVALID_STATE);
    nookery_boot_unregister_callbacks();
}

#[test]
fn abi_unresolved_report_is_bounded_and_callback_owned() {
    let _guard = abi_test_guard();
    let mut trace = AbiTrace::default();
    let context = (&mut trace as *mut AbiTrace).cast();
    let mut table = abi_table(context);
    table.unresolved = Some(unresolved_callback);
    assert_eq!(register(&table), NOOKERY_BOOT_OK);
    let frames = [NookeryBootStackFrame {
        address: 0x1000,
        back_chain: 0x2000,
        link_register: 0x3000,
    }];
    assert_eq!(
        unsafe { nookery_boot_unresolved(frames.as_ptr(), frames.len() as u32) },
        NOOKERY_BOOT_OK
    );
    assert_eq!(trace.events, vec!["unresolved-1"]);
    assert_eq!(
        unsafe {
            nookery_boot_unresolved(
                frames.as_ptr(),
                (NOOKERY_BOOT_MAX_UNRESOLVED_FRAMES + 1) as u32,
            )
        },
        NOOKERY_BOOT_INVALID_ARGUMENT
    );
    nookery_boot_unregister_callbacks();
}

#[derive(Default)]
struct AbiTrace {
    events: Vec<&'static str>,
}

unsafe fn abi_trace(context: *mut std::ffi::c_void, event: &'static str) -> i32 {
    let trace = &mut *(context.cast::<AbiTrace>());
    trace.events.push(event);
    NOOKERY_BOOT_OK
}

unsafe extern "C-unwind" fn ctor_0(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "ctor-0")
}

unsafe extern "C-unwind" fn ctor_1(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "ctor-1")
}

unsafe extern "C-unwind" fn main_callback(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "main")
}

unsafe extern "C-unwind" fn drop_0(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "drop-0")
}

unsafe extern "C-unwind" fn drop_1(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "drop-1")
}

unsafe extern "C-unwind" fn entry_pad_init(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "entry-pad-init")
}

unsafe extern "C-unwind" fn entry_debug_mode(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "entry-debug-mode")
}

unsafe extern "C-unwind" fn entry_reset_register(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "entry-reset-register")
}

unsafe extern "C-unwind" fn entry_mainproc(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "entry-mainproc")
}

unsafe extern "C-unwind" fn failing_callback(context: *mut std::ffi::c_void) -> i32 {
    abi_trace(context, "fail");
    NOOKERY_BOOT_CALLBACK_FAILED
}

unsafe extern "C-unwind" fn panic_callback(_context: *mut std::ffi::c_void) -> i32 {
    panic!("test callback panic")
}

unsafe extern "C-unwind" fn report_callback(
    context: *mut std::ffi::c_void,
    _code: i32,
    _message: *const std::ffi::c_char,
) -> i32 {
    abi_trace(context, "report")
}

unsafe extern "C-unwind" fn unresolved_callback(
    context: *mut std::ffi::c_void,
    _frames: *const NookeryBootStackFrame,
    frame_count: u32,
) -> i32 {
    abi_trace(
        context,
        if frame_count == 1 {
            "unresolved-1"
        } else {
            "unresolved-other"
        },
    )
}

fn register(table: &NookeryBootCallbackTable) -> i32 {
    // The production registration contract requires the caller to provide a
    // live table for the duration of registration and later calls.
    unsafe { nookery_boot_register_callbacks(table) }
}

fn abi_table(context: *mut std::ffi::c_void) -> NookeryBootCallbackTable {
    let mut table = NookeryBootCallbackTable::empty(context);
    table.constructor_count = 2;
    table.constructors[0] = Some(ctor_0);
    table.constructors[1] = Some(ctor_1);
    table.main = Some(main_callback);
    table.destructor_count = 2;
    table.destructors[0] = Some(drop_0);
    table.destructors[1] = Some(drop_1);
    table
}
