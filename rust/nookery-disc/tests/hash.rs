//! XXH3-128 parsing and streaming vectors.
//!
//! Every expected digest was computed outside this crate with the reference
//! `xxhash` Python binding (`xxhash.xxh3_128`), not with `xxhash-rust`.

use nookery_disc::{Digest128, Hasher, ParseDigestError};

#[test]
fn digest_parse_splits_high_bits_first() {
    let parsed = Digest128::parse("0123456789abcdeffedcba9876543210").unwrap();
    assert_eq!(parsed.high, 0x0123_4567_89ab_cdef);
    assert_eq!(parsed.low, 0xfedc_ba98_7654_3210);
}

#[test]
fn digest_parse_accepts_uppercase_like_the_cpp_helper() {
    let lower = Digest128::parse("0123456789abcdeffedcba9876543210").unwrap();
    let upper = Digest128::parse("0123456789ABCDEFFEDCBA9876543210").unwrap();
    assert_eq!(lower, upper);
}

#[test]
fn digest_parse_rejects_wrong_length_and_bad_digits() {
    assert_eq!(
        Digest128::parse("abc"),
        Err(ParseDigestError::Length { len: 3 })
    );
    assert_eq!(
        Digest128::parse("0123456789abcdeffedcba98765432100"),
        Err(ParseDigestError::Length { len: 33 })
    );
    // Offset 4 is the first non-hex character in this input.
    assert_eq!(
        Digest128::parse("0123z56789abcdeffedcba9876543210"),
        Err(ParseDigestError::Digit { index: 4 })
    );
    // Offset 20 lies in the low half, proving the index is absolute.
    assert_eq!(
        Digest128::parse("0123456789abcdeffedcz9876543210a"),
        Err(ParseDigestError::Digit { index: 20 })
    );
    // A two-byte non-ASCII character at the end is 32 bytes, and the offending
    // byte index is reported without slicing at a char boundary.
    assert_eq!(
        Digest128::parse("0123456789abcdeffedcba98765432é"),
        Err(ParseDigestError::Digit { index: 30 })
    );
}

fn hashed(chunks: &[&[u8]]) -> String {
    let mut hasher = Hasher::new();
    for chunk in chunks {
        hasher.update(chunk);
    }
    hasher.finish().to_string()
}

#[test]
fn empty_input_matches_reference_vector() {
    assert_eq!(hashed(&[]), "99aa06d3014798d86001c324468d497f");
}

#[test]
fn small_input_matches_reference_vector() {
    assert_eq!(hashed(&[b"abc"]), "06b05ab6733a618578af5f94892f3950");
}

#[test]
fn multi_stripe_input_matches_reference_vector() {
    let pattern: Vec<u8> = (0..256u16).map(|i| i as u8).cycle().take(4096).collect();
    assert_eq!(
        hashed(&[&pattern]),
        "03916578969f7a66eb4b7c3707879151",
        "4 KiB crosses the XXH3 stripe boundaries"
    );
}

#[test]
fn chunked_streaming_matches_the_whole_buffer() {
    // 3 MiB + 7 bytes: the reference digest for this body is
    // 3a975734b5e2b847a0dc5033bb5aa2aa.  Splitting at 1 MiB boundaries must not
    // change it, which is what makes the bounded verification loop valid.
    let body: Vec<u8> = (0..3 * 1024 * 1024 + 7usize)
        .map(|i| ((i + (i >> 8) * 7) & 0xFF) as u8)
        .collect();
    assert_eq!(hashed(&[&body]), "3a975734b5e2b847a0dc5033bb5aa2aa");
    let chunks: Vec<&[u8]> = body.chunks(1024 * 1024).collect();
    assert_eq!(chunks.len(), 4);
    assert_eq!(hashed(&chunks), "3a975734b5e2b847a0dc5033bb5aa2aa");
}
