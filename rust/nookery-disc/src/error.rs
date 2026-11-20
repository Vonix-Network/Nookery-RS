//! User-facing validation categories, ported from `acgc::disc::ValidationError`.
//!
//! `pc/include/acgc/disc.hpp` declares the enum with `Unknown = 0` first and
//! `Success` last; that discriminant order is part of the settings/config
//! contract, so this mirror keeps it and maps statuses explicitly rather than
//! using `as` casts on foreign enums.

use crate::status::Status;

/// Result category surfaced to the UI (`acgc::disc::ValidationError`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ValidationError {
    /// Anything without a more specific mapping, including `Status::Failed`.
    #[default]
    Unknown,
    /// The backend could not read the image.
    IOError,
    /// The image is not a recognizable disc image.
    InvalidImage,
    /// The game ID is not recognized at all.
    WrongGame,
    /// The game ID is recognized but this disc/revision is not accepted.
    WrongVersion,
    /// Verification was canceled.
    Canceled,
    /// The hash did not match the accepted record.
    HashMismatch,
    /// The image matched the accepted record.
    Success,
}

/// Upstream discriminant, for the settings/`config_enum_values` boundary.
impl ValidationError {
    /// Numeric wire value used by `acgc::disc::ValidationError`.
    pub const fn code(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::IOError => 1,
            Self::InvalidImage => 2,
            Self::WrongGame => 3,
            Self::WrongVersion => 4,
            Self::Canceled => 5,
            Self::HashMismatch => 6,
            Self::Success => 7,
        }
    }

    /// Inverse of [`ValidationError::code`].
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Unknown),
            1 => Some(Self::IOError),
            2 => Some(Self::InvalidImage),
            3 => Some(Self::WrongGame),
            4 => Some(Self::WrongVersion),
            5 => Some(Self::Canceled),
            6 => Some(Self::HashMismatch),
            7 => Some(Self::Success),
            _ => None,
        }
    }

    /// Whether this category permits launching the game.
    ///
    /// Mirrors `prelaunch.cpp::verification_allows_launch` and
    /// `aurora_main.cpp`'s `inspect(..) == Success` gate: only `Success` and
    /// `HashMismatch` may launch, though `--disc` requires exact `Success`.
    pub const fn allows_launch(self) -> bool {
        matches!(self, Self::Success | Self::HashMismatch)
    }
}

/// Mirrors `acgc_disc.cpp::validation_error`.
///
/// `Status::Failed` and any future variant map to [`ValidationError::Unknown`],
/// matching the `case Failed: default:` arm.
pub const fn validation_error(status: Status) -> ValidationError {
    match status {
        Status::Success => ValidationError::Success,
        Status::IOError => ValidationError::IOError,
        Status::InvalidImage => ValidationError::InvalidImage,
        Status::UnknownGame => ValidationError::WrongGame,
        Status::UnsupportedVersion => ValidationError::WrongVersion,
        Status::Canceled => ValidationError::Canceled,
        Status::HashMismatch => ValidationError::HashMismatch,
        Status::Failed => ValidationError::Unknown,
    }
}
