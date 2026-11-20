//! The profile-store boundary.
//!
//! `acgc::input::editable_bindings` and `replace_binding` read and write Aurora
//! profiles through `aurora::pad::bindings_for_control` / `set_binding` /
//! `rebind` / `profile`, plus `input::save_profiles()` for persistence. Aurora
//! keeps those profiles in process-global containers
//! (`lib/input/profiles.cpp`) and serializes them to `input.json`, so this crate
//! declares the operations as a trait and ships one in-memory implementation
//! that follows the same rules. Nothing here touches SDL, the filesystem, or a
//! live input path.
//!
//! Porting the real adapter means implementing [`ProfileStore`] over
//! `aurora::pad::*` and `input::save_profiles()`; until that lands, no game input
//! path is claimed.

use crate::binding::{
    Binding, ConflictPolicy, PhysicalInput, ProfileTarget, accepts, same_input, valid_binding,
};
use crate::control::{ControlId, linked};

/// The profile operations the editing functions need.
///
/// A store resolves a target to a *effective* binding map: per-target defaults,
/// with any stored control entry replacing the default list wholesale (an empty
/// stored list is an explicit unbind). That is `effective()` in aurora
/// `lib/input/profiles.cpp`.
pub trait ProfileStore {
    /// `aurora::pad::bindings_for_control`, with `target` stamped on every
    /// binding. Unknown targets yield an empty list.
    fn bindings_for_control(&self, target: ProfileTarget, control: ControlId) -> Vec<Binding>;

    /// `aurora::pad::set_binding` by control id.
    ///
    /// Stores `bindings` as the whole list for `control`, clearing
    /// `input.source` (and `held` sources) first. Rejects the write and returns
    /// `false` when the target is unknown or any binding is invalid for it.
    fn set_binding(
        &mut self,
        target: ProfileTarget,
        control: ControlId,
        bindings: Vec<Binding>,
    ) -> bool;

    /// `aurora::pad::rebind`: apply `policy` to `binding.input`, then store
    /// `binding` as the only binding of `control`. `false` means nothing changed.
    fn rebind(
        &mut self,
        target: ProfileTarget,
        control: ControlId,
        binding: Binding,
        policy: ConflictPolicy,
    ) -> bool;

    /// Whether the stored profile names `control` explicitly — the
    /// `profile->bindings.contains(descriptor->name)` test in
    /// `apply_legacy_controller_input`. This is what preserves explicit unbinds.
    fn has_stored_control(&self, target: ProfileTarget, control: ControlId) -> bool;

    /// Every `(control, bindings)` pair in the effective map, for conflict
    /// lookup. Upstream iterates the same map by registered name.
    fn effective_controls(&self, target: ProfileTarget) -> Vec<(ControlId, Binding)>;
}

/// `find_conflicts()`: other controls that already bind `input`.
///
/// A conflict is a control in the same conflict group, not [`linked`] to
/// `control`, holding an input for which [`same_input`] holds. `controls`
/// yields `(control, binding)` pairs from the effective map.
///
/// Because `same_input` treats `Full` as matching both halves, rebinding one
/// stick half also unbinds a whole-axis binding on the same stick.
pub fn find_conflicts<I>(control: ControlId, input: PhysicalInput, controls: I) -> Vec<ControlId>
where
    I: IntoIterator<Item = (ControlId, Binding)>,
{
    let group = control.conflict_group();
    let mut conflicts = Vec::new();
    for (other, binding) in controls {
        if other == control
            || other.conflict_group() != group
            || linked(other, control)
            || !same_input(binding.input, input)
            || conflicts.contains(&other)
        {
            continue;
        }
        conflicts.push(other);
    }
    conflicts
}

/// `aurora::pad::rebind`, generic over the store.
///
/// Used both by [`ProfileStore::rebind`] implementations and directly by
/// [`crate::edit::replace_binding`].
pub fn rebind<S: ProfileStore + ?Sized>(
    store: &mut S,
    target: ProfileTarget,
    control: ControlId,
    binding: Binding,
    policy: ConflictPolicy,
) -> bool {
    if !valid_binding(target, &binding) || !target.is_known() {
        return false;
    }

    if policy == ConflictPolicy::Unbind {
        for other in find_conflicts(control, binding.input, store.effective_controls(target)) {
            let mut list = store.bindings_for_control(target, other);
            list.retain(|old| !same_input(old.input, binding.input));
            store.set_binding(target, other, list);
        }
    }

    store.set_binding(target, control, vec![binding])
}

/// `set_default_keyboard_bindings(port, …)` table maintenance.
///
/// Upstream drops inputs the target does not accept, erases existing default
/// entries for any target the new list mentions, then appends. The caller owns
/// the table, so no global is involved.
pub fn merge_defaults(defaults: &mut Vec<Binding>, bindings: Vec<Binding>, target: ProfileTarget) {
    let accepted: Vec<Binding> = bindings
        .into_iter()
        .filter(|binding| accepts(target, binding.input))
        .collect();
    defaults.retain(|old| !accepted.iter().any(|new| new.target == old.target));
    defaults.extend(accepted);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdl::{AxisDirection, Control, GamepadAxis, Scancode};

    #[test]
    fn conflicts_skip_linked_and_self() {
        let key = PhysicalInput::new(Control::Key(Scancode::Q));
        let controls = vec![
            (ControlId::L, Binding::new(key, ControlId::L, 1.0)),
            (
                ControlId::TriggerL,
                Binding::new(key, ControlId::TriggerL, 1.0),
            ),
            (ControlId::A, Binding::new(key, ControlId::A, 1.0)),
        ];

        // L and trigger_l are linked, so neither reports the other.
        assert_eq!(
            find_conflicts(ControlId::L, key, controls.clone()),
            vec![ControlId::A]
        );
        assert_eq!(
            find_conflicts(ControlId::TriggerL, key, controls.clone()),
            vec![ControlId::A]
        );
        // A itself is skipped, and both L controls are conflicts.
        assert_eq!(
            find_conflicts(ControlId::A, key, controls),
            vec![ControlId::L, ControlId::TriggerL]
        );
    }

    #[test]
    fn conflicts_ignore_different_inputs_and_duplicate_matches() {
        let key = PhysicalInput::new(Control::Key(Scancode::Q));
        let other = PhysicalInput::new(Control::Key(Scancode::E));
        let controls = vec![
            (ControlId::A, Binding::new(other, ControlId::A, 1.0)),
            // Two bindings for the same control still yield one conflict.
            (ControlId::B, Binding::new(key, ControlId::B, 1.0)),
            (ControlId::B, Binding::new(key, ControlId::B, -1.0)),
        ];
        assert_eq!(
            find_conflicts(ControlId::Z, key, controls),
            vec![ControlId::B]
        );
    }

    #[test]
    fn full_axis_binding_conflicts_with_a_single_half() {
        let full = PhysicalInput::new(Control::GamepadAxis(
            GamepadAxis::LeftX,
            AxisDirection::Full,
        ));
        let positive = PhysicalInput::new(Control::GamepadAxis(
            GamepadAxis::LeftX,
            AxisDirection::Positive,
        ));
        let controls = vec![
            (ControlId::LeftX, Binding::new(full, ControlId::LeftX, 1.0)),
            (
                ControlId::LeftY,
                Binding::new(positive, ControlId::LeftY, 1.0),
            ),
        ];
        // Conflicts are matched on physical input alone, so a query matches every
        // control bound to that stick: the half sees the `Full` binding and the
        // other control's identical half, and `Full` sees both halves.
        assert_eq!(
            find_conflicts(ControlId::RightX, positive, controls.clone()),
            vec![ControlId::LeftX, ControlId::LeftY]
        );
        assert_eq!(
            find_conflicts(ControlId::RightX, full, controls),
            vec![ControlId::LeftX, ControlId::LeftY]
        );
    }

    #[test]
    fn merge_defaults_replaces_targets_and_filters_rejected_inputs() {
        let target = ProfileTarget::keyboard(0);
        let mut defaults = vec![
            Binding::new(
                PhysicalInput::new(Control::Key(Scancode::SPACE)),
                ControlId::A,
                1.0,
            ),
            Binding::new(
                PhysicalInput::new(Control::Key(Scancode::X)),
                ControlId::X,
                1.0,
            ),
        ];
        let new = vec![
            Binding::new(
                PhysicalInput::new(Control::Key(Scancode::Z)),
                ControlId::A,
                1.0,
            ),
            // Gamepad input on a keyboard target: dropped, as upstream's accepts() does.
            Binding::new(
                PhysicalInput::new(Control::GamepadButton(crate::sdl::GamepadButton::South)),
                ControlId::Y,
                1.0,
            ),
        ];
        merge_defaults(&mut defaults, new, target);
        let rendered: Vec<(ControlId, Control)> = defaults
            .iter()
            .map(|binding| (binding.target, binding.input.control))
            .collect();
        assert_eq!(
            rendered,
            vec![
                (ControlId::X, Control::Key(Scancode::X)),
                (ControlId::A, Control::Key(Scancode::Z))
            ]
        );

        // A list whose only entry is rejected leaves the table untouched: upstream
        // filters the new list *before* erasing defaults for its targets.
        let rejected_only = vec![Binding::new(
            PhysicalInput::new(Control::GamepadAxis(
                GamepadAxis::LeftX,
                AxisDirection::Positive,
            )),
            ControlId::X,
            1.0,
        )];
        merge_defaults(&mut defaults, rejected_only, target);
        assert_eq!(defaults.len(), 2);
        assert!(
            defaults
                .iter()
                .any(|binding| binding.target == ControlId::X)
        );
    }
}
