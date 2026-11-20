//! Path layout and relative-path resolution.
//!
//! Mirrors `acgc::data::initialize`'s observable directory set
//! (`pc/src/acgc_data.cpp`, `ensure_folders`) and the two path-facing C entry
//! points `pc_platform_user_path` / `pc_platform_base_path`.

use std::path::{Component, Path, PathBuf};

use crate::status::{ErrorCode, Status};

/// Why a relative path was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathError {
    /// The caller passed an empty path.
    Empty,
    /// The path was absolute and would replace the data root.
    Absolute,
    /// The path contained a `..` component.
    Traversal,
    /// The path contained a platform root or prefix component.
    Rooted,
}

impl PathError {
    /// The status code this rejection reports.
    pub const fn code(self) -> ErrorCode {
        match self {
            // An unusable selection is reported as an empty/invalid selection.
            Self::Empty | Self::Absolute | Self::Traversal | Self::Rooted => ErrorCode::EmptyPath,
        }
    }
}

/// The directories the game expects below its active data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLayout {
    root: PathBuf,
    cache: PathBuf,
}

impl DataLayout {
    /// Root the layout at `root`, with the cache directory directly below it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let cache = root.join("cache");
        Self { root, cache }
    }

    /// Root with an explicit cache directory, for platforms that keep caches
    /// outside the user directory (the upstream `Paths::cachePath` contract).
    pub fn with_cache(root: impl Into<PathBuf>, cache: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cache: cache.into(),
        }
    }

    /// The active data directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The cache directory reported back to the log/Aurora configuration.
    pub fn cache(&self) -> &Path {
        &self.cache
    }

    pub fn save_card_a(&self) -> PathBuf {
        self.root.join("save/card_a")
    }

    pub fn save_card_b(&self) -> PathBuf {
        self.root.join("save/card_b")
    }

    pub fn texture_pack(&self) -> PathBuf {
        self.root.join("texture_pack")
    }

    pub fn nes_roms(&self) -> PathBuf {
        self.root.join("nes_roms")
    }

    /// The directories created unconditionally on initialization.
    pub fn required_directories(&self) -> [PathBuf; 4] {
        [
            self.save_card_a(),
            self.save_card_b(),
            self.texture_pack(),
            self.nes_roms(),
        ]
    }

    /// Resolve a relative user-data path under the root.
    ///
    /// Rejects empty, absolute, rooted and `..`-containing paths.  Upstream's
    /// `pc_platform_user_path` joins blindly, so `std::filesystem::path`'s
    /// absolute-operand rule would silently escape the data root; refusing is a
    /// deliberate hardening, not parity.
    pub fn relative(&self, relative: impl AsRef<Path>) -> Result<PathBuf, PathError> {
        let relative = relative.as_ref();
        if relative.as_os_str().is_empty() {
            return Err(PathError::Empty);
        }
        if relative.is_absolute() {
            return Err(PathError::Absolute);
        }
        for component in relative.components() {
            match component {
                Component::ParentDir => return Err(PathError::Traversal),
                Component::RootDir | Component::Prefix(_) => return Err(PathError::Rooted),
                Component::CurDir | Component::Normal(_) => {}
            }
        }
        Ok(self.root.join(relative))
    }

    /// Like [`DataLayout::relative`], but returns the upstream-shaped status
    /// and names the offending input rather than the root.
    pub fn resolve(&self, relative: impl AsRef<Path>) -> Result<PathBuf, Status> {
        let relative = relative.as_ref();
        self.relative(relative)
            .map_err(|error| Status::new(error.code(), relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_save_cards_and_packs_below_the_root() {
        let layout = DataLayout::new("/data");
        assert_eq!(layout.save_card_a(), Path::new("/data/save/card_a"));
        assert_eq!(layout.save_card_b(), Path::new("/data/save/card_b"));
        assert_eq!(layout.texture_pack(), Path::new("/data/texture_pack"));
        assert_eq!(layout.nes_roms(), Path::new("/data/nes_roms"));
        assert_eq!(layout.cache(), Path::new("/data/cache"));
        assert_eq!(layout.required_directories().len(), 4);
    }

    #[test]
    fn honours_an_out_of_tree_cache_directory() {
        let layout = DataLayout::with_cache("/data", "/var/cache/nookery");
        assert_eq!(layout.cache(), Path::new("/var/cache/nookery"));
        assert_eq!(layout.root(), Path::new("/data"));
    }

    #[test]
    fn resolves_plain_relative_paths() {
        let layout = DataLayout::new("/data");
        assert_eq!(
            layout.relative("save/card_a/town.gci"),
            Ok(PathBuf::from("/data/save/card_a/town.gci"))
        );
        assert_eq!(
            layout.relative("./nes_roms/game.nes"),
            Ok(PathBuf::from("/data/./nes_roms/game.nes"))
        );
    }

    #[test]
    fn refuses_escape_and_absolute_paths() {
        let layout = DataLayout::new("/data");
        assert_eq!(layout.relative("../outside"), Err(PathError::Traversal));
        assert_eq!(
            layout.relative("save/../../outside"),
            Err(PathError::Traversal)
        );
        assert_eq!(layout.relative("/etc/passwd"), Err(PathError::Absolute));
        assert_eq!(layout.relative(""), Err(PathError::Empty));
        assert_eq!(
            layout.resolve("/etc/passwd").unwrap_err().code,
            ErrorCode::EmptyPath
        );
    }
}
