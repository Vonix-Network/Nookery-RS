//! Runtime differential test against the independent Rust-only RARC helper.
//!
//! No archive fixture is committed. Set `NOOKERY_RARC_FIXTURES` to a
//! platform path list containing raw `.arc` files.

use std::path::{Path, PathBuf};
use std::process::Command;

use nookery_disc::RarcArchive;

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn helper_path() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?.parent()?;
    let path = root.join("tools/rarc-oracle/build/rarc_oracle");
    path.is_file().then_some(path)
}

fn helper_manifest(output: &str) -> Vec<(String, String, usize, u64)> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            (fields.next() == Some("FILE")).then(|| {
                let path = fields.next().unwrap_or_default().to_owned();
                let name = fields.next().unwrap_or_default().to_owned();
                let size = fields
                    .next()
                    .unwrap_or_default()
                    .parse()
                    .unwrap_or(usize::MAX);
                let digest =
                    u64::from_str_radix(fields.next().unwrap_or_default(), 16).unwrap_or(u64::MAX);
                (path, name, size, digest)
            })
        })
        .collect()
}

#[test]
fn rust_matches_independent_manifest_for_every_supplied_fixture() {
    let Some(helper) = helper_path() else {
        eprintln!("SKIP: build tools/rarc-oracle first (make -C tools/rarc-oracle)");
        return;
    };
    let Some(fixtures) = std::env::var_os("NOOKERY_RARC_FIXTURES") else {
        eprintln!("SKIP: set NOOKERY_RARC_FIXTURES to a path-list of raw .arc files");
        return;
    };

    let mut compared = 0usize;
    for path in std::env::split_paths(&fixtures) {
        if !path.is_file() {
            eprintln!("SKIP: not a file: {}", path.display());
            continue;
        }
        let bytes = std::fs::read(&path).expect("fixture reads");
        let output = Command::new(&helper)
            .arg(&path)
            .output()
            .expect("RARC helper runs");
        assert!(
            output.status.success(),
            "helper rejected {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = helper_manifest(&String::from_utf8_lossy(&output.stdout));
        let archive = RarcArchive::parse(&bytes)
            .unwrap_or_else(|error| panic!("Rust rejected {}: {error}", path.display()));
        let actual: Vec<_> = archive
            .files()
            .into_iter()
            .map(|file| {
                (
                    file.path().to_owned(),
                    file.name().to_owned(),
                    file.size(),
                    fnv1a64(file.data()),
                )
            })
            .collect();
        assert_eq!(actual, expected, "manifest differs for {}", path.display());
        println!("MATCH {} files={}", path.display(), actual.len());
        compared += 1;
    }
    assert!(compared > 0, "no RARC fixtures were compared");
}
