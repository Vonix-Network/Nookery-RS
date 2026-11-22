//! Differential tests against the exact `src/static/libc64/qrand.c` source.

use std::path::{Path, PathBuf};
use std::process::Command;

use nookery_libc64::QrandState;

const SEEDS: [u32; 6] = [0, 1, u32::MAX, 0x8000_0000, 0xffff_fffe, 0x1234_5678];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_owned()
}

fn rust_transcript() -> String {
    let mut output = String::new();
    for seed in SEEDS {
        let mut state = QrandState::new(seed);
        output.push_str(&format!("u {seed:08x}"));
        for _ in 0..16 {
            output.push_str(&format!(" {:08x}", state.qrand()));
        }
        output.push('\n');

        let mut state = QrandState::new(seed);
        output.push_str(&format!("f {seed:08x}"));
        for i in 0..12 {
            let bits = if i & 1 == 0 {
                state.fqrand().to_bits()
            } else {
                state.fqrand2().to_bits()
            };
            output.push_str(&format!(" {bits:08x}"));
        }
        output.push('\n');
    }
    output
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nookery-libc64-oracle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&path).expect("create oracle temp directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn transcript_matches_the_unmodified_qrand_source() {
    let root = workspace_root();
    let source = root.join("src/static/libc64/qrand.c");
    let header_root = root.join("include");
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/driver.c");
    let parity_include = Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/include");
    assert!(
        source.is_file(),
        "missing oracle source: {}",
        source.display()
    );
    assert!(
        driver.is_file(),
        "missing differential driver: {}",
        driver.display()
    );

    let temp = TempDir::new();
    let binary = temp.0.join("qrand_oracle");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let compile = Command::new(&compiler)
        .args(["-std=c11", "-O0", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(&parity_include)
        .arg("-I")
        .arg(&header_root)
        .arg("-o")
        .arg(&binary)
        .arg(&driver)
        .arg(&source)
        .output()
        .unwrap_or_else(|error| panic!("run C compiler {compiler:?}: {error}"));
    assert!(
        compile.status.success(),
        "oracle compilation failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let oracle = Command::new(&binary).output().expect("run qrand oracle");
    assert!(
        oracle.status.success(),
        "qrand oracle failed:\n{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let oracle = String::from_utf8(oracle.stdout).expect("oracle is UTF-8");
    assert_eq!(oracle, rust_transcript());
}
