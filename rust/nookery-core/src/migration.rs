//! The migration contract declared by the upstream port.
//!
//! Source of truth: `acgc::data::migration_rules()` in `pc/src/acgc_data.cpp`.
//! The lists are reproduced verbatim; the copying policy belongs to Borealis
//! (`borealis::data::Manager`), which is not vendored at this pin, so this
//! module only *plans* candidates and never mutates the filesystem.

use std::fs;
use std::path::{Path, PathBuf};

/// The on-disk names the upstream port migrates from a legacy install.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct MigrationRules;

impl MigrationRules {
    /// Directory names copied wholesale into the data directory.
    pub const DIRECTORIES: &'static [&'static str] = &["save", "texture_pack", "nes_roms"];

    /// Individual files copied into the data directory.
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

    /// File extensions migrated from the legacy root, leading dot included.
    pub const EXTENSIONS: &'static [&'static str] = &[".controller", ".gci"];
}

/// Legacy entries that are absent from the active data directory.
///
/// Paths point into `source`; the executor copies them into `target`.  `target`
/// itself is never modified here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationPlan {
    /// Directories to copy, in `MigrationRules::DIRECTORIES` order.
    pub directories: Vec<PathBuf>,
    /// Files to copy: named files first, then extension matches by name.
    pub files: Vec<PathBuf>,
}

impl MigrationPlan {
    /// `true` when nothing needs copying.
    pub fn is_empty(&self) -> bool {
        self.directories.is_empty() && self.files.is_empty()
    }
}

/// List legacy entries that `target` does not already contain.
///
/// A candidate is reported only when the source entry exists with the expected
/// type and the same-named entry is absent from `target`; an existing entry of
/// any type wins over migration, matching the upstream intent of never
/// clobbering a live data directory.
///
/// Assumption (Borealis unavailable at this pin): extension matching is a
/// byte-exact, case-sensitive comparison against the last file extension.
pub fn plan(source: &Path, target: &Path) -> MigrationPlan {
    let mut plan = MigrationPlan::default();

    for name in MigrationRules::DIRECTORIES {
        let candidate = source.join(name);
        if candidate.is_dir() && !target.join(name).exists() {
            plan.directories.push(candidate);
        }
    }

    for name in MigrationRules::FILES {
        let candidate = source.join(name);
        if candidate.is_file() && !target.join(name).exists() {
            plan.files.push(candidate);
        }
    }

    let Ok(entries) = fs::read_dir(source) else {
        return plan;
    };

    // `read_dir` order is unspecified; sort so plans are reproducible.
    let mut matched: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    MigrationRules::EXTENSIONS
                        .iter()
                        .any(|known| known.strip_prefix('.') == Some(extension))
                })
        })
        .filter(|path| match path.file_name() {
            Some(name) => !target.join(name).exists(),
            None => false,
        })
        .collect();
    matched.sort();
    plan.files.extend(matched);

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs;

    #[test]
    fn pins_the_upstream_rule_lists() {
        assert_eq!(
            MigrationRules::DIRECTORIES,
            ["save", "texture_pack", "nes_roms"]
        );
        assert_eq!(
            MigrationRules::FILES,
            [
                "config.json",
                "config.json.bad",
                "settings.ini",
                "keybindings.ini",
                "controller_ports.dat",
                "gamecontrollerdb.txt",
                "imgui.ini",
                "keyboard_bindings.dat",
                "input.json",
            ]
        );
        assert_eq!(MigrationRules::EXTENSIONS, [".controller", ".gci"]);
    }

    #[test]
    fn plans_only_entries_absent_from_the_target() {
        let source = TempDir::new("legacy");
        let target = TempDir::new("active");

        fs::create_dir_all(source.path().join("save")).expect("legacy save");
        fs::create_dir_all(source.path().join("nes_roms")).expect("legacy nes");
        fs::write(source.path().join("config.json"), b"{}").expect("legacy config");
        fs::write(source.path().join("settings.ini"), b"x").expect("legacy settings");
        fs::write(source.path().join("town.gci"), b"gci").expect("legacy gci");
        fs::write(source.path().join("pad.controller"), b"pad").expect("legacy controller");
        fs::write(source.path().join("save.dat"), b"ignored").expect("ignored file");

        fs::create_dir_all(target.path().join("save")).expect("active save");
        fs::write(target.path().join("settings.ini"), b"keep").expect("active settings");

        let planned = plan(source.path(), target.path());

        assert_eq!(
            planned.directories,
            vec![source.path().join("nes_roms")],
            "`save` already exists in the target and must not be re-copied"
        );
        // Named files come first in rule order, then extension matches sorted by name.
        assert_eq!(
            planned.files,
            vec![
                source.path().join("config.json"),
                source.path().join("pad.controller"),
                source.path().join("town.gci"),
            ]
        );
        assert!(!planned.is_empty());
    }

    #[test]
    fn ignores_extension_matches_that_are_not_files_or_already_present() {
        let source = TempDir::new("legacy-edge");
        let target = TempDir::new("active-edge");

        // A directory named like a match, and an unrelated extension.
        fs::create_dir_all(source.path().join("fake.gci")).expect("dir named .gci");
        fs::write(source.path().join("notes.txt"), b"n").expect("unrelated file");
        fs::write(source.path().join("taken.gci"), b"n").expect("present in target");
        fs::write(target.path().join("taken.gci"), b"target").expect("target copy");

        let planned = plan(source.path(), target.path());

        assert!(planned.directories.is_empty());
        assert!(planned.files.is_empty(), "{:?}", planned.files);
        assert!(planned.is_empty());
    }

    #[test]
    fn empty_source_yields_no_work() {
        let source = TempDir::new("legacy-empty");
        let target = TempDir::new("active-empty");

        let planned = plan(source.path(), target.path());

        assert!(planned.is_empty());
        assert!(plan(&source.path().join("missing"), target.path()).is_empty());
    }
}
