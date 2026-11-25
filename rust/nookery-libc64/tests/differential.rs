//! Differential test against the real `libc64` C sources and repository headers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

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

fn allocator_oracle_output() -> String {
    let root = repo_root();
    let work = tempfile_dir();
    let binary = work.join("allocator-oracle");
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/allocator_driver.c");
    let status = Command::new(std::env::var_os("CC").unwrap_or_else(|| "gcc".into()))
        .args([
            "-std=c11",
            "-O0",
            "-DTARGET_PC=1",
            "-DVERSION=0",
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
        .arg(&driver)
        .current_dir(&root)
        .status()
        .expect("compile the allocator C oracle");
    assert!(status.success(), "allocator C oracle compilation failed");

    let output = Command::new(&binary)
        .output()
        .expect("run the allocator C oracle");
    assert!(
        output.status.success(),
        "allocator C oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&work);
    String::from_utf8(output.stdout).expect("allocator C oracle output is UTF-8")
}

fn format_oracle_output() -> String {
    let root = repo_root();
    let work = tempfile_dir();
    let binary = work.join("aprintf-oracle");
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/aprintf_driver.c");
    let status = Command::new(std::env::var_os("CC").unwrap_or_else(|| "gcc".into()))
        .args([
            "-std=c11",
            "-O0",
            "-DTARGET_PC=1",
            "-DVERSION=0",
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
        .arg(&driver)
        .current_dir(&root)
        .status()
        .expect("compile the aprintf C oracle");
    assert!(status.success(), "aprintf C oracle compilation failed");

    let output = Command::new(&binary)
        .output()
        .expect("run the aprintf C oracle");
    assert!(
        output.status.success(),
        "aprintf C oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&work);
    String::from_utf8(output.stdout).expect("aprintf C oracle output is UTF-8")
}

fn sprintf_oracle_output() -> String {
    let root = repo_root();
    let work = tempfile_dir();
    let binary = work.join("sprintf-oracle");
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("parity/sprintf_driver.c");
    let status = Command::new(std::env::var_os("CC").unwrap_or_else(|| "gcc".into()))
        .args([
            "-std=c11",
            "-O0",
            "-DTARGET_PC=1",
            "-DVERSION=0",
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
        .arg(&driver)
        .current_dir(&root)
        .status()
        .expect("compile the sprintf C oracle");
    assert!(status.success(), "sprintf C oracle compilation failed");

    let output = Command::new(&binary)
        .output()
        .expect("run the sprintf C oracle");
    assert!(
        output.status.success(),
        "sprintf C oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&work);
    String::from_utf8(output.stdout).expect("sprintf C oracle output is UTF-8")
}

fn tempfile_dir() -> PathBuf {
    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "nookery-libc64-oracle-{}-{sequence}",
        std::process::id()
    ));
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

fn allocator_rust_output() -> String {
    use std::fmt::Write as _;

    let mut arena = nookery_libc64::Arena::new(2048, 3);
    let base = arena.backing_ptr() as usize;
    let mut output = String::new();
    let mut panic_count = 0;
    let mut panic_line = 0;

    let print_state = |output: &mut String,
                       tag: &str,
                       arena: &nookery_libc64::Arena,
                       result: *mut u8,
                       panic_count: i32,
                       panic_line: i32| {
        let (max_free, total_free, used) = arena.analyze();
        let result_offset = if result.is_null() {
            -1
        } else {
            (result as usize - base) as i32
        };
        writeln!(
            output,
            "{tag} result={result_offset} panic={panic_count}:{panic_line} head={} free={max_free}:{total_free} used={used}",
            arena
                .snapshot()
                .first()
                .map_or(-1, |block| block.offset as i32),
        )
        .unwrap();
        for block in arena.snapshot() {
            writeln!(
                output,
                "  block={} next={} prev={} size={} free={} magic={}",
                block.offset,
                block.next.map_or(-1, |offset| offset as i32),
                block.prev.map_or(-1, |offset| offset as i32),
                block.size,
                i32::from(block.free),
                block.magic,
            )
            .unwrap();
        }
    };

    print_state(
        &mut output,
        "init",
        &arena,
        std::ptr::null_mut(),
        panic_count,
        panic_line,
    );
    let first = arena.malloc(17);
    print_state(
        &mut output,
        "malloc17",
        &arena,
        first,
        panic_count,
        panic_line,
    );
    let aligned = arena.malloc_align(33, 256);
    print_state(
        &mut output,
        "align33_256",
        &arena,
        aligned,
        panic_count,
        panic_line,
    );
    let zero = arena.malloc(0);
    print_state(
        &mut output,
        "malloc0",
        &arena,
        zero,
        panic_count,
        panic_line,
    );
    let exhausted = arena.malloc(0x7fff_ffff);
    print_state(
        &mut output,
        "exhausted",
        &arena,
        exhausted,
        panic_count,
        panic_line,
    );

    let _ = arena.free(first);
    print_state(
        &mut output,
        "free_first",
        &arena,
        std::ptr::null_mut(),
        panic_count,
        panic_line,
    );
    let status = arena.free(first);
    if status == nookery_libc64::AllocatorStatus::DoubleFree {
        panic_count += 1;
        panic_line = 743;
    }
    print_state(
        &mut output,
        "double_first",
        &arena,
        std::ptr::null_mut(),
        panic_count,
        panic_line,
    );
    let status = unsafe { arena.free(aligned.add(1)) };
    if status == nookery_libc64::AllocatorStatus::InvalidFree {
        panic_count += 1;
        panic_line = 738;
    }
    print_state(
        &mut output,
        "misaligned",
        &arena,
        std::ptr::null_mut(),
        panic_count,
        panic_line,
    );
    let _ = arena.free(aligned);
    print_state(
        &mut output,
        "free_aligned",
        &arena,
        std::ptr::null_mut(),
        panic_count,
        panic_line,
    );
    let reverse = arena.malloc_reverse(64);
    print_state(
        &mut output,
        "malloc_reverse64",
        &arena,
        reverse,
        panic_count,
        panic_line,
    );
    let _ = arena.free(std::ptr::null_mut());
    print_state(
        &mut output,
        "free_null",
        &arena,
        std::ptr::null_mut(),
        panic_count,
        panic_line,
    );
    output
}

fn format_rust_output() -> String {
    use nookery_libc64::{FormatArg, format_string};

    let first = format_string(
        "oak:%s:%d:%u:%04x:%.2f",
        &[
            FormatArg::String("leaf"),
            FormatArg::Signed(-7),
            FormatArg::Unsigned(42),
            FormatArg::Unsigned(0x2a),
            FormatArg::Float(3.5),
        ],
    )
    .unwrap();
    let second = format_string("percent:%%:%ld", &[FormatArg::Signed(-12)]).unwrap();
    format!(
        "{} {} {}\n{} {} {}\n",
        first.len(),
        first.len(),
        first,
        second.len(),
        second.len(),
        second
    )
}

fn sprintf_rust_output() -> String {
    use nookery_libc64::{FormatArg, sprintf};

    let mut first = [0; 128];
    let first_count = sprintf(
        &mut first,
        "oak:%s:%d:%u:%04x:%.2f",
        &[
            FormatArg::String("leaf"),
            FormatArg::Signed(-7),
            FormatArg::Unsigned(42),
            FormatArg::Unsigned(0x2a),
            FormatArg::Float(3.5),
        ],
    )
    .unwrap();
    let mut second = [0; 128];
    let second_count = sprintf(&mut second, "percent:%%:%ld", &[FormatArg::Signed(-12)]).unwrap();
    let first = std::str::from_utf8(&first[..first_count as usize]).unwrap();
    let second = std::str::from_utf8(&second[..second_count as usize]).unwrap();
    format!("{first_count} {first}\n{second_count} {second}\n")
}

#[test]
fn rust_matches_real_math64_and_sleep_sources() {
    assert_eq!(rust_output(), oracle_output());
}

#[test]
fn rust_matches_real_boot_allocator_source_step_by_step() {
    assert_eq!(allocator_rust_output(), allocator_oracle_output());
}

#[test]
fn rust_matches_real_target_pc_aprintf_source() {
    assert_eq!(format_rust_output(), format_oracle_output());
}

#[test]
fn rust_matches_real_sprintf_wrapper_source() {
    assert_eq!(sprintf_rust_output(), sprintf_oracle_output());
}
