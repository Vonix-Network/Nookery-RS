//! Synthetic composite, recovery, and staged-write coverage.

mod support;

use std::fs;
use std::path::PathBuf;

use nookery_save::composite::{
    KEEP_DIARY_SIZE, KEEP_MAIL_SIZE, KEEP_ORIGINAL_SIZE, KEEP_ORIGINAL_TRAILER_OFFSET, MAIL_SIZE,
    SAVE_PRIVATE_OFFSET, SwapDirection, swap_keep_diary, swap_keep_mail, swap_keep_original,
    swap_save_t_legacy, swap_save_t_verified,
};
use nookery_save::{
    GciError, SaveResult, decode_gci_composite, encode_gci_composite, read_town_record_from_slots,
    read_town_record_recovering, write_town_record,
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
fn full_save_walker_hits_source_anchored_families_and_round_trips() {
    let mut bytes = vec![0u8; nookery_save::SAVE_SIZE];
    let be16 = |at: usize, value: u16, data: &mut [u8]| {
        data[at..at + 2].copy_from_slice(&value.to_be_bytes());
    };
    let be32 = |at: usize, value: u32, data: &mut [u8]| {
        data[at..at + 4].copy_from_slice(&value.to_be_bytes());
    };

    be16(0x10, 0x1234, &mut bytes); // save-check player-independent time year
    be32(0x14, 0x0102_0304, &mut bytes); // scene_no
    be16(0x1A, 0xA1B2, &mut bytes); // copy protection
    be16(SAVE_PRIVATE_OFFSET + 0x10, 0x3344, &mut bytes);
    be16(SAVE_PRIVATE_OFFSET + 0x94 + 0x08, 0x5566, &mut bytes);
    be16(0x912A, 0x3001, &mut bytes);
    be16(0x9CE8 + 0x10, 0x7788, &mut bytes);
    be16(0x137A8, 0x99AA, &mut bytes);
    be16(0x173A8, 0xC123, &mut bytes);
    be16(0x17438, 0xBEEF, &mut bytes);
    be16(0x20340 + 0xCE, 0xCAFE, &mut bytes);
    be16(0x20694 + 8 + 0x2C, 0xD00D, &mut bytes);
    be16(0x20498 + 6, 0x1357, &mut bytes);
    be16(0x20498 + 0x50 + 0x10, 0x2468, &mut bytes);
    be16(0x20554 + 4, 0x369C, &mut bytes);
    be16(0x22540 + 0x12, 0x3002, &mut bytes);
    be16(0x23E68 + 0x10, 0xABCD, &mut bytes);
    be32(0x23E68 + 0x1C, 0x1020_3040, &mut bytes);
    be16(0x23F20 + 0x229, 0x5678, &mut bytes);
    be16(0x2416C, 0x4567, &mut bytes);
    be32(0x24180, 0x5060_7080, &mut bytes);
    let original = bytes.clone();

    swap_save_t_verified(&mut bytes, SwapDirection::FromBigEndian).expect("full source walker");
    assert_eq!(&bytes[0x14..0x18], &0x0102_0304u32.to_le_bytes());
    assert_eq!(
        &bytes[0x9CE8 + 0x10..0x9CE8 + 0x12],
        &0x7788u16.to_le_bytes()
    );
    assert_eq!(
        &bytes[0x20340 + 0xCE..0x20340 + 0xD0],
        &0xCAFEu16.to_le_bytes()
    );
    assert_eq!(
        &bytes[0x20694 + 8 + 0x2C..0x20694 + 8 + 0x2E],
        &0xD00Du16.to_le_bytes()
    );
    assert_eq!(
        &bytes[0x23E68 + 0x1C..0x23E68 + 0x20],
        &0x1020_3040u32.to_le_bytes()
    );
    assert_eq!(
        &bytes[0x23F20 + 0x229..0x23F20 + 0x22B],
        &0x5678u16.to_le_bytes()
    );
    swap_save_t_verified(&mut bytes, SwapDirection::ToBigEndian).expect("full source restore");
    assert_eq!(bytes, original);
}

/// `swap_mHm_lyr` swaps `UT_Z_NUM * UT_X_NUM` (16 * 16) furniture `u16`
/// slots, i.e. the whole `items[0][0..0x200]` span of `mHm_lyr_c`, not just
/// the first 64. A slot above index 63 must therefore convert in both
/// directions while the surrounding structure bytes stay byte-exact.
#[test]
fn home_layer_furniture_converts_all_256_ut_slots_and_keeps_neighbours() {
    // `mHm_hs_c` #0 is at 0x9CE8; `floors[0]` at +0x38; `layer_main` at +0x000.
    const HOME: usize = 0x9CE8;
    const LAYER: usize = HOME + 0x38;
    const LAYER_SIZE: usize = 0x228;

    assert_eq!(nookery_save::composite::HOME_LAYER_ITEM_COUNT, 256);

    let mut bytes = vec![0u8; nookery_save::SAVE_SIZE];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(41).wrapping_add(0x5D);
    }

    // Slot 64 is the first slot the pre-fix 64-item walk skipped; 255 is the
    // last `u16` of the `items[16][16]` array.
    let seeded = [
        (0usize, 0x1234u16),
        (63, 0x00FF),
        (64, 0xABCD),
        (100, 0x5A5A),
        (255, 0xFE01),
    ];
    for (slot, value) in seeded {
        let at = LAYER + slot * 2;
        bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    // Neighbouring `mHm_lyr_c` fields the oracle also swaps.
    bytes[LAYER + 0x200..LAYER + 0x208].copy_from_slice(&0x0102_0304_0506_0708u64.to_be_bytes());
    for index in 0..8usize {
        let at = LAYER + 0x208 + index * 4;
        bytes[at..at + 4].copy_from_slice(&(0x1111_0000u32 + index as u32).to_be_bytes());
    }
    let original = bytes.clone();

    swap_save_t_verified(&mut bytes, SwapDirection::FromBigEndian).expect("BE home save");

    for (slot, value) in seeded {
        let at = LAYER + slot * 2;
        assert_eq!(
            &bytes[at..at + 2],
            &value.to_le_bytes(),
            "home layer item slot {slot} must convert"
        );
    }
    let mut ftr_switch = 0x0102_0304_0506_0708u64.to_be_bytes();
    ftr_switch.reverse();
    assert_eq!(&bytes[LAYER + 0x200..LAYER + 0x208], &ftr_switch[..]);
    for index in 0..8usize {
        let at = LAYER + 0x208 + index * 4;
        assert_eq!(
            &bytes[at..at + 4],
            &(0x1111_0000u32 + index as u32).to_le_bytes(),
            "haniwa_step[{index}] must convert"
        );
    }

    // Second layer's first slot is a different `mHm_lyr_c`; it converts too,
    // which pins the layer stride at `sizeof(mHm_lyr_c)`.
    let next_layer = LAYER + LAYER_SIZE;
    assert_eq!(
        &bytes[next_layer..next_layer + 2],
        &[original[next_layer + 1], original[next_layer]]
    );

    // Unswapped home bytes adjacent to the walk: `unk_14`/`haniwa_tempo`
    // (0x14..0x1C) and the palette/`unk_30` span (0x2C..0x38).
    assert_eq!(
        &bytes[HOME + 0x14..HOME + 0x1C],
        &original[HOME + 0x14..HOME + 0x1C]
    );
    assert_eq!(&bytes[HOME + 0x2C..LAYER], &original[HOME + 0x2C..LAYER]);

    swap_save_t_verified(&mut bytes, SwapDirection::ToBigEndian).expect("LE home save");
    assert_eq!(bytes, original);
}

#[test]
fn legacy_save_walker_uses_legacy_union_and_post_office_order() {
    let errand_id = SAVE_PRIVATE_OFFSET + 0x2EC + 0x2C;
    let post_flags = 0x20694 + 0x830;
    let mut legacy = vec![0u8; nookery_save::SAVE_SIZE];
    legacy[errand_id..errand_id + 2].copy_from_slice(&0x1234u16.to_be_bytes());
    legacy[post_flags..post_flags + 4].copy_from_slice(&0x1122_3344u32.to_be_bytes());
    swap_save_t_legacy(&mut legacy, SwapDirection::FromBigEndian).expect("legacy decode");
    assert_eq!(&legacy[errand_id..errand_id + 2], &0x1234u16.to_le_bytes());
    assert_eq!(
        &legacy[post_flags..post_flags + 4],
        &0x1122_3344u32.to_le_bytes()
    );
}

#[test]
fn quest_deadline_relocation_matches_c_pre_swap_guard() {
    let base = SAVE_PRIVATE_OFFSET + 0x94;
    let mut bytes = vec![0u8; nookery_save::SAVE_SIZE];
    // Invalid bytes at the old base location, followed by a valid BE RTC at
    // the location used by the historical PC layout.
    bytes[base + 2..base + 10].copy_from_slice(&[0xFF, 0, 0, 0, 0, 0, 0, 0]);
    bytes[base + 4..base + 12].copy_from_slice(&[0, 0, 12, 3, 1, 1, 0x07, 0xD5]);
    nookery_save::relocate_quest_time_limits(&mut bytes).expect("relocate quest deadline");
    assert_eq!(
        &bytes[base + 2..base + 10],
        &[0, 0, 12, 3, 1, 1, 0x07, 0xD5]
    );
    assert_eq!(&bytes[base + 10..base + 12], &[0, 0]);
}

#[test]
fn decoded_composite_swaps_source_offsets_and_restores_persisted_bytes() {
    let mut persisted = support::valid_record();
    let save = nookery_save::HEADER_SIZE + nookery_save::SAVE_MAIN_OFFSET;
    let delivery = save + 0x20 + 0x94;
    persisted[delivery..delivery + 2].copy_from_slice(&0xA501u16.to_be_bytes());
    persisted[delivery + 0x08..delivery + 0x0A].copy_from_slice(&0x1234u16.to_be_bytes());
    persisted[delivery + 0x0C..delivery + 0x0E].copy_from_slice(&0x5678u16.to_be_bytes());
    let mut save_block = persisted[save..save + nookery_save::SAVE_SIZE].to_vec();
    support::seal_at(&mut save_block, support::SAVE_CHECKSUM_OFFSET);
    persisted[save..save + nookery_save::SAVE_SIZE].copy_from_slice(&save_block);

    let mut decoded = persisted.clone();
    let selection = decode_gci_composite(&mut decoded).expect("valid GCI composite");

    assert_eq!(selection.save_copy, 0);
    assert!(!selection.aux.legacy_order);
    assert_eq!(&decoded[save..save + 4], &6i32.to_le_bytes());
    assert_eq!(
        &decoded[delivery + 0x08..delivery + 0x0A],
        &0x1234u16.to_le_bytes()
    );
    assert_eq!(
        &decoded[delivery + 0x0C..delivery + 0x0E],
        &0x5678u16.to_le_bytes()
    );
    let mail = nookery_save::HEADER_SIZE + support::aux_modern().mail;
    assert_eq!(
        &decoded[mail + 2..mail + 4],
        &support::LAND_ID.to_le_bytes()
    );

    encode_gci_composite(&mut decoded, selection).expect("restore card representation");
    assert_eq!(decoded, persisted);
}

#[test]
fn composite_selects_backup_and_legacy_aram_order() {
    let mut backup = support::build(Options {
        main: SaveSpec::Corrupt,
        backup: SaveSpec::Valid { version: 5 },
        ..Options::default()
    });
    let selection = decode_gci_composite(&mut backup).expect("backup fallback");
    assert_eq!(selection.save_copy, 1);
    encode_gci_composite(&mut backup, selection).expect("restore backup fixture");

    let persisted = support::build(Options {
        aux: support::AuxOrder::Legacy,
        ..Options::default()
    });
    let mut decoded = persisted.clone();
    let selection = decode_gci_composite(&mut decoded).expect("legacy auxiliary order");
    assert!(selection.aux.legacy_order);
    encode_gci_composite(&mut decoded, selection).expect("restore legacy fixture");
    assert_eq!(decoded, persisted);
}

#[test]
fn composite_failures_are_non_mutating_and_replay_restores_exact_candidate() {
    let mut truncated = support::valid_record();
    truncated.truncate(nookery_save::RECORD_SIZE - 1);
    let before = truncated.clone();
    assert!(matches!(
        decode_gci_composite(&mut truncated),
        Err(GciError::TruncatedPayload { .. })
    ));
    assert_eq!(truncated, before);

    let mut wrong_checksum = support::valid_record();
    let mutation = nookery_save::HEADER_SIZE + support::aux_modern().original + 0x40;
    wrong_checksum[mutation] ^= 0x01;
    let mutated = wrong_checksum.clone();
    assert_eq!(
        decode_gci_composite(&mut wrong_checksum),
        Err(GciError::BadAuxChecksums)
    );
    assert_eq!(wrong_checksum, mutated);

    // Mutation replay: restore exactly the changed byte, then the same
    // candidate loads and can be restored to its persisted bytes.
    wrong_checksum[mutation] ^= 0x01;
    let selection = decode_gci_composite(&mut wrong_checksum).expect("restored candidate");
    encode_gci_composite(&mut wrong_checksum, selection).expect("restored candidate round-trip");
    assert_eq!(wrong_checksum, support::valid_record());
}

#[test]
fn composite_rejects_wrong_save_checksum_and_directory_size() {
    let mut bad_save = support::valid_record();
    let save_checksum =
        nookery_save::HEADER_SIZE + nookery_save::SAVE_MAIN_OFFSET + support::SAVE_CHECKSUM_OFFSET;
    bad_save[save_checksum] ^= 0x01;
    let before = bad_save.clone();
    assert_eq!(
        decode_gci_composite(&mut bad_save),
        Err(GciError::NoValidSaveCopy)
    );
    assert_eq!(bad_save, before);

    let mut bad_size = support::build(Options {
        block_count: nookery_save::TOWN_BLOCK_COUNT - 1,
        ..Options::default()
    });
    assert_eq!(
        decode_gci_composite(&mut bad_size),
        Err(GciError::BadBlockCount {
            found: nookery_save::TOWN_BLOCK_COUNT - 1,
            expected: nookery_save::TOWN_BLOCK_COUNT,
        })
    );
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
