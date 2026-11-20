//! Verification-state logging and settings round trip from
//! `acgc_disc.cpp::log_verification_state` and `prelaunch.cpp`.

use nookery_disc::{
    ValidationError, VerificationState, verification_from_config, verification_log_line,
    verification_to_config,
};

#[test]
fn log_line_matches_the_source_format() {
    assert_eq!(
        verification_log_line("game.rvz", VerificationState::Success),
        "Disc verification status: verified (path: game.rvz)"
    );
    assert_eq!(
        verification_log_line("game.rvz", VerificationState::HashMismatch),
        "Disc verification status: hash mismatch (path: game.rvz)"
    );
    assert_eq!(
        verification_log_line("game.rvz", VerificationState::Unknown),
        "Disc verification status: unknown (path: game.rvz)"
    );
}

#[test]
fn empty_path_renders_the_none_placeholder() {
    // `path.empty() ? "<none>" : path`.
    assert_eq!(
        verification_log_line("", VerificationState::Success),
        "Disc verification status: verified (path: <none>)"
    );
}

#[test]
fn only_success_and_hash_mismatch_survive_persistence() {
    for value in [
        ValidationError::Unknown,
        ValidationError::IOError,
        ValidationError::InvalidImage,
        ValidationError::WrongGame,
        ValidationError::WrongVersion,
        ValidationError::Canceled,
    ] {
        assert_eq!(
            verification_to_config(value),
            VerificationState::Unknown,
            "{value:?}"
        );
    }
    assert_eq!(
        verification_to_config(ValidationError::Success),
        VerificationState::Success
    );
    assert_eq!(
        verification_to_config(ValidationError::HashMismatch),
        VerificationState::HashMismatch
    );
}

#[test]
fn config_round_trip_is_stable_for_launchable_states() {
    for state in [
        VerificationState::Unknown,
        VerificationState::Success,
        VerificationState::HashMismatch,
    ] {
        assert_eq!(
            verification_to_config(verification_from_config(state)),
            state,
            "{state:?}"
        );
    }
    // The other direction is lossy exactly where upstream is lossy.
    assert_eq!(
        verification_from_config(verification_to_config(ValidationError::WrongGame)),
        ValidationError::Unknown
    );
}

#[test]
fn config_values_match_the_settings_enum_table() {
    assert_eq!(VerificationState::default(), VerificationState::Unknown);
    assert_eq!(VerificationState::Unknown.config_value(), "unknown");
    assert_eq!(VerificationState::Success.config_value(), "success");
    assert_eq!(
        VerificationState::HashMismatch.config_value(),
        "hash_mismatch"
    );
}
