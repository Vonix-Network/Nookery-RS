//! Header decoding from `NodDiscHeader` offsets (`nod.h`).

mod support;

use nookery_disc::HeaderError;
use nookery_disc::header::{self, HEADER_SIZE, Platform};

/// `NodDiscHeader::game_title` offset (`nod.h`: 6 + 1 + 1 + 1 + 1 + 14 + 4 + 4).
const TITLE_OFFSET: usize = 32;

/// `NodDiscHeader::wii_magic` offset (`nod.h`: 6 + 1 + 1 + 1 + 1 + 14).
const WII_MAGIC_OFFSET: usize = 24;

#[test]
fn decodes_game_id_disc_revision_and_title() {
    let bytes = support::image(b"GAFE01", 0, 0, support::Magic::GameCube);
    let decoded = header::decode(&bytes).expect("full header decodes");
    assert_eq!(decoded.game_id_text(), "GAFE01");
    assert_eq!(decoded.disc_number, 0);
    assert_eq!(decoded.revision, 0);
    assert_eq!(decoded.title_text(), "Nookery synthetic image");
    assert_eq!(decoded.platform, Platform::GameCube);
}

#[test]
fn reads_nonzero_disc_and_revision_bytes() {
    let bytes = support::image(b"GAFJ01", 3, 2, support::Magic::Wii);
    let decoded = header::decode(&bytes).unwrap();
    assert_eq!(decoded.disc_number, 3);
    assert_eq!(decoded.revision, 2);
    assert_eq!(decoded.platform, Platform::Wii);
}

#[test]
fn title_stops_at_first_nul() {
    let mut bytes = support::image(b"GAFE01", 0, 0, support::Magic::GameCube);
    // Byte 32 is the first title byte; cut the title after "Nookery".
    bytes[TITLE_OFFSET + 7] = 0;
    bytes[TITLE_OFFSET + 8] = b'X';
    assert_eq!(header::decode(&bytes).unwrap().title_text(), "Nookery");
}

#[test]
fn missing_magic_is_unknown_platform() {
    let bytes = support::image(b"GAFE01", 0, 0, support::Magic::None);
    assert_eq!(header::decode(&bytes).unwrap().platform, Platform::Unknown);
}

#[test]
fn gcn_magic_wins_when_both_are_present() {
    // `platform_from_header` tests GameCube first.
    let mut bytes = support::image(b"GAFE01", 0, 0, support::Magic::GameCube);
    bytes[WII_MAGIC_OFFSET..WII_MAGIC_OFFSET + 4].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
    assert_eq!(header::decode(&bytes).unwrap().platform, Platform::GameCube);
}

#[test]
fn truncated_header_reports_available_length() {
    let bytes = support::image(b"GAFE01", 0, 0, support::Magic::GameCube);
    assert_eq!(
        header::decode(&bytes[..HEADER_SIZE - 1]),
        Err(HeaderError::Truncated {
            len: HEADER_SIZE - 1
        })
    );
    assert_eq!(header::decode(&[]), Err(HeaderError::Truncated { len: 0 }));
    // Exactly HEADER_SIZE is enough.
    assert!(header::decode(&bytes[..HEADER_SIZE]).is_ok());
}

#[test]
fn non_utf8_fields_do_not_panic() {
    let mut bytes = support::image(b"GAFE01", 0, 0, support::Magic::GameCube);
    bytes[0..6].copy_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA]);
    bytes[TITLE_OFFSET] = 0x80;
    let decoded = header::decode(&bytes).unwrap();
    // Lossy text, never an error: upstream assigns raw bytes into a std::string.
    assert!(!decoded.game_id_text().is_empty());
    assert_eq!(decoded.platform, Platform::GameCube);
}

#[test]
fn magic_is_read_from_the_nod_layout_not_the_iso_1024_offsets() {
    // `NodDiscHeader` puts wii_magic at 24 and gcn_magic at 28. `game_title`
    // therefore starts at 32, so title bytes must not leak into the magic check.
    let bytes = support::image(b"GAFE01", 0, 0, support::Magic::GameCube);
    assert_eq!(
        &bytes[TITLE_OFFSET - 4..TITLE_OFFSET],
        &[0xC2, 0x33, 0x9F, 0x3D]
    );
    assert_ne!(
        &bytes[WII_MAGIC_OFFSET..WII_MAGIC_OFFSET + 4],
        &[0x5D, 0x1C, 0x9E, 0xA3]
    );
    assert_eq!(&bytes[TITLE_OFFSET..TITLE_OFFSET + 7], b"Nookery");

    // Placing the GameCube magic two bytes early must not be detected: that is
    // the ISO-1024 offset the previous candidate used.
    let mut shifted = bytes;
    shifted[TITLE_OFFSET - 4..TITLE_OFFSET].fill(0);
    shifted[WII_MAGIC_OFFSET + 2..WII_MAGIC_OFFSET + 6].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    assert_eq!(
        header::decode(&shifted).unwrap().platform,
        Platform::Unknown
    );
}
