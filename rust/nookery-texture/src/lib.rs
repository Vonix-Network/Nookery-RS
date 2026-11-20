//! Bounded texture-pack path selection for Nookery-RS.
//!
//! This crate ports only the source-verifiable path-selection seam from
//! `pc/include/acgc/texture_replacements.hpp:6-12` and
//! `pc/src/acgc_texture_replacements.cpp:27-51`.
//!
//! The active texture-pack directory is `<active_data_path>/texture_pack`.
//! When that path is not a directory, selection falls back in exact order to
//! `<cwd>/texture_pack`, then `<base_path_relative("texture_pack")>`. If no
//! candidate is a directory, the primary path is returned.
//!
//! Aurora texture registration, decoding, GPU integration, and reload
//! semantics are explicitly unported. Those behaviors depend on
//! `extern/aurora` and are outside this crate's bounded contract.

use std::path::{Path, PathBuf};

/// Resolves the active texture-pack directory according to the bounded C++
/// path-selection seam.
///
/// `base_path` is the root used to model the C++
/// `base_path_relative("texture_pack")` call. Existence is intentionally not
/// enough for a fallback: each fallback candidate must be a directory.
///
/// The returned path is the primary path when no candidate directory exists.
#[must_use]
pub fn resolve_texture_pack_path(active_data_path: &Path, cwd: &Path, base_path: &Path) -> PathBuf {
    let primary = active_data_path.join("texture_pack");
    if primary.is_dir() {
        return primary;
    }

    let cwd_fallback = cwd.join("texture_pack");
    if cwd_fallback.is_dir() {
        return cwd_fallback;
    }

    let base_fallback = base_path.join("texture_pack");
    if base_fallback.is_dir() {
        return base_fallback;
    }

    primary
}

#[cfg(test)]
mod tests {
    use super::resolve_texture_pack_path;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "nookery-texture-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create temporary test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn create_texture_pack(&self) -> PathBuf {
            let path = self.path().join("texture_pack");
            fs::create_dir(&path).expect("create texture-pack directory");
            path
        }

        fn create_file(&self, name: &str) -> PathBuf {
            let path = self.path().join(name);
            fs::write(&path, b"not a directory").expect("create test file");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn selects_primary_directory() {
        let active = TempDir::new("primary");
        let cwd = TempDir::new("primary-cwd");
        let base = TempDir::new("primary-base");
        let primary = active.create_texture_pack();

        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            primary
        );
    }

    #[test]
    fn falls_back_to_cwd_when_primary_is_missing() {
        let active = TempDir::new("cwd-primary");
        let cwd = TempDir::new("cwd");
        let base = TempDir::new("cwd-base");
        let cwd_fallback = cwd.create_texture_pack();

        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            cwd_fallback
        );
    }

    #[test]
    fn falls_back_to_base_relative_when_primary_and_cwd_are_missing() {
        let active = TempDir::new("base-primary");
        let cwd = TempDir::new("base-cwd");
        let base = TempDir::new("base");
        let base_fallback = base.create_texture_pack();

        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            base_fallback
        );
    }

    #[test]
    fn preserves_exact_primary_cwd_base_precedence() {
        let active = TempDir::new("precedence-primary");
        let cwd = TempDir::new("precedence-cwd");
        let base = TempDir::new("precedence-base");
        let primary = active.create_texture_pack();
        let _cwd_fallback = cwd.create_texture_pack();
        let _base_fallback = base.create_texture_pack();

        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            primary
        );

        fs::remove_dir(&primary).expect("remove primary texture-pack directory");
        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            cwd.path().join("texture_pack")
        );

        fs::remove_dir(cwd.path().join("texture_pack")).expect("remove cwd texture-pack directory");
        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            base.path().join("texture_pack")
        );
    }

    #[test]
    fn returns_primary_when_all_directories_are_missing() {
        let active = TempDir::new("missing-primary");
        let cwd = TempDir::new("missing-cwd");
        let base = TempDir::new("missing-base");
        let expected = active.path().join("texture_pack");

        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            expected
        );
    }

    #[test]
    fn ignores_non_directory_candidates() {
        let active = TempDir::new("file-primary");
        let cwd = TempDir::new("file-cwd");
        let base = TempDir::new("file-base");
        let primary = active.create_file("texture_pack");
        let cwd_file = cwd.create_file("texture_pack");
        let base_fallback = base.create_texture_pack();

        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            base_fallback
        );

        fs::remove_dir(&base_fallback).expect("remove base texture-pack directory");
        assert_eq!(
            resolve_texture_pack_path(active.path(), cwd.path(), base.path()),
            active.path().join("texture_pack")
        );
        assert!(primary.is_file());
        assert!(cwd_file.is_file());
    }
}
