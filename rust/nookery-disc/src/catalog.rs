//! The accepted-disc and recognized-game-id catalogs, ported verbatim from
//! `pc/src/acgc_disc.cpp`.
//!
//! Upstream declares:
//!
//! ```text
//! AcceptedDiscs       = { GAFE01, disc 0, revision 0,
//!                         xxh3-128("9ffba17f08aa24e8a084ba30e038cef3") }
//! RecognizedGameIds   = { "GAFJ01", "GAFP01" }
//! ```
//!
//! No further records are invented here: a different revision, disc number, or
//! hash would change what the port accepts.

use crate::hash::Digest128;
use crate::header::Header;

/// One accepted disc record (`borealis::disc::AcceptedDisc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedDisc {
    /// Six-character Nintendo game ID.
    pub game_id: &'static str,
    /// Disc number within a multi-disc release.
    pub disc_number: u8,
    /// Disc revision.
    pub revision: u8,
    /// Expected XXH3-128 of the reconstructed image.
    pub expected_hash: Digest128,
}

/// Catalog threaded through inspection (`borealis::disc::Catalog`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Catalog {
    /// Fully accepted discs, matched before any hash work.
    pub accepted_discs: &'static [AcceptedDisc],
    /// Game IDs that report [`crate::Status::UnsupportedVersion`] instead of
    /// [`crate::Status::UnknownGame`].
    pub recognized_game_ids: &'static [&'static str],
}

/// Hash of the accepted GAFE01 revision-0 disc, from `acgc_disc.cpp`.
///
/// Written in canonical high-bits-first form; [`Digest128::parse`] splits it
/// exactly like `parse_xxh3_128`.
const GAFE01_00_HASH: &str = "9ffba17f08aa24e8a084ba30e038cef3";

const ACCEPTED_DISCS: [AcceptedDisc; 1] = [AcceptedDisc {
    game_id: "GAFE01",
    disc_number: 0,
    revision: 0,
    // `Digest128::parse` is not `const`, so the record is built from the split
    // halves produced by the same nibble arithmetic.  `tests` re-parses the text
    // form to prove the halves agree with `parse`.
    expected_hash: Digest128 {
        high: 0x9ffb_a17f_08aa_24e8,
        low: 0xa084_ba30_e038_cef3,
    },
}];

const RECOGNIZED_GAME_IDS: [&str; 2] = ["GAFJ01", "GAFP01"];

/// The exact catalog declared by [`Catalog::upstream`] in `acgc_disc.cpp`.
pub const UPSTREAM: Catalog = Catalog {
    accepted_discs: &ACCEPTED_DISCS,
    recognized_game_ids: &RECOGNIZED_GAME_IDS,
};

impl Catalog {
    /// The upstream Nookery catalog: GAFE01 rev 0 accepted, GAFJ01/GAFP01
    /// recognized only.
    pub const fn upstream() -> &'static Self {
        &UPSTREAM
    }

    /// Canonical text form of the accepted hash, for tests and diagnostics.
    pub const fn accepted_hash_text(&self) -> &'static str {
        GAFE01_00_HASH
    }

    /// Mirrors `find_accepted_disc`: first record whose game ID, disc number,
    /// and revision all match, in catalog order.
    ///
    /// The game ID comparison is byte-exact against the raw six header bytes,
    /// matching `record.gameId == metadata.gameId` over `std::string`, so no
    /// lossy UTF-8 conversion can merge two distinct IDs.
    pub fn accepted_index(&self, header: &Header) -> Option<usize> {
        self.accepted_discs.iter().position(|record| {
            record.game_id.as_bytes() == header.game_id
                && record.disc_number == header.disc_number
                && record.revision == header.revision
        })
    }

    /// Mirrors `is_recognized_game`: any accepted record for the game ID counts
    /// as recognized, even when the disc number or revision does not match.
    pub fn is_recognized_game(&self, header: &Header) -> bool {
        let game_id = &header.game_id[..];
        self.accepted_discs
            .iter()
            .any(|record| record.game_id.as_bytes() == game_id)
            || self
                .recognized_game_ids
                .iter()
                .any(|candidate| candidate.as_bytes() == game_id)
    }
}
