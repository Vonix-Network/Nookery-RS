//! Bounded filesystem preparation for the data directory.
//!
//! Upstream `acgc::data::initialize` asks Borealis to resolve and migrate the
//! data directory, then runs `ensure_folders` (`pc/src/acgc_data.cpp`), which
//! creates each folder independently and only warns on failure.  This module
//! ports that bounded, non-fatal creation step: every required directory is
//! attempted, all failures are reported, and later directories are still
//! created after an earlier one fails.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::layout::DataLayout;
use crate::status::{ErrorCode, Status};

/// The result of preparing a data directory.
///
/// Failure is non-fatal by construction, matching the upstream code that logs a
/// warning and continues booting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Initialized {
    layout: DataLayout,
    failures: Vec<Status>,
}

impl Initialized {
    /// The prepared layout.
    pub fn layout(&self) -> &DataLayout {
        &self.layout
    }

    /// `true` when every required directory already existed or was created.
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    /// Failures in creation order; empty when [`Initialized::is_ok`].
    pub fn failures(&self) -> &[Status] {
        &self.failures
    }

    /// The first failure, or success.
    pub fn status(&self) -> Status {
        self.failures.first().cloned().unwrap_or_else(Status::ok)
    }
}

/// Create a directory and any missing parents.
///
/// Idempotent: an existing directory is success and is not modified.  An
/// existing non-directory is reported as [`ErrorCode::NotDirectory`] instead of
/// being replaced.
pub fn ensure_directory(path: &Path) -> Result<(), Status> {
    if path.as_os_str().is_empty() {
        // Unreachable upstream (`ensure_folder` is always called with literals);
        // reported instead of silently doing nothing.
        return Err(Status::new(ErrorCode::EmptyPath, path));
    }

    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(Status::new(ErrorCode::NotDirectory, path)),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            // `create_dir_all` is idempotent for directories and fails on a
            // file in place of any component, so its verdict is final.
            fs::create_dir_all(path).map_err(|error| {
                Status::with_system(ErrorCode::CreateDirectoryFailed, path, error.to_string())
            })
        }
        Err(error) => Err(Status::with_system(
            ErrorCode::CreateDirectoryFailed,
            path,
            error.to_string(),
        )),
    }
}

/// Create the required game directories plus the cache directory.
///
/// Only the fixed set from [`DataLayout::required_directories`] and
/// [`DataLayout::cache`] is touched.  Upstream additionally skips
/// `texture_pack`/`nes_roms` when a legacy copy exists next to the executable;
/// that check needs `base_path_relative` and therefore the Borealis runtime, so
/// it stays at the adapter boundary and this function always creates them.
pub fn initialize(layout: &DataLayout) -> Initialized {
    let mut failures = Vec::new();

    for directory in layout
        .required_directories()
        .into_iter()
        .chain(std::iter::once(layout.cache().to_path_buf()))
    {
        if let Err(status) = ensure_directory(&directory) {
            failures.push(status);
        }
    }

    Initialized {
        layout: layout.clone(),
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::path::PathBuf;

    fn required(layout: &DataLayout) -> Vec<PathBuf> {
        layout
            .required_directories()
            .into_iter()
            .chain(std::iter::once(layout.cache().to_path_buf()))
            .collect()
    }

    #[test]
    fn creates_every_required_directory() {
        let temp = TempDir::new("create");
        let layout = DataLayout::new(temp.path());

        let initialized = initialize(&layout);

        assert!(initialized.is_ok(), "{:?}", initialized.failures());
        for directory in required(&layout) {
            assert!(
                directory.is_dir(),
                "{} should be a directory",
                directory.display()
            );
        }
        assert!(temp.path().join("save/card_a").is_dir());
        assert!(temp.path().join("save/card_b").is_dir());
    }

    #[test]
    fn second_initialization_is_idempotent_and_leaves_content_alone() {
        let temp = TempDir::new("idempotent");
        let layout = DataLayout::new(temp.path());
        assert!(initialize(&layout).is_ok());

        let marker = layout.save_card_a().join("town.gci");
        fs::write(&marker, b"save").expect("write marker");

        let second = initialize(&layout);

        assert!(second.is_ok(), "{:?}", second.failures());
        assert_eq!(fs::read(&marker).expect("read marker"), b"save");
        assert_eq!(fs::read_dir(temp.path()).expect("read root").count(), 4);
    }

    #[test]
    fn reports_a_file_where_a_directory_is_required() {
        let temp = TempDir::new("file-blocker");
        let layout = DataLayout::new(temp.path());
        fs::create_dir_all(layout.save_card_a().parent().expect("save dir")).expect("create save");
        fs::write(layout.save_card_a(), b"not a directory").expect("write blocker");

        let initialized = initialize(&layout);

        assert!(!initialized.is_ok());
        let failure = initialized.status();
        assert_eq!(failure.code, ErrorCode::NotDirectory);
        assert_eq!(failure.path(), Some(layout.save_card_a().as_path()));
        // The blocker is preserved, and the remaining directories are still made.
        assert_eq!(
            fs::read(layout.save_card_a()).expect("blocker intact"),
            b"not a directory"
        );
        assert!(layout.save_card_b().is_dir());
        assert!(layout.cache().is_dir());
        assert_eq!(initialized.failures().len(), 1);
    }

    #[test]
    fn rejects_empty_directories_without_touching_the_filesystem() {
        let status = ensure_directory(Path::new("")).unwrap_err();
        assert_eq!(status.code, ErrorCode::EmptyPath);
        assert_eq!(status.path(), Some(Path::new("")));
    }

    #[test]
    fn reports_the_os_error_when_creation_is_impossible() {
        let temp = TempDir::new("under-file");
        let blocker = temp.path().join("blocker");
        fs::write(&blocker, b"x").expect("write blocker");
        let nested = blocker.join("child");

        let status = ensure_directory(&nested).unwrap_err();

        assert_eq!(status.code, ErrorCode::CreateDirectoryFailed);
        assert_eq!(status.path(), Some(nested.as_path()));
        assert!(status.system_error.is_some());
    }
}
