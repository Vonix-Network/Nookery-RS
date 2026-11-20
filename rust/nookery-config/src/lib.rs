//! Settings model ported from Nookery's `pc/include/acgc/settings.hpp` and
//! `pc/src/acgc_settings.cpp`.
//!
//! Scope: defaults, ranges, validation, sanitization, legacy INI import, and
//! settings-file migration. No filesystem access, no UI, no Aurora calls.
//!
//! # Runtime adapter boundaries
//!
//! Three upstream behaviours depend on the host runtime and are exposed as
//! explicit seams instead of being faked:
//!
//! * `video.renderer` is an `AuroraBackend` upstream, and its numeric codec plus
//!   `aurora_get_available_backends()` are runtime-owned here modeled as an
//!   opaque string ([`settings::Renderer`]) repaired through
//!   [`settings::Settings::sanitize_renderer`] with a caller-supplied list.
//! * Numeric enum values are decoded by `borealis::config::Codec<T>`; see
//!   [`migration::EnumCodec`].
//! * `video.uiScale` and `input.touchControls` default from
//!   `ACGC_HAS_WINDOW_CONTROLS`. This crate defaults to the desktop build and
//!   documents the mobile values ([`settings::MOBILE_UI_SCALE_DEFAULT`]).

mod enums;
mod legacy;
mod migration;
mod settings;

pub use enums::{DiscVerificationState, InterfaceTheme, TravelMoveOutChance};
pub use legacy::{LegacyImport, apply_legacy_value, parse_legacy_settings};
pub use migration::{EnumCodec, migrate_enum, migrate_settings, migrate_travel_move_out};
pub use settings::{
    Audio, Backend, CSTICK_DEADZONE, DESKTOP_UI_SCALE_DEFAULT, Enhancements, EnumStrings, Input,
    MASTER_VOLUME, MAX_FRAME_RATE, MOBILE_UI_SCALE_DEFAULT, PRELOAD_TEXTURES, Renderer,
    STICK_DEADZONE, SettingValue, Settings, SettingsError, UI_SCALE, Video, WINDOW_HEIGHT,
    WINDOW_MODE, WINDOW_WIDTH, sanitize_ui_scale,
};

#[cfg(test)]
mod tests;
