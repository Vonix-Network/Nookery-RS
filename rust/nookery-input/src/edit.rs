//! Binding-edit semantics: `editable_bindings` and `replace_binding`.
//!
//! Source of truth: `pc/src/acgc_input_profiles.cpp`. Both functions are the
//! controller-config UI's whole write path (`pc/src/ui/controller_config.cpp`
//! calls them for every captured input), so the details below are observable
//! behavior, not implementation detail:
//!
//! * `direction == 0` means "the whole control": the list is returned unchanged,
//!   and a replacement clears every existing binding for that control.
//! * A non-zero direction splits each `Full` axis binding into its positive and
//!   negative halves, in that order, negating the second half's scale. Both
//!   halves are produced for either sign; the caller's sign only chooses which
//!   half the *replacement* targets.
//! * A replacement drops the bindings of the requested half, keeps the opposite
//!   half in its existing order, and appends the new binding at the end.
//! * The new binding's scale is `+1`/`-1` by direction and its source is reset to
//!   `0`, because profiles resolve sources at runtime.
//! * The conflict policy is applied *before* the list is written, so a rejected
//!   rebind leaves the profile untouched while an accepted one may unbind other
//!   controls.

use crate::binding::{Binding, ConflictPolicy, INVALID_SOURCE, PhysicalInput, ProfileTarget};
use crate::control::ControlId;
use crate::sdl::{AxisDirection, Control};
use crate::store::{ProfileStore, rebind};

/// Whether a direction can be passed to the upstream edit functions.
///
/// The C++ API takes a plain `int`: zero means the whole control, a negative
/// value means the negative half, and every positive value means the positive
/// half. Keep that permissive behavior, including for values other than
/// `-1`/`0`/`1`.
pub const fn is_expressible_direction(direction: i32) -> bool {
    let _ = direction;
    true
}

/// `editable_bindings()`.
///
/// Returns the bindings the UI shows/edits for one half of `control`.
pub fn editable_bindings<S: ProfileStore + ?Sized>(
    store: &S,
    target: ProfileTarget,
    control: ControlId,
    direction: i32,
) -> Vec<Binding> {
    let bindings = store.bindings_for_control(target, control);
    if direction == 0 {
        return bindings;
    }
    let mut result = Vec::with_capacity(bindings.len());
    for mut binding in bindings {
        if let Control::GamepadAxis(axis, AxisDirection::Full) = binding.input.control {
            let mut positive = binding.clone();
            positive.input.control = Control::GamepadAxis(axis, AxisDirection::Positive);
            result.push(positive);

            binding.input.control = Control::GamepadAxis(axis, AxisDirection::Negative);
            binding.scale = -binding.scale;
        }
        result.push(binding);
    }
    result
}

/// `replace_binding()`.
///
/// Rebinds one half of `control` — the whole control when `direction == 0` — to
/// `physical`. Returns `false` and mutates nothing when the target or physical
/// input is invalid, or when the store rejects the rebind.
///
/// On success the control's stored list is exactly the surviving
/// opposite-direction bindings in their existing order, followed by the new one.
/// Other controls change only through the conflict policy.
pub fn replace_binding<S: ProfileStore + ?Sized>(
    store: &mut S,
    target: ProfileTarget,
    control: ControlId,
    direction: i32,
    physical: PhysicalInput,
    policy: ConflictPolicy,
) -> bool {
    if !target.is_known() {
        return false;
    }

    let mut bindings = editable_bindings(store, target, control, direction);
    let physical = PhysicalInput {
        source: INVALID_SOURCE,
        ..physical
    };
    let scale = if direction < 0 { -1.0 } else { 1.0 };

    // Keep only the opposite half; for a whole-control edit this empties the list.
    bindings.retain(|binding| direction != 0 && (binding.scale < 0.0) != (direction < 0));

    let binding = Binding::new(physical, control, scale);
    if !rebind(store, target, control, binding.clone(), policy) {
        return false;
    }
    bindings.push(binding);
    store.set_binding(target, control, bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryProfiles;
    use crate::sdl::{Control, Scancode};
    use crate::store::ProfileStore;

    #[test]
    fn noncanonical_nonzero_directions_match_upstream_half_behavior() {
        let profiles = MemoryProfiles::with_nookery_keyboard_defaults(0);
        let bindings =
            editable_bindings(&profiles, ProfileTarget::keyboard(0), ControlId::LeftX, 2);
        assert_eq!(
            bindings
                .iter()
                .map(|binding| (binding.input.control, binding.scale))
                .collect::<Vec<_>>(),
            vec![
                (Control::Key(Scancode::D), 1.0),
                (Control::Key(Scancode::A), -1.0),
            ]
        );
        assert!(is_expressible_direction(-2));
        assert!(is_expressible_direction(2));
    }

    #[test]
    fn replacement_keeps_the_opposite_half_for_noncanonical_direction() {
        let mut profiles = MemoryProfiles::with_nookery_keyboard_defaults(0);
        assert!(replace_binding(
            &mut profiles,
            ProfileTarget::keyboard(0),
            ControlId::LeftX,
            2,
            PhysicalInput::new(Control::Key(Scancode::Q)),
            ConflictPolicy::Allow,
        ));
        let result = profiles.bindings_for_control(ProfileTarget::keyboard(0), ControlId::LeftX);
        assert_eq!(
            result
                .iter()
                .map(|binding| (binding.input.control, binding.scale))
                .collect::<Vec<_>>(),
            vec![
                (Control::Key(Scancode::A), -1.0),
                (Control::Key(Scancode::Q), 1.0),
            ]
        );
    }
}
