//! Differential test against the real `libc64` C sources and repository headers.

use std::path::{Path, PathBuf};
use std::process::Command;

use nookery_libc64::{SleepOperation, c_sleep_operations, facos, fatan2, libc64_fsqrt};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is under repo/rust")
        .to_path_buf()
}

fn oracle_output() -> String {
    let root = repo_root();
    let work = tempfile_dir();
    let binary = work.join("oracle");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle.c");
    let status = Command::new(std::env::var_os("CC").unwrap_or_else(|| "gcc".into()))
        .args([
            "-std=c11",
            "-O0",
            "-DTARGET_PC=1",
            "-DVERSION=0",
            "-I",
            "include",
            "-I",
            "libs/dolphin/include",
            "-w",
            "-o",
        ])
        .arg(&binary)
        .arg(&source)
        .arg(root.join("src/static/libc64/math64.c"))
        .arg(root.join("src/static/libc64/sleep.c"))
        .arg("-lm")
        .current_dir(&root)
        .status()
        .expect("compile the C oracle");
    assert!(status.success(), "C oracle compilation failed");

    let output = Command::new(&binary).output().expect("run the C oracle");
    assert!(
        output.status.success(),
        "C oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&work);
    String::from_utf8(output.stdout).expect("C oracle output is UTF-8")
}

fn tempfile_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("nookery-libc64-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create oracle temp directory");
    path
}

fn rust_output() -> String {
    let mut output = String::new();
    use std::fmt::Write as _;
    use std::mem::{offset_of, size_of};

    writeln!(
        &mut output,
        "layout.queue {} {} {} {} {} {} {}",
        size_of::<nookery_libc64::OsMessageQueue>(),
        offset_of!(nookery_libc64::OsMessageQueue, queue_send),
        offset_of!(nookery_libc64::OsMessageQueue, queue_receive),
        offset_of!(nookery_libc64::OsMessageQueue, msg_array),
        offset_of!(nookery_libc64::OsMessageQueue, msg_count),
        offset_of!(nookery_libc64::OsMessageQueue, first_index),
        offset_of!(nookery_libc64::OsMessageQueue, used_count),
    )
    .unwrap();
    writeln!(
        &mut output,
        "layout.alarm {} {} {} {} {} {} {}",
        size_of::<nookery_libc64::OsAlarm>(),
        offset_of!(nookery_libc64::OsAlarm, handler),
        offset_of!(nookery_libc64::OsAlarm, fire),
        offset_of!(nookery_libc64::OsAlarm, prev),
        offset_of!(nookery_libc64::OsAlarm, next),
        offset_of!(nookery_libc64::OsAlarm, period),
        offset_of!(nookery_libc64::OsAlarm, start),
    )
    .unwrap();
    writeln!(
        &mut output,
        "layout.timer {} {} {} {} {} {} {} {}",
        size_of::<nookery_libc64::OsTimer>(),
        offset_of!(nookery_libc64::OsTimer, alarm),
        offset_of!(nookery_libc64::OsTimer, next),
        offset_of!(nookery_libc64::OsTimer, prev),
        offset_of!(nookery_libc64::OsTimer, interval),
        offset_of!(nookery_libc64::OsTimer, value),
        offset_of!(nookery_libc64::OsTimer, mq),
        offset_of!(nookery_libc64::OsTimer, msg),
    )
    .unwrap();

    let values = [
        0x00000000u32,
        0x80000000,
        0x3f800000,
        0xbf800000,
        0x40000000,
        0xc0000000,
        0x7f800000,
    ];
    for &x_bits in &values {
        let x = f32::from_bits(x_bits);
        let mut line = |name: &str, bits: u32, value: f32| {
            writeln!(&mut output, "{name} {bits:08x} {:08x}", value.to_bits()).unwrap();
        };
        line("fsqrt", x_bits, libc64_fsqrt(x));
        line("facos", x_bits, facos(x));
        for &y_bits in &values {
            line("fatan2", x_bits ^ y_bits, fatan2(x, f32::from_bits(y_bits)));
        }
    }

    for countdown in [-1, 0, 1, 7, i64::MAX] {
        for operation in c_sleep_operations(countdown) {
            match operation {
                SleepOperation::CreateMessageQueue {
                    msg_count,
                    message_buffered,
                } => writeln!(
                    &mut output,
                    "sleep.create {} {msg_count}",
                    i32::from(message_buffered)
                )
                .unwrap(),
                SleepOperation::SetTimer {
                    countdown,
                    interval,
                    message_is_null,
                } => writeln!(
                    &mut output,
                    "sleep.set {countdown} {interval} {}",
                    i32::from(message_is_null)
                )
                .unwrap(),
                SleepOperation::ReceiveMessage {
                    flags,
                    output_is_null,
                } => writeln!(
                    &mut output,
                    "sleep.recv {} {flags}",
                    i32::from(output_is_null)
                )
                .unwrap(),
            }
        }
    }
    output
}

#[test]
fn rust_matches_real_math64_and_sleep_sources() {
    assert_eq!(rust_output(), oracle_output());
}
