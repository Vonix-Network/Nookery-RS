//! Inspection, verification, and status classification from
//! `extern/borealis/src/disc.cpp`.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nookery_disc::{
    AcceptedDisc, Bytes, CancelFlag, Catalog, Digest128, DiscImage, ImageError, Progress, Status,
    inspect, inspect_error, validation_error, verify, verify_simple,
};
use support::{
    FailAfterImage, FailingImage, Magic, OversizedAfterHeaderImage, OversizedImage, RecordingImage,
    ShortImage,
};

/// Digest of the synthetic GAFE01 image built by `support::image`, computed
/// independently with `libxxhash`'s `XXH3_128bits` at the `NodDiscHeader`
/// offsets (title at 32, GameCube magic at 28).
const SYNTHETIC_GAFE01_HASH: &str = "a48903e27361c482c9045e00c8005e47";

const SYNTHETIC_ACCEPTED: [AcceptedDisc; 1] = [AcceptedDisc {
    game_id: "GAFE01",
    disc_number: 0,
    revision: 0,
    expected_hash: Digest128 {
        high: 0xa489_03e2_7361_c482,
        low: 0xc904_5e00_c800_5e47,
    },
}];

const SYNTHETIC_CATALOG: Catalog = Catalog {
    accepted_discs: &SYNTHETIC_ACCEPTED,
    recognized_game_ids: &["GAFJ01", "GAFP01"],
};

fn synthetic() -> Vec<u8> {
    support::image(b"GAFE01", 0, 0, Magic::GameCube)
}

#[test]
fn synthetic_catalog_hash_is_the_reference_digest() {
    // Guards the literal against a typo: parse it back and compare with the
    // documented text form.
    assert_eq!(
        SYNTHETIC_ACCEPTED[0].expected_hash.to_string(),
        SYNTHETIC_GAFE01_HASH
    );
}

#[test]
fn inspect_accepts_the_accepted_record() {
    let bytes = synthetic();
    let mut image = Bytes::new(&bytes);
    let result = inspect(&mut image, Catalog::upstream());
    assert_eq!(result.status, Status::Success);
    assert_eq!(result.accepted_disc_index, Some(0));
    assert_eq!(result.metadata.game_id, "GAFE01");
    assert_eq!(result.metadata.title, "Nookery synthetic image");
    assert_eq!(result.metadata.disc_number, 0);
    assert_eq!(result.metadata.revision, 0);
    assert_eq!(result.metadata.logical_size, bytes.len() as u64);
    assert_eq!(result.metadata.platform, nookery_disc::Platform::GameCube);
    assert!(result.message.is_empty(), "success carries no diagnostic");
}

#[test]
fn recognized_ids_are_wrong_version_not_wrong_game() {
    for id in [b"GAFJ01", b"GAFP01"] {
        let bytes = support::image(id, 0, 0, Magic::GameCube);
        let mut image = Bytes::new(&bytes);
        let result = inspect(&mut image, Catalog::upstream());
        assert_eq!(
            result.status,
            Status::UnsupportedVersion,
            "{} must be recognized, not unknown",
            String::from_utf8_lossy(id)
        );
        assert_eq!(result.accepted_disc_index, None);
        assert_eq!(result.metadata.logical_size, bytes.len() as u64);
    }
}

#[test]
fn unaccepted_disc_number_or_revision_of_an_accepted_game_is_wrong_version() {
    // `is_recognized_game` consults the accepted table by game ID alone, so a
    // different disc number or revision is UnsupportedVersion, never UnknownGame.
    for (disc, revision) in [(1u8, 0u8), (0, 1), (2, 3)] {
        let bytes = support::image(b"GAFE01", disc, revision, Magic::GameCube);
        let mut image = Bytes::new(&bytes);
        assert_eq!(
            inspect(&mut image, Catalog::upstream()).status,
            Status::UnsupportedVersion,
            "disc {disc} revision {revision}"
        );
    }
}

#[test]
fn unknown_game_id_is_unknown_game() {
    let bytes = support::image(b"GAFU01", 0, 0, Magic::GameCube);
    let mut image = Bytes::new(&bytes);
    let result = inspect(&mut image, Catalog::upstream());
    assert_eq!(result.status, Status::UnknownGame);
    assert_eq!(result.accepted_disc_index, None);
    // Metadata is still reported: `open_and_inspect` fills it before classifying.
    assert_eq!(result.metadata.game_id, "GAFU01");
}

#[test]
fn missing_magic_is_invalid_image() {
    let bytes = support::image(b"GAFE01", 0, 0, Magic::None);
    let mut image = Bytes::new(&bytes);
    let result = inspect(&mut image, Catalog::upstream());
    assert_eq!(result.status, Status::InvalidImage);
    assert_eq!(
        result.message,
        "Disc header contains no recognized platform magic"
    );
    assert_eq!(result.accepted_disc_index, None);
}

#[test]
fn short_image_is_invalid_image_not_io_error() {
    // Fewer than 0x400 bytes cannot yield a header; upstream pages a 0x400-byte
    // header too, and a missing platform magic is InvalidImage.
    let bytes = vec![0u8; 0x200];
    let mut image = Bytes::new(&bytes);
    let result = inspect(&mut image, Catalog::upstream());
    assert_eq!(result.status, Status::InvalidImage);
    assert_eq!(result.accepted_disc_index, None);
    let empty = [0u8; 0];
    let mut image = Bytes::new(&empty);
    assert_eq!(
        inspect(&mut image, Catalog::upstream()).status,
        Status::InvalidImage
    );
}

#[test]
fn read_failures_map_through_status_from_nod_result() {
    for (error, expected, label) in [
        (ImageError::Io, Status::IOError, "NOD_RESULT_ERR_IO"),
        (
            ImageError::Format,
            Status::InvalidImage,
            "NOD_RESULT_ERR_FORMAT",
        ),
        (ImageError::Failed, Status::Failed, "other"),
    ] {
        let mut image = FailingImage::new(error, 0x400);
        let result = inspect(&mut image, Catalog::upstream());
        assert_eq!(result.status, expected, "{label}");
        assert!(image.attempts > 0, "the adapter was actually consulted");
        assert_eq!(result.message, "Failed to read disc header");
    }
}

#[test]
fn verify_succeeds_when_the_digest_matches() {
    let bytes = synthetic();
    let mut image = Bytes::new(&bytes);
    let result = verify_simple(&mut image, &SYNTHETIC_CATALOG);
    assert_eq!(result.status, Status::Success);
    assert_eq!(result.accepted_disc_index, Some(0));
    assert_eq!(result.metadata.logical_size, bytes.len() as u64);
}

#[test]
fn verify_mismatches_the_upstream_hash_for_a_synthetic_image() {
    // The real catalog expects the upstream GAFE01 hash; a synthetic image with
    // the same header must therefore be a hash mismatch, not a success.
    let bytes = synthetic();
    let mut image = Bytes::new(&bytes);
    let result = verify_simple(&mut image, Catalog::upstream());
    assert_eq!(result.status, Status::HashMismatch);
    assert_eq!(result.accepted_disc_index, Some(0));

    // Flipping one body byte changes the digest and stays a mismatch.
    let mut mutated = synthetic();
    mutated[0x500] ^= 0xFF;
    let mut image = Bytes::new(&mutated);
    assert_eq!(
        verify_simple(&mut image, Catalog::upstream()).status,
        Status::HashMismatch
    );
}

#[test]
fn verify_reports_body_mutation_against_the_synthetic_catalog() {
    let mut mutated = synthetic();
    mutated[0x400] ^= 0xFF;
    let mut image = Bytes::new(&mutated);
    let result = verify_simple(&mut image, &SYNTHETIC_CATALOG);
    assert_eq!(result.status, Status::HashMismatch);
}

#[test]
fn verify_stops_before_hashing_when_the_header_is_not_accepted() {
    let bytes = support::image(b"GAFJ01", 0, 0, Magic::GameCube);
    let counter = AtomicUsize::new(0);
    let mut image = Bytes::new(&bytes);
    let result = verify(
        &mut image,
        Catalog::upstream(),
        &nookery_disc::Never,
        &mut |p| {
            counter.fetch_add(1, Ordering::Relaxed);
            assert_eq!(p, Progress::default());
        },
    );
    assert_eq!(result.status, Status::UnsupportedVersion);
    // Upstream resets progress, opens, and returns early: exactly one sample.
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}

#[test]
fn verify_polls_cancel_before_opening() {
    let bytes = synthetic();
    let flag = CancelFlag::new();
    flag.cancel();
    let counter = AtomicUsize::new(0);
    let mut image = Bytes::new(&bytes);
    let result = verify(&mut image, &SYNTHETIC_CATALOG, &flag, &mut |_| {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(result.status, Status::Canceled);
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}

#[test]
fn verify_polls_cancel_between_chunks() {
    struct CancelAfterFirstChunk<'a> {
        inner: Bytes<'a>,
        flag: Arc<CancelFlag>,
        reads: usize,
    }

    impl DiscImage for CancelAfterFirstChunk<'_> {
        fn logical_size(&self) -> u64 {
            self.inner.logical_size()
        }

        fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ImageError> {
            let read = self.inner.read_at(offset, buf)?;
            self.reads += 1;
            // The header read plus one hash chunk, then ask to stop.
            if self.reads >= 2 {
                self.flag.cancel();
            }
            Ok(read)
        }
    }

    let bytes = synthetic();
    let flag = Arc::new(CancelFlag::new());
    let mut image = CancelAfterFirstChunk {
        inner: Bytes::new(&bytes),
        flag: Arc::clone(&flag),
        reads: 0,
    };
    let result = verify(&mut image, &SYNTHETIC_CATALOG, &*flag, &mut |_| {});
    assert_eq!(result.status, Status::Canceled);
    assert_eq!(image.reads, 2, "stopped after the first hash chunk");
}

#[test]
fn verify_rejects_a_short_image() {
    let bytes = synthetic();
    let mut image = ShortImage {
        bytes: bytes.clone(),
        reported: bytes.len() as u64 + 4096,
    };
    let result = verify_simple(&mut image, &SYNTHETIC_CATALOG);
    assert_eq!(result.status, Status::IOError);
    assert_eq!(
        result.message,
        "Disc image ended before its reported logical size"
    );
}

#[test]
fn verify_rejects_a_backend_that_overreports_bytes_read() {
    // `verify_owned_stream` treats `read > HashBufferSize` as a backend bug and
    // reports `Failed`; it never hashes a clamped count.
    let mut image = OversizedAfterHeaderImage {
        bytes: synthetic(),
        reads: 0,
    };
    let result = verify_simple(&mut image, &SYNTHETIC_CATALOG);
    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.message, "Disc backend returned an oversized read");
    assert_eq!(image.reads, 2, "header read, then the bad hash read");
}

#[test]
fn inspect_rejects_a_header_read_that_overreports() {
    // The header phase is bounded by construction; an adapter that claims more
    // bytes than the 0x400-byte slice is rejected instead of clamping.
    let mut image = OversizedImage { bytes: synthetic() };
    let result = inspect(&mut image, &SYNTHETIC_CATALOG);
    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.message, "Failed to read disc header");
}

#[test]
fn verify_reports_a_midstream_read_failure_as_io_error() {
    // Mirrors `FakeBackend::failReadCall = 1`: the header read and one hash
    // chunk succeed, then the backend errors out.
    let mut image = FailAfterImage {
        bytes: synthetic(),
        fail_at: 1,
        error: ImageError::Io,
        reads: 0,
    };
    let result = verify_simple(&mut image, &SYNTHETIC_CATALOG);
    assert_eq!(result.status, Status::IOError);
    assert_eq!(result.message, "Failed while reading disc image");
    assert_eq!(image.reads, 2, "header read plus one chunk");
}

#[test]
fn verify_maps_a_midstream_format_failure_to_invalid_image() {
    let mut image = FailAfterImage {
        bytes: synthetic(),
        fail_at: 1,
        error: ImageError::Format,
        reads: 0,
    };
    assert_eq!(
        verify_simple(&mut image, &SYNTHETIC_CATALOG).status,
        Status::InvalidImage
    );
}

#[test]
fn reads_stay_bounded_by_the_header_and_chunk_sizes() {
    // Structural bound: the header read is exactly 0x400 bytes and every hash
    // read is at most HASH_BUFFER_SIZE, with offsets advancing by bytes served.
    use nookery_disc::HASH_BUFFER_SIZE;
    use nookery_disc::HEADER_SIZE;

    let bytes = synthetic();
    let total = bytes.len() as u64;
    let mut image = RecordingImage::new(bytes);
    let result = verify_simple(&mut image, &SYNTHETIC_CATALOG);
    assert_eq!(result.status, Status::Success);

    let requests = &image.requests;
    assert_eq!(requests[0], (0, HEADER_SIZE), "one fixed header read");
    assert!(
        image.max_requested <= HASH_BUFFER_SIZE,
        "no read may ask for more than the verification chunk size"
    );
    let mut expected_offset = 0u64;
    for &(offset, _) in &requests[1..] {
        assert_eq!(offset, expected_offset, "offsets are sequential");
        let served = (total - offset).min(HASH_BUFFER_SIZE as u64);
        expected_offset += served;
    }
    assert_eq!(expected_offset, total);
    // Round up: 3 MiB + 7 bytes needs four chunks, plus the terminal 0 read.
    assert_eq!(requests.len(), 1 + 4 + 1);
}

#[test]
fn verify_reports_progress_totals_and_monotonic_reads() {
    let bytes = synthetic();
    let total = bytes.len() as u64;
    let mut samples: Vec<Progress> = Vec::new();
    let mut image = Bytes::new(&bytes);
    let result = verify(
        &mut image,
        &SYNTHETIC_CATALOG,
        &nookery_disc::Never,
        &mut |progress| samples.push(progress),
    );
    assert_eq!(result.status, Status::Success);
    // First: reset. Second: total announced. Then one sample per chunk.
    assert_eq!(samples.first(), Some(&Progress::default()));
    assert_eq!(
        samples[1],
        Progress {
            bytes_read: 0,
            bytes_total: total
        }
    );
    assert!(samples.len() >= 3);
    assert!(
        samples
            .windows(2)
            .all(|pair| pair[0].bytes_read <= pair[1].bytes_read)
    );
    assert_eq!(samples.last().unwrap().bytes_read, total);
    // Every sample after the reset sample knows the total.
    assert!(
        samples[1..]
            .iter()
            .all(|sample| sample.bytes_total == total)
    );
    assert_eq!(samples.last().unwrap().fraction(), 1.0);
}

#[test]
fn progress_fraction_is_clamped_and_zero_total_is_zero() {
    let zero = Progress::default();
    assert_eq!(zero.fraction(), 0.0);
    let over = Progress {
        bytes_read: 20,
        bytes_total: 10,
    };
    assert_eq!(over.fraction(), 1.0);
    let half = Progress {
        bytes_read: 5,
        bytes_total: 10,
    };
    assert_eq!(half.fraction(), 0.5);
}

#[test]
fn inspect_error_is_the_status_mapping() {
    let accepted = synthetic();
    let mut image = Bytes::new(&accepted);
    assert_eq!(
        inspect_error(&mut image, Catalog::upstream()),
        nookery_disc::ValidationError::Success
    );
    let unknown = support::image(b"GAFU01", 0, 0, Magic::GameCube);
    let mut image = Bytes::new(&unknown);
    assert_eq!(
        inspect_error(&mut image, Catalog::upstream()),
        nookery_disc::ValidationError::WrongGame
    );
    let mut image = FailingImage::new(ImageError::Io, 0);
    assert_eq!(
        inspect_error(&mut image, Catalog::upstream()),
        validation_error(Status::IOError)
    );
}

#[test]
fn undecodable_platform_containers_are_reported_as_invalid_image_not_a_hash_mismatch() {
    // Platform-backed container decoding (RVZ/GCZ/WBFS/WIA/NFS/TGC) lives in
    // `nod`, which is not in this repository. The contract is that an adapter
    // which cannot decode the container reports
    // `ImageError::Format`, which must surface as `InvalidImage` — never as
    // `HashMismatch`, which would tell the user a valid image is corrupt.
    let mut image = FailingImage::new(ImageError::Format, 0x400);
    assert_eq!(
        inspect_error(&mut image, Catalog::upstream()),
        nookery_disc::ValidationError::InvalidImage
    );

    // A raw compressed container handed over without decoding must fail the
    // platform-magic check, not be hashed as if it were a logical image.
    let container = vec![0x52u8; 0x400]; // 'R' fill: an RVZ-ish blob, no disc magic
    let mut image = Bytes::new(&container);
    let result = verify_simple(&mut image, Catalog::upstream());
    assert_eq!(result.status, Status::InvalidImage);
    assert_eq!(
        validation_error(result.status),
        nookery_disc::ValidationError::InvalidImage
    );
    // The header decoded (0x400 bytes were available) but no platform matched,
    // so no hash work happened and no accepted record was claimed.
    assert_eq!(result.accepted_disc_index, None);
}
