//! Shared synthetic-image helpers.
//!
//! Each integration-test binary compiles its own copy, so unused helpers must
//! not warn.  No real disc data is used anywhere: every byte is generated from
//! the index, and the header fields are patched in.

#![allow(dead_code)]

use nookery_disc::{DiscImage, ImageError};

/// Logical size used by the synthetic image: four 1 MiB hash chunks plus 7
/// bytes, so chunked hashing is exercised.
pub const IMAGE_LEN: usize = 3 * 1024 * 1024 + 7;

/// Disc magic patched into the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Magic {
    /// GameCube magic `C2 33 9F 3D`.
    GameCube,
    /// Wii magic `5D 1C 9E A3`.
    Wii,
    /// No magic at all.
    None,
}

/// Title used by [`image`].
pub const TITLE: &[u8] = b"Nookery synthetic image";

/// Builds a deterministic synthetic image of [`IMAGE_LEN`] bytes.
///
/// Body byte `i` is `(i + (i >> 8) * 7) & 0xFF`; then the header fields are
/// overwritten at the offsets `NodDiscHeader` declares in `nod.h`:
/// `game_id` at 0, `disc_num` at 6, `disc_version` at 7, `wii_magic` at 24,
/// `gcn_magic` at 28, `game_title` at 32.
pub fn image(game_id: &[u8; 6], disc: u8, revision: u8, magic: Magic) -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..IMAGE_LEN)
        .map(|i| ((i + (i >> 8) * 7) & 0xFF) as u8)
        .collect();
    bytes[0..6].copy_from_slice(game_id);
    bytes[6] = disc;
    bytes[7] = revision;
    match magic {
        Magic::GameCube => bytes[28..32].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]),
        Magic::Wii => bytes[24..28].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]),
        Magic::None => {
            bytes[24..28].fill(0);
            bytes[28..32].fill(0);
        }
    }
    // The title field is NUL-free pattern bytes otherwise; `header_title` stops
    // at the first NUL, so clear it before writing the title.
    bytes[32..96].fill(0);
    bytes[32..32 + TITLE.len()].copy_from_slice(TITLE);
    bytes
}

/// Adapter that fails every read with `error`.
pub struct FailingImage {
    /// Error returned by every read.
    pub error: ImageError,
    /// Reported logical size.
    pub size: u64,
    /// Number of read attempts made.
    pub attempts: usize,
}

impl FailingImage {
    /// Creates the adapter.
    pub fn new(error: ImageError, size: u64) -> Self {
        Self {
            error,
            size,
            attempts: 0,
        }
    }
}

impl DiscImage for FailingImage {
    fn logical_size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, ImageError> {
        self.attempts += 1;
        Err(self.error)
    }
}

/// Adapter over a real image that claims a larger logical size than it can
/// serve, to prove verification rejects a short read instead of hashing it.
pub struct ShortImage {
    /// Wrapped logical image bytes.
    pub bytes: Vec<u8>,
    /// Size the backend claims.
    pub reported: u64,
}

impl DiscImage for ShortImage {
    fn logical_size(&self) -> u64 {
        self.reported
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError> {
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let count = (self.bytes.len() - start).min(buf.len());
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }
}

/// Adapter wrapping a real image but reporting one byte more than it wrote on
/// **every** read.
///
/// Proves the header phase rejects an over-report (`Status::Failed`) rather
/// than clamping and decoding bytes the adapter never wrote.
pub struct OversizedImage {
    /// Wrapped logical image bytes.
    pub bytes: Vec<u8>,
}

impl DiscImage for OversizedImage {
    fn logical_size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError> {
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let count = (self.bytes.len() - start).min(buf.len());
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count + 1)
    }
}

/// Adapter that serves the header honestly and then over-reports on hash
/// chunks, isolating `verify_owned_stream`'s oversized-read arm.
pub struct OversizedAfterHeaderImage {
    /// Wrapped logical image bytes.
    pub bytes: Vec<u8>,
    /// Read calls served so far.
    pub reads: usize,
}

impl DiscImage for OversizedAfterHeaderImage {
    fn logical_size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError> {
        let call = self.reads;
        self.reads += 1;
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let count = (self.bytes.len() - start).min(buf.len());
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(if call == 0 { count } else { count + 1 })
    }
}

/// Adapter that serves `bytes` normally until read call `fail_at` (0-based),
/// then fails with `error`.  Mirrors `FakeBackend::failReadCall` upstream.
pub struct FailAfterImage {
    /// Wrapped logical image bytes.
    pub bytes: Vec<u8>,
    /// Zero-based read call index that fails.
    pub fail_at: usize,
    /// Error returned by the failing call.
    pub error: ImageError,
    /// Read calls served so far.
    pub reads: usize,
}

impl DiscImage for FailAfterImage {
    fn logical_size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError> {
        let call = self.reads;
        self.reads += 1;
        if call == self.fail_at {
            return Err(self.error);
        }
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let count = (self.bytes.len() - start).min(buf.len());
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }
}

/// Adapter that records every `(offset, requested_len)` pair it is asked for,
/// so tests can assert the read discipline structurally.
pub struct RecordingImage {
    /// Wrapped logical image bytes.
    pub bytes: Vec<u8>,
    /// Every request, in order.
    pub requests: Vec<(u64, usize)>,
    /// Largest buffer size ever offered.
    pub max_requested: usize,
}

impl RecordingImage {
    /// Wraps an image.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            requests: Vec::new(),
            max_requested: 0,
        }
    }
}

impl DiscImage for RecordingImage {
    fn logical_size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError> {
        self.requests.push((offset, buf.len()));
        self.max_requested = self.max_requested.max(buf.len());
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let count = (self.bytes.len() - start).min(buf.len());
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }
}
