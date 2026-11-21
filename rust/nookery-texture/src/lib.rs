//! Bounded texture-pack path selection for Nookery-RS.
//!
//! This crate ports the source-verifiable path-selection seam from
//! `pc/include/acgc/texture_replacements.hpp:6-12` and
//! `pc/src/acgc_texture_replacements.cpp:27-51`, plus bounded replacement
//! metadata indexing.
//!
//! The active texture-pack directory is `<active_data_path>/texture_pack`.
//! When that path is not a directory, selection falls back in exact order to
//! `<cwd>/texture_pack`, then `<base_path_relative("texture_pack")>`. If no
//! candidate is a directory, the primary path is returned.
//!
//! Aurora texture registration, decoding, GPU integration, and reload
//! semantics are explicitly unported. [`index_texture_pack`] stops at fixed
//! PNG/DDS headers and returns registration metadata; those behaviors depend
//! on `extern/aurora` and are outside this crate's bounded contract.

mod index;

use std::path::{Path, PathBuf};

pub use index::{
    MAX_INDEXED_ENTRIES, TextureFormat, TextureIndex, TextureIndexError, TextureIndexStats,
    TextureRegistration, index_texture_pack,
};

/// Which source root won the C++ texture-pack precedence chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturePackOrigin {
    /// <active_data_path>/texture_pack.
    ActiveData,
    /// `cwd/texture_pack`, the pre-Aurora fallback.
    CurrentDirectory,
    /// <base_path_relative("texture_pack")>, the legacy fallback.
    BasePath,
    /// No candidate was a directory; the primary path is retained for the
    /// same diagnostic/unload behavior as the C++ implementation.
    Unavailable,
}

/// The selected path and its source before metadata indexing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TexturePackSelection {
    path: PathBuf,
    origin: TexturePackOrigin,
}

impl TexturePackSelection {
    /// Selected directory path, or the unavailable primary path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Precedence source that selected the path.
    #[must_use]
    pub const fn origin(&self) -> TexturePackOrigin {
        self.origin
    }

    /// Whether the selected path was an existing directory.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        !matches!(self.origin, TexturePackOrigin::Unavailable)
    }
}

/// A selected texture-pack root with bounded replacement metadata.
///
/// This is the production handoff between path selection/indexing and the
/// parent-owned Aurora hook. The index contains paths and fixed-header
/// metadata only; it never decodes or retains texture payloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TexturePack {
    selection: TexturePackSelection,
    index: TextureIndex,
}

impl TexturePack {
    /// Selected directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.selection.path()
    }

    /// Source selected by the C++ precedence chain.
    #[must_use]
    pub const fn origin(&self) -> TexturePackOrigin {
        self.selection.origin
    }

    /// Bounded replacement metadata in deterministic key order.
    #[must_use]
    pub const fn index(&self) -> &TextureIndex {
        &self.index
    }

    /// Lookup a normalized replacement key.
    #[must_use]
    pub fn lookup(&self, key: &str) -> Option<&TextureRegistration> {
        self.index.lookup(key)
    }
}

/// Selects the active texture-pack root while preserving the C++ precedence
/// and the primary-path fallback when no directory exists.
#[must_use]
pub fn select_texture_pack(
    active_data_path: &Path,
    cwd: &Path,
    base_path: &Path,
) -> TexturePackSelection {
    let primary = active_data_path.join("texture_pack");
    if is_real_directory(&primary) {
        return TexturePackSelection {
            path: primary,
            origin: TexturePackOrigin::ActiveData,
        };
    }

    let cwd_fallback = cwd.join("texture_pack");
    if is_real_directory(&cwd_fallback) {
        return TexturePackSelection {
            path: cwd_fallback,
            origin: TexturePackOrigin::CurrentDirectory,
        };
    }

    let base_fallback = base_path.join("texture_pack");
    if is_real_directory(&base_fallback) {
        return TexturePackSelection {
            path: base_fallback,
            origin: TexturePackOrigin::BasePath,
        };
    }

    TexturePackSelection {
        path: primary,
        origin: TexturePackOrigin::Unavailable,
    }
}

/// Selects and indexes the active texture pack in one production call.
///
/// TextureIndexError::RootNotDirectory is preserved for the C++ reload branch
/// where the selected primary path is unavailable; the parent can use that
/// result to perform Aurora's unregister hook.
pub fn discover_texture_pack(
    active_data_path: &Path,
    cwd: &Path,
    base_path: &Path,
) -> Result<TexturePack, TextureIndexError> {
    let selection = select_texture_pack(active_data_path, cwd, base_path);
    let index = index_texture_pack(selection.path())?;
    Ok(TexturePack { selection, index })
}

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
    select_texture_pack(active_data_path, cwd, base_path).path
}

fn is_real_directory(path: &Path) -> bool {
    if has_symlink_component(path) {
        return false;
    }
    std::fs::symlink_metadata(path)
        .map(|metadata| !metadata.file_type().is_symlink() && metadata.is_dir())
        .unwrap_or(false)
}

fn has_symlink_component(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if std::fs::symlink_metadata(&current)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        TexturePackOrigin, discover_texture_pack, resolve_texture_pack_path, select_texture_pack,
    };
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

        fn write_png(&self, relative: &str) -> PathBuf {
            let path = self.path().join(relative);
            let parent = path.parent().expect("fixture has a parent");
            fs::create_dir_all(parent).expect("create fixture parent");
            let mut value = [0u8; 24];
            value[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
            value[8..12].copy_from_slice(&13u32.to_be_bytes());
            value[12..16].copy_from_slice(b"IHDR");
            value[16..20].copy_from_slice(&1u32.to_be_bytes());
            value[20..24].copy_from_slice(&1u32.to_be_bytes());
            fs::write(&path, value).expect("write PNG fixture");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // Differential vector transcribed from
    // pc/src/acgc_texture_replacements.cpp::directory:
    // primary, then current_path, then base_path_relative, otherwise primary.
    const SOURCE_ORACLE_PRECEDENCE: &[(bool, bool, bool, TexturePackOrigin)] = &[
        (false, false, false, TexturePackOrigin::Unavailable),
        (false, false, true, TexturePackOrigin::BasePath),
        (false, true, false, TexturePackOrigin::CurrentDirectory),
        (false, true, true, TexturePackOrigin::CurrentDirectory),
        (true, false, false, TexturePackOrigin::ActiveData),
        (true, false, true, TexturePackOrigin::ActiveData),
        (true, true, false, TexturePackOrigin::ActiveData),
        (true, true, true, TexturePackOrigin::ActiveData),
    ];

    #[test]
    fn source_oracle_precedence_vector_is_exhaustive() {
        for &(active_exists, cwd_exists, base_exists, expected_origin) in SOURCE_ORACLE_PRECEDENCE {
            let active = TempDir::new("oracle-active");
            let cwd = TempDir::new("oracle-cwd");
            let base = TempDir::new("oracle-base");
            if active_exists {
                active.create_texture_pack();
            }
            if cwd_exists {
                cwd.create_texture_pack();
            }
            if base_exists {
                base.create_texture_pack();
            }

            let selection = select_texture_pack(active.path(), cwd.path(), base.path());
            assert_eq!(
                selection.origin(),
                expected_origin,
                "active={active_exists} cwd={cwd_exists} base={base_exists}"
            );
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

    #[test]
    fn selection_reports_source_precedence_and_discovery_indexes_it() {
        let active = TempDir::new("discover-active");
        let cwd = TempDir::new("discover-cwd");
        let base = TempDir::new("discover-base");
        let primary = active.create_texture_pack();
        let _cwd_fallback = cwd.create_texture_pack();
        let _base_fallback = base.create_texture_pack();
        active.write_png("texture_pack/ui/icon.png");

        let selection = select_texture_pack(active.path(), cwd.path(), base.path());
        assert_eq!(selection.origin(), TexturePackOrigin::ActiveData);
        assert_eq!(selection.path(), primary);
        assert!(selection.is_available());

        let pack =
            discover_texture_pack(active.path(), cwd.path(), base.path()).expect("discover pack");
        assert_eq!(pack.origin(), TexturePackOrigin::ActiveData);
        assert_eq!(pack.path(), primary);
        assert_eq!(
            pack.lookup("UI\\ICON.PNG").map(|entry| entry.key.as_str()),
            Some("ui/icon.png")
        );
    }

    #[test]
    fn unavailable_selection_remains_primary_and_is_distinct_from_fallbacks() {
        let active = TempDir::new("discover-missing-active");
        let cwd = TempDir::new("discover-missing-cwd");
        let base = TempDir::new("discover-missing-base");
        let selection = select_texture_pack(active.path(), cwd.path(), base.path());

        assert_eq!(selection.origin(), TexturePackOrigin::Unavailable);
        assert!(!selection.is_available());
        assert_eq!(selection.path(), active.path().join("texture_pack"));
        assert_eq!(
            discover_texture_pack(active.path(), cwd.path(), base.path()),
            Err(super::TextureIndexError::RootNotDirectory)
        );
    }
}
