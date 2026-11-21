//! Synthetic temp-tree coverage for the PC card-manager parity layer.

mod support;

use std::fs;
use std::path::PathBuf;

use nookery_save::manager::{
    AramBlocks, BIRTHDAY_ACTIVE_FLAG, CARD_SLOT_B, CardManager, EMPTY_ITEM, PreWriteState,
    PrivateState, SavePaths, WISP_FIRST, aram_offsets, pc_card_b_town_base, pc_card_count_town,
    pc_card_scan_towns, pc_fix_zeroed_original_entries, pc_get_land_copy_protect,
    pc_save_find_and_load, pc_save_migrate_legacy, pc_save_write_gci_to,
};
use nookery_save::{GciError, SaveResult, TownRecord};
use support::{Options, SaveSpec};

fn temp_tree(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "nookery-save-card-manager-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create synthetic manager tree");
    path
}

fn named_record(name: &[u8; 8]) -> Vec<u8> {
    let mut record = support::valid_record();
    let save = nookery_save::HEADER_SIZE + nookery_save::SAVE_MAIN_OFFSET;
    record[save + 0x9120..save + 0x9128].copy_from_slice(name);
    let mut block = record[save..save + nookery_save::SAVE_SIZE].to_vec();
    support::seal_at(&mut block, support::SAVE_CHECKSUM_OFFSET);
    record[save..save + nookery_save::SAVE_SIZE].copy_from_slice(&block);
    record
}

#[test]
fn load_order_migrates_only_after_primary_is_missing() {
    let root = temp_tree("load-order");
    let working = root.join("working");
    let base = root.join("base");
    let paths = SavePaths::new(&root, &working, &base).expect("paths fit");
    fs::create_dir_all(working.join("save/card_a")).expect("working card A");
    let legacy = working
        .join("save/card_a")
        .join(nookery_save::PC_GCI_FILENAME);
    fs::write(&legacy, support::valid_record()).expect("legacy record");

    let loaded = pc_save_find_and_load(&paths).expect("working legacy must load");
    assert_eq!(loaded.land_id(), 0x3001);
    assert!(
        paths.gci_path.is_file(),
        "import destination is active Card A"
    );
    assert!(legacy.is_file(), "migration retains its source");

    let mut invalid = support::build(Options {
        main: SaveSpec::Corrupt,
        backup: SaveSpec::Corrupt,
        ..Options::default()
    });
    fs::write(&paths.gci_path, &invalid).expect("replace with invalid primary");
    invalid.shrink_to_fit();
    assert_eq!(
        pc_save_find_and_load(&paths).unwrap_err().as_save_result(),
        SaveResult::Invalid,
        "an invalid primary stops the fallback ordering"
    );
    fs::remove_dir_all(root).expect("remove synthetic manager tree");
}

#[test]
fn card_b_keeps_damaged_entries_and_excludes_current_town() {
    let root = temp_tree("card-b");
    let paths = SavePaths::new(&root, root.join("cwd"), root.join("base")).expect("paths fit");
    fs::create_dir_all(&paths.card_b_dir).expect("card B");
    fs::write(
        paths.card_b_dir.join("z-town.gci"),
        named_record(b"ZED     "),
    )
    .expect("good town");
    fs::write(paths.card_b_dir.join("a-damaged.gci"), {
        support::build(Options {
            main: SaveSpec::Corrupt,
            backup: SaveSpec::Corrupt,
            ..Options::default()
        })
    })
    .expect("damaged town");
    fs::write(
        paths.card_b_dir.join("m-current.gci"),
        named_record(b"CURRENT "),
    )
    .expect("current town");

    let mut manager = CardManager::new(paths.clone());
    manager.current_town = Some(nookery_save::TownIdentity {
        name: *b"CURRENT ",
        id: 0x3001,
    });
    assert_eq!(manager.pc_card_b_refresh_towns(false), 1);
    assert_eq!(manager.pc_card_b_town_count(), 2);
    assert!(!manager.pc_card_b_town(0).expect("first entry").usable);
    assert_eq!(
        manager.pc_card_b_town(1).expect("second entry").name,
        *b"ZED     "
    );
    assert_eq!(manager.pc_card_b_scan_town_files(), 3);
    assert!(manager.pc_card_b_select_town(1, false));
    assert_eq!(
        manager.pc_card_b_town_base(),
        Some(paths.card_b_dir.join("z-town"))
    );
    assert!(!manager.pc_card_b_select_town(0, false));
    assert!(!manager.pc_card_b_select_town(99, false));
    assert_eq!(pc_card_count_town(&paths, CARD_SLOT_B), 3);
    assert_eq!(pc_card_scan_towns(&paths, CARD_SLOT_B, false).len(), 3);
    assert_eq!(
        pc_card_b_town_base(&paths.card_b_dir.join("z-town.gci")),
        Some(paths.card_b_dir.join("z-town"))
    );
    fs::remove_dir_all(root).expect("remove synthetic manager tree");
}

#[test]
fn aram_initialization_fix_and_copy_follow_c_indices() {
    let mut aram = AramBlocks::new();
    assert_eq!(aram.block(0).expect("original")[4], b' ');
    assert_eq!(
        &aram.block(0).expect("original")[0x64..0x74],
        b"blank           "
    );
    assert_eq!(aram.block(0).expect("original")[0x84], 0xFF);
    assert_eq!(aram.block(1).expect("mail")[0x64 + 0x14], 0xFF);
    assert_eq!(aram.block(1).expect("mail")[0x64 + 0x2E], 0xFF);
    assert_eq!(aram.block(2).expect("diary")[2], b' ');
    assert_eq!(
        aram_offsets(false),
        [
            nookery_save::layout::AUX_START + nookery_save::layout::KEEP_MAIL_SIZE,
            nookery_save::layout::AUX_START,
            nookery_save::layout::AUX_START
                + nookery_save::layout::KEEP_MAIL_SIZE
                + nookery_save::layout::KEEP_ORIGINAL_SIZE,
        ]
    );

    let source = vec![0xA5; 32];
    assert!(aram.mCD_save_data_main_to_aram(&source, source.len(), 99));
    let mut destination = vec![0; 40];
    let destination_len = destination.len();
    assert!(aram.mCD_save_data_aram_to_main(&mut destination, destination_len, 0));
    assert_eq!(&destination[..32], &source);

    let mut zeroed = vec![0; nookery_save::layout::KEEP_ORIGINAL_SIZE];
    assert!(pc_fix_zeroed_original_entries(&mut zeroed, 0x3001));
    assert_eq!(&zeroed[2..4], &0x3001u16.to_ne_bytes());
    assert!(zeroed.iter().any(|byte| *byte != 0));
    assert!(!pc_fix_zeroed_original_entries(&mut zeroed, 0x3002));
}

#[test]
fn reset_prewrite_and_write_gates_match_c_side_effects() {
    let mut birthday = PrivateState {
        state_flags: BIRTHDAY_ACTIVE_FLAG,
        reset_code: 99,
        ..PrivateState::default()
    };
    assert!(nookery_save::pc_check_reset_code(&mut birthday));
    assert_eq!(birthday.reset_code, 0);
    assert_eq!(pc_get_land_copy_protect(0), 1);
    assert_eq!(pc_get_land_copy_protect(0xFFEE), 0xFFEF);

    let mut state = PreWriteState {
        private: Some(PrivateState {
            reset_code: 7,
            pockets: [WISP_FIRST, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
            ..PrivateState::default()
        }),
        ..PreWriteState::default()
    };
    nookery_save::pc_save_pre_write_side_effects(&mut state, 0, 3, 4, 123);
    let private = state.private.as_ref().expect("private state");
    assert_eq!(private.reset_code, 0);
    assert_eq!(private.pockets[0], EMPTY_ITEM);
    assert!(state.money_stone_cleared);
    assert_eq!(state.copy_protect, 5);
    assert_eq!(state.travel_hard_time, 123);

    let root = temp_tree("write-gate");
    let paths = SavePaths::new(&root, root.join("cwd"), root.join("base")).expect("paths fit");
    let record = support::valid_record();
    let mut manager = CardManager::new(paths.clone());
    assert_eq!(
        pc_save_write_gci_to(&mut manager, &paths.gci_path, &record),
        Err(nookery_save::CardManagerError::NotReady)
    );
    manager.save_ready = true;
    manager.load_status = SaveResult::Missing;
    pc_save_write_gci_to(&mut manager, &paths.gci_path, &record).expect("ready manager writes");
    assert_eq!(
        TownRecord::parse(&fs::read(&paths.gci_path).expect("written record"))
            .expect("written record")
            .land_id(),
        0x3001
    );
    let _ = manager;
    fs::remove_dir_all(root).expect("remove synthetic manager tree");
}

#[test]
fn legacy_import_refuses_existing_destination() {
    let root = temp_tree("legacy-refusal");
    let working = root.join("cwd");
    let base = root.join("base");
    let paths = SavePaths::new(&root, &working, &base).expect("paths fit");
    fs::create_dir_all(working.join("save/card_a")).expect("working card A");
    fs::write(
        working
            .join("save/card_a")
            .join(nookery_save::PC_GCI_FILENAME),
        support::valid_record(),
    )
    .expect("source record");
    fs::create_dir_all(&paths.card_a_dir).expect("active card A");
    fs::write(&paths.gci_path, support::valid_record()).expect("destination record");
    assert!(!pc_save_migrate_legacy(&paths));
    assert_eq!(
        fs::read(&paths.gci_path).expect("destination"),
        support::valid_record()
    );
    fs::remove_dir_all(root).expect("remove synthetic manager tree");
}

#[test]
fn malformed_short_aram_input_is_safe() {
    let mut bytes = [0u8; 1];
    nookery_save::pc_init_original_entries(&mut bytes);
    nookery_save::pc_init_mail_entries(&mut bytes);
    nookery_save::pc_init_diary_entries(&mut bytes);
    assert_eq!(bytes, [0]);
    assert_eq!(
        TownRecord::parse(&[]),
        Err(GciError::TruncatedHeader { available: 0 })
    );
}
