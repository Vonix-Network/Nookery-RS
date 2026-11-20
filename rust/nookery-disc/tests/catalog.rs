//! Catalog facts from `pc/src/acgc_disc.cpp`.

use nookery_disc::{Catalog, Digest128, UPSTREAM};

#[test]
fn upstream_catalog_has_exactly_one_accepted_disc() {
    let catalog = Catalog::upstream();
    assert_eq!(catalog.accepted_discs.len(), 1);

    let record = &catalog.accepted_discs[0];
    assert_eq!(record.game_id, "GAFE01");
    assert_eq!(record.disc_number, 0);
    assert_eq!(record.revision, 0);
}

#[test]
fn accepted_hash_matches_source_literal() {
    // `parse_xxh3_128` splits the canonical string into `.high64` from the first
    // 16 characters and `.low64` from the last 16.
    let parsed = Digest128::parse("9ffba17f08aa24e8a084ba30e038cef3").expect("literal parses");
    assert_eq!(parsed.high, 0x9ffb_a17f_08aa_24e8);
    assert_eq!(parsed.low, 0xa084_ba30_e038_cef3);
    assert_eq!(parsed.to_string(), "9ffba17f08aa24e8a084ba30e038cef3");

    assert_eq!(UPSTREAM.accepted_discs[0].expected_hash, parsed);
    assert_eq!(UPSTREAM.accepted_hash_text(), parsed.to_string());
}

#[test]
fn recognized_ids_are_the_two_non_english_releases() {
    assert_eq!(UPSTREAM.recognized_game_ids, ["GAFJ01", "GAFP01"]);
    assert_eq!(
        Catalog::upstream().recognized_game_ids,
        ["GAFJ01", "GAFP01"]
    );
}

#[test]
fn accepted_record_is_also_recognized() {
    // `is_recognized_game` checks accepted records first, so a GAFE01 image with
    // a mismatched revision is "recognized" and reports UnsupportedVersion.
    let catalog = Catalog::upstream();
    assert!(
        catalog.recognized_game_ids.iter().all(|id| *id != "GAFE01"),
        "GAFE01 recognition must come from the accepted table, not the extra list"
    );
}

#[test]
fn game_id_matching_is_byte_exact_not_lossy_text() {
    // C++ compares `std::string` bytes, so a non-UTF-8 game ID cannot alias an
    // ASCII record through lossy replacement characters.
    use nookery_disc::{Header, Platform};

    let mut header = Header {
        game_id: *b"GAFE01",
        disc_number: 0,
        revision: 0,
        title: Vec::new(),
        platform: Platform::GameCube,
    };
    let catalog = Catalog::upstream();
    assert_eq!(catalog.accepted_index(&header), Some(0));
    assert_eq!(header.game_id_text(), "GAFE01");

    // 0xFF is not valid UTF-8; lossy text would become U+FFFD, not "GAFE01".
    header.game_id = *b"GAF\xFF01";
    assert_eq!(catalog.accepted_index(&header), None);
    assert!(!catalog.is_recognized_game(&header));
}
