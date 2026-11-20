//! `famicom_gba_getImage` catalog: every id boundary, exact names and paths,
//! empty slots, and malformed ids.

use nookery_famicom::{
    MAX_ROM_ID, RESOURCE_PATH_CAPACITY, ROM_NAMES, ROM_SLOT_COUNT, ResourcePath, RomId, RomIdError,
};

/// Upstream `names[]` transcribed independently of `ROM_NAMES`, so a typo in
/// the table is caught rather than mirrored.
const UPSTREAM_NAMES: [Option<&str>; 19] = [
    Some("cluclu"),
    Some("usa_balloon"),
    Some("donkey"),
    Some("usa_jr_math"),
    Some("pinball"),
    Some("tennis"),
    Some("usa_golf"),
    None,
    Some("usa_baseball"),
    None,
    Some("usa_donkey3"),
    Some("donkeyjr"),
    Some("soccer"),
    Some("exbike"),
    None,
    Some("usa_icecl"),
    Some("mario"),
    Some("smario"),
    None,
];

fn path_for(id: u32) -> Option<String> {
    RomId::new(id)
        .expect("id within range")
        .resource_path()
        .map(|p| p.as_str().to_owned())
}

#[test]
fn table_matches_upstream_names_verbatim() {
    assert_eq!(ROM_SLOT_COUNT, 19);
    assert_eq!(ROM_SLOT_COUNT, UPSTREAM_NAMES.len());
    assert_eq!(ROM_NAMES, UPSTREAM_NAMES);
}

#[test]
fn empty_slots_are_exactly_the_four_null_entries() {
    let empty: Vec<usize> = ROM_NAMES
        .iter()
        .enumerate()
        .filter(|(_, name)| name.is_none())
        .map(|(index, _)| index)
        .collect();
    // Slot indices 7, 9, 14, 18 → rom ids 8, 10, 15, 19.
    assert_eq!(empty, vec![7, 9, 14, 18]);
}

#[test]
fn every_id_one_through_nineteen_maps_to_its_slot() {
    for id in 1..=MAX_ROM_ID {
        let rom = RomId::new(id).unwrap();
        assert_eq!(rom.get(), id);
        assert_eq!(rom.slot_index(), (id - 1) as usize);
        assert_eq!(rom.name(), UPSTREAM_NAMES[(id - 1) as usize]);

        match UPSTREAM_NAMES[(id - 1) as usize] {
            Some(name) => assert_eq!(
                path_for(id).as_deref(),
                Some(format!("/FAMICOM/GBA/jb_{name}.bin.szs").as_str()),
                "id {id}"
            ),
            None => assert_eq!(path_for(id), None, "empty slot id {id}"),
        }
    }
}

#[test]
fn id_zero_aliases_slot_one() {
    let zero = RomId::new(0).unwrap();
    let one = RomId::new(1).unwrap();
    assert_eq!(zero, RomId::ZERO);
    assert_eq!(zero.slot_index(), one.slot_index());
    assert_eq!(zero.name(), Some("cluclu"));
    assert_eq!(zero.resource_path(), one.resource_path());
    assert_eq!(
        path_for(0).as_deref(),
        Some("/FAMICOM/GBA/jb_cluclu.bin.szs")
    );
}

#[test]
fn ids_above_nineteen_are_rejected_before_the_table() {
    assert_eq!(RomId::new(MAX_ROM_ID).map(RomId::get), Ok(19));
    for id in [20u32, 21, 32, 255, u32::MAX] {
        assert_eq!(
            RomId::new(id),
            Err(RomIdError::OutOfRange { id }),
            "id {id}"
        );
    }
}

#[test]
fn out_of_range_message_names_the_id_and_bound() {
    let err = RomId::new(20).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("20"), "{text}");
    assert!(text.contains("19"), "{text}");
}

#[test]
fn exact_paths_for_notable_entries() {
    let cases: [(u32, Option<&str>); 10] = [
        (1, Some("/FAMICOM/GBA/jb_cluclu.bin.szs")),
        (2, Some("/FAMICOM/GBA/jb_usa_balloon.bin.szs")),
        (8, None),
        (9, Some("/FAMICOM/GBA/jb_usa_baseball.bin.szs")),
        (10, None),
        (11, Some("/FAMICOM/GBA/jb_usa_donkey3.bin.szs")),
        (14, Some("/FAMICOM/GBA/jb_exbike.bin.szs")),
        (15, None),
        (16, Some("/FAMICOM/GBA/jb_usa_icecl.bin.szs")),
        (17, Some("/FAMICOM/GBA/jb_mario.bin.szs")),
    ];
    for (id, expected) in cases {
        assert_eq!(path_for(id).as_deref(), expected, "id {id}");
    }
}

#[test]
fn path_is_nul_terminated_for_c_hosts() {
    let path = RomId::new(7).unwrap().resource_path().unwrap();
    let bytes = path.as_bytes_with_nul();
    assert_eq!(bytes.last(), Some(&0));
    assert_eq!(&bytes[..bytes.len() - 1], path.as_str().as_bytes());
}

#[test]
fn every_table_name_fits_the_path_capacity() {
    for name in UPSTREAM_NAMES.iter().flatten() {
        let path = ResourcePath::for_name(name).unwrap_or_else(|| panic!("{name} overflowed"));
        // 16 prefix + name + 8 suffix, plus sentinel.
        assert_eq!(path.as_str().len(), 16 + name.len() + 8);
        assert!(path.as_str().len() < RESOURCE_PATH_CAPACITY);
    }
}

#[test]
fn path_builder_rejects_names_that_would_truncate() {
    let too_long = "x".repeat(RESOURCE_PATH_CAPACITY);
    assert!(ResourcePath::for_name(&too_long).is_none());
    let longest = "x".repeat(RESOURCE_PATH_CAPACITY - 16 - 8 - 1);
    assert!(ResourcePath::for_name(&longest).is_some());
}

#[test]
fn resource_path_compares_by_content() {
    let a = RomId::new(0).unwrap().resource_path().unwrap();
    let b = RomId::new(1).unwrap().resource_path().unwrap();
    assert_eq!(a, b);
    assert_eq!(a, *"/FAMICOM/GBA/jb_cluclu.bin.szs");
    assert_ne!(a, *"different");
}
