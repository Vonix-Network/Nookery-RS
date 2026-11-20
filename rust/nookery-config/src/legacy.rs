//! Legacy INI-style import, mirroring `import_legacy_file` /
//! `set_legacy_value` in `pc/src/acgc_settings.cpp`.
//!
//! The parser is pure: it takes the file text and returns the applied settings
//! plus how many lines were recognized. Reading the file, choosing candidates,
//! and logging stay outside this crate.

use crate::settings::Settings;

/// Result of one legacy import pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImport {
    /// Settings after overlaying every recognized line onto the defaults.
    pub settings: Settings,
    /// Number of recognized `key=value` lines, as upstream logs it.
    pub imported: usize,
}

impl LegacyImport {
    /// Upstream requires at least one recognized value before treating the file
    /// as a successful import.
    pub fn is_effective(&self) -> bool {
        self.imported > 0
    }
}

/// Parses legacy settings text.
///
/// Comments (`#`, `;`), blank lines, section headers (`[...]`), lines without
/// `=`, malformed integers, and unrecognized keys are all ignored; recognized
/// keys are then applied with the legacy rules. Never fails: a file with nothing
/// recognizable yields `imported == 0` and default settings.
pub fn parse_legacy_settings(text: &str) -> LegacyImport {
    let mut settings = Settings::default();
    let mut imported = 0usize;

    for line in text.lines() {
        let stripped = trim(line);
        if stripped.is_empty() || matches!(stripped.as_bytes().first(), Some(b'#' | b';' | b'[')) {
            continue;
        }
        let Some((key, raw)) = stripped.split_once('=') else {
            continue;
        };
        let key = trim(key);
        let Some(value) = parse_integer(trim(raw)) else {
            continue;
        };
        if apply_legacy_value(&mut settings, key, value) {
            imported += 1;
        }
    }

    LegacyImport { settings, imported }
}

/// Applies one legacy `key`/integer pair.
///
/// Returns `false` when the key is unknown or the value fails the legacy rule;
/// matches `set_legacy_value` exactly, including its asymmetric bounds:
/// `window_width`/`window_height` only have a lower bound here, so a huge value
/// is imported and must be repaired by [`Settings::sanitize`].
pub fn apply_legacy_value(settings: &mut Settings, key: &str, value: i64) -> bool {
    match key {
        "window_width" if value >= 640 => settings.video.window_width = value,
        "window_height" if value >= 480 => settings.video.window_height = value,
        "fullscreen" if (0..=2).contains(&value) => settings.video.window_mode = value,
        "vsync" if matches!(value, 0 | 1) => settings.video.vsync = value != 0,
        "max_fps" if (0..=1000).contains(&value) => settings.video.max_frame_rate = value,
        "preload_textures" if (0..=2).contains(&value) => {
            settings.enhancements.preload_textures = value
        }
        "disable_resetti" if matches!(value, 0 | 1) => {
            settings.enhancements.disable_resetti = value != 0
        }
        "disable_shop_visitor_req" if matches!(value, 0 | 1) => {
            settings.enhancements.disable_shop_visitor_requirement = value != 0;
        }
        "borderless_acres" if matches!(value, 0 | 1) => {
            settings.enhancements.borderless_acres = value != 0
        }
        "nes_aspect" if matches!(value, 0 | 1) => settings.enhancements.pillarbox_nes = value != 0,
        "master_volume" if (0..=100).contains(&value) => settings.audio.master_volume = value,
        "stick_deadzone" if (0..=40).contains(&value) => settings.input.stick_deadzone = value,
        "cstick_deadzone" if (0..=40).contains(&value) => settings.input.cstick_deadzone = value,
        _ => return false,
    }
    true
}

/// `std::from_chars` equivalent for `int` over an already-trimmed string: an
/// optional sign, at least one digit, no trailing characters, no overflow.
fn parse_integer(value: &str) -> Option<i64> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse::<i64>().ok()
}

/// ASCII trim, matching the C++ `trim` helper closely enough: the legacy file is
/// ASCII, and non-ASCII surrounding text is left untouched by every key rule.
fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_ascii_whitespace())
}
