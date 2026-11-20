//! Logical controls: the `aurora.pad.*` namespace Nookery binds to.
//!
//! Source of truth: `aurora::pad::controls()` in
//! `encounter/aurora@7c8c6117` `lib/input/port.cpp`, and
//! `aurora::pad::control_for_button` in the same file. Upstream hands out
//! `uint64_t` ids from a process-global registry in registration order; here the
//! ids are a closed enum in *that same order*, so the ordering and the
//! name lookup stay stable without a global.
//!
//! Scope: only the stock `aurora.pad.*` controls. Game-defined controls (e.g.
//! `dusklight.open_map`, registered through
//! `aurora::binding::register_control`) are open-ended by construction and are
//! not modelled; a port that needs them must extend this enum or add a registry
//! adapter.

/// A logical control id, ordered as upstream registers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ControlId {
    // `PADButton`-backed buttons, in `kButtons` order.
    A,
    B,
    X,
    Y,
    Z,
    Start,
    L,
    R,
    Up,
    Down,
    Left,
    Right,
    // `[-1, 1]` axes, positive right and up.
    LeftX,
    LeftY,
    RightX,
    RightY,
    // `[0, 1]` triggers.
    TriggerL,
    TriggerR,
    // `PADExtButton` bits, in `kExtButtonNames` order.
    ExtBack,
    ExtGuide,
    ExtMisc1,
    ExtMisc2,
    ExtMisc3,
    ExtMisc4,
    ExtMisc5,
    ExtMisc6,
    ExtRightPaddle1,
    ExtLeftPaddle1,
    ExtRightPaddle2,
    ExtLeftPaddle2,
    ExtRightStick,
    ExtLeftStick,
    ExtTouchpad,
}

/// `aurora::binding::ControlKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlKind {
    Button,
    Axis,
}

impl ControlId {
    /// Every control in upstream registration order.
    pub const ALL: [Self; 33] = [
        Self::A,
        Self::B,
        Self::X,
        Self::Y,
        Self::Z,
        Self::Start,
        Self::L,
        Self::R,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::LeftX,
        Self::LeftY,
        Self::RightX,
        Self::RightY,
        Self::TriggerL,
        Self::TriggerR,
        Self::ExtBack,
        Self::ExtGuide,
        Self::ExtMisc1,
        Self::ExtMisc2,
        Self::ExtMisc3,
        Self::ExtMisc4,
        Self::ExtMisc5,
        Self::ExtMisc6,
        Self::ExtRightPaddle1,
        Self::ExtLeftPaddle1,
        Self::ExtRightPaddle2,
        Self::ExtLeftPaddle2,
        Self::ExtRightStick,
        Self::ExtLeftStick,
        Self::ExtTouchpad,
    ];

    /// The registered profile name, e.g. `aurora.pad.left_x`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::A => "aurora.pad.a",
            Self::B => "aurora.pad.b",
            Self::X => "aurora.pad.x",
            Self::Y => "aurora.pad.y",
            Self::Z => "aurora.pad.z",
            Self::Start => "aurora.pad.start",
            Self::L => "aurora.pad.l",
            Self::R => "aurora.pad.r",
            Self::Up => "aurora.pad.up",
            Self::Down => "aurora.pad.down",
            Self::Left => "aurora.pad.left",
            Self::Right => "aurora.pad.right",
            Self::LeftX => "aurora.pad.left_x",
            Self::LeftY => "aurora.pad.left_y",
            Self::RightX => "aurora.pad.right_x",
            Self::RightY => "aurora.pad.right_y",
            Self::TriggerL => "aurora.pad.trigger_l",
            Self::TriggerR => "aurora.pad.trigger_r",
            Self::ExtBack => "aurora.pad.ext.back",
            Self::ExtGuide => "aurora.pad.ext.guide",
            Self::ExtMisc1 => "aurora.pad.ext.misc1",
            Self::ExtMisc2 => "aurora.pad.ext.misc2",
            Self::ExtMisc3 => "aurora.pad.ext.misc3",
            Self::ExtMisc4 => "aurora.pad.ext.misc4",
            Self::ExtMisc5 => "aurora.pad.ext.misc5",
            Self::ExtMisc6 => "aurora.pad.ext.misc6",
            Self::ExtRightPaddle1 => "aurora.pad.ext.right_paddle1",
            Self::ExtLeftPaddle1 => "aurora.pad.ext.left_paddle1",
            Self::ExtRightPaddle2 => "aurora.pad.ext.right_paddle2",
            Self::ExtLeftPaddle2 => "aurora.pad.ext.left_paddle2",
            Self::ExtRightStick => "aurora.pad.ext.right_stick",
            Self::ExtLeftStick => "aurora.pad.ext.left_stick",
            Self::ExtTouchpad => "aurora.pad.ext.touchpad",
        }
    }

    /// `ControlDescriptor::kind`.
    pub const fn kind(self) -> ControlKind {
        match self {
            Self::LeftX
            | Self::LeftY
            | Self::RightX
            | Self::RightY
            | Self::TriggerL
            | Self::TriggerR => ControlKind::Axis,
            _ => ControlKind::Button,
        }
    }

    /// `aurora::input::profiles.cpp` `group()`: the conflict bucket.
    ///
    /// Every `aurora.pad.*` name is truncated at 10 bytes, so all of them share
    /// the single bucket `aurora.pad` (extension controls included).
    pub const fn conflict_group(self) -> &'static str {
        "aurora.pad"
    }

    /// Look up a control by its registered name; `None` is `kInvalidControlId`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|control| control.name() == name)
    }
}

/// The digital L/R controls are linked to their analog trigger axes so the two
/// may share an input without being reported as a conflict.
///
/// Port of `linked()` in aurora `lib/input/profiles.cpp`.
pub fn linked(a: ControlId, b: ControlId) -> bool {
    matches!(
        (a, b),
        (ControlId::L, ControlId::TriggerL)
            | (ControlId::TriggerL, ControlId::L)
            | (ControlId::R, ControlId::TriggerR)
            | (ControlId::TriggerR, ControlId::R)
    )
}

/// The `aurora::pad::controls()` record, as constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Controls {
    pub a: ControlId,
    pub b: ControlId,
    pub x: ControlId,
    pub y: ControlId,
    pub z: ControlId,
    pub start: ControlId,
    pub l: ControlId,
    pub r: ControlId,
    pub up: ControlId,
    pub down: ControlId,
    pub left: ControlId,
    pub right: ControlId,
    pub left_x: ControlId,
    pub left_y: ControlId,
    pub right_x: ControlId,
    pub right_y: ControlId,
    pub trigger_l: ControlId,
    pub trigger_r: ControlId,
    pub ext: [ControlId; 15],
}

/// `aurora::pad::controls()`.
pub const CONTROLS: Controls = Controls {
    a: ControlId::A,
    b: ControlId::B,
    x: ControlId::X,
    y: ControlId::Y,
    z: ControlId::Z,
    start: ControlId::Start,
    l: ControlId::L,
    r: ControlId::R,
    up: ControlId::Up,
    down: ControlId::Down,
    left: ControlId::Left,
    right: ControlId::Right,
    left_x: ControlId::LeftX,
    left_y: ControlId::LeftY,
    right_x: ControlId::RightX,
    right_y: ControlId::RightY,
    trigger_l: ControlId::TriggerL,
    trigger_r: ControlId::TriggerR,
    ext: [
        ControlId::ExtBack,
        ControlId::ExtGuide,
        ControlId::ExtMisc1,
        ControlId::ExtMisc2,
        ControlId::ExtMisc3,
        ControlId::ExtMisc4,
        ControlId::ExtMisc5,
        ControlId::ExtMisc6,
        ControlId::ExtRightPaddle1,
        ControlId::ExtLeftPaddle1,
        ControlId::ExtRightPaddle2,
        ControlId::ExtLeftPaddle2,
        ControlId::ExtRightStick,
        ControlId::ExtLeftStick,
        ControlId::ExtTouchpad,
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stock_identifier_round_trips_exactly() {
        assert_eq!(ControlId::ALL.len(), 33);
        for control in ControlId::ALL {
            assert_eq!(ControlId::from_name(control.name()), Some(control));
            assert!(control.name().starts_with("aurora.pad."));
        }
        assert_eq!(ControlId::from_name("aurora.pad.missing"), None);
    }

    #[test]
    fn linked_controls_are_only_the_trigger_pairs() {
        assert!(linked(ControlId::L, ControlId::TriggerL));
        assert!(linked(ControlId::TriggerL, ControlId::L));
        assert!(linked(ControlId::R, ControlId::TriggerR));
        assert!(linked(ControlId::TriggerR, ControlId::R));
        assert!(!linked(ControlId::L, ControlId::TriggerR));
        assert!(!linked(ControlId::A, ControlId::B));
    }
}

/// The twelve `PADButton` bits that `aurora::pad::controls()` covers.
///
/// Source of truth: `kButtons` in aurora `lib/input/port.cpp` and
/// `PAD_BUTTON_*` in `dolphin/pad.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    Z,
    Start,
    L,
    R,
    Up,
    Down,
    Left,
    Right,
}

impl PadButton {
    /// `PAD_BUTTON_*` bit; `PAD_BUTTON_MENU` and `PAD_BUTTON_START` alias.
    pub const fn bit(self) -> u16 {
        match self {
            Self::Left => 0x0001,
            Self::Right => 0x0002,
            Self::Down => 0x0004,
            Self::Up => 0x0008,
            Self::Z => 0x0010,
            Self::R => 0x0020,
            Self::L => 0x0040,
            Self::A => 0x0100,
            Self::B => 0x0200,
            Self::X => 0x0400,
            Self::Y => 0x0800,
            Self::Start => 0x1000,
        }
    }
}

/// `aurora::pad::control_for_button`; `None` is `kInvalidControlId`.
pub const fn control_for_button(button: PadButton) -> ControlId {
    match button {
        PadButton::A => ControlId::A,
        PadButton::B => ControlId::B,
        PadButton::X => ControlId::X,
        PadButton::Y => ControlId::Y,
        PadButton::Z => ControlId::Z,
        PadButton::Start => ControlId::Start,
        PadButton::L => ControlId::L,
        PadButton::R => ControlId::R,
        PadButton::Up => ControlId::Up,
        PadButton::Down => ControlId::Down,
        PadButton::Left => ControlId::Left,
        PadButton::Right => ControlId::Right,
    }
}
