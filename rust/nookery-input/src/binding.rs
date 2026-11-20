//! Physical input records and the binding validity rules.
//!
//! Source of truth: `aurora/binding.hpp` (`PhysicalInput`, `Binding`,
//! `same_input`) and `accepts()` / `valid_binding()` in aurora
//! `lib/input/profiles.cpp`. The records are carried through the editing
//! functions verbatim, so they keep every field upstream keeps — including the
//! ones this slice never interprets (`held`, `dead_zone`, `threshold`).

use crate::control::ControlId;
use crate::sdl::Control;

/// `aurora::input::SourceId`; `0` means "unresolved, filled in at runtime".
pub type SourceId = u64;

/// `aurora::input::kInvalidSourceId`.
pub const INVALID_SOURCE: SourceId = 0;

/// `aurora::binding::PhysicalInput`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalInput {
    pub source: SourceId,
    pub control: Control,
}

impl PhysicalInput {
    /// An unresolved input; profiles resolve the source when they build a set.
    pub const fn new(control: Control) -> Self {
        Self {
            source: INVALID_SOURCE,
            control,
        }
    }
}

/// `aurora::binding::HeldInput`: a second input that must be held.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeldInput {
    pub input: PhysicalInput,
    pub threshold: f32,
}

impl HeldInput {
    pub const DEFAULT_THRESHOLD: f32 = 0.5;
}

/// `aurora::binding::Binding`.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub input: PhysicalInput,
    pub target: ControlId,
    pub held: Vec<HeldInput>,
    pub dead_zone: f32,
    pub scale: f32,
    pub threshold: f32,
}

impl Binding {
    pub const DEFAULT_THRESHOLD: f32 = 0.5;

    /// A binding with upstream's default `held`, `dead_zone` and `threshold`.
    pub fn new(input: PhysicalInput, target: ControlId, scale: f32) -> Self {
        Self {
            input,
            target,
            held: Vec::new(),
            dead_zone: 0.0,
            scale,
            threshold: Self::DEFAULT_THRESHOLD,
        }
    }
}

/// `aurora::binding::same_input`: physical equality, ignoring the source.
///
/// Either direction being `Full` makes two axis halves compare equal, so
/// rebinding one stick half unbinds a whole-axis binding on the same stick.
pub fn same_input(a: PhysicalInput, b: PhysicalInput) -> bool {
    match (a.control, b.control) {
        (Control::Key(x), Control::Key(y)) => x == y,
        (Control::MouseButton(x), Control::MouseButton(y)) => x == y,
        (Control::GamepadButton(x), Control::GamepadButton(y)) => x == y,
        (Control::GamepadAxis(a_axis, a_dir), Control::GamepadAxis(b_axis, b_dir)) => {
            use crate::sdl::AxisDirection::Full;
            a_axis == b_axis && (a_dir == b_dir || a_dir == Full || b_dir == Full)
        }
        _ => false,
    }
}

/// A profile a binding can be installed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileTarget {
    /// `ProfileTarget::keyboard(port)`.
    Keyboard(u32),
    /// `ProfileTarget::controller(source)`.
    Controller(SourceId),
}

impl ProfileTarget {
    pub const fn keyboard(port: u32) -> Self {
        Self::Keyboard(port)
    }

    pub const fn controller(source: SourceId) -> Self {
        Self::Controller(source)
    }

    /// Whether the target is a keyboard profile, which only takes keys and mouse
    /// buttons.
    pub const fn is_keyboard(self) -> bool {
        matches!(self, Self::Keyboard(_))
    }

    /// Ports/profile slots upstream supports (`port < 4`), and source `0` is not
    /// a controller.
    pub const fn is_known(self) -> bool {
        match self {
            Self::Keyboard(port) => port < 4,
            Self::Controller(source) => source != INVALID_SOURCE,
        }
    }
}

/// `aurora::pad::ConflictPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictPolicy {
    /// Unbind the same input from every conflicting control.
    Unbind,
    /// Leave other controls alone; the input drives several controls.
    Allow,
}

/// `accepts()`: does this profile target take this physical input?
pub fn accepts(target: ProfileTarget, input: PhysicalInput) -> bool {
    let keyboard_input = input.control.is_keyboard();
    if keyboard_input != target.is_keyboard() {
        return false;
    }
    match input.control {
        Control::Key(key) => key.is_valid(),
        Control::MouseButton(button) => button.is_valid(),
        Control::GamepadButton(button) => button.value() < crate::sdl::GamepadButton::COUNT,
        Control::GamepadAxis(axis, _) => axis.value() < crate::sdl::GamepadAxis::COUNT,
    }
}

/// `valid_binding()`: an accepted input plus finite, in-range range settings.
pub fn valid_binding(target: ProfileTarget, binding: &Binding) -> bool {
    accepts(target, binding.input)
        && binding.scale.is_finite()
        && binding.dead_zone.is_finite()
        && (0.0..1.0).contains(&binding.dead_zone)
        && binding.threshold.is_finite()
        && (0.0..=1.0).contains(&binding.threshold)
        && binding.held.iter().all(|held| {
            accepts(target, held.input)
                && held.threshold.is_finite()
                && (0.0..=1.0).contains(&held.threshold)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdl::{AxisDirection, GamepadAxis, MouseButton, Scancode};

    #[test]
    fn same_input_ignores_source_but_not_control() {
        let key = PhysicalInput::new(Control::Key(Scancode::SPACE));
        let mut other_source = key;
        other_source.source = 42;
        assert!(same_input(key, other_source));
        assert!(!same_input(
            key,
            PhysicalInput::new(Control::Key(Scancode::RETURN))
        ));
        assert!(!same_input(
            key,
            PhysicalInput::new(Control::MouseButton(MouseButton::LEFT))
        ));
    }

    #[test]
    fn full_axis_direction_matches_both_halves() {
        let full = PhysicalInput::new(Control::GamepadAxis(
            GamepadAxis::LeftX,
            AxisDirection::Full,
        ));
        let positive = PhysicalInput::new(Control::GamepadAxis(
            GamepadAxis::LeftX,
            AxisDirection::Positive,
        ));
        let negative = PhysicalInput::new(Control::GamepadAxis(
            GamepadAxis::LeftX,
            AxisDirection::Negative,
        ));
        let other_axis = PhysicalInput::new(Control::GamepadAxis(
            GamepadAxis::RightX,
            AxisDirection::Full,
        ));

        assert!(same_input(full, positive));
        assert!(same_input(positive, full));
        assert!(same_input(full, negative));
        assert!(!same_input(positive, negative));
        assert!(!same_input(full, other_axis));
    }

    #[test]
    fn keyboard_targets_reject_gamepad_inputs_and_vice_versa() {
        let keyboard = ProfileTarget::keyboard(0);
        let controller = ProfileTarget::controller(7);
        assert!(accepts(
            keyboard,
            PhysicalInput::new(Control::Key(Scancode::A))
        ));
        assert!(accepts(
            keyboard,
            PhysicalInput::new(Control::MouseButton(MouseButton::RIGHT))
        ));
        assert!(!accepts(
            keyboard,
            PhysicalInput::new(Control::GamepadButton(crate::sdl::GamepadButton::South))
        ));
        assert!(!accepts(
            controller,
            PhysicalInput::new(Control::Key(Scancode::A))
        ));
        assert!(accepts(
            controller,
            PhysicalInput::new(Control::GamepadAxis(
                GamepadAxis::LeftTrigger,
                AxisDirection::Positive
            ))
        ));
    }

    #[test]
    fn accepts_rejects_out_of_range_codes() {
        let keyboard = ProfileTarget::keyboard(0);
        assert!(!accepts(
            keyboard,
            PhysicalInput::new(Control::Key(Scancode::UNKNOWN))
        ));
        assert!(!accepts(
            keyboard,
            PhysicalInput::new(Control::Key(Scancode(Scancode::COUNT)))
        ));
        assert!(!accepts(
            keyboard,
            PhysicalInput::new(Control::MouseButton(MouseButton(0)))
        ));
        assert!(!accepts(
            keyboard,
            PhysicalInput::new(Control::MouseButton(MouseButton(33)))
        ));
    }

    #[test]
    fn valid_binding_rejects_non_finite_and_out_of_range_ranges() {
        let target = ProfileTarget::keyboard(0);
        let input = PhysicalInput::new(Control::Key(Scancode::A));
        let good = Binding::new(input, ControlId::A, 1.0);
        assert!(valid_binding(target, &good));

        let mut zero_dead_zone = good.clone();
        zero_dead_zone.dead_zone = 0.0;
        assert!(valid_binding(target, &zero_dead_zone));

        let mut almost_full_dead_zone = good.clone();
        almost_full_dead_zone.dead_zone = 0.999_999_94;
        assert!(valid_binding(target, &almost_full_dead_zone));

        let mut nan_scale = good.clone();
        nan_scale.scale = f32::NAN;
        assert!(!valid_binding(target, &nan_scale));

        let mut bad_dead_zone = good.clone();
        bad_dead_zone.dead_zone = 1.0;
        assert!(!valid_binding(target, &bad_dead_zone));

        let mut negative_dead_zone = good.clone();
        negative_dead_zone.dead_zone = -f32::EPSILON;
        assert!(!valid_binding(target, &negative_dead_zone));

        let mut infinite_dead_zone = good.clone();
        infinite_dead_zone.dead_zone = f32::INFINITY;
        assert!(!valid_binding(target, &infinite_dead_zone));

        let mut bad_threshold = good.clone();
        bad_threshold.threshold = 1.5;
        assert!(!valid_binding(target, &bad_threshold));

        let mut bad_held = good.clone();
        bad_held.held.push(HeldInput {
            input,
            threshold: -0.1,
        });
        assert!(!valid_binding(target, &bad_held));

        let unknown_key = Binding::new(
            PhysicalInput::new(Control::Key(Scancode::UNKNOWN)),
            ControlId::A,
            1.0,
        );
        assert!(!valid_binding(target, &unknown_key));
    }

    #[test]
    fn profile_targets_validate_keyboard_ports_and_controller_sources() {
        assert!(ProfileTarget::keyboard(0).is_known());
        assert!(ProfileTarget::keyboard(3).is_known());
        assert!(!ProfileTarget::keyboard(4).is_known());
        assert!(!ProfileTarget::controller(INVALID_SOURCE).is_known());
        assert!(ProfileTarget::controller(1).is_known());
    }

    #[test]
    fn every_gamepad_value_round_trips() {
        for value in 0..crate::sdl::GamepadButton::COUNT {
            let button = crate::sdl::GamepadButton::from_value(i32::from(value)).expect("in range");
            assert_eq!(button.value(), value);
        }
        assert_eq!(crate::sdl::GamepadButton::from_value(-1), None);
        assert_eq!(crate::sdl::GamepadButton::from_value(26), None);
        for value in 0..crate::sdl::GamepadAxis::COUNT {
            let axis = crate::sdl::GamepadAxis::from_value(i32::from(value)).expect("in range");
            assert_eq!(axis.value(), value);
        }
        assert_eq!(crate::sdl::GamepadAxis::from_value(-1), None);
        assert_eq!(crate::sdl::GamepadAxis::from_value(6), None);
    }
}
