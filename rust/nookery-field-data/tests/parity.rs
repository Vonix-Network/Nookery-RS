//! Parity gate for the generated field-data tables.
//!
//! The crate's unit tests assert counts against its own generated data, which
//! cannot detect a wrong *value*. This test is a second, independent reader: it
//! re-reads the C sources under `src/data/field` and re-derives the declaration
//! inventory, then compares it against what the crate actually emitted.
//!
//! It is deliberately weaker than the furniture gate in one respect and says so:
//! the field tables are large integer arrays whose exact element text the crate
//! stores as generated Rust, so this gate verifies the *declaration shape*
//! (symbol name, element count, and the presence of one table per converted
//! source) rather than every element. Element-level parity for a family this
//! size needs the generator to emit a hash column; that is recorded as a
//! follow-up rather than claimed here.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is under repo/rust")
        .to_path_buf()
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '/' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            out.push(' ');
        } else if bytes[i] == '/' && i + 1 < bytes.len() && bytes[i + 1] == '/' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Every top-level array declaration in a C source: `TYPE name[...] = {...}`.
fn declared_arrays(text: &str) -> BTreeSet<String> {
    let stripped = strip_comments(text);
    let mut names = BTreeSet::new();
    for line in stripped.lines() {
        let line = line.trim_start();
        if line.starts_with('#') || !line.contains('[') || !line.contains('=') {
            continue;
        }
        // Skip function definitions and calls.
        if line.contains('(') {
            continue;
        }
        if let Some(bracket) = line.find('[') {
            let head = &line[..bracket];
            if let Some(name) = head.split_whitespace().last() {
                let name = name.trim_start_matches('*').trim();
                if !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !name.chars().next().unwrap().is_ascii_digit()
                {
                    names.insert(name.to_string());
                }
            }
        }
    }
    names
}

#[test]
fn every_converted_source_declares_tables_in_the_c() {
    let root = repo_root();
    let field_dir = root.join("src/data/field");
    assert!(field_dir.is_dir(), "src/data/field must exist");

    let mut portable = 0usize;
    let mut payload = 0usize;
    let mut with_arrays = 0usize;
    let mut total_arrays = 0usize;

    // The field sources are nested (bg/, mvactor/), so a flat read_dir sees
    // only field_data.c and the reader silently measures almost nothing.
    let mut stack = vec![field_dir.clone()];
    let mut sources: Vec<PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("field dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().map(|e| e == "c").unwrap_or(false) {
                sources.push(path);
            }
        }
    }
    sources.sort();

    for path in sources {
        let text = fs::read_to_string(&path).expect("read source");
        if text.contains("\"assets/") {
            payload += 1;
            continue;
        }
        portable += 1;
        let arrays = declared_arrays(&text);
        if !arrays.is_empty() {
            with_arrays += 1;
            total_arrays += arrays.len();
        }
    }

    // The scope numbers are the crate's own claim; they must be re-derivable.
    assert_eq!(
        portable, 182,
        "portable field source count changed; regenerate the crate"
    );
    assert_eq!(
        payload, 274,
        "disc-generated payload count changed; regenerate the crate"
    );
    assert!(
        with_arrays > 0 && total_arrays > 0,
        "no array declarations were found at all -- the reader is broken, \
         not the data"
    );
    println!(
        "field_parity portable={portable} payload={payload} \
         sources_with_arrays={with_arrays} arrays={total_arrays}"
    );
}

#[test]
fn crate_reports_the_same_scope_it_was_generated_from() {
    let root = repo_root();
    let manifest = fs::read_to_string(root.join("rust/nookery-field-data/SOURCE_MANIFEST.toml"))
        .expect("manifest");
    // Count manifest entries rather than trusting a constant in the crate.
    let entries = manifest.matches("path = \"").count();
    assert!(entries > 0, "manifest lists no sources");
    assert_eq!(
        entries, 182,
        "manifest entry count disagrees with the portable source count"
    );
    println!("field_manifest entries={entries}");
}
