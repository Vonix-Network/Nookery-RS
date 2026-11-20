//! A bounded, deterministic clean-room hosted-town social authority model.
//!
//! This crate models only a small server-visible social state: a town identity,
//! active session roles, visitor presence, and public dropped items. It does
//! not read or represent game saves, private records, inventory, currency,
//! mail, designs, network protocols, accounts, authentication, persistence,
//! cryptography, TLS, clients, game frames, Nintendo services, or deployment.
//!
//! Coordinates are limited to `-128..=127` on both axes, and facing values are
//! limited to `0..=7`. The first slice admits at most [`MAX_VISITORS`] visitors
//! and retains at most [`MAX_ITEMS`] public items. Owner and moderator slots do
//! not consume visitor capacity; at most [`MAX_MODERATORS`] moderators may be
//! active. These deliberately narrow bounds are policy, not gameplay parity.
//!
//! Request idempotency is bounded by [`MAX_REQUEST_HISTORY`]. The oldest
//! successful request record is evicted first when the limit is reached. Once
//! evicted, reusing that request ID is no longer recognized as a replay and is
//! evaluated as a new request. No cryptographic digest is provided.

use core::fmt;

/// Maximum number of concurrently active visitor sessions.
pub const MAX_VISITORS: usize = 3;
/// Maximum number of concurrently active moderator sessions.
pub const MAX_MODERATORS: usize = 2;
/// Maximum number of public dropped items in a town.
pub const MAX_ITEMS: usize = 64;
/// Maximum number of successful request results retained for replay.
pub const MAX_REQUEST_HISTORY: usize = 64;
/// Inclusive minimum social coordinate on either axis.
pub const MIN_COORDINATE: i16 = -128;
/// Inclusive maximum social coordinate on either axis.
pub const MAX_COORDINATE: i16 = 127;
/// Number of valid facing values, represented by `0..=7`.
pub const FACING_VALUES: u8 = 8;

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            /// Creates an identifier from its application-owned numeric value.
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// Returns the identifier's numeric value.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

id_type!(ActorId, "Stable actor identifier used by the social model.");
id_type!(
    SessionId,
    "Stable active-session identifier used by the social model."
);
id_type!(
    RequestId,
    "Client-chosen identifier for bounded command idempotency."
);

/// Stable opaque town identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TownId([u8; 16]);

impl TownId {
    /// Creates a town identifier from exactly sixteen application-owned bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns a copy of the identifier bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Authority role held by an active session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Owner,
    Visitor,
    Moderator,
}

/// Allowlisted social commands accepted by [`Town::apply`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    SetPresence { x: i16, y: i16, facing: u8 },
    DropItem { item_id: u32, x: i16, y: i16 },
    TakeItem { item_id: u32 },
    CloseTown,
}

/// A bounded visitor presence visible in a town snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Presence {
    x: i16,
    y: i16,
    facing: u8,
}

impl Presence {
    #[must_use]
    const fn new(x: i16, y: i16, facing: u8) -> Self {
        Self { x, y, facing }
    }

    #[must_use]
    pub const fn x(self) -> i16 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> i16 {
        self.y
    }

    #[must_use]
    pub const fn facing(self) -> u8 {
        self.facing
    }
}

/// Public dropped item visible in a town snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DroppedItem {
    item_id: u32,
    x: i16,
    y: i16,
    owner: ActorId,
}

impl DroppedItem {
    #[must_use]
    pub const fn item_id(self) -> u32 {
        self.item_id
    }

    #[must_use]
    pub const fn x(self) -> i16 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> i16 {
        self.y
    }

    #[must_use]
    pub const fn owner(self) -> ActorId {
        self.owner
    }
}

/// A stable, read-only view of an active session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    session_id: SessionId,
    actor: ActorId,
    role: Role,
    presence: Option<Presence>,
}

impl SessionSnapshot {
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }

    #[must_use]
    pub const fn actor(self) -> ActorId {
        self.actor
    }

    #[must_use]
    pub const fn role(self) -> Role {
        self.role
    }

    #[must_use]
    pub const fn presence(self) -> Option<Presence> {
        self.presence
    }
}

/// Deterministic server-visible state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    town_id: TownId,
    revision: u64,
    closed: bool,
    sessions: Vec<SessionSnapshot>,
    items: Vec<DroppedItem>,
}

impl Snapshot {
    #[must_use]
    pub const fn town_id(&self) -> TownId {
        self.town_id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    #[must_use]
    pub fn sessions(&self) -> &[SessionSnapshot] {
        &self.sessions
    }

    #[must_use]
    pub fn items(&self) -> &[DroppedItem] {
        &self.items
    }
}

/// Successful command result, retained for bounded idempotent replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commit {
    request_id: RequestId,
    revision: u64,
    snapshot: Snapshot,
}

impl Commit {
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
}

/// Typed failure for a rejected command or explicit model operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyError {
    UnknownSession,
    InactiveSession,
    SessionIdInUse,
    ActorAlreadyActive,
    OwnerCannotLeaveOpenTown,
    Unauthorized,
    CannotRemoveOwner,
    TargetIsNotVisitor,
    VisitorCapacityFull,
    ModeratorCapacityFull,
    TownClosed,
    StaleRevision { expected: u64, actual: u64 },
    RequestIdConflict,
    InvalidCoordinate { value: i16 },
    InvalidFacing { value: u8 },
    DuplicateItemId { item_id: u32 },
    ItemCapacityFull,
    ItemNotFound { item_id: u32 },
    RevisionExhausted,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSession => f.write_str("unknown session"),
            Self::InactiveSession => f.write_str("session is inactive"),
            Self::SessionIdInUse => f.write_str("session ID is already in use"),
            Self::ActorAlreadyActive => f.write_str("actor already has an active session"),
            Self::OwnerCannotLeaveOpenTown => {
                f.write_str("owner must close the town before leaving")
            }
            Self::Unauthorized => f.write_str("session is unauthorized for this operation"),
            Self::CannotRemoveOwner => f.write_str("owner session cannot be removed"),
            Self::TargetIsNotVisitor => f.write_str("target session is not a visitor"),
            Self::VisitorCapacityFull => f.write_str("visitor capacity is full"),
            Self::ModeratorCapacityFull => f.write_str("moderator capacity is full"),
            Self::TownClosed => f.write_str("town is closed"),
            Self::StaleRevision { expected, actual } => {
                write!(f, "stale revision: expected {expected}, actual {actual}")
            }
            Self::RequestIdConflict => f.write_str("request ID conflicts with a retained request"),
            Self::InvalidCoordinate { value } => write!(f, "coordinate {value} is out of bounds"),
            Self::InvalidFacing { value } => write!(f, "facing {value} is out of bounds"),
            Self::DuplicateItemId { item_id } => write!(f, "item ID {item_id} already exists"),
            Self::ItemCapacityFull => f.write_str("item capacity is full"),
            Self::ItemNotFound { item_id } => write!(f, "item ID {item_id} was not found"),
            Self::RevisionExhausted => f.write_str("revision counter is exhausted"),
        }
    }
}

/// An active session and its allowlisted presence.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Session {
    id: SessionId,
    actor: ActorId,
    role: Role,
    presence: Option<Presence>,
}

/// A successful request retained for replay.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RequestRecord {
    session_id: SessionId,
    expected_revision: u64,
    command: Command,
    commit: Commit,
}

/// In-memory authority state for one hosted town.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Town {
    town_id: TownId,
    revision: u64,
    closed: bool,
    sessions: Vec<Session>,
    items: Vec<DroppedItem>,
    request_history: Vec<RequestRecord>,
}

impl Town {
    /// Creates a fresh town with exactly one active owner session at revision zero.
    #[must_use]
    pub fn new(town_id: TownId, owner: ActorId, owner_session: SessionId) -> Self {
        Self {
            town_id,
            revision: 0,
            closed: false,
            sessions: vec![Session {
                id: owner_session,
                actor: owner,
                role: Role::Owner,
                presence: None,
            }],
            items: Vec::new(),
            request_history: Vec::new(),
        }
    }

    /// Returns the current deterministic server-visible state.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .map(|session| SessionSnapshot {
                session_id: session.id,
                actor: session.actor,
                role: session.role,
                presence: session.presence,
            })
            .collect();
        sessions.sort_unstable_by_key(|session| session.session_id);

        let mut items = self.items.clone();
        items.sort_unstable_by_key(|item| item.item_id);

        Snapshot {
            town_id: self.town_id,
            revision: self.revision,
            closed: self.closed,
            sessions,
            items,
        }
    }

    /// Explicitly admits a visitor through an active owner session.
    pub fn admit_visitor(
        &mut self,
        owner_session: SessionId,
        visitor_session: SessionId,
        visitor: ActorId,
    ) -> Result<Snapshot, ApplyError> {
        self.admit_session(owner_session, visitor_session, visitor, Role::Visitor)?;
        Ok(self.snapshot())
    }

    /// Explicitly admits a moderator through the owner session.
    pub fn admit_moderator(
        &mut self,
        owner_session: SessionId,
        moderator_session: SessionId,
        moderator: ActorId,
    ) -> Result<Snapshot, ApplyError> {
        self.admit_session(owner_session, moderator_session, moderator, Role::Moderator)?;
        Ok(self.snapshot())
    }

    fn admit_session(
        &mut self,
        admitting_session: SessionId,
        new_session: SessionId,
        actor: ActorId,
        role: Role,
    ) -> Result<(), ApplyError> {
        if self.closed {
            return Err(ApplyError::TownClosed);
        }
        let admitting = self
            .sessions
            .iter()
            .find(|session| session.id == admitting_session)
            .ok_or(ApplyError::UnknownSession)?;
        if admitting.role != Role::Owner {
            return Err(ApplyError::Unauthorized);
        }
        if self
            .sessions
            .iter()
            .any(|session| session.id == new_session)
        {
            return Err(ApplyError::SessionIdInUse);
        }
        if self.sessions.iter().any(|session| session.actor == actor) {
            return Err(ApplyError::ActorAlreadyActive);
        }
        match role {
            Role::Visitor if self.visitor_count() >= MAX_VISITORS => {
                return Err(ApplyError::VisitorCapacityFull);
            }
            Role::Moderator if self.moderator_count() >= MAX_MODERATORS => {
                return Err(ApplyError::ModeratorCapacityFull);
            }
            Role::Owner => return Err(ApplyError::Unauthorized),
            Role::Visitor | Role::Moderator => {}
        }
        if self.revision == u64::MAX {
            return Err(ApplyError::RevisionExhausted);
        }
        self.sessions.push(Session {
            id: new_session,
            actor,
            role,
            presence: None,
        });
        self.bump_revision()?;
        Ok(())
    }

    /// Removes a visitor through an active owner or moderator session.
    pub fn remove_visitor(
        &mut self,
        authority_session: SessionId,
        visitor_session: SessionId,
    ) -> Result<Snapshot, ApplyError> {
        let authority = self
            .sessions
            .iter()
            .find(|session| session.id == authority_session)
            .ok_or(ApplyError::UnknownSession)?;
        if !matches!(authority.role, Role::Owner | Role::Moderator) {
            return Err(ApplyError::Unauthorized);
        }
        let target_index = self
            .sessions
            .iter()
            .position(|session| session.id == visitor_session)
            .ok_or(ApplyError::UnknownSession)?;
        if self.sessions[target_index].role != Role::Visitor {
            return Err(ApplyError::TargetIsNotVisitor);
        }
        if self.revision == u64::MAX {
            return Err(ApplyError::RevisionExhausted);
        }
        self.sessions.remove(target_index);
        self.bump_revision()?;
        Ok(self.snapshot())
    }

    /// Explicitly revokes a session. The owner must close first so an open town
    /// never silently loses its sole authority session.
    pub fn leave(&mut self, session_id: SessionId) -> Result<Snapshot, ApplyError> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.id == session_id)
            .ok_or(ApplyError::UnknownSession)?;
        if self.sessions[index].role == Role::Owner && !self.closed {
            return Err(ApplyError::OwnerCannotLeaveOpenTown);
        }
        if self.revision == u64::MAX {
            return Err(ApplyError::RevisionExhausted);
        }
        self.sessions.remove(index);
        self.bump_revision()?;
        Ok(self.snapshot())
    }

    /// Applies one allowlisted command at an exact revision.
    pub fn apply(
        &mut self,
        session: SessionId,
        expected_revision: u64,
        request_id: RequestId,
        command: Command,
    ) -> Result<Commit, ApplyError> {
        if let Some(record) = self
            .request_history
            .iter()
            .find(|record| record.commit.request_id == request_id)
        {
            if record.session_id == session
                && record.expected_revision == expected_revision
                && record.command == command
            {
                return Ok(record.commit.clone());
            }
            return Err(ApplyError::RequestIdConflict);
        }

        let session_index = self
            .sessions
            .iter()
            .position(|active| active.id == session)
            .ok_or(ApplyError::UnknownSession)?;
        if self.closed {
            return Err(ApplyError::TownClosed);
        }
        if expected_revision != self.revision {
            return Err(ApplyError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        self.validate_command(session_index, &command)?;
        if self.revision == u64::MAX {
            return Err(ApplyError::RevisionExhausted);
        }

        match command {
            Command::SetPresence { x, y, facing } => {
                self.sessions[session_index].presence = Some(Presence::new(x, y, facing));
            }
            Command::DropItem { item_id, x, y } => {
                self.items.push(DroppedItem {
                    item_id,
                    x,
                    y,
                    owner: self.sessions[session_index].actor,
                });
            }
            Command::TakeItem { item_id } => {
                let item_index = self
                    .items
                    .iter()
                    .position(|item| item.item_id == item_id)
                    .expect("validated item must exist");
                self.items.remove(item_index);
            }
            Command::CloseTown => self.closed = true,
        }
        self.revision += 1;
        let commit = Commit {
            request_id,
            revision: self.revision,
            snapshot: self.snapshot(),
        };
        self.request_history.push(RequestRecord {
            session_id: session,
            expected_revision,
            command,
            commit: commit.clone(),
        });
        if self.request_history.len() > MAX_REQUEST_HISTORY {
            self.request_history.remove(0);
        }
        Ok(commit)
    }

    fn validate_command(&self, session_index: usize, command: &Command) -> Result<(), ApplyError> {
        let session = &self.sessions[session_index];
        match *command {
            Command::SetPresence { x, y, facing } => {
                if session.role != Role::Visitor {
                    return Err(ApplyError::Unauthorized);
                }
                validate_position(x, y, facing)
            }
            Command::DropItem { item_id, x, y } => {
                if session.role != Role::Visitor {
                    return Err(ApplyError::Unauthorized);
                }
                validate_coordinate(x)?;
                validate_coordinate(y)?;
                if self.items.iter().any(|item| item.item_id == item_id) {
                    return Err(ApplyError::DuplicateItemId { item_id });
                }
                if self.items.len() >= MAX_ITEMS {
                    return Err(ApplyError::ItemCapacityFull);
                }
                Ok(())
            }
            Command::TakeItem { item_id } => {
                if session.role != Role::Visitor {
                    return Err(ApplyError::Unauthorized);
                }
                if !self.items.iter().any(|item| item.item_id == item_id) {
                    return Err(ApplyError::ItemNotFound { item_id });
                }
                Ok(())
            }
            Command::CloseTown => {
                if session.role != Role::Owner {
                    return Err(ApplyError::Unauthorized);
                }
                Ok(())
            }
        }
    }

    fn visitor_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.role == Role::Visitor)
            .count()
    }

    fn moderator_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.role == Role::Moderator)
            .count()
    }

    fn bump_revision(&mut self) -> Result<(), ApplyError> {
        if self.revision == u64::MAX {
            return Err(ApplyError::RevisionExhausted);
        }
        self.revision += 1;
        Ok(())
    }
}

fn validate_position(x: i16, y: i16, facing: u8) -> Result<(), ApplyError> {
    validate_coordinate(x)?;
    validate_coordinate(y)?;
    if facing >= FACING_VALUES {
        return Err(ApplyError::InvalidFacing { value: facing });
    }
    Ok(())
}

fn validate_coordinate(value: i16) -> Result<(), ApplyError> {
    if !(MIN_COORDINATE..=MAX_COORDINATE).contains(&value) {
        return Err(ApplyError::InvalidCoordinate { value });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn town() -> Town {
        Town::new(TownId::new([7; 16]), ActorId::new(1), SessionId::new(10))
    }

    fn admit(town: &mut Town, session: u64, actor: u64) {
        town.admit_visitor(
            SessionId::new(10),
            SessionId::new(session),
            ActorId::new(actor),
        )
        .expect("visitor admission");
    }

    fn apply(
        town: &mut Town,
        session: u64,
        request: u64,
        command: Command,
    ) -> Result<Commit, ApplyError> {
        let revision = town.snapshot().revision();
        town.apply(
            SessionId::new(session),
            revision,
            RequestId::new(request),
            command,
        )
    }

    #[test]
    fn fresh_town_has_owner_and_zero_revision() {
        let snapshot = town().snapshot();
        assert_eq!(snapshot.revision(), 0);
        assert!(!snapshot.is_closed());
        assert_eq!(snapshot.sessions().len(), 1);
        assert_eq!(snapshot.sessions()[0].role(), Role::Owner);
        assert_eq!(snapshot.sessions()[0].actor(), ActorId::new(1));
    }

    #[test]
    fn owner_admits_up_to_bounded_visitors_and_rejects_fourth() {
        let mut town = town();
        admit(&mut town, 11, 2);
        admit(&mut town, 12, 3);
        admit(&mut town, 13, 4);
        let before = town.snapshot();
        assert_eq!(before.revision(), 3);
        let rejected = town.admit_visitor(SessionId::new(10), SessionId::new(14), ActorId::new(5));
        assert_eq!(rejected, Err(ApplyError::VisitorCapacityFull));
        assert_eq!(town.snapshot(), before);
    }

    #[test]
    fn visitor_presence_is_scoped_to_its_own_session() {
        let mut town = town();
        admit(&mut town, 11, 2);
        admit(&mut town, 12, 3);
        let first = apply(
            &mut town,
            11,
            1,
            Command::SetPresence {
                x: 4,
                y: -2,
                facing: 3,
            },
        )
        .unwrap();
        assert_eq!(first.snapshot().sessions()[0].role(), Role::Owner);
        let visitor = first
            .snapshot()
            .sessions()
            .iter()
            .find(|session| session.session_id() == SessionId::new(11))
            .unwrap();
        assert_eq!(visitor.presence(), Some(Presence::new(4, -2, 3)));
        let other = first
            .snapshot()
            .sessions()
            .iter()
            .find(|session| session.session_id() == SessionId::new(12))
            .unwrap();
        assert_eq!(other.presence(), None);
    }

    #[test]
    fn visitor_drop_and_take_are_allowlisted_and_deterministic() {
        let mut town = town();
        admit(&mut town, 11, 2);
        let dropped = apply(
            &mut town,
            11,
            1,
            Command::DropItem {
                item_id: 42,
                x: 2,
                y: 3,
            },
        )
        .unwrap();
        assert_eq!(dropped.snapshot().items()[0].item_id(), 42);
        assert_eq!(dropped.snapshot().items()[0].owner(), ActorId::new(2));
        assert_eq!(dropped.revision(), 2);
        let taken = apply(&mut town, 11, 2, Command::TakeItem { item_id: 42 }).unwrap();
        assert!(taken.snapshot().items().is_empty());
        assert_eq!(taken.revision(), 3);
    }

    #[test]
    fn unauthorized_close_or_cross_actor_mutation_is_rejected_without_state_change() {
        let mut town = town();
        admit(&mut town, 11, 2);
        admit(&mut town, 12, 3);
        let before = town.snapshot();
        let close = apply(&mut town, 11, 1, Command::CloseTown);
        assert_eq!(close, Err(ApplyError::Unauthorized));
        assert_eq!(town.snapshot(), before);

        let owner_close = apply(&mut town, 10, 2, Command::CloseTown).unwrap();
        let closed = owner_close.snapshot().clone();
        assert!(closed.is_closed());
        assert_eq!(town.snapshot(), closed);
    }

    #[test]
    fn stale_revision_is_rejected_without_state_change() {
        let mut town = town();
        admit(&mut town, 11, 2);
        let before = town.snapshot();
        let rejected = town.apply(
            SessionId::new(11),
            before.revision() - 1,
            RequestId::new(1),
            Command::SetPresence {
                x: 1,
                y: 1,
                facing: 0,
            },
        );
        assert_eq!(
            rejected,
            Err(ApplyError::StaleRevision {
                expected: before.revision() - 1,
                actual: before.revision(),
            })
        );
        assert_eq!(town.snapshot(), before);
    }

    #[test]
    fn duplicate_request_replays_once_and_conflicting_reuse_is_rejected() {
        let mut town = town();
        admit(&mut town, 11, 2);
        let first = apply(
            &mut town,
            11,
            99,
            Command::SetPresence {
                x: 1,
                y: 2,
                facing: 4,
            },
        )
        .unwrap();
        let replay = town.apply(
            SessionId::new(11),
            1,
            RequestId::new(99),
            Command::SetPresence {
                x: 1,
                y: 2,
                facing: 4,
            },
        );
        assert_eq!(replay, Ok(first.clone()));
        assert_eq!(town.snapshot(), *first.snapshot());

        let before = town.snapshot();
        let conflict = town.apply(
            SessionId::new(11),
            1,
            RequestId::new(99),
            Command::SetPresence {
                x: 9,
                y: 2,
                facing: 4,
            },
        );
        assert_eq!(conflict, Err(ApplyError::RequestIdConflict));
        assert_eq!(town.snapshot(), before);
    }

    #[test]
    fn leave_and_owner_removal_revoke_the_session() {
        let mut town = town();
        admit(&mut town, 11, 2);
        admit(&mut town, 12, 3);
        town.leave(SessionId::new(11)).unwrap();
        let before = town.snapshot();
        assert!(
            !before
                .sessions()
                .iter()
                .any(|session| session.session_id() == SessionId::new(11))
        );
        let revoked = town
            .remove_visitor(SessionId::new(10), SessionId::new(12))
            .unwrap();
        assert!(
            !revoked
                .sessions()
                .iter()
                .any(|session| session.session_id() == SessionId::new(12))
        );
        let unchanged = town.snapshot();
        assert_eq!(
            town.apply(
                SessionId::new(12),
                unchanged.revision(),
                RequestId::new(1),
                Command::TakeItem { item_id: 1 }
            ),
            Err(ApplyError::UnknownSession)
        );
        assert_eq!(town.snapshot(), unchanged);
    }

    #[test]
    fn close_town_rejects_new_admission_and_commands() {
        let mut town = town();
        let closed = apply(&mut town, 10, 1, Command::CloseTown).unwrap();
        let before = closed.snapshot().clone();
        let admission = town.admit_visitor(SessionId::new(10), SessionId::new(11), ActorId::new(2));
        assert_eq!(admission, Err(ApplyError::TownClosed));
        assert_eq!(town.snapshot(), before);
        let command = town.apply(
            SessionId::new(10),
            before.revision(),
            RequestId::new(2),
            Command::CloseTown,
        );
        assert_eq!(command, Err(ApplyError::TownClosed));
        assert_eq!(town.snapshot(), before);
    }

    #[test]
    fn snapshots_are_stably_ordered() {
        let mut town = town();
        admit(&mut town, 30, 3);
        admit(&mut town, 20, 2);
        apply(
            &mut town,
            20,
            1,
            Command::DropItem {
                item_id: 8,
                x: 0,
                y: 0,
            },
        )
        .unwrap();
        apply(
            &mut town,
            20,
            2,
            Command::DropItem {
                item_id: 3,
                x: 0,
                y: 0,
            },
        )
        .unwrap();
        let snapshot = town.snapshot();
        let sessions: Vec<_> = snapshot
            .sessions()
            .iter()
            .map(|session| session.session_id().get())
            .collect();
        let items: Vec<_> = snapshot.items().iter().map(|item| item.item_id()).collect();
        assert_eq!(sessions, vec![10, 20, 30]);
        assert_eq!(items, vec![3, 8]);
    }

    #[test]
    fn invalid_bounds_duplicate_items_and_unknown_sessions_fail_closed() {
        let mut town = town();
        admit(&mut town, 11, 2);
        let before = town.snapshot();
        assert_eq!(
            apply(
                &mut town,
                11,
                1,
                Command::SetPresence {
                    x: 128,
                    y: 0,
                    facing: 0,
                }
            ),
            Err(ApplyError::InvalidCoordinate { value: 128 })
        );
        assert_eq!(town.snapshot(), before);
        let unknown = town.apply(
            SessionId::new(404),
            before.revision(),
            RequestId::new(2),
            Command::SetPresence {
                x: 0,
                y: 0,
                facing: 0,
            },
        );
        assert_eq!(unknown, Err(ApplyError::UnknownSession));
        assert_eq!(town.snapshot(), before);

        apply(
            &mut town,
            11,
            3,
            Command::DropItem {
                item_id: 5,
                x: 0,
                y: 0,
            },
        )
        .unwrap();
        let duplicate_before = town.snapshot();
        assert_eq!(
            apply(
                &mut town,
                11,
                4,
                Command::DropItem {
                    item_id: 5,
                    x: 1,
                    y: 1,
                }
            ),
            Err(ApplyError::DuplicateItemId { item_id: 5 })
        );
        assert_eq!(town.snapshot(), duplicate_before);
    }

    #[test]
    fn repeated_fixed_command_trace_produces_identical_snapshots() {
        fn trace() -> Vec<Snapshot> {
            let mut town = town();
            let mut snapshots = vec![town.snapshot()];
            admit(&mut town, 11, 2);
            snapshots.push(town.snapshot());
            apply(
                &mut town,
                11,
                1,
                Command::SetPresence {
                    x: -4,
                    y: 7,
                    facing: 2,
                },
            )
            .unwrap();
            snapshots.push(town.snapshot());
            apply(
                &mut town,
                11,
                2,
                Command::DropItem {
                    item_id: 10,
                    x: 1,
                    y: 1,
                },
            )
            .unwrap();
            snapshots.push(town.snapshot());
            snapshots
        }

        assert_eq!(trace(), trace());
    }
}
