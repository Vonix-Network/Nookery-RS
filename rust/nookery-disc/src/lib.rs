//! Bounded Animal Crossing disc catalog, header decoding, and verification for
//! the Nookery Rust port.
//!
//! This crate ports the seam owned by `pc/include/acgc/disc.hpp` and
//! `pc/src/acgc_disc.cpp` together with the source-verifiable parts of
//! `borealis::disc` at pin `8e9427fab92c1935a5d6bacadc6053c687f10c1e`:
//!
//! * the accepted-disc catalog (GAFE01, disc 0, revision 0, one XXH3-128 hash)
//!   and the recognized-only game IDs (GAFJ01, GAFP01) — [`catalog`];
//! * `parse_xxh3_128` and streaming `XXH3_128bits_*` — [`hash`];
//! * `open_and_inspect` / `verify_owned_stream` classification and hashing
//!   order — [`engine`];
//! * bounded RVZ/CISO metadata inspection and raw ISO/GCM file verification —
//!   [`stream_verify`];
//! * `validation_error` status mapping and the launch gate — [`error`];
//! * the `log_verification_state` line and `verification_to_config` /
//!   `verification_from_config` round trip — [`log`].
//!
//! `header::decode` follows `NodDiscHeader` in `nod.h` byte for byte: `game_id`
//! at 0, `disc_num` at 6, `disc_version` at 7, `wii_magic` at 24, `gcn_magic` at
//! 28, `game_title` at 32, for a 0x400-byte struct.  Those are the offsets
//! `nod_disc_header` fills and `platform_from_header` reads; the ISO-1024
//! equivalents are two bytes earlier and are *not* used.
//!
//! # Explicit unsupported boundary
//!
//! Upstream reaches the image through an `SDL_IOStream` and hands it to `nod`,
//! which decompresses RVZ/CISO/GCZ/WBFS/WIA/NFS/TGC and strips Wii partition
//! framing to produce a *logical* image.  That runtime is not in this repository
//! (the `extern/borealis` and `extern/aurora` submodules are unpopulated in this
//! worktree at this pin, and `nod` is supplied by CMake as `aurora::dvd` or
//! `nod::nod`), so this crate takes a [`image::DiscImage`] instead:
//!
//! | Upstream | Here |
//! |---|---|
//! | `SDL_IOStream`, `DiscStream` callbacks | caller-supplied [`image::DiscImage`] |
//! | `nod_disc_open_stream`, `nod_free` | not ported |
//! | `nod_disc_header` | [`header::decode`] on the first 0x400 bytes |
//! | `nod_disc_size` | [`image::DiscImage::logical_size`] |
//! | `nod_read` | [`image::DiscImage::read_at`] |
//! | `nod_error_message` | [`image::ImageError`] |
//!
//! An adapter that cannot decode a container must return
//! [`image::ImageError::Format`]; hashing compressed bytes would misreport a
//! valid image as [`ValidationError::HashMismatch`].
//! [`stream_verify::verify_path`] makes this boundary explicit for RVZ/CISO by
//! returning `Status::Failed` with a decoder-unavailable diagnostic after
//! bounded header/index inspection.
//!
//! Also out of scope, because no source evidence establishes them as pure
//! catalog operations: the prelaunch UI state machine, settings persistence
//! (`backend.discPath`, `backend.discVerification`), file dialogs, and
//! `aurora_dvd_open`.
//!
//! # Determinism and bounds
//!
//! * Reads are exactly one fixed 0x400-byte header plus fixed
//!   [`engine::HASH_BUFFER_SIZE`] chunks over the logical image.
//! * `DiscImage::read_at` must not return more than `buf.len()`.  An adapter
//!   that over-reports is rejected — `Status::Failed` for the header phase and
//!   `"Disc backend returned an oversized read"` for the hash loop — exactly as
//!   `verify_owned_stream`'s `read > HashBufferSize` arm does.  The count is
//!   never clamped, because clamping would hash or decode bytes the adapter
//!   never wrote.
//! * No hash or metadata value is invented: the catalog is the upstream literal.
//! * No disc image, game asset, or proprietary data ships with the crate; tests
//!   use synthetic byte buffers and XXH3-128 vectors recomputed outside this
//!   crate with reference `libxxhash`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod catalog;
pub mod engine;
pub mod error;
pub mod hash;
pub mod header;
pub mod image;
pub mod log;
pub mod metadata;
pub mod status;
pub mod stream_verify;
pub mod yaz0;

pub use catalog::{AcceptedDisc, Catalog, UPSTREAM};
pub use engine::{HASH_BUFFER_SIZE, inspect, inspect_error, verify, verify_simple};
pub use error::{ValidationError, validation_error};
pub use hash::{Digest128, Hasher, ParseDigestError};
pub use header::{HEADER_SIZE, Header, HeaderError, Platform};
pub use image::{Bytes, DiscImage, ImageError};
pub use log::{
    VerificationState, verification_from_config, verification_log_line, verification_to_config,
};
pub use metadata::{CancelFlag, Cancellation, Metadata, Never, Progress, Result};
pub use status::Status;
pub use stream_verify::{
    CISO_HEADER_SIZE, CISO_MAGIC, CISO_MIN_BLOCK_SIZE, ContainerKind, ContainerMetadata, RVZ_MAGIC,
    StreamVerifyError, inspect_path, inspect_stream, verify_path,
};
pub use yaz0::{
    HEADER_SIZE as YAZ0_HEADER_SIZE, MAGIC as YAZ0_MAGIC, Yaz0Error, decode as yaz0_decode,
};
