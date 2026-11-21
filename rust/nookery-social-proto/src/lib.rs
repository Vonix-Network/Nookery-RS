//! A bounded, independently named Nookery social protocol frame codec.
//!
//! This crate defines only an opaque-byte frame envelope and an incremental
//! stream decoder. It does not provide sockets, TLS, cryptography, accounts,
//! authentication, authorization, town state, persistence, game integration,
//! deployment, or compatibility with any other service or protocol. Payloads
//! are opaque bytes and are never interpreted as game or save data.

use core::fmt;

/// The only protocol version supported by this crate.
pub const PROTOCOL_VERSION: u16 = 1;
/// The fixed four-byte frame magic.
pub const MAGIC: [u8; 4] = *b"NKS1";
/// The encoded header size, in bytes.
pub const HEADER_SIZE: usize = 36;
/// The maximum complete frame size, including [`HEADER_SIZE`].
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// The maximum payload size implied by [`MAX_FRAME_BYTES`].
pub const MAX_PAYLOAD_BYTES: usize = MAX_FRAME_BYTES - HEADER_SIZE;
/// The maximum number of incomplete stream bytes retained by [`FrameDecoder`].
pub const MAX_BUFFERED_BYTES: usize = MAX_FRAME_BYTES;

/// Stable message kinds in the protocol envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageType {
    Hello = 1,
    JoinTown = 2,
    LeaveTown = 3,
    Command = 4,
    Snapshot = 5,
    Error = 6,
    /// Server-to-client challenge containing a connection nonce.
    AuthChallenge = 7,
    /// Client-to-server proof of possession of the configured secret.
    AuthResponse = 8,
    /// Server-to-client authentication result.
    AuthResult = 9,
}

impl MessageType {
    fn from_byte(value: u8) -> Result<Self, DecodeError> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::JoinTown),
            3 => Ok(Self::LeaveTown),
            4 => Ok(Self::Command),
            5 => Ok(Self::Snapshot),
            6 => Ok(Self::Error),
            7 => Ok(Self::AuthChallenge),
            8 => Ok(Self::AuthResponse),
            9 => Ok(Self::AuthResult),
            _ => Err(DecodeError::UnknownMessageType(value)),
        }
    }
}

/// A complete protocol frame with opaque payload bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub version: u16,
    pub message_type: MessageType,
    pub flags: u8,
    pub request_id: u64,
    pub sequence: u64,
    pub town_revision: u64,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Encodes this frame into its canonical big-endian wire representation.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let payload_len = self.payload.len();
        let frame_len = HEADER_SIZE
            .checked_add(payload_len)
            .ok_or(EncodeError::FrameTooLarge)?;
        if frame_len > MAX_FRAME_BYTES {
            return Err(EncodeError::FrameTooLarge);
        }

        let payload_len = u32::try_from(payload_len).map_err(|_| EncodeError::PayloadTooLarge)?;
        let mut encoded = Vec::with_capacity(frame_len);
        encoded.extend_from_slice(&MAGIC);
        encoded.extend_from_slice(&self.version.to_be_bytes());
        encoded.push(self.message_type as u8);
        encoded.push(self.flags);
        encoded.extend_from_slice(&payload_len.to_be_bytes());
        encoded.extend_from_slice(&self.request_id.to_be_bytes());
        encoded.extend_from_slice(&self.sequence.to_be_bytes());
        encoded.extend_from_slice(&self.town_revision.to_be_bytes());
        encoded.extend_from_slice(&self.payload);
        Ok(encoded)
    }
}

/// Errors produced while encoding a frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    FrameTooLarge,
    PayloadTooLarge,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameTooLarge => formatter.write_str("frame exceeds maximum size"),
            Self::PayloadTooLarge => formatter.write_str("payload exceeds encodable length"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// Errors produced while decoding a complete frame or stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    ShortHeader { actual: usize },
    BadMagic { actual: [u8; 4] },
    UnsupportedVersion { actual: u16 },
    UnknownMessageType(u8),
    LengthOverflow,
    FrameTooLarge { frame_bytes: u64 },
    TruncatedPayload { expected: usize, actual: usize },
    TrailingBytes { count: usize },
    BufferLimitExceeded { attempted: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortHeader { actual } => write!(formatter, "short header: {actual} bytes"),
            Self::BadMagic { actual } => write!(formatter, "bad magic: {actual:?}"),
            Self::UnsupportedVersion { actual } => {
                write!(formatter, "unsupported protocol version: {actual}")
            }
            Self::UnknownMessageType(actual) => write!(formatter, "unknown message type: {actual}"),
            Self::LengthOverflow => formatter.write_str("frame length overflows"),
            Self::FrameTooLarge { frame_bytes } => {
                write!(formatter, "frame is too large: {frame_bytes} bytes")
            }
            Self::TruncatedPayload { expected, actual } => {
                write!(
                    formatter,
                    "truncated payload: expected {expected}, got {actual}"
                )
            }
            Self::TrailingBytes { count } => write!(formatter, "trailing bytes: {count}"),
            Self::BufferLimitExceeded { attempted } => {
                write!(formatter, "buffer limit exceeded: {attempted} bytes")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

fn payload_length(bytes: &[u8]) -> Result<usize, DecodeError> {
    let length = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let frame_bytes = length
        .checked_add(HEADER_SIZE)
        .ok_or(DecodeError::LengthOverflow)?;
    if frame_bytes > MAX_FRAME_BYTES {
        return Err(DecodeError::FrameTooLarge {
            frame_bytes: frame_bytes as u64,
        });
    }
    Ok(length)
}

fn validate_header(bytes: &[u8]) -> Result<usize, DecodeError> {
    let actual_magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if actual_magic != MAGIC {
        return Err(DecodeError::BadMagic {
            actual: actual_magic,
        });
    }
    let version = u16::from_be_bytes([bytes[4], bytes[5]]);
    if version != PROTOCOL_VERSION {
        return Err(DecodeError::UnsupportedVersion { actual: version });
    }
    MessageType::from_byte(bytes[6])?;
    payload_length(bytes)
}

/// Decodes exactly one complete frame.
pub fn decode_frame(bytes: &[u8]) -> Result<Frame, DecodeError> {
    if bytes.len() < HEADER_SIZE {
        return Err(DecodeError::ShortHeader {
            actual: bytes.len(),
        });
    }

    let payload_len = validate_header(bytes)?;
    let frame_len = HEADER_SIZE
        .checked_add(payload_len)
        .ok_or(DecodeError::LengthOverflow)?;
    if bytes.len() < frame_len {
        return Err(DecodeError::TruncatedPayload {
            expected: payload_len,
            actual: bytes.len() - HEADER_SIZE,
        });
    }
    if bytes.len() > frame_len {
        return Err(DecodeError::TrailingBytes {
            count: bytes.len() - frame_len,
        });
    }

    Ok(Frame {
        version: u16::from_be_bytes([bytes[4], bytes[5]]),
        message_type: MessageType::from_byte(bytes[6])?,
        flags: bytes[7],
        request_id: u64::from_be_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
        ]),
        sequence: u64::from_be_bytes([
            bytes[20], bytes[21], bytes[22], bytes[23], bytes[24], bytes[25], bytes[26], bytes[27],
        ]),
        town_revision: u64::from_be_bytes([
            bytes[28], bytes[29], bytes[30], bytes[31], bytes[32], bytes[33], bytes[34], bytes[35],
        ]),
        payload: bytes[HEADER_SIZE..].to_vec(),
    })
}

/// Incrementally decodes arbitrary chunking and coalescing of frame bytes.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffered: Vec<u8>,
}

impl FrameDecoder {
    /// Creates an empty decoder. Incomplete input is retained up to
    /// [`MAX_BUFFERED_BYTES`], which is one maximum frame.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of incomplete bytes currently retained.
    pub fn buffered_len(&self) -> usize {
        self.buffered.len()
    }

    /// Returns complete frames and retains any incomplete suffix.
    pub fn push(&mut self, mut input: &[u8]) -> Result<Vec<Frame>, DecodeError> {
        let mut frames = Vec::new();

        while !input.is_empty() {
            if self.buffered.is_empty() {
                if input.len() < HEADER_SIZE {
                    self.buffered.extend_from_slice(input);
                    break;
                }

                let payload_len = validate_header(input)?;
                let frame_len = HEADER_SIZE
                    .checked_add(payload_len)
                    .ok_or(DecodeError::LengthOverflow)?;
                if input.len() < frame_len {
                    self.buffered.extend_from_slice(input);
                    break;
                }

                frames.push(decode_frame(&input[..frame_len])?);
                input = &input[frame_len..];
                continue;
            }

            if self.buffered.len() < HEADER_SIZE {
                let needed = HEADER_SIZE - self.buffered.len();
                let take = needed.min(input.len());
                self.buffered.extend_from_slice(&input[..take]);
                input = &input[take..];
                if self.buffered.len() < HEADER_SIZE {
                    break;
                }
            }

            let payload_len = validate_header(&self.buffered)?;
            let frame_len = HEADER_SIZE
                .checked_add(payload_len)
                .ok_or(DecodeError::LengthOverflow)?;
            let needed = frame_len - self.buffered.len();
            let take = needed.min(input.len());
            self.buffered.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.buffered.len() < frame_len {
                break;
            }

            let complete = core::mem::take(&mut self.buffered);
            frames.push(decode_frame(&complete)?);
        }

        if self.buffered.len() > MAX_BUFFERED_BYTES {
            return Err(DecodeError::BufferLimitExceeded {
                attempted: self.buffered.len(),
            });
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(payload: Vec<u8>) -> Frame {
        Frame {
            version: PROTOCOL_VERSION,
            message_type: MessageType::Command,
            flags: 0xa5,
            request_id: 0x0102_0304_0506_0708,
            sequence: 0x1112_1314_1516_1718,
            town_revision: 0x2122_2324_2526_2728,
            payload,
        }
    }

    #[test]
    fn header_size_and_big_endian_layout_are_stable() {
        let mut expected = Vec::from(*b"NKS1");
        expected.extend_from_slice(&1u16.to_be_bytes());
        expected.extend_from_slice(&[MessageType::Hello as u8, 0x7f]);
        expected.extend_from_slice(&3u32.to_be_bytes());
        expected.extend_from_slice(&0x0102_0304_0506_0708u64.to_be_bytes());
        expected.extend_from_slice(&0x1112_1314_1516_1718u64.to_be_bytes());
        expected.extend_from_slice(&0x2122_2324_2526_2728u64.to_be_bytes());
        expected.extend_from_slice(b"abc");

        let encoded = Frame {
            version: 1,
            message_type: MessageType::Hello,
            flags: 0x7f,
            request_id: 0x0102_0304_0506_0708,
            sequence: 0x1112_1314_1516_1718,
            town_revision: 0x2122_2324_2526_2728,
            payload: b"abc".to_vec(),
        }
        .encode()
        .unwrap();

        assert_eq!(HEADER_SIZE, 36);
        assert_eq!(&encoded[..HEADER_SIZE], &expected[..HEADER_SIZE]);
        assert_eq!(encoded, expected);
    }

    #[test]
    fn frame_round_trip_preserves_all_fields_and_payload() {
        let original = frame(vec![0, 1, 2, 255]);
        assert_eq!(decode_frame(&original.encode().unwrap()).unwrap(), original);
    }

    #[test]
    fn empty_payload_round_trips() {
        let original = frame(Vec::new());
        let encoded = original.encode().unwrap();
        assert_eq!(encoded.len(), HEADER_SIZE);
        assert_eq!(decode_frame(&encoded).unwrap(), original);
    }

    #[test]
    fn rejects_bad_magic_version_and_unknown_message_type() {
        let encoded = frame(Vec::new()).encode().unwrap();
        let mut bad_magic = encoded.clone();
        bad_magic[0] = b'X';
        assert!(matches!(
            decode_frame(&bad_magic),
            Err(DecodeError::BadMagic { .. })
        ));

        let mut bad_version = encoded.clone();
        bad_version[5] = 2;
        assert!(matches!(
            decode_frame(&bad_version),
            Err(DecodeError::UnsupportedVersion { actual: 2 })
        ));

        let mut bad_type = encoded;
        bad_type[6] = 99;
        assert_eq!(
            decode_frame(&bad_type),
            Err(DecodeError::UnknownMessageType(99))
        );
    }

    #[test]
    fn rejects_short_header_and_truncated_payload() {
        assert!(matches!(
            decode_frame(&[0; HEADER_SIZE - 1]),
            Err(DecodeError::ShortHeader { .. })
        ));
        let encoded = frame(vec![1, 2, 3]).encode().unwrap();
        assert_eq!(
            decode_frame(&encoded[..encoded.len() - 1]),
            Err(DecodeError::TruncatedPayload {
                expected: 3,
                actual: 2
            })
        );
    }

    #[test]
    fn rejects_oversized_length_overflow_and_trailing_bytes() {
        let mut oversized = frame(Vec::new()).encode().unwrap();
        oversized[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            decode_frame(&oversized),
            Err(DecodeError::FrameTooLarge { .. })
        ));

        let mut trailing = frame(Vec::new()).encode().unwrap();
        trailing.push(0);
        assert_eq!(
            decode_frame(&trailing),
            Err(DecodeError::TrailingBytes { count: 1 })
        );
    }

    #[test]
    fn fragmented_input_emits_only_after_complete_frame() {
        let encoded = frame(b"fragment".to_vec()).encode().unwrap();
        let mut decoder = FrameDecoder::new();
        for byte in &encoded[..encoded.len() - 1] {
            assert!(
                decoder
                    .push(core::slice::from_ref(byte))
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(
            decoder.push(&encoded[encoded.len() - 1..]).unwrap(),
            vec![frame(b"fragment".to_vec())]
        );
        assert_eq!(decoder.buffered_len(), 0);
    }

    #[test]
    fn coalesced_input_emits_frames_in_order() {
        let first = frame(vec![1]).encode().unwrap();
        let mut second_frame = frame(vec![2, 3]);
        second_frame.message_type = MessageType::Snapshot;
        let second = second_frame.encode().unwrap();
        let mut input = first;
        input.extend_from_slice(&second);
        let mut decoder = FrameDecoder::new();
        assert_eq!(
            decoder.push(&input).unwrap(),
            vec![frame(vec![1]), second_frame]
        );
    }

    #[test]
    fn buffer_limit_rejects_unbounded_peer_input() {
        let mut decoder = FrameDecoder::new();
        let mut input = vec![0; HEADER_SIZE - 1];
        input.resize(MAX_BUFFERED_BYTES + 1, 0);
        assert!(matches!(
            decoder.push(&input),
            Err(DecodeError::BufferLimitExceeded { .. }) | Err(DecodeError::BadMagic { .. })
        ));
        assert!(decoder.buffered_len() <= MAX_BUFFERED_BYTES);
    }

    #[test]
    fn malformed_frames_do_not_panic() {
        let cases = [
            Vec::new(),
            vec![0; HEADER_SIZE - 1],
            vec![0xff; HEADER_SIZE],
            {
                let mut bytes = vec![0; HEADER_SIZE];
                bytes[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
                bytes
            },
        ];
        for bytes in cases {
            let result = std::panic::catch_unwind(|| decode_frame(&bytes));
            assert!(result.is_ok());
        }
    }
}
