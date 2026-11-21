//! Tests for the Yaz0 decoder ported from `pc/src/pc_assets.c:47-94`.
//!
//! Vectors are constructed by hand so the expected behavior comes from the C
//! algorithm's control flow, and each rejection case names the exact upstream
//! line it reproduces.

use nookery_disc::yaz0::{self, Yaz0Error};

/// Build a Yaz0 header for a stream body.
fn stream(decoded_size: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Yaz0");
    out.extend_from_slice(&decoded_size.to_be_bytes());
    // The C decoder reads the size from offset 4 and starts at offset 16, so
    // the remaining eight header bytes are unused padding.
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(body);
    out
}

#[test]
fn a_source_shorter_than_the_header_is_rejected_before_the_magic() {
    // pc_assets.c:52 tests `src_size < 16` first, so even a valid magic of four
    // bytes must report TooShort rather than BadMagic.
    assert_eq!(yaz0::decode(b"Yaz0", 1 << 20), Err(Yaz0Error::TooShort));
    assert_eq!(yaz0::decode(&[], 1 << 20), Err(Yaz0Error::TooShort));
}

#[test]
fn a_long_enough_source_with_the_wrong_magic_is_rejected() {
    // pc_assets.c:52 second half.
    let mut bad = vec![0u8; 32];
    bad[..4].copy_from_slice(b"Yaz1");
    assert_eq!(yaz0::decode(&bad, 1 << 20), Err(Yaz0Error::BadMagic));
}

#[test]
fn literals_are_copied_one_byte_per_set_flag_bit() {
    // Three literal bytes behind flags 0b1110_0000.
    let data = stream(3, &[0b1110_0000, b'A', b'B', b'C']);
    assert_eq!(yaz0::decode(&data, 1 << 20), Ok(b"ABC".to_vec()));
}

#[test]
fn an_overlapping_back_reference_repeats_the_previous_byte() {
    // One literal then a copy with distance 0 (the immediately preceding byte)
    // and length 3, which is how upstream expands runs (pc_assets.c:76-83).
    let data = stream(4, &[0b1010_0000, b'A', 0x10, 0x00]);
    assert_eq!(yaz0::decode(&data, 1 << 20), Ok(b"AAAA".to_vec()));
}

#[test]
fn a_copy_reaching_before_the_output_start_is_a_hard_rejection() {
    // pc_assets.c:76-79: distance >= dst_pos frees and returns NULL.  Here the
    // very first opcode is a copy at dst_pos 0, so distance 0 is already
    // out of range.
    let data = stream(2, &[0b0100_0000, 0x10, 0x00]);
    assert_eq!(yaz0::decode(&data, 1 << 20), Err(Yaz0Error::BadDistance));
}

#[test]
fn ending_early_is_rejected_rather_than_returning_partial_output() {
    // pc_assets.c:88-91: the result must be exactly decoded_size bytes.
    let data = stream(10, &[0b1000_0000, b'X']);
    assert_eq!(yaz0::decode(&data, 1 << 20), Err(Yaz0Error::ShortOutput));
}

#[test]
fn the_long_copy_form_reads_the_extra_length_byte() {
    // Bit 7 set consumes one literal; bit 6 clear is then a copy that reads
    // b1, b2 and the extra length byte.  b1's high nibble is 0, so the length
    // comes from the extra byte (pc_assets.c:70-72).
    let body = vec![
        0b1000_0000, // bit 7 literal, bit 6 copy
        b'Z',        // the literal byte
        0x00,        // b1: high nibble 0 -> long form, distance high nibble 0
        0x00,        // b2: distance 0 -> repeat the previous byte
        0x02,        // extra length -> 0x02 + 0x12 = 20 bytes
    ];
    let data = stream(21, &body);
    let decoded = yaz0::decode(&data, 1 << 20).expect("long-form copy decodes");
    assert_eq!(decoded.len(), 21);
    assert!(decoded.iter().all(|byte| *byte == b'Z'));
}

#[test]
fn the_caller_limit_bounds_allocation_without_changing_small_inputs() {
    let data = stream(3, &[0b1110_0000, b'A', b'B', b'C']);
    assert_eq!(
        yaz0::decode(&data, 2),
        Err(Yaz0Error::TooLarge {
            declared: 3,
            limit: 2,
        })
    );
    // Exactly at the limit is still accepted.
    assert_eq!(yaz0::decode(&data, 3), Ok(b"ABC".to_vec()));
}

#[test]
fn a_declared_size_of_zero_is_accepted_as_empty_output() {
    // dst_pos == decoded_size == 0 satisfies the final check, so the C path
    // returns a zero-length buffer rather than failing.
    let data = stream(0, &[]);
    assert_eq!(yaz0::decode(&data, 1 << 20), Ok(Vec::new()));
}
