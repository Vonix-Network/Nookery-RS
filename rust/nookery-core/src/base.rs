//! The executable/base-directory boundary.
//!
//! Upstream asks Borealis for this (`acgc::data::base_path_relative` →
//! `borealis::data::Manager::base_path_relative`) and re-exports it through
//! `pc_platform_base_path`.  Resolving the directory itself is platform work
//! (Windows executable dir, macOS bundle, Linux `/proc/self/exe`, Android APK),
//! and Borealis is not vendored at this pin, so the Rust side takes the
//! directory as an explicit input and models absence as a first-class state.

use std::path::{Component, Path, PathBuf};

use crate::status::{ErrorCode, Status};

/// Where the base (read-only install) directory lives, if anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasePath {
    /// A known absolute base directory.
    Directory(PathBuf),
    /// Not resolvable in this build: no `base_path_relative` results exist.
    Unavailable,
}

impl BasePath {
    /// Wrap a directory, rejecting relative or empty inputs.
    ///
    /// Upstream guarantees an absolute result: `pc_platform_base_path` reports
    /// failure when `base_path_relative` returns an empty or relative path, so a
    /// relative value is a caller bug rather than a usable answer.
    pub fn new(directory: impl Into<PathBuf>) -> Result<Self, Status> {
        let directory = directory.into();
        if !directory.is_absolute() {
            return Err(Status::new(
                ErrorCode::PreferencePathUnavailable,
                directory.as_path(),
            ));
        }
        Ok(Self::Directory(directory))
    }

    /// The directory, when known.
    pub fn directory(&self) -> Option<&Path> {
        match self {
            Self::Directory(directory) => Some(directory),
            Self::Unavailable => None,
        }
    }

    /// Resolve `relative` against the base directory.
    ///
    /// `Ok(None)` is the upstream "not available on this platform" answer
    /// (`pc_platform_base_path` returning 0); `Err` is a rejected path.
    pub fn relative(&self, relative: impl AsRef<Path>) -> Result<Option<PathBuf>, Status> {
        let relative = relative.as_ref();
        let Some(base) = self.directory() else {
            return Ok(None);
        };
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(Status::new(ErrorCode::EmptyPath, relative));
        }
        let resolved = base.join(relative);
        if !resolved.is_absolute() {
            return Ok(None);
        }
        Ok(Some(resolved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_base_has_no_resolution() {
        let base = BasePath::Unavailable;
        assert_eq!(base.directory(), None);
        assert_eq!(base.relative("texture_pack"), Ok(None));
    }

    #[test]
    fn resolves_below_a_known_base_directory() {
        let base = BasePath::new("/opt/nookery").unwrap();
        assert_eq!(base.directory(), Some(Path::new("/opt/nookery")));
        assert_eq!(
            base.relative("texture_pack"),
            Ok(Some(PathBuf::from("/opt/nookery/texture_pack")))
        );
    }

    #[test]
    fn rejects_relative_and_escaping_base_inputs() {
        assert_eq!(
            BasePath::new("opt/nookery").unwrap_err().code,
            ErrorCode::PreferencePathUnavailable
        );
        let base = BasePath::new("/opt/nookery").unwrap();
        assert!(base.relative("../nookery").is_err());
        assert!(base.relative("/opt/nookery").is_err());
    }
}
