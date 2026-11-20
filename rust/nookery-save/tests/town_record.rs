//! End-to-end tests over the public API, using hand-built fixtures only.

mod support;

use nookery_save::layout::{
    AUX_START, KEEP_DIARY_SIZE, KEEP_MAIL_SIZE, KEEP_ORIGINAL_SIZE, OTHERS_SIZE,
};
use nookery_save::{
    AuxBlock, FILE_DATA_SIZE, GciError, HEADER_SIZE, RECORD_SIZE, SAVE_BACK_OFFSET,
    SAVE_MAIN_OFFSET, SAVE_SIZE, TOWN_BLOCK_COUNT, TOWN_FILE_NAME, TownRecord, is_gci_file,
    is_town_file, read_town_record,
};
use support::{AuxOrder, Options, SaveSpec, VALID_BACKUP};

#[test]
fn rejects_wrong_save_code_and_non_gaf_game_name() {
    // mFRm_SAVE_ID gate: every other field valid, only `code` wrong.
    let mut record = support::valid_record();
    let base = HEADER_SIZE + SAVE_MAIN_OFFSET;
    record[base + 0x04..base + 0x08].copy_from_slice(b"XXXX");
    let mut block = record[base..base + SAVE_SIZE].to_vec();
    support::seal_at(&mut block, support::SAVE_CHECKSUM_OFFSET);
    record[base..base + SAVE_SIZE].copy_from_slice(&block);
    assert_eq!(TownRecord::parse(&record), Err(GciError::NoValidSaveCopy));

    // Game name must start with "GAF" even when the file name matches.
    let mut record = support::valid_record();
    record[0x00..0x04].copy_from_slice(b"GAME");
    record[0x04] = b'C';
    assert_eq!(TownRecord::parse(&record), Err(GciError::FileNameMismatch));
}

#[test]
fn accepts_minimal_valid_record_and_exposes_validated_blocks() {
    let record = support::valid_record();
    support::assert_record_size(&record);

    let parsed = TownRecord::parse(&record).expect("fixture must validate");
    assert_eq!(parsed.save_copy(), 0);
    assert_eq!(parsed.land_id(), 0x3001);
    assert_eq!(parsed.dir().file_name(), TOWN_FILE_NAME);
    assert_eq!(parsed.dir().block_count(), TOWN_BLOCK_COUNT);
    assert_eq!(parsed.save_block().len(), SAVE_SIZE);
    assert_eq!(&parsed.save_block()[0x04..0x08], b"GAFE");
    assert!(nookery_save::verify_block(parsed.save_block()));

    let aux = parsed.aux_layout();
    assert!(!aux.legacy_order);
    assert_eq!(aux.mail, AUX_START);
    assert_eq!(aux.original, AUX_START + KEEP_MAIL_SIZE);
    assert_eq!(aux.diary, AUX_START + KEEP_MAIL_SIZE + KEEP_ORIGINAL_SIZE);
    let original = parsed.aux_block(AuxBlock::Original);
    assert_eq!(original.len(), KEEP_ORIGINAL_SIZE);
    assert!(nookery_save::verify_block(original));
    assert!(nookery_save::verify_block(parsed.aux_block(AuxBlock::Mail)));
    let diary = parsed.aux_block(AuxBlock::Diary);
    assert_eq!(diary.len(), KEEP_DIARY_SIZE);
    assert!(nookery_save::verify_block(diary));

    // Blocks are handed back byte-identical to the input, not re-encoded.
    assert_eq!(parsed.payload(), &record[HEADER_SIZE..]);
    assert_eq!(
        parsed.save_block(),
        &record[HEADER_SIZE + SAVE_MAIN_OFFSET..HEADER_SIZE + SAVE_MAIN_OFFSET + SAVE_SIZE]
    );
    assert!(aux.diary + KEEP_DIARY_SIZE <= OTHERS_SIZE);
}

#[test]
fn falls_back_to_embedded_backup_when_main_is_damaged() {
    let record = support::build(Options {
        main: SaveSpec::Corrupt,
        backup: VALID_BACKUP,
        ..Options::default()
    });
    let parsed = TownRecord::parse(&record).expect("backup must be accepted");
    assert_eq!(parsed.save_copy(), 1);
    assert_eq!(
        parsed.save_block(),
        &record[HEADER_SIZE + SAVE_BACK_OFFSET..HEADER_SIZE + SAVE_BACK_OFFSET + SAVE_SIZE]
    );
}

#[test]
fn accepts_legacy_auxiliary_order() {
    let record = support::build(Options {
        aux: AuxOrder::Legacy,
        ..Options::default()
    });
    let parsed = TownRecord::parse(&record).expect("legacy order must be detected");
    let aux = parsed.aux_layout();
    assert!(aux.legacy_order);
    assert_eq!(aux.original, AUX_START);
    assert_eq!(aux.mail, AUX_START + KEEP_ORIGINAL_SIZE);
    assert!(nookery_save::verify_block(
        parsed.aux_block(AuxBlock::Original)
    ));
    assert!(nookery_save::verify_block(parsed.aux_block(AuxBlock::Mail)));
}

#[test]
fn accepts_both_known_save_versions() {
    for version in [5, 6] {
        let record = support::build(Options {
            main: SaveSpec::Valid { version },
            ..Options::default()
        });
        let parsed = TownRecord::parse(&record).unwrap_or_else(|err| {
            panic!("version {version} must be accepted: {err}");
        });
        assert_eq!(parsed.save_copy(), 0);
    }
}

#[test]
fn rejects_unknown_save_version_and_wrong_ids() {
    // Version 4 and 7 exist upstream but are not accepted by select_copy.
    for version in [0, 4, 7] {
        let record = support::build(Options {
            main: SaveSpec::Valid { version },
            ..Options::default()
        });
        let err = TownRecord::parse(&record).expect_err("version must be rejected");
        assert_eq!(err, GciError::NoValidSaveCopy);
        assert!(err.is_invalid());
    }

    // Land id must satisfy mLd_CHECK_LAND_ID and match land_info.
    for land_id in [0x2001u16, 0x31FF] {
        let record = support::build(Options {
            main: SaveSpec::Valid { version: 6 },
            ..Options::default()
        });
        // Rewrite both id fields consistently, then reseal: the record must
        // still be rejected because the bitmask test fails.
        let mut record = record;
        for copy in 0..2 {
            let base = HEADER_SIZE + SAVE_MAIN_OFFSET + copy * SAVE_SIZE;
            record[base + 0x08..base + 0x0A].copy_from_slice(&land_id.to_be_bytes());
            record[base + 0x912A..base + 0x912C].copy_from_slice(&land_id.to_be_bytes());
        }
        let main = HEADER_SIZE + SAVE_MAIN_OFFSET;
        let tail = main..main + SAVE_SIZE;
        let mut block = record[tail.clone()].to_vec();
        support::seal_at(&mut block, support::SAVE_CHECKSUM_OFFSET);
        record[tail].copy_from_slice(&block);
        assert_eq!(
            TownRecord::parse(&record),
            Err(GciError::NoValidSaveCopy),
            "land id {land_id:#06x} must fail the bitmask gate"
        );
    }
}

#[test]
fn rejects_mismatched_land_id_between_check_and_land_info() {
    let mut record = support::valid_record();
    let base = HEADER_SIZE + SAVE_MAIN_OFFSET;
    record[base + 0x912A..base + 0x912C].copy_from_slice(&0x3002u16.to_be_bytes());
    let mut block = record[base..base + SAVE_SIZE].to_vec();
    support::seal_at(&mut block, support::SAVE_CHECKSUM_OFFSET);
    record[base..base + SAVE_SIZE].copy_from_slice(&block);
    assert_eq!(TownRecord::parse(&record), Err(GciError::NoValidSaveCopy));
}

#[test]
fn rejects_damaged_auxiliary_blocks() {
    let record = support::build(Options {
        corrupt_original: true,
        ..Options::default()
    });
    let err = TownRecord::parse(&record).expect_err("damaged aux data must be refused");
    assert_eq!(err, GciError::BadAuxChecksums);
    assert_eq!(err.as_save_result(), nookery_save::SaveResult::Invalid);
}

#[test]
fn rejects_foreign_file_name_and_block_count() {
    let record = support::build(Options {
        file_name: b"CUSTOM_MURA".to_vec(),
        ..Options::default()
    });
    assert_eq!(TownRecord::parse(&record), Err(GciError::FileNameMismatch));

    // Prefix-only name is not a match; upstream compares length + 1 bytes.
    let record = support::build(Options {
        file_name: b"DobutsunomoriP".to_vec(),
        ..Options::default()
    });
    assert_eq!(TownRecord::parse(&record), Err(GciError::FileNameMismatch));

    for blocks in [0u16, 1, TOWN_BLOCK_COUNT - 1, TOWN_BLOCK_COUNT + 1] {
        let record = support::build(Options {
            block_count: blocks,
            ..Options::default()
        });
        assert_eq!(
            TownRecord::parse(&record),
            Err(GciError::BadBlockCount {
                found: blocks,
                expected: TOWN_BLOCK_COUNT
            })
        );
    }
}

#[test]
fn rejects_truncated_headers_and_payloads() {
    let record = support::valid_record();

    for len in [0usize, 1, HEADER_SIZE - 1] {
        assert_eq!(
            TownRecord::parse(&record[..len]),
            Err(GciError::TruncatedHeader { available: len })
        );
    }

    // One byte short of a complete payload.
    let short = RECORD_SIZE - 1;
    assert_eq!(
        TownRecord::parse(&record[..short]),
        Err(GciError::TruncatedPayload {
            found: short - HEADER_SIZE,
            expected: FILE_DATA_SIZE,
        })
    );

    // Header only: payload is entirely missing.
    assert_eq!(
        TownRecord::parse(&record[..HEADER_SIZE]),
        Err(GciError::TruncatedPayload {
            found: 0,
            expected: FILE_DATA_SIZE,
        })
    );
}

#[test]
fn rejects_oversized_input_without_reading_it() {
    let mut record = support::valid_record();
    record.push(0);
    assert_eq!(
        TownRecord::parse(&record),
        Err(GciError::Oversized {
            found: RECORD_SIZE + 1,
            expected: RECORD_SIZE,
        })
    );

    // The prefix parser mirrors upstream, which ignores trailing bytes.
    assert!(TownRecord::parse_prefix(&record).is_ok());
}

#[test]
fn prefix_parser_still_requires_full_payload_and_header() {
    let record = support::valid_record();
    assert_eq!(
        TownRecord::parse_prefix(&record[..HEADER_SIZE - 1]),
        Err(GciError::TruncatedHeader {
            available: HEADER_SIZE - 1
        })
    );
    assert!(matches!(
        TownRecord::parse_prefix(&record[..RECORD_SIZE - 2]),
        Err(GciError::TruncatedPayload { .. })
    ));
}

#[test]
fn rejects_empty_payload_variant() {
    // Even with a correct header, a zero payload cannot satisfy the gate.
    let record = support::build(Options {
        payload_len: 0,
        ..Options::default()
    });
    assert_eq!(
        TownRecord::parse(&record),
        Err(GciError::TruncatedPayload {
            found: 0,
            expected: FILE_DATA_SIZE,
        })
    );
}

#[test]
fn file_helpers_bound_their_reads() {
    let dir = std::env::temp_dir().join(format!(
        "nookery-save-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let valid = dir.join("town.gci");
    std::fs::write(&valid, support::valid_record()).unwrap();
    assert!(is_town_file(&valid));
    assert!(is_gci_file(&valid));
    let parsed = read_town_record(&valid).expect("file must validate");
    assert_eq!(parsed.land_id(), 0x3001);
    assert!(nookery_save::read_card_dir(&valid).is_ok());

    let missing = dir.join("absent.gci");
    assert_eq!(read_town_record(&missing), Err(GciError::Missing));
    assert!(!is_town_file(&missing));
    assert!(!is_gci_file(&missing));

    // Oversized file: rejected from metadata, never read into memory.
    let oversized = dir.join("oversized.gci");
    let mut bytes = support::valid_record();
    bytes.extend_from_slice(&[0u8; 64]);
    std::fs::write(&oversized, &bytes).unwrap();
    assert_eq!(
        read_town_record(&oversized),
        Err(GciError::Oversized {
            found: RECORD_SIZE + 64,
            expected: RECORD_SIZE,
        })
    );

    // Truncated file on disk.
    let truncated = dir.join("truncated.gci");
    std::fs::write(&truncated, &support::valid_record()[..0x20]).unwrap();
    assert_eq!(
        read_town_record(&truncated),
        Err(GciError::TruncatedHeader { available: 0x20 })
    );
    assert!(!is_town_file(&truncated));
    assert!(!is_gci_file(&truncated));

    // Non-GCI file that is big enough.
    let foreign = dir.join("foreign.bin");
    std::fs::write(&foreign, vec![0u8; HEADER_SIZE + FILE_DATA_SIZE]).unwrap();
    assert!(!is_gci_file(&foreign));
    assert!(!is_town_file(&foreign));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn parsing_is_panic_free_across_mutations() {
    // Deterministic single-byte mutations must never panic; they may parse or
    // be rejected, but the validator must always answer.
    let base = support::valid_record();
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for offset in (0..RECORD_SIZE).step_by(0x7DF) {
        for mask in [0xFFu8, 0x01] {
            let mut record = base.clone();
            record[offset] ^= mask;
            match TownRecord::parse(&record) {
                Ok(parsed) => {
                    assert_eq!(parsed.payload().len(), FILE_DATA_SIZE);
                    accepted += 1;
                }
                Err(err) => {
                    assert_eq!(err.as_save_result(), nookery_save::SaveResult::Invalid);
                    rejected += 1;
                }
            }
        }
    }
    assert!(accepted > 0, "baseline mutations must still parse");
    assert!(rejected > 0, "some mutations must be rejected");
}

#[test]
fn every_prefix_length_is_handled() {
    // Truncation at every boundary the crate can be handed.
    let record = support::valid_record();
    for len in (0..RECORD_SIZE).step_by(0x7DF) {
        assert!(TownRecord::parse(&record[..len]).is_err(), "len {len}");
    }
    assert!(TownRecord::parse(&record).is_ok());
}
