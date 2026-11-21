//! Legacy input migration: INI normalization and the prepared fallbacks.
//!
//! Source of truth: `pc/src/acgc_legacy_input.cpp` and
//! `pc/include/acgc/legacy_input.hpp`. Only the pure part is ported here — file
//! discovery, `SDL_GetScancodeFromName`, `aurora::input::SourceId` allocation, and
//! `input::save_profiles()` stay with the caller/adapter:
//!
//! | Upstream | Here |
//! |---|---|
//! | `prepare_legacy_input(userDirectory, candidates)` `is_regular_file` / `ifstream` | caller reads the file, passes the text ([`prepare`]) |
//! | `std::filesystem::exists(input.json / keyboard_bindings.dat)` | [`Precedence`] flags |
//! | `SDL_GetScancodeFromName` | [`Scancode::from_name`] over the SDL 3.4.10 table |
//! | `s_preparedKeyboard` / `s_controllerFallback` / `s_importedControllers` statics | [`LegacyInput`] value + caller-owned imported set |
//! | `input::save_profiles()` | [`ApplyKeyboard::Applied`] means "caller must persist" |
//!
//! Behavior preserved deliberately, because each is observable:
//!
//! * INI keys are matched **case-sensitively** against the legacy names (`A`,
//!   `DPad_Up`, …); INI *values* are matched case-insensitively.
//! * `none` is an explicit unbind for both keyboards and controllers: the control
//!   is recorded with an empty list, which afterwards wins over defaults.
//! * An unknown keyboard value keeps the default key for that row but still marks
//!   the keyboard migration as pending (upstream's `found` flag).
//! * `Pad_L` / `Pad_R` also populate the analog `trigger_l` / `trigger_r`
//!   controls with a positive-direction axis binding.

use std::collections::BTreeMap;

use crate::binding::{Binding, PhysicalInput, ProfileTarget};
use crate::control::{ControlId, PadButton, control_for_button};
use crate::defaults::KEYBOARD_DEFAULTS;
use crate::sdl::{AxisDirection, Control, GamepadAxis, GamepadButton, MouseButton, Scancode};
use crate::store::ProfileStore;

/// A control's prepared bindings, keyed by control id (upstream uses
/// `std::map<ControlId, …>`, i.e. registration order).
pub type BindingMap = BTreeMap<ControlId, Vec<Binding>>;

/// Which legacy artifacts already existed when the migration ran.
///
/// Upstream records this *before* its loader creates `input.json` or removes
/// `keyboard_bindings.dat`, so an existing profile always beats the INI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Precedence {
    /// `input.json` existed.
    pub existing_profiles: bool,
    /// `keyboard_bindings.dat` existed.
    pub existing_keyboard_file: bool,
}

impl Precedence {
    /// `existingKeyboard` in `prepare_legacy_input`: either artifact wins.
    pub const fn existing_keyboard(self) -> bool {
        self.existing_profiles || self.existing_keyboard_file
    }
}

/// A row of `kControllerBindings[]`.
///
/// Upstream's fourth field (`defaultValue`) is never read; it is recorded here as
/// a comment instead of a field so the table has no dead data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerBinding {
    /// INI key, e.g. `Pad_A`.
    pub key: &'static str,
    /// Target `PADButton`.
    pub pad_button: PadButton,
}

/// `kControllerBindings`, in upstream order.
///
/// The unread upstream `defaultValue` column was `a`, `b`, `x`, `y`, `start`,
/// `rightshoulder`, `lefttrigger`, `righttrigger`, `dpup`, `dpdown`, `dpleft`,
/// `dpright` for the rows below.
pub const CONTROLLER_BINDINGS: &[ControllerBinding] = &[
    ControllerBinding {
        key: "Pad_A",
        pad_button: PadButton::A,
    },
    ControllerBinding {
        key: "Pad_B",
        pad_button: PadButton::B,
    },
    ControllerBinding {
        key: "Pad_X",
        pad_button: PadButton::X,
    },
    ControllerBinding {
        key: "Pad_Y",
        pad_button: PadButton::Y,
    },
    ControllerBinding {
        key: "Pad_Start",
        pad_button: PadButton::Start,
    },
    ControllerBinding {
        key: "Pad_Z",
        pad_button: PadButton::Z,
    },
    ControllerBinding {
        key: "Pad_L",
        pad_button: PadButton::L,
    },
    ControllerBinding {
        key: "Pad_R",
        pad_button: PadButton::R,
    },
    ControllerBinding {
        key: "Pad_DPad_Up",
        pad_button: PadButton::Up,
    },
    ControllerBinding {
        key: "Pad_DPad_Down",
        pad_button: PadButton::Down,
    },
    ControllerBinding {
        key: "Pad_DPad_Left",
        pad_button: PadButton::Left,
    },
    ControllerBinding {
        key: "Pad_DPad_Right",
        pad_button: PadButton::Right,
    },
];

/// `NativeInputKind` + value, as `native_input()` returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeInput {
    /// `none`: explicitly unbound.
    None,
    /// SDL gamepad button.
    Button(GamepadButton),
    /// SDL gamepad axis, always used in the positive direction upstream.
    Axis(GamepadAxis),
    /// Unrecognized name.
    Invalid,
}

/// A diagnostic upstream emits through `borealis::Log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyWarning {
    /// `Ignoring unknown legacy key '<value>' for <legacy_key>`.
    UnknownKey { legacy_key: String, value: String },
    /// `Ignoring unknown legacy controller input '<value>' for <key>`.
    UnknownControllerInput { key: String, value: String },
}

/// Everything `prepare_legacy_input` leaves behind, as a value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LegacyInput {
    /// Install on the keyboard profile when present. `Some` means "pending";
    /// an empty control list is an explicit unbind.
    pub keyboard: Option<BindingMap>,
    /// Fallbacks for controller profiles; only fills controls the profile does
    /// not name, so explicit unbinds survive.
    pub controller_fallback: BindingMap,
    /// Warnings in upstream emission order.
    pub warnings: Vec<LegacyWarning>,
}

impl LegacyInput {
    /// `LegacyInputLoadResult::keyboardPending`.
    pub const fn keyboard_pending(&self) -> bool {
        self.keyboard.is_some()
    }

    /// `LegacyInputLoadResult::controllerDefaultsApplied`.
    pub fn controller_defaults_applied(&self) -> bool {
        !self.controller_fallback.is_empty()
    }

    /// `LegacyInputLoadResult::loaded`.
    pub fn loaded(&self) -> bool {
        self.keyboard_pending() || self.controller_defaults_applied()
    }
}

/// `trim()` over the C locale's whitespace, matching `std::isspace` in the
/// upstream build rather than Unicode whitespace.
pub fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_ascii_whitespace())
}

/// `lowercase()`; the C locale folds ASCII only.
pub fn lowercase(value: &str) -> String {
    value.to_ascii_lowercase()
}

/// `read_ini()`.
///
/// Skips blank lines, `#`/`;` comments and `[section]` headers, splits on the
/// first `=`, trims both sides, drops empty keys or values, and lets the last
/// duplicate key win (`insert_or_assign`). Duplicate keys are *not* an error.
pub fn parse_ini(text: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for line in text.split('\n') {
        let stripped = trim(line);
        if stripped.is_empty()
            || stripped.starts_with('#')
            || stripped.starts_with(';')
            || stripped.starts_with('[')
        {
            continue;
        }
        let Some(separator) = stripped.find('=') else {
            continue;
        };
        let key = trim(&stripped[..separator]);
        let value = trim(&stripped[separator + 1..]);
        if !key.is_empty() && !value.is_empty() {
            result.insert(key.to_owned(), value.to_owned());
        }
    }
    result
}

/// `keyboard_input()`.
///
/// `mouse1`/`mouse2`/`mouse3` are matched case-insensitively and map to
/// `SDL_BUTTON_LEFT`/`RIGHT`/`MIDDLE`, in that order. Everything else goes through
/// the SDL scancode-name table; an unknown name yields `None` (upstream's
/// `SDL_SCANCODE_UNKNOWN`).
pub fn keyboard_input(value: &str) -> Option<PhysicalInput> {
    match lowercase(value).as_str() {
        "mouse1" => return Some(PhysicalInput::new(Control::MouseButton(MouseButton::LEFT))),
        "mouse2" => return Some(PhysicalInput::new(Control::MouseButton(MouseButton::RIGHT))),
        "mouse3" => {
            return Some(PhysicalInput::new(Control::MouseButton(
                MouseButton::MIDDLE,
            )));
        }
        _ => {}
    }
    Scancode::from_name(value).map(|key| PhysicalInput::new(Control::Key(key)))
}

/// `native_input()`.
pub fn native_input(value: &str) -> NativeInput {
    let name = lowercase(value);
    if name == "none" {
        return NativeInput::None;
    }

    let button = match name.as_str() {
        "a" => Some(GamepadButton::South),
        "b" => Some(GamepadButton::East),
        "x" => Some(GamepadButton::West),
        "y" => Some(GamepadButton::North),
        "back" => Some(GamepadButton::Back),
        "guide" => Some(GamepadButton::Guide),
        "start" => Some(GamepadButton::Start),
        "leftstick" => Some(GamepadButton::LeftStick),
        "rightstick" => Some(GamepadButton::RightStick),
        "leftshoulder" => Some(GamepadButton::LeftShoulder),
        "rightshoulder" => Some(GamepadButton::RightShoulder),
        "dpup" => Some(GamepadButton::DPadUp),
        "dpdown" => Some(GamepadButton::DPadDown),
        "dpleft" => Some(GamepadButton::DPadLeft),
        "dpright" => Some(GamepadButton::DPadRight),
        _ => None,
    };
    if let Some(button) = button {
        return NativeInput::Button(button);
    }

    match name.as_str() {
        "lefttrigger" => NativeInput::Axis(GamepadAxis::LeftTrigger),
        "righttrigger" => NativeInput::Axis(GamepadAxis::RightTrigger),
        _ => NativeInput::Invalid,
    }
}

/// `prepare_legacy_input()`, minus file discovery.
///
/// `values` is [`parse_ini`]'s output. Returns the prepared bindings plus the
/// warnings upstream logs; `loaded()` mirrors `LegacyInputLoadResult::loaded`.
/// An empty `values` map means "empty INI" — upstream warns and returns nothing.
pub fn prepare(values: &BTreeMap<String, String>, precedence: Precedence) -> LegacyInput {
    let mut result = LegacyInput::default();
    if values.is_empty() {
        return result;
    }

    if !precedence.existing_keyboard() {
        let mut keyboard = BindingMap::new();
        let mut found = false;
        for entry in KEYBOARD_DEFAULTS {
            let list = keyboard.entry(entry.control).or_default();
            let mut physical = PhysicalInput::new(Control::Key(entry.key));
            if let Some(value) = values.get(entry.legacy_key) {
                found = true;
                if lowercase(value) == "none" {
                    // Explicit unbind: the control keeps its (empty) entry.
                    continue;
                }
                match keyboard_input(value) {
                    Some(parsed) => physical = parsed,
                    None => result.warnings.push(LegacyWarning::UnknownKey {
                        legacy_key: entry.legacy_key.to_owned(),
                        value: value.clone(),
                    }),
                }
            }
            list.push(Binding::new(physical, entry.control, entry.scale));
        }
        if found {
            result.keyboard = Some(keyboard);
        }
    }

    for entry in CONTROLLER_BINDINGS {
        let Some(value) = values.get(entry.key) else {
            continue;
        };
        let parsed = native_input(value);
        if parsed == NativeInput::Invalid {
            result.warnings.push(LegacyWarning::UnknownControllerInput {
                key: entry.key.to_owned(),
                value: value.clone(),
            });
            continue;
        }
        let control = control_for_button(entry.pad_button);
        let list = result.controller_fallback.entry(control).or_default();
        match parsed {
            NativeInput::Button(button) => {
                list.push(Binding::new(
                    PhysicalInput::new(Control::GamepadButton(button)),
                    control,
                    1.0,
                ));
            }
            NativeInput::Axis(axis) => {
                list.push(Binding::new(
                    PhysicalInput::new(Control::GamepadAxis(axis, AxisDirection::Positive)),
                    control,
                    1.0,
                ));
            }
            // `none` (or a rejected value, already skipped) leaves the control
            // present with no bindings: an explicit unbind.
            NativeInput::None | NativeInput::Invalid => {}
        }

        if matches!(entry.pad_button, PadButton::L | PadButton::R) {
            let axis = match entry.pad_button {
                PadButton::L => ControlId::TriggerL,
                _ => ControlId::TriggerR,
            };
            let analog: Vec<Binding> = list
                .iter()
                .map(|binding| Binding {
                    target: axis,
                    ..binding.clone()
                })
                .collect();
            result.controller_fallback.insert(axis, analog);
        }
    }

    result
}

/// [`prepare`] straight from INI text.
pub fn prepare_from_ini(text: &str, precedence: Precedence) -> LegacyInput {
    prepare(&parse_ini(text), precedence)
}

/// Outcome of [`apply_keyboard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyKeyboard {
    /// Nothing was prepared (no legacy keyboard data).
    NotPrepared,
    /// Every control was stored; the caller must persist the profile and only
    /// then forget the prepared data (`save_profiles()` upstream).
    Applied,
    /// The store rejected at least one control. Upstream has already written the
    /// controls before the failure, so partial state is possible and the prepared
    /// data is kept for a retry.
    Rejected,
}

/// `apply_legacy_keyboard_input()`.
///
/// Installs the prepared lists on `target` and clears the preparation only after
/// every control was stored. Persistence is the adapter's job: upstream resets
/// the prepared map only when `save_profiles()` also succeeds, so a caller must
/// persist between this call and treating the migration as complete.
pub fn apply_keyboard<S: ProfileStore + ?Sized>(
    legacy: &mut LegacyInput,
    store: &mut S,
    target: ProfileTarget,
) -> ApplyKeyboard {
    let Some(keyboard) = legacy.keyboard.as_ref() else {
        return ApplyKeyboard::NotPrepared;
    };

    let mut applied = true;
    for (control, bindings) in keyboard {
        applied = store.set_binding(target, *control, bindings.clone()) && applied;
    }
    if !applied {
        return ApplyKeyboard::Rejected;
    }

    legacy.keyboard = None;
    ApplyKeyboard::Applied
}

/// `apply_legacy_controller_input()`, for one controller profile.
///
/// Writes each fallback list whose control the profile does not already name.
/// Returns whether anything changed. Upstream calls this once per source, only
/// while the source is not already in its imported set, and treats "changed but
/// unsaved" as not imported — both are caller state.
pub fn apply_controller<S: ProfileStore + ?Sized>(
    legacy: &LegacyInput,
    store: &mut S,
    source: crate::binding::SourceId,
) -> bool {
    if source == crate::binding::INVALID_SOURCE {
        return false;
    }
    let target = ProfileTarget::controller(source);
    let mut changed = false;
    for (control, bindings) in &legacy.controller_fallback {
        if !store.has_stored_control(target, *control) {
            changed = store.set_binding(target, *control, bindings.clone()) || changed;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn parse_ini_trims_skips_comments_and_last_key_wins() {
        let text = "\
; comment
# comment too
[Section]
  A = space\x20\x20
no_equals_here
= no key
B =
C = first
C = second
DPad_Up=dpup
";
        let values = parse_ini(text);
        assert_eq!(
            values,
            map(&[("A", "space"), ("C", "second"), ("DPad_Up", "dpup")])
        );
    }

    #[test]
    fn parse_ini_handles_crlf_and_missing_trailing_newline() {
        let values = parse_ini("A = space\r\nB = lshift\r\n");
        assert_eq!(values, map(&[("A", "space"), ("B", "lshift")]));
        assert_eq!(parse_ini("A = space"), map(&[("A", "space")]));
    }

    #[test]
    fn malformed_ini_entries_are_ignored_without_creating_mappings() {
        let legacy = prepare_from_ini(
            "not an assignment\n=missing-key\nA=\nPad_A=\n[broken section\nPad_Z=bogus\n",
            Precedence::default(),
        );
        assert!(!legacy.keyboard_pending());
        assert!(legacy.controller_fallback.is_empty());
        assert_eq!(
            legacy.warnings,
            vec![LegacyWarning::UnknownControllerInput {
                key: "Pad_Z".to_owned(),
                value: "bogus".to_owned(),
            }]
        );
    }

    #[test]
    fn keyboard_input_maps_mouse_names_then_scancodes() {
        assert_eq!(
            keyboard_input("mouse1"),
            Some(PhysicalInput::new(Control::MouseButton(MouseButton::LEFT)))
        );
        assert_eq!(
            keyboard_input("Mouse2"),
            Some(PhysicalInput::new(Control::MouseButton(MouseButton::RIGHT)))
        );
        assert_eq!(
            keyboard_input("MOUSE3"),
            Some(PhysicalInput::new(Control::MouseButton(
                MouseButton::MIDDLE
            )))
        );
        assert_eq!(
            keyboard_input("space"),
            Some(PhysicalInput::new(Control::Key(Scancode::SPACE)))
        );
        assert_eq!(
            keyboard_input("Left Shift"),
            Some(PhysicalInput::new(Control::Key(Scancode::LSHIFT)))
        );
        assert_eq!(keyboard_input("not-a-key"), None);
        assert_eq!(keyboard_input(""), None);
    }

    #[test]
    fn native_input_maps_buttons_axes_and_none() {
        assert_eq!(native_input("NONE"), NativeInput::None);
        assert_eq!(native_input("a"), NativeInput::Button(GamepadButton::South));
        assert_eq!(
            native_input("dpup"),
            NativeInput::Button(GamepadButton::DPadUp)
        );
        assert_eq!(
            native_input("lefttrigger"),
            NativeInput::Axis(GamepadAxis::LeftTrigger)
        );
        assert_eq!(
            native_input("righttrigger"),
            NativeInput::Axis(GamepadAxis::RightTrigger)
        );
        assert_eq!(native_input("trigger"), NativeInput::Invalid);
        assert_eq!(native_input(""), NativeInput::Invalid);
    }

    #[test]
    fn prepare_without_legacy_keys_prepares_nothing() {
        let legacy = prepare(&map(&[("Pad_A", "a")]), Precedence::default());
        assert!(!legacy.keyboard_pending());
        assert!(legacy.controller_defaults_applied());
        assert!(legacy.loaded());
        assert!(legacy.warnings.is_empty());

        let empty = prepare(&BTreeMap::new(), Precedence::default());
        assert!(!empty.loaded());
        assert!(empty.keyboard.is_none());
        assert!(empty.controller_fallback.is_empty());
    }

    #[test]
    fn prepare_installs_defaults_for_every_control_when_any_key_matches() {
        let legacy = prepare(&map(&[("A", "x")]), Precedence::default());
        let keyboard = legacy.keyboard.expect("pending");
        // Upstream stores the prepared lists in `std::map<ControlId, …>`, so
        // iteration — and therefore install order — follows the registered
        // control ids, not the `keyboard_defaults()` table order (where `Start`
        // precedes `Z`).
        let mut expected: Vec<ControlId> = crate::defaults::defaulted_controls();
        expected.sort();
        assert_eq!(keyboard.keys().copied().collect::<Vec<_>>(), expected);

        // `A` took the INI value; every other row keeps its default key.
        let a = &keyboard[&ControlId::A];
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].input.control, Control::Key(Scancode::X));
        assert_eq!(a[0].scale, 1.0);
        let b = &keyboard[&ControlId::B];
        assert_eq!(b[0].input.control, Control::Key(Scancode::LSHIFT));
        // Sticks keep both halves, positive first.
        let left_x = &keyboard[&ControlId::LeftX];
        assert_eq!(
            left_x
                .iter()
                .map(|binding| (binding.input.control, binding.scale))
                .collect::<Vec<_>>(),
            vec![
                (Control::Key(Scancode::D), 1.0),
                (Control::Key(Scancode::A), -1.0)
            ]
        );
        // The duplicate L/R rows land on the triggers too.
        assert_eq!(
            keyboard[&ControlId::TriggerL][0].input.control,
            Control::Key(Scancode::Q)
        );
        assert_eq!(
            keyboard[&ControlId::TriggerR][0].input.control,
            Control::Key(Scancode::E)
        );
    }

    #[test]
    fn prepare_treats_none_as_explicit_unbind() {
        let legacy = prepare(&map(&[("A", "none")]), Precedence::default());
        let keyboard = legacy.keyboard.expect("pending");
        assert!(
            keyboard[&ControlId::A].is_empty(),
            "A must be recorded as unbound"
        );
        assert_eq!(keyboard[&ControlId::B].len(), 1);
    }

    #[test]
    fn prepare_keeps_default_for_unknown_value_but_stays_pending() {
        let legacy = prepare(
            &map(&[("A", "definitely-not-a-key")]),
            Precedence::default(),
        );
        assert!(legacy.keyboard_pending());
        assert_eq!(
            legacy.warnings,
            vec![LegacyWarning::UnknownKey {
                legacy_key: "A".to_owned(),
                value: "definitely-not-a-key".to_owned(),
            }]
        );
        let keyboard = legacy.keyboard.expect("pending");
        assert_eq!(
            keyboard[&ControlId::A][0].input.control,
            Control::Key(Scancode::SPACE)
        );
    }

    #[test]
    fn prepare_is_skipped_when_a_keyboard_profile_already_exists() {
        let values = map(&[("A", "x")]);
        for precedence in [
            Precedence {
                existing_profiles: true,
                existing_keyboard_file: false,
            },
            Precedence {
                existing_profiles: false,
                existing_keyboard_file: true,
            },
        ] {
            assert!(precedence.existing_keyboard());
            let legacy = prepare(&values, precedence);
            assert!(!legacy.keyboard_pending());
        }
    }

    #[test]
    fn prepare_keys_are_case_sensitive_but_values_are_not() {
        let lower = prepare(&map(&[("a", "x")]), Precedence::default());
        assert!(
            !lower.keyboard_pending(),
            "lowercase key must not match row 'A'"
        );

        let upper = prepare(&map(&[("A", "SPACE")]), Precedence::default());
        let keyboard = upper.keyboard.expect("pending");
        assert_eq!(
            keyboard[&ControlId::A][0].input.control,
            Control::Key(Scancode::SPACE)
        );
    }

    #[test]
    fn prepare_controller_fallbacks_and_trigger_duplication() {
        let legacy = prepare(
            &map(&[
                ("Pad_A", "a"),
                ("Pad_L", "lefttrigger"),
                ("Pad_R", "none"),
                ("Pad_Z", "bogus"),
            ]),
            Precedence::default(),
        );

        assert_eq!(
            legacy.controller_fallback[&ControlId::A],
            vec![Binding::new(
                PhysicalInput::new(Control::GamepadButton(GamepadButton::South)),
                ControlId::A,
                1.0
            )]
        );
        assert_eq!(
            legacy.controller_fallback[&ControlId::L]
                .iter()
                .map(|binding| (binding.target, binding.input.control))
                .collect::<Vec<_>>(),
            vec![(
                ControlId::L,
                Control::GamepadAxis(GamepadAxis::LeftTrigger, AxisDirection::Positive)
            )]
        );
        assert_eq!(
            legacy.controller_fallback[&ControlId::TriggerL]
                .iter()
                .map(|binding| (binding.target, binding.input.control))
                .collect::<Vec<_>>(),
            vec![(
                ControlId::TriggerL,
                Control::GamepadAxis(GamepadAxis::LeftTrigger, AxisDirection::Positive)
            )]
        );
        // `Pad_R=none` records an explicit unbind for both R controls.
        assert!(legacy.controller_fallback[&ControlId::R].is_empty());
        assert!(legacy.controller_fallback[&ControlId::TriggerR].is_empty());
        // A rejected value writes nothing at all.
        assert!(!legacy.controller_fallback.contains_key(&ControlId::Z));
        assert_eq!(
            legacy.warnings,
            vec![LegacyWarning::UnknownControllerInput {
                key: "Pad_Z".to_owned(),
                value: "bogus".to_owned(),
            }]
        );
    }

    #[test]
    fn prepare_controller_button_names_cover_the_table() {
        for (name, button) in [
            ("b", GamepadButton::East),
            ("x", GamepadButton::West),
            ("y", GamepadButton::North),
            ("start", GamepadButton::Start),
            ("rightshoulder", GamepadButton::RightShoulder),
            ("dpright", GamepadButton::DPadRight),
        ] {
            assert_eq!(native_input(name), NativeInput::Button(button), "{name}");
        }
        assert_eq!(
            native_input("righttrigger"),
            NativeInput::Axis(GamepadAxis::RightTrigger)
        );
    }

    #[test]
    fn apply_keyboard_migrates_lists_and_clears_pending_state() {
        let mut legacy = prepare_from_ini("A=none\n", Precedence::default());
        let mut profiles = crate::memory::MemoryProfiles::with_nookery_keyboard_defaults(0);
        assert_eq!(
            apply_keyboard(&mut legacy, &mut profiles, ProfileTarget::keyboard(0)),
            ApplyKeyboard::Applied
        );
        assert!(!legacy.keyboard_pending());
        assert!(
            profiles
                .bindings_for_control(ProfileTarget::keyboard(0), ControlId::A)
                .is_empty()
        );
    }

    #[test]
    fn apply_controller_preserves_explicit_profile_unbinds() {
        let legacy = prepare_from_ini("Pad_A=a\n", Precedence::default());
        let mut profiles = crate::memory::MemoryProfiles::new();
        let target = ProfileTarget::controller(1);
        assert!(profiles.set_binding(target, ControlId::A, Vec::new()));
        assert!(!apply_controller(&legacy, &mut profiles, 1));
        assert!(
            profiles
                .bindings_for_control(target, ControlId::A)
                .is_empty()
        );
    }

    #[test]
    fn apply_controller_is_idempotent_for_late_reconnects() {
        let legacy = prepare_from_ini("Pad_A=a\nPad_L=lefttrigger\n", Precedence::default());
        let mut profiles = crate::memory::MemoryProfiles::new();
        assert!(apply_controller(&legacy, &mut profiles, 7));
        assert!(!apply_controller(&legacy, &mut profiles, 7));
        assert_eq!(
            profiles
                .bindings_for_control(ProfileTarget::controller(7), ControlId::A)
                .len(),
            1
        );
        assert_eq!(
            profiles
                .bindings_for_control(ProfileTarget::controller(7), ControlId::TriggerL)
                .len(),
            1
        );
    }
}
