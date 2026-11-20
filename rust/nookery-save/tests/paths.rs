//! Path-derivation tests for the recovery companion suffixes.

use std::path::{Path, PathBuf};

use nookery_save::{
    DEFAULT_FILE_NAME, GciError, PATH_MAX, PENDING_SUFFIXES, ROTATION_SUFFIXES,
    recovery_candidates, sibling_path, validate_record_name,
};

#[test]
fn appends_upstream_suffixes_in_order() {
    let base = Path::new("/tmp/save/DobutsunomoriP_MURA.gci");
    assert_eq!(DEFAULT_FILE_NAME, "DobutsunomoriP_MURA.gci");
    assert_eq!(
        sibling_path(base, "").unwrap(),
        PathBuf::from("/tmp/save/DobutsunomoriP_MURA.gci")
    );
    assert_eq!(
        sibling_path(base, ".write").unwrap(),
        PathBuf::from("/tmp/save/DobutsunomoriP_MURA.gci.write")
    );
    assert_eq!(
        sibling_path(base, ".write.temp").unwrap(),
        PathBuf::from("/tmp/save/DobutsunomoriP_MURA.gci.write.temp")
    );
    assert_eq!(
        sibling_path(base, ".bak3").unwrap(),
        PathBuf::from("/tmp/save/DobutsunomoriP_MURA.gci.bak3")
    );
}

#[test]
fn candidate_order_matches_pc_gci_read() {
    let base = Path::new("/tmp/save/town.gci");
    let candidates = recovery_candidates(base).unwrap();
    assert_eq!(candidates.len(), 8);
    assert_eq!(candidates[0], PathBuf::from("/tmp/save/town.gci"));
    assert_eq!(candidates[1], PathBuf::from("/tmp/save/town.gci.write"));
    assert_eq!(
        candidates[2],
        PathBuf::from("/tmp/save/town.gci.write.temp")
    );
    assert_eq!(candidates[3], PathBuf::from("/tmp/save/town.gci.tmp"));
    assert_eq!(candidates[4], PathBuf::from("/tmp/save/town.gci.bak1"));
    assert_eq!(candidates[5], PathBuf::from("/tmp/save/town.gci.bak2"));
    assert_eq!(candidates[6], PathBuf::from("/tmp/save/town.gci.bak3"));
    assert_eq!(candidates[7], PathBuf::from("/tmp/save/town.gci.temp"));

    // Constant tables agree with the enumeration above.
    assert_eq!(PENDING_SUFFIXES, [".write", ".write.temp", ".tmp"]);
    assert_eq!(ROTATION_SUFFIXES, ["", ".bak1", ".bak2", ".bak3", ".temp"]);
}

#[test]
fn rejects_separator_and_parent_suffixes() {
    let base = Path::new("/tmp/save/town.gci");
    for suffix in [
        "/etc/passwd",
        "\\evil",
        "../escape",
        "..",
        "a\0b",
        "sub/dir",
    ] {
        assert_eq!(
            sibling_path(base, suffix),
            Err(GciError::InvalidSuffix),
            "suffix {suffix:?} must be refused"
        );
    }
    // A leading dot that is not a parent traversal is a normal suffix.
    assert!(sibling_path(base, ".bak1").is_ok());
}

#[test]
fn enforces_path_max_boundary() {
    let base_len = PATH_MAX - 6;
    let base = PathBuf::from("a".repeat(base_len));
    // Exactly PATH_MAX - 1 characters still fits (snprintf would succeed).
    assert!(sibling_path(&base, ".bak1").is_ok());
    // One more character overflows the upstream buffer.
    let base = PathBuf::from("a".repeat(PATH_MAX - 5));
    assert_eq!(sibling_path(&base, ".bak1"), Err(GciError::PathTooLong));
    assert_eq!(
        recovery_candidates(&base),
        Err(GciError::PathTooLong),
        "candidate enumeration must fail loudly rather than truncate"
    );
}

#[test]
fn record_name_validation_rejects_traversal_shapes() {
    assert_eq!(
        validate_record_name(b"DobutsunomoriP_MURA").unwrap(),
        b"DobutsunomoriP_MURA"
    );
    for bad in [
        &b""[..],
        &b".."[..],
        &b"."[..],
        &b"a/b"[..],
        &b"a\\b"[..],
        &b"a\0b"[..],
        &[b'A'; 32][..],
    ] {
        assert_eq!(
            validate_record_name(bad),
            Err(GciError::InvalidRecordName),
            "name {bad:?} must be refused"
        );
    }
    // 31 characters is the largest representable NUL-terminated name.
    assert!(validate_record_name(&[b'A'; 31]).is_ok());
}
