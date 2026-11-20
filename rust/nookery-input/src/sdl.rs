//! SDL3 value types named by the input contract.
//!
//! Source of truth: SDL3 3.4.10, the version aurora pins through
//! `AURORA_SDL3_VERSION` (`cmake/AuroraDependencyVersions.cmake` in
//! `encounter/aurora`). No SDL runtime is linked or initialized here; these are
//! plain data types so the pure layer can name physical inputs without a
//! platform global.
//!
//! Numeric values match SDL's enums exactly because they appear in profile
//! JSON (`encode_binding`/`decode_binding` in aurora `lib/input/profiles.cpp`)
//! and because `accepts()` range-checks them.

use crate::scancode_names::SCANCODE_NAMES;

/// `SDL_Scancode`: physical key identity, independent of keyboard layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Scancode(pub u16);

impl Scancode {
    /// `SDL_SCANCODE_UNKNOWN`.
    pub const UNKNOWN: Self = Self(0);
    /// `SDL_SCANCODE_COUNT`: one past the last valid scancode.
    pub const COUNT: u16 = 512;

    // Only the codes named by `acgc::input::keyboard_defaults()` and the tests.
    pub const A: Self = Self(4);
    pub const B: Self = Self(5);
    pub const D: Self = Self(7);
    pub const E: Self = Self(8);
    pub const I: Self = Self(12);
    pub const J: Self = Self(13);
    pub const K: Self = Self(14);
    pub const L: Self = Self(15);
    pub const Q: Self = Self(20);
    pub const S: Self = Self(22);
    pub const W: Self = Self(26);
    pub const X: Self = Self(27);
    pub const Y: Self = Self(28);
    pub const Z: Self = Self(29);
    pub const RETURN: Self = Self(40);
    pub const SPACE: Self = Self(44);
    pub const RIGHT: Self = Self(79);
    pub const LEFT: Self = Self(80);
    pub const DOWN: Self = Self(81);
    pub const UP: Self = Self(82);
    pub const LSHIFT: Self = Self(225);

    /// In `(SDL_SCANCODE_UNKNOWN, SDL_SCANCODE_COUNT)`, as `accepts()` requires.
    pub const fn is_valid(self) -> bool {
        self.0 > Self::UNKNOWN.0 && self.0 < Self::COUNT
    }

    /// The SDL name for this scancode, if the table names it.
    pub fn name(self) -> Option<&'static str> {
        SCANCODE_NAMES
            .binary_search_by_key(&self.0, |(code, _)| *code)
            .ok()
            .map(|index| SCANCODE_NAMES[index].1)
    }

    /// Port of `SDL_GetScancodeFromName` for this table: first case-insensitive
    /// match in index order, `None` where SDL returns `SDL_SCANCODE_UNKNOWN`.
    pub fn from_name(name: &str) -> Option<Self> {
        if name.is_empty() {
            return None;
        }
        SCANCODE_NAMES
            .iter()
            .find(|(_, candidate)| candidate.eq_ignore_ascii_case(name))
            .map(|(code, _)| Self(*code))
    }
}

/// `SDL_BUTTON_*` mouse button index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MouseButton(pub u8);

impl MouseButton {
    pub const LEFT: Self = Self(1);
    pub const MIDDLE: Self = Self(2);
    pub const RIGHT: Self = Self(3);
    pub const X1: Self = Self(4);
    pub const X2: Self = Self(5);

    /// In `[1, 32]`, as `accepts()` requires.
    pub const fn is_valid(self) -> bool {
        self.0 >= 1 && self.0 <= 32
    }
}

/// `SDL_GamepadButton`. `INVALID` is modelled as `Option`, so the enum starts at
/// `SOUTH == 0` and `GamepadButton::COUNT` closes the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GamepadButton {
    South = 0,
    East = 1,
    West = 2,
    North = 3,
    Back = 4,
    Guide = 5,
    Start = 6,
    LeftStick = 7,
    RightStick = 8,
    LeftShoulder = 9,
    RightShoulder = 10,
    DPadUp = 11,
    DPadDown = 12,
    DPadLeft = 13,
    DPadRight = 14,
    Misc1 = 15,
    RightPaddle1 = 16,
    LeftPaddle1 = 17,
    RightPaddle2 = 18,
    LeftPaddle2 = 19,
    Touchpad = 20,
    Misc2 = 21,
    Misc3 = 22,
    Misc4 = 23,
    Misc5 = 24,
    Misc6 = 25,
}

impl GamepadButton {
    /// `SDL_GAMEPAD_BUTTON_COUNT`.
    pub const COUNT: u8 = 26;

    /// Numeric `SDL_GamepadButton` value.
    pub const fn value(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::value`]; `None` for `SDL_GAMEPAD_BUTTON_INVALID` and
    /// anything `>= SDL_GAMEPAD_BUTTON_COUNT`.
    pub const fn from_value(value: i32) -> Option<Self> {
        Some(match value {
            0 => Self::South,
            1 => Self::East,
            2 => Self::West,
            3 => Self::North,
            4 => Self::Back,
            5 => Self::Guide,
            6 => Self::Start,
            7 => Self::LeftStick,
            8 => Self::RightStick,
            9 => Self::LeftShoulder,
            10 => Self::RightShoulder,
            11 => Self::DPadUp,
            12 => Self::DPadDown,
            13 => Self::DPadLeft,
            14 => Self::DPadRight,
            15 => Self::Misc1,
            16 => Self::RightPaddle1,
            17 => Self::LeftPaddle1,
            18 => Self::RightPaddle2,
            19 => Self::LeftPaddle2,
            20 => Self::Touchpad,
            21 => Self::Misc2,
            22 => Self::Misc3,
            23 => Self::Misc4,
            24 => Self::Misc5,
            25 => Self::Misc6,
            _ => return None,
        })
    }
}

/// `SDL_GamepadAxis`; `INVALID` is modelled as `Option`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GamepadAxis {
    LeftX = 0,
    LeftY = 1,
    RightX = 2,
    RightY = 3,
    LeftTrigger = 4,
    RightTrigger = 5,
}

impl GamepadAxis {
    /// `SDL_GAMEPAD_AXIS_COUNT`.
    pub const COUNT: u8 = 6;

    /// Numeric `SDL_GamepadAxis` value.
    pub const fn value(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::value`]; `None` for `SDL_GAMEPAD_AXIS_INVALID` and
    /// anything `>= SDL_GAMEPAD_AXIS_COUNT`.
    pub const fn from_value(value: i32) -> Option<Self> {
        Some(match value {
            0 => Self::LeftX,
            1 => Self::LeftY,
            2 => Self::RightX,
            3 => Self::RightY,
            4 => Self::LeftTrigger,
            5 => Self::RightTrigger,
            _ => return None,
        })
    }
}

/// `PhysicalInput::GamepadAxis::Direction` from `aurora/binding.hpp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AxisDirection {
    /// Whole `[-1, 1]` range.
    Full,
    /// Positive half only.
    Positive,
    /// Negative half only.
    Negative,
}

impl AxisDirection {
    /// `GamepadAxis::select`: the half a binding consumes from a raw axis value.
    pub fn select(self, value: f32) -> f32 {
        match self {
            Self::Positive => {
                if value > 0.0 {
                    value
                } else {
                    0.0
                }
            }
            Self::Negative => {
                if value < 0.0 {
                    -value
                } else {
                    0.0
                }
            }
            Self::Full => value,
        }
    }

    /// The `direction` argument `acgc::input` passes around: `0` for a whole
    /// control, `-1`/`1` for one half of an axis.
    pub const fn from_edit_direction(direction: i32) -> Option<Self> {
        match direction {
            0 => Some(Self::Full),
            1 => Some(Self::Positive),
            -1 => Some(Self::Negative),
            _ => None,
        }
    }
}

/// The physical control a binding reads, mirroring `aurora::binding::PhysicalInput`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Control {
    Key(Scancode),
    MouseButton(MouseButton),
    GamepadButton(GamepadButton),
    GamepadAxis(GamepadAxis, AxisDirection),
}

impl Control {
    /// Keyboard-profile inputs: keys and mouse buttons.
    pub const fn is_keyboard(self) -> bool {
        matches!(self, Self::Key(_) | Self::MouseButton(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scancode_names_are_case_insensitive_and_keep_sdl_duplicate_order() {
        assert_eq!(Scancode::from_name("space"), Some(Scancode::SPACE));
        assert_eq!(Scancode::from_name("LEFT SHIFT"), Some(Scancode::LSHIFT));
        assert_eq!(Scancode::from_name("Return"), Some(Scancode::RETURN));
        assert_eq!(Scancode(40).name(), Some("Return"));
        assert_eq!(Scancode(158).name(), Some("Return"));
        assert_eq!(Scancode::from_name("not an SDL key"), None);
        assert_eq!(Scancode::from_name(""), None);
    }

    #[test]
    fn axis_selection_preserves_half_boundaries() {
        assert_eq!(AxisDirection::Positive.select(0.0), 0.0);
        assert_eq!(AxisDirection::Positive.select(-0.5), 0.0);
        assert_eq!(AxisDirection::Positive.select(0.5), 0.5);
        assert_eq!(AxisDirection::Negative.select(0.0), 0.0);
        assert_eq!(AxisDirection::Negative.select(-0.5), 0.5);
        assert_eq!(AxisDirection::Negative.select(0.5), 0.0);
        assert_eq!(AxisDirection::Full.select(-0.5), -0.5);
    }
}
