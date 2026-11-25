//! Source profile metadata for the no-op actor units in the first half.
//!
//! `ac_dummy.c`, `ac_boxManager.c`, `ac_boxMove.c`, and `ac_boxTrick01.c`
//! intentionally have empty lifecycle bodies.  Their profile constants are
//! still useful to callers that enumerate actors, so they are represented as
//! data while the lifecycle methods remain explicit no-ops.

/// Source actor part/category values used by the represented profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ActorPart {
    /// Background actor.
    Background = 5,
    /// Control actor.
    Control = 7,
}

/// Minimal profile metadata copied from the four empty lifecycle units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoopProfile {
    /// Profile identifier from `m_actor.h`.
    pub profile_id: u16,
    /// Actor part/category.
    pub part: ActorPart,
    /// State flags.
    pub state_flags: u32,
    /// World object/name identifier.
    pub name_id: u16,
}

/// `ac_dummy.c` profile metadata.
pub const DUMMY: NoopProfile = NoopProfile {
    profile_id: 180,
    part: ActorPart::Control,
    state_flags: (1 << 4) | (1 << 5),
    name_id: 0,
};

/// `ac_boxManager.c` profile metadata.
pub const BOX_MANAGER: NoopProfile = NoopProfile {
    profile_id: 14,
    part: ActorPart::Background,
    state_flags: (1 << 4) | (1 << 5),
    name_id: 0x8005,
};

/// `ac_boxMove.c` profile metadata.
pub const BOX_MOVE: NoopProfile = NoopProfile {
    profile_id: 15,
    part: ActorPart::Background,
    state_flags: 0,
    name_id: 0,
};

/// `ac_boxTrick01.c` profile metadata.
pub const BOX_TRICK_01: NoopProfile = NoopProfile {
    profile_id: 16,
    part: ActorPart::Background,
    state_flags: (1 << 4) | (1 << 5),
    name_id: 0x8004,
};

/// Invoke the empty constructor body represented by a no-op profile.
pub const fn construct(_: NoopProfile) {}

/// Invoke the empty destructor body represented by a no-op profile.
pub const fn destruct(_: NoopProfile) {}

/// Invoke the empty movement body represented by a no-op profile.
pub const fn move_actor(_: NoopProfile) {}

/// Invoke the empty draw body represented by a no-op profile.
pub const fn draw(_: NoopProfile) {}
