//! Source-grounded GameCube/Wii CISO mapping vectors.
//!
//! The expected bytes are produced by an independent sparse-map calculation,
//! not by calling the implementation under test. All inputs are synthetic.

use std::io::Cursor;

use nookery_disc::{
    CISO_HEADER_SIZE, CisoError, CisoImage, CisoMetadata, ContainerKind, DiscImage,
    GAMECUBE_BLOCK_SIZE, GAMECUBE_LOGICAL_BLOCKS, GAMECUBE_LOGICAL_SIZE, Never, Platform,
    StreamVerifyError, inspect_stream, verify_path,
};

const BLOCK: usize = GAMECUBE_BLOCK_SIZE as usize;

fn ciso(map: &[u8], blocks: &[&[u8]]) -> Vec<u8> {
    ciso_with_block_size(BLOCK, map, blocks)
}

fn ciso_with_block_size(block_size: usize, map: &[u8], blocks: &[&[u8]]) -> Vec<u8> {
    let mut source = vec![0u8; CISO_HEADER_SIZE];
    source[..4].copy_from_slice(b"CISO");
    source[4..8].copy_from_slice(&(block_size as u32).to_le_bytes());
    source[8..8 + map.len()].copy_from_slice(map);
    for block in blocks {
        assert_eq!(block.len(), block_size);
        source.extend_from_slice(block);
    }
    source
}

fn reference_read(source: &[u8], map: &[u8], offset: usize, length: usize) -> Vec<u8> {
    let logical_len = GAMECUBE_LOGICAL_SIZE as usize;
    let mut output = Vec::with_capacity(length);
    let count = length.min(logical_len.saturating_sub(offset));
    for logical in offset..offset + count {
        let block = logical / BLOCK;
        let within = logical % BLOCK;
        if map[block] == 0 {
            output.push(0);
        } else {
            let packed = map[..block].iter().filter(|value| **value == 1).count();
            output.push(source[CISO_HEADER_SIZE + packed * BLOCK + within]);
        }
    }
    output
}

#[test]
fn ciso_logical_size_and_sector_mapping_match_independent_calculation() {
    let mut first = vec![0u8; BLOCK];
    first[..6].copy_from_slice(b"GAFE01");
    first[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
    first[BLOCK - 1] = 0xa1;
    let mut third = vec![0u8; BLOCK];
    third.fill(0x7e);
    let mut map = vec![0u8; GAMECUBE_LOGICAL_BLOCKS];
    map[..3].copy_from_slice(&[1, 0, 1]);
    let source = ciso(&map, &[&first, &third]);
    let mut reader = Cursor::new(source.clone());
    let mut image = CisoImage::open(&mut reader).expect("valid CISO");

    assert_eq!(
        image.metadata(),
        &CisoMetadata {
            block_size: BLOCK as u32,
            logical_blocks: GAMECUBE_LOGICAL_BLOCKS as u64,
            present_blocks: 2,
            logical_size: GAMECUBE_LOGICAL_SIZE,
        }
    );
    assert_eq!(image.map_entry(1), Some(0));
    assert_eq!(image.map_entry(3), Some(0));

    for (offset, length) in [
        (0, BLOCK),
        (BLOCK - 1, 3),
        (BLOCK, BLOCK),
        (2 * BLOCK, BLOCK),
    ] {
        let mut actual = vec![0u8; length];
        let read = image
            .read_at(offset as u64, &mut actual)
            .expect("mapped read");
        assert_eq!(read, length);
        assert_eq!(actual, reference_read(&source, &map, offset, length));
    }

    let tail_offset = GAMECUBE_LOGICAL_SIZE as usize - 3;
    let mut tail = [0u8; 8];
    assert_eq!(
        image
            .read_at(tail_offset as u64, &mut tail)
            .expect("tail read"),
        3
    );
    assert_eq!(
        &tail[..3],
        &reference_read(&source, &map, tail_offset, 8)[..]
    );
}

#[test]
fn ciso_tail_length_and_offset_are_mutation_sensitive() {
    let block_size = 0x400000usize;
    let logical_blocks = GAMECUBE_LOGICAL_SIZE.div_ceil(block_size as u64) as usize;
    let mut first = vec![0u8; block_size];
    first[..6].copy_from_slice(b"GAFE01");
    first[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
    let mut final_block = vec![0u8; block_size];
    let tail = GAMECUBE_LOGICAL_SIZE as usize % block_size;
    for (index, byte) in final_block[..tail].iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(3).wrapping_add(1);
    }
    let mut map = vec![0u8; logical_blocks];
    map[0] = 1;
    map[logical_blocks - 1] = 1;
    let source = ciso_with_block_size(block_size, &map, &[&first, &final_block]);
    let mut reader = Cursor::new(source.clone());
    let mut image = CisoImage::open(&mut reader).expect("valid CISO");

    let offset = GAMECUBE_LOGICAL_SIZE as usize - tail + 17;
    let length = tail - 17 + 9;
    let mut actual = vec![0xa5; length];
    assert_eq!(
        image
            .read_at(offset as u64, &mut actual)
            .expect("bounded final-block read"),
        tail - 17
    );
    assert_eq!(&actual[..tail - 17], &final_block[17..tail]);
    assert!(actual[tail - 17..].iter().all(|&byte| byte == 0xa5));

    let mut shifted = vec![0u8; 4];
    image
        .read_at((offset + 1) as u64, &mut shifted)
        .expect("shifted final-block read");
    assert_eq!(&shifted, &final_block[18..22]);
}

#[test]
fn ciso_inspection_decodes_the_logical_gamecube_header() {
    let mut block = vec![0u8; BLOCK];
    block[..6].copy_from_slice(b"GAFE01");
    block[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
    block[32..47].copy_from_slice(b"synthetic CISO\0");
    let source = ciso(&[1], &[&block]);
    let mut reader = Cursor::new(source.clone());
    let metadata = inspect_stream(&mut reader, source.len() as u64, &Never, None)
        .expect("logical CISO inspection");
    assert_eq!(metadata.kind, ContainerKind::Ciso);
    assert_eq!(metadata.logical_size, Some(GAMECUBE_LOGICAL_SIZE));
    assert_eq!(metadata.block_size, Some(BLOCK as u32));
    assert_eq!(metadata.index_entries, Some(GAMECUBE_LOGICAL_BLOCKS as u64));
    assert_eq!(
        metadata.header.as_ref().map(|header| header.platform),
        Some(Platform::GameCube)
    );
}

#[test]
fn malformed_and_truncated_ciso_headers_are_classified_precisely() {
    let mut bad_map = vec![0u8; CISO_HEADER_SIZE];
    bad_map[..4].copy_from_slice(b"CISO");
    bad_map[4..8].copy_from_slice(&(BLOCK as u32).to_le_bytes());
    bad_map[8] = 2;
    let mut reader = Cursor::new(bad_map);
    assert_eq!(
        inspect_stream(&mut reader, CISO_HEADER_SIZE as u64, &Never, None),
        Err(StreamVerifyError::InvalidHeader)
    );

    let mut bad_block = vec![0u8; CISO_HEADER_SIZE];
    bad_block[..4].copy_from_slice(b"CISO");
    bad_block[4..8].copy_from_slice(&0x400u32.to_le_bytes());
    let mut reader = Cursor::new(bad_block);
    assert_eq!(
        inspect_stream(&mut reader, CISO_HEADER_SIZE as u64, &Never, None),
        Err(StreamVerifyError::InvalidHeader)
    );

    let truncated = vec![b'C', b'I', b'S', b'O'];
    let mut reader = Cursor::new(truncated);
    assert_eq!(
        inspect_stream(&mut reader, 4, &Never, None),
        Err(StreamVerifyError::ShortRead)
    );
}

#[test]
fn ciso_missing_packed_sector_is_a_short_read_and_cancellation_precedes_io() {
    let source = ciso(&[1, 1], &[&vec![0u8; BLOCK]]);
    let mut reader = Cursor::new(source);
    assert!(matches!(
        CisoImage::open(&mut reader),
        Err(CisoError::ShortData { .. })
    ));

    let mut reader = Cursor::new(vec![0u8; CISO_HEADER_SIZE]);
    let flag = nookery_disc::CancelFlag::new();
    flag.cancel();
    assert_eq!(
        inspect_stream(&mut reader, CISO_HEADER_SIZE as u64, &flag, None),
        Err(StreamVerifyError::Cancelled)
    );
}

#[test]
fn ciso_path_verification_honors_cancellation_before_large_logical_hash() {
    let mut block = vec![0u8; BLOCK];
    block[..6].copy_from_slice(b"GAFE01");
    block[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
    let source = ciso(&[1], &[&block]);

    let path = std::env::temp_dir().join(format!("nookery-disc-ciso-{}.ciso", std::process::id()));
    std::fs::write(&path, source).expect("write synthetic CISO");
    let flag = nookery_disc::CancelFlag::new();
    flag.cancel();
    let result = verify_path(&path, nookery_disc::Catalog::upstream(), &flag, &mut |_| {});
    std::fs::remove_file(&path).expect("remove synthetic CISO");
    assert_eq!(result.status, nookery_disc::Status::Canceled);
}
