//! Deterministic controller-port and PAD runtime behavior.
//!
//! This is the pure runtime edge of the PC input port. It keeps Aurora-style
//! bindings as the input contract, then applies the button masks, stick
//! clamping, trigger thresholds, hotplug state, and error behavior consumed by
//! `padmgr.c`. Platform polling and Aurora event registration remain adapter
//! work; [`ControllerSample`] is the bounded hand-off for those adapters.

use std::collections::BTreeMap;
use std::fmt;

use crate::binding::{Binding, ProfileTarget, SourceId, same_input, valid_binding};
use crate::control::{ControlId, ControlKind};
use crate::sdl::{AxisDirection, Control, GamepadAxis, GamepadButton};

/// Number of controller channels in Dolphin's PAD ABI.
pub const PAD_MAX_CONTROLLERS: usize = 4;
/// PAD channel 0, the single player port used by the PC controller manager.
pub const PAD0: usize = 0;

/// Exact GameCube PAD button positions from `dolphin/pad.h`.
pub const PAD_BUTTON_LEFT: u16 = 0x0001;
pub const PAD_BUTTON_RIGHT: u16 = 0x0002;
pub const PAD_BUTTON_DOWN: u16 = 0x0004;
pub const PAD_BUTTON_UP: u16 = 0x0008;
pub const PAD_TRIGGER_Z: u16 = 0x0010;
pub const PAD_TRIGGER_R: u16 = 0x0020;
pub const PAD_TRIGGER_L: u16 = 0x0040;
pub const PAD_BUTTON_A: u16 = 0x0100;
pub const PAD_BUTTON_B: u16 = 0x0200;
pub const PAD_BUTTON_X: u16 = 0x0400;
pub const PAD_BUTTON_Y: u16 = 0x0800;
pub const PAD_BUTTON_START: u16 = 0x1000;

/// Exact `PADStatus.err` values from `dolphin/pad.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadError {
    /// `PAD_ERR_NONE`.
    None,
    /// `PAD_ERR_NO_CONTROLLER`.
    NoController,
    /// `PAD_ERR_NOT_READY`.
    NotReady,
    /// `PAD_ERR_TRANSFER`.
    Transfer,
    /// An error value not known by this port.
    Unknown(i8),
}

impl PadError {
    /// Converts the C ABI error byte without panicking on malformed input.
    pub const fn from_raw(value: i8) -> Self {
        match value {
            0 => Self::None,
            -1 => Self::NoController,
            -2 => Self::NotReady,
            -3 => Self::Transfer,
            other => Self::Unknown(other),
        }
    }

    /// Returns the C ABI error byte.
    pub const fn raw(self) -> i8 {
        match self {
            Self::None => 0,
            Self::NoController => -1,
            Self::NotReady => -2,
            Self::Transfer => -3,
            Self::Unknown(value) => value,
        }
    }

    const fn is_ok(self) -> bool {
        matches!(self, Self::None)
    }
}

/// The values returned to the game in the GameCube `PADStatus` layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PadStatus {
    /// GC button bit mask.
    pub button: u16,
    /// Main stick X/Y, after the original SDK clamp.
    pub stick_x: i8,
    pub stick_y: i8,
    /// C-stick X/Y, after the original SDK clamp.
    pub substick_x: i8,
    pub substick_y: i8,
    /// Analog trigger values in the PAD ABI range.
    pub trigger_left: u8,
    pub trigger_right: u8,
    /// Analog A/B values, unused by this runtime slice.
    pub analog_a: u8,
    pub analog_b: u8,
    /// PAD error byte (`PADStatus.err`).
    pub err: i8,
}

impl PadStatus {
    /// A zeroed status carrying the requested error, matching padmgr's failure
    /// path for no controller and failed transfers.
    pub const fn zero(err: PadError) -> Self {
        Self {
            button: 0,
            stick_x: 0,
            stick_y: 0,
            substick_x: 0,
            substick_y: 0,
            trigger_left: 0,
            trigger_right: 0,
            analog_a: 0,
            analog_b: 0,
            err: err.raw(),
        }
    }

    /// Interprets the C error byte without exposing invalid enum values.
    pub const fn error(self) -> PadError {
        PadError::from_raw(self.err)
    }

    /// Returns the N64 button bits synthesized by `padmgr_UpdatePC` from this
    /// status. C-stick directions use the exact inclusive ±29 thresholds.
    pub const fn n64_button_mask(self) -> u16 {
        let mut result = 0;
        if self.button & PAD_BUTTON_A != 0 {
            result |= 0x8000;
        }
        if self.button & PAD_BUTTON_B != 0 {
            result |= 0x4000;
        }
        if self.button & PAD_BUTTON_X != 0 {
            result |= 0x0040;
        }
        if self.button & PAD_BUTTON_Y != 0 {
            result |= 0x0080;
        }
        if self.button & PAD_BUTTON_START != 0 {
            result |= 0x1000;
        }
        if self.button & PAD_TRIGGER_Z != 0 {
            result |= 0x2000;
        }
        if self.button & PAD_TRIGGER_R != 0 {
            result |= 0x0010;
        }
        if self.button & PAD_TRIGGER_L != 0 {
            result |= 0x0020;
        }
        if self.button & PAD_BUTTON_UP != 0 {
            result |= 0x0800;
        }
        if self.button & PAD_BUTTON_DOWN != 0 {
            result |= 0x0400;
        }
        if self.button & PAD_BUTTON_LEFT != 0 {
            result |= 0x0200;
        }
        if self.button & PAD_BUTTON_RIGHT != 0 {
            result |= 0x0100;
        }
        if self.substick_x >= 29 {
            result |= 0x0001;
        }
        if self.substick_x <= -29 {
            result |= 0x0002;
        }
        if self.substick_y >= 29 {
            result |= 0x0008;
        }
        if self.substick_y <= -29 {
            result |= 0x0004;
        }
        result
    }
}

/// A host controller sample supplied to the pure runtime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControllerSample {
    /// Aurora source identity.
    pub source: SourceId,
    /// Transport status for this sample.
    pub error: PadError,
    /// SDL gamepad button bits, indexed by [`GamepadButton::value`].
    pub buttons: u32,
    /// SDL normalized axes in `[-1, 1]`; malformed values are sanitized when
    /// evaluated rather than indexing or unwrapping external data.
    pub axes: [f32; GamepadAxis::COUNT as usize],
}

impl ControllerSample {
    /// Creates a successful sample with no controls active.
    pub const fn empty(source: SourceId) -> Self {
        Self {
            source,
            error: PadError::None,
            buttons: 0,
            axes: [0.0; GamepadAxis::COUNT as usize],
        }
    }

    /// Sets one gamepad button without exposing a shift operation to callers.
    pub fn with_button(mut self, button: GamepadButton, pressed: bool) -> Self {
        let mask = 1_u32 << button.value();
        if pressed {
            self.buttons |= mask;
        } else {
            self.buttons &= !mask;
        }
        self
    }

    /// Sets an axis if it is a recognized SDL axis.
    pub fn with_axis(mut self, axis: GamepadAxis, value: f32) -> Self {
        if let Some(slot) = self.axes.get_mut(axis.value() as usize) {
            *slot = value;
        }
        self
    }

    fn axis(self, axis: GamepadAxis) -> f32 {
        self.axes
            .get(axis.value() as usize)
            .copied()
            .filter(|value| value.is_finite())
            .map_or(0.0, |value| value.clamp(-1.0, 1.0))
    }
}

/// Whether a controller port is allowed to claim a source automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortAssignment {
    /// Wait for a physical button press before assigning a source.
    Auto,
    /// Keep the port unassigned.
    None,
    /// Use one explicit source identity.
    Controller(SourceId),
}

/// A controller port's assignment and hotplug state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerPort {
    number: u8,
    assignment: PortAssignment,
    status: PortStatus,
}

/// The runtime-visible connection state of an assigned port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    /// No controller is currently connected to the port.
    Disconnected,
    /// A controller is connected and available for input.
    Connected,
}

impl ControllerPort {
    /// Creates an automatic, disconnected port.
    pub const fn new(number: u8) -> Self {
        Self {
            number,
            assignment: PortAssignment::Auto,
            status: PortStatus::Disconnected,
        }
    }

    /// Returns the port number.
    pub const fn number(self) -> u8 {
        self.number
    }

    /// Returns the current preference.
    pub const fn assignment(self) -> PortAssignment {
        self.assignment
    }

    /// Returns the explicit source, if assigned.
    pub const fn source(self) -> Option<SourceId> {
        match self.assignment {
            PortAssignment::Controller(source) => Some(source),
            PortAssignment::Auto | PortAssignment::None => None,
        }
    }

    /// Returns the current connection status.
    pub const fn status(self) -> PortStatus {
        self.status
    }

    const fn with_assignment(mut self, assignment: PortAssignment) -> Self {
        self.assignment = assignment;
        self.status = PortStatus::Disconnected;
        self
    }

    /// Assigns a source and marks the port connected.
    pub const fn connect(mut self, source: SourceId) -> Self {
        self.assignment = PortAssignment::Controller(source);
        self.status = PortStatus::Connected;
        self
    }

    /// Marks the port disconnected while retaining its assignment.
    pub const fn disconnect(mut self) -> Self {
        self.status = PortStatus::Disconnected;
        self
    }
}

/// Runtime errors that are recoverable at a platform boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A port index was outside `[0, PAD_MAX_CONTROLLERS)`.
    InvalidPort(usize),
    /// Source zero is Aurora's unresolved/invalid source ID.
    InvalidSource(SourceId),
    /// Two ports attempted to retain the same source.
    DuplicateSource(SourceId),
    /// A binding did not satisfy the existing Aurora profile contract.
    InvalidBinding { index: usize },
    /// A persisted snapshot was malformed.
    Snapshot(SnapshotError),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPort(port) => write!(formatter, "invalid controller port {port}"),
            Self::InvalidSource(source) => write!(formatter, "invalid controller source {source}"),
            Self::DuplicateSource(source) => {
                write!(formatter, "controller source {source} is already assigned")
            }
            Self::InvalidBinding { index } => {
                write!(formatter, "invalid controller binding {index}")
            }
            Self::Snapshot(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Bounded persistence failures. Unknown fields are rejected so malformed
/// configuration cannot be silently discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// The serialized snapshot exceeded the runtime bound.
    TooLarge,
    /// A required field was absent.
    MissingField(String),
    /// A key appeared more than once.
    DuplicateKey(String),
    /// The key is not part of this versioned format.
    UnknownKey(String),
    /// A line had no valid `key=value` form.
    MalformedLine,
    /// A known value was not valid for its field.
    InvalidValue(String),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("controller snapshot is too large"),
            Self::MissingField(key) => write!(formatter, "controller snapshot misses {key}"),
            Self::DuplicateKey(key) => write!(formatter, "controller snapshot duplicates {key}"),
            Self::UnknownKey(key) => write!(formatter, "unknown controller snapshot key {key}"),
            Self::MalformedLine => formatter.write_str("malformed controller snapshot line"),
            Self::InvalidValue(key) => {
                write!(formatter, "invalid controller snapshot value for {key}")
            }
        }
    }
}

impl std::error::Error for SnapshotError {}

const MAX_SNAPSHOT_BYTES: usize = 4096;
const MAX_SNAPSHOT_LINE: usize = 128;

/// A single-port controller runtime with persistent preferences and source
/// profiles. Ports 1..3 are retained only to preserve PAD's four-channel shape;
/// [`Self::enforce_single_port`] clears their assignments like the C++ owner.
#[derive(Debug, Clone, PartialEq)]
pub struct ControllerRuntime {
    ports: [ControllerPort; PAD_MAX_CONTROLLERS],
    auto_assign_controller: bool,
    profiles: BTreeMap<SourceId, Vec<Binding>>,
}

impl Default for ControllerRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ControllerRuntime {
    /// Creates the desktop-default runtime: automatic assignment enabled and
    /// no connected source.
    pub const fn new() -> Self {
        Self {
            ports: [
                ControllerPort::new(0),
                ControllerPort::new(1),
                ControllerPort::new(2),
                ControllerPort::new(3),
            ],
            auto_assign_controller: true,
            profiles: BTreeMap::new(),
        }
    }

    /// Returns a port without indexing through untrusted input.
    pub fn port(&self, port: usize) -> Result<ControllerPort, RuntimeError> {
        self.ports
            .get(port)
            .copied()
            .ok_or(RuntimeError::InvalidPort(port))
    }

    /// Returns whether automatic button-press assignment is enabled.
    pub const fn auto_assign(&self) -> bool {
        self.auto_assign_controller
    }

    /// Clears every secondary PAD port, matching `enforce_single_port()`.
    pub fn enforce_single_port(&mut self) {
        for port in self.ports.iter_mut().skip(1) {
            *port = port.with_assignment(PortAssignment::None);
        }
    }

    /// Changes a port preference, rejecting invalid ports, sources, and
    /// duplicate explicit assignments.
    pub fn set_assignment(
        &mut self,
        port: usize,
        assignment: PortAssignment,
    ) -> Result<(), RuntimeError> {
        if self.ports.get(port).is_none() {
            return Err(RuntimeError::InvalidPort(port));
        }
        if let PortAssignment::Controller(source) = assignment {
            if source == 0 {
                return Err(RuntimeError::InvalidSource(source));
            }
            if self
                .ports
                .iter()
                .enumerate()
                .any(|(index, current)| index != port && current.source() == Some(source))
            {
                return Err(RuntimeError::DuplicateSource(source));
            }
        }
        if let Some(current) = self.ports.get_mut(port) {
            *current = current.with_assignment(assignment);
        }
        Ok(())
    }

    /// Selects automatic assignment and clears the active controller.
    pub fn select_automatic(&mut self) {
        self.auto_assign_controller = true;
        let _ = self.set_assignment(PAD0, PortAssignment::Auto);
    }

    /// Selects no automatic controller assignment and clears the active port.
    pub fn select_none(&mut self) {
        self.auto_assign_controller = false;
        let _ = self.set_assignment(PAD0, PortAssignment::None);
    }

    /// Selects an explicit controller and enables the setting used by the C++
    /// UI after an explicit selection.
    pub fn select_controller(&mut self, source: SourceId) -> Result<(), RuntimeError> {
        self.set_assignment(PAD0, PortAssignment::Controller(source))?;
        self.auto_assign_controller = true;
        if let Some(port) = self.ports.get_mut(PAD0) {
            port.status = PortStatus::Connected;
        }
        Ok(())
    }

    /// Assigns the first physical button press when automatic assignment is
    /// enabled. Non-button activity never claims the port.
    pub fn button_press(&mut self, source: SourceId) -> Result<bool, RuntimeError> {
        if source == 0 {
            return Err(RuntimeError::InvalidSource(source));
        }
        let current = self.port(PAD0)?;
        if !self.auto_assign_controller || current.source().is_some() {
            return Ok(false);
        }
        self.set_assignment(PAD0, PortAssignment::Controller(source))?;
        if let Some(port) = self.ports.get_mut(PAD0) {
            port.status = PortStatus::Connected;
        }
        Ok(true)
    }

    /// Marks the assigned source connected after a successful hotplug/sample
    /// event. An unassigned source is ignored, as Aurora does.
    pub fn reconnect(&mut self, source: SourceId) -> Result<bool, RuntimeError> {
        if source == 0 {
            return Err(RuntimeError::InvalidSource(source));
        }
        let mut changed = false;
        for port in &mut self.ports {
            if port.source() == Some(source) {
                port.status = PortStatus::Connected;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Marks the assigned source disconnected while retaining its preference,
    /// allowing the same device identity to reconnect to the same port.
    pub fn disconnect(&mut self, source: SourceId) -> Result<bool, RuntimeError> {
        if source == 0 {
            return Err(RuntimeError::InvalidSource(source));
        }
        let mut changed = false;
        for port in &mut self.ports {
            if port.source() == Some(source) {
                port.status = PortStatus::Disconnected;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Installs a source profile using the existing Aurora binding validity
    /// contract. Empty lists are preserved as explicit unbinds.
    pub fn set_bindings(
        &mut self,
        source: SourceId,
        mut bindings: Vec<Binding>,
    ) -> Result<(), RuntimeError> {
        if source == 0 {
            return Err(RuntimeError::InvalidSource(source));
        }
        let target = ProfileTarget::controller(source);
        for (index, binding) in bindings.iter_mut().enumerate() {
            if !valid_binding(target, binding) {
                return Err(RuntimeError::InvalidBinding { index });
            }
            binding.input.source = 0;
            for held in &mut binding.held {
                held.input.source = 0;
            }
        }
        self.profiles.insert(source, bindings);
        Ok(())
    }

    /// Returns duplicate physical inputs in a profile without rejecting
    /// intentional allow-policy bindings.
    pub fn duplicate_bindings(&self, source: SourceId) -> Vec<(usize, usize)> {
        let Some(bindings) = self.profiles.get(&source) else {
            return Vec::new();
        };
        let mut duplicates = Vec::new();
        for (index, binding) in bindings.iter().enumerate() {
            for (other, previous) in bindings.iter().enumerate().take(index) {
                if same_input(binding.input, previous.input) {
                    duplicates.push((other, index));
                }
            }
        }
        duplicates
    }

    /// Evaluates the assigned source into the exact GC-facing PAD status.
    pub fn status(
        &mut self,
        port: usize,
        sample: ControllerSample,
    ) -> Result<PadStatus, RuntimeError> {
        let current = self.port(port)?;
        let Some(source) = current.source() else {
            return Ok(PadStatus::zero(PadError::NoController));
        };
        if current.status() != PortStatus::Connected {
            return Ok(PadStatus::zero(PadError::NoController));
        }
        if source != sample.source {
            return Ok(PadStatus::zero(PadError::NoController));
        }
        if !sample.error.is_ok() {
            if matches!(sample.error, PadError::NoController) {
                let _ = self.disconnect(source);
            }
            return Ok(PadStatus::zero(sample.error));
        }
        if let Some(active) = self.ports.get_mut(port) {
            active.status = PortStatus::Connected;
        }

        let bindings = self
            .profiles
            .get(&source)
            .map_or_else(default_controller_bindings, Clone::clone);
        Ok(evaluate_bindings(&bindings, sample))
    }

    /// Serializes assignments and the two C++ setting values in a bounded,
    /// deterministic format. Profile JSON remains owned by the profile store;
    /// this snapshot is the controller-port persistence boundary.
    pub fn serialize(&self) -> String {
        let mut result = String::from("version=1\nauto_assign=");
        result.push_str(if self.auto_assign_controller {
            "true"
        } else {
            "false"
        });
        result.push('\n');
        for (index, port) in self.ports.iter().enumerate() {
            result.push_str("port");
            result.push_str(&index.to_string());
            result.push('=');
            match port.assignment() {
                PortAssignment::Auto => result.push_str("auto"),
                PortAssignment::None => result.push_str("none"),
                PortAssignment::Controller(source) => {
                    result.push_str("controller:");
                    result.push_str(&source.to_string());
                }
            }
            result.push('\n');
        }
        result
    }

    /// Restores a bounded snapshot without panicking or silently ignoring
    /// unknown/malformed fields.
    pub fn deserialize(serialized: &str) -> Result<Self, RuntimeError> {
        if serialized.len() > MAX_SNAPSHOT_BYTES {
            return Err(RuntimeError::Snapshot(SnapshotError::TooLarge));
        }
        let mut version = None;
        let mut auto_assign = None;
        let mut assignments: [Option<PortAssignment>; PAD_MAX_CONTROLLERS] =
            [None; PAD_MAX_CONTROLLERS];
        for line in serialized.lines() {
            if line.len() > MAX_SNAPSHOT_LINE {
                return Err(RuntimeError::Snapshot(SnapshotError::MalformedLine));
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(RuntimeError::Snapshot(SnapshotError::MalformedLine));
            };
            if key.is_empty() || value.is_empty() || value.contains('=') {
                return Err(RuntimeError::Snapshot(SnapshotError::MalformedLine));
            }
            match key {
                "version" => {
                    if version.replace(value).is_some() {
                        return Err(RuntimeError::Snapshot(SnapshotError::DuplicateKey(
                            key.to_owned(),
                        )));
                    }
                }
                "auto_assign" => {
                    if auto_assign.replace(value).is_some() {
                        return Err(RuntimeError::Snapshot(SnapshotError::DuplicateKey(
                            key.to_owned(),
                        )));
                    }
                }
                _ if key.starts_with("port") => {
                    let Some(index) = key
                        .strip_prefix("port")
                        .and_then(|value| value.parse::<usize>().ok())
                    else {
                        return Err(RuntimeError::Snapshot(SnapshotError::UnknownKey(
                            key.to_owned(),
                        )));
                    };
                    let Some(slot) = assignments.get_mut(index) else {
                        return Err(RuntimeError::Snapshot(SnapshotError::UnknownKey(
                            key.to_owned(),
                        )));
                    };
                    if slot.is_some() {
                        return Err(RuntimeError::Snapshot(SnapshotError::DuplicateKey(
                            key.to_owned(),
                        )));
                    }
                    *slot = Some(parse_assignment(value).map_err(RuntimeError::Snapshot)?);
                }
                _ => {
                    return Err(RuntimeError::Snapshot(SnapshotError::UnknownKey(
                        key.to_owned(),
                    )));
                }
            }
        }
        if version != Some("1") {
            return Err(RuntimeError::Snapshot(SnapshotError::InvalidValue(
                "version".to_owned(),
            )));
        }
        let auto_assign = match auto_assign {
            Some("true") => true,
            Some("false") => false,
            Some(_) => {
                return Err(RuntimeError::Snapshot(SnapshotError::InvalidValue(
                    "auto_assign".to_owned(),
                )));
            }
            None => {
                return Err(RuntimeError::Snapshot(SnapshotError::MissingField(
                    "auto_assign".to_owned(),
                )));
            }
        };
        let mut runtime = Self::new();
        runtime.auto_assign_controller = auto_assign;
        for (index, assignment) in assignments.into_iter().enumerate() {
            let Some(assignment) = assignment else {
                return Err(RuntimeError::Snapshot(SnapshotError::MissingField(
                    format!("port{index}"),
                )));
            };
            runtime.set_assignment(index, assignment).map_err(|error| {
                RuntimeError::Snapshot(SnapshotError::InvalidValue(error.to_string()))
            })?;
        }
        Ok(runtime)
    }
}

fn parse_assignment(value: &str) -> Result<PortAssignment, SnapshotError> {
    match value {
        "auto" => Ok(PortAssignment::Auto),
        "none" => Ok(PortAssignment::None),
        value if value.starts_with("controller:") => {
            let source = value
                .strip_prefix("controller:")
                .filter(|value| !value.is_empty())
                .and_then(|value| value.parse::<SourceId>().ok())
                .filter(|source| *source != 0)
                .ok_or_else(|| SnapshotError::InvalidValue("controller".to_owned()))?;
            Ok(PortAssignment::Controller(source))
        }
        _ => Err(SnapshotError::InvalidValue("port".to_owned())),
    }
}

fn default_controller_bindings() -> Vec<Binding> {
    let mut bindings = Vec::with_capacity(18);
    let buttons = [
        (GamepadButton::South, ControlId::A),
        (GamepadButton::East, ControlId::B),
        (GamepadButton::West, ControlId::X),
        (GamepadButton::North, ControlId::Y),
        (GamepadButton::Start, ControlId::Start),
        (GamepadButton::RightShoulder, ControlId::Z),
        (GamepadButton::DPadUp, ControlId::Up),
        (GamepadButton::DPadDown, ControlId::Down),
        (GamepadButton::DPadLeft, ControlId::Left),
        (GamepadButton::DPadRight, ControlId::Right),
    ];
    for (button, target) in buttons {
        bindings.push(Binding::new(
            crate::binding::PhysicalInput::new(Control::GamepadButton(button)),
            target,
            1.0,
        ));
    }
    let axes = [
        (GamepadAxis::LeftX, ControlId::LeftX, AxisDirection::Full),
        (GamepadAxis::LeftY, ControlId::LeftY, AxisDirection::Full),
        (GamepadAxis::RightX, ControlId::RightX, AxisDirection::Full),
        (GamepadAxis::RightY, ControlId::RightY, AxisDirection::Full),
        (
            GamepadAxis::LeftTrigger,
            ControlId::L,
            AxisDirection::Positive,
        ),
        (
            GamepadAxis::RightTrigger,
            ControlId::R,
            AxisDirection::Positive,
        ),
        (
            GamepadAxis::LeftTrigger,
            ControlId::TriggerL,
            AxisDirection::Positive,
        ),
        (
            GamepadAxis::RightTrigger,
            ControlId::TriggerR,
            AxisDirection::Positive,
        ),
    ];
    for (axis, target, direction) in axes {
        bindings.push(Binding::new(
            crate::binding::PhysicalInput::new(Control::GamepadAxis(axis, direction)),
            target,
            1.0,
        ));
    }
    bindings
}

fn evaluate_bindings(bindings: &[Binding], sample: ControllerSample) -> PadStatus {
    let mut values = [0.0_f32; 33];
    let mut digital = [false; 33];
    for binding in bindings {
        let index = binding.target as usize;
        let Some(slot) = values.get_mut(index) else {
            continue;
        };
        let value = binding_value(binding, sample);
        if binding.target.kind() == ControlKind::Button {
            if value >= binding.threshold {
                if let Some(active) = digital.get_mut(index) {
                    *active = true;
                }
            }
        } else if value.abs() > slot.abs() {
            *slot = value;
        }
    }

    let button = digital_mask(&digital, ControlId::A, PAD_BUTTON_A)
        | digital_mask(&digital, ControlId::B, PAD_BUTTON_B)
        | digital_mask(&digital, ControlId::X, PAD_BUTTON_X)
        | digital_mask(&digital, ControlId::Y, PAD_BUTTON_Y)
        | digital_mask(&digital, ControlId::Start, PAD_BUTTON_START)
        | digital_mask(&digital, ControlId::Z, PAD_TRIGGER_Z)
        | digital_mask(&digital, ControlId::L, PAD_TRIGGER_L)
        | digital_mask(&digital, ControlId::R, PAD_TRIGGER_R)
        | digital_mask(&digital, ControlId::Up, PAD_BUTTON_UP)
        | digital_mask(&digital, ControlId::Down, PAD_BUTTON_DOWN)
        | digital_mask(&digital, ControlId::Left, PAD_BUTTON_LEFT)
        | digital_mask(&digital, ControlId::Right, PAD_BUTTON_RIGHT);

    let (stick_x, stick_y) =
        scaled_axis_pair(values, ControlId::LeftX, ControlId::LeftY, 72, 40, 0);
    let (substick_x, substick_y) =
        scaled_axis_pair(values, ControlId::RightX, ControlId::RightY, 59, 31, 0);
    PadStatus {
        button,
        stick_x,
        stick_y,
        substick_x,
        substick_y,
        trigger_left: trigger_value(values, ControlId::TriggerL),
        trigger_right: trigger_value(values, ControlId::TriggerR),
        analog_a: 0,
        analog_b: 0,
        err: PadError::None.raw(),
    }
}

fn binding_value(binding: &Binding, sample: ControllerSample) -> f32 {
    if binding
        .held
        .iter()
        .any(|held| input_value(held.input.control, sample) < held.threshold)
    {
        return 0.0;
    }
    let raw = input_value(binding.input.control, sample);
    apply_deadzone(raw, binding.dead_zone) * binding.scale
}

fn input_value(control: Control, sample: ControllerSample) -> f32 {
    match control {
        Control::GamepadButton(button) => {
            if sample.buttons & (1_u32 << button.value()) != 0 {
                1.0
            } else {
                0.0
            }
        }
        Control::GamepadAxis(axis, direction) => direction.select(sample.axis(axis)),
        Control::Key(_) | Control::MouseButton(_) => 0.0,
    }
}

/// Applies the Aurora binding deadzone with an inclusive boundary. The C PAD
/// clamp subtracts the deadzone from values beyond it; the same bounded math
/// is used here for normalized controller values.
pub fn apply_deadzone(value: f32, deadzone: f32) -> f32 {
    if !value.is_finite() || !deadzone.is_finite() {
        return 0.0;
    }
    let value = value.clamp(-1.0, 1.0);
    let deadzone = deadzone.clamp(0.0, 1.0);
    let magnitude = value.abs();
    if magnitude <= deadzone {
        0.0
    } else {
        value.signum() * (magnitude - deadzone).min(1.0)
    }
}

fn digital_mask(values: &[bool; 33], control: ControlId, mask: u16) -> u16 {
    values
        .get(control as usize)
        .copied()
        .map_or(0, |active| if active { mask } else { 0 })
}

fn trigger_value(values: [f32; 33], control: ControlId) -> u8 {
    values
        .get(control as usize)
        .copied()
        .filter(|value| value.is_finite())
        .map_or(0, |value| {
            let raw = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            if raw <= 30 { 0 } else { raw.min(180) - 30 }
        })
}

fn scaled_axis_pair(
    values: [f32; 33],
    x_control: ControlId,
    y_control: ControlId,
    max: i32,
    xy: i32,
    min: i32,
) -> (i8, i8) {
    let normalized = |control: ControlId| {
        values
            .get(control as usize)
            .copied()
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0)
    };
    let x = (normalized(x_control) * 127.0).round() as i32;
    let y = (normalized(y_control) * 127.0).round() as i32;
    clamp_stick_pair(x, y, max, xy, min)
}

/// The integer `ClampStick` operation from `libs/dolphin/src/pad/Padclamp.c`.
/// It is public so differential vectors can exercise the exact boundary math.
pub fn clamp_stick_pair(x: i32, y: i32, max: i32, xy: i32, min: i32) -> (i8, i8) {
    if max <= 0 || xy <= 0 || xy > max || min < 0 {
        return (0, 0);
    }
    let sign_x = if x < 0 { -1_i64 } else { 1 };
    let sign_y = if y < 0 { -1_i64 } else { 1 };
    let mut x = i64::from(x).abs();
    let mut y = i64::from(y).abs();
    let max = i64::from(max);
    let xy = i64::from(xy);
    let min = i64::from(min);
    if x <= min {
        x = 0;
    } else {
        x -= min;
    }
    if y <= min {
        y = 0;
    } else {
        y -= min;
    }
    if x == 0 && y == 0 {
        return (0, 0);
    }
    if xy * y <= xy * x {
        let denominator = xy * x + (max - xy) * y;
        if xy * max < denominator {
            x = xy * max * x / denominator;
            y = xy * max * y / denominator;
        }
    } else {
        let denominator = xy * y + (max - xy) * x;
        if xy * max < denominator {
            x = xy * max * x / denominator;
            y = xy * max * y / denominator;
        }
    }
    (
        (sign_x * x).clamp(-128, 127) as i8,
        (sign_y * y).clamp(-128, 127) as i8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, PhysicalInput};

    fn runtime_with_source(source: SourceId) -> ControllerRuntime {
        let mut runtime = ControllerRuntime::new();
        runtime
            .select_controller(source)
            .expect("synthetic source is valid");
        runtime
    }

    #[test]
    fn assignment_and_status_transition_preserve_source() {
        let mut runtime = ControllerRuntime::new();
        assert!(runtime.button_press(42).expect("synthetic source is valid"));
        let port = runtime.port(PAD0).expect("port zero exists");
        assert_eq!(port.source(), Some(42));
        assert_eq!(port.status(), PortStatus::Connected);
        assert!(runtime.disconnect(42).expect("source was assigned"));
        assert_eq!(
            runtime.port(PAD0).expect("port zero exists").status(),
            PortStatus::Disconnected
        );
        assert!(runtime.reconnect(42).expect("source was retained"));
        assert_eq!(
            runtime.port(PAD0).expect("port zero exists").status(),
            PortStatus::Connected
        );

        runtime
            .set_assignment(1, PortAssignment::Controller(99))
            .expect("secondary assignment is valid before policy enforcement");
        runtime.enforce_single_port();
        assert_eq!(
            runtime.port(1).expect("port one exists").assignment(),
            PortAssignment::None
        );
    }

    #[test]
    fn deadzone_boundary_is_inclusive() {
        assert_eq!(apply_deadzone(0.25, 0.25), 0.0);
        assert!((apply_deadzone(0.250_001, 0.25) - 0.000_001).abs() < 0.000_000_1);
        assert_eq!(apply_deadzone(-0.25, 0.25), 0.0);
        assert!((apply_deadzone(-0.5, 0.25) + 0.25).abs() < f32::EPSILON);
        assert_eq!(apply_deadzone(2.0, 0.25), 0.75);
        assert_eq!(apply_deadzone(-2.0, -1.0), -1.0);
        assert_eq!(apply_deadzone(0.5, 2.0), 0.0);
    }

    #[test]
    fn threshold_is_zero_exactly_and_pressed_above() {
        let mut runtime = runtime_with_source(7);
        let mut binding = Binding::new(
            PhysicalInput::new(Control::GamepadAxis(
                GamepadAxis::LeftTrigger,
                AxisDirection::Positive,
            )),
            ControlId::A,
            1.0,
        );
        binding.threshold = 0.5;
        runtime
            .set_bindings(7, vec![binding])
            .expect("binding is valid");
        let exact = ControllerSample::empty(7).with_axis(GamepadAxis::LeftTrigger, 0.5);
        let status = runtime.status(PAD0, exact).expect("port zero exists");
        assert_eq!(status.button, PAD_BUTTON_A);
        assert_eq!(status.n64_button_mask(), 0x8000);
        let below = ControllerSample::empty(7).with_axis(GamepadAxis::LeftTrigger, 0.499_999);
        assert_eq!(
            runtime
                .status(PAD0, below)
                .expect("port zero exists")
                .button,
            0
        );
        let above = ControllerSample::empty(7).with_axis(GamepadAxis::LeftTrigger, 0.500_001);
        assert_eq!(
            runtime
                .status(PAD0, above)
                .expect("port zero exists")
                .button,
            PAD_BUTTON_A
        );
    }

    #[test]
    fn inverted_axis_preserves_negative_sign_and_cstick_threshold() {
        let mut runtime = runtime_with_source(8);
        let mut binding = Binding::new(
            PhysicalInput::new(Control::GamepadAxis(
                GamepadAxis::LeftX,
                AxisDirection::Positive,
            )),
            ControlId::LeftX,
            -1.0,
        );
        binding.dead_zone = 0.0;
        runtime
            .set_bindings(8, vec![binding])
            .expect("binding is valid");
        let sample = ControllerSample::empty(8).with_axis(GamepadAxis::LeftX, 0.75);
        let status = runtime.status(PAD0, sample).expect("port zero exists");
        assert!(status.stick_x < 0);
        assert_eq!(
            PadStatus {
                substick_x: 29,
                ..PadStatus::zero(PadError::None)
            }
            .n64_button_mask(),
            1
        );
        assert_eq!(
            PadStatus {
                substick_x: 28,
                ..PadStatus::zero(PadError::None)
            }
            .n64_button_mask(),
            0
        );
    }

    #[test]
    fn pad_status_vectors_match_button_axis_trigger_and_diagonal_boundaries() {
        let mut runtime = runtime_with_source(11);
        let mut left_x = Binding::new(
            PhysicalInput::new(Control::GamepadAxis(
                GamepadAxis::LeftX,
                AxisDirection::Full,
            )),
            ControlId::LeftX,
            1.0,
        );
        left_x.dead_zone = 0.25;
        let bindings = vec![
            Binding::new(
                PhysicalInput::new(Control::GamepadButton(GamepadButton::South)),
                ControlId::A,
                1.0,
            ),
            left_x.clone(),
            Binding::new(
                PhysicalInput::new(Control::GamepadAxis(
                    GamepadAxis::LeftY,
                    AxisDirection::Full,
                )),
                ControlId::LeftY,
                1.0,
            ),
            Binding::new(
                PhysicalInput::new(Control::GamepadAxis(
                    GamepadAxis::LeftTrigger,
                    AxisDirection::Positive,
                )),
                ControlId::TriggerL,
                1.0,
            ),
        ];
        runtime
            .set_bindings(11, bindings)
            .expect("source profile is valid");

        let neutral = runtime
            .status(PAD0, ControllerSample::empty(11))
            .expect("port zero exists");
        assert_eq!(neutral, PadStatus::zero(PadError::None));

        let deadzone = ControllerSample::empty(11).with_axis(GamepadAxis::LeftX, 0.25);
        assert_eq!(
            runtime
                .status(PAD0, deadzone)
                .expect("port zero exists")
                .stick_x,
            0
        );

        left_x.dead_zone = 0.0;
        runtime
            .set_bindings(
                11,
                vec![
                    Binding::new(
                        PhysicalInput::new(Control::GamepadButton(GamepadButton::South)),
                        ControlId::A,
                        1.0,
                    ),
                    left_x,
                    Binding::new(
                        PhysicalInput::new(Control::GamepadAxis(
                            GamepadAxis::LeftY,
                            AxisDirection::Full,
                        )),
                        ControlId::LeftY,
                        1.0,
                    ),
                    Binding::new(
                        PhysicalInput::new(Control::GamepadAxis(
                            GamepadAxis::LeftTrigger,
                            AxisDirection::Positive,
                        )),
                        ControlId::TriggerL,
                        1.0,
                    ),
                ],
            )
            .expect("source profile is valid");

        let full_diagonal = ControllerSample::empty(11)
            .with_button(GamepadButton::South, true)
            .with_axis(GamepadAxis::LeftX, 1.0)
            .with_axis(GamepadAxis::LeftY, 1.0)
            .with_axis(GamepadAxis::LeftTrigger, 1.0);
        let status = runtime
            .status(PAD0, full_diagonal)
            .expect("port zero exists");
        assert_eq!(status.button, PAD_BUTTON_A);
        assert_eq!((status.stick_x, status.stick_y), (40, 40));
        assert_eq!(status.trigger_left, 150);
    }

    #[test]
    fn held_input_gates_binding_without_changing_repeated_updates() {
        let mut runtime = runtime_with_source(12);
        let mut binding = Binding::new(
            PhysicalInput::new(Control::GamepadButton(GamepadButton::South)),
            ControlId::A,
            1.0,
        );
        binding.held.push(crate::binding::HeldInput {
            input: PhysicalInput::new(Control::GamepadButton(GamepadButton::Back)),
            threshold: 0.5,
        });
        runtime
            .set_bindings(12, vec![binding])
            .expect("source profile is valid");
        let pressed = ControllerSample::empty(12).with_button(GamepadButton::South, true);
        assert_eq!(
            runtime
                .status(PAD0, pressed)
                .expect("port zero exists")
                .button,
            0
        );
        let held = pressed.with_button(GamepadButton::Back, true);
        let first = runtime.status(PAD0, held).expect("port zero exists");
        let second = runtime.status(PAD0, held).expect("port zero exists");
        assert_eq!(first, second);
        assert_eq!(first.button, PAD_BUTTON_A);
    }

    #[test]
    fn disconnect_and_transport_errors_zero_status() {
        let mut runtime = runtime_with_source(9);
        let pressed = ControllerSample::empty(9).with_button(GamepadButton::South, true);
        assert_eq!(
            runtime
                .status(PAD0, pressed)
                .expect("port zero exists")
                .button,
            PAD_BUTTON_A
        );
        runtime.disconnect(9).expect("source was assigned");
        assert_eq!(
            runtime
                .status(PAD0, pressed)
                .expect("port zero exists")
                .error(),
            PadError::NoController
        );
        runtime.reconnect(9).expect("source was retained");
        let mut failed = pressed;
        failed.error = PadError::Transfer;
        let status = runtime.status(PAD0, failed).expect("port zero exists");
        assert_eq!(status, PadStatus::zero(PadError::Transfer));
        let unknown = ControllerSample {
            error: PadError::Unknown(-99),
            ..pressed
        };
        assert_eq!(
            runtime.status(PAD0, unknown).expect("port zero exists"),
            PadStatus::zero(PadError::Unknown(-99))
        );
        assert_eq!(
            runtime
                .status(PAD0, ControllerSample::empty(123))
                .expect("port zero exists"),
            PadStatus::zero(PadError::NoController)
        );
    }

    #[test]
    fn reconnect_and_reset_from_snapshot_are_deterministic() {
        let mut runtime = runtime_with_source(13);
        let sample = ControllerSample::empty(13).with_button(GamepadButton::South, true);
        let expected = runtime.status(PAD0, sample).expect("port zero exists");
        runtime.disconnect(13).expect("source was assigned");
        assert_eq!(
            runtime.status(PAD0, sample).expect("port zero exists"),
            PadStatus::zero(PadError::NoController)
        );
        runtime.reconnect(13).expect("source was retained");
        assert_eq!(
            runtime.status(PAD0, sample).expect("port zero exists"),
            expected
        );

        let restored =
            ControllerRuntime::deserialize(&runtime.serialize()).expect("valid snapshot");
        assert_eq!(
            restored,
            ControllerRuntime::deserialize(&restored.serialize()).expect("stable snapshot")
        );
        assert_eq!(
            restored.port(PAD0).expect("port zero exists").status(),
            PortStatus::Disconnected
        );
    }

    #[test]
    fn invalid_port_and_duplicate_binding_are_bounded() {
        let mut runtime = runtime_with_source(10);
        assert_eq!(
            runtime.port(PAD_MAX_CONTROLLERS),
            Err(RuntimeError::InvalidPort(4))
        );
        let binding = Binding::new(
            PhysicalInput::new(Control::GamepadButton(GamepadButton::South)),
            ControlId::A,
            1.0,
        );
        runtime
            .set_bindings(10, vec![binding.clone(), binding])
            .expect("duplicate allow policy is valid");
        assert_eq!(runtime.duplicate_bindings(10), vec![(0, 1)]);
        assert_eq!(
            runtime
                .status(
                    PAD0,
                    ControllerSample::empty(10).with_button(GamepadButton::South, true)
                )
                .expect("port zero exists")
                .button,
            PAD_BUTTON_A
        );
    }

    #[test]
    fn snapshot_round_trip_and_malformed_inputs() {
        let mut runtime = ControllerRuntime::new();
        runtime
            .select_controller(77)
            .expect("synthetic source is valid");
        let encoded = runtime.serialize();
        let restored = ControllerRuntime::deserialize(&encoded).expect("snapshot is valid");
        assert_eq!(
            restored.port(PAD0).expect("port zero exists").assignment(),
            PortAssignment::Controller(77)
        );
        assert_eq!(
            ControllerRuntime::deserialize("version=1\nauto_assign=true\nport0=wat\n"),
            Err(RuntimeError::Snapshot(SnapshotError::InvalidValue(
                "port".to_owned()
            )))
        );
        assert_eq!(
            ControllerRuntime::deserialize("version=1\nauto_assign=true\nport0=auto\nunknown=x\n"),
            Err(RuntimeError::Snapshot(SnapshotError::UnknownKey(
                "unknown".to_owned()
            )))
        );
        assert_eq!(
            ControllerRuntime::deserialize("version=1\nauto_assign=true\nport0=auto\nport0=auto\n"),
            Err(RuntimeError::Snapshot(SnapshotError::DuplicateKey(
                "port0".to_owned()
            )))
        );
    }

    #[test]
    fn focused_differential_vectors_have_stable_count() {
        let vectors = [
            (0, 0, (0, 0)),
            (29, 0, (29, 0)),
            (72, 0, (72, 0)),
            (-72, 0, (-72, 0)),
            (72, 72, (40, 40)),
        ];
        let mut matched = 0;
        for (x, y, expected) in vectors {
            assert_eq!(clamp_stick_pair(x, y, 72, 40, 0), expected);
            matched += 1;
        }
        println!("focused differential vectors: {matched}");
        assert_eq!(matched, 5);
        assert_eq!(clamp_stick_pair(i32::MIN, i32::MIN, 72, 40, 0), (-40, -40));
        assert_eq!(clamp_stick_pair(72, 72, 0, 40, 0), (0, 0));
    }
}
