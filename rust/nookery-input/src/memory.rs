//! An in-memory reference [`ProfileStore`] with aurora's rules.
//!
//! This is *not* the runtime adapter. It exists so the editing and migration
//! functions can be exercised end to end without SDL, Aurora, or disk I/O, and so
//! the store contract has one worked implementation. Behavior mirrors
//! `lib/input/profiles.cpp`:
//!
//! * Per-target defaults, with stored control lists replacing the default list
//!   wholesale (an empty list is an explicit unbind).
//! * `set_binding` validates the target and every binding, then clears
//!   `input.source` and `held` sources, because profiles resolve sources at
//!   runtime.
//! * `rebind` applies the conflict policy first, then stores the new list.
//!
//! Documented divergence: upstream keys controller profiles by controller
//! *identity* (GUID/serial/vid/pid, with fallback matching) so one profile follows
//! one physical controller across reconnects. Here a controller profile is keyed
//! by `SourceId`, which is what `ProfileTarget::controller` already carries; the
//! identity matching belongs to the Aurora adapter, not to this value type.

use std::collections::BTreeMap;

use crate::binding::{Binding, ConflictPolicy, INVALID_SOURCE, ProfileTarget, valid_binding};
use crate::control::ControlId;
use crate::defaults;
use crate::store::{ProfileStore, rebind};

/// One profile's stored control lists, keyed by registration order.
type StoredControls = BTreeMap<ControlId, Vec<Binding>>;

/// An in-memory profile set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryProfiles {
    keyboard_defaults: BTreeMap<u32, Vec<Binding>>,
    controller_defaults: Vec<Binding>,
    keyboard: BTreeMap<u32, StoredControls>,
    controllers: BTreeMap<u64, StoredControls>,
}

impl MemoryProfiles {
    /// An empty profile set: no defaults, no stored profiles.
    pub fn new() -> Self {
        Self::default()
    }

    /// A keyboard port with the Nookery default table installed, matching
    /// `register_defaults()` → `set_default_keyboard_bindings(0, …)`.
    ///
    /// Controller defaults are *not* installed: upstream derives them from the
    /// device mapping (`detail::default_pad_bindings`), which is adapter work.
    pub fn with_nookery_keyboard_defaults(port: u32) -> Self {
        let mut profiles = Self::new();
        crate::store::merge_defaults(
            profiles.keyboard_defaults.entry(port).or_default(),
            defaults::default_bindings(),
            ProfileTarget::keyboard(port),
        );
        profiles
    }

    /// `set_default_keyboard_bindings(port, …)` for a caller-supplied table.
    pub fn set_default_keyboard_bindings(&mut self, port: u32, bindings: Vec<Binding>) {
        if port < 4 {
            crate::store::merge_defaults(
                self.keyboard_defaults.entry(port).or_default(),
                bindings,
                ProfileTarget::keyboard(port),
            );
        }
    }

    /// `set_default_controller_bindings(…)`.
    pub fn set_default_controller_bindings(&mut self, bindings: Vec<Binding>) {
        crate::store::merge_defaults(
            &mut self.controller_defaults,
            bindings,
            ProfileTarget::controller(1),
        );
    }

    /// `reset_to_defaults(…)`: drop every stored list for the target.
    pub fn reset_to_defaults(&mut self, target: ProfileTarget) {
        match (target, target.is_known()) {
            (_, false) => {}
            (ProfileTarget::Keyboard(port), true) => {
                if let Some(stored) = self.keyboard.get_mut(&port) {
                    stored.clear();
                }
            }
            (ProfileTarget::Controller(source), true) => {
                if let Some(stored) = self.controllers.get_mut(&source) {
                    stored.clear();
                }
            }
        }
    }

    fn defaults_for(&self, target: ProfileTarget) -> Vec<Binding> {
        match target {
            ProfileTarget::Keyboard(port) => self
                .keyboard_defaults
                .get(&port)
                .cloned()
                .unwrap_or_default(),
            ProfileTarget::Controller(_) => self.controller_defaults.clone(),
        }
    }

    fn stored_for(&self, target: ProfileTarget) -> Option<&StoredControls> {
        match target {
            ProfileTarget::Keyboard(port) => self.keyboard.get(&port),
            ProfileTarget::Controller(source) => self.controllers.get(&source),
        }
    }

    fn stored_for_mut(&mut self, target: ProfileTarget) -> &mut StoredControls {
        match target {
            ProfileTarget::Keyboard(port) => self.keyboard.entry(port).or_default(),
            ProfileTarget::Controller(source) => self.controllers.entry(source).or_default(),
        }
    }
}

impl ProfileStore for MemoryProfiles {
    fn bindings_for_control(&self, target: ProfileTarget, control: ControlId) -> Vec<Binding> {
        if !target.is_known() {
            return Vec::new();
        }
        if let Some(stored) = self
            .stored_for(target)
            .and_then(|stored| stored.get(&control))
        {
            return stored
                .iter()
                .map(|binding| Binding {
                    target: control,
                    ..binding.clone()
                })
                .collect();
        }
        self.defaults_for(target)
            .into_iter()
            .filter(|binding| binding.target == control)
            .map(|binding| Binding {
                target: control,
                ..binding
            })
            .collect()
    }

    fn set_binding(
        &mut self,
        target: ProfileTarget,
        control: ControlId,
        mut bindings: Vec<Binding>,
    ) -> bool {
        if !target.is_known()
            || !bindings
                .iter()
                .all(|binding| valid_binding(target, binding))
        {
            return false;
        }
        for binding in &mut bindings {
            binding.input.source = INVALID_SOURCE;
            for held in &mut binding.held {
                held.input.source = INVALID_SOURCE;
            }
        }
        self.stored_for_mut(target).insert(control, bindings);
        true
    }

    fn rebind(
        &mut self,
        target: ProfileTarget,
        control: ControlId,
        binding: Binding,
        policy: ConflictPolicy,
    ) -> bool {
        rebind(self, target, control, binding, policy)
    }

    fn has_stored_control(&self, target: ProfileTarget, control: ControlId) -> bool {
        self.stored_for(target)
            .is_some_and(|stored| stored.contains_key(&control))
    }

    fn effective_controls(&self, target: ProfileTarget) -> Vec<(ControlId, Binding)> {
        if !target.is_known() {
            return Vec::new();
        }
        let mut effective: BTreeMap<ControlId, Vec<Binding>> = BTreeMap::new();
        for binding in self.defaults_for(target) {
            effective.entry(binding.target).or_default().push(binding);
        }
        if let Some(stored) = self.stored_for(target) {
            for (control, bindings) in stored {
                effective.insert(*control, bindings.clone());
            }
        }
        effective
            .into_iter()
            .flat_map(|(control, bindings)| {
                bindings.into_iter().map(move |binding| {
                    (
                        control,
                        Binding {
                            target: control,
                            ..binding
                        },
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{HeldInput, PhysicalInput};
    use crate::sdl::{Control, Scancode};

    #[test]
    fn stored_bindings_round_trip_without_runtime_sources() {
        let mut profiles = MemoryProfiles::with_nookery_keyboard_defaults(0);
        let target = ProfileTarget::keyboard(0);
        let mut binding = Binding::new(
            PhysicalInput {
                source: 41,
                control: Control::Key(Scancode::Q),
            },
            ControlId::A,
            -1.0,
        );
        binding.held.push(HeldInput {
            input: PhysicalInput {
                source: 42,
                control: Control::Key(Scancode::E),
            },
            threshold: 0.25,
        });
        assert!(profiles.set_binding(target, ControlId::A, vec![binding]));

        let stored = profiles.bindings_for_control(target, ControlId::A);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].input.source, INVALID_SOURCE);
        assert_eq!(stored[0].held[0].input.source, INVALID_SOURCE);
        assert_eq!(stored[0].input.control, Control::Key(Scancode::Q));
        assert_eq!(stored[0].held[0].threshold, 0.25);
        assert_eq!(stored[0].scale, -1.0);
    }

    #[test]
    fn empty_stored_binding_is_an_explicit_unbind() {
        let mut profiles = MemoryProfiles::with_nookery_keyboard_defaults(0);
        let target = ProfileTarget::keyboard(0);
        assert_eq!(profiles.bindings_for_control(target, ControlId::A).len(), 1);
        assert!(profiles.set_binding(target, ControlId::A, Vec::new()));
        assert!(profiles.has_stored_control(target, ControlId::A));
        assert!(
            profiles
                .bindings_for_control(target, ControlId::A)
                .is_empty()
        );
    }

    #[test]
    fn invalid_keyboard_ports_cannot_store_or_install_defaults() {
        let mut profiles = MemoryProfiles::new();
        let invalid = ProfileTarget::keyboard(4);
        let binding = Binding::new(
            PhysicalInput::new(Control::Key(Scancode::A)),
            ControlId::A,
            1.0,
        );
        assert!(!profiles.set_binding(invalid, ControlId::A, vec![binding.clone()]));
        profiles.set_default_keyboard_bindings(4, vec![binding]);
        assert!(
            profiles
                .bindings_for_control(invalid, ControlId::A)
                .is_empty()
        );
    }
}
