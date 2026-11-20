//! The default keyboard binding table.
//!
//! Source of truth: `acgc::input::keyboard_defaults()` in
//! `pc/src/acgc_input_profiles.cpp`. The table is reproduced entry for entry,
//! including the two entries whose legacy key repeats (`L`/`R` also feed the
//! analog triggers) and the `-1` scales that invert the negative stick halves.
//!
//! Upstream registers these through
//! `aurora::pad::set_default_keyboard_bindings(0, …)`, which accepts every key
//! input for a keyboard profile and replaces default entries by target. The
//! table is static data here; [`default_bindings`] is the array form.

use crate::binding::{Binding, PhysicalInput};
use crate::control::{CONTROLS, ControlId};
use crate::sdl::{Control, Scancode};

/// One row of `keyboard_defaults()`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardDefault {
    /// The `keybindings.ini` key that overrides this row.
    pub legacy_key: &'static str,
    /// Target control.
    pub control: ControlId,
    /// Default physical key.
    pub key: Scancode,
    /// Binding scale; `-1` turns the key into the negative stick half.
    pub scale: f32,
}

/// `keyboard_defaults()`, in upstream order.
///
/// Upstream order matters twice: [`default_bindings`] appends in this order, and
/// `prepare_legacy_input` walks it to build the migrated per-control lists.
pub const KEYBOARD_DEFAULTS: &[KeyboardDefault] = &[
    KeyboardDefault {
        legacy_key: "A",
        control: CONTROLS.a,
        key: Scancode::SPACE,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "B",
        control: CONTROLS.b,
        key: Scancode::LSHIFT,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "X",
        control: CONTROLS.x,
        key: Scancode::X,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "Y",
        control: CONTROLS.y,
        key: Scancode::Y,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "Start",
        control: CONTROLS.start,
        key: Scancode::RETURN,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "Z",
        control: CONTROLS.z,
        key: Scancode::Z,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "L",
        control: CONTROLS.l,
        key: Scancode::Q,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "R",
        control: CONTROLS.r,
        key: Scancode::E,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "DPad_Up",
        control: CONTROLS.up,
        key: Scancode::I,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "DPad_Down",
        control: CONTROLS.down,
        key: Scancode::K,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "DPad_Left",
        control: CONTROLS.left,
        key: Scancode::J,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "DPad_Right",
        control: CONTROLS.right,
        key: Scancode::L,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "Stick_Right",
        control: CONTROLS.left_x,
        key: Scancode::D,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "Stick_Left",
        control: CONTROLS.left_x,
        key: Scancode::A,
        scale: -1.0,
    },
    KeyboardDefault {
        legacy_key: "Stick_Up",
        control: CONTROLS.left_y,
        key: Scancode::W,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "Stick_Down",
        control: CONTROLS.left_y,
        key: Scancode::S,
        scale: -1.0,
    },
    KeyboardDefault {
        legacy_key: "CStick_Right",
        control: CONTROLS.right_x,
        key: Scancode::RIGHT,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "CStick_Left",
        control: CONTROLS.right_x,
        key: Scancode::LEFT,
        scale: -1.0,
    },
    KeyboardDefault {
        legacy_key: "CStick_Up",
        control: CONTROLS.right_y,
        key: Scancode::UP,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "CStick_Down",
        control: CONTROLS.right_y,
        key: Scancode::DOWN,
        scale: -1.0,
    },
    KeyboardDefault {
        legacy_key: "L",
        control: CONTROLS.trigger_l,
        key: Scancode::Q,
        scale: 1.0,
    },
    KeyboardDefault {
        legacy_key: "R",
        control: CONTROLS.trigger_r,
        key: Scancode::E,
        scale: 1.0,
    },
];

/// `register_defaults()`: the table as bindings, ready for
/// `set_default_keyboard_bindings(0, …)`.
///
/// One allocation sized to the table; entries are appended in table order, so
/// `left_x`/`left_y`/`right_x`/`right_y` end up with two entries each and the
/// duplicated `L`/`R` rows land on `trigger_l`/`trigger_r`.
pub fn default_bindings() -> Vec<Binding> {
    let mut bindings = Vec::with_capacity(KEYBOARD_DEFAULTS.len());
    for entry in KEYBOARD_DEFAULTS {
        bindings.push(Binding::new(
            PhysicalInput::new(Control::Key(entry.key)),
            entry.control,
            entry.scale,
        ));
    }
    bindings
}

/// The default bindings for one control, in table order.
pub fn defaults_for(control: ControlId) -> Vec<Binding> {
    KEYBOARD_DEFAULTS
        .iter()
        .filter(|entry| entry.control == control)
        .map(|entry| {
            Binding::new(
                PhysicalInput::new(Control::Key(entry.key)),
                entry.control,
                entry.scale,
            )
        })
        .collect()
}

/// The controls `keyboard_defaults()` covers, in first-appearance order.
pub fn defaulted_controls() -> Vec<ControlId> {
    let mut controls: Vec<ControlId> = Vec::with_capacity(KEYBOARD_DEFAULTS.len());
    for entry in KEYBOARD_DEFAULTS {
        if !controls.contains(&entry.control) {
            controls.push(entry.control);
        }
    }
    controls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_defaults_match_the_oracle_table_exactly() {
        let expected = [
            ("A", ControlId::A, Scancode::SPACE, 1.0),
            ("B", ControlId::B, Scancode::LSHIFT, 1.0),
            ("X", ControlId::X, Scancode::X, 1.0),
            ("Y", ControlId::Y, Scancode::Y, 1.0),
            ("Start", ControlId::Start, Scancode::RETURN, 1.0),
            ("Z", ControlId::Z, Scancode::Z, 1.0),
            ("L", ControlId::L, Scancode::Q, 1.0),
            ("R", ControlId::R, Scancode::E, 1.0),
            ("DPad_Up", ControlId::Up, Scancode::I, 1.0),
            ("DPad_Down", ControlId::Down, Scancode::K, 1.0),
            ("DPad_Left", ControlId::Left, Scancode::J, 1.0),
            ("DPad_Right", ControlId::Right, Scancode::L, 1.0),
            ("Stick_Right", ControlId::LeftX, Scancode::D, 1.0),
            ("Stick_Left", ControlId::LeftX, Scancode::A, -1.0),
            ("Stick_Up", ControlId::LeftY, Scancode::W, 1.0),
            ("Stick_Down", ControlId::LeftY, Scancode::S, -1.0),
            ("CStick_Right", ControlId::RightX, Scancode::RIGHT, 1.0),
            ("CStick_Left", ControlId::RightX, Scancode::LEFT, -1.0),
            ("CStick_Up", ControlId::RightY, Scancode::UP, 1.0),
            ("CStick_Down", ControlId::RightY, Scancode::DOWN, -1.0),
            ("L", ControlId::TriggerL, Scancode::Q, 1.0),
            ("R", ControlId::TriggerR, Scancode::E, 1.0),
        ];
        let actual: Vec<_> = KEYBOARD_DEFAULTS
            .iter()
            .map(|entry| (entry.legacy_key, entry.control, entry.key, entry.scale))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn default_bindings_preserve_order_duplicates_and_defaults() {
        let bindings = default_bindings();
        assert_eq!(bindings.len(), KEYBOARD_DEFAULTS.len());
        assert_eq!(bindings[0].target, ControlId::A);
        assert_eq!(bindings[0].input.control, Control::Key(Scancode::SPACE));
        assert_eq!(bindings[0].dead_zone, 0.0);
        assert_eq!(bindings[0].threshold, Binding::DEFAULT_THRESHOLD);
        assert_eq!(
            bindings
                .iter()
                .filter(|binding| binding.target == ControlId::L)
                .count(),
            1
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|binding| binding.target == ControlId::TriggerL)
                .count(),
            1
        );
    }
}
