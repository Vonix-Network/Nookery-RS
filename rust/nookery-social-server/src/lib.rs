//! Deterministic local hosted-town server core.
//!
//! The public transport path uses the real social frame codec and delegates
//! command authority to the clean-room social model. Payloads are a small,
//! versioned social command encoding; no game-save or proprietary structures
//! cross this boundary. This crate does not bind a socket or deploy a service.

use nookery_social_model as model;
use nookery_social_persistence::{
    CommandRecord, PersistenceError, Snapshot as StoredSnapshot, Store,
};
use nookery_social_proto::{
    DecodeError, EncodeError, Frame, MessageType, PROTOCOL_VERSION, decode_frame,
};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};

pub const MAX_FRAME_BYTES: usize = nookery_social_proto::MAX_FRAME_BYTES;
pub const MAX_IDEMPOTENCY_ENTRIES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TownKey([u8; 16]);

impl TownKey {
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    pub const fn bytes(self) -> [u8; 16] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerRole {
    Owner,
    Resident,
    Visitor,
    Moderator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    CreateTown {
        town_id: TownKey,
    },
    Discover,
    Join {
        town_id: TownKey,
    },
    Leave {
        town_id: TownKey,
    },
    Apply {
        town_id: TownKey,
        command: model::Command,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Response {
    Accepted { revision: u64 },
    Discovered(Vec<TownKey>),
    Rejected(ErrorCode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    Malformed,
    Unauthorized,
    NotFound,
    Duplicate,
    Conflict,
    StaleRevision,
    Closed,
    Expired,
    TooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    Frame(DecodeError),
    Encode(EncodeError),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TransportError {}

#[derive(Debug)]
pub enum ServerError {
    Persistence(PersistenceError),
    InvalidConfig(&'static str),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ServerError {}

impl From<PersistenceError> for ServerError {
    fn from(error: PersistenceError) -> Self {
        Self::Persistence(error)
    }
}

#[derive(Clone)]
struct SessionLease {
    actor: model::ActorId,
    town: Option<TownKey>,
    role: ServerRole,
    expires_at: Instant,
}

struct HostedTown {
    model: model::Town,
    store: Store,
}

struct IdempotentResult {
    session: model::SessionId,
    request: model::RequestId,
    command: Command,
    response: Response,
}

pub struct TownServer {
    root_store: Store,
    towns: BTreeMap<TownKey, HostedTown>,
    sessions: HashMap<model::SessionId, SessionLease>,
    idempotency: VecDeque<IdempotentResult>,
    lease: Duration,
}

impl TownServer {
    pub fn open(root_store: Store, lease: Duration) -> Result<Self, ServerError> {
        if lease.is_zero() {
            return Err(ServerError::InvalidConfig("session lease must be nonzero"));
        }
        Ok(Self {
            root_store,
            towns: BTreeMap::new(),
            sessions: HashMap::new(),
            idempotency: VecDeque::new(),
            lease,
        })
    }

    /// Registers a local session. A client never supplies a role; the server
    /// assigns Resident until an owner-authorized admission changes it.
    pub fn connect(
        &mut self,
        session: model::SessionId,
        actor: model::ActorId,
    ) -> Result<(), ErrorCode> {
        if session.get() == 0 || actor.get() == 0 {
            return Err(ErrorCode::Malformed);
        }
        self.expire_due();
        if let Some(existing) = self.sessions.get_mut(&session) {
            if existing.actor != actor {
                return Err(ErrorCode::Unauthorized);
            }
            existing.expires_at = Instant::now() + self.lease;
            return Ok(());
        }
        self.sessions.insert(
            session,
            SessionLease {
                actor,
                town: None,
                role: ServerRole::Resident,
                expires_at: Instant::now() + self.lease,
            },
        );
        Ok(())
    }

    /// Refreshes the same session lease after a temporary disconnect.
    pub fn reconnect(
        &mut self,
        session: model::SessionId,
        actor: model::ActorId,
    ) -> Result<(), ErrorCode> {
        self.connect(session, actor)
    }

    pub fn session_role(&mut self, session: model::SessionId) -> Result<ServerRole, ErrorCode> {
        self.expire_due();
        self.sessions
            .get(&session)
            .map(|lease| lease.role)
            .ok_or(ErrorCode::Expired)
    }

    pub fn discover(&mut self, session: model::SessionId) -> Result<Response, ErrorCode> {
        self.ensure_session(session)?;
        Ok(Response::Discovered(self.towns.keys().copied().collect()))
    }

    pub fn execute(
        &mut self,
        session: model::SessionId,
        request: model::RequestId,
        expected_revision: u64,
        command: Command,
    ) -> Result<Response, ErrorCode> {
        self.ensure_session(session)?;
        if request.get() == 0 {
            return Err(ErrorCode::Malformed);
        }
        if let Some(cached) = self
            .idempotency
            .iter()
            .find(|entry| entry.session == session && entry.request == request)
        {
            return if cached.command == command {
                Ok(cached.response.clone())
            } else {
                Err(ErrorCode::Conflict)
            };
        }
        let response = match command {
            Command::CreateTown { town_id } => self.create_town(session, town_id)?,
            Command::Discover => self.discover(session)?,
            Command::Join { town_id } => self.join(session, town_id)?,
            Command::Leave { town_id } => self.leave(session, town_id)?,
            Command::Apply { town_id, command } => {
                self.apply(session, request, expected_revision, town_id, command)?
            }
        };
        self.remember(session, request, command, response.clone());
        Ok(response)
    }

    /// Decodes one real social-proto frame and returns one encoded response frame.
    /// Frame decode failures are returned without allocating a command or mutating state.
    pub fn handle_frame(
        &mut self,
        session: model::SessionId,
        bytes: &[u8],
    ) -> Result<Vec<u8>, TransportError> {
        let frame = decode_frame(bytes).map_err(TransportError::Frame)?;
        if frame.flags != 0 || frame.request_id == 0 || frame.sequence == 0 {
            return self.encode_response(&frame, Response::Rejected(ErrorCode::Malformed));
        }
        let response = match self.parse_frame_command(&frame) {
            Ok(command) => self
                .execute(
                    session,
                    model::RequestId::new(frame.request_id),
                    frame.town_revision,
                    command,
                )
                .unwrap_or_else(Response::Rejected),
            Err(error) => Response::Rejected(error),
        };
        self.encode_response(&frame, response)
    }

    pub fn admit_moderator(
        &mut self,
        owner_session: model::SessionId,
        target_session: model::SessionId,
        town_id: TownKey,
        request: model::RequestId,
    ) -> Result<Response, ErrorCode> {
        self.ensure_session(owner_session)?;
        self.ensure_session(target_session)?;
        if request.get() == 0 {
            return Err(ErrorCode::Malformed);
        }
        let owner_lease = self
            .sessions
            .get(&owner_session)
            .ok_or(ErrorCode::Expired)?;
        if owner_lease.town != Some(town_id) || owner_lease.role != ServerRole::Owner {
            return Err(ErrorCode::Unauthorized);
        }
        let target = self
            .sessions
            .get(&target_session)
            .ok_or(ErrorCode::Expired)?;
        if target.town.is_some() {
            return Err(ErrorCode::Conflict);
        }
        let hosted = self.towns.get(&town_id).ok_or(ErrorCode::NotFound)?;
        let mut candidate = hosted.model.clone();
        let snapshot = candidate
            .admit_moderator(owner_session, target_session, target.actor)
            .map_err(map_model_error)?;
        let revision = snapshot.revision();
        let store = hosted.store.clone();
        persist_state(
            &store,
            revision,
            &snapshot,
            request,
            membership_bytes(2, target_session, town_id),
        )?;
        self.towns
            .get_mut(&town_id)
            .ok_or(ErrorCode::NotFound)?
            .model = candidate;
        let lease = self
            .sessions
            .get_mut(&target_session)
            .ok_or(ErrorCode::Expired)?;
        lease.town = Some(town_id);
        lease.role = ServerRole::Moderator;
        let response = Response::Accepted { revision };
        self.remember(
            owner_session,
            request,
            Command::Join { town_id },
            response.clone(),
        );
        Ok(response)
    }

    fn create_town(
        &mut self,
        session: model::SessionId,
        town_id: TownKey,
    ) -> Result<Response, ErrorCode> {
        if self
            .sessions
            .get(&session)
            .ok_or(ErrorCode::Expired)?
            .town
            .is_some()
        {
            return Err(ErrorCode::Conflict);
        }
        if self.towns.contains_key(&town_id) {
            return Err(ErrorCode::Conflict);
        }
        let actor = self.sessions.get(&session).ok_or(ErrorCode::Expired)?.actor;
        let store = self
            .root_store
            .for_town(town_id.bytes())
            .map_err(map_persistence_error)?;
        let town = model::Town::new(model::TownId::new(town_id.bytes()), actor, session);
        let snapshot = town.snapshot();
        store
            .write_snapshot(&StoredSnapshot {
                revision: snapshot.revision(),
                bytes: encode_model_snapshot(&snapshot),
            })
            .map_err(map_persistence_error)?;
        self.towns
            .insert(town_id, HostedTown { model: town, store });
        let lease = self.sessions.get_mut(&session).ok_or(ErrorCode::Expired)?;
        lease.town = Some(town_id);
        lease.role = ServerRole::Owner;
        Ok(Response::Accepted { revision: 0 })
    }

    fn join(&mut self, session: model::SessionId, town_id: TownKey) -> Result<Response, ErrorCode> {
        if self
            .sessions
            .get(&session)
            .ok_or(ErrorCode::Expired)?
            .town
            .is_some()
        {
            return Err(ErrorCode::Conflict);
        }
        let target_actor = self.sessions.get(&session).ok_or(ErrorCode::Expired)?.actor;
        let hosted = self.towns.get(&town_id).ok_or(ErrorCode::NotFound)?;
        let owner = hosted
            .model
            .snapshot()
            .sessions()
            .iter()
            .find(|active| active.role() == model::Role::Owner)
            .ok_or(ErrorCode::NotFound)?
            .session_id();
        let mut candidate = hosted.model.clone();
        let snapshot = candidate
            .admit_visitor(owner, session, target_actor)
            .map_err(map_model_error)?;
        let revision = snapshot.revision();
        let store = hosted.store.clone();
        persist_state(
            &store,
            revision,
            &snapshot,
            model::RequestId::new(session.get()),
            membership_bytes(1, session, town_id),
        )?;
        self.towns
            .get_mut(&town_id)
            .ok_or(ErrorCode::NotFound)?
            .model = candidate;
        let lease = self.sessions.get_mut(&session).ok_or(ErrorCode::Expired)?;
        lease.town = Some(town_id);
        lease.role = ServerRole::Visitor;
        Ok(Response::Accepted { revision })
    }

    fn leave(
        &mut self,
        session: model::SessionId,
        town_id: TownKey,
    ) -> Result<Response, ErrorCode> {
        let lease = self.sessions.get(&session).ok_or(ErrorCode::Expired)?;
        if lease.town != Some(town_id) {
            return Err(ErrorCode::Unauthorized);
        }
        let hosted = self.towns.get(&town_id).ok_or(ErrorCode::NotFound)?;
        let mut candidate = hosted.model.clone();
        let snapshot = candidate.leave(session).map_err(map_model_error)?;
        let revision = snapshot.revision();
        let store = hosted.store.clone();
        persist_state(
            &store,
            revision,
            &snapshot,
            model::RequestId::new(session.get()),
            membership_bytes(3, session, town_id),
        )?;
        self.towns
            .get_mut(&town_id)
            .ok_or(ErrorCode::NotFound)?
            .model = candidate;
        let lease = self.sessions.get_mut(&session).ok_or(ErrorCode::Expired)?;
        lease.town = None;
        lease.role = ServerRole::Resident;
        Ok(Response::Accepted { revision })
    }

    fn apply(
        &mut self,
        session: model::SessionId,
        request: model::RequestId,
        expected: u64,
        town_id: TownKey,
        command: model::Command,
    ) -> Result<Response, ErrorCode> {
        let lease = self.sessions.get(&session).ok_or(ErrorCode::Expired)?;
        if lease.town != Some(town_id) {
            return Err(ErrorCode::Unauthorized);
        }
        let hosted = self.towns.get(&town_id).ok_or(ErrorCode::NotFound)?;
        let before = hosted.model.snapshot().revision();
        let mut candidate = hosted.model.clone();
        let commit = candidate
            .apply(session, expected, request, command)
            .map_err(map_model_error)?;
        let revision = commit.revision();
        if revision != before {
            let store = hosted.store.clone();
            persist_state(
                &store,
                revision,
                commit.snapshot(),
                request,
                command_bytes(town_id, command),
            )?;
            self.towns
                .get_mut(&town_id)
                .ok_or(ErrorCode::NotFound)?
                .model = candidate;
        }
        Ok(Response::Accepted { revision })
    }

    fn parse_frame_command(&self, frame: &Frame) -> Result<Command, ErrorCode> {
        match frame.message_type {
            MessageType::Hello => match frame.payload.as_slice() {
                [1, rest @ ..] => Ok(Command::CreateTown {
                    town_id: parse_town(rest)?,
                }),
                [2] => Ok(Command::Discover),
                _ => Err(ErrorCode::Malformed),
            },
            MessageType::JoinTown => Ok(Command::Join {
                town_id: parse_town(&frame.payload)?,
            }),
            MessageType::LeaveTown => Ok(Command::Leave {
                town_id: parse_town(&frame.payload)?,
            }),
            MessageType::Command => parse_model_command(&frame.payload),
            MessageType::Snapshot | MessageType::Error => Err(ErrorCode::Malformed),
        }
    }

    fn encode_response(
        &self,
        request: &Frame,
        response: Response,
    ) -> Result<Vec<u8>, TransportError> {
        let (message_type, revision, payload) = match response {
            Response::Accepted { revision } => (
                MessageType::Snapshot,
                revision,
                [0u8; 9].into_iter().chain(revision.to_be_bytes()).collect(),
            ),
            Response::Discovered(towns) => {
                let count = u8::try_from(towns.len()).unwrap_or(u8::MAX);
                let mut payload = vec![0, 1, count];
                for town in towns.into_iter().take(usize::from(count)) {
                    payload.extend_from_slice(&town.bytes());
                }
                (MessageType::Snapshot, 0, payload)
            }
            Response::Rejected(error) => (MessageType::Error, 0, vec![1, error as u8]),
        };
        Frame {
            version: PROTOCOL_VERSION,
            message_type,
            flags: 0,
            request_id: request.request_id,
            sequence: request.sequence,
            town_revision: revision,
            payload,
        }
        .encode()
        .map_err(TransportError::Encode)
    }

    fn ensure_session(&mut self, session: model::SessionId) -> Result<(), ErrorCode> {
        self.expire_due();
        self.sessions
            .get(&session)
            .map(|_| ())
            .ok_or(ErrorCode::Expired)
    }

    fn expire_due(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .sessions
            .iter()
            .filter_map(|(id, lease)| (now >= lease.expires_at).then_some(*id))
            .collect();
        for session in expired {
            self.expire_one(session);
        }
    }

    fn expire_one(&mut self, session: model::SessionId) {
        let Some(lease) = self.sessions.get(&session).cloned() else {
            return;
        };
        if let Some(town_id) = lease.town {
            if lease.role != ServerRole::Owner {
                if let Some(hosted) = self.towns.get(&town_id) {
                    let mut candidate = hosted.model.clone();
                    if let Ok(snapshot) = candidate.leave(session) {
                        let _ = hosted.store.write_snapshot(&StoredSnapshot {
                            revision: snapshot.revision(),
                            bytes: encode_model_snapshot(&snapshot),
                        });
                        if let Some(current) = self.towns.get_mut(&town_id) {
                            current.model = candidate;
                        }
                    }
                }
            }
        }
        self.sessions.remove(&session);
    }

    fn remember(
        &mut self,
        session: model::SessionId,
        request: model::RequestId,
        command: Command,
        response: Response,
    ) {
        self.idempotency.push_back(IdempotentResult {
            session,
            request,
            command,
            response,
        });
        while self.idempotency.len() > MAX_IDEMPOTENCY_ENTRIES {
            self.idempotency.pop_front();
        }
    }
}

fn parse_town(bytes: &[u8]) -> Result<TownKey, ErrorCode> {
    if bytes.len() != 16 {
        return Err(ErrorCode::Malformed);
    }
    Ok(TownKey::new(
        bytes.try_into().map_err(|_| ErrorCode::Malformed)?,
    ))
}

fn parse_model_command(bytes: &[u8]) -> Result<Command, ErrorCode> {
    if bytes.len() < 17 {
        return Err(ErrorCode::Malformed);
    }
    let town_id = parse_town(&bytes[1..17])?;
    let payload = &bytes[17..];
    let command = match bytes[0] {
        1 if payload.len() == 5 => model::Command::SetPresence {
            x: i16::from_be_bytes([payload[0], payload[1]]),
            y: i16::from_be_bytes([payload[2], payload[3]]),
            facing: payload[4],
        },
        2 if payload.len() == 8 => model::Command::DropItem {
            item_id: u32::from_be_bytes(
                payload[0..4].try_into().map_err(|_| ErrorCode::Malformed)?,
            ),
            x: i16::from_be_bytes([payload[4], payload[5]]),
            y: i16::from_be_bytes([payload[6], payload[7]]),
        },
        3 if payload.len() == 4 => model::Command::TakeItem {
            item_id: u32::from_be_bytes(payload.try_into().map_err(|_| ErrorCode::Malformed)?),
        },
        4 if payload.is_empty() => model::Command::CloseTown,
        _ => return Err(ErrorCode::Malformed),
    };
    Ok(Command::Apply { town_id, command })
}

fn command_bytes(town: TownKey, command: model::Command) -> Vec<u8> {
    let mut bytes = town.bytes().to_vec();
    match command {
        model::Command::SetPresence { x, y, facing } => {
            bytes.push(1);
            bytes.extend_from_slice(&x.to_be_bytes());
            bytes.extend_from_slice(&y.to_be_bytes());
            bytes.push(facing);
        }
        model::Command::DropItem { item_id, x, y } => {
            bytes.push(2);
            bytes.extend_from_slice(&item_id.to_be_bytes());
            bytes.extend_from_slice(&x.to_be_bytes());
            bytes.extend_from_slice(&y.to_be_bytes());
        }
        model::Command::TakeItem { item_id } => {
            bytes.push(3);
            bytes.extend_from_slice(&item_id.to_be_bytes());
        }
        model::Command::CloseTown => bytes.push(4),
    }
    bytes
}

fn membership_bytes(kind: u8, session: model::SessionId, town: TownKey) -> Vec<u8> {
    let mut bytes = vec![kind];
    bytes.extend_from_slice(&session.get().to_be_bytes());
    bytes.extend_from_slice(&town.bytes());
    bytes
}

fn persist_state(
    store: &Store,
    revision: u64,
    snapshot: &model::Snapshot,
    request: model::RequestId,
    bytes: Vec<u8>,
) -> Result<(), ErrorCode> {
    store
        .write_snapshot(&StoredSnapshot {
            revision,
            bytes: encode_model_snapshot(snapshot),
        })
        .map_err(map_persistence_error)?;
    let mut key = [0u8; 16];
    key[..8].copy_from_slice(&request.get().to_be_bytes());
    key[8..].copy_from_slice(&revision.to_be_bytes());
    store
        .append_command(&CommandRecord {
            revision,
            idempotency_key: key,
            bytes,
        })
        .map_err(map_persistence_error)
}

fn encode_model_snapshot(snapshot: &model::Snapshot) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"NKSNAP1");
    bytes.extend_from_slice(&snapshot.town_id().bytes());
    bytes.extend_from_slice(&snapshot.revision().to_be_bytes());
    bytes.push(u8::from(snapshot.is_closed()));
    bytes.extend_from_slice(&(snapshot.sessions().len() as u16).to_be_bytes());
    for session in snapshot.sessions() {
        bytes.extend_from_slice(&session.session_id().get().to_be_bytes());
        bytes.extend_from_slice(&session.actor().get().to_be_bytes());
        bytes.push(match session.role() {
            model::Role::Owner => 1,
            model::Role::Visitor => 2,
            model::Role::Moderator => 3,
        });
        match session.presence() {
            Some(presence) => {
                bytes.push(1);
                bytes.extend_from_slice(&presence.x().to_be_bytes());
                bytes.extend_from_slice(&presence.y().to_be_bytes());
                bytes.push(presence.facing());
            }
            None => bytes.push(0),
        }
    }
    bytes.extend_from_slice(&(snapshot.items().len() as u16).to_be_bytes());
    for item in snapshot.items() {
        bytes.extend_from_slice(&item.item_id().to_be_bytes());
        bytes.extend_from_slice(&item.x().to_be_bytes());
        bytes.extend_from_slice(&item.y().to_be_bytes());
        bytes.extend_from_slice(&item.owner().get().to_be_bytes());
    }
    bytes
}

fn map_model_error(error: model::ApplyError) -> ErrorCode {
    match error {
        model::ApplyError::UnknownSession
        | model::ApplyError::InactiveSession
        | model::ApplyError::Unauthorized
        | model::ApplyError::CannotRemoveOwner
        | model::ApplyError::OwnerCannotLeaveOpenTown => ErrorCode::Unauthorized,
        model::ApplyError::TownClosed => ErrorCode::Closed,
        model::ApplyError::StaleRevision { .. } => ErrorCode::StaleRevision,
        model::ApplyError::RequestIdConflict
        | model::ApplyError::SessionIdInUse
        | model::ApplyError::ActorAlreadyActive => ErrorCode::Conflict,
        model::ApplyError::RevisionExhausted => ErrorCode::Conflict,
        model::ApplyError::VisitorCapacityFull
        | model::ApplyError::ModeratorCapacityFull
        | model::ApplyError::ItemCapacityFull => ErrorCode::TooLarge,
        model::ApplyError::InvalidCoordinate { .. }
        | model::ApplyError::InvalidFacing { .. }
        | model::ApplyError::DuplicateItemId { .. }
        | model::ApplyError::ItemNotFound { .. }
        | model::ApplyError::TargetIsNotVisitor => ErrorCode::Malformed,
    }
}

fn map_persistence_error(error: PersistenceError) -> ErrorCode {
    match error {
        PersistenceError::TooLarge(_) => ErrorCode::TooLarge,
        PersistenceError::NonMonotonicRevision => ErrorCode::Conflict,
        _ => ErrorCode::Conflict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn server(lease: Duration) -> (TownServer, std::path::PathBuf) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nookery-server-{}-{}-{nonce}",
            std::process::id(),
            lease.as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        (
            TownServer::open(Store::open(&root).unwrap(), lease).unwrap(),
            root,
        )
    }

    #[test]
    fn join_and_leave_require_authorized_session() {
        let (mut server, root) = server(Duration::from_secs(60));
        let owner = model::SessionId::new(10);
        let guest = model::SessionId::new(11);
        let town = TownKey::new([7; 16]);
        server.connect(owner, model::ActorId::new(1)).unwrap();
        server
            .execute(
                owner,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            )
            .unwrap();
        assert_eq!(
            server.execute(
                guest,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town }
            ),
            Err(ErrorCode::Expired)
        );
        server.connect(guest, model::ActorId::new(2)).unwrap();
        assert_eq!(
            server.execute(
                guest,
                model::RequestId::new(3),
                0,
                Command::Join { town_id: town }
            ),
            Ok(Response::Accepted { revision: 1 })
        );
        assert_eq!(
            server.execute(
                guest,
                model::RequestId::new(4),
                1,
                Command::Leave { town_id: town }
            ),
            Ok(Response::Accepted { revision: 2 })
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn frame_codec_rejects_hostile_packets_and_duplicate_model_delivery_is_idempotent() {
        let (mut server, root) = server(Duration::from_secs(60));
        let owner = model::SessionId::new(10);
        let guest = model::SessionId::new(11);
        let town = TownKey::new([8; 16]);
        server.connect(owner, model::ActorId::new(1)).unwrap();
        server
            .execute(
                owner,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            )
            .unwrap();
        server.connect(guest, model::ActorId::new(2)).unwrap();
        server
            .execute(
                guest,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town },
            )
            .unwrap();
        let command = Command::Apply {
            town_id: town,
            command: model::Command::SetPresence {
                x: 1,
                y: 2,
                facing: 3,
            },
        };
        let first = server
            .execute(guest, model::RequestId::new(3), 1, command)
            .unwrap();
        let replay = server
            .execute(guest, model::RequestId::new(3), 1, command)
            .unwrap();
        assert_eq!(first, replay);
        assert!(matches!(
            server.handle_frame(guest, b"bad"),
            Err(TransportError::Frame(DecodeError::ShortHeader { .. }))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reconnect_refreshes_lease_and_expiry_revokes_visitor() {
        let (mut server, root) = server(Duration::from_millis(5));
        let owner = model::SessionId::new(10);
        let guest = model::SessionId::new(11);
        let town = TownKey::new([9; 16]);
        server.connect(owner, model::ActorId::new(1)).unwrap();
        server
            .execute(
                owner,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            )
            .unwrap();
        server.connect(guest, model::ActorId::new(2)).unwrap();
        server
            .execute(
                guest,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town },
            )
            .unwrap();
        server.reconnect(guest, model::ActorId::new(2)).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(server.session_role(guest), Err(ErrorCode::Expired));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn moderator_admission_is_owner_only_and_roles_are_server_assigned() {
        let (mut server, root) = server(Duration::from_secs(60));
        let owner = model::SessionId::new(10);
        let moderator = model::SessionId::new(12);
        let town = TownKey::new([6; 16]);
        server.connect(owner, model::ActorId::new(1)).unwrap();
        server.connect(moderator, model::ActorId::new(3)).unwrap();
        assert_eq!(server.session_role(moderator), Ok(ServerRole::Resident));
        server
            .execute(
                owner,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            )
            .unwrap();
        assert_eq!(
            server.admit_moderator(moderator, moderator, town, model::RequestId::new(2)),
            Err(ErrorCode::Unauthorized)
        );
        assert_eq!(
            server.admit_moderator(owner, moderator, town, model::RequestId::new(3)),
            Ok(Response::Accepted { revision: 1 })
        );
        assert_eq!(server.session_role(moderator), Ok(ServerRole::Moderator));
        assert_eq!(
            server.execute(
                moderator,
                model::RequestId::new(4),
                1,
                Command::Apply {
                    town_id: town,
                    command: model::Command::SetPresence {
                        x: 0,
                        y: 0,
                        facing: 0
                    }
                }
            ),
            Err(ErrorCode::Unauthorized)
        );
        let _ = fs::remove_dir_all(root);
    }
}
