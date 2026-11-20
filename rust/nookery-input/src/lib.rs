//! Behavior-preserving Rust port of Nookery's input-profile and keybinding slice.
//!
//! Source of truth: `pc/src/acgc_input_profiles.cpp`,
//! `pc/include/acgc/input_profiles.hpp`, `pc/src/acgc_legacy_input.cpp`,
//! `pc/include/acgc/legacy_input.hpp` (pure parts only), and the aurora
//! interfaces they call (`encounter/aurora@7c8c6117`: `include/aurora/binding.hpp`,
//! `include/aurora/pad.hpp`, `lib/input/profiles.cpp`, `lib/input/port.cpp`,
//! `lib/input/names.cpp`).
//!
//! What is real here: the default keyboard table, the direction-splitting edit
//! semantics, conflict detection, binding validation, and the legacy INI
//! normalization/migration planning — each with tests that assert upstream's
//! observable results.
//!
//! What is deliberately *not* claimed:
//!
//! * No SDL runtime, no Aurora registration, no `input.json` serialization, no
//!   game input path. [`ProfileStore`] is the boundary for those; [`MemoryProfiles`]
//!   is a reference implementation, not an integration.
//! * Controller defaults come from the device mapping (`detail::default_pad_bindings`
//!   plus SDL mappings), which is adapter work and is absent here.
//! * Controller profiles are keyed by `SourceId`; upstream matches by controller
//!   identity (GUID/serial/vid/pid).
//! * Game-defined controls (`aurora.binding::register_control` callers) are not
//!   modelled; [`ControlId`] is a closed enum over `aurora.pad.*`.
//! * `acgc::controller_ports` is a runtime concern (layers, source assignment,
//!   `singlePortMigrated`/`autoAssignController` settings) and is not ported.
//!
//! Attribution: the ported logic is MIT-licensed upstream Nookery PC-layer code;
//! see the repository root `LICENSE`.

pub mod binding;
pub mod control;
pub mod defaults;
pub mod edit;
pub mod legacy;
pub mod memory;
mod scancode_names;
pub mod sdl;
pub mod store;

pub use binding::{
    Binding, ConflictPolicy, HeldInput, INVALID_SOURCE, PhysicalInput, ProfileTarget, SourceId,
    accepts, same_input, valid_binding,
};
pub use control::{
    CONTROLS, ControlId, ControlKind, Controls, PadButton, control_for_button, linked,
};
pub use defaults::{KEYBOARD_DEFAULTS, KeyboardDefault, default_bindings};
pub use edit::{editable_bindings, is_expressible_direction, replace_binding};
pub use legacy::{
    ApplyKeyboard, BindingMap, LegacyInput, LegacyWarning, NativeInput, Precedence,
    apply_controller, apply_keyboard, keyboard_input, native_input, parse_ini, prepare,
    prepare_from_ini, trim,
};
pub use memory::MemoryProfiles;
pub use sdl::{AxisDirection, Control, GamepadAxis, GamepadButton, MouseButton, Scancode};
pub use store::{ProfileStore, find_conflicts, merge_defaults, rebind};
