//! Boundary tests for the C ABI, driven through the same calls a C caller makes.
//!
//! These tests speak the *raw* ABI (raw pointers, `struct_size` fields, caller
//! buffers) rather than Rust wrappers: the failure mode worth catching is a
//! wrapper that is safe while the ABI underneath is not.
//!
//! Every asserted status comes from [`nookery_data_ffi`]'s public constants, and
//! `tests/abi_header.rs` proves those constants equal the numbers in
//! `include/nookery_data.h`. No magic numbers here.

use std::ffi::{CStr, CString, c_char};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use nookery_data_ffi::*;

/* --- harness ------------------------------------------------------------ */

/// Self-cleaning temporary directory; nothing test-related escapes it.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = format!(
            "nookery-data-ffi-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("create temp root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn utf8(&self) -> CString {
        CString::new(self.0.to_str().expect("temp path is UTF-8")).expect("no interior NUL")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Caller-side output storage, filled to a sentinel so unwritten bytes are
/// detectable rather than looking like data.
struct Out {
    storage: Vec<u8>,
}

const SENTINEL: u8 = 0x7f;

impl Out {
    fn new(capacity: usize) -> Self {
        Self {
            storage: vec![SENTINEL; capacity],
        }
    }

    fn buffer(&mut self) -> NookeryDataBuffer {
        NookeryDataBuffer {
            struct_size: NookeryDataBuffer::SIZE,
            ptr: self.storage.as_mut_ptr().cast::<c_char>(),
            capacity: self.storage.len(),
            length: 0,
        }
    }

    fn untouched(&self) -> bool {
        self.storage.iter().all(|byte| *byte == SENTINEL)
    }

    /// The NUL-terminated string at the start of storage.
    fn text(&self) -> String {
        let end = self
            .storage
            .iter()
            .position(|byte| *byte == 0)
            .expect("buffer must be NUL-terminated after a successful write");
        String::from_utf8(self.storage[..end].to_vec()).expect("UTF-8 output")
    }
}

/// Build a handle over `root` (and optionally `base`), failing the test loudly.
fn make_handle(root: &Path, base: Option<&Path>) -> *mut DataHandle {
    let root = CString::new(root.to_str().expect("root is UTF-8")).expect("no interior NUL");
    let base = base
        .map(|path| CString::new(path.to_str().expect("base is UTF-8")).expect("no interior NUL"));
    let options = NookeryDataCreateOptions {
        struct_size: NookeryDataCreateOptions::SIZE,
        abi_version: nookery_data_abi_version(),
        root_utf8: root.as_ptr(),
        base_root_utf8: base.as_ref().map_or(std::ptr::null(), |text| text.as_ptr()),
    };
    let mut handle: *mut DataHandle = std::ptr::null_mut();
    let status = unsafe { nookery_data_handle_create(&options, &mut handle) };
    assert_eq!(status, STATUS_OK, "{}", status_name(status));
    assert!(!handle.is_null());
    handle
}

/// Read a borrowed `'static` status string.
fn c_string(ptr: *const c_char) -> String {
    assert!(!ptr.is_null());
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("status text is UTF-8")
        .to_owned()
}

/// Run one path-returning call and return `(status, text)`.
fn call_path<F>(mut out: Out, call: F) -> (RawStatus, usize, String, bool)
where
    F: FnOnce(&mut NookeryDataBuffer) -> RawStatus,
{
    let mut buffer = out.buffer();
    let status = call(&mut buffer);
    let length = buffer.length;
    let untouched = out.untouched();
    (status, length, out.text(), untouched)
}

/* --- identity and status text ------------------------------------------- */

#[test]
fn reports_its_own_layout_version_and_known_status_count() {
    assert_eq!(nookery_data_abi_version(), ABI_VERSION);
    assert_eq!(nookery_data_status_count() as usize, STATUS_TABLE.len());
}

#[test]
fn every_status_has_a_stable_name_and_the_upstream_message() {
    for (name, value) in STATUS_TABLE {
        let name = name.to_str().expect("status names are ASCII");
        assert_eq!(c_string(nookery_data_status_name(*value)), name);
        let message = c_string(nookery_data_status_message(*value));
        // Statuses the core models carry upstream wording; ABI-only statuses
        // must stay empty rather than gain invented user-facing text.
        if *value != STATUS_OK && *value <= STATUS_OPEN_FOLDER_FAILED {
            assert!(!message.is_empty(), "{name} lost its upstream message");
        } else {
            assert_eq!(message, "", "{name} must not invent upstream wording");
        }
    }
    assert_eq!(
        c_string(nookery_data_status_name(STATUS_MAX + 1)),
        "<unknown>"
    );
    assert_eq!(c_string(nookery_data_status_message(STATUS_MAX + 1)), "");
}

#[test]
fn status_ok_is_the_only_success_value_and_message_is_empty() {
    assert_eq!(STATUS_OK, 0);
    assert_eq!(c_string(nookery_data_status_message(STATUS_OK)), "");
    assert_eq!(c_string(nookery_data_status_name(STATUS_OK)), "OK");
}

/* --- create: null, layout, version, root validation --------------------- */

#[test]
fn create_rejects_null_out_pointer_null_options_and_bad_layout() {
    let temp = TempDir::new("create-null");
    let root = temp.utf8();

    let status = unsafe { nookery_data_handle_create(std::ptr::null(), std::ptr::null_mut()) };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);

    let mut handle: *mut DataHandle = std::ptr::null_mut();
    let status = unsafe { nookery_data_handle_create(std::ptr::null(), &mut handle) };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);
    assert!(handle.is_null(), "out-parameter is cleared on failure");

    let good = NookeryDataCreateOptions {
        struct_size: NookeryDataCreateOptions::SIZE,
        abi_version: nookery_data_abi_version(),
        root_utf8: root.as_ptr(),
        base_root_utf8: std::ptr::null(),
    };

    // Wrong struct_size: rejected before the string fields are even read.
    let status = unsafe {
        nookery_data_handle_create(
            &NookeryDataCreateOptions {
                struct_size: 0,
                ..copy_options(&good)
            },
            &mut handle,
        )
    };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);
    assert!(handle.is_null());

    // Wrong ABI version: rejected rather than reinterpreted.
    let status = unsafe {
        nookery_data_handle_create(
            &NookeryDataCreateOptions {
                abi_version: 0,
                ..copy_options(&good)
            },
            &mut handle,
        )
    };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);
    assert!(handle.is_null());

    // Null root.
    let status = unsafe {
        nookery_data_handle_create(
            &NookeryDataCreateOptions {
                root_utf8: std::ptr::null(),
                ..copy_options(&good)
            },
            &mut handle,
        )
    };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);
    assert!(handle.is_null());

    // Relative and escaping roots.
    for bad in ["relative/path", "", "/data/../etc"] {
        let bad = CString::new(bad).expect("no NUL");
        let status = unsafe {
            nookery_data_handle_create(
                &NookeryDataCreateOptions {
                    root_utf8: bad.as_ptr(),
                    ..copy_options(&good)
                },
                &mut handle,
            )
        };
        assert_eq!(
            status, STATUS_INVALID_ARGUMENT,
            "root {bad:?} must be refused"
        );
        assert!(handle.is_null());
    }

    // Non-UTF-8 root bytes.
    let mut bytes = temp.path().to_str().expect("utf8").as_bytes().to_vec();
    bytes.push(0xff);
    let bad = CString::new(bytes).expect("no interior NUL");
    let status = unsafe {
        nookery_data_handle_create(
            &NookeryDataCreateOptions {
                root_utf8: bad.as_ptr(),
                ..copy_options(&good)
            },
            &mut handle,
        )
    };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);
    assert!(handle.is_null());

    // Oversized paths are rejected before any filesystem work.
    let oversized = CString::new(format!("/{}", "x".repeat(4096))).expect("no NUL");
    let status = unsafe {
        nookery_data_handle_create(
            &NookeryDataCreateOptions {
                root_utf8: oversized.as_ptr(),
                ..copy_options(&good)
            },
            &mut handle,
        )
    };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);
    assert!(handle.is_null());
}

/// `NookeryDataCreateOptions` is not `Copy`; rebuild it field by field.
fn copy_options(source: &NookeryDataCreateOptions) -> NookeryDataCreateOptions {
    NookeryDataCreateOptions {
        struct_size: source.struct_size,
        abi_version: source.abi_version,
        root_utf8: source.root_utf8,
        base_root_utf8: source.base_root_utf8,
    }
}

#[test]
fn create_accepts_a_valid_root_and_does_no_filesystem_work() {
    let temp = TempDir::new("create-ok");
    let target = temp.path().join("not-created-yet");
    let handle = make_handle(&target, None);
    assert_eq!(unsafe { nookery_data_last_status(handle) }, STATUS_OK);
    assert!(
        !target.exists(),
        "handle creation must not touch the filesystem"
    );
    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn destroy_accepts_null() {
    unsafe { nookery_data_handle_destroy(std::ptr::null_mut()) };
}

#[test]
fn cache_root_can_be_injected_without_changing_the_active_root() {
    let temp = TempDir::new("cache-injection");
    let cache = TempDir::new("cache-injection-external");
    let handle = make_handle(temp.path(), None);
    let cache_text = CString::new(cache.path().to_str().expect("cache is UTF-8")).expect("no NUL");

    assert_eq!(
        unsafe { nookery_data_set_cache_root(handle, cache_text.as_ptr()) },
        STATUS_OK
    );
    assert_eq!(
        unsafe { nookery_data_initialize(handle, std::ptr::null_mut()) },
        STATUS_OK
    );
    assert!(cache.path().is_dir());
    let (status, _, reported, _) = call_path(Out::new(512), |buffer| unsafe {
        nookery_data_directory(handle, 4, buffer)
    });
    assert_eq!(status, STATUS_OK);
    assert_eq!(reported, cache.path().to_str().expect("cache is UTF-8"));
    assert!(!temp.path().join("cache").exists());

    unsafe { nookery_data_handle_destroy(handle) };
}

/* --- initialize: success, idempotence, blockers ------------------------- */

#[test]
fn initialize_creates_every_required_directory_in_upstream_order() {
    let temp = TempDir::new("init-ok");
    let handle = make_handle(temp.path(), None);

    let mut report = NookeryDataDirReport {
        struct_size: NookeryDataDirReport::SIZE,
        failure_count: u32::MAX,
    };
    let status = unsafe { nookery_data_initialize(handle, &mut report) };
    assert_eq!(status, STATUS_OK, "{}", status_name(status));
    assert_eq!(report.failure_count, 0);

    for kind in [0u32, 1, 2, 3, 4] {
        let (status, _, path, _) = call_path(Out::new(512), |buffer| unsafe {
            nookery_data_directory(handle, kind, buffer)
        });
        assert_eq!(status, STATUS_OK);
        assert!(
            Path::new(&path).is_dir(),
            "kind {kind} reported {path} which is not a directory"
        );
    }

    assert!(temp.path().join("save/card_a").is_dir());
    assert!(temp.path().join("save/card_b").is_dir());
    assert!(temp.path().join("texture_pack").is_dir());
    assert!(temp.path().join("nes_roms").is_dir());
    assert!(temp.path().join("cache").is_dir());
    // Only the four top-level entries upstream creates; nothing extra.
    let mut top: Vec<String> = fs::read_dir(temp.path())
        .expect("read root")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    top.sort();
    assert_eq!(top, vec!["cache", "nes_roms", "save", "texture_pack"]);

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn repeated_initialization_succeeds_and_preserves_existing_content() {
    let temp = TempDir::new("init-twice");
    let handle = make_handle(temp.path(), None);

    assert_eq!(
        unsafe { nookery_data_initialize(handle, std::ptr::null_mut()) },
        STATUS_OK
    );
    let marker = temp.path().join("save/card_a/town.gci");
    fs::write(&marker, b"save").expect("write marker");

    let mut report = NookeryDataDirReport {
        struct_size: NookeryDataDirReport::SIZE,
        failure_count: u32::MAX,
    };
    let status = unsafe { nookery_data_initialize(handle, &mut report) };
    assert_eq!(
        status, STATUS_OK,
        "second initialization must be idempotent"
    );
    assert_eq!(report.failure_count, 0);
    assert_eq!(fs::read(&marker).expect("marker intact"), b"save");

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn initialize_reports_a_file_blocker_without_losing_other_directories() {
    let temp = TempDir::new("init-blocked");
    let handle = make_handle(temp.path(), None);
    fs::create_dir_all(temp.path().join("save")).expect("create save");
    fs::write(temp.path().join("save/card_a"), b"not a directory").expect("write blocker");

    let mut report = NookeryDataDirReport {
        struct_size: NookeryDataDirReport::SIZE,
        failure_count: u32::MAX,
    };
    let status = unsafe { nookery_data_initialize(handle, &mut report) };

    assert_eq!(status, STATUS_NOT_DIRECTORY, "{}", status_name(status));
    assert_eq!(report.failure_count, 1);
    assert_eq!(
        fs::read(temp.path().join("save/card_a")).expect("blocker intact"),
        b"not a directory"
    );
    assert!(
        temp.path().join("save/card_b").is_dir(),
        "later dirs still made"
    );
    assert!(temp.path().join("cache").is_dir());
    assert_eq!(
        unsafe { nookery_data_last_status(handle) },
        STATUS_NOT_DIRECTORY
    );

    // The message names the failing path, as upstream's helper does.
    let (status, _, message, _) = call_path(Out::new(1024), |buffer| unsafe {
        nookery_data_last_message(handle, buffer)
    });
    assert_eq!(status, STATUS_OK);
    assert!(
        message.starts_with("The selected path is not a folder. ("),
        "unexpected message {message:?}"
    );
    assert!(
        message.contains("card_a"),
        "message must name the path: {message:?}"
    );

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn initialize_rejects_null_handle_a_bad_report_layout_and_nulls() {
    assert_eq!(
        unsafe { nookery_data_initialize(std::ptr::null_mut(), std::ptr::null_mut()) },
        STATUS_INVALID_ARGUMENT
    );

    let temp = TempDir::new("init-bad-report");
    let handle = make_handle(temp.path(), None);
    let mut report = NookeryDataDirReport {
        struct_size: NookeryDataDirReport::SIZE + 8,
        failure_count: u32::MAX,
    };
    let status = unsafe { nookery_data_initialize(handle, &mut report) };
    assert_eq!(status, STATUS_INVALID_ARGUMENT);
    assert_eq!(
        report.failure_count,
        u32::MAX,
        "report untouched on rejection"
    );
    assert!(
        !temp.path().join("save").exists(),
        "no work before validation"
    );

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn a_null_report_is_optional() {
    let temp = TempDir::new("init-null-report");
    let handle = make_handle(temp.path(), None);
    assert_eq!(
        unsafe { nookery_data_initialize(handle, std::ptr::null_mut()) },
        STATUS_OK
    );
    unsafe { nookery_data_handle_destroy(handle) };
}

/* --- buffers: undersized, exact, null, zero capacity -------------------- */

#[test]
fn an_undersized_buffer_writes_nothing_and_reports_the_required_size() {
    let temp = TempDir::new("buffer-small");
    let handle = make_handle(temp.path(), None);

    // Required size first, from a zero-capacity probe.
    let mut buffer = NookeryDataBuffer {
        struct_size: NookeryDataBuffer::SIZE,
        ptr: std::ptr::null_mut(),
        capacity: 0,
        length: 0,
    };
    let status = unsafe { nookery_data_root(handle, &mut buffer) };
    assert_eq!(status, STATUS_BUFFER_TOO_SMALL);
    let required = buffer.length;
    assert!(required > 1, "root text plus NUL");

    // One byte short: nothing written, exact requirement reported.
    let mut short = Out::new(required - 1);
    let mut buffer = short.buffer();
    let status = unsafe { nookery_data_root(handle, &mut buffer) };
    assert_eq!(status, STATUS_BUFFER_TOO_SMALL);
    assert_eq!(buffer.length, required);
    assert!(
        short.untouched(),
        "an undersized buffer must not be partially filled"
    );

    // Exactly enough: success, NUL-terminated, length excludes the terminator.
    let mut exact = Out::new(required);
    let mut buffer = exact.buffer();
    let status = unsafe { nookery_data_root(handle, &mut buffer) };
    assert_eq!(status, STATUS_OK);
    assert_eq!(buffer.length, required - 1);
    assert_eq!(exact.text(), temp.path().to_str().expect("utf8"));
    assert_eq!(exact.storage[required - 1], 0, "terminator written");

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn buffers_reject_a_null_pointer_with_capacity_and_a_wrong_struct_size() {
    let temp = TempDir::new("buffer-bad");
    let handle = make_handle(temp.path(), None);

    let mut null_ptr = NookeryDataBuffer {
        struct_size: NookeryDataBuffer::SIZE,
        ptr: std::ptr::null_mut(),
        capacity: 16,
        length: 99,
    };
    assert_eq!(
        unsafe { nookery_data_root(handle, &mut null_ptr) },
        STATUS_INVALID_ARGUMENT
    );
    assert_eq!(null_ptr.length, 0);

    let mut wrong_size = Out::new(64);
    let mut buffer = wrong_size.buffer();
    buffer.struct_size = NookeryDataBuffer::SIZE - 4;
    assert_eq!(
        unsafe { nookery_data_root(handle, &mut buffer) },
        STATUS_INVALID_ARGUMENT
    );
    assert!(wrong_size.untouched());

    // A null out-struct itself is an argument error, not a crash.
    assert_eq!(
        unsafe { nookery_data_root(handle, std::ptr::null_mut()) },
        STATUS_INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { nookery_data_user_path(handle, c"save".as_ptr(), std::ptr::null_mut()) },
        STATUS_INVALID_ARGUMENT
    );

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn directory_rejects_an_out_of_range_selector_and_a_null_handle() {
    let temp = TempDir::new("directory-bad");
    let handle = make_handle(temp.path(), None);

    let mut out = Out::new(256);
    let mut buffer = out.buffer();
    let status = unsafe { nookery_data_directory(handle, 5, &mut buffer) };
    assert_eq!(status, STATUS_INDEX_OUT_OF_RANGE);
    assert_eq!(buffer.length, 0);
    assert!(out.untouched());

    let mut out = Out::new(256);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_directory(std::ptr::null_mut(), 0, &mut buffer) },
        STATUS_INVALID_ARGUMENT
    );

    unsafe { nookery_data_handle_destroy(handle) };
}

/* --- path resolution: success and escape attempts ---------------------- */

#[test]
fn user_path_joins_below_the_root() {
    let temp = TempDir::new("user-path");
    let handle = make_handle(temp.path(), None);

    let (status, _, path, untouched) = call_path(Out::new(512), |buffer| unsafe {
        nookery_data_user_path(handle, c"save/card_a/town.gci".as_ptr(), buffer)
    });
    assert_eq!(status, STATUS_OK);
    assert!(!untouched);
    assert_eq!(
        path,
        temp.path()
            .join("save/card_a/town.gci")
            .to_str()
            .expect("utf8")
    );

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn user_path_rejects_escapes_absolute_paths_and_nulls() {
    let temp = TempDir::new("user-escape");
    let handle = make_handle(temp.path(), None);

    for bad in ["../outside", "save/../../outside", "/etc/passwd"] {
        let bad = CString::new(bad).expect("no NUL");
        let mut out = Out::new(256);
        let mut buffer = out.buffer();
        let status = unsafe { nookery_data_user_path(handle, bad.as_ptr(), &mut buffer) };
        assert_eq!(
            status,
            STATUS_PATH_REJECTED,
            "{bad:?} must be refused with {}, got {}",
            status_name(STATUS_PATH_REJECTED),
            status_name(status)
        );
        assert_eq!(buffer.length, 0);
        assert!(out.untouched(), "{bad:?} must not produce an escaping path");
        assert_eq!(
            unsafe { nookery_data_last_status(handle) },
            STATUS_PATH_REJECTED
        );
    }

    // Empty relative path.
    let mut out = Out::new(256);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_user_path(handle, c"".as_ptr(), &mut buffer) },
        STATUS_PATH_REJECTED
    );

    // Null and non-UTF-8 inputs are argument errors, distinct from a rejection.
    let mut out = Out::new(256);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_user_path(handle, std::ptr::null(), &mut buffer) },
        STATUS_INVALID_ARGUMENT
    );

    let bad = CString::new(vec![b's', b'a', 0xff]).expect("no interior NUL");
    let mut out = Out::new(256);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_user_path(handle, bad.as_ptr(), &mut buffer) },
        STATUS_INVALID_ARGUMENT
    );

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn base_path_answers_unavailable_without_a_base_root() {
    let temp = TempDir::new("base-none");
    let handle = make_handle(temp.path(), None);

    let mut out = Out::new(256);
    let mut buffer = out.buffer();
    let status = unsafe { nookery_data_base_path(handle, c"texture_pack".as_ptr(), &mut buffer) };
    assert_eq!(
        status, STATUS_BASE_PATH_UNAVAILABLE,
        "mirrors pc_platform_base_path returning 0 on platforms without a base dir"
    );
    assert_eq!(buffer.length, 0);
    assert!(out.untouched());

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn base_path_joins_below_a_supplied_base_root() {
    let temp = TempDir::new("base-some");
    let base = TempDir::new("base-root");
    let handle = make_handle(temp.path(), Some(base.path()));

    let (status, _, path, _) = call_path(Out::new(512), |buffer| unsafe {
        nookery_data_base_path(handle, c"texture_pack".as_ptr(), buffer)
    });
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        path,
        base.path().join("texture_pack").to_str().expect("utf8")
    );

    // Escapes are refused for the base root too.
    let mut out = Out::new(512);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_base_path(handle, c"../etc".as_ptr(), &mut buffer) },
        STATUS_PATH_REJECTED
    );
    assert!(out.untouched());

    unsafe { nookery_data_handle_destroy(handle) };
}

/* --- diagnostics -------------------------------------------------------- */

#[test]
fn last_status_and_message_track_the_most_recent_operation() {
    let temp = TempDir::new("diagnostics");
    let handle = make_handle(temp.path(), None);
    assert_eq!(unsafe { nookery_data_last_status(handle) }, STATUS_OK);

    let _ = unsafe { nookery_data_initialize(handle, std::ptr::null_mut()) };
    let (status, _, message, _) = call_path(Out::new(512), |buffer| unsafe {
        nookery_data_last_message(handle, buffer)
    });
    assert_eq!(status, STATUS_OK);
    assert_eq!(message, "", "success renders as the empty upstream message");

    // A rejected path records its own status; the message follows the code.
    let mut out = Out::new(256);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_user_path(handle, c"../x".as_ptr(), &mut buffer) },
        STATUS_PATH_REJECTED
    );
    assert_eq!(
        unsafe { nookery_data_last_status(handle) },
        STATUS_PATH_REJECTED
    );
    let (status, _, message, _) = call_path(Out::new(256), |buffer| unsafe {
        nookery_data_last_message(handle, buffer)
    });
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        message, "",
        "ABI-level statuses have no upstream wording; inventing one would be a lie"
    );

    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn null_handle_diagnostics_are_rejected_not_dereferenced() {
    assert_eq!(
        unsafe { nookery_data_last_status(std::ptr::null()) },
        STATUS_INVALID_ARGUMENT
    );
    let mut out = Out::new(64);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_last_message(std::ptr::null_mut(), &mut buffer) },
        STATUS_INVALID_ARGUMENT
    );
    assert!(out.untouched());
}

/* --- migration planning ------------------------------------------------- */

#[test]
fn plan_lists_legacy_entries_absent_from_the_active_root() {
    let active = TempDir::new("plan-active");
    let legacy = TempDir::new("plan-legacy");
    let handle = make_handle(active.path(), None);

    fs::create_dir_all(legacy.path().join("save")).expect("legacy save");
    fs::create_dir_all(legacy.path().join("nes_roms")).expect("legacy nes");
    fs::write(legacy.path().join("config.json"), b"{}").expect("config");
    fs::write(legacy.path().join("settings.ini"), b"x").expect("settings");
    fs::write(legacy.path().join("town.gci"), b"gci").expect("gci");
    fs::write(legacy.path().join("pad.controller"), b"pad").expect("controller");
    fs::write(legacy.path().join("notes.txt"), b"ignored").expect("ignored");

    fs::create_dir_all(active.path().join("save")).expect("active save");
    fs::write(active.path().join("settings.ini"), b"keep").expect("active settings");

    let legacy_root = legacy.utf8();
    let mut plan: *mut DataPlan = std::ptr::null_mut();
    let status = unsafe { nookery_data_plan_migration(handle, legacy_root.as_ptr(), &mut plan) };
    assert_eq!(status, STATUS_OK, "{}", status_name(status));
    assert!(!plan.is_null());

    let (mut directories, mut files) = (usize::MAX, usize::MAX);
    assert_eq!(
        unsafe { nookery_data_plan_counts(plan, &mut directories, &mut files) },
        STATUS_OK
    );
    assert_eq!(directories, 1, "`save` exists in the active root already");
    assert_eq!(files, 3, "config.json, pad.controller, town.gci");

    let mut out = Out::new(512);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_plan_directory(plan, 0, &mut buffer) },
        STATUS_OK
    );
    assert_eq!(
        out.text(),
        legacy.path().join("nes_roms").to_str().expect("utf8")
    );

    // Named files come first, then extension matches sorted by name.
    let expected: Vec<PathBuf> = [
        legacy.path().join("config.json"),
        legacy.path().join("pad.controller"),
        legacy.path().join("town.gci"),
    ]
    .to_vec();
    for (index, want) in expected.iter().enumerate() {
        let mut out = Out::new(512);
        let mut buffer = out.buffer();
        assert_eq!(
            unsafe { nookery_data_plan_file(plan, index, &mut buffer) },
            STATUS_OK
        );
        assert_eq!(out.text(), want.to_str().expect("utf8"));
    }

    // Planning never modifies either tree.
    assert!(!active.path().join("nes_roms").exists());
    assert!(!active.path().join("config.json").exists());
    assert_eq!(
        fs::read(active.path().join("settings.ini")).expect("keep"),
        b"keep"
    );

    unsafe { nookery_data_plan_destroy(plan) };
    unsafe { nookery_data_handle_destroy(handle) };
}

#[test]
fn plan_rejects_bad_inputs_and_out_of_range_indices() {
    let active = TempDir::new("plan-bad");
    let handle = make_handle(active.path(), None);

    let mut plan: *mut DataPlan = std::ptr::null_mut();
    assert_eq!(
        unsafe { nookery_data_plan_migration(handle, std::ptr::null(), &mut plan) },
        STATUS_INVALID_ARGUMENT
    );
    assert!(plan.is_null());

    assert_eq!(
        unsafe { nookery_data_plan_migration(handle, c"/legacy".as_ptr(), std::ptr::null_mut()) },
        STATUS_INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { nookery_data_plan_migration(handle, c"relative".as_ptr(), &mut plan) },
        STATUS_INVALID_ARGUMENT,
        "the legacy root is validated exactly like the handle root"
    );

    // An empty legacy directory plans nothing and still succeeds.
    let legacy = TempDir::new("plan-empty");
    let legacy_root = legacy.utf8();
    let status = unsafe { nookery_data_plan_migration(handle, legacy_root.as_ptr(), &mut plan) };
    assert_eq!(status, STATUS_OK);
    let (mut directories, mut files) = (usize::MAX, usize::MAX);
    assert_eq!(
        unsafe { nookery_data_plan_counts(plan, &mut directories, &mut files) },
        STATUS_OK
    );
    assert_eq!((directories, files), (0, 0));

    let mut out = Out::new(64);
    let mut buffer = out.buffer();
    assert_eq!(
        unsafe { nookery_data_plan_file(plan, 0, &mut buffer) },
        STATUS_INDEX_OUT_OF_RANGE
    );
    assert!(out.untouched());
    assert_eq!(
        unsafe { nookery_data_plan_counts(plan, std::ptr::null_mut(), &mut files) },
        STATUS_INVALID_ARGUMENT
    );

    unsafe { nookery_data_plan_destroy(plan) };
    unsafe { nookery_data_plan_destroy(std::ptr::null_mut()) };
    unsafe { nookery_data_handle_destroy(handle) };
}

/* --- status mapping fidelity -------------------------------------------- */

#[test]
fn core_statuses_map_one_to_one_with_upstream_messages() {
    // Every status the core models must render the exact upstream string, so a
    // C caller reading `nookery_data_status_message` sees what the C++ does.
    let expected: &[(RawStatus, &str)] = &[
        (STATUS_OK, ""),
        (STATUS_EMPTY_PATH, "Choose a folder."),
        (STATUS_NOT_DIRECTORY, "The selected path is not a folder."),
        (
            STATUS_CREATE_DIRECTORY_FAILED,
            "The selected folder could not be created.",
        ),
        (
            STATUS_OVERRIDE_ACTIVE,
            "The data folder is fixed by --user-dir for this session.",
        ),
        (
            STATUS_UNSUPPORTED,
            "Changing the data folder is not supported on this platform.",
        ),
        (
            STATUS_OPEN_FOLDER_FAILED,
            "The data folder could not be opened.",
        ),
    ];
    for (status, message) in expected {
        assert_eq!(&c_string(nookery_data_status_message(*status)), message);
    }
}
