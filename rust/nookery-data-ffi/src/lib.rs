//! C ABI adapter candidate over the `nookery-core` data-folder primitives.
//!
//! **This crate is not wired into the production application.**
//! `pc/src/acgc_data.cpp` remains the default path; CMake exposes a separate
//! opt-in parity executable.  The scope is the already-portable subset:
//! directory creation, layout enumeration, the two `pc_platform_*_path`
//! resolvers, and migration *planning*.  Window-system handoff
//! (`open_data_path`), the `--user-dir` override/restart bookkeeping
//! (`set_custom_data_path`, `is_data_path_restart_pending`), and platform path
//! discovery stay host-side and are deliberately absent.
//!
//! # Contract
//!
//! * Every entry point is total.  `rust/Cargo.toml` sets `panic = "abort"`, so
//!   a panic would kill the process with no status a caller can observe; every
//!   function validates its inputs and returns a [`abi::RawStatus`] instead.
//! * Every type crossing the boundary is `#[repr(C)]` and carries a
//!   `struct_size` or is versioned by [`ffi::ABI_VERSION`].
//! * Ownership is explicit: handles and plans are owned by the caller until the
//!   matching `*_destroy`; the only borrowed pointers returned are `'static`
//!   NUL-terminated strings that live for the process lifetime.
//! * Strings cross as UTF-8.  A non-UTF-8 path is reported, never lossily
//!   rewritten, because the upstream `u8string()` result is what callers get.
//!
//! # Divergence ledger
//!
//! | Upstream | Here | Why |
//! |---|---|---|
//! | `pc_platform_user_path` joins blindly | [`abi::STATUS_PATH_REJECTED`] for absolute/`..` inputs | joining an absolute path under the root silently escapes it |
//! | `ensure_folders` skips `texture_pack`/`nes_roms` when a legacy copy exists | always created | the legacy check needs `base_path_relative` + CWD, which this adapter does not own |
//! | migration copies files | [`ffi::nookery_data_plan_migration`] plans only | copying is Borealis' job and is out of scope for a data-folder ABI |
//! | `status_message` is `std::string` | [`ffi::nookery_data_status_message`] is `'static` | the message table is compile-time constant, so no allocation is needed |
//!
//! See `README.md` for what is proven, what is not, and the exact commands.

pub mod abi;
pub mod ffi;
pub mod handle;

pub use abi::{
    DirKind, NookeryDataBuffer, RawStatus, STATUS_BASE_PATH_UNAVAILABLE, STATUS_BUFFER_TOO_SMALL,
    STATUS_CREATE_DIRECTORY_FAILED, STATUS_EMPTY_PATH, STATUS_INDEX_OUT_OF_RANGE,
    STATUS_INVALID_ARGUMENT, STATUS_MAX, STATUS_NOT_DIRECTORY, STATUS_OK,
    STATUS_OPEN_FOLDER_FAILED, STATUS_OUT_OF_MEMORY, STATUS_OVERRIDE_ACTIVE, STATUS_PATH_REJECTED,
    STATUS_TABLE, STATUS_UNSUPPORTED, status_message, status_message_cstr, status_name,
    status_name_cstr,
};
pub use ffi::{
    ABI_VERSION, NookeryDataCreateOptions, NookeryDataDirReport, nookery_data_abi_version,
    nookery_data_base_path, nookery_data_directory, nookery_data_handle_create,
    nookery_data_handle_destroy, nookery_data_initialize, nookery_data_last_message,
    nookery_data_last_status, nookery_data_plan_counts, nookery_data_plan_destroy,
    nookery_data_plan_directory, nookery_data_plan_file, nookery_data_plan_migration,
    nookery_data_root, nookery_data_set_cache_root, nookery_data_status_count,
    nookery_data_status_message, nookery_data_status_name, nookery_data_user_path,
};
pub use handle::{DataHandle, DataPlan};
