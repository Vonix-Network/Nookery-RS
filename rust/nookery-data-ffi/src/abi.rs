//! `#[repr(C)]` types, status codes, and string-buffer mechanics.
//!
//! Everything a C caller can name lives here, so the FFI entry points in
//! [`crate::ffi`] contain only validation and delegation.
//!
//! Status numbering: `0..=12` are exactly `nookery_core::ErrorCode` in
//! declaration order, which is the upstream `borealis::data::ErrorCode` order
//! consumed by `pc/src/acgc_data.cpp::status_message`.  `13..=18` are ABI-level
//! outcomes that upstream expresses as a bare `0`/`false` return and therefore
//! have no numeric counterpart.

use std::ffi::{CStr, c_char};
use std::mem::size_of;
use std::path::{Component, Path, PathBuf};

use nookery_core::ErrorCode;

/// The integer type crossing the ABI for every outcome.
pub type RawStatus = u32;

pub const STATUS_OK: RawStatus = 0;
pub const STATUS_NOT_INITIALIZED: RawStatus = 1;
pub const STATUS_PREFERENCE_PATH_UNAVAILABLE: RawStatus = 2;
pub const STATUS_EMPTY_PATH: RawStatus = 3;
pub const STATUS_CREATE_DIRECTORY_FAILED: RawStatus = 4;
pub const STATUS_NOT_DIRECTORY: RawStatus = 5;
pub const STATUS_WRITE_PROBE_FAILED: RawStatus = 6;
pub const STATUS_WRITE_PROBE_CLEANUP_FAILED: RawStatus = 7;
pub const STATUS_DESCRIPTOR_WRITE_FAILED: RawStatus = 8;
pub const STATUS_MIGRATION_INCOMPLETE: RawStatus = 9;
pub const STATUS_OVERRIDE_ACTIVE: RawStatus = 10;
pub const STATUS_UNSUPPORTED: RawStatus = 11;
pub const STATUS_OPEN_FOLDER_FAILED: RawStatus = 12;
pub const STATUS_INVALID_ARGUMENT: RawStatus = 13;
pub const STATUS_BUFFER_TOO_SMALL: RawStatus = 14;
pub const STATUS_PATH_REJECTED: RawStatus = 15;
pub const STATUS_BASE_PATH_UNAVAILABLE: RawStatus = 16;
pub const STATUS_INDEX_OUT_OF_RANGE: RawStatus = 17;
pub const STATUS_OUT_OF_MEMORY: RawStatus = 18;

/// Highest defined status.  Used by tests to sweep the whole space.
pub const STATUS_MAX: RawStatus = STATUS_OUT_OF_MEMORY;

/// Every `(macro suffix, value)` pair, in declaration order.
///
/// Pointer-valued, NUL-terminated: `include/nookery_data.h` defines the same
/// names, and `tests/abi.rs` cross-checks this table against that header.
pub const STATUS_TABLE: &[(&CStr, RawStatus)] = &[
    (c"OK", STATUS_OK),
    (c"NOT_INITIALIZED", STATUS_NOT_INITIALIZED),
    (
        c"PREFERENCE_PATH_UNAVAILABLE",
        STATUS_PREFERENCE_PATH_UNAVAILABLE,
    ),
    (c"EMPTY_PATH", STATUS_EMPTY_PATH),
    (c"CREATE_DIRECTORY_FAILED", STATUS_CREATE_DIRECTORY_FAILED),
    (c"NOT_DIRECTORY", STATUS_NOT_DIRECTORY),
    (c"WRITE_PROBE_FAILED", STATUS_WRITE_PROBE_FAILED),
    (
        c"WRITE_PROBE_CLEANUP_FAILED",
        STATUS_WRITE_PROBE_CLEANUP_FAILED,
    ),
    (c"DESCRIPTOR_WRITE_FAILED", STATUS_DESCRIPTOR_WRITE_FAILED),
    (c"MIGRATION_INCOMPLETE", STATUS_MIGRATION_INCOMPLETE),
    (c"OVERRIDE_ACTIVE", STATUS_OVERRIDE_ACTIVE),
    (c"UNSUPPORTED", STATUS_UNSUPPORTED),
    (c"OPEN_FOLDER_FAILED", STATUS_OPEN_FOLDER_FAILED),
    (c"INVALID_ARGUMENT", STATUS_INVALID_ARGUMENT),
    (c"BUFFER_TOO_SMALL", STATUS_BUFFER_TOO_SMALL),
    (c"PATH_REJECTED", STATUS_PATH_REJECTED),
    (c"BASE_PATH_UNAVAILABLE", STATUS_BASE_PATH_UNAVAILABLE),
    (c"INDEX_OUT_OF_RANGE", STATUS_INDEX_OUT_OF_RANGE),
    (c"OUT_OF_MEMORY", STATUS_OUT_OF_MEMORY),
];

/// The `NOOKERY_DATA_STATUS_`-less macro name as a C string, or `"<unknown>"`.
pub fn status_name_cstr(status: RawStatus) -> &'static CStr {
    STATUS_TABLE
        .iter()
        .find(|(_, value)| *value == status)
        .map_or(c"<unknown>", |(name, _)| *name)
}

/// The name as Rust text, for diagnostics and tests.
pub fn status_name(status: RawStatus) -> &'static str {
    status_name_cstr(status).to_str().unwrap_or("<non-utf8>")
}

/// The upstream `status_message` wording as a NUL-terminated C string.
///
/// Statuses `0..=12` carry the byte-identical upstream text; `tests` asserts each
/// one equals [`ErrorCode::message`], so this table cannot drift from the core.
/// ABI-level statuses (`13..=18`) have no upstream wording: upstream returns a
/// bare failure, so anything invented here would be a fabricated user-facing
/// string. They render as `""`.
pub fn status_message_cstr(status: RawStatus) -> &'static CStr {
    match error_code_of(status) {
        Some(ErrorCode::None) => c"",
        Some(ErrorCode::NotInitialized) => c"Data folders have not been initialized.",
        Some(ErrorCode::PreferencePathUnavailable) => c"The system data folder is unavailable.",
        Some(ErrorCode::EmptyPath) => c"Choose a folder.",
        Some(ErrorCode::CreateDirectoryFailed) => c"The selected folder could not be created.",
        Some(ErrorCode::NotDirectory) => c"The selected path is not a folder.",
        Some(ErrorCode::WriteProbeFailed) => c"The selected folder is not writable.",
        Some(ErrorCode::WriteProbeCleanupFailed) => c"The selected folder could not be validated.",
        Some(ErrorCode::DescriptorWriteFailed) => c"The data folder setting could not be saved.",
        Some(ErrorCode::MigrationIncomplete) => c"Data migration could not be completed.",
        Some(ErrorCode::OverrideActive) => {
            c"The data folder is fixed by --user-dir for this session."
        }
        Some(ErrorCode::Unsupported) => {
            c"Changing the data folder is not supported on this platform."
        }
        Some(ErrorCode::OpenFolderFailed) => c"The data folder could not be opened.",
        None => c"",
    }
}

/// The message as Rust text, for diagnostics and tests.
pub fn status_message(status: RawStatus) -> &'static str {
    status_message_cstr(status).to_str().unwrap_or("")
}

/// Map a status back onto the core enum, for the statuses that are core codes.
pub const fn error_code_of(status: RawStatus) -> Option<ErrorCode> {
    Some(match status {
        STATUS_OK => ErrorCode::None,
        STATUS_NOT_INITIALIZED => ErrorCode::NotInitialized,
        STATUS_PREFERENCE_PATH_UNAVAILABLE => ErrorCode::PreferencePathUnavailable,
        STATUS_EMPTY_PATH => ErrorCode::EmptyPath,
        STATUS_CREATE_DIRECTORY_FAILED => ErrorCode::CreateDirectoryFailed,
        STATUS_NOT_DIRECTORY => ErrorCode::NotDirectory,
        STATUS_WRITE_PROBE_FAILED => ErrorCode::WriteProbeFailed,
        STATUS_WRITE_PROBE_CLEANUP_FAILED => ErrorCode::WriteProbeCleanupFailed,
        STATUS_DESCRIPTOR_WRITE_FAILED => ErrorCode::DescriptorWriteFailed,
        STATUS_MIGRATION_INCOMPLETE => ErrorCode::MigrationIncomplete,
        STATUS_OVERRIDE_ACTIVE => ErrorCode::OverrideActive,
        STATUS_UNSUPPORTED => ErrorCode::Unsupported,
        STATUS_OPEN_FOLDER_FAILED => ErrorCode::OpenFolderFailed,
        _ => return None,
    })
}

/// The status a core [`ErrorCode`] reports.  Total inverse of [`error_code_of`].
pub const fn status_of(code: ErrorCode) -> RawStatus {
    match code {
        ErrorCode::None => STATUS_OK,
        ErrorCode::NotInitialized => STATUS_NOT_INITIALIZED,
        ErrorCode::PreferencePathUnavailable => STATUS_PREFERENCE_PATH_UNAVAILABLE,
        ErrorCode::EmptyPath => STATUS_EMPTY_PATH,
        ErrorCode::CreateDirectoryFailed => STATUS_CREATE_DIRECTORY_FAILED,
        ErrorCode::NotDirectory => STATUS_NOT_DIRECTORY,
        ErrorCode::WriteProbeFailed => STATUS_WRITE_PROBE_FAILED,
        ErrorCode::WriteProbeCleanupFailed => STATUS_WRITE_PROBE_CLEANUP_FAILED,
        ErrorCode::DescriptorWriteFailed => STATUS_DESCRIPTOR_WRITE_FAILED,
        ErrorCode::MigrationIncomplete => STATUS_MIGRATION_INCOMPLETE,
        ErrorCode::OverrideActive => STATUS_OVERRIDE_ACTIVE,
        ErrorCode::Unsupported => STATUS_UNSUPPORTED,
        ErrorCode::OpenFolderFailed => STATUS_OPEN_FOLDER_FAILED,
    }
}

/// In/out UTF-8 string buffer.  Field order is ABI.
///
/// Semantics are documented in `include/nookery_data.h`; in short: `length`
/// always reports the size needed *including* the NUL terminator, and bytes are
/// only written when that fits in `capacity`.
#[repr(C)]
pub struct NookeryDataBuffer {
    /// Must equal `size_of::<NookeryDataBuffer>()`.
    pub struct_size: u32,
    /// Caller storage; may be null only while `capacity` is 0.
    pub ptr: *mut c_char,
    /// Bytes available at `ptr`.
    pub capacity: usize,
    /// Out: bytes written (excluding NUL), or bytes required (including NUL).
    pub length: usize,
}

impl NookeryDataBuffer {
    /// The `struct_size` a C caller must send.
    pub const SIZE: u32 = size_of::<Self>() as u32;

    /// Validate the in-fields without touching caller memory beyond `self`.
    fn validate(&self) -> Result<(), RawStatus> {
        if self.struct_size != Self::SIZE {
            return Err(STATUS_INVALID_ARGUMENT);
        }
        if self.ptr.is_null() && self.capacity != 0 {
            return Err(STATUS_INVALID_ARGUMENT);
        }
        Ok(())
    }

    /// Copy `bytes` as a NUL-terminated string, or report the required size.
    ///
    /// On [`STATUS_BUFFER_TOO_SMALL`] nothing is written and `length` holds the
    /// required size including the terminator, so the caller can retry.
    pub fn write(&mut self, bytes: &[u8]) -> RawStatus {
        if let Err(status) = self.validate() {
            self.length = 0;
            return status;
        }
        let needed = bytes.len() + 1;
        if needed > self.capacity {
            self.length = needed;
            return STATUS_BUFFER_TOO_SMALL;
        }
        // SAFETY: `validate` proved `ptr` is non-null when `capacity != 0`, and
        // `needed <= capacity`, so `0..needed` is in bounds of caller storage.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), self.ptr, bytes.len());
            *self.ptr.add(bytes.len()) = 0;
        }
        self.length = bytes.len();
        STATUS_OK
    }

    /// Copy a path's UTF-8 bytes.  Non-UTF-8 paths cannot cross this ABI (the
    /// upstream `u8string()` conversion would also have to pick an encoding),
    /// so they are reported instead of lossily rewritten.
    pub fn write_path(&mut self, path: &Path) -> RawStatus {
        match path.to_str() {
            Some(text) => self.write(text.as_bytes()),
            None => {
                self.length = 0;
                STATUS_INVALID_ARGUMENT
            }
        }
    }
}

/// Directory selector for [`crate::ffi::nookery_data_directory`].
///
/// Order matches `nookery_core::DataLayout::required_directories` plus the cache
/// directory, which upstream appends in `ensure_folders`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirKind {
    SaveCardA = 0,
    SaveCardB = 1,
    TexturePack = 2,
    NesRoms = 3,
    Cache = 4,
}

impl DirKind {
    /// Decode a C `NookeryDataDirKind`, rejecting anything else.
    pub const fn from_raw(kind: u32) -> Option<Self> {
        Some(match kind {
            0 => Self::SaveCardA,
            1 => Self::SaveCardB,
            2 => Self::TexturePack,
            3 => Self::NesRoms,
            4 => Self::Cache,
            _ => return None,
        })
    }

    /// The path this selector names below `layout`.
    pub fn path(self, layout: &nookery_core::DataLayout) -> PathBuf {
        match self {
            Self::SaveCardA => layout.save_card_a(),
            Self::SaveCardB => layout.save_card_b(),
            Self::TexturePack => layout.texture_pack(),
            Self::NesRoms => layout.nes_roms(),
            Self::Cache => layout.cache().to_path_buf(),
        }
    }
}

/// Reject roots that are not absolute or that smuggle in `..`.
///
/// The core already refuses relative *child* paths; this is the same rule
/// applied to the root itself, so a handle can never describe a path that
/// resolves outside what the caller passed.
pub fn validate_root(path: &Path) -> Result<(), RawStatus> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_numbers_match_core_error_codes() {
        // Every core code has a status, and the round trip is lossless.
        for code in [
            ErrorCode::None,
            ErrorCode::NotInitialized,
            ErrorCode::PreferencePathUnavailable,
            ErrorCode::EmptyPath,
            ErrorCode::CreateDirectoryFailed,
            ErrorCode::NotDirectory,
            ErrorCode::WriteProbeFailed,
            ErrorCode::WriteProbeCleanupFailed,
            ErrorCode::DescriptorWriteFailed,
            ErrorCode::MigrationIncomplete,
            ErrorCode::OverrideActive,
            ErrorCode::Unsupported,
            ErrorCode::OpenFolderFailed,
        ] {
            assert_eq!(error_code_of(status_of(code)), Some(code));
        }
        assert_eq!(status_of(ErrorCode::None), STATUS_OK);
        assert_eq!(
            status_of(ErrorCode::OpenFolderFailed),
            STATUS_OPEN_FOLDER_FAILED
        );
        assert_eq!(error_code_of(STATUS_INVALID_ARGUMENT), None);
    }

    #[test]
    fn status_messages_are_the_upstream_strings_and_empty_off_table() {
        assert_eq!(status_message(STATUS_OK), "");
        assert_eq!(status_message(STATUS_EMPTY_PATH), "Choose a folder.");
        assert_eq!(
            status_message(STATUS_NOT_DIRECTORY),
            "The selected path is not a folder."
        );
        assert_eq!(status_message(STATUS_INVALID_ARGUMENT), "");
        assert_eq!(status_message(STATUS_MAX), "");
        for (name, value) in STATUS_TABLE {
            let name = name.to_str().expect("status names are ASCII");
            assert_eq!(status_name(*value), name);
        }
        assert_eq!(status_name(STATUS_MAX + 1), "<unknown>");
    }

    #[test]
    fn c_string_messages_are_byte_identical_to_the_core_messages() {
        // The C-facing table is hand-written so the pointers are NUL-terminated;
        // this is the guard that it cannot drift from `ErrorCode::message`.
        for (_, value) in STATUS_TABLE {
            if let Some(code) = error_code_of(*value) {
                assert_eq!(
                    status_message_cstr(*value).to_bytes(),
                    code.message().as_bytes(),
                    "status {value} diverged from nookery_core"
                );
            } else {
                assert_eq!(status_message_cstr(*value).to_bytes(), b"");
            }
        }
    }

    #[test]
    fn buffer_reports_required_size_before_writing() {
        let mut storage = [0x7fi8 as c_char; 4];
        let mut buffer = NookeryDataBuffer {
            struct_size: NookeryDataBuffer::SIZE,
            ptr: storage.as_mut_ptr(),
            capacity: storage.len(),
            length: 0,
        };
        assert_eq!(buffer.write(b"abcd"), STATUS_BUFFER_TOO_SMALL);
        assert_eq!(buffer.length, 5, "required size includes the terminator");
        assert!(storage.iter().all(|byte| *byte == 0x7f), "nothing written");

        assert_eq!(buffer.write(b"abc"), STATUS_OK);
        assert_eq!(buffer.length, 3);
        let text = unsafe { std::ffi::CStr::from_ptr(storage.as_ptr()) };
        assert_eq!(text.to_bytes(), b"abc");
    }

    #[test]
    fn buffer_rejects_bad_layout_and_null_storage() {
        let mut null_buffer = NookeryDataBuffer {
            struct_size: NookeryDataBuffer::SIZE - 1,
            ptr: std::ptr::null_mut(),
            capacity: 0,
            length: 9,
        };
        assert_eq!(null_buffer.write(b"x"), STATUS_INVALID_ARGUMENT);
        assert_eq!(null_buffer.length, 0);

        let mut null_with_capacity = NookeryDataBuffer {
            struct_size: NookeryDataBuffer::SIZE,
            ptr: std::ptr::null_mut(),
            capacity: 8,
            length: 9,
        };
        assert_eq!(null_with_capacity.write(b"x"), STATUS_INVALID_ARGUMENT);

        let mut empty_capacity = NookeryDataBuffer {
            struct_size: NookeryDataBuffer::SIZE,
            ptr: std::ptr::null_mut(),
            capacity: 0,
            length: 0,
        };
        assert_eq!(empty_capacity.write(b""), STATUS_BUFFER_TOO_SMALL);
        assert_eq!(
            empty_capacity.length, 1,
            "an empty string still needs a NUL"
        );
    }

    #[test]
    fn roots_must_be_absolute_and_free_of_parent_components() {
        assert_eq!(validate_root(Path::new("/data")), Ok(()));
        assert_eq!(validate_root(Path::new("")), Err(STATUS_INVALID_ARGUMENT));
        assert_eq!(
            validate_root(Path::new("data")),
            Err(STATUS_INVALID_ARGUMENT)
        );
        assert_eq!(
            validate_root(Path::new("/data/../etc")),
            Err(STATUS_INVALID_ARGUMENT)
        );
    }

    #[test]
    fn directory_kinds_match_the_required_order() {
        let layout = nookery_core::DataLayout::new("/data");
        let paths: Vec<PathBuf> = (0..5)
            .filter_map(DirKind::from_raw)
            .map(|kind| kind.path(&layout))
            .collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/data/save/card_a"),
                PathBuf::from("/data/save/card_b"),
                PathBuf::from("/data/texture_pack"),
                PathBuf::from("/data/nes_roms"),
                PathBuf::from("/data/cache"),
            ]
        );
        assert_eq!(DirKind::from_raw(5), None);
    }
}
