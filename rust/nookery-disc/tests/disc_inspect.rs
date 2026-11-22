//! Path-level `acgc::disc::inspect` parity vectors.
//!
//! `pc/src/acgc_disc.cpp::inspect` calls `borealis::disc::inspect` (which is
//! `open_and_inspect`: header read plus catalog classification, no hashing) and
//! then `update_info`, which assigns `DiscInfo` from the result *unconditionally*.
//! Every vector below uses synthetic bytes only; no disc image is required.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use nookery_disc::{
    CISO_HEADER_SIZE, Catalog, ContainerKind, DiscInfo, GAMECUBE_BLOCK_SIZE, Never, Platform,
    Status, ValidationError, inspect_disc_path, validation_error, verify_path,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nookery-disc-inspect-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create temporary test directory");
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).expect("write synthetic fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A synthetic GameCube image whose 0x400-byte header is accepted by the
/// upstream catalog, but whose remaining bytes can never match the accepted
/// XXH3-128. `len` is the total image length.
fn synthetic_image(game_id: &[u8; 6], disc_number: u8, revision: u8, len: usize) -> Vec<u8> {
    let mut bytes = vec![0x5au8; len];
    bytes[..6].copy_from_slice(game_id);
    bytes[6] = disc_number;
    bytes[7] = revision;
    bytes[24..28].copy_from_slice(&[0, 0, 0, 0]);
    bytes[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
    bytes[32..32 + 11].copy_from_slice(b"synthetic\0\0");
    bytes
}

/// A CISO whose only present block is the (synthetic) header block, so the
/// logical image is mostly zeroes and cannot match the accepted hash either.
fn synthetic_ciso() -> Vec<u8> {
    let block = GAMECUBE_BLOCK_SIZE as usize;
    let mut header_block = vec![0u8; block];
    header_block[..6].copy_from_slice(b"GAFE01");
    header_block[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
    header_block[32..41].copy_from_slice(b"synthetic");

    let mut bytes = vec![0u8; CISO_HEADER_SIZE];
    bytes[..4].copy_from_slice(b"CISO");
    bytes[4..8].copy_from_slice(&(block as u32).to_le_bytes());
    bytes[8] = 1;
    bytes.extend_from_slice(&header_block);
    bytes
}

/// Inspecting must classify from the header alone: the accepted ID/revision
/// wins even though the body can never match the accepted hash. `verify_path`
/// on the same bytes must disagree (`HashMismatch`), which is what proves the
/// inspect call performed no hashing work.
#[test]
fn inspect_classifies_from_the_header_without_hashing_the_image() {
    let dir = TempDir::new("no-hash");
    let image = synthetic_image(b"GAFE01", 0, 0, 64 * 1024);
    let iso = dir.write("game.iso", &image);
    let gcm = dir.write("game.gcm", &image);
    let ciso = dir.write("game.ciso", &synthetic_ciso());

    for path in [&iso, &gcm, &ciso] {
        let result = inspect_disc_path(path, Catalog::upstream(), &Never);
        assert_eq!(result.status, Status::Success, "{}", path.display());
        assert_eq!(
            validation_error(result.status),
            ValidationError::Success,
            "{}",
            path.display()
        );
        assert!(result.accepted_disc_index.is_some());
        assert_eq!(result.metadata.platform, Platform::GameCube);
        assert_eq!(result.metadata.game_id, "GAFE01");
        assert_eq!(result.metadata.title, "synthetic");
        assert_eq!(result.metadata.disc_number, 0);
        assert_eq!(result.metadata.revision, 0);
        assert!(result.message.is_empty());
    }

    let verified = verify_path(&iso, Catalog::upstream(), &Never, &mut |_| {});
    assert_eq!(verified.status, Status::HashMismatch);
}

/// `update_info` copies the four `DiscInfo` fields straight from the result
/// metadata, including on failure (where the metadata is the empty default).
#[test]
fn disc_info_projects_result_metadata_unconditionally() {
    let dir = TempDir::new("projection");
    let iso = dir.write("ok.iso", &synthetic_image(b"GAFE01", 0, 0, 0x800));

    let success = inspect_disc_path(&iso, Catalog::upstream(), &Never);
    let info = DiscInfo::from_result(&success);
    assert_eq!(
        info,
        DiscInfo {
            game_id: "GAFE01".to_owned(),
            title: "synthetic".to_owned(),
            disc_number: 0,
            revision: 0,
        }
    );

    let wrong_game = inspect_disc_path(
        &dir.write("other.iso", &synthetic_image(b"ZZZE01", 0, 0, 0x800)),
        Catalog::upstream(),
        &Never,
    );
    assert_eq!(wrong_game.status, Status::UnknownGame);
    // The header decoded, so `update_info` still copies it: the C++ keeps the
    // identified ID and title even when the disc is the wrong game. Only a
    // failed *header* read leaves the empty default.
    assert_eq!(
        DiscInfo::from_result(&wrong_game),
        DiscInfo {
            game_id: "ZZZE01".to_owned(),
            title: "synthetic".to_owned(),
            disc_number: 0,
            revision: 0,
        }
    );

    let truncated = inspect_disc_path(
        &dir.write("truncated.iso", b"GAFE01"),
        Catalog::upstream(),
        &Never,
    );
    assert_eq!(truncated.status, Status::InvalidImage);
    assert_eq!(DiscInfo::from_result(&truncated), DiscInfo::default());

    let missing = inspect_disc_path(&dir.0.join("absent.iso"), Catalog::upstream(), &Never);
    assert_eq!(missing.status, Status::IOError);
    assert_eq!(DiscInfo::from_result(&missing), DiscInfo::default());
}

/// Classification order and failure mapping from `open_and_inspect` and
/// `validation_error`, exercised through the path entry point.
#[test]
fn inspect_maps_every_classification_to_the_pc_validation_category() {
    let dir = TempDir::new("classes");

    let recognized = dir.write("recognized.iso", &synthetic_image(b"GAFJ01", 0, 0, 0x800));
    assert_eq!(
        inspect_disc_path(&recognized, Catalog::upstream(), &Never).status,
        Status::UnsupportedVersion
    );

    let wrong_disc = dir.write("disc1.iso", &synthetic_image(b"GAFE01", 1, 0, 0x800));
    assert_eq!(
        inspect_disc_path(&wrong_disc, Catalog::upstream(), &Never).status,
        Status::UnsupportedVersion
    );

    let wrong_revision = dir.write("rev1.iso", &synthetic_image(b"GAFE01", 0, 1, 0x800));
    assert_eq!(
        inspect_disc_path(&wrong_revision, Catalog::upstream(), &Never).status,
        Status::UnsupportedVersion
    );

    let mut unknown_platform = synthetic_image(b"GAFE01", 0, 0, 0x800);
    unknown_platform[24..32].fill(0);
    let unknown_platform = dir.write("no-magic.iso", &unknown_platform);
    assert_eq!(
        inspect_disc_path(&unknown_platform, Catalog::upstream(), &Never).status,
        Status::InvalidImage
    );

    let truncated = dir.write("short.iso", b"GAFE01");
    assert_eq!(
        inspect_disc_path(&truncated, Catalog::upstream(), &Never).status,
        Status::InvalidImage
    );

    let mut bad_ciso = vec![0u8; CISO_HEADER_SIZE];
    bad_ciso[..4].copy_from_slice(b"CISO");
    bad_ciso[4..8].copy_from_slice(&0x400u32.to_le_bytes());
    bad_ciso[8] = 1;
    let bad_ciso = dir.write("bad.ciso", &bad_ciso);
    assert_eq!(
        inspect_disc_path(&bad_ciso, Catalog::upstream(), &Never).status,
        Status::InvalidImage
    );
}

/// RVZ stays an explicit decoder boundary: the container is recognized, but
/// compressed bytes are never classified as if they were a logical header.
#[test]
fn inspect_reports_the_rvz_decoder_boundary_without_reading_a_logical_header() {
    let dir = TempDir::new("rvz");
    let rvz = dir.write("game.rvz", b"RVZ\0");

    let result = inspect_disc_path(&rvz, Catalog::upstream(), &Never);
    assert_eq!(result.status, Status::Failed);
    assert!(result.message.contains("decoder unavailable"));
    assert_eq!(result.metadata, Default::default());
    assert_eq!(result.accepted_disc_index, None);
}

/// Cancellation is polled before the header read, so a cancelled inspect does
/// no I/O and reports `Canceled` rather than a header verdict.
#[test]
fn cancelled_inspect_stops_before_reading_the_header() {
    let dir = TempDir::new("cancelled");
    let iso = dir.write("game.iso", &synthetic_image(b"GAFE01", 0, 0, 0x800));

    let flag = nookery_disc::CancelFlag::new();
    flag.cancel();
    let result = inspect_disc_path(&iso, Catalog::upstream(), &flag);
    assert_eq!(result.status, Status::Canceled);
    assert_eq!(DiscInfo::from_result(&result), DiscInfo::default());
}

/// A CISO named `.iso` must still take the CISO route: detection is by magic,
/// and the raw-image header path must never see the container bytes.
#[test]
fn container_kind_matches_the_detected_format_not_the_extension() {
    let dir = TempDir::new("kind");
    let disguised = dir.write("game.iso", &synthetic_ciso());

    let metadata = nookery_disc::inspect_path(&disguised, &Never).expect("CISO metadata");
    assert_eq!(metadata.kind, ContainerKind::Ciso);
    assert_eq!(
        inspect_disc_path(&disguised, Catalog::upstream(), &Never).status,
        Status::Success
    );
}
