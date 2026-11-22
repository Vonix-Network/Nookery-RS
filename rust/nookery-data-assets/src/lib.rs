//! Pure-Rust replacement candidate for the title-demo data plane.
//!
//! The five C sources under `src/data/titledemo/` contain only static `u16`
//! arrays.  [`pact0_head_table`] and the other exported tables are produced by
//! `tools/titledemo/generate_rust.py`; it is checked in so builds do not need a
//! C compiler, FFI, or access to generated build output.  The exported Rust
//! statics retain the C identifiers, element counts, ordering, and values.
//!
//! Attribution: the source data is part of ACreTeam/ac-decomp (CC0 1.0), with
//! the repository's dual-license and attribution terms retained in `LICENSE`.

mod generated;

pub use generated::{
    pact0_head_table, pact0_key_data, pact1_head_table, pact1_key_data, pact2_head_table,
    pact2_key_data, pact3_head_table, pact3_key_data, pact4_head_table, pact4_key_data,
};

/// Number of title-demo recordings selected by `mTD_TITLE_DEMO_NUM`.
pub const TITLE_DEMO_COUNT: usize = 5;
/// Number of words in each source `pactN_head_table` object.
pub const TITLE_DEMO_HEADER_WORDS: usize = 6;
/// The source manifest is included in the crate for audit tooling.
pub const SOURCE_MANIFEST: &str = include_str!("../source-manifest.txt");

/// The exact source-order header tables used by `get_demo_header`.
pub static TITLE_DEMO_HEAD_TABLES: [&[u16; TITLE_DEMO_HEADER_WORDS]; TITLE_DEMO_COUNT] = [
    &pact0_head_table,
    &pact1_head_table,
    &pact2_head_table,
    &pact3_head_table,
    &pact4_head_table,
];

/// The exact source-order key streams used by `get_tdemo_keydata`.
pub static TITLE_DEMO_KEY_DATA: [&[u16]; TITLE_DEMO_COUNT] = [
    &pact0_key_data,
    &pact1_key_data,
    &pact2_key_data,
    &pact3_key_data,
    &pact4_key_data,
];

/// Look up one word using the same demo-then-field indexing as the C caller.
///
/// The C implementation receives a validated title-demo number and one of the
/// five header fields it consumes.  Rust makes malformed indices explicit by
/// returning `None`; valid indices return the original `u16` without decoding
/// or changing its byte value.
pub const fn demo_header(demo: usize, field: usize) -> Option<u16> {
    if demo >= TITLE_DEMO_COUNT || field >= TITLE_DEMO_HEADER_WORDS {
        return None;
    }
    Some(TITLE_DEMO_HEAD_TABLES[demo][field])
}

/// Look up one recorded 30-fps input word by demo and frame.
pub const fn demo_key_data(demo: usize, frame: usize) -> Option<u16> {
    if demo >= TITLE_DEMO_COUNT {
        return None;
    }
    let stream = TITLE_DEMO_KEY_DATA[demo];
    if frame >= stream.len() {
        None
    } else {
        Some(stream[frame])
    }
}

/// Return a complete header table for a valid title-demo number.
pub const fn demo_header_table(demo: usize) -> Option<&'static [u16; TITLE_DEMO_HEADER_WORDS]> {
    if demo >= TITLE_DEMO_COUNT {
        None
    } else {
        Some(TITLE_DEMO_HEAD_TABLES[demo])
    }
}

/// Return a complete key stream for a valid title-demo number.
pub const fn demo_key_stream(demo: usize) -> Option<&'static [u16]> {
    if demo >= TITLE_DEMO_COUNT {
        None
    } else {
        Some(TITLE_DEMO_KEY_DATA[demo])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_boundaries_are_rejected_without_panicking() {
        assert_eq!(demo_header(0, 0), Some(0x0884));
        assert_eq!(demo_header(4, TITLE_DEMO_HEADER_WORDS - 1), Some(0x072A));
        assert_eq!(demo_header(TITLE_DEMO_COUNT, 0), None);
        assert_eq!(demo_header(0, TITLE_DEMO_HEADER_WORDS), None);

        assert_eq!(demo_key_data(0, 0), Some(0));
        assert_eq!(
            demo_key_data(4, pact4_key_data.len() - 1),
            Some(pact4_key_data[pact4_key_data.len() - 1])
        );
        assert_eq!(demo_key_data(TITLE_DEMO_COUNT, 0), None);
        assert_eq!(demo_key_data(0, pact0_key_data.len()), None);
    }

    #[test]
    fn tables_are_in_the_same_order_as_the_c_pointer_tables() {
        assert!(std::ptr::eq(
            TITLE_DEMO_HEAD_TABLES[0].as_ptr(),
            pact0_head_table.as_ptr()
        ));
        assert!(std::ptr::eq(
            TITLE_DEMO_HEAD_TABLES[4].as_ptr(),
            pact4_head_table.as_ptr()
        ));
        assert!(std::ptr::eq(
            TITLE_DEMO_KEY_DATA[0].as_ptr(),
            pact0_key_data.as_ptr()
        ));
        assert!(std::ptr::eq(
            TITLE_DEMO_KEY_DATA[4].as_ptr(),
            pact4_key_data.as_ptr()
        ));
    }
}
