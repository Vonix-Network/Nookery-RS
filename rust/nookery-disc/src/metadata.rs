//! Metadata, progress, and result shapes mirroring `borealis::disc`.

use crate::header::{Header, Platform};
use crate::status::Status;

/// Disc metadata (`borealis::disc::Metadata`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Six-character game ID as text.
    pub game_id: String,
    /// Title text, NUL-trimmed.
    pub title: String,
    /// Platform implied by the header magic.
    pub platform: Platform,
    /// Disc number within a multi-disc release.
    pub disc_number: u8,
    /// Disc revision.
    pub revision: u8,
    /// Reconstructed image size used for verification.
    pub logical_size: u64,
}

impl Metadata {
    /// Builds metadata from a decoded header and the backing image size,
    /// following `open_and_inspect`.
    pub fn from_header(header: &Header, logical_size: u64) -> Self {
        Self {
            game_id: header.game_id_text(),
            title: header.title_text(),
            platform: header.platform,
            disc_number: header.disc_number,
            revision: header.revision,
            logical_size,
        }
    }
}

/// Metadata shown to the user for one path (`acgc::disc::DiscInfo`).
///
/// Mirrors the aggregate in `pc/include/acgc/disc.hpp`. [`DiscInfo::from_result`]
/// ports `acgc_disc.cpp::update_info`, which assigns all four fields from the
/// result metadata with no success test: a failed inspect shows the empty
/// metadata, not the previously displayed disc.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscInfo {
    /// Six-character game ID as text.
    pub game_id: String,
    /// Title text, NUL-trimmed.
    pub title: String,
    /// Disc number within a multi-disc release.
    pub disc_number: u8,
    /// Disc revision.
    pub revision: u8,
}

impl DiscInfo {
    /// Ports `acgc_disc.cpp::update_info`.
    ///
    /// The projection is unconditional: it is the caller's job to discard the
    /// value when the validation category does not allow launch, exactly as
    /// `prelaunch.cpp` resets `configuredDiscInfo` on a non-`Success` inspect.
    #[must_use]
    pub fn from_result(result: &Result) -> Self {
        Self {
            game_id: result.metadata.game_id.clone(),
            title: result.metadata.title.clone(),
            disc_number: result.metadata.disc_number,
            revision: result.metadata.revision,
        }
    }
}

/// Whether verification should stop (`Progress::cancelRequested`).
pub trait Cancellation {
    /// Returns `true` once the caller has asked to stop.
    fn is_canceled(&self) -> bool;
}

/// Cancellation that never fires.
#[derive(Clone, Copy, Debug, Default)]
pub struct Never;

impl Cancellation for Never {
    fn is_canceled(&self) -> bool {
        false
    }
}

/// Cancellation backed by an atomic flag, mirroring `Progress::cancelRequested`
/// being polled from another thread.
#[derive(Debug, Default)]
pub struct CancelFlag(std::sync::atomic::AtomicBool);

impl CancelFlag {
    /// Creates a flag that is not yet set.
    pub const fn new() -> Self {
        Self(std::sync::atomic::AtomicBool::new(false))
    }

    /// Requests cancellation.  Safe to call from any thread.
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Cancellation for CancelFlag {
    fn is_canceled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl<T: Cancellation + ?Sized> Cancellation for &T {
    fn is_canceled(&self) -> bool {
        (**self).is_canceled()
    }
}

/// Progress and outcome of one inspection or verification
/// (`borealis::disc::Result`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Result {
    /// Backend status.
    pub status: Status,
    /// Header metadata; populated once the header decodes.
    pub metadata: Metadata,
    /// Index into `Catalog::accepted_discs` when a record matched.
    pub accepted_disc_index: Option<usize>,
    /// Backend diagnostic.  Ports supply user-facing text separately.
    pub message: String,
}

/// A progress sample (`Progress::bytesRead` / `bytesTotal`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Bytes hashed so far.
    pub bytes_read: u64,
    /// Reconstructed image size, once known.
    pub bytes_total: u64,
}

impl Progress {
    /// Completion fraction clamped to `0.0..=1.0`, matching
    /// `prelaunch.cpp::DiscVerificationModal::refresh`.
    pub fn fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            return 0.0;
        }
        let fraction = self.bytes_read as f32 / self.bytes_total as f32;
        if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        }
    }
}
