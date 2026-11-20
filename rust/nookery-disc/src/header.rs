//! Decoding of the shared GameCube/Wii disc header (`NodDiscHeader` in `nod.h`).
//!
//! [`decode`] is the only parser in this crate.  It reads a fixed-size header
//! slice and never trusts a length field, so it cannot over-read or allocate on
//! attacker-controlled data.

/// Size in bytes of the shared disc header (`NodDiscHeader`).
pub const HEADER_SIZE: usize = 0x400;

/// Byte offset of the header within a disc image and within each Wii partition.
pub const HEADER_OFFSET: u64 = 0;

// Byte offsets of `NodDiscHeader` members, exactly as the C struct in `nod.h`
// lays them out:
//
//   0   game_id[6]                  24  wii_magic[4]
//   6   disc_num                    28  gcn_magic[4]
//   7   disc_version                32  game_title[64]
//   8   audio_streaming             96  no_partition_hashes
//   9   audio_stream_buf_size       97  no_partition_encryption
//   10  _pad1[14]                   98  _pad2[926]  (ends at 0x400)
const GAME_ID_RANGE: core::ops::Range<usize> = 0..6;
const DISC_NUMBER_INDEX: usize = 6;
const DISC_VERSION_INDEX: usize = 7;
const WII_MAGIC_RANGE: core::ops::Range<usize> = 24..28;
const GCN_MAGIC_RANGE: core::ops::Range<usize> = 28..32;
const TITLE_RANGE: core::ops::Range<usize> = 32..96;

/// `nod::disc::WII_MAGIC`.
pub const WII_MAGIC: [u8; 4] = [0x5D, 0x1C, 0x9E, 0xA3];

/// `nod::disc::GCN_MAGIC`.
pub const GCN_MAGIC: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];

/// Platform implied by the header magic, mirroring `borealis::disc::Platform`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Neither magic matched.
    #[default]
    Unknown,
    /// `gcn_magic` matched [`GCN_MAGIC`].
    GameCube,
    /// `wii_magic` matched [`WII_MAGIC`].
    Wii,
}

/// Decoded header fields used by the catalog match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Raw six-byte game ID, as `header.game_id`.
    pub game_id: [u8; 6],
    /// Disc number within a multi-disc release.
    pub disc_number: u8,
    /// Disc revision (`header.disc_version`).
    pub revision: u8,
    /// Title bytes up to the first NUL, as `header_title` returns them.
    pub title: Vec<u8>,
    /// Platform implied by the magic bytes.
    pub platform: Platform,
}

/// Failure while decoding a header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderError {
    /// The slice is shorter than [`HEADER_SIZE`].
    Truncated {
        /// Actual byte length available.
        len: usize,
    },
}

impl std::fmt::Display for HeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Truncated { len } => {
                write!(
                    f,
                    "disc header truncated: {len} bytes available, {HEADER_SIZE} required"
                )
            }
        }
    }
}

impl std::error::Error for HeaderError {}

impl Header {
    /// Game ID as text, mirroring `Metadata::gameId.assign(begin, end)`.
    ///
    /// The bytes are not validated as UTF-8 upstream either; non-ASCII IDs are
    /// surfaced lossily rather than rejected, so a catalog lookup stays
    /// deterministic.
    pub fn game_id_text(&self) -> String {
        String::from_utf8_lossy(&self.game_id).into_owned()
    }

    /// Title as text, mirroring `std::string{begin(header.game_title), end}`.
    pub fn title_text(&self) -> String {
        String::from_utf8_lossy(&self.title).into_owned()
    }
}

fn platform_from_magic(wii: &[u8], gcn: &[u8]) -> Platform {
    // `platform_from_header` tests GameCube first, then Wii.
    if gcn == GCN_MAGIC {
        Platform::GameCube
    } else if wii == WII_MAGIC {
        Platform::Wii
    } else {
        Platform::Unknown
    }
}

/// Decodes a header from the first [`HEADER_SIZE`] bytes of an image.
///
/// Mirrors `platform_from_header` and `header_title` from
/// `extern/borealis/src/disc.cpp`: the title stops at the first NUL byte and the
/// platform magic check prefers GameCube.
pub fn decode(bytes: &[u8]) -> Result<Header, HeaderError> {
    if bytes.len() < HEADER_SIZE {
        return Err(HeaderError::Truncated { len: bytes.len() });
    }
    let header = &bytes[..HEADER_SIZE];
    let title_end = header[TITLE_RANGE]
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(TITLE_RANGE.len());
    let mut game_id = [0u8; 6];
    game_id.copy_from_slice(&header[GAME_ID_RANGE]);
    Ok(Header {
        game_id,
        disc_number: header[DISC_NUMBER_INDEX],
        revision: header[DISC_VERSION_INDEX],
        title: header[TITLE_RANGE.start..TITLE_RANGE.start + title_end].to_vec(),
        platform: platform_from_magic(&header[WII_MAGIC_RANGE], &header[GCN_MAGIC_RANGE]),
    })
}
