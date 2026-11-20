//! Keyed settings model, ranges, validation, and sanitization.
//!
//! Mirrors `UserSettings` in `pc/src/acgc_settings.cpp`. Field names, defaults,
//! and ranges are taken from the registry declarations in that file; nothing
//! here talks to a filesystem, a UI, or Aurora.

use core::ops::RangeInclusive;

use crate::enums::{DiscVerificationState, InterfaceTheme, TravelMoveOutChance};

/// `video.windowWidth` bounds from the registry declaration.
pub const WINDOW_WIDTH: RangeInclusive<i64> = 320..=16384;
/// `video.windowHeight` bounds from the registry declaration.
pub const WINDOW_HEIGHT: RangeInclusive<i64> = 240..=16384;
/// `video.windowMode` bounds from the registry declaration.
pub const WINDOW_MODE: RangeInclusive<i64> = 0..=2;
/// `video.maxFrameRate` bounds from the registry declaration.
pub const MAX_FRAME_RATE: RangeInclusive<i64> = 0..=1000;
/// `enhancements.preloadTextures` bounds from the registry declaration.
pub const PRELOAD_TEXTURES: RangeInclusive<i64> = 0..=2;
/// `audio.masterVolume` bounds from the registry declaration.
pub const MASTER_VOLUME: RangeInclusive<i64> = 0..=100;
/// `input.stickDeadzone` bounds from the registry declaration.
pub const STICK_DEADZONE: RangeInclusive<i64> = 0..=40;
/// `input.cstickDeadzone` bounds from the registry declaration.
pub const CSTICK_DEADZONE: RangeInclusive<i64> = 0..=40;
/// `video.uiScale` lower bound once the user opts out of "automatic" (`0`).
pub const UI_SCALE: RangeInclusive<i64> = 50..=200;

/// `video.uiScale` default where the platform has window controls (desktop).
pub const DESKTOP_UI_SCALE_DEFAULT: i64 = 0;
/// `video.uiScale` default on platforms without window controls (mobile).
///
/// Runtime-dependent upstream (`ACGC_HAS_WINDOW_CONTROLS ? 0 : 75`). This crate
/// defaults to the desktop value and only documents the mobile one.
pub const MOBILE_UI_SCALE_DEFAULT: i64 = 75;

/// `video.renderer` value. Upstream stores an `AuroraBackend`; the numeric form
/// is codec-owned, so this model keeps the canonical string and leaves
/// availability to an adapter (see [`Settings::sanitize_renderer`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Renderer(String);

impl Renderer {
    /// The value Aurora reports for "pick something supported".
    pub const AUTO: &'static str = "auto";

    /// Builds a renderer name, enforcing the canonical lowercase form that the
    /// settings file and the enum codec use.
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("renderer name must not be empty");
        }
        if name.len() > 64 {
            return Err("renderer name is longer than 64 bytes");
        }
        if !name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        }) {
            return Err("renderer name must be lowercase ascii with '_', '-' or '.'");
        }
        Ok(Self(name.to_owned()))
    }

    /// [`Renderer::AUTO`].
    pub fn auto() -> Self {
        Self(Self::AUTO.to_owned())
    }

    /// Canonical string form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when the backend is left to the runtime.
    pub fn is_auto(&self) -> bool {
        self.0 == Self::AUTO
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::auto()
    }
}

impl core::fmt::Display for Renderer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl core::str::FromStr for Renderer {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// A settings value after the settings file has been parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Int(i64),
    Bool(bool),
    Text(String),
}

impl SettingValue {
    /// Integer payload, or `None` for another variant.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }

    /// Boolean payload, or `None` for another variant.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Text payload, or `None` for another variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }
}

/// Why a keyed write was refused.
///
/// Deliberately split: an unlisted key and an out-of-range value must never be
/// reported through the same variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    /// The key is not part of the settings schema.
    UnknownKey(String),
    /// The key exists and takes an integer, but the value is outside its range.
    OutOfRange {
        key: String,
        value: i64,
        min: i64,
        max: i64,
    },
    /// The key exists but the value type or encoding is not accepted.
    InvalidValue { key: String, reason: String },
}

impl core::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownKey(key) => write!(f, "unknown settings key '{key}'"),
            Self::OutOfRange {
                key,
                value,
                min,
                max,
            } => {
                write!(
                    f,
                    "settings key '{key}' value {value} is outside {min}..={max}"
                )
            }
            Self::InvalidValue { key, reason } => {
                write!(f, "settings key '{key}' rejected: {reason}")
            }
        }
    }
}

impl std::error::Error for SettingsError {}

impl SettingsError {
    fn invalid(key: &str, reason: impl Into<String>) -> Self {
        Self::InvalidValue {
            key: key.to_owned(),
            reason: reason.into(),
        }
    }

    fn range(key: &str, value: i64, range: &RangeInclusive<i64>) -> Self {
        Self::OutOfRange {
            key: key.to_owned(),
            value,
            min: *range.start(),
            max: *range.end(),
        }
    }
}

/// `backend.*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    /// `backend.discPath`; the registry applies no validation to this string.
    pub disc_path: String,
    /// `backend.discVerification`.
    pub disc_verification: DiscVerificationState,
    /// `backend.skipPrelaunch`.
    pub skip_prelaunch: bool,
    /// `backend.checkForUpdates`.
    pub check_for_updates: bool,
}

impl Default for Backend {
    fn default() -> Self {
        Self {
            disc_path: "game.rvz".to_owned(),
            disc_verification: DiscVerificationState::Unknown,
            skip_prelaunch: false,
            check_for_updates: true,
        }
    }
}

/// `video.*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    /// `video.renderer`.
    pub renderer: Renderer,
    /// `video.windowWidth`.
    pub window_width: i64,
    /// `video.windowHeight`.
    pub window_height: i64,
    /// `video.windowMode`.
    pub window_mode: i64,
    /// `video.uiScale`; `0` means automatic, otherwise [`UI_SCALE`].
    pub ui_scale: i64,
    /// `video.theme`.
    pub theme: InterfaceTheme,
    /// `video.vsync`.
    pub vsync: bool,
    /// `video.maxFrameRate`.
    pub max_frame_rate: i64,
    /// `video.hudAtEdges`.
    pub hud_at_edges: bool,
}

impl Default for Video {
    fn default() -> Self {
        Self {
            renderer: Renderer::auto(),
            window_width: 1280,
            window_height: 960,
            window_mode: 0,
            ui_scale: DESKTOP_UI_SCALE_DEFAULT,
            theme: InterfaceTheme::Dynamic,
            vsync: true,
            max_frame_rate: 60,
            hud_at_edges: false,
        }
    }
}

/// `enhancements.*` and `gameplay.*` flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enhancements {
    /// `enhancements.preloadTextures`.
    pub preload_textures: i64,
    /// `gameplay.disableResetti`.
    pub disable_resetti: bool,
    /// `gameplay.disableShopVisitorRequirement`.
    pub disable_shop_visitor_requirement: bool,
    /// `gameplay.borderlessAcres`.
    pub borderless_acres: bool,
    /// `gameplay.pillarboxNes`.
    pub pillarbox_nes: bool,
    /// `gameplay.fastText`.
    pub fast_text: bool,
    /// `gameplay.travelMoveOutChance`.
    pub travel_move_out_chance: TravelMoveOutChance,
}

impl Default for Enhancements {
    fn default() -> Self {
        Self {
            preload_textures: 0,
            disable_resetti: false,
            disable_shop_visitor_requirement: false,
            borderless_acres: true,
            pillarbox_nes: true,
            fast_text: true,
            travel_move_out_chance: TravelMoveOutChance::Default,
        }
    }
}

/// `audio.*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    /// `audio.masterVolume`.
    pub master_volume: i64,
}

impl Default for Audio {
    fn default() -> Self {
        Self { master_volume: 100 }
    }
}

/// `input.*`.
///
/// `input.touchLayout` is intentionally absent: it is a touch-layout struct with
/// no scalar form, and this crate only models the scalar settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// `input.stickDeadzone`.
    pub stick_deadzone: i64,
    /// `input.cstickDeadzone`.
    pub cstick_deadzone: i64,
    /// `input.autoAssignController`.
    pub auto_assign_controller: bool,
    /// `input.singlePortMigrated`.
    pub single_port_migrated: bool,
    /// `input.allowBackgroundInput`.
    pub allow_background_input: bool,
    /// `input.touchControls`; runtime-dependent upstream, desktop default `false`.
    pub touch_controls: bool,
    /// `input.touchHaptics`.
    pub touch_haptics: bool,
    /// `input.keyboardTyping`.
    pub keyboard_typing: bool,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            stick_deadzone: 12,
            cstick_deadzone: 12,
            auto_assign_controller: true,
            single_port_migrated: false,
            allow_background_input: true,
            touch_controls: false,
            touch_haptics: true,
            keyboard_typing: false,
        }
    }
}

/// Full settings model. [`Settings::default`] equals the upstream registry
/// defaults for a desktop build.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// `backend.*`.
    pub backend: Backend,
    /// `video.*`.
    pub video: Video,
    /// `enhancements.*` / `gameplay.*`.
    pub enhancements: Enhancements,
    /// `audio.*`.
    pub audio: Audio,
    /// `input.*`.
    pub input: Input,
}

impl Settings {
    /// Reads a key in its stored form. `None` means the key is not in the schema.
    pub fn get(&self, key: &str) -> Option<SettingValue> {
        let value = match key {
            "backend.discPath" => SettingValue::Text(self.backend.disc_path.clone()),
            "backend.discVerification" => {
                SettingValue::Text(self.backend.disc_verification.as_config_str().to_owned())
            }
            "backend.skipPrelaunch" => SettingValue::Bool(self.backend.skip_prelaunch),
            "backend.checkForUpdates" => SettingValue::Bool(self.backend.check_for_updates),
            "video.renderer" => SettingValue::Text(self.video.renderer.as_str().to_owned()),
            "video.windowWidth" => SettingValue::Int(self.video.window_width),
            "video.windowHeight" => SettingValue::Int(self.video.window_height),
            "video.windowMode" => SettingValue::Int(self.video.window_mode),
            "video.uiScale" => SettingValue::Int(self.video.ui_scale),
            "video.theme" => SettingValue::Text(self.video.theme.as_config_str().to_owned()),
            "video.vsync" => SettingValue::Bool(self.video.vsync),
            "video.maxFrameRate" => SettingValue::Int(self.video.max_frame_rate),
            "video.hudAtEdges" => SettingValue::Bool(self.video.hud_at_edges),
            "enhancements.preloadTextures" => SettingValue::Int(self.enhancements.preload_textures),
            "gameplay.disableResetti" => SettingValue::Bool(self.enhancements.disable_resetti),
            "gameplay.disableShopVisitorRequirement" => {
                SettingValue::Bool(self.enhancements.disable_shop_visitor_requirement)
            }
            "gameplay.borderlessAcres" => SettingValue::Bool(self.enhancements.borderless_acres),
            "gameplay.pillarboxNes" => SettingValue::Bool(self.enhancements.pillarbox_nes),
            "gameplay.fastText" => SettingValue::Bool(self.enhancements.fast_text),
            "gameplay.travelMoveOutChance" => SettingValue::Text(
                self.enhancements
                    .travel_move_out_chance
                    .as_config_str()
                    .to_owned(),
            ),
            "audio.masterVolume" => SettingValue::Int(self.audio.master_volume),
            "input.stickDeadzone" => SettingValue::Int(self.input.stick_deadzone),
            "input.cstickDeadzone" => SettingValue::Int(self.input.cstick_deadzone),
            "input.autoAssignController" => SettingValue::Bool(self.input.auto_assign_controller),
            "input.singlePortMigrated" => SettingValue::Bool(self.input.single_port_migrated),
            "input.allowBackgroundInput" => SettingValue::Bool(self.input.allow_background_input),
            "input.touchControls" => SettingValue::Bool(self.input.touch_controls),
            "input.touchHaptics" => SettingValue::Bool(self.input.touch_haptics),
            "input.keyboardTyping" => SettingValue::Bool(self.input.keyboard_typing),
            _ => return None,
        };
        Some(value)
    }

    /// Writes a key. Rejects unknown keys ([`SettingsError::UnknownKey`]) and
    /// wrong encodings ([`SettingsError::InvalidValue`]) without mutating `self`.
    /// Ranged integers are constrained immediately, matching Borealis
    /// `Var<T>::set_user`: values below the minimum become the minimum and values
    /// above the maximum become the maximum.
    ///
    /// Numeric enum values are rejected here: the enum codec is runtime-owned, so
    /// such values must go through [`crate::migration`] first.
    pub fn set(&mut self, key: &str, value: SettingValue) -> Result<(), SettingsError> {
        match key {
            "backend.discPath" => {
                let text = value
                    .as_text()
                    .ok_or_else(|| SettingsError::invalid(key, "expected a string"))?;
                self.backend.disc_path = text.to_owned();
            }
            "backend.discVerification" => {
                self.backend.disc_verification = decode_enum(key, &value)?;
            }
            "backend.skipPrelaunch" => {
                self.backend.skip_prelaunch = decode_bool(key, &value)?;
            }
            "backend.checkForUpdates" => {
                self.backend.check_for_updates = decode_bool(key, &value)?;
            }
            "video.renderer" => {
                let text = value.as_text().ok_or_else(|| {
                    SettingsError::invalid(key, "expected a renderer name string")
                })?;
                self.video.renderer =
                    Renderer::new(text).map_err(|reason| SettingsError::invalid(key, reason))?;
            }
            "video.windowWidth" => {
                self.video.window_width = decode_range(key, &value, &WINDOW_WIDTH)?;
            }
            "video.windowHeight" => {
                self.video.window_height = decode_range(key, &value, &WINDOW_HEIGHT)?;
            }
            "video.windowMode" => {
                self.video.window_mode = decode_range(key, &value, &WINDOW_MODE)?;
            }
            "video.uiScale" => {
                self.video.ui_scale = sanitize_ui_scale(
                    value
                        .as_int()
                        .ok_or_else(|| SettingsError::invalid(key, "expected an integer"))?,
                );
            }
            "video.theme" => {
                self.video.theme = decode_enum(key, &value)?;
            }
            "video.vsync" => {
                self.video.vsync = decode_bool(key, &value)?;
            }
            "video.maxFrameRate" => {
                self.video.max_frame_rate = decode_range(key, &value, &MAX_FRAME_RATE)?;
            }
            "video.hudAtEdges" => {
                self.video.hud_at_edges = decode_bool(key, &value)?;
            }
            "enhancements.preloadTextures" => {
                self.enhancements.preload_textures = decode_range(key, &value, &PRELOAD_TEXTURES)?;
            }
            "gameplay.disableResetti" => {
                self.enhancements.disable_resetti = decode_bool(key, &value)?;
            }
            "gameplay.disableShopVisitorRequirement" => {
                self.enhancements.disable_shop_visitor_requirement = decode_bool(key, &value)?;
            }
            "gameplay.borderlessAcres" => {
                self.enhancements.borderless_acres = decode_bool(key, &value)?;
            }
            "gameplay.pillarboxNes" => {
                self.enhancements.pillarbox_nes = decode_bool(key, &value)?;
            }
            "gameplay.fastText" => {
                self.enhancements.fast_text = decode_bool(key, &value)?;
            }
            "gameplay.travelMoveOutChance" => {
                self.enhancements.travel_move_out_chance = decode_enum(key, &value)?;
            }
            "audio.masterVolume" => {
                self.audio.master_volume = decode_range(key, &value, &MASTER_VOLUME)?;
            }
            "input.stickDeadzone" => {
                self.input.stick_deadzone = decode_range(key, &value, &STICK_DEADZONE)?;
            }
            "input.cstickDeadzone" => {
                self.input.cstick_deadzone = decode_range(key, &value, &CSTICK_DEADZONE)?;
            }
            "input.autoAssignController" => {
                self.input.auto_assign_controller = decode_bool(key, &value)?;
            }
            "input.singlePortMigrated" => {
                self.input.single_port_migrated = decode_bool(key, &value)?;
            }
            "input.allowBackgroundInput" => {
                self.input.allow_background_input = decode_bool(key, &value)?;
            }
            "input.touchControls" => {
                self.input.touch_controls = decode_bool(key, &value)?;
            }
            "input.touchHaptics" => {
                self.input.touch_haptics = decode_bool(key, &value)?;
            }
            "input.keyboardTyping" => {
                self.input.keyboard_typing = decode_bool(key, &value)?;
            }
            _ => return Err(SettingsError::UnknownKey(key.to_owned())),
        }
        Ok(())
    }

    /// Every schema key.
    pub const KEYS: [&'static str; 29] = [
        "backend.discPath",
        "backend.discVerification",
        "backend.skipPrelaunch",
        "backend.checkForUpdates",
        "video.renderer",
        "video.windowWidth",
        "video.windowHeight",
        "video.windowMode",
        "video.uiScale",
        "video.theme",
        "video.vsync",
        "video.maxFrameRate",
        "video.hudAtEdges",
        "enhancements.preloadTextures",
        "gameplay.disableResetti",
        "gameplay.disableShopVisitorRequirement",
        "gameplay.borderlessAcres",
        "gameplay.pillarboxNes",
        "gameplay.fastText",
        "gameplay.travelMoveOutChance",
        "audio.masterVolume",
        "input.stickDeadzone",
        "input.cstickDeadzone",
        "input.autoAssignController",
        "input.singlePortMigrated",
        "input.allowBackgroundInput",
        "input.touchControls",
        "input.touchHaptics",
        "input.keyboardTyping",
    ];

    /// Reports every field that is out of range, in [`Settings::KEYS`] order.
    ///
    /// The model is reachable through public fields, so this is the check that a
    /// caller runs before trusting a `Settings` it did not build through
    /// [`Settings::set`].
    pub fn validate(&self) -> Result<(), Vec<SettingsError>> {
        let mut errors = Vec::new();
        let mut check = |key: &str, range: &RangeInclusive<i64>| {
            let value = self
                .get(key)
                .and_then(|v| v.as_int())
                .expect("declared integer key");
            if !range.contains(&value) {
                errors.push(SettingsError::range(key, value, range));
            }
        };
        for (key, range) in [
            ("video.windowWidth", &WINDOW_WIDTH),
            ("video.windowHeight", &WINDOW_HEIGHT),
            ("video.windowMode", &WINDOW_MODE),
            ("video.maxFrameRate", &MAX_FRAME_RATE),
            ("enhancements.preloadTextures", &PRELOAD_TEXTURES),
            ("audio.masterVolume", &MASTER_VOLUME),
            ("input.stickDeadzone", &STICK_DEADZONE),
            ("input.cstickDeadzone", &CSTICK_DEADZONE),
        ] {
            check(key, range);
        }
        if self.video.ui_scale != 0 && !UI_SCALE.contains(&self.video.ui_scale) {
            errors.push(SettingsError::OutOfRange {
                key: "video.uiScale".to_owned(),
                value: self.video.ui_scale,
                min: *UI_SCALE.start(),
                max: *UI_SCALE.end(),
            });
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Applies the registry's own repair rules: clamps the ranged integers and
    /// normalizes `video.uiScale` (`0` stays automatic, anything else is clamped
    /// to 50..=200). Enum and string values are left alone.
    pub fn sanitize(&mut self) {
        self.video.window_width = clamp(self.video.window_width, &WINDOW_WIDTH);
        self.video.window_height = clamp(self.video.window_height, &WINDOW_HEIGHT);
        self.video.window_mode = clamp(self.video.window_mode, &WINDOW_MODE);
        self.video.max_frame_rate = clamp(self.video.max_frame_rate, &MAX_FRAME_RATE);
        self.enhancements.preload_textures =
            clamp(self.enhancements.preload_textures, &PRELOAD_TEXTURES);
        self.audio.master_volume = clamp(self.audio.master_volume, &MASTER_VOLUME);
        self.input.stick_deadzone = clamp(self.input.stick_deadzone, &STICK_DEADZONE);
        self.input.cstick_deadzone = clamp(self.input.cstick_deadzone, &CSTICK_DEADZONE);
        self.video.ui_scale = sanitize_ui_scale(self.video.ui_scale);
    }

    /// Mirrors `sanitize_renderer`: a renderer Aurora does not report is reset to
    /// [`Renderer::AUTO`]. `available` must include `"auto"`; this crate never
    /// queries Aurora, so the caller supplies the list.
    pub fn sanitize_renderer(&mut self, available: &[&str]) {
        if !available.contains(&self.video.renderer.as_str()) {
            self.video.renderer = Renderer::auto();
        }
    }
}

/// `sanitize_ui_scale` from `pc/src/acgc_settings.cpp`.
pub fn sanitize_ui_scale(value: i64) -> i64 {
    if value == 0 {
        0
    } else {
        clamp(value, &UI_SCALE)
    }
}

fn clamp(value: i64, range: &RangeInclusive<i64>) -> i64 {
    value.clamp(*range.start(), *range.end())
}

fn decode_bool(key: &str, value: &SettingValue) -> Result<bool, SettingsError> {
    value
        .as_bool()
        .ok_or_else(|| SettingsError::invalid(key, "expected a boolean"))
}

fn decode_range(
    key: &str,
    value: &SettingValue,
    range: &RangeInclusive<i64>,
) -> Result<i64, SettingsError> {
    let int = value
        .as_int()
        .ok_or_else(|| SettingsError::invalid(key, "expected an integer"))?;
    Ok(clamp(int, range))
}

fn decode_enum<T: EnumStrings>(key: &str, value: &SettingValue) -> Result<T, SettingsError> {
    let text = value.as_text().ok_or_else(|| {
        SettingsError::invalid(
            key,
            "expected an enum string; numeric enum values need the codec adapter",
        )
    })?;
    T::from_config_str(text)
        .ok_or_else(|| SettingsError::invalid(key, format!("unrecognized value '{text}'")))
}

/// String codec shared by the config enums, so [`Settings::set`] stays free of
/// per-enum duplication.
pub trait EnumStrings: Sized {
    /// Canonical settings-file string.
    fn as_config_str(&self) -> &'static str;
    /// Inverse of [`EnumStrings::as_config_str`].
    fn from_config_str(value: &str) -> Option<Self>;
}

impl EnumStrings for DiscVerificationState {
    fn as_config_str(&self) -> &'static str {
        DiscVerificationState::as_config_str(self)
    }

    fn from_config_str(value: &str) -> Option<Self> {
        DiscVerificationState::from_config_str(value)
    }
}

impl EnumStrings for InterfaceTheme {
    fn as_config_str(&self) -> &'static str {
        InterfaceTheme::as_config_str(self)
    }

    fn from_config_str(value: &str) -> Option<Self> {
        InterfaceTheme::from_config_str(value)
    }
}

impl EnumStrings for TravelMoveOutChance {
    fn as_config_str(&self) -> &'static str {
        TravelMoveOutChance::as_config_str(self)
    }

    fn from_config_str(value: &str) -> Option<Self> {
        TravelMoveOutChance::from_config_str(value)
    }
}
