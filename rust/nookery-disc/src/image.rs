//! The disc-image seam.
//!
//! Upstream, `borealis::disc` obtains bytes from an `SDL_IOStream` wrapped in
//! `DiscStream`, and `nod` reconstructs the *logical* image (decompressing RVZ /
//! CISO / GCZ / WBFS / WIA / NFS / TGC and stripping Wii partition framing)
//! before the header is decoded and the hash is computed.  Porting `nod` means
//! importing a compressed-image runtime, so this crate stops at a trait:
//!
//! | Upstream | Here |
//! |---|---|
//! | `SDL_IOStream` + `DiscStream` callbacks | caller-provided [`DiscImage`] |
//! | `nod_disc_open_stream` | not ported; the caller builds the adapter |
//! | `nod_disc_header` | [`crate::header::decode`] over the first 0x400 bytes |
//! | `nod_disc_size` | [`DiscImage::logical_size`] |
//! | `nod_read` | [`DiscImage::read_at`] |
//! | `nod_error_message` | [`ImageError`] |
//!
//! An adapter that cannot decode a container must report [`ImageError::Format`]
//! rather than hashing the container's own bytes: hashing compressed bytes
//! would produce `HashMismatch` for a *valid* image, which is a wrong answer, not
//! a missing feature.

/// Backend failure classes, mirroring `status_from_nod_result`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageError {
    /// `NOD_RESULT_ERR_IO` — the image could not be read.
    Io,
    /// `NOD_RESULT_ERR_FORMAT` — the image is not a decodable disc container.
    Format,
    /// Any other backend failure (`Status::Failed`).
    #[default]
    Failed,
}

/// A sequential reader over a **reconstructed logical disc image**.
///
/// `logical_size` must be the size `nod_disc_size` would report (the
/// uncompressed, unframed image), because verification compares the bytes read
/// against it and hashes them.
pub trait DiscImage {
    /// Reconstructed image size in bytes.
    fn logical_size(&self) -> u64;

    /// Reads up to `buf.len()` bytes at `offset`.
    ///
    /// Returns the number of bytes written, which must not exceed `buf.len()`;
    /// `0` means end of image.  The slice bound replaces nod's "oversized read"
    /// guard structurally.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError>;
}

/// A caller-supplied byte buffer treated as the reconstructed image.
///
/// Useful for tests and for hosts that already decoded a container; the bytes
/// are trusted to be logical image content, exactly as a `nod` adapter's output.
#[derive(Clone, Copy, Debug)]
pub struct Bytes<'a> {
    bytes: &'a [u8],
    position: u64,
}

impl<'a> Bytes<'a> {
    /// Wraps a logical image slice.
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    /// Current sequential read position.
    pub const fn position(&self) -> u64 {
        self.position
    }

    /// Logical size of the wrapped slice.
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the wrapped slice is empty.
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl DiscImage for Bytes<'_> {
    fn logical_size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError> {
        let start = usize::try_from(offset).map_err(|_| ImageError::Io)?;
        if start >= self.bytes.len() {
            self.position = self.bytes.len() as u64;
            return Ok(0);
        }
        let available = &self.bytes[start..];
        let count = available.len().min(buf.len());
        buf[..count].copy_from_slice(&available[..count]);
        self.position = start as u64 + count as u64;
        Ok(count)
    }
}
