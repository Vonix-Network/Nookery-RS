//! The verification-state log line from `acgc_disc.cpp::log_verification_state`.

/// Persisted verification state (`acgc::DiscVerificationState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerificationState {
    /// Not yet verified.
    #[default]
    Unknown,
    /// Verified against the accepted hash.
    Success,
    /// Verified, hash differed.
    HashMismatch,
}

impl VerificationState {
    /// Label used in the log line, matching the ternary in
    /// `log_verification_state`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Success => "verified",
            Self::HashMismatch => "hash mismatch",
            Self::Unknown => "unknown",
        }
    }

    /// Settings text from `config_enum_values(DiscVerificationState)`.
    pub const fn config_value(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Success => "success",
            Self::HashMismatch => "hash_mismatch",
        }
    }
}

/// Mirrors `prelaunch.cpp::verification_to_config`: only `Success` and
/// `HashMismatch` survive persistence; everything else becomes `Unknown`.
pub fn verification_to_config(error: crate::ValidationError) -> VerificationState {
    match error {
        crate::ValidationError::Success => VerificationState::Success,
        crate::ValidationError::HashMismatch => VerificationState::HashMismatch,
        _ => VerificationState::Unknown,
    }
}

/// Mirrors `prelaunch.cpp::verification_from_config`.
pub fn verification_from_config(state: VerificationState) -> crate::ValidationError {
    match state {
        VerificationState::Success => crate::ValidationError::Success,
        VerificationState::HashMismatch => crate::ValidationError::HashMismatch,
        VerificationState::Unknown => crate::ValidationError::Unknown,
    }
}

/// Renders the exact `Log.info` payload: `Disc verification status: {label}
/// (path: {path})`, with `path` replaced by `<none>` when empty.
pub fn verification_log_line(path: &str, state: VerificationState) -> String {
    let shown = if path.is_empty() { "<none>" } else { path };
    format!(
        "Disc verification status: {} (path: {shown})",
        state.label()
    )
}
