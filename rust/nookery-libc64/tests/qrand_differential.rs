//! Differential test for the `qrand` slice against the real
//! `src/static/libc64/qrand.c`.
//!
//! The C source is compiled directly (never transcribed), driven by the
//! existing `parity/driver.c`, and its stdout is compared byte-for-byte with a
//! sequence produced by the Rust port. Six seeds are exercised, including 0,
//! 1, `UINT32_MAX`, `0x80000000` and `0xfffffffe`, and each seed runs both the
//! integer stream and the interleaved `fqrand`/`fqrand2` float stream so a port
//! that advances the state the wrong number of times cannot pass.

use std::path::{Path, PathBuf};
use std::process::Command;

use nookery_libc64::qrand::QRand;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is under repo/rust")
        .to_path_buf()
}

fn tempfile_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("nookery-qrand-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create oracle temp directory");
    path
}

fn oracle_output() -> String {
    let root = repo_root();
    let work = tempfile_dir();
    let binary = work.join("qrand_oracle");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/driver.c");

    let status = Command::new(std::env::var_os("CC").unwrap_or_else(|| "gcc".into()))
        .args([
            "-std=c11",
            "-O0",
            "-DTARGET_PC=1",
            "-DVERSION=0",
            // The test-only shim must come FIRST: the shipped include/types.h
            // maps u32 to `unsigned long`, which is 64-bit on the host, so the
            // C recurrence would not wrap at 32 bits and would diverge from the
            // real console arithmetic. The shim pins the console widths without
            // touching the shipped source.
            "-I",
            "rust/nookery-libc64/parity/include",
            "-I",
            "include",
            "-I",
            "libs/dolphin/include",
            "-w",
            "-o",
        ])
        .arg(&binary)
        .arg(&source)
        .arg(root.join("src/static/libc64/qrand.c"))
        .current_dir(&root)
        .status()
        .expect("compile the C qrand oracle");
    assert!(status.success(), "C qrand oracle compilation failed");

    let output = Command::new(&binary)
        .output()
        .expect("run the C qrand oracle");
    assert!(
        output.status.success(),
        "C qrand oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&work);
    String::from_utf8(output.stdout).expect("C oracle output is UTF-8")
}

/// Rebuild the same six-seed, two-stream report from the Rust port.
fn rust_output() -> String {
    let mut output = String::new();
    use std::fmt::Write as _;

    let seeds: [u32; 6] = [0, 1, u32::MAX, 0x8000_0000, 0xffff_fffe, 0x1234_5678];

    for seed in seeds {
        let mut rng = QRand::new(seed);
        write!(&mut output, "u {seed:08x}").unwrap();
        for _ in 0..16 {
            write!(&mut output, " {:08x}", rng.qrand()).unwrap();
        }
        output.push('\n');

        let mut rng = QRand::new(seed);
        write!(&mut output, "f {seed:08x}").unwrap();
        for step in 0..12u32 {
            let bits = if step & 1 == 1 {
                rng.fqrand2().to_bits()
            } else {
                rng.fqrand().to_bits()
            };
            write!(&mut output, " {bits:08x}").unwrap();
        }
        output.push('\n');
    }
    output
}

#[test]
fn qrand_rust_matches_the_real_c_source() {
    let oracle = oracle_output();
    let rust = rust_output();

    assert!(
        !oracle.trim().is_empty(),
        "the C oracle produced no output; the gate would compare nothing"
    );

    let oracle_lines: Vec<&str> = oracle.lines().collect();
    let rust_lines: Vec<&str> = rust.lines().collect();
    assert_eq!(
        oracle_lines.len(),
        rust_lines.len(),
        "record count differs: C={} Rust={}",
        oracle_lines.len(),
        rust_lines.len()
    );

    for (index, (c_line, rust_line)) in oracle_lines.iter().zip(&rust_lines).enumerate() {
        assert_eq!(
            c_line, rust_line,
            "record {index} differs\n  C:    {c_line}\n  Rust: {rust_line}"
        );
    }
    println!("qrand records compared={}", oracle_lines.len());
}
