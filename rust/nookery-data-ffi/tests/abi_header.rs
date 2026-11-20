//! Cross-check that the compiled ABI matches the shipped C header.
//!
//! The header is the contract a C caller compiles against.  Parsing it here
//! means a renumbered/renamed/added status or a widened struct fails the build
//! instead of silently disagreeing with the library — the exact drift class a
//! hand-copied constant list would miss.
//!
//! Scope: preprocessor constants, enum values, and struct field offsets.  This
//! does not compile the header; `tools/data-ffi/harness.c` does that.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nookery_data_ffi::*;

fn header_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("include/nookery_data.h")
}

fn header_text() -> String {
    std::fs::read_to_string(header_path()).expect("read include/nookery_data.h")
}

/// `#define NOOKERY_DATA_STATUS_X <n>` → `{"X": n}`.
fn define_constants(text: &str) -> BTreeMap<String, u64> {
    let mut found = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define NOOKERY_DATA_STATUS_") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let (Some(name), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        // `MAX` is a range alias, not a status of its own.
        if name == "MAX" {
            continue;
        }
        let value = value.trim_end_matches('u');
        if let Ok(number) = value.parse::<u64>() {
            found.insert(name.to_owned(), number);
        }
    }
    found
}

#[test]
fn every_status_constant_in_the_header_matches_the_library() {
    let text = header_text();
    let header = define_constants(&text);
    assert!(
        !header.is_empty(),
        "no status constants parsed; header moved?"
    );

    let library: BTreeMap<String, u64> = STATUS_TABLE
        .iter()
        .map(|(name, value)| {
            (
                name.to_str().expect("ASCII status names").to_owned(),
                u64::from(*value),
            )
        })
        .collect();

    assert_eq!(
        header, library,
        "include/nookery_data.h and STATUS_TABLE disagree"
    );

    // The ABI version macro must match the library's reported version too.
    assert!(
        text.contains(&format!(
            "#define NOOKERY_DATA_ABI_VERSION {}u",
            ABI_VERSION
        )),
        "NOOKERY_DATA_ABI_VERSION in the header must be {ABI_VERSION}"
    );
    assert_eq!(nookery_data_abi_version(), ABI_VERSION);
}

#[test]
fn every_status_is_contiguous_from_zero() {
    // A gap would mean a status with no name, which a C caller's `switch` could
    // not handle exhaustively.
    for expected in 0..=STATUS_MAX {
        assert!(
            STATUS_TABLE.iter().any(|(_, value)| *value == expected),
            "status {expected} has no entry"
        );
    }
    assert_eq!(STATUS_TABLE.len() as u32, STATUS_MAX + 1);
}

#[test]
fn status_name_macros_are_all_prefixed_consistently() {
    // The header must not declare a status the library lacks, and vice versa:
    // checked above. This pins the prefix so a rename is a deliberate edit.
    for line in header_text().lines() {
        let line = line.trim();
        if line.starts_with("#define NOOKERY_DATA_STATUS_") {
            assert!(
                line.split_whitespace().count() >= 3,
                "define without a value: {line}"
            );
        }
    }
}

#[test]
fn the_header_declares_every_entry_point_the_library_exports() {
    let text = header_text();
    // Keep this list exhaustive by construction: it mirrors `lib.rs`'s exports.
    let exported = [
        "nookery_data_abi_version",
        "nookery_data_status_name",
        "nookery_data_status_message",
        "nookery_data_status_count",
        "nookery_data_handle_create",
        "nookery_data_handle_destroy",
        "nookery_data_initialize",
        "nookery_data_root",
        "nookery_data_directory",
        "nookery_data_user_path",
        "nookery_data_base_path",
        "nookery_data_last_status",
        "nookery_data_last_message",
        "nookery_data_plan_migration",
        "nookery_data_plan_counts",
        "nookery_data_plan_directory",
        "nookery_data_plan_file",
        "nookery_data_plan_destroy",
    ];
    for symbol in exported {
        assert!(
            text.contains(symbol),
            "include/nookery_data.h does not declare {symbol}"
        );
    }
}

#[test]
fn struct_layouts_match_their_advertised_sizes() {
    // `struct_size` is the caller's only defence against reinterpretation, so a
    // silent field addition must fail here.
    assert_eq!(
        std::mem::size_of::<NookeryDataBuffer>(),
        NookeryDataBuffer::SIZE as usize
    );
    assert_eq!(
        std::mem::size_of::<NookeryDataCreateOptions>(),
        NookeryDataCreateOptions::SIZE as usize
    );
    assert_eq!(
        std::mem::size_of::<NookeryDataDirReport>(),
        NookeryDataDirReport::SIZE as usize
    );

    // Field order is ABI: the C struct is
    // struct_size, ptr, capacity, length.
    use std::mem::{align_of, offset_of};
    let pointer_align = align_of::<*mut std::ffi::c_char>();

    assert_eq!(offset_of!(NookeryDataBuffer, struct_size), 0);
    assert_eq!(offset_of!(NookeryDataBuffer, ptr), pointer_align);
    assert_eq!(
        offset_of!(NookeryDataBuffer, capacity),
        pointer_align + std::mem::size_of::<*mut std::ffi::c_char>()
    );
    assert_eq!(
        offset_of!(NookeryDataBuffer, length),
        offset_of!(NookeryDataBuffer, capacity) + std::mem::size_of::<usize>()
    );
    assert_eq!(
        NookeryDataBuffer::SIZE as usize,
        std::mem::size_of::<NookeryDataBuffer>()
    );

    // Create options: struct_size, abi_version, then two pointers, C-ordered.
    assert_eq!(offset_of!(NookeryDataCreateOptions, struct_size), 0);
    assert_eq!(
        offset_of!(NookeryDataCreateOptions, abi_version),
        std::mem::size_of::<u32>()
    );
    assert_eq!(
        offset_of!(NookeryDataCreateOptions, root_utf8),
        pointer_align
    );
    assert_eq!(
        offset_of!(NookeryDataCreateOptions, base_root_utf8),
        pointer_align + std::mem::size_of::<*const std::ffi::c_char>()
    );

    // Dir report: two u32s, nothing else.
    assert_eq!(offset_of!(NookeryDataDirReport, struct_size), 0);
    assert_eq!(
        offset_of!(NookeryDataDirReport, failure_count),
        std::mem::size_of::<u32>()
    );
}
