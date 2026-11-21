//! Differential harness: compare the Rust Yaz0 decoder against the C oracle on
//! whatever `.szs` files the caller supplies.
//!
//! This is the real gate — it runs both decoders over actual game data and
//! requires byte-identical output plus an identical rejection reason. Game data
//! is never committed; paths are supplied at run time as arguments.
//!
//! Run with:
//!   cargo test -p nookery-disc --test yaz0_differential -- --nocapture
//! after building `tools/yaz0-oracle`, and pass file paths via the
//! `NOOKERY_YAZ0_FIXTURES` environment variable (colon-separated).

use std::path::Path;
use std::process::Command;

use nookery_disc::yaz0;

/// Matches the oracle harness's FNV-1a 64-bit digest.
fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Locate the oracle binary relative to the workspace root.
fn oracle_path() -> Option<std::path::PathBuf> {
    // CARGO_MANIFEST_DIR is <repo>/rust/nookery-disc.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent()?.parent()?;
    let oracle = root.join("tools/yaz0-oracle/build/yaz0_oracle");
    oracle.is_file().then_some(oracle)
}

/// Parse the oracle's `key=value` output.
fn parse_oracle(output: &str) -> (String, u64, u64) {
    let mut result = String::new();
    let mut decoded_size = 0u64;
    let mut digest = 0u64;
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("result=") {
            result = value.trim().to_owned();
        } else if let Some(value) = line.strip_prefix("decoded_size=") {
            decoded_size = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("decoded_fnv1a64=") {
            digest = u64::from_str_radix(value.trim(), 16).unwrap_or(0);
        }
    }
    (result, decoded_size, digest)
}

#[test]
fn rust_and_c_agree_on_every_supplied_fixture() {
    let Some(oracle) = oracle_path() else {
        eprintln!("SKIP: build tools/yaz0-oracle first (make -C tools/yaz0-oracle)");
        return;
    };
    let Some(fixtures) = std::env::var_os("NOOKERY_YAZ0_FIXTURES") else {
        eprintln!("SKIP: set NOOKERY_YAZ0_FIXTURES to colon-separated .szs paths");
        return;
    };

    let mut compared = 0usize;
    for path in std::env::split_paths(&fixtures) {
        if !path.is_file() {
            eprintln!("SKIP: not a file: {}", path.display());
            continue;
        }
        let bytes = std::fs::read(&path).expect("fixture reads");

        let output = Command::new(&oracle)
            .arg(&path)
            .output()
            .expect("oracle runs");
        assert!(
            output.status.success(),
            "oracle failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let (c_result, c_size, c_digest) = parse_oracle(&stdout);

        let rust = yaz0::decode(&bytes, usize::MAX);
        match (c_result.as_str(), &rust) {
            ("OK", Ok(decoded)) => {
                assert_eq!(
                    decoded.len() as u64,
                    c_size,
                    "{}: decoded size differs",
                    path.display()
                );
                let rust_digest = fnv1a64(decoded);
                assert_eq!(
                    rust_digest,
                    c_digest,
                    "{}: decoded bytes differ (rust {rust_digest:016x} vs c {c_digest:016x})",
                    path.display()
                );
                println!(
                    "MATCH {} size={} digest={:016x}",
                    path.display(),
                    c_size,
                    c_digest
                );
            }
            ("OK", Err(error)) => {
                panic!(
                    "{}: C accepted but Rust rejected with {error:?}",
                    path.display()
                );
            }
            (c_reason, Ok(decoded)) => {
                panic!(
                    "{}: C rejected ({c_reason}) but Rust produced {} bytes",
                    path.display(),
                    decoded.len()
                );
            }
            (c_reason, Err(error)) => {
                // Rejection reasons must correspond, not merely both fail.
                let mapped = match error {
                    yaz0::Yaz0Error::TooShort => "TOO_SHORT",
                    yaz0::Yaz0Error::BadMagic => "BAD_MAGIC",
                    yaz0::Yaz0Error::BadDistance => "BAD_DISTANCE",
                    yaz0::Yaz0Error::ShortOutput => "SHORT_OUTPUT",
                    yaz0::Yaz0Error::SizeOverflow | yaz0::Yaz0Error::TooLarge { .. } => "LIMIT",
                };
                assert_eq!(
                    mapped,
                    c_reason,
                    "{}: rejection reason differs",
                    path.display()
                );
                println!("MATCH {} rejected={c_reason}", path.display());
            }
        }
        compared += 1;
    }
    assert!(compared > 0, "no fixtures were compared");
    println!("compared {compared} fixture(s)");
}
