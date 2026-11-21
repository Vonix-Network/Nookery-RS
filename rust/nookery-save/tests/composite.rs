//! Synthetic composite, recovery, and staged-write coverage.

mod support;

use std::fs;
use std::path::PathBuf;

use nookery_save::composite::{
    KEEP_DIARY_SIZE, KEEP_MAIL_SIZE, KEEP_ORIGINAL_SIZE, KEEP_ORIGINAL_TRAILER_OFFSET, MAIL_SIZE,
    SwapDirection, swap_keep_diary, swap_keep_mail, swap_keep_original,
};
use nookery_save::{
    GciError, SaveResult, read_town_record_from_slots, read_town_record_recovering,
    write_town_record,
};
use support::{Options, SaveSpec};

fn test_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "nookery-save-composite-{label}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create isolated synthetic fixture directory");
    path
}

#[test]
fn keep_mail_round_trip_preserves_synthetic_persisted_bytes() {
    let mut bytes = vec![0u8; KEEP_MAIL_SIZE];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(19).wrapping_add(7);
    }
    let original = bytes.clone();
    swap_keep_mail(&mut bytes, SwapDirection::FromBigEndian).expect("synthetic keep-mail BE");
    swap_keep_mail(&mut bytes, SwapDirection::ToBigEndian).expect("synthetic keep-mail LE");
    assert_eq!(bytes, original);
    assert_eq!(MAIL_SIZE, 0x12A);
}

#[test]
fn keep_original_swaps_only_c_oracle_fields() {
    let mut bytes = vec![0u8; KEEP_ORIGINAL_SIZE];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(23).wrapping_add(5);
    }
    let original = bytes.clone();
    let mut expected = original.clone();
    expected[0..2].reverse();
    expected[2..4].reverse();
    expected[KEEP_ORIGINAL_TRAILER_OFFSET..KEEP_ORIGINAL_TRAILER_OFFSET + 4].reverse();

    swap_keep_original(&mut bytes, SwapDirection::FromBigEndian)
        .expect("synthetic keep-original BE");
    assert_eq!(bytes, expected);
    swap_keep_original(&mut bytes, SwapDirection::ToBigEndian).expect("synthetic keep-original LE");
    assert_eq!(bytes, original);
}

#[test]
fn keep_diary_swaps_only_checksum_and_round_trips() {
    let mut bytes = vec![0u8; KEEP_DIARY_SIZE];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(29).wrapping_add(9);
    }
    let original = bytes.clone();
    let mut expected = original.clone();
    expected[0..2].reverse();

    swap_keep_diary(&mut bytes, SwapDirection::FromBigEndian).expect("synthetic diary BE");
    assert_eq!(bytes, expected);
    swap_keep_diary(&mut bytes, SwapDirection::ToBigEndian).expect("synthetic diary LE");
    assert_eq!(bytes, original);
}

#[test]
fn keep_blocks_reject_truncation_and_checksum_mutations() {
    for (size, swap) in [
        (
            KEEP_ORIGINAL_SIZE,
            swap_keep_original as fn(&mut [u8], SwapDirection) -> Result<(), GciError>,
        ),
        (
            KEEP_DIARY_SIZE,
            swap_keep_diary as fn(&mut [u8], SwapDirection) -> Result<(), GciError>,
        ),
    ] {
        let mut short = vec![0u8; size - 1];
        assert_eq!(
            swap(&mut short, SwapDirection::FromBigEndian),
            Err(GciError::Truncated {
                found: size - 1,
                expected: size,
            })
        );

        let mut block = vec![0u8; size];
        for (index, byte) in block.iter_mut().enumerate().skip(2) {
            *byte = (index as u8).wrapping_mul(7).wrapping_add(0x31);
        }
        support::seal(&mut block);
        assert!(nookery_save::verify_block(&block));
        // Parent-replayable invariant mutation: any persisted payload byte
        // changed after sealing must be rejected by the C checksum rule.
        block[size / 2] ^= 0x01;
        assert!(!nookery_save::verify_block(&block));
    }
}

#[test]
fn keep_original_checksum_mutation_rejects_a_complete_record() {
    let mut record = support::valid_record();
    // `support::aux_modern` is relative to the payload; convert directly to
    // the complete-record offset for the original-design block.
    let original = nookery_save::HEADER_SIZE + support::aux_modern().original;
    record[original + 0xCC80] ^= 0x01;
    assert_eq!(
        nookery_save::TownRecord::parse(&record),
        Err(GciError::BadAuxChecksums)
    );
}

#[test]
fn recovery_matches_pending_rotation_and_card_slot_precedence() {
    let dir = test_dir("recovery");
    let base = dir.join("town.gci");
    let valid = support::valid_record();

    fs::write(base.with_extension("gci.write"), &valid).expect("write pending fixture");
    let recovered = read_town_record_recovering(&base).expect("pending fixture loads");
    assert_eq!(recovered.save_copy(), 0);

    let card_a = dir.join("card-a.gci");
    let card_b = dir.join("card-b.gci");
    let invalid = support::build(Options {
        main: SaveSpec::Corrupt,
        backup: SaveSpec::Corrupt,
        ..Options::default()
    });
    fs::write(&card_a, invalid).expect("write invalid card A fixture");
    fs::write(&card_b, &valid).expect("write valid card B fixture");
    let from_slots = read_town_record_from_slots(&card_a, &card_b).expect("card B fallback");
    assert_eq!(from_slots.land_id(), 0x3001);

    fs::remove_dir_all(dir).expect("remove synthetic recovery directory");
}

#[test]
fn recovery_reports_exact_missing_invalid_and_io_categories() {
    let dir = test_dir("outcomes");
    let missing = dir.join("missing.gci");
    assert_eq!(
        read_town_record_recovering(&missing)
            .unwrap_err()
            .as_save_result(),
        SaveResult::Missing
    );

    let invalid = dir.join("invalid.gci");
    let invalid_record = support::build(Options {
        main: SaveSpec::Corrupt,
        backup: SaveSpec::Corrupt,
        ..Options::default()
    });
    fs::write(&invalid, invalid_record).expect("write invalid fixture");
    assert_eq!(
        read_town_record_recovering(&invalid).unwrap_err(),
        GciError::NoValidSaveCopy
    );

    let io_path = dir.join("directory.gci");
    fs::create_dir(&io_path).expect("create unreadable-shape fixture");
    assert!(matches!(
        read_town_record_recovering(&io_path),
        Err(GciError::Io(_))
    ));

    fs::remove_dir_all(dir).expect("remove synthetic outcome directory");
}

#[test]
fn staged_write_round_trips_and_rotates_previous_record() {
    let dir = test_dir("write");
    let base = dir.join("town.gci");
    let valid = support::valid_record();

    write_town_record(&base, &valid).expect("install first synthetic record");
    assert_eq!(
        read_town_record_recovering(&base)
            .expect("first record")
            .land_id(),
        0x3001
    );
    write_town_record(&base, &valid).expect("install second synthetic record");
    assert!(base.with_extension("gci.bak1").is_file());
    assert_eq!(
        read_town_record_recovering(base.with_extension("gci.bak1"))
            .expect("rotated record")
            .land_id(),
        0x3001
    );

    fs::remove_dir_all(dir).expect("remove synthetic write directory");
}
