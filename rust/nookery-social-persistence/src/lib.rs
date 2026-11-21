//! Crash-safe, bounded persistence for opaque hosted-town state.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"NTRP";
const VERSION: u8 = 1;
const SNAPSHOT_KIND: u8 = 1;
const COMMAND_KIND: u8 = 2;
const HEADER_LEN: usize = 4 + 1 + 1 + 8 + 4 + 4;
const TRAILER_LEN: usize = 4;
pub const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;
pub const MAX_COMMANDS_PER_LOG: usize = 100_000;
pub const MAX_LOG_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub revision: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecord {
    pub revision: u64,
    pub idempotency_key: [u8; 16],
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceError {
    Io(String),
    InvalidFormat(&'static str),
    Corrupt(&'static str),
    TooLarge(&'static str),
    NonMonotonicRevision,
    TooManyCommands,
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::InvalidFormat(message) => write!(f, "invalid format: {message}"),
            Self::Corrupt(message) => write!(f, "corrupt persistence: {message}"),
            Self::TooLarge(kind) => write!(f, "{kind} exceeds configured bound"),
            Self::NonMonotonicRevision => write!(f, "revision is not monotonic"),
            Self::TooManyCommands => write!(f, "command log exceeds configured bound"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<io::Error> for PersistenceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        fs::create_dir_all(root.as_ref())?;
        Ok(Self {
            root: root.as_ref().to_path_buf(),
        })
    }

    pub fn for_town(&self, town_id: [u8; 16]) -> Result<Self, PersistenceError> {
        let mut name = String::with_capacity(32);
        for byte in town_id {
            use std::fmt::Write as _;
            let _ = write!(name, "{byte:02x}");
        }
        Self::open(self.root.join(name))
    }

    /// Lists town directories containing only the bounded sixteen-byte hex
    /// identifiers accepted by [`Self::for_town`]. Non-town entries are
    /// ignored so unrelated files cannot become hosted state.
    pub fn town_ids(&self) -> Result<Vec<[u8; 16]>, PersistenceError> {
        let mut towns = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.len() != 32 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                continue;
            }
            let mut id = [0u8; 16];
            let bytes = name.as_bytes();
            let mut valid = true;
            for index in 0..16 {
                let Some(high) = hex_value(bytes[index * 2]) else {
                    valid = false;
                    break;
                };
                let Some(low) = hex_value(bytes[index * 2 + 1]) else {
                    valid = false;
                    break;
                };
                id[index] = (high << 4) | low;
            }
            if valid {
                towns.push(id);
            }
        }
        towns.sort_unstable();
        Ok(towns)
    }

    pub fn write_snapshot(&self, snapshot: &Snapshot) -> Result<(), PersistenceError> {
        validate_snapshot(snapshot)?;
        let encoded = encode_snapshot(snapshot);
        atomic_replace(&self.snapshot_path(), &encoded)
    }

    pub fn read_snapshot(&self) -> Result<Option<Snapshot>, PersistenceError> {
        read_optional(
            &self.snapshot_path(),
            MAX_SNAPSHOT_BYTES + HEADER_LEN + TRAILER_LEN,
        )?
        .map(|bytes| decode_snapshot(&bytes))
        .transpose()
    }

    pub fn append_command(&self, record: &CommandRecord) -> Result<(), PersistenceError> {
        validate_command(record)?;
        let current = self.read_commands()?;
        if current.len() >= MAX_COMMANDS_PER_LOG {
            return Err(PersistenceError::TooManyCommands);
        }
        if let Some(previous) = current.last() {
            if record.revision <= previous.revision {
                return Err(PersistenceError::NonMonotonicRevision);
            }
        }
        let encoded = encode_command(record);
        let current_len = fs::metadata(self.log_path())
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if current_len > (MAX_LOG_BYTES.saturating_sub(encoded.len())) as u64 {
            return Err(PersistenceError::TooLarge("command log"));
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path())?;
        file.write_all(&encoded)?;
        file.sync_data()?;
        Ok(())
    }

    pub fn read_commands(&self) -> Result<Vec<CommandRecord>, PersistenceError> {
        let Some(bytes) = read_optional(&self.log_path(), MAX_LOG_BYTES)? else {
            return Ok(Vec::new());
        };
        decode_commands(&bytes, false)
    }

    /// Returns the last valid checkpoint and command prefix after a torn final append.
    /// Checksummed corruption in a complete record is still refused.
    pub fn recover(&self) -> Result<(Option<Snapshot>, Vec<CommandRecord>), PersistenceError> {
        let snapshot = match self.read_snapshot() {
            Ok(snapshot) => snapshot,
            // A torn snapshot has no trustworthy bytes, but the command log
            // can still provide a complete recovery path. Checksummed
            // corruption remains a hard refusal.
            Err(PersistenceError::Corrupt("truncated header"))
            | Err(PersistenceError::Corrupt("truncated record")) => None,
            Err(error) => return Err(error),
        };
        let commands = match read_optional(&self.log_path(), MAX_LOG_BYTES)? {
            Some(bytes) => decode_commands(&bytes, true)?,
            None => Vec::new(),
        };
        Ok((snapshot, commands))
    }

    fn snapshot_path(&self) -> PathBuf {
        self.root.join("snapshot.bin")
    }

    fn log_path(&self) -> PathBuf {
        self.root.join("commands.log")
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), PersistenceError> {
    if snapshot.bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(PersistenceError::TooLarge("snapshot"));
    }
    Ok(())
}

fn validate_command(record: &CommandRecord) -> Result<(), PersistenceError> {
    if record.bytes.len() > MAX_COMMAND_BYTES {
        return Err(PersistenceError::TooLarge("command"));
    }
    Ok(())
}

fn encode_snapshot(snapshot: &Snapshot) -> Vec<u8> {
    encode_record(SNAPSHOT_KIND, snapshot.revision, &[], &snapshot.bytes)
}

fn encode_command(record: &CommandRecord) -> Vec<u8> {
    encode_record(
        COMMAND_KIND,
        record.revision,
        &record.idempotency_key,
        &record.bytes,
    )
}

fn encode_record(kind: u8, revision: u64, key: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + key.len() + payload.len() + TRAILER_LEN);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(kind);
    out.extend_from_slice(&revision.to_le_bytes());
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(payload);
    out.extend_from_slice(&checksum(&out).to_le_bytes());
    out
}

fn decode_snapshot(bytes: &[u8]) -> Result<Snapshot, PersistenceError> {
    let record = decode_record(bytes, SNAPSHOT_KIND)?;
    if !record.key.is_empty() {
        return Err(PersistenceError::Corrupt("snapshot key"));
    }
    let snapshot = Snapshot {
        revision: record.revision,
        bytes: record.payload,
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn decode_commands(
    bytes: &[u8],
    recover_torn_tail: bool,
) -> Result<Vec<CommandRecord>, PersistenceError> {
    let mut records = Vec::new();
    let mut offset = 0usize;
    let mut previous_revision = None;
    while offset < bytes.len() {
        if records.len() >= MAX_COMMANDS_PER_LOG {
            return Err(PersistenceError::TooManyCommands);
        }
        if bytes.len() - offset < HEADER_LEN + TRAILER_LEN {
            if recover_torn_tail && offset != 0 {
                break;
            }
            return Err(PersistenceError::Corrupt("truncated record"));
        }
        let key_len =
            u32::from_le_bytes(bytes[offset + 14..offset + 18].try_into().unwrap()) as usize;
        let payload_len =
            u32::from_le_bytes(bytes[offset + 18..offset + 22].try_into().unwrap()) as usize;
        let total = HEADER_LEN
            .checked_add(key_len)
            .and_then(|n| n.checked_add(payload_len))
            .and_then(|n| n.checked_add(TRAILER_LEN))
            .ok_or(PersistenceError::Corrupt("record length overflow"))?;
        if total > bytes.len() - offset {
            if recover_torn_tail {
                break;
            }
            return Err(PersistenceError::Corrupt("truncated record"));
        }
        let record = decode_record(&bytes[offset..offset + total], COMMAND_KIND)?;
        if record.key.len() != 16 {
            return Err(PersistenceError::Corrupt("idempotency key length"));
        }
        if previous_revision.is_some_and(|revision| record.revision <= revision) {
            return Err(PersistenceError::NonMonotonicRevision);
        }
        previous_revision = Some(record.revision);
        records.push(CommandRecord {
            revision: record.revision,
            idempotency_key: record
                .key
                .try_into()
                .map_err(|_| PersistenceError::Corrupt("idempotency key"))?,
            bytes: record.payload,
        });
        offset += total;
    }
    Ok(records)
}

struct DecodedRecord {
    revision: u64,
    key: Vec<u8>,
    payload: Vec<u8>,
}

fn decode_record(bytes: &[u8], expected_kind: u8) -> Result<DecodedRecord, PersistenceError> {
    if bytes.len() < HEADER_LEN + TRAILER_LEN {
        return Err(PersistenceError::Corrupt("truncated header"));
    }
    if &bytes[..4] != MAGIC {
        return Err(PersistenceError::InvalidFormat("magic"));
    }
    if bytes[4] != VERSION {
        return Err(PersistenceError::InvalidFormat("version"));
    }
    if bytes[5] != expected_kind {
        return Err(PersistenceError::InvalidFormat("record kind"));
    }
    let revision = u64::from_le_bytes(
        bytes[6..14]
            .try_into()
            .map_err(|_| PersistenceError::Corrupt("revision"))?,
    );
    let key_len = u32::from_le_bytes(
        bytes[14..18]
            .try_into()
            .map_err(|_| PersistenceError::Corrupt("key length"))?,
    ) as usize;
    let payload_len = u32::from_le_bytes(
        bytes[18..22]
            .try_into()
            .map_err(|_| PersistenceError::Corrupt("payload length"))?,
    ) as usize;
    let total = HEADER_LEN
        .checked_add(key_len)
        .and_then(|n| n.checked_add(payload_len))
        .and_then(|n| n.checked_add(TRAILER_LEN))
        .ok_or(PersistenceError::Corrupt("record length overflow"))?;
    if total != bytes.len() {
        return Err(PersistenceError::Corrupt("record length"));
    }
    if payload_len
        > if expected_kind == SNAPSHOT_KIND {
            MAX_SNAPSHOT_BYTES
        } else {
            MAX_COMMAND_BYTES
        }
    {
        return Err(PersistenceError::TooLarge(
            if expected_kind == SNAPSHOT_KIND {
                "snapshot"
            } else {
                "command"
            },
        ));
    }
    let expected_checksum = u32::from_le_bytes(
        bytes[total - TRAILER_LEN..]
            .try_into()
            .map_err(|_| PersistenceError::Corrupt("checksum"))?,
    );
    if checksum(&bytes[..total - TRAILER_LEN]) != expected_checksum {
        return Err(PersistenceError::Corrupt("checksum"));
    }
    let key_start = HEADER_LEN;
    let payload_start = key_start + key_len;
    Ok(DecodedRecord {
        revision,
        key: bytes[key_start..payload_start].to_vec(),
        payload: bytes[payload_start..payload_start + payload_len].to_vec(),
    })
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), PersistenceError> {
    let temp = path.with_extension("tmp");
    let mut file = File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    if let Ok(directory) = File::open(
        path.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?,
    ) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn read_optional(path: &Path, max_bytes: usize) -> Result<Option<Vec<u8>>, PersistenceError> {
    match File::open(path) {
        Ok(mut file) => {
            let metadata = file.metadata()?;
            if metadata.len() > max_bytes as u64 {
                return Err(PersistenceError::TooLarge("persistence file"));
            }
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            file.read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_corrupt_snapshot_checksum() {
        let root = std::env::temp_dir().join(format!("nookery-persistence-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = Store::open(&root).unwrap();
        store
            .write_snapshot(&Snapshot {
                revision: 1,
                bytes: b"opaque".to_vec(),
            })
            .unwrap();
        let path = root.join("snapshot.bin");
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            store.read_snapshot(),
            Err(PersistenceError::Corrupt("checksum"))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recovers_snapshot_and_valid_prefix_after_torn_append() {
        let root = std::env::temp_dir().join(format!("nookery-recovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = Store::open(&root).unwrap();
        store
            .write_snapshot(&Snapshot {
                revision: 5,
                bytes: b"checkpoint".to_vec(),
            })
            .unwrap();
        store
            .append_command(&CommandRecord {
                revision: 6,
                idempotency_key: [6; 16],
                bytes: b"one".to_vec(),
            })
            .unwrap();
        store
            .append_command(&CommandRecord {
                revision: 7,
                idempotency_key: [7; 16],
                bytes: b"two".to_vec(),
            })
            .unwrap();
        let path = root.join("commands.log");
        let mut bytes = fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 2);
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            store.read_commands(),
            Err(PersistenceError::Corrupt("truncated record"))
        ));
        let (snapshot, commands) = store.recover().unwrap();
        assert_eq!(snapshot.unwrap().revision, 5);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].revision, 6);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_corrupt_complete_command_record() {
        let root = std::env::temp_dir().join(format!("nookery-log-corrupt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = Store::open(&root).unwrap();
        store
            .append_command(&CommandRecord {
                revision: 1,
                idempotency_key: [1; 16],
                bytes: b"opaque".to_vec(),
            })
            .unwrap();
        let path = root.join("commands.log");
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            store.recover(),
            Err(PersistenceError::Corrupt("checksum"))
        ));
        let _ = fs::remove_dir_all(root);
    }
}
