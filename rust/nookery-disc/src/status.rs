//! Backend status codes, mirroring `borealis::disc::Status`.

/// Outcome reported by a disc backend.
///
/// Ordering matches `borealis::disc::Status` in
/// `extern/borealis/include/borealis/disc.hpp` at pin
/// `8e9427fab92c1935a5d6bacadc6053c687f10c1e`.  The PC port never shows these to
/// the user; [`crate::ValidationError`] is the user-facing category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Status {
    /// The image is an accepted disc record.
    Success,
    /// The backend could not read the image.
    IOError,
    /// The image is not a recognizable disc image.
    InvalidImage,
    /// The game ID is not recognized at all.
    UnknownGame,
    /// The game ID is recognized but this disc/revision is not accepted.
    UnsupportedVersion,
    /// Verification was canceled by the caller.
    Canceled,
    /// The reconstructed image hash did not match the accepted record.
    HashMismatch,
    /// The backend failed for a reason without a more specific status.
    #[default]
    Failed,
}
