//! Outcome and diagnostic types shared by the data-manager primitive.
//!
//! The enumerators and operator-visible strings mirror
//! `borealis::data::Status` / `borealis::data::ErrorCode` as they are consumed
//! by `pc/src/acgc_data.cpp` (`status_message`) and `pc/src/ui/settings.cpp`
//! (`data_folder_error_message`).  Borealis itself is not vendored at this pin,
//! so the type shape below is derived from those call sites only.

use std::fmt;
use std::path::{Path, PathBuf};

/// Failure modes surfaced by the data-folder layer.
///
/// Declaration order matches the upstream `switch` statements; the `None`
/// arm means "success", not "unknown".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// No error; the operation succeeded.
    None,
    /// `initialize` was never called before the operation.
    NotInitialized,
    /// The platform preference path could not be resolved.
    PreferencePathUnavailable,
    /// The caller supplied an empty folder.
    EmptyPath,
    /// The folder could not be created.
    CreateDirectoryFailed,
    /// The path exists but is not a directory.
    NotDirectory,
    /// The write probe inside the folder failed.
    WriteProbeFailed,
    /// The write probe file could not be removed.
    WriteProbeCleanupFailed,
    /// The folder setting could not be persisted.
    DescriptorWriteFailed,
    /// Legacy data could not be migrated completely.
    MigrationIncomplete,
    /// The folder is pinned by `--user-dir` for this session.
    OverrideActive,
    /// The platform does not support the requested operation.
    Unsupported,
    /// The folder could not be handed to the window system.
    OpenFolderFailed,
}

impl ErrorCode {
    /// The upstream operator-facing message, byte-for-byte.
    ///
    /// Source: `pc/src/acgc_data.cpp`, `status_message`.
    pub const fn message(self) -> &'static str {
        match self {
            Self::None => "",
            Self::NotInitialized => "Data folders have not been initialized.",
            Self::PreferencePathUnavailable => "The system data folder is unavailable.",
            Self::EmptyPath => "Choose a folder.",
            Self::CreateDirectoryFailed => "The selected folder could not be created.",
            Self::NotDirectory => "The selected path is not a folder.",
            Self::WriteProbeFailed => "The selected folder is not writable.",
            Self::WriteProbeCleanupFailed => "The selected folder could not be validated.",
            Self::DescriptorWriteFailed => "The data folder setting could not be saved.",
            Self::MigrationIncomplete => "Data migration could not be completed.",
            Self::OverrideActive => "The data folder is fixed by --user-dir for this session.",
            Self::Unsupported => "Changing the data folder is not supported on this platform.",
            Self::OpenFolderFailed => "The data folder could not be opened.",
        }
    }
}

/// A status value: code plus the offending path and OS error text.
///
/// Upstream stores a `std::error_code`; the message is rendered eagerly here so
/// the type stays `Send + Sync + Clone` without an OS error handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// What went wrong.
    pub code: ErrorCode,
    /// The path the operation touched, when meaningful.  Upstream reports at
    /// most one path per status.
    pub path: Option<PathBuf>,
    /// Rendered platform error text, when the failure came from the OS.
    pub system_error: Option<String>,
}

impl Status {
    /// Success.
    pub const fn ok() -> Self {
        Self {
            code: ErrorCode::None,
            path: None,
            system_error: None,
        }
    }

    /// A failure that carries one path.
    pub fn new(code: ErrorCode, path: impl Into<PathBuf>) -> Self {
        Self {
            code,
            path: Some(path.into()),
            system_error: None,
        }
    }

    /// A failure caused by the operating system.
    pub fn with_system(
        code: ErrorCode,
        path: impl Into<PathBuf>,
        system_error: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: Some(path.into()),
            system_error: Some(system_error.into()),
        }
    }

    /// `true` when no error was reported.
    pub fn is_ok(&self) -> bool {
        self.code == ErrorCode::None
    }

    /// The offending path, if known.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

impl fmt::Display for Status {
    /// Renders exactly like the upstream `status_message` helper: message, then
    /// ` (<path>)` when a path is known, then `: <system error>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code.message())?;
        if let Some(path) = self.path() {
            if !path.as_os_str().is_empty() {
                write!(f, " ({})", path.display())?;
            }
        }
        if let Some(system_error) = &self.system_error {
            write!(f, ": {system_error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Status {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_like_the_upstream_status_message_helper() {
        assert_eq!(Status::ok().to_string(), "");
        assert_eq!(
            Status::new(ErrorCode::EmptyPath, PathBuf::new()).to_string(),
            "Choose a folder."
        );
        assert_eq!(
            Status::with_system(
                ErrorCode::CreateDirectoryFailed,
                "/data/save/card_a",
                "denied"
            )
            .to_string(),
            "The selected folder could not be created. (/data/save/card_a): denied"
        );
    }

    #[test]
    fn success_is_the_none_code() {
        assert!(Status::ok().is_ok());
        assert_eq!(Status::ok().code, ErrorCode::None);
        assert!(!Status::new(ErrorCode::NotDirectory, "/tmp/x").is_ok());
    }
}
