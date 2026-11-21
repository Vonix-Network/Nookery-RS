//! The disc-image and host-stream seams.
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

/// Failure from the seekable host stream adapter.
#[derive(Debug)]
pub enum StreamError {
    /// The operation was attempted after [`DiscStream::close`].
    Closed,
    /// The host stream returned an I/O error.
    Io(std::io::Error),
    /// The host stream reported a position that cannot be represented.
    PositionOverflow,
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("disc stream is closed"),
            Self::Io(error) => write!(formatter, "disc stream I/O failed: {error}"),
            Self::PositionOverflow => formatter.write_str("disc stream position overflows u64"),
        }
    }
}

impl std::error::Error for StreamError {}

/// An owned, seekable stream adapter matching Borealis' `DiscStream` callback
/// contract (`read_at`, `stream_len`, and `close`).
///
/// Opening measures the source once and rewinds it to offset zero. Reads use
/// the same absolute-offset semantics as `stream_read_at`: they seek before
/// reading and return a short count at EOF. `close` is explicit and idempotent;
/// dropping an open adapter closes its owned reader as well.
pub struct DiscStream<R> {
    reader: Option<R>,
    length: u64,
    position: u64,
}

impl<R: std::io::Read + std::io::Seek> DiscStream<R> {
    /// Opens a seekable reader and records its byte length.
    pub fn open(mut reader: R) -> Result<Self, StreamError> {
        let end = reader
            .seek(std::io::SeekFrom::End(0))
            .map_err(StreamError::Io)?;
        reader
            .seek(std::io::SeekFrom::Start(0))
            .map_err(StreamError::Io)?;
        Ok(Self {
            reader: Some(reader),
            length: end,
            position: 0,
        })
    }

    /// Returns the source length recorded at open time.
    pub const fn len(&self) -> u64 {
        self.length
    }

    /// Whether the source length recorded at open time is zero.
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Whether the adapter has been explicitly closed.
    pub const fn is_closed(&self) -> bool {
        self.reader.is_none()
    }

    /// Returns the current host-stream cursor.
    pub const fn tell(&self) -> u64 {
        self.position
    }

    /// Reads from the current cursor, returning a short count at EOF.
    pub fn read(&mut self, output: &mut [u8]) -> Result<usize, StreamError> {
        let reader = self.reader.as_mut().ok_or(StreamError::Closed)?;
        let read = reader.read(output).map_err(StreamError::Io)?;
        self.position = self
            .position
            .checked_add(read as u64)
            .ok_or(StreamError::PositionOverflow)?;
        Ok(read)
    }

    /// Reads from an absolute offset, preserving the callback's seek-before-
    /// read behavior and updating the current cursor.
    pub fn read_at(&mut self, offset: u64, output: &mut [u8]) -> Result<usize, StreamError> {
        self.seek(std::io::SeekFrom::Start(offset))?;
        self.read(output)
    }

    /// Moves the current cursor using standard `Read + Seek` semantics.
    pub fn seek(&mut self, from: std::io::SeekFrom) -> Result<u64, StreamError> {
        let reader = self.reader.as_mut().ok_or(StreamError::Closed)?;
        let position = reader.seek(from).map_err(StreamError::Io)?;
        self.position = position;
        Ok(position)
    }

    /// Closes the adapter. Repeated closes are successful and do not touch the
    /// underlying reader again.
    pub fn close(&mut self) -> Result<(), StreamError> {
        self.reader.take();
        Ok(())
    }
}

impl<R> Drop for DiscStream<R> {
    fn drop(&mut self) {
        self.reader.take();
    }
}

impl<R: std::io::Read + std::io::Seek> DiscImage for DiscStream<R> {
    fn logical_size(&self) -> u64 {
        self.length
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, ImageError> {
        self.read_at(offset, buffer).map_err(|error| match error {
            StreamError::Closed => ImageError::Failed,
            StreamError::Io(_) | StreamError::PositionOverflow => ImageError::Io,
        })
    }
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
