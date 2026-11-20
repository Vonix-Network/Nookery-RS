//! Status-to-category mapping from `acgc_disc.cpp::validation_error`, the
//! numeric contract, and the launch gate.

use nookery_disc::{Status, ValidationError, validation_error};

#[test]
fn every_status_maps_to_the_source_category() {
    // Exhaustive by construction: `Status` has no other variants, and this test
    // fails to compile if one is added.
    let mapping = [
        (Status::Success, ValidationError::Success),
        (Status::IOError, ValidationError::IOError),
        (Status::InvalidImage, ValidationError::InvalidImage),
        (Status::UnknownGame, ValidationError::WrongGame),
        (Status::UnsupportedVersion, ValidationError::WrongVersion),
        (Status::Canceled, ValidationError::Canceled),
        (Status::HashMismatch, ValidationError::HashMismatch),
        (Status::Failed, ValidationError::Unknown),
    ];
    for (status, expected) in mapping {
        assert_eq!(validation_error(status), expected, "{status:?}");
    }
}

#[test]
fn discriminants_match_the_cpp_enum() {
    // `Unknown` is 0 and `Success` is 7; the settings codec depends on this.
    let order = [
        ValidationError::Unknown,
        ValidationError::IOError,
        ValidationError::InvalidImage,
        ValidationError::WrongGame,
        ValidationError::WrongVersion,
        ValidationError::Canceled,
        ValidationError::HashMismatch,
        ValidationError::Success,
    ];
    for (index, value) in order.into_iter().enumerate() {
        assert_eq!(value.code(), index as u8);
        assert_eq!(ValidationError::from_code(index as u8), Some(value));
    }
    assert_eq!(ValidationError::from_code(8), None);
    assert_eq!(ValidationError::default(), ValidationError::Unknown);
}

#[test]
fn launch_gate_allows_only_success_and_hash_mismatch() {
    // `verification_allows_launch` in prelaunch.cpp.
    for value in [
        ValidationError::Unknown,
        ValidationError::IOError,
        ValidationError::InvalidImage,
        ValidationError::WrongGame,
        ValidationError::WrongVersion,
        ValidationError::Canceled,
    ] {
        assert!(!value.allows_launch(), "{value:?}");
    }
    assert!(ValidationError::Success.allows_launch());
    assert!(ValidationError::HashMismatch.allows_launch());
}
