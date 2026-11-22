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
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_FRAME_BYTES: usize = nookery_social_proto::MAX_FRAME_BYTES;
pub const MAX_IDEMPOTENCY_ENTRIES: usize = 4096;
pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_millis(250);
pub const DEFAULT_NONCE_TTL: Duration = Duration::from_millis(500);
const AUTH_NONCE_BYTES: usize = 32;
const AUTH_TOKEN_BYTES: usize = 32;
const MAX_READ_CHUNK: usize = 4096;
const MAX_USED_NONCES: usize = 4096;

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
    Sync {
        town_id: TownKey,
    },
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
    State { revision: u64, bytes: Vec<u8> },
    Rejected(ErrorCode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
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
    AuthRequired,
    Protocol,
    NoToken,
    BadToken,
    ExpiredNonce,
    ReplayedNonce,
    TimedOut,
    RoleEscalation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AuthStatus {
    Authenticated = 0,
    NoToken = 1,
    BadToken = 2,
    ExpiredNonce = 3,
    ReplayedNonce = 4,
    Malformed = 5,
}

impl AuthStatus {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Authenticated),
            1 => Some(Self::NoToken),
            2 => Some(Self::BadToken),
            3 => Some(Self::ExpiredNonce),
            4 => Some(Self::ReplayedNonce),
            5 => Some(Self::Malformed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryPath {
    Snapshot,
    CommandLog,
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
    recovery: BTreeMap<TownKey, RecoveryPath>,
}

impl TownServer {
    pub fn open(root_store: Store, lease: Duration) -> Result<Self, ServerError> {
        if lease.is_zero() {
            return Err(ServerError::InvalidConfig("session lease must be nonzero"));
        }
        let mut server = Self {
            root_store,
            towns: BTreeMap::new(),
            sessions: HashMap::new(),
            idempotency: VecDeque::new(),
            lease,
            recovery: BTreeMap::new(),
        };
        server.recover_towns()?;
        Ok(server)
    }

    /// Reports how each hosted town was restored during [`Self::open`].
    #[must_use]
    pub fn recovery_paths(&self) -> &BTreeMap<TownKey, RecoveryPath> {
        &self.recovery
    }

    /// Returns the canonical persisted public state bytes for deterministic
    /// client synchronization and test comparison.
    pub fn state_bytes(&mut self, town_id: TownKey) -> Result<Vec<u8>, ErrorCode> {
        self.expire_due();
        self.towns
            .get(&town_id)
            .map(|town| encode_model_snapshot(&town.model.snapshot()))
            .ok_or(ErrorCode::NotFound)
    }

    fn recover_towns(&mut self) -> Result<(), ServerError> {
        for town_bytes in self.root_store.town_ids()? {
            let town_id = TownKey::new(town_bytes);
            let store = self.root_store.for_town(town_bytes)?;
            let (stored_snapshot, commands) = store.recover()?;
            let (town, path) = if let Some(snapshot) = stored_snapshot {
                let stored_revision = snapshot.revision;
                let snapshot = decode_model_snapshot(&snapshot.bytes)
                    .map_err(|_| ServerError::InvalidConfig("invalid town snapshot"))?;
                if snapshot.revision() != stored_revision {
                    return Err(ServerError::InvalidConfig(
                        "town snapshot revision mismatch",
                    ));
                }
                if snapshot.town_id().bytes() != town_bytes {
                    return Err(ServerError::InvalidConfig(
                        "town snapshot identity mismatch",
                    ));
                }
                (model::Town::from_snapshot(snapshot), RecoveryPath::Snapshot)
            } else {
                (
                    replay_command_log(&commands, town_id)
                        .map_err(|_| ServerError::InvalidConfig("invalid town command log"))?,
                    RecoveryPath::CommandLog,
                )
            };
            for active in town.snapshot().sessions() {
                self.sessions.insert(
                    active.session_id(),
                    SessionLease {
                        actor: active.actor(),
                        town: Some(town_id),
                        role: server_role(active.role()),
                        expires_at: Instant::now() + self.lease,
                    },
                );
            }
            self.recovery.insert(town_id, path);
            self.towns
                .insert(town_id, HostedTown { model: town, store });
        }
        Ok(())
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
            Command::Sync { town_id } => self.sync(session, town_id)?,
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
            membership_bytes(2, target_session, target.actor, town_id),
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
        persist_state(
            &store,
            snapshot.revision(),
            &snapshot,
            model::RequestId::new(session.get()),
            create_bytes(session, actor, town_id),
        )?;
        self.towns
            .insert(town_id, HostedTown { model: town, store });
        let lease = self.sessions.get_mut(&session).ok_or(ErrorCode::Expired)?;
        lease.town = Some(town_id);
        lease.role = ServerRole::Owner;
        Ok(Response::Accepted { revision: 0 })
    }

    fn sync(&mut self, session: model::SessionId, town_id: TownKey) -> Result<Response, ErrorCode> {
        let lease = self.sessions.get(&session).ok_or(ErrorCode::Expired)?;
        if lease.town != Some(town_id) {
            return Err(ErrorCode::Unauthorized);
        }
        let hosted = self.towns.get(&town_id).ok_or(ErrorCode::NotFound)?;
        let snapshot = hosted.model.snapshot();
        Ok(Response::State {
            revision: snapshot.revision(),
            bytes: encode_model_snapshot(&snapshot),
        })
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
        // Recovery keeps the model's owner membership, but admission still
        // requires the corresponding live server lease. An expired owner must
        // never authorize a new visitor from persisted state alone.
        let owner_lease = self.sessions.get(&owner).ok_or(ErrorCode::NotFound)?;
        if owner_lease.role != ServerRole::Owner || owner_lease.town != Some(town_id) {
            return Err(ErrorCode::NotFound);
        }
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
            membership_bytes(1, session, target_actor, town_id),
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
            membership_bytes(3, session, lease.actor, town_id),
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
                command_bytes(town_id, session, command),
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
                [3, rest @ ..] => Ok(Command::Sync {
                    town_id: parse_town(rest)?,
                }),
                _ => Err(ErrorCode::Malformed),
            },
            MessageType::JoinTown => Ok(Command::Join {
                town_id: parse_town(&frame.payload)?,
            }),
            MessageType::LeaveTown => Ok(Command::Leave {
                town_id: parse_town(&frame.payload)?,
            }),
            MessageType::Command => parse_model_command(&frame.payload),
            MessageType::Snapshot
            | MessageType::Error
            | MessageType::AuthChallenge
            | MessageType::AuthResponse
            | MessageType::AuthResult => Err(ErrorCode::Malformed),
        }
    }

    fn encode_response(
        &self,
        request: &Frame,
        response: Response,
    ) -> Result<Vec<u8>, TransportError> {
        let (message_type, revision, payload) = match response {
            Response::Accepted { revision } => (MessageType::Snapshot, revision, vec![0u8; 9]),
            Response::Discovered(towns) => {
                let count = u8::try_from(towns.len()).unwrap_or(u8::MAX);
                let mut payload = vec![0, 1, count];
                for town in towns.into_iter().take(usize::from(count)) {
                    payload.extend_from_slice(&town.bytes());
                }
                (MessageType::Snapshot, 0, payload)
            }
            Response::State { revision, bytes } => (MessageType::Snapshot, revision, bytes),
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
                        if persist_state(
                            &hosted.store,
                            snapshot.revision(),
                            &snapshot,
                            model::RequestId::new(session.get()),
                            membership_bytes(3, session, lease.actor, town_id),
                        )
                        .is_ok()
                        {
                            if let Some(current) = self.towns.get_mut(&town_id) {
                                current.model = candidate;
                            }
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
        5 => return Err(ErrorCode::RoleEscalation),
        _ => return Err(ErrorCode::Malformed),
    };
    Ok(Command::Apply { town_id, command })
}

fn command_bytes(town: TownKey, session: model::SessionId, command: model::Command) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(25);
    match command {
        model::Command::SetPresence { x, y, facing } => {
            bytes.push(1);
            bytes.extend_from_slice(&town.bytes());
            bytes.extend_from_slice(&x.to_be_bytes());
            bytes.extend_from_slice(&y.to_be_bytes());
            bytes.push(facing);
        }
        model::Command::DropItem { item_id, x, y } => {
            bytes.push(2);
            bytes.extend_from_slice(&town.bytes());
            bytes.extend_from_slice(&item_id.to_be_bytes());
            bytes.extend_from_slice(&x.to_be_bytes());
            bytes.extend_from_slice(&y.to_be_bytes());
        }
        model::Command::TakeItem { item_id } => {
            bytes.push(3);
            bytes.extend_from_slice(&town.bytes());
            bytes.extend_from_slice(&item_id.to_be_bytes());
        }
        model::Command::CloseTown => {
            bytes.push(4);
            bytes.extend_from_slice(&town.bytes());
        }
    }
    bytes.extend_from_slice(&session.get().to_be_bytes());
    bytes
}

fn model_command_payload(town: TownKey, command: model::Command) -> Vec<u8> {
    let mut payload = Vec::with_capacity(25);
    match command {
        model::Command::SetPresence { x, y, facing } => {
            payload.push(1);
            payload.extend_from_slice(&town.bytes());
            payload.extend_from_slice(&x.to_be_bytes());
            payload.extend_from_slice(&y.to_be_bytes());
            payload.push(facing);
        }
        model::Command::DropItem { item_id, x, y } => {
            payload.push(2);
            payload.extend_from_slice(&town.bytes());
            payload.extend_from_slice(&item_id.to_be_bytes());
            payload.extend_from_slice(&x.to_be_bytes());
            payload.extend_from_slice(&y.to_be_bytes());
        }
        model::Command::TakeItem { item_id } => {
            payload.push(3);
            payload.extend_from_slice(&town.bytes());
            payload.extend_from_slice(&item_id.to_be_bytes());
        }
        model::Command::CloseTown => {
            payload.push(4);
            payload.extend_from_slice(&town.bytes());
        }
    }
    payload
}

fn create_bytes(session: model::SessionId, actor: model::ActorId, town: TownKey) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend_from_slice(&session.get().to_be_bytes());
    bytes.extend_from_slice(&actor.get().to_be_bytes());
    bytes.extend_from_slice(&town.bytes());
    bytes
}

fn membership_bytes(
    kind: u8,
    session: model::SessionId,
    actor: model::ActorId,
    town: TownKey,
) -> Vec<u8> {
    let mut bytes = vec![kind];
    bytes.extend_from_slice(&session.get().to_be_bytes());
    bytes.extend_from_slice(&actor.get().to_be_bytes());
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

fn decode_model_snapshot(bytes: &[u8]) -> Result<model::Snapshot, ()> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(7)? != b"NKSNAP1" {
        return Err(());
    }
    let town_id = model::TownId::new(cursor.array::<16>()?);
    let revision = cursor.u64()?;
    let closed = cursor.byte()? != 0;
    let session_count = usize::from(cursor.u16()?);
    if session_count > model::MAX_VISITORS + model::MAX_MODERATORS + 1 {
        return Err(());
    }
    let mut sessions = Vec::with_capacity(session_count);
    let mut owner_count = 0usize;
    let mut visitor_count = 0usize;
    let mut moderator_count = 0usize;
    for _ in 0..session_count {
        let session_id = model::SessionId::new(cursor.u64()?);
        let actor = model::ActorId::new(cursor.u64()?);
        if session_id.get() == 0 || actor.get() == 0 {
            return Err(());
        }
        let role = match cursor.byte()? {
            1 => {
                owner_count += 1;
                model::Role::Owner
            }
            2 => {
                visitor_count += 1;
                model::Role::Visitor
            }
            3 => {
                moderator_count += 1;
                model::Role::Moderator
            }
            _ => return Err(()),
        };
        let presence = match cursor.byte()? {
            0 => None,
            1 => {
                let x = cursor.i16()?;
                let y = cursor.i16()?;
                let facing = cursor.byte()?;
                if !(model::MIN_COORDINATE..=model::MAX_COORDINATE).contains(&x)
                    || !(model::MIN_COORDINATE..=model::MAX_COORDINATE).contains(&y)
                    || facing >= model::FACING_VALUES
                {
                    return Err(());
                }
                Some(model_presence(x, y, facing))
            }
            _ => return Err(()),
        };
        if sessions.iter().any(|active: &model::SessionSnapshot| {
            active.session_id() == session_id || active.actor() == actor
        }) {
            return Err(());
        }
        sessions.push(model::SessionSnapshot::from_parts(
            session_id, actor, role, presence,
        ));
    }
    if owner_count != 1
        || visitor_count > model::MAX_VISITORS
        || moderator_count > model::MAX_MODERATORS
    {
        return Err(());
    }
    let item_count = usize::from(cursor.u16()?);
    if item_count > model::MAX_ITEMS {
        return Err(());
    }
    let mut items = Vec::with_capacity(item_count);
    for _ in 0..item_count {
        let item_id = cursor.u32()?;
        let x = cursor.i16()?;
        let y = cursor.i16()?;
        let owner = model::ActorId::new(cursor.u64()?);
        if !(model::MIN_COORDINATE..=model::MAX_COORDINATE).contains(&x)
            || !(model::MIN_COORDINATE..=model::MAX_COORDINATE).contains(&y)
            || items
                .iter()
                .any(|item: &model::DroppedItem| item.item_id() == item_id)
        {
            return Err(());
        }
        items.push(model::DroppedItem::from_parts(item_id, x, y, owner));
    }
    if !cursor.is_empty() {
        return Err(());
    }
    Ok(model::Snapshot::from_parts(
        town_id, revision, closed, sessions, items,
    ))
}

fn model_presence(x: i16, y: i16, facing: u8) -> model::Presence {
    // The model validates these values whenever a command creates them. A
    // persisted snapshot is also bounded by decode_model_snapshot's sizes.
    // Reuse its public state constructor by deriving the value through a
    // tiny valid town operation is unnecessary; Presence is opaque by design,
    // so this helper is supplied by the model crate below.
    model::Presence::from_parts(x, y, facing)
}

fn replay_command_log(
    commands: &[CommandRecord],
    expected_town: TownKey,
) -> Result<model::Town, ()> {
    let first = commands.first().ok_or(())?;
    if first.revision != 0 || first.bytes.len() != 1 + 8 + 8 + 16 || first.bytes[0] != 0 {
        return Err(());
    }
    let session = model::SessionId::new(u64::from_be_bytes(
        first.bytes[1..9].try_into().map_err(|_| ())?,
    ));
    let actor = model::ActorId::new(u64::from_be_bytes(
        first.bytes[9..17].try_into().map_err(|_| ())?,
    ));
    let town = TownKey::new(first.bytes[17..33].try_into().map_err(|_| ())?);
    if town != expected_town || session.get() == 0 || actor.get() == 0 {
        return Err(());
    }
    let mut town = model::Town::new(model::TownId::new(town.bytes()), actor, session);
    for record in &commands[1..] {
        let request = model::RequestId::new(u64::from_be_bytes(
            record.idempotency_key[..8].try_into().map_err(|_| ())?,
        ));
        if record.bytes.len() == 1 + 8 + 8 + 16 && matches!(record.bytes[0], 1..=3) {
            let target = model::SessionId::new(u64::from_be_bytes(
                record.bytes[1..9].try_into().map_err(|_| ())?,
            ));
            let target_actor = model::ActorId::new(u64::from_be_bytes(
                record.bytes[9..17].try_into().map_err(|_| ())?,
            ));
            let record_town = TownKey::new(record.bytes[17..33].try_into().map_err(|_| ())?);
            if record_town != expected_town {
                return Err(());
            }
            let owner = town
                .snapshot()
                .sessions()
                .iter()
                .find(|active| active.role() == model::Role::Owner)
                .ok_or(())?
                .session_id();
            match record.bytes[0] {
                1 => town
                    .admit_visitor(owner, target, target_actor)
                    .map_err(|_| ())?,
                2 => town
                    .admit_moderator(owner, target, target_actor)
                    .map_err(|_| ())?,
                3 => town.leave(target).map_err(|_| ())?,
                _ => return Err(()),
            };
            continue;
        }
        if record.bytes.len() < 25 {
            return Err(());
        }
        let command =
            parse_model_command(&record.bytes[..record.bytes.len() - 8]).map_err(|_| ())?;
        let Command::Apply { town_id, command } = command else {
            return Err(());
        };
        if town_id != expected_town {
            return Err(());
        }
        // Command records carry the originating session in their final eight
        // bytes so replay preserves model authorization.
        let session = model::SessionId::new(u64::from_be_bytes(
            record.bytes[record.bytes.len() - 8..]
                .try_into()
                .map_err(|_| ())?,
        ));
        town.apply(session, town.snapshot().revision(), request, command)
            .map_err(|_| ())?;
    }
    Ok(town)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ()> {
        let end = self.offset.checked_add(length).ok_or(())?;
        let value = self.bytes.get(self.offset..end).ok_or(())?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, ()> {
        Ok(*self.take(1)?.first().ok_or(())?)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ()> {
        self.take(N)?.try_into().map_err(|_| ())
    }

    fn u16(&mut self) -> Result<u16, ()> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, ()> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ()> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn i16(&mut self) -> Result<i16, ()> {
        Ok(i16::from_be_bytes(self.array()?))
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn server_role(role: model::Role) -> ServerRole {
    match role {
        model::Role::Owner => ServerRole::Owner,
        model::Role::Visitor => ServerRole::Visitor,
        model::Role::Moderator => ServerRole::Moderator,
    }
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

/// Computes the token used by the loopback challenge-response handshake.
/// The message binds the proof to the nonce, session, actor, and town, so a
/// token for one hosted town cannot be replayed for another.
#[must_use]
pub fn session_token(
    secret: &[u8],
    nonce: [u8; AUTH_NONCE_BYTES],
    session: model::SessionId,
    actor: model::ActorId,
    town: TownKey,
) -> [u8; AUTH_TOKEN_BYTES] {
    let mut message = Vec::with_capacity(32 + 8 + 8 + 16);
    message.extend_from_slice(&nonce);
    message.extend_from_slice(&session.get().to_be_bytes());
    message.extend_from_slice(&actor.get().to_be_bytes());
    message.extend_from_slice(&town.bytes());
    hmac_sha256(secret, &message)
}

/// Self-contained HMAC-SHA256 used only for the local clean-room handshake.
#[must_use]
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > key_block.len() {
        key_block[..32].copy_from_slice(&sha256(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + message.len());
    let mut outer = Vec::with_capacity(64 + 32);
    for byte in key_block {
        inner.push(byte ^ 0x36);
        outer.push(byte ^ 0x5c);
    }
    inner.extend_from_slice(message);
    outer.extend_from_slice(&sha256(&inner));
    sha256(&outer)
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut data = input.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    let bit_len = (input.len() as u64).wrapping_mul(8);
    data.extend_from_slice(&bit_len.to_be_bytes());
    let mut hash: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    for chunk in data.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, bytes) in chunk.chunks_exact(4).take(16).enumerate() {
            words[index] = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
            hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
        );
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            (h, g, f, e, d, c, b, a) = (
                g,
                f,
                e,
                d.wrapping_add(temp1),
                c,
                b,
                a,
                temp1.wrapping_add(temp2),
            );
        }
        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }
    let mut output = [0u8; 32];
    for (index, word) in hash.into_iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

#[derive(Debug)]
pub enum NetError {
    Io(io::Error),
    Frame(DecodeError),
    Encode(EncodeError),
    Protocol(&'static str),
    Auth(AuthStatus),
    Refused(ErrorCode),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for NetError {}

impl From<io::Error> for NetError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<TransportError> for NetError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::Frame(error) => Self::Frame(error),
            TransportError::Encode(error) => Self::Encode(error),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LoopbackConfig {
    pub io_timeout: Duration,
    pub nonce_ttl: Duration,
}

impl Default for LoopbackConfig {
    fn default() -> Self {
        Self {
            io_timeout: DEFAULT_IO_TIMEOUT,
            nonce_ttl: DEFAULT_NONCE_TTL,
        }
    }
}

struct NonceBook {
    used: VecDeque<[u8; AUTH_NONCE_BYTES]>,
}

impl NonceBook {
    fn new() -> Self {
        Self {
            used: VecDeque::new(),
        }
    }

    fn contains(&self, nonce: &[u8; AUTH_NONCE_BYTES]) -> bool {
        self.used.iter().any(|used| used == nonce)
    }

    fn consume(&mut self, nonce: [u8; AUTH_NONCE_BYTES]) -> bool {
        if self.contains(&nonce) {
            return false;
        }
        self.used.push_back(nonce);
        while self.used.len() > MAX_USED_NONCES {
            self.used.pop_front();
        }
        true
    }
}

/// A loopback-only TCP server around [`TownServer`].
pub struct LoopbackServer {
    listener: TcpListener,
    context: ConnectionContext,
}

#[derive(Clone)]
struct ConnectionContext {
    state: Arc<Mutex<TownServer>>,
    secret: Arc<Vec<u8>>,
    config: LoopbackConfig,
    nonces: Arc<Mutex<NonceBook>>,
    nonce_counter: Arc<AtomicU64>,
    port: u16,
}

impl LoopbackServer {
    pub fn bind(
        town_server: TownServer,
        secret: impl AsRef<[u8]>,
        config: LoopbackConfig,
    ) -> Result<Self, NetError> {
        if secret.as_ref().is_empty() || config.io_timeout.is_zero() || config.nonce_ttl.is_zero() {
            return Err(NetError::Protocol("invalid loopback configuration"));
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            context: ConnectionContext {
                state: Arc::new(Mutex::new(town_server)),
                secret: Arc::new(secret.as_ref().to_vec()),
                config,
                nonces: Arc::new(Mutex::new(NonceBook::new())),
                nonce_counter: Arc::new(AtomicU64::new(1)),
                port,
            },
        })
    }

    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 0)))
    }

    /// Starts an owned accept loop. Dropping the handle closes the listener
    /// and joins the worker, so tests never leave a socket running.
    pub fn spawn(self) -> RunningServer {
        let addr = self.local_addr();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || self.run(thread_stop));
        RunningServer {
            addr,
            stop,
            worker: Some(worker),
        }
    }

    fn run(self, stop: Arc<AtomicBool>) {
        while !stop.load(Ordering::Acquire) {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let worker = self.context.clone();
                    thread::spawn(move || {
                        let _ = worker.handle_connection(stream);
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(_) => break,
            }
        }
    }
}

impl ConnectionContext {
    fn handle_connection(&self, mut stream: TcpStream) -> Result<(), NetError> {
        stream.set_read_timeout(Some(self.config.io_timeout))?;
        stream.set_write_timeout(Some(self.config.io_timeout))?;
        let nonce = self.make_nonce();
        let nonce_issued = Instant::now();
        write_wire_frame(
            &mut stream,
            &Frame {
                version: PROTOCOL_VERSION,
                message_type: MessageType::AuthChallenge,
                flags: 0,
                request_id: 0,
                sequence: 1,
                town_revision: 0,
                payload: nonce.to_vec(),
            },
        )?;
        let mut decoder = nookery_social_proto::FrameDecoder::new();
        let auth =
            match read_wire_frame_with_budget(&mut stream, &mut decoder, self.config.io_timeout) {
                Ok(frame) => frame,
                Err(error) => {
                    let code = classify_read_error(&error);
                    let _ = write_error_frame(&mut stream, 0, 1, code);
                    return Err(error);
                }
            };
        let (session, actor, town, status) = self.authenticate(nonce, nonce_issued, &auth);
        if status != AuthStatus::Authenticated {
            let _ = write_auth_result(&mut stream, status);
            return Err(NetError::Auth(status));
        }
        let (session, actor, town) = (
            session.ok_or(NetError::Protocol("auth session"))?,
            actor.ok_or(NetError::Protocol("auth actor"))?,
            town.ok_or(NetError::Protocol("auth town"))?,
        );
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| NetError::Protocol("server state poisoned"))?;
            state.connect(session, actor).map_err(NetError::Refused)?;
        }
        write_auth_result(&mut stream, AuthStatus::Authenticated)?;
        loop {
            let frame = match read_wire_frame_with_budget(
                &mut stream,
                &mut decoder,
                self.config.io_timeout,
            ) {
                Ok(frame) => frame,
                Err(error) => {
                    let code = classify_read_error(&error);
                    let _ = write_error_frame(&mut stream, 0, 1, code);
                    return Err(error);
                }
            };
            if frame.flags != 0 || frame.request_id == 0 || frame.sequence == 0 {
                let _ = write_error_frame(
                    &mut stream,
                    frame.request_id,
                    frame.sequence,
                    ErrorCode::Malformed,
                );
                return Err(NetError::Protocol("invalid request envelope"));
            }
            match authorized_town(&frame, town) {
                Ok(true) => {}
                Ok(false) => {
                    write_error_frame(
                        &mut stream,
                        frame.request_id,
                        frame.sequence,
                        ErrorCode::Unauthorized,
                    )?;
                    continue;
                }
                Err(error) => {
                    write_error_frame(&mut stream, frame.request_id, frame.sequence, error)?;
                    continue;
                }
            }
            let malformed = {
                let state = self
                    .state
                    .lock()
                    .map_err(|_| NetError::Protocol("server state poisoned"))?;
                state.parse_frame_command(&frame).is_err()
            };
            let response = {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| NetError::Protocol("server state poisoned"))?;
                state.handle_frame(session, &frame.encode().map_err(NetError::Encode)?)?
            };
            stream.write_all(&response)?;
            if malformed {
                return Err(NetError::Protocol("malformed command"));
            }
        }
    }

    fn make_nonce(&self) -> [u8; AUTH_NONCE_BYTES] {
        let counter = self.nonce_counter.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos() as u64);
        let mut source = Vec::with_capacity(24);
        source.extend_from_slice(&now.to_be_bytes());
        source.extend_from_slice(&counter.to_be_bytes());
        source.extend_from_slice(&self.port.to_be_bytes());
        hmac_sha256(&self.secret, &source)
    }

    fn authenticate(
        &self,
        challenge: [u8; AUTH_NONCE_BYTES],
        issued: Instant,
        frame: &Frame,
    ) -> (
        Option<model::SessionId>,
        Option<model::ActorId>,
        Option<TownKey>,
        AuthStatus,
    ) {
        if issued.elapsed() >= self.config.nonce_ttl {
            return (None, None, None, AuthStatus::ExpiredNonce);
        }
        if frame.message_type != MessageType::AuthResponse || frame.flags != 0 {
            return (None, None, None, AuthStatus::Malformed);
        }
        if frame.payload.len() < AUTH_NONCE_BYTES + 8 + 8 + 16 {
            return (None, None, None, AuthStatus::NoToken);
        }
        let claimed = match frame.payload[..AUTH_NONCE_BYTES].try_into() {
            Ok(value) => value,
            Err(_) => return (None, None, None, AuthStatus::Malformed),
        };
        let nonce_status = match self.nonces.lock() {
            Ok(mut nonces) => {
                if nonces.contains(&claimed) || !nonces.consume(claimed) {
                    AuthStatus::ReplayedNonce
                } else {
                    AuthStatus::Authenticated
                }
            }
            Err(_) => AuthStatus::Malformed,
        };
        if nonce_status == AuthStatus::ReplayedNonce {
            return (None, None, None, nonce_status);
        }
        if claimed != challenge {
            return (None, None, None, AuthStatus::BadToken);
        }
        let session =
            model::SessionId::new(u64::from_be_bytes(match frame.payload[32..40].try_into() {
                Ok(value) => value,
                Err(_) => return (None, None, None, AuthStatus::Malformed),
            }));
        let actor =
            model::ActorId::new(u64::from_be_bytes(match frame.payload[40..48].try_into() {
                Ok(value) => value,
                Err(_) => return (None, None, None, AuthStatus::Malformed),
            }));
        let town = TownKey::new(match frame.payload[48..64].try_into() {
            Ok(value) => value,
            Err(_) => return (None, None, None, AuthStatus::Malformed),
        });
        let token = match frame.payload.get(64..).filter(|value| value.len() == 32) {
            Some(value) => value,
            None => return (None, None, None, AuthStatus::NoToken),
        };
        let expected = session_token(&self.secret, challenge, session, actor, town);
        if !constant_time_eq(token, &expected) {
            return (None, None, None, AuthStatus::BadToken);
        }
        (
            Some(session),
            Some(actor),
            Some(town),
            AuthStatus::Authenticated,
        )
    }
}

pub struct RunningServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl RunningServer {
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn authorized_town(frame: &Frame, auth_town: TownKey) -> Result<bool, ErrorCode> {
    let zero = TownKey::new([0; 16]);
    match frame.message_type {
        MessageType::Hello => match frame.payload.first() {
            Some(1 | 2) => Ok(auth_town == zero),
            Some(3) => Ok(frame
                .payload
                .get(1..)
                .and_then(|value| value.try_into().ok())
                .map(TownKey::new)
                == Some(auth_town)),
            _ => Err(ErrorCode::Malformed),
        },
        MessageType::JoinTown | MessageType::LeaveTown => {
            Ok(frame.payload.as_slice().try_into().ok().map(TownKey::new) == Some(auth_town))
        }
        MessageType::Command => {
            let Command::Apply { town_id, .. } = parse_model_command(&frame.payload)? else {
                return Err(ErrorCode::Malformed);
            };
            Ok(town_id == auth_town)
        }
        _ => Err(ErrorCode::Malformed),
    }
}

fn classify_read_error(error: &NetError) -> ErrorCode {
    match error {
        NetError::Io(io_error)
            if matches!(
                io_error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ) =>
        {
            ErrorCode::TimedOut
        }
        _ => ErrorCode::Protocol,
    }
}

fn write_wire_frame(stream: &mut TcpStream, frame: &Frame) -> Result<(), NetError> {
    stream.write_all(&frame.encode().map_err(NetError::Encode)?)?;
    Ok(())
}

fn write_auth_result(stream: &mut TcpStream, status: AuthStatus) -> Result<(), NetError> {
    write_wire_frame(
        stream,
        &Frame {
            version: PROTOCOL_VERSION,
            message_type: MessageType::AuthResult,
            flags: 0,
            request_id: 0,
            sequence: 1,
            town_revision: 0,
            payload: vec![status as u8],
        },
    )
}

fn write_error_frame(
    stream: &mut TcpStream,
    request_id: u64,
    sequence: u64,
    code: ErrorCode,
) -> Result<(), NetError> {
    write_wire_frame(
        stream,
        &Frame {
            version: PROTOCOL_VERSION,
            message_type: MessageType::Error,
            flags: 0,
            request_id,
            sequence,
            town_revision: 0,
            payload: vec![1, code as u8],
        },
    )
}

fn read_wire_frame(
    stream: &mut TcpStream,
    decoder: &mut nookery_social_proto::FrameDecoder,
) -> Result<Frame, NetError> {
    read_wire_frame_with_budget(stream, decoder, DEFAULT_IO_TIMEOUT)
}

fn read_wire_frame_with_budget(
    stream: &mut TcpStream,
    decoder: &mut nookery_social_proto::FrameDecoder,
    budget: Duration,
) -> Result<Frame, NetError> {
    let mut buffer = [0u8; MAX_READ_CHUNK];
    let started = Instant::now();
    loop {
        let elapsed = started.elapsed();
        if elapsed >= budget {
            return Err(NetError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "frame read budget elapsed",
            )));
        }
        stream.set_read_timeout(Some(budget.saturating_sub(elapsed)))?;
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(NetError::Protocol(if decoder.buffered_len() == 0 {
                "peer closed"
            } else {
                "truncated frame"
            }));
        }
        let frames = decoder.push(&buffer[..count]).map_err(NetError::Frame)?;
        if frames.len() > 1 {
            return Err(NetError::Protocol("multiple frames in one read"));
        }
        if let Some(frame) = frames.into_iter().next() {
            return Ok(frame);
        }
    }
}

/// Small client connector using the identical frame envelope and handshake.
pub struct SocialClient {
    stream: TcpStream,
    sequence: u64,
    request: u64,
    decoder: nookery_social_proto::FrameDecoder,
}

impl SocialClient {
    pub fn connect(
        addr: SocketAddr,
        secret: &[u8],
        session: model::SessionId,
        actor: model::ActorId,
        town: TownKey,
    ) -> Result<Self, NetError> {
        Self::connect_inner(addr, secret, session, actor, town, true)
    }

    pub fn connect_without_token(
        addr: SocketAddr,
        session: model::SessionId,
        actor: model::ActorId,
        town: TownKey,
    ) -> Result<Self, NetError> {
        Self::connect_inner(addr, &[], session, actor, town, false)
    }

    fn connect_inner(
        addr: SocketAddr,
        secret: &[u8],
        session: model::SessionId,
        actor: model::ActorId,
        town: TownKey,
        include_token: bool,
    ) -> Result<Self, NetError> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(DEFAULT_IO_TIMEOUT))?;
        stream.set_write_timeout(Some(DEFAULT_IO_TIMEOUT))?;
        let mut decoder = nookery_social_proto::FrameDecoder::new();
        let challenge = read_wire_frame(&mut stream, &mut decoder)?;
        if challenge.message_type != MessageType::AuthChallenge
            || challenge.payload.len() != AUTH_NONCE_BYTES
        {
            return Err(NetError::Protocol("invalid auth challenge"));
        }
        let nonce: [u8; AUTH_NONCE_BYTES] = challenge
            .payload
            .try_into()
            .map_err(|_| NetError::Protocol("nonce"))?;
        let mut payload = Vec::with_capacity(64 + 32);
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&session.get().to_be_bytes());
        payload.extend_from_slice(&actor.get().to_be_bytes());
        payload.extend_from_slice(&town.bytes());
        if include_token {
            payload.extend_from_slice(&session_token(secret, nonce, session, actor, town));
        }
        write_wire_frame(
            &mut stream,
            &Frame {
                version: PROTOCOL_VERSION,
                message_type: MessageType::AuthResponse,
                flags: 0,
                request_id: 0,
                sequence: 1,
                town_revision: 0,
                payload,
            },
        )?;
        let result = read_wire_frame(&mut stream, &mut decoder)?;
        if result.message_type != MessageType::AuthResult || result.payload.len() != 1 {
            return Err(NetError::Protocol("invalid auth result"));
        }
        let status =
            AuthStatus::from_byte(result.payload[0]).ok_or(NetError::Protocol("auth status"))?;
        if status != AuthStatus::Authenticated {
            return Err(NetError::Auth(status));
        }
        Ok(Self {
            stream,
            sequence: 1,
            request: 1,
            decoder,
        })
    }

    pub fn discover(&mut self) -> Result<Vec<TownKey>, NetError> {
        match self.request(MessageType::Hello, vec![2], 0)? {
            Response::Discovered(towns) => Ok(towns),
            Response::Rejected(error) => Err(NetError::Refused(error)),
            _ => Err(NetError::Protocol("discover response")),
        }
    }

    pub fn create_town(&mut self, town: TownKey) -> Result<u64, NetError> {
        let mut payload = vec![1];
        payload.extend_from_slice(&town.bytes());
        let response = self.request(MessageType::Hello, payload, 0)?;
        self.accepted(response)
    }

    pub fn join(&mut self, town: TownKey) -> Result<u64, NetError> {
        let response = self.request(MessageType::JoinTown, town.bytes().to_vec(), 0)?;
        self.accepted(response)
    }

    pub fn leave(&mut self, town: TownKey, revision: u64) -> Result<u64, NetError> {
        let response = self.request(MessageType::LeaveTown, town.bytes().to_vec(), revision)?;
        self.accepted(response)
    }

    pub fn sync(&mut self, town: TownKey) -> Result<(u64, Vec<u8>), NetError> {
        let mut payload = vec![3];
        payload.extend_from_slice(&town.bytes());
        match self.request(MessageType::Hello, payload, 0)? {
            Response::State { revision, bytes } => Ok((revision, bytes)),
            Response::Rejected(error) => Err(NetError::Refused(error)),
            _ => Err(NetError::Protocol("sync response")),
        }
    }

    pub fn apply(
        &mut self,
        town: TownKey,
        revision: u64,
        command: model::Command,
    ) -> Result<u64, NetError> {
        let payload = model_command_payload(town, command);
        let response = self.request(MessageType::Command, payload, revision)?;
        self.accepted(response)
    }

    fn accepted(&self, response: Response) -> Result<u64, NetError> {
        match response {
            Response::Accepted { revision } => Ok(revision),
            Response::Rejected(error) => Err(NetError::Refused(error)),
            _ => Err(NetError::Protocol("accepted response")),
        }
    }

    fn request(
        &mut self,
        message_type: MessageType,
        payload: Vec<u8>,
        revision: u64,
    ) -> Result<Response, NetError> {
        self.request = self
            .request
            .checked_add(1)
            .ok_or(NetError::Protocol("request exhausted"))?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(NetError::Protocol("sequence exhausted"))?;
        write_wire_frame(
            &mut self.stream,
            &Frame {
                version: PROTOCOL_VERSION,
                message_type,
                flags: 0,
                request_id: self.request,
                sequence: self.sequence,
                town_revision: revision,
                payload,
            },
        )?;
        let frame = read_wire_frame(&mut self.stream, &mut self.decoder)?;
        Self::decode_response(frame)
    }

    fn decode_response(frame: Frame) -> Result<Response, NetError> {
        if frame.message_type == MessageType::Error {
            if frame.payload.len() == 2 && frame.payload[0] == 1 {
                let code = error_code_from_byte(frame.payload[1])
                    .ok_or(NetError::Protocol("error code"))?;
                return Ok(Response::Rejected(code));
            }
            return Err(NetError::Protocol("error response"));
        }
        if frame.message_type != MessageType::Snapshot {
            return Err(NetError::Protocol("command response"));
        }
        if frame.payload.starts_with(b"NKSNAP1") {
            return Ok(Response::State {
                revision: frame.town_revision,
                bytes: frame.payload,
            });
        }
        if frame.payload.len() == 9 && frame.payload[..1] == [0] {
            return Ok(Response::Accepted {
                revision: frame.town_revision,
            });
        }
        if frame.payload.len() >= 3 && frame.payload[..2] == [0, 1] {
            let count = usize::from(frame.payload[2]);
            if frame.payload.len() != 3 + count * 16 {
                return Err(NetError::Protocol("discovery payload"));
            }
            let mut towns = Vec::with_capacity(count);
            for chunk in frame.payload[3..].chunks_exact(16) {
                towns.push(TownKey::new(
                    chunk
                        .try_into()
                        .map_err(|_| NetError::Protocol("town id"))?,
                ));
            }
            return Ok(Response::Discovered(towns));
        }
        Err(NetError::Protocol("snapshot payload"))
    }
}

fn error_code_from_byte(value: u8) -> Option<ErrorCode> {
    let values = [
        ErrorCode::Malformed,
        ErrorCode::Unauthorized,
        ErrorCode::NotFound,
        ErrorCode::Duplicate,
        ErrorCode::Conflict,
        ErrorCode::StaleRevision,
        ErrorCode::Closed,
        ErrorCode::Expired,
        ErrorCode::TooLarge,
        ErrorCode::AuthRequired,
        ErrorCode::Protocol,
        ErrorCode::NoToken,
        ErrorCode::BadToken,
        ErrorCode::ExpiredNonce,
        ErrorCode::ReplayedNonce,
        ErrorCode::TimedOut,
        ErrorCode::RoleEscalation,
    ];
    values.into_iter().find(|code| *code as u8 == value)
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
    fn expired_owner_lease_cannot_admit_joiner_from_persisted_membership() {
        let (mut server, root) = server(Duration::from_millis(5));
        let owner = model::SessionId::new(20);
        let guest = model::SessionId::new(21);
        let town = TownKey::new([0x44; 16]);
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
        std::thread::sleep(Duration::from_millis(20));

        // Reconnecting only the guest expires the owner lease and leaves the
        // persisted owner membership in the hosted model.
        server.reconnect(guest, model::ActorId::new(2)).unwrap();
        assert_eq!(server.session_role(owner), Err(ErrorCode::Expired));
        assert_eq!(
            server.execute(
                guest,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town },
            ),
            Err(ErrorCode::NotFound)
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
    fn in_memory_multi_client_vertical_covers_authority_conflict_replay_and_recovery() {
        let (mut server, root) = server(Duration::from_secs(60));
        let owner = model::SessionId::new(10);
        let visitor = model::SessionId::new(11);
        let town = TownKey::new([0x2a; 16]);
        server.connect(owner, model::ActorId::new(1)).unwrap();
        server.connect(visitor, model::ActorId::new(2)).unwrap();

        assert_eq!(
            server.execute(
                owner,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            ),
            Ok(Response::Accepted { revision: 0 })
        );
        assert_eq!(
            server.discover(visitor),
            Ok(Response::Discovered(vec![town]))
        );
        assert_eq!(
            server.execute(
                visitor,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town },
            ),
            Ok(Response::Accepted { revision: 1 })
        );
        assert_eq!(server.session_role(visitor), Ok(ServerRole::Visitor));

        let presence = Command::Apply {
            town_id: town,
            command: model::Command::SetPresence {
                x: 7,
                y: -3,
                facing: 4,
            },
        };
        assert_eq!(
            server.execute(visitor, model::RequestId::new(3), 1, presence),
            Ok(Response::Accepted { revision: 2 })
        );
        assert_eq!(
            server.execute(visitor, model::RequestId::new(3), 1, presence),
            Ok(Response::Accepted { revision: 2 })
        );
        assert_eq!(
            server.execute(
                visitor,
                model::RequestId::new(3),
                1,
                Command::Apply {
                    town_id: town,
                    command: model::Command::SetPresence {
                        x: 8,
                        y: -3,
                        facing: 4,
                    },
                },
            ),
            Err(ErrorCode::Conflict)
        );
        assert_eq!(
            server.execute(
                visitor,
                model::RequestId::new(4),
                1,
                Command::Apply {
                    town_id: town,
                    command: model::Command::SetPresence {
                        x: 8,
                        y: -3,
                        facing: 4,
                    },
                },
            ),
            Err(ErrorCode::StaleRevision)
        );
        assert_eq!(
            server.execute(
                visitor,
                model::RequestId::new(5),
                2,
                Command::Apply {
                    town_id: town,
                    command: model::Command::CloseTown,
                },
            ),
            Err(ErrorCode::Unauthorized)
        );
        assert_eq!(
            server.execute(
                visitor,
                model::RequestId::new(6),
                2,
                Command::Leave { town_id: town },
            ),
            Ok(Response::Accepted { revision: 3 })
        );
        assert_eq!(server.session_role(visitor), Ok(ServerRole::Resident));

        drop(server);
        let mut recovered =
            TownServer::open(Store::open(&root).unwrap(), Duration::from_secs(60)).unwrap();
        assert_eq!(
            recovered.recovery_paths().get(&town),
            Some(&RecoveryPath::Snapshot)
        );
        assert_eq!(recovered.state_bytes(town).unwrap(), {
            let mut expected = model::Town::new(
                model::TownId::new(town.bytes()),
                model::ActorId::new(1),
                owner,
            );
            expected
                .admit_visitor(owner, visitor, model::ActorId::new(2))
                .unwrap();
            expected
                .apply(
                    visitor,
                    1,
                    model::RequestId::new(3),
                    model::Command::SetPresence {
                        x: 7,
                        y: -3,
                        facing: 4,
                    },
                )
                .unwrap();
            expected.leave(visitor).unwrap();
            encode_model_snapshot(&expected.snapshot())
        });
        recovered.connect(visitor, model::ActorId::new(2)).unwrap();
        assert_eq!(
            recovered.execute(
                visitor,
                model::RequestId::new(7),
                3,
                Command::Join { town_id: town },
            ),
            Ok(Response::Accepted { revision: 4 })
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn accepted_response_payload_round_trips_without_duplicate_revision() {
        let (server, root) = server(Duration::from_secs(60));
        let revision = 0x0102_0304_0506_0708;
        let request = Frame {
            version: PROTOCOL_VERSION,
            message_type: MessageType::Command,
            flags: 0,
            request_id: 9,
            sequence: 4,
            town_revision: 0,
            payload: Vec::new(),
        };
        let encoded = server
            .encode_response(&request, Response::Accepted { revision })
            .unwrap();
        let frame = decode_frame(&encoded).unwrap();
        assert_eq!(frame.message_type, MessageType::Snapshot);
        assert_eq!(frame.town_revision, revision);
        assert_eq!(frame.payload.len(), 9);
        assert_eq!(frame.payload, vec![0u8; 9]);
        assert!(matches!(
            SocialClient::decode_response(frame),
            Ok(Response::Accepted { revision: decoded }) if decoded == revision
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reconnect_refreshes_lease_and_expiry_revokes_visitor() {
        let (mut server, root) = server(Duration::from_secs(1));
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
        std::thread::sleep(Duration::from_millis(1100));
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

    fn command_frame(request_id: u64, revision: u64, payload: Vec<u8>) -> Frame {
        Frame {
            version: PROTOCOL_VERSION,
            message_type: MessageType::Command,
            flags: 0,
            request_id,
            sequence: request_id,
            town_revision: revision,
            payload,
        }
    }

    fn set_presence_payload(town: TownKey) -> Vec<u8> {
        model_command_payload(
            town,
            model::Command::SetPresence {
                x: 4,
                y: -2,
                facing: 3,
            },
        )
    }

    fn close_town_payload(town: TownKey) -> Vec<u8> {
        model_command_payload(town, model::Command::CloseTown)
    }

    fn response_error(bytes: Vec<u8>) -> ErrorCode {
        let frame = decode_frame(&bytes).unwrap();
        assert_eq!(frame.message_type, MessageType::Error);
        assert_eq!(frame.payload.first(), Some(&1));
        error_code_from_byte(frame.payload[1]).unwrap()
    }

    #[test]
    fn command_authorization_and_parser_share_the_canonical_byte_range() {
        let (mut server, root) = server(Duration::from_secs(60));
        let owner = model::SessionId::new(10);
        let visitor = model::SessionId::new(11);
        let town = TownKey::new([0x31; 16]);
        let other_town = TownKey::new([0x42; 16]);
        server.connect(owner, model::ActorId::new(1)).unwrap();
        server
            .execute(
                owner,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            )
            .unwrap();
        server.connect(visitor, model::ActorId::new(2)).unwrap();
        assert_eq!(
            server.execute(
                visitor,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town },
            ),
            Ok(Response::Accepted { revision: 1 })
        );

        let valid = command_frame(3, 1, set_presence_payload(town));
        let parsed = parse_model_command(&valid.payload).unwrap();
        assert!(matches!(
            parsed,
            Command::Apply { town_id, .. } if town_id == town
        ));
        assert_eq!(authorized_town(&valid, town), Ok(true));
        assert_eq!(authorized_town(&valid, other_town), Ok(false));
        let accepted = server
            .handle_frame(visitor, &valid.encode().unwrap())
            .unwrap();
        let accepted = decode_frame(&accepted).unwrap();
        assert_eq!(accepted.message_type, MessageType::Snapshot);
        assert_eq!(accepted.town_revision, 2);

        let close = command_frame(4, 2, close_town_payload(town));
        assert_eq!(authorized_town(&close, town), Ok(true));
        assert_eq!(
            response_error(
                server
                    .handle_frame(visitor, &close.encode().unwrap())
                    .unwrap()
            ),
            ErrorCode::Unauthorized
        );

        let wrong_town = command_frame(5, 2, set_presence_payload(other_town));
        let parsed = parse_model_command(&wrong_town.payload).unwrap();
        assert!(matches!(
            parsed,
            Command::Apply { town_id, .. } if town_id == other_town
        ));
        assert_eq!(authorized_town(&wrong_town, town), Ok(false));
        assert_eq!(
            response_error(
                server
                    .handle_frame(visitor, &wrong_town.encode().unwrap())
                    .unwrap()
            ),
            ErrorCode::Unauthorized
        );

        let mut truncated = vec![1];
        truncated.extend_from_slice(&town.bytes()[..15]);
        assert_eq!(
            authorized_town(&command_frame(6, 2, truncated.clone()), town),
            Err(ErrorCode::Malformed)
        );
        assert_eq!(
            response_error(
                server
                    .handle_frame(visitor, &command_frame(6, 2, truncated).encode().unwrap())
                    .unwrap()
            ),
            ErrorCode::Malformed
        );

        let mut overlong = set_presence_payload(town);
        overlong.insert(17, 0xaa);
        assert_eq!(
            authorized_town(&command_frame(7, 2, overlong.clone()), town),
            Err(ErrorCode::Malformed)
        );
        assert_eq!(
            response_error(
                server
                    .handle_frame(visitor, &command_frame(7, 2, overlong).encode().unwrap())
                    .unwrap()
            ),
            ErrorCode::Malformed
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_error_classification_distinguishes_timeout_and_protocol_errors() {
        assert_eq!(
            classify_read_error(&NetError::Io(io::Error::from(io::ErrorKind::TimedOut,))),
            ErrorCode::TimedOut
        );
        assert_eq!(
            classify_read_error(&NetError::Io(io::Error::from(io::ErrorKind::WouldBlock,))),
            ErrorCode::TimedOut
        );
        assert_eq!(
            classify_read_error(&NetError::Frame(DecodeError::BadMagic { actual: [0; 4] })),
            ErrorCode::Protocol
        );
        assert_eq!(
            classify_read_error(&NetError::Protocol("malformed frame")),
            ErrorCode::Protocol
        );
    }

    fn running_loopback(config: LoopbackConfig) -> Option<(RunningServer, std::path::PathBuf)> {
        let (town_server, root) = server(Duration::from_secs(60));
        let loopback = match LoopbackServer::bind(town_server, b"lane secret", config) {
            Ok(loopback) => loopback,
            Err(NetError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied => {
                eprintln!("SKIP loopback socket test: localhost bind permission denied: {error}");
                let _ = fs::remove_dir_all(&root);
                return None;
            }
            Err(error) => panic!("test loopback configuration: {error:?}"),
        };
        let running = loopback.spawn();
        std::thread::sleep(Duration::from_millis(10));
        Some((running, root))
    }

    fn auth_frame(
        nonce: [u8; AUTH_NONCE_BYTES],
        secret: &[u8],
        session: u64,
        actor: u64,
        town: TownKey,
        include_token: bool,
    ) -> Frame {
        let session = model::SessionId::new(session);
        let actor = model::ActorId::new(actor);
        let mut payload = nonce.to_vec();
        payload.extend_from_slice(&session.get().to_be_bytes());
        payload.extend_from_slice(&actor.get().to_be_bytes());
        payload.extend_from_slice(&town.bytes());
        if include_token {
            payload.extend_from_slice(&session_token(secret, nonce, session, actor, town));
        }
        Frame {
            version: PROTOCOL_VERSION,
            message_type: MessageType::AuthResponse,
            flags: 0,
            request_id: 0,
            sequence: 1,
            town_revision: 0,
            payload,
        }
    }

    #[test]
    fn loopback_lifecycle_authz_idempotency_and_determinism() {
        fn script() -> Option<Vec<u8>> {
            let (running, root) = running_loopback(LoopbackConfig::default())?;
            let town = TownKey::new([0x31; 16]);
            let mut owner = SocialClient::connect(
                running.local_addr(),
                b"lane secret",
                model::SessionId::new(10),
                model::ActorId::new(1),
                TownKey::new([0; 16]),
            )
            .unwrap();
            assert!(matches!(owner.create_town(town), Ok(0)));
            let mut visitor = SocialClient::connect(
                running.local_addr(),
                b"lane secret",
                model::SessionId::new(11),
                model::ActorId::new(2),
                town,
            )
            .unwrap();
            assert!(matches!(visitor.join(town), Ok(1)));
            assert!(matches!(
                visitor.apply(town, 1, model::Command::CloseTown,),
                Err(NetError::Refused(ErrorCode::Unauthorized))
            ));
            assert!(matches!(
                visitor.apply(
                    town,
                    1,
                    model::Command::SetPresence {
                        x: 4,
                        y: -2,
                        facing: 3,
                    },
                ),
                Ok(2)
            ));
            let (revision, state) = visitor.sync(town).unwrap();
            assert_eq!(revision, 2);
            drop(owner);
            drop(visitor);
            drop(running);
            let _ = fs::remove_dir_all(root);
            Some(state)
        }

        let Some(first) = script() else { return };
        let Some(second) = script() else {
            panic!("loopback bind permission changed during deterministic rerun");
        };
        assert_eq!(first, second);
    }

    #[test]
    fn auth_refusals_are_distinct_and_nonce_is_single_use() {
        let Some((running, root)) = running_loopback(LoopbackConfig {
            io_timeout: Duration::from_millis(100),
            nonce_ttl: Duration::from_millis(30),
        }) else {
            return;
        };
        let addr = running.local_addr();
        assert!(matches!(
            SocialClient::connect(
                addr,
                b"wrong secret",
                model::SessionId::new(10),
                model::ActorId::new(1),
                TownKey::new([0; 16]),
            ),
            Err(NetError::Auth(AuthStatus::BadToken))
        ));
        assert!(matches!(
            SocialClient::connect_without_token(
                addr,
                model::SessionId::new(10),
                model::ActorId::new(1),
                TownKey::new([0; 16]),
            ),
            Err(NetError::Auth(AuthStatus::NoToken))
        ));

        let mut first = TcpStream::connect(addr).unwrap();
        first
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        first
            .set_write_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut decoder = nookery_social_proto::FrameDecoder::new();
        let challenge = read_wire_frame(&mut first, &mut decoder).unwrap();
        let nonce: [u8; AUTH_NONCE_BYTES] = challenge.payload.try_into().unwrap();
        write_wire_frame(
            &mut first,
            &auth_frame(nonce, b"lane secret", 10, 1, TownKey::new([0; 16]), true),
        )
        .unwrap();
        assert_eq!(
            read_wire_frame(&mut first, &mut decoder).unwrap().payload,
            vec![AuthStatus::Authenticated as u8]
        );
        drop(first);

        let mut replay = TcpStream::connect(addr).unwrap();
        replay
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        replay
            .set_write_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut replay_decoder = nookery_social_proto::FrameDecoder::new();
        let _new_challenge = read_wire_frame(&mut replay, &mut replay_decoder).unwrap();
        write_wire_frame(
            &mut replay,
            &auth_frame(nonce, b"lane secret", 10, 1, TownKey::new([0; 16]), true),
        )
        .unwrap();
        assert_eq!(
            read_wire_frame(&mut replay, &mut replay_decoder)
                .unwrap()
                .payload,
            vec![AuthStatus::ReplayedNonce as u8]
        );

        let mut expired = TcpStream::connect(addr).unwrap();
        expired
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        expired
            .set_write_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let mut expired_decoder = nookery_social_proto::FrameDecoder::new();
        let challenge = read_wire_frame(&mut expired, &mut expired_decoder).unwrap();
        let nonce: [u8; AUTH_NONCE_BYTES] = challenge.payload.try_into().unwrap();
        std::thread::sleep(Duration::from_millis(45));
        write_wire_frame(
            &mut expired,
            &auth_frame(nonce, b"lane secret", 12, 3, TownKey::new([0; 16]), true),
        )
        .unwrap();
        assert_eq!(
            read_wire_frame(&mut expired, &mut expired_decoder)
                .unwrap()
                .payload,
            vec![AuthStatus::ExpiredNonce as u8]
        );
        drop(replay);
        drop(expired);
        drop(running);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn auth_nonce_policy_is_testable_without_socket_access() {
        let root = std::env::temp_dir().join(format!("nookery-auth-policy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let context = ConnectionContext {
            state: Arc::new(Mutex::new(
                TownServer::open(Store::open(&root).unwrap(), Duration::from_secs(1)).unwrap(),
            )),
            secret: Arc::new(b"lane secret".to_vec()),
            config: LoopbackConfig {
                io_timeout: Duration::from_secs(1),
                nonce_ttl: Duration::from_millis(10),
            },
            nonces: Arc::new(Mutex::new(NonceBook::new())),
            nonce_counter: Arc::new(AtomicU64::new(1)),
            port: 0,
        };
        let nonce = [0xabu8; AUTH_NONCE_BYTES];
        let frame = auth_frame(nonce, b"lane secret", 30, 3, TownKey::new([0; 16]), true);
        let expired =
            context.authenticate(nonce, Instant::now() - Duration::from_millis(20), &frame);
        assert_eq!(expired.3, AuthStatus::ExpiredNonce);
        let accepted = context.authenticate(nonce, Instant::now(), &frame);
        assert_eq!(accepted.3, AuthStatus::Authenticated);
        let replayed = context.authenticate(nonce, Instant::now(), &frame);
        assert_eq!(replayed.3, AuthStatus::ReplayedNonce);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hostile_frames_and_stalls_receive_typed_refusal_and_close() {
        let Some((running, root)) = running_loopback(LoopbackConfig {
            io_timeout: Duration::from_millis(60),
            nonce_ttl: Duration::from_millis(200),
        }) else {
            return;
        };
        let mut stream = TcpStream::connect(running.local_addr()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut decoder = nookery_social_proto::FrameDecoder::new();
        let challenge = read_wire_frame(&mut stream, &mut decoder).unwrap();
        let nonce: [u8; AUTH_NONCE_BYTES] = challenge.payload.try_into().unwrap();
        write_wire_frame(
            &mut stream,
            &auth_frame(nonce, b"lane secret", 10, 1, TownKey::new([0; 16]), true),
        )
        .unwrap();
        assert_eq!(
            read_wire_frame(&mut stream, &mut decoder)
                .unwrap()
                .message_type,
            MessageType::AuthResult
        );
        let mut oversized = vec![0u8; nookery_social_proto::HEADER_SIZE];
        oversized[..4].copy_from_slice(&nookery_social_proto::MAGIC);
        oversized[4..6].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        oversized[6] = MessageType::Command as u8;
        oversized[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        stream.write_all(&oversized).unwrap();
        let refusal = read_wire_frame(&mut stream, &mut decoder).unwrap();
        assert_eq!(refusal.message_type, MessageType::Error);
        assert_eq!(refusal.payload, vec![1, ErrorCode::Protocol as u8]);
        let mut tail = [0u8; 1];
        assert_eq!(stream.read(&mut tail).unwrap(), 0);

        let mut idle = TcpStream::connect(running.local_addr()).unwrap();
        idle.set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let mut idle_decoder = nookery_social_proto::FrameDecoder::new();
        assert_eq!(
            read_wire_frame(&mut idle, &mut idle_decoder)
                .unwrap()
                .message_type,
            MessageType::AuthChallenge
        );
        let started = Instant::now();
        let refusal = read_wire_frame(&mut idle, &mut idle_decoder).unwrap();
        assert!(started.elapsed() < Duration::from_millis(180));
        assert_eq!(refusal.message_type, MessageType::Error);
        assert_eq!(refusal.request_id, 0);
        assert_eq!(refusal.sequence, 1);
        assert_eq!(refusal.payload, vec![1, ErrorCode::TimedOut as u8]);
        let mut tail = [0u8; 1];
        assert_eq!(idle.read(&mut tail).unwrap(), 0);
        drop(stream);
        drop(idle);
        drop(running);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn torn_snapshot_restarts_from_command_log() {
        let (mut town_server, root) = server(Duration::from_secs(60));
        let session = model::SessionId::new(10);
        let visitor = model::SessionId::new(11);
        let town = TownKey::new([0x44; 16]);
        town_server
            .connect(session, model::ActorId::new(1))
            .unwrap();
        town_server
            .execute(
                session,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            )
            .unwrap();
        town_server
            .connect(visitor, model::ActorId::new(2))
            .unwrap();
        town_server
            .execute(
                visitor,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town },
            )
            .unwrap();
        town_server
            .execute(
                visitor,
                model::RequestId::new(3),
                1,
                Command::Apply {
                    town_id: town,
                    command: model::Command::SetPresence {
                        x: 3,
                        y: 4,
                        facing: 2,
                    },
                },
            )
            .unwrap();
        let expected = town_server.state_bytes(town).unwrap();
        let snapshot_path = root.join("44444444444444444444444444444444/snapshot.bin");
        fs::write(snapshot_path, [0u8; 1]).unwrap();
        drop(town_server);
        let recovered =
            TownServer::open(Store::open(&root).unwrap(), Duration::from_secs(60)).unwrap();
        assert_eq!(
            recovered.recovery_paths().get(&town),
            Some(&RecoveryPath::CommandLog)
        );
        let mut recovered = recovered;
        assert_eq!(recovered.state_bytes(town).unwrap(), expected);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expired_visitor_removal_is_replayed_after_snapshot_torn() {
        let lease = Duration::from_secs(1);
        let (mut town_server, root) = server(lease);
        let owner = model::SessionId::new(10);
        let visitor = model::SessionId::new(11);
        let town = TownKey::new([0x55; 16]);
        town_server.connect(owner, model::ActorId::new(1)).unwrap();
        town_server
            .execute(
                owner,
                model::RequestId::new(1),
                0,
                Command::CreateTown { town_id: town },
            )
            .unwrap();
        town_server
            .connect(visitor, model::ActorId::new(2))
            .unwrap();
        town_server
            .execute(
                visitor,
                model::RequestId::new(2),
                0,
                Command::Join { town_id: town },
            )
            .unwrap();

        std::thread::sleep(lease + Duration::from_millis(100));
        town_server
            .reconnect(owner, model::ActorId::new(1))
            .unwrap();
        let expected = {
            let mut expected = model::Town::new(
                model::TownId::new(town.bytes()),
                model::ActorId::new(1),
                owner,
            );
            expected
                .admit_visitor(owner, visitor, model::ActorId::new(2))
                .unwrap();
            expected.leave(visitor).unwrap();
            encode_model_snapshot(&expected.snapshot())
        };
        assert_eq!(town_server.state_bytes(town).unwrap(), expected);

        let snapshot_path = root.join("55555555555555555555555555555555/snapshot.bin");
        fs::write(snapshot_path, [0u8; 1]).unwrap();
        drop(town_server);
        let mut recovered =
            TownServer::open(Store::open(&root).unwrap(), Duration::from_secs(60)).unwrap();
        assert_eq!(
            recovered.recovery_paths().get(&town),
            Some(&RecoveryPath::CommandLog)
        );
        assert_eq!(recovered.state_bytes(town).unwrap(), expected);
        let _ = fs::remove_dir_all(root);
    }
}
