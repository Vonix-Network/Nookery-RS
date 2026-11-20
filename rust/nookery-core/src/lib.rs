//! Behavior-preserving Rust primitives for the Nookery port.
//!
//! This crate is intentionally dependency-free at the bootstrap stage.  The
//! first slice mirrors the data-folder contract in `pc/src/acgc_data.cpp` and
//! keeps policy separate from platform-specific I/O so later platform ports
//! can share the same allocation-light path logic.

use std::path::{Path, PathBuf};

/// The on-disk directories and files migrated by the original port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationRules;

impl MigrationRules {
    pub const DIRECTORIES: &'static [&'static str] = &["save", "texture_pack", "nes_roms"];
    pub const FILES: &'static [&'static str] = &[
        "config.json",
        "config.json.bad",
        "settings.ini",
        "keybindings.ini",
        "controller_ports.dat",
        "gamecontrollerdb.txt",
        "imgui.ini",
        "keyboard_bindings.dat",
        "input.json",
    ];
    pub const EXTENSIONS: &'static [&'static str] = &[".controller", ".gci"];
}

/// The paths that the game expects below its active data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLayout {
    root: PathBuf,
}

impl DataLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
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

    /// Returns the fixed set of directories created during initialization.
    pub fn required_directories(&self) -> [PathBuf; 5] {
        [
            self.save_card_a(),
            self.save_card_b(),
            self.texture_pack(),
            self.nes_roms(),
            self.cache(),
        ]
    }

    /// Resolve a relative user-data path without allocating intermediate
    /// strings or accepting an absolute path that escapes the data root.
    pub fn relative(&self, relative: impl AsRef<Path>) -> Option<PathBuf> {
        let relative = relative.as_ref();
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        {
            return None;
        }
        Some(self.root.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_upstream_migration_contract() {
        assert_eq!(
            MigrationRules::DIRECTORIES,
            ["save", "texture_pack", "nes_roms"]
        );
        assert!(MigrationRules::FILES.contains(&"config.json"));
        assert!(MigrationRules::FILES.contains(&"input.json"));
        assert_eq!(MigrationRules::EXTENSIONS, [".controller", ".gci"]);
    }

    #[test]
    fn exposes_expected_layout_without_string_round_trips() {
        let layout = DataLayout::new("/tmp/nookery");
        assert_eq!(
            layout.save_card_a(),
            PathBuf::from("/tmp/nookery/save/card_a")
        );
        assert_eq!(
            layout.save_card_b(),
            PathBuf::from("/tmp/nookery/save/card_b")
        );
        assert_eq!(layout.required_directories().len(), 5);
    }

    #[test]
    fn rejects_escape_paths() {
        let layout = DataLayout::new("/tmp/nookery");
        assert!(layout.relative("../outside").is_none());
        assert!(layout.relative("/outside").is_none());
        assert_eq!(
            layout.relative("save/card_a/town.gci"),
            Some(PathBuf::from("/tmp/nookery/save/card_a/town.gci"))
        );
    }
}
