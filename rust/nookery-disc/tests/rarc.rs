//! Synthetic RARC vectors and mutation guards.

use nookery_disc::{Bytes, RarcArchive, RarcError};

const HEADER: usize = 0x20;
const INFO: usize = 0x130;
const NODE_OFFSET: usize = 0x20;
const ENTRY_OFFSET: usize = 0x40;
const STRING_OFFSET: usize = 0x100;
const DATA_LEN: usize = 15;

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn entry(bytes: &mut [u8], index: usize, id: u16, flags: u8, name: u32, offset: u32, size: u32) {
    let at = HEADER + ENTRY_OFFSET + index * 0x14;
    put_u16(bytes, at, id);
    put_u16(bytes, at + 2, 0);
    put_u32(bytes, at + 4, (u32::from(flags) << 24) | name);
    put_u32(bytes, at + 8, offset);
    put_u32(bytes, at + 12, size);
}

fn synthetic_rarc() -> Vec<u8> {
    let file_length = HEADER + INFO + DATA_LEN;
    let mut bytes = vec![0u8; file_length];
    bytes[0..4].copy_from_slice(b"RARC");
    put_u32(&mut bytes, 4, file_length as u32);
    put_u32(&mut bytes, 8, HEADER as u32);
    put_u32(&mut bytes, 12, INFO as u32);
    put_u32(&mut bytes, 16, DATA_LEN as u32);

    let info = HEADER;
    put_u32(&mut bytes, info, 2); // nodes
    put_u32(&mut bytes, info + 4, NODE_OFFSET as u32);
    put_u32(&mut bytes, info + 8, 8); // entries
    put_u32(&mut bytes, info + 12, ENTRY_OFFSET as u32);
    let strings = b".\0..\0root\0Dir\0root.txt\0nested.bin\0";
    put_u32(&mut bytes, info + 16, strings.len() as u32);
    put_u32(&mut bytes, info + 20, STRING_OFFSET as u32);
    put_u16(&mut bytes, info + 24, 8);
    bytes[info + 26] = 1;
    bytes[info + STRING_OFFSET..info + STRING_OFFSET + strings.len()].copy_from_slice(strings);

    let nodes = info + NODE_OFFSET;
    put_u32(&mut bytes, nodes, 0x524f4f54);
    put_u32(&mut bytes, nodes + 4, 5); // root
    put_u16(&mut bytes, nodes + 10, 5);
    put_u32(&mut bytes, nodes + 12, 0);
    put_u32(&mut bytes, nodes + 16, 0x44495220);
    put_u32(&mut bytes, nodes + 20, 10); // Dir
    put_u16(&mut bytes, nodes + 26, 3);
    put_u32(&mut bytes, nodes + 28, 5);

    entry(&mut bytes, 0, 0xffff, 0x02, 0, 0, 0);
    entry(&mut bytes, 1, 0xffff, 0x02, 2, u32::MAX, 0);
    entry(&mut bytes, 2, 1, 0x02, 10, 1, 0);
    entry(&mut bytes, 3, 2, 0x11, 14, 0, 4);
    entry(&mut bytes, 4, 3, 0x11, 14, 4, 5); // duplicate; last lookup winner
    entry(&mut bytes, 5, 0xffff, 0x02, 0, 1, 0);
    entry(&mut bytes, 6, 0xffff, 0x02, 2, 0, 0);
    entry(&mut bytes, 7, 4, 0x21, 23, 8, 6);

    let data = info + INFO;
    bytes[data..data + DATA_LEN].copy_from_slice(b"rootOLD!nestDIR");
    bytes
}

fn yaz0_literal_wrap(decoded: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(16 + decoded.len() + decoded.len() / 8);
    output.extend_from_slice(b"Yaz0");
    output.extend_from_slice(&(decoded.len() as u32).to_be_bytes());
    output.extend_from_slice(&[0; 8]);
    for chunk in decoded.chunks(8) {
        output.push(0xff);
        output.extend_from_slice(chunk);
    }
    output
}

fn assert_error(bytes: &[u8], expected: fn(&RarcError) -> bool) {
    let result = std::panic::catch_unwind(|| RarcArchive::parse(bytes));
    assert!(result.is_ok(), "parser panicked");
    let error = result.unwrap().expect_err("mutation must be rejected");
    assert!(expected(&error), "unexpected error: {error:?}");
}

#[test]
fn nested_reads_and_oracle_path_forms() {
    let archive = RarcArchive::parse(&synthetic_rarc()).expect("valid RARC");
    assert_eq!(archive.root_name(), "root");
    assert!(archive.directory_exists("root/dir"));
    assert!(archive.directory_exists("/dir"));
    assert_eq!(archive.list_folders("root").unwrap(), ["Dir"]);
    assert_eq!(archive.list_files("root").unwrap().len(), 2);
    let nested = archive.get_file("ROOT/DIR/NESTED.BIN").unwrap();
    assert_eq!(nested.path(), "root/Dir/nested.bin");
    assert_eq!(nested.name(), "nested.bin");
    assert_eq!(nested.data(), b"nestDI");
}

#[test]
fn duplicate_name_uses_last_lookup_entry_but_listing_preserves_both() {
    let archive = RarcArchive::parse(&synthetic_rarc()).unwrap();
    assert_eq!(archive.get_file("root/root.txt").unwrap().data(), b"OLD!n");
    assert_eq!(archive.list_files("root").unwrap().len(), 2);
}

#[test]
fn explicit_yaz0_wrapper_reuses_decoder() {
    let raw = synthetic_rarc();
    let wrapped = yaz0_literal_wrap(&raw);
    let archive = RarcArchive::parse_yaz0(&wrapped, raw.len()).expect("wrapped RARC");
    assert_eq!(archive.get_file("root/dir/nested.bin").unwrap().size(), 6);
    assert!(matches!(
        RarcArchive::parse(&wrapped),
        Err(RarcError::WrongMagic { .. })
    ));
}

#[test]
fn truncated_vector_is_rejected_without_panic() {
    let raw = synthetic_rarc();
    assert_error(&raw[..raw.len() - 1], |error| {
        matches!(error, RarcError::Truncated { .. })
    });
}

#[test]
fn wrong_magic_vector_is_rejected() {
    let mut raw = synthetic_rarc();
    raw[0..4].copy_from_slice(b"NOPE");
    assert_error(&raw, |error| matches!(error, RarcError::WrongMagic { .. }));
}

#[test]
fn table_offset_mutation_is_rejected() {
    let mut raw = synthetic_rarc();
    put_u32(&mut raw, HEADER + 4, 0xffff_ff00);
    assert_error(&raw, |error| matches!(error, RarcError::OutOfRange { .. }));
}

#[test]
fn invalid_name_offset_vector_is_rejected() {
    let mut raw = synthetic_rarc();
    put_u32(&mut raw, HEADER + NODE_OFFSET + 4, 0xffff);
    assert_error(&raw, |error| {
        matches!(error, RarcError::InvalidNameOffset { .. })
    });
}

#[test]
fn invalid_entry_range_vector_is_rejected() {
    let mut raw = synthetic_rarc();
    put_u16(&mut raw, HEADER + NODE_OFFSET + 10, 0xffff);
    assert_error(&raw, |error| {
        matches!(error, RarcError::InvalidEntryRange { .. })
    });
}

#[test]
fn entry_length_mutation_is_rejected_as_data_overflow() {
    let mut raw = synthetic_rarc();
    put_u32(&mut raw, HEADER + ENTRY_OFFSET + 3 * 0x14 + 12, u32::MAX);
    assert_error(&raw, |error| {
        matches!(error, RarcError::DataRangeOverflow { .. })
    });
}

#[test]
fn bounded_image_handoff_reads_a_rarc_from_logical_disc_bytes() {
    let raw = synthetic_rarc();
    let offset = 0x180usize;
    let mut logical = vec![0u8; offset + raw.len() + 3];
    logical[offset..offset + raw.len()].copy_from_slice(&raw);
    let mut image = Bytes::new(&logical);
    let archive =
        RarcArchive::parse_from_image(&mut image, offset as u64, raw.len()).expect("RARC handoff");
    assert_eq!(
        archive.get_file("root/dir/nested.bin").unwrap().data(),
        b"nestDI"
    );

    let mut image = Bytes::new(&logical);
    assert!(matches!(
        RarcArchive::parse_from_image(&mut image, offset as u64, raw.len() - 1),
        Err(RarcError::ImageLimit { .. })
    ));

    let mut image = Bytes::new(&logical);
    assert!(matches!(
        RarcArchive::parse_from_image(&mut image, u64::MAX, raw.len()),
        Err(RarcError::ImageRange { .. })
    ));
}
