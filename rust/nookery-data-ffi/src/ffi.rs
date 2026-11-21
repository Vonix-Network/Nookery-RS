//! Total `extern "C"` entry points.
//!
//! Rules every function here follows, because `panic = "abort"` makes any panic
//! an undeclared process exit rather than a status a caller can handle:
//!
//! 1. Null-check every pointer parameter before use.
//! 2. Validate every `struct_size` and any ABI version field.
//! 3. Decode all strings as UTF-8; never `unwrap`, never index unchecked.
//! 4. Return a status; redact nothing, invent nothing.
//! 5. Record the outcome on the handle so `nookery_data_last_status` agrees.

use std::ffi::{CStr, c_char};
use std::mem::size_of;
use std::path::Path;

use nookery_core::{BasePath, initialize, plan_migration};

use crate::abi::{
    DirKind, NookeryDataBuffer, RawStatus, STATUS_BASE_PATH_UNAVAILABLE, STATUS_INDEX_OUT_OF_RANGE,
    STATUS_INVALID_ARGUMENT, STATUS_OK, STATUS_OUT_OF_MEMORY, STATUS_PATH_REJECTED, STATUS_TABLE,
    status_message_cstr, status_name_cstr,
};
use crate::handle::{DataHandle, DataPlan, destroy};

/// Layout version of every struct in `include/nookery_data.h`.
pub const ABI_VERSION: u32 = 1;
pub const MAX_PATH_BYTES: usize = 4096;

/// The `NOOKERY_DATA_ABI_VERSION` a C caller must send.
#[unsafe(no_mangle)]
pub extern "C" fn nookery_data_abi_version() -> u32 {
    ABI_VERSION
}

/// Stable identifier for a status, e.g. `"PATH_REJECTED"`.
#[unsafe(no_mangle)]
pub extern "C" fn nookery_data_status_name(status: RawStatus) -> *const c_char {
    // Every entry is a NUL-terminated literal that lives for the process.
    status_name_cstr(status).as_ptr()
}

/// Upstream `status_message` wording, `""` where upstream has none.
#[unsafe(no_mangle)]
pub extern "C" fn nookery_data_status_message(status: RawStatus) -> *const c_char {
    status_message_cstr(status).as_ptr()
}

/// Decode a required C string as UTF-8.
///
/// # Safety
/// `text` must be null or a valid NUL-terminated string.
unsafe fn required_str<'a>(text: *const c_char) -> Result<&'a str, RawStatus> {
    if text.is_null() {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller contract: non-null, NUL-terminated.
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes();
    if bytes.len() > MAX_PATH_BYTES {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    std::str::from_utf8(bytes).map_err(|_| STATUS_INVALID_ARGUMENT)
}

/// Decode an optional C string as UTF-8.
///
/// # Safety
/// `text` must be null or a valid NUL-terminated string.
unsafe fn optional_str<'a>(text: *const c_char) -> Result<Option<&'a str>, RawStatus> {
    if text.is_null() {
        return Ok(None);
    }
    // SAFETY: forwarded contract.
    unsafe { required_str(text) }.map(Some)
}

/// Borrow a caller buffer for writing.
///
/// # Safety
/// `out` must be null or point to a readable/writable `NookeryDataBuffer`.
unsafe fn buffer<'a>(out: *mut NookeryDataBuffer) -> Option<&'a mut NookeryDataBuffer> {
    // SAFETY: caller contract.
    unsafe { out.as_mut() }
}

/// Borrow a caller handle.
///
/// # Safety
/// `handle` must be null or a live `into_raw` pointer.
unsafe fn handle<'a>(handle: *mut DataHandle) -> Option<&'a mut DataHandle> {
    // SAFETY: caller contract.
    unsafe { handle.as_mut() }
}

/* --- Handles ------------------------------------------------------------- */

/// `NookeryDataCreateOptions`, as it arrives from C.
#[repr(C)]
pub struct NookeryDataCreateOptions {
    pub struct_size: u32,
    pub abi_version: u32,
    pub root_utf8: *const c_char,
    pub base_root_utf8: *const c_char,
}

impl NookeryDataCreateOptions {
    /// The `struct_size` a C caller must send.
    pub const SIZE: u32 = size_of::<Self>() as u32;
}

/// `NookeryDataDirReport`, as it arrives from C.
#[repr(C)]
pub struct NookeryDataDirReport {
    pub struct_size: u32,
    pub failure_count: u32,
}

impl NookeryDataDirReport {
    /// The `struct_size` a C caller must send.
    pub const SIZE: u32 = size_of::<Self>() as u32;
}

/// Create an owned handle.  No filesystem access.
///
/// # Safety
/// `options` must be null or a valid `NookeryDataCreateOptions`; `out_handle`
/// must be null or point to writable storage; string fields follow `required_str`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_handle_create(
    options: *const NookeryDataCreateOptions,
    out_handle: *mut *mut DataHandle,
) -> RawStatus {
    if out_handle.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: non-null and writable by contract.
    unsafe { *out_handle = std::ptr::null_mut() };

    let Some(options) = (unsafe { options.as_ref() }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    if options.struct_size != NookeryDataCreateOptions::SIZE {
        return STATUS_INVALID_ARGUMENT;
    }
    if options.abi_version != ABI_VERSION {
        return STATUS_INVALID_ARGUMENT;
    }

    // SAFETY: string fields follow the `required_str` contract.
    let (root, base_root) = unsafe {
        match (
            required_str(options.root_utf8),
            optional_str(options.base_root_utf8),
        ) {
            (Ok(root), Ok(base_root)) => (root, base_root),
            _ => return STATUS_INVALID_ARGUMENT,
        }
    };

    let handle = match DataHandle::new(root, base_root) {
        Ok(handle) => handle,
        Err(status) => return status,
    };
    let raw = DataHandle::into_raw(handle);
    if raw.is_null() {
        return STATUS_OUT_OF_MEMORY;
    }
    // SAFETY: non-null writable out-parameter.
    unsafe { *out_handle = raw };
    STATUS_OK
}

/// Free a handle; null is accepted.
///
/// # Safety
/// `handle` must be null or a live `into_raw` pointer not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_handle_destroy(handle: *mut DataHandle) {
    // SAFETY: forwarded contract.
    unsafe { destroy(handle) };
}

/// Override the cache directory used by initialization.  This is an explicit
/// injection point for hosts whose Borealis `Paths::cachePath` is outside the
/// active data root; the default remains `<root>/cache` for ABI compatibility.
///
/// # Safety
/// `handle` must be null or a live handle; `cache_root_utf8` must be null or a
/// valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_set_cache_root(
    handle_ptr: *mut DataHandle,
    cache_root_utf8: *const c_char,
) -> RawStatus {
    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: forwarded string contract.
    let Ok(cache_root) = (unsafe { required_str(cache_root_utf8) }) else {
        handle.record(STATUS_INVALID_ARGUMENT);
        return STATUS_INVALID_ARGUMENT;
    };
    let status = match handle.set_cache_root(cache_root) {
        Ok(()) => STATUS_OK,
        Err(status) => status,
    };
    handle.record(status);
    status
}

/* --- Initialization ------------------------------------------------------ */

/// Create the required directories below the handle's root.
///
/// # Safety
/// `handle` must be null or a live handle; `out_report` must be null or a valid
/// `NookeryDataDirReport`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_initialize(
    handle_ptr: *mut DataHandle,
    out_report: *mut NookeryDataDirReport,
) -> RawStatus {
    // Validate the report before doing any work so a caller bug cannot cause
    // directory creation followed by an unannounced layout mismatch.
    if !out_report.is_null() {
        // SAFETY: non-null, readable by contract.
        let report = unsafe { &*out_report };
        if report.struct_size != NookeryDataDirReport::SIZE {
            return STATUS_INVALID_ARGUMENT;
        }
    }

    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        return STATUS_INVALID_ARGUMENT;
    };

    let initialized = initialize(handle.layout());
    let failures: Vec<_> = initialized.failures().to_vec();
    let status = if failures.is_empty() {
        STATUS_OK
    } else {
        crate::abi::status_of(failures[0].code)
    };

    if !out_report.is_null() {
        // SAFETY: validated above and non-null.
        unsafe {
            (*out_report).failure_count = u32::try_from(failures.len()).unwrap_or(u32::MAX);
        }
    }

    match failures.first() {
        Some(first) => handle.record_status(first),
        None => handle.record(STATUS_OK),
    }
    if status == STATUS_OK {
        handle.mark_initialized(handle.layout().clone());
    }
    status
}

/* --- Layout queries ------------------------------------------------------ */

/// Write the active data root.
///
/// # Safety
/// `handle` must be null or a live handle; `out` must be null or a valid buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_root(
    handle_ptr: *mut DataHandle,
    out: *mut NookeryDataBuffer,
) -> RawStatus {
    let Some(out) = (unsafe { buffer(out) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        out.length = 0;
        return STATUS_INVALID_ARGUMENT;
    };
    let status = out.write_path(handle.layout().root());
    handle.record(status);
    status
}

/// Write one required directory by [`DirKind`] selector.
///
/// # Safety
/// As [`nookery_data_root`]; `kind` must be a valid `NookeryDataDirKind`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_directory(
    handle_ptr: *mut DataHandle,
    kind: u32,
    out: *mut NookeryDataBuffer,
) -> RawStatus {
    let Some(out) = (unsafe { buffer(out) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        out.length = 0;
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(kind) = DirKind::from_raw(kind) else {
        out.length = 0;
        handle.record(STATUS_INDEX_OUT_OF_RANGE);
        return STATUS_INDEX_OUT_OF_RANGE;
    };
    let status = out.write_path(&kind.path(handle.layout()));
    handle.record(status);
    status
}

/* --- Path resolution ----------------------------------------------------- */

/// Port of `pc_platform_user_path`: join `relative_utf8` below the data root.
///
/// Deliberate divergence, documented in `layout.rs`: upstream's
/// `std::filesystem` join treats an absolute operand as a replacement, so
/// `"/etc/passwd"` would escape; here it is [`STATUS_PATH_REJECTED`].
///
/// # Safety
/// `handle` must be null or a live handle; `relative_utf8` must be null or a
/// valid NUL-terminated string; `out` must be null or a valid buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_user_path(
    handle_ptr: *mut DataHandle,
    relative_utf8: *const c_char,
    out: *mut NookeryDataBuffer,
) -> RawStatus {
    let Some(out) = (unsafe { buffer(out) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        out.length = 0;
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: forwarded string contract.
    let Ok(relative) = (unsafe { required_str(relative_utf8) }) else {
        out.length = 0;
        handle.record(STATUS_INVALID_ARGUMENT);
        return STATUS_INVALID_ARGUMENT;
    };

    // Every rejection (empty, absolute, rooted, traversal) is one deliberate
    // hardening class; upstream answers all of them by writing an escaping
    // path, so a single status is the honest report.
    let status = match handle.layout().relative(relative) {
        Ok(path) => out.write_path(&path),
        Err(_) => {
            out.length = 0;
            STATUS_PATH_REJECTED
        }
    };
    handle.record(status);
    status
}

/// Port of `pc_platform_base_path`: join `relative_utf8` below the base root.
///
/// # Safety
/// As [`nookery_data_user_path`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_base_path(
    handle_ptr: *mut DataHandle,
    relative_utf8: *const c_char,
    out: *mut NookeryDataBuffer,
) -> RawStatus {
    let Some(out) = (unsafe { buffer(out) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        out.length = 0;
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: forwarded string contract.
    let Ok(relative) = (unsafe { required_str(relative_utf8) }) else {
        out.length = 0;
        handle.record(STATUS_INVALID_ARGUMENT);
        return STATUS_INVALID_ARGUMENT;
    };

    let base = match handle.base_root() {
        Some(base) => BasePath::new(base),
        None => {
            out.length = 0;
            handle.record(STATUS_BASE_PATH_UNAVAILABLE);
            return STATUS_BASE_PATH_UNAVAILABLE;
        }
    };

    let status = match base {
        Ok(base) => match base.relative(relative) {
            Ok(Some(path)) => out.write_path(&path),
            Ok(None) => {
                out.length = 0;
                STATUS_BASE_PATH_UNAVAILABLE
            }
            Err(_) => {
                out.length = 0;
                STATUS_PATH_REJECTED
            }
        },
        Err(status) => {
            out.length = 0;
            crate::abi::status_of(status.code)
        }
    };
    handle.record(status);
    status
}

/* --- Diagnostics --------------------------------------------------------- */

/// The status recorded by the most recent handle operation.
///
/// # Safety
/// `handle` must be null or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_last_status(handle_ptr: *const DataHandle) -> RawStatus {
    // SAFETY: caller contract; null is reported rather than dereferenced.
    match unsafe { handle_ptr.as_ref() } {
        Some(handle) => handle.last_status(),
        None => STATUS_INVALID_ARGUMENT,
    }
}

/// Rendered diagnostic for the most recent operation.
///
/// # Safety
/// `handle` must be null or a live handle; `out` must be null or a valid buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_last_message(
    handle_ptr: *mut DataHandle,
    out: *mut NookeryDataBuffer,
) -> RawStatus {
    let Some(out) = (unsafe { buffer(out) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        out.length = 0;
        return STATUS_INVALID_ARGUMENT;
    };
    // Pure read: recording here would overwrite the very status being reported.
    handle.write_last_message(out)
}

/* --- Migration planning -------------------------------------------------- */

/// Plan migration from `legacy_root_utf8` into the active root.  Copies nothing.
///
/// # Safety
/// `handle` must be null or a live handle; `legacy_root_utf8` must be null or a
/// valid NUL-terminated string; `out_plan` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_plan_migration(
    handle_ptr: *mut DataHandle,
    legacy_root_utf8: *const c_char,
    out_plan: *mut *mut DataPlan,
) -> RawStatus {
    if out_plan.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: non-null writable out-parameter.
    unsafe { *out_plan = std::ptr::null_mut() };

    let Some(handle) = (unsafe { handle(handle_ptr) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: forwarded string contract.
    let Ok(legacy) = (unsafe { required_str(legacy_root_utf8) }) else {
        handle.record(STATUS_INVALID_ARGUMENT);
        return STATUS_INVALID_ARGUMENT;
    };
    let legacy = Path::new(legacy);
    if let Err(status) = crate::abi::validate_root(legacy) {
        handle.record(status);
        return status;
    }

    let plan = plan_migration(legacy, handle.layout().root());
    let plan = match DataPlan::from_core(plan) {
        Ok(plan) => plan,
        Err(status) => {
            handle.record(status);
            return status;
        }
    };
    let raw = DataPlan::into_raw(plan);
    if raw.is_null() {
        handle.record(STATUS_OUT_OF_MEMORY);
        return STATUS_OUT_OF_MEMORY;
    }
    // SAFETY: non-null writable out-parameter.
    unsafe { *out_plan = raw };
    handle.record(STATUS_OK);
    STATUS_OK
}

/// Report plan entry counts.
///
/// # Safety
/// `plan` must be null or a live plan; both out-pointers must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_plan_counts(
    plan: *const DataPlan,
    out_directories: *mut usize,
    out_files: *mut usize,
) -> RawStatus {
    // SAFETY: caller contract.
    let Some(plan) = (unsafe { plan.as_ref() }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    if out_directories.is_null() || out_files.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: validated non-null writable out-parameters.
    unsafe {
        *out_directories = plan.directory_count();
        *out_files = plan.file_count();
    }
    STATUS_OK
}

/// Write plan directory `index`.
///
/// # Safety
/// `plan` must be null or a live plan; `out` must be null or a valid buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_plan_directory(
    plan: *const DataPlan,
    index: usize,
    out: *mut NookeryDataBuffer,
) -> RawStatus {
    let Some(out) = (unsafe { buffer(out) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: caller contract.
    let Some(plan) = (unsafe { plan.as_ref() }) else {
        out.length = 0;
        return STATUS_INVALID_ARGUMENT;
    };
    match plan.directory(index) {
        Some(path) => out.write_path(path),
        None => {
            out.length = 0;
            STATUS_INDEX_OUT_OF_RANGE
        }
    }
}

/// Write plan file `index`.
///
/// # Safety
/// As [`nookery_data_plan_directory`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_plan_file(
    plan: *const DataPlan,
    index: usize,
    out: *mut NookeryDataBuffer,
) -> RawStatus {
    let Some(out) = (unsafe { buffer(out) }) else {
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: caller contract.
    let Some(plan) = (unsafe { plan.as_ref() }) else {
        out.length = 0;
        return STATUS_INVALID_ARGUMENT;
    };
    match plan.file(index) {
        Some(path) => out.write_path(path),
        None => {
            out.length = 0;
            STATUS_INDEX_OUT_OF_RANGE
        }
    }
}

/// Free a plan; null is accepted.
///
/// # Safety
/// `plan` must be null or a live plan not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_data_plan_destroy(plan: *mut DataPlan) {
    // SAFETY: forwarded contract.
    unsafe { DataPlan::free_raw(plan) };
}

/// Number of defined statuses.  A C caller can iterate `0..len` to check that
/// every status it compiles against is known to the linked library.
#[unsafe(no_mangle)]
pub extern "C" fn nookery_data_status_count() -> u32 {
    STATUS_TABLE.len() as u32
}
