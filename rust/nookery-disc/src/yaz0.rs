//! Yaz0 decompression, ported 1:1 from `pc/src/pc_assets.c:47-94`
//! (`aurora_yaz0_decode`) and exercised against the C oracle by
//! `tools/yaz0-oracle`.
//!
//! # Scope
//!
//! This is the decoder the PC port uses to unpack `/foresta.rel.szs` before
//! mounting the REL. It is intentionally a faithful transcription of the
//! upstream algorithm, including its exact truncation and rejection behavior:
//!
//! * a source shorter than 16 bytes is rejected before the magic is examined;
//! * the header magic must be `Yaz0`, else the input is rejected;
//! * the decoded size comes from the big-endian `u32` at offset 4;
//! * literals are taken one byte per set flag bit, copies otherwise;
//! * a copy whose back-distance reaches before the start of output is a hard
//!   rejection (`BAD_DISTANCE`) rather than a clamp;
//! * running out of source mid-stream ends decoding, and a short result is
//!   rejected (`SHORT_OUTPUT`) rather than returned partially.
//!
//! The upstream function returns `NULL` for every failure, so the C path
//! cannot distinguish them; this port returns a typed error instead, which is
//! a strict improvement that does not change which inputs are accepted.
//!
//! # Explicit non-goals
//!
//! No encoder, no streaming API, no RARC parsing, and no allocation strategy
//! matching beyond the output size. This module does not touch the disc
//! filesystem; callers hand it bytes.

#![forbid(unsafe_code)]

/// Header magic for a Yaz0 stream.
pub const MAGIC: [u8; 4] = *b"Yaz0";

/// Bytes of header consumed before the first control byte
/// (`pc_assets.c:48`, `src_pos = 16`).
pub const HEADER_SIZE: usize = 16;

/// Why a Yaz0 stream could not be decoded.
///
/// The C implementation collapses every case into a `NULL` return
/// (`pc_assets.c:52-91`); these variants name the same conditions so a
/// differential harness can compare rejection *reasons*, not just rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Yaz0Error {
    /// `src_size < 16`, checked before the magic (`pc_assets.c:52`).
    TooShort,
    /// The first four bytes are not `Yaz0` (`pc_assets.c:52`).
    BadMagic,
    /// A copy referenced a distance at or beyond the produced output
    /// (`pc_assets.c:76-79`).
    BadDistance,
    /// Decoding ended before `decoded_size` bytes were produced
    /// (`pc_assets.c:88-91`).
    ShortOutput,
    /// `decoded_size` cannot be represented as a `usize` on this target.
    SizeOverflow,
    /// The declared output size exceeds the caller's limit.
    TooLarge {
        /// The decoded size declared in the header.
        declared: u32,
        /// The maximum this call would allocate.
        limit: usize,
    },
}

/// Reads a big-endian `u32`, mirroring `pc_load_be32` (`pc_bswap.h:24`).
#[must_use]
fn load_be32(bytes: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*bytes)
}

/// Decode a Yaz0 stream, reproducing `aurora_yaz0_decode`.
///
/// Returns the decoded bytes on success. On failure the returned error names
/// the same condition the C path would have failed on.
///
/// # Errors
///
/// Returns [`Yaz0Error`] for every rejection the C implementation performs, plus
/// [`Yaz0Error::TooLarge`] when `limit` is exceeded, which lets a host bound
/// allocation without changing accept/reject behavior for inputs under the cap.
pub fn decode(src: &[u8], limit: usize) -> Result<Vec<u8>, Yaz0Error> {
    // pc_assets.c:52 tests the length first, so a short buffer never reaches
    // the magic comparison.
    if src.len() < HEADER_SIZE {
        return Err(Yaz0Error::TooShort);
    }
    if src[..4] != MAGIC {
        return Err(Yaz0Error::BadMagic);
    }

    let decoded_size = load_be32(&[src[4], src[5], src[6], src[7]]);
    let decoded_size = usize::try_from(decoded_size).map_err(|_| Yaz0Error::SizeOverflow)?;
    if decoded_size > limit {
        return Err(Yaz0Error::TooLarge {
            declared: u32::try_from(decoded_size).unwrap_or(u32::MAX),
            limit,
        });
    }

    let mut dst: Vec<u8> = Vec::with_capacity(decoded_size);
    let mut src_pos = HEADER_SIZE;
    let mut dst_pos = 0usize;

    // pc_assets.c:57
    while dst_pos < decoded_size && src_pos < src.len() {
        let flags = src[src_pos];
        src_pos += 1;

        // pc_assets.c:59 -- note the loop stops on either the bit running out
        // or the output being complete.
        let mut bit = 7i32;
        while bit >= 0 && dst_pos < decoded_size {
            if src_pos >= src.len() {
                break;
            }
            if flags & (1u8 << bit) != 0 {
                // Literal byte.
                dst.push(src[src_pos]);
                src_pos += 1;
                dst_pos += 1;
            } else {
                // Back-reference.
                if src_pos + 1 >= src.len() {
                    break;
                }
                let b1 = src[src_pos];
                let b2 = src[src_pos + 1];
                src_pos += 2;

                let distance = (u32::from(b1 & 0x0F) << 8) | u32::from(b2);
                let length = if (b1 >> 4) == 0 {
                    if src_pos >= src.len() {
                        break;
                    }
                    let extra = u32::from(src[src_pos]);
                    src_pos += 1;
                    extra + 0x12
                } else {
                    u32::from(b1 >> 4) + 2
                };

                // pc_assets.c:76 -- reaching before the output start is a hard
                // rejection, not a clamp.
                let distance = usize::try_from(distance).map_err(|_| Yaz0Error::BadDistance)?;
                if distance >= dst_pos {
                    return Err(Yaz0Error::BadDistance);
                }
                let mut copy_pos = dst_pos - distance - 1;

                let mut remaining = length;
                while remaining != 0 && dst_pos < decoded_size {
                    // Overlapping copies are expected: the source and the
                    // destination are the same buffer, exactly as upstream.
                    let byte = dst[copy_pos];
                    dst.push(byte);
                    copy_pos += 1;
                    dst_pos += 1;
                    remaining -= 1;
                }
            }
            bit -= 1;
        }
    }

    // pc_assets.c:88
    if dst_pos != decoded_size {
        return Err(Yaz0Error::ShortOutput);
    }
    Ok(dst)
}
