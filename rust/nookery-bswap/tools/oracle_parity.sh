#!/bin/sh
# Differential parity check: nookery-bswap vs the C oracle it was ported from.
#
# Compiles the *verbatim* oracle sources (an excerpt of pc/src/pc_save_bswap.c
# and pc/include/pc_bswap.h, copied at run time, never edited) and diffs their
# output against the Rust crate's, for every byte of every repacker and a
# spread of swap/load/store vectors.
#
# Usage: tools/oracle_parity.sh   (from rust/nookery-bswap, or any directory)
# Exit status 0 means byte-identical output. Requires gcc + cargo.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
crate=$(CDPATH= cd -- "$here/.." && pwd)
repo=$(CDPATH= cd -- "$crate/../.." && pwd)

oracle_c="$repo/pc/src/pc_save_bswap.c"
oracle_h="$repo/pc/include/pc_bswap.h"
[ -f "$oracle_c" ] || { echo "missing oracle source: $oracle_c" >&2; exit 2; }
[ -f "$oracle_h" ] || { echo "missing oracle header: $oracle_h" >&2; exit 2; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

# The leaf/directional helper block: from the reverse_bits8 comment to the end
# of repack_u8_4_1_3. Line numbers are re-derived each run, so oracle edits that
# move the block are picked up rather than silently ignored.
start=$(grep -n '^/\* Reverse all 8 bits' "$oracle_c" | cut -d: -f1)
start=$((start - 1)) # include the "u8 bitfield repack helpers" banner above it
end=$(grep -n '^static void swap_lbRTC_time' "$oracle_c" | cut -d: -f1)
[ -n "$start" ] && [ -n "$end" ] || { echo "cannot locate helper block" >&2; exit 2; }
sed -n "${start},$((end - 1))p" "$oracle_c" > "$work/repackers.inc"

cat > "$work/oracle.c" <<'EOF'
#include <stdint.h>
#include <stdio.h>
typedef uint8_t u8;
/* Mirrors the enum in pc/include/pc_save_bswap.h; must precede the excerpt. */
typedef enum { PC_BSWAP_TO_BE, PC_BSWAP_FROM_BE } pc_bswap_dir_t;
#include "repackers.inc"

static void emit(const char* name, void (*fn)(u8*, pc_bswap_dir_t), u8 v) {
    u8 from_be = v, to_be = v;
    fn(&from_be, PC_BSWAP_FROM_BE);
    fn(&to_be, PC_BSWAP_TO_BE);
    printf("%s %u %u\n", name, from_be, to_be);
}

int main(void) {
    for (unsigned v = 0; v < 256; v++) {
        u8 b = (u8)v;
        printf("reverse_bits8 %u %u\n", reverse_bits8(b), reverse_bits8(b));
        emit("1_7", repack_u8_1_7, b);
        emit("7_1", repack_u8_7_1, b);
        emit("5_3", repack_u8_5_3, b);
        emit("2222", repack_u8_2222, b);
        emit("1_1_6", repack_u8_1_1_6, b);
        emit("11111_3", repack_u8_11111_3, b);
        emit("111_5", repack_u8_111_5, b);
        emit("4_1_3", repack_u8_4_1_3, b);
    }
    return 0;
}
EOF

cp "$oracle_h" "$work/nookery_pc_bswap.h"
cat > "$work/endian.c" <<'EOF'
#include <stdint.h>
#include <stdio.h>
#include "nookery_pc_bswap.h"

int main(void) {
    const uint16_t v16[] = {0, 1, 0x00FF, 0xFF00, 0x1234, 0x8000, 0xFFFF};
    const uint32_t v32[] = {0, 1, 0x80000000u, 0x12345678u, 0xDEADBEEFu, 0xFFFFFFFFu};
    for (unsigned i = 0; i < sizeof v16 / sizeof *v16; i++)
        printf("swap16 %u %u\n", v16[i], (unsigned)pc_bswap16(v16[i]));
    for (unsigned i = 0; i < sizeof v32 / sizeof *v32; i++)
        printf("swap32 %u %u\n", v32[i], (unsigned)pc_bswap32(v32[i]));

    uint8_t buf[8] = {0x12, 0x34, 0xAB, 0xCD, 0, 0, 0, 0};
    printf("load_be16 %u\n", (unsigned)pc_load_be16(buf));
    uint8_t word[4] = {0x12, 0x34, 0x56, 0x78};
    printf("load_be32 %u\n", (unsigned)pc_load_be32(word));

    uint8_t store[4] = {0, 0, 0, 0};
    pc_store_be16(store, 0x1234);
    printf("store_be16 %u %u %u %u\n", store[0], store[1], store[2], store[3]);
    pc_store_be32(store, 0x12345678u);
    printf("store_be32 %u %u %u %u\n", store[0], store[1], store[2], store[3]);

    uint8_t a16[8] = {0x12, 0x34, 0xAB, 0xCD, 0x00, 0xFF, 0x55, 0xAA};
    pc_bswap16_array(a16, 4);
    printf("swap16_array4");
    for (unsigned i = 0; i < 8; i++) printf(" %u", a16[i]);
    printf("\n");

    uint8_t a32[8] = {1, 2, 3, 4, 5, 6, 7, 8};
    pc_bswap32_array(a32, 2);
    printf("swap32_array2");
    for (unsigned i = 0; i < 8; i++) printf(" %u", a32[i]);
    printf("\n");

    uint8_t zero[4] = {0x12, 0x34, 0x56, 0x78};
    pc_bswap16_array(zero, 0);
    printf("swap16_array0 %u %u %u %u\n", zero[0], zero[1], zero[2], zero[3]);
    return 0;
}
EOF

mkdir -p "$work/parity/src"
cat > "$work/parity/Cargo.toml" <<EOF
[package]
name = "parity"
version = "0.0.0"
edition = "2024"

[workspace]

[dependencies]
nookery-bswap = { path = "$crate" }
EOF

cat > "$work/parity/src/main.rs" <<'EOF'
use nookery_bswap::{BswapResult, Direction, *};

fn emit(name: &str, f: impl Fn(&mut [u8], Direction) -> BswapResult<()>, v: u8) {
    let mut from_be = [v];
    let mut to_be = [v];
    f(&mut from_be, Direction::FromBe).unwrap();
    f(&mut to_be, Direction::ToBe).unwrap();
    println!("{name} {} {}", from_be[0], to_be[0]);
}

fn main() {
    for v in 0..=u8::MAX {
        println!("reverse_bits8 {0} {0}", reverse_bits8(v));
        emit("1_7", repack_1_7, v);
        emit("7_1", repack_7_1, v);
        emit("5_3", repack_5_3, v);
        emit("2222", repack_2222, v);
        emit("1_1_6", repack_1_1_6, v);
        emit("11111_3", repack_11111_3, v);
        emit("111_5", repack_111_5, v);
        emit("4_1_3", repack_4_1_3, v);
    }

    for v in [0u16, 1, 0x00FF, 0xFF00, 0x1234, 0x8000, 0xFFFF] {
        println!("swap16 {v} {}", swap_u16(v));
    }
    for v in [0u32, 1, 0x8000_0000, 0x1234_5678, 0xDEAD_BEEF, 0xFFFF_FFFF] {
        println!("swap32 {v} {}", swap_u32(v));
    }

    let buf = [0x12u8, 0x34, 0xAB, 0xCD, 0, 0, 0, 0];
    println!("load_be16 {}", load_be16(&buf).unwrap());
    println!("load_be32 {}", load_be32(&[0x12, 0x34, 0x56, 0x78]).unwrap());

    let mut store = [0u8; 4];
    store_be16(&mut store, 0x1234).unwrap();
    println!("store_be16 {} {} {} {}", store[0], store[1], store[2], store[3]);
    store_be32(&mut store, 0x1234_5678).unwrap();
    println!("store_be32 {} {} {} {}", store[0], store[1], store[2], store[3]);

    let mut a16 = [0x12u8, 0x34, 0xAB, 0xCD, 0x00, 0xFF, 0x55, 0xAA];
    swap_u16_array(&mut a16, 4).unwrap();
    print!("swap16_array4");
    for b in a16 {
        print!(" {b}");
    }
    println!();

    let mut a32 = [1u8, 2, 3, 4, 5, 6, 7, 8];
    swap_u32_array(&mut a32, 2).unwrap();
    print!("swap32_array2");
    for b in a32 {
        print!(" {b}");
    }
    println!();

    let mut zero = [0x12u8, 0x34, 0x56, 0x78];
    swap_u16_array(&mut zero, 0).unwrap();
    println!("swap16_array0 {} {} {} {}", zero[0], zero[1], zero[2], zero[3]);
}
EOF

gcc -std=c11 -O1 -Wall -Wextra -Werror -I "$work" -o "$work/oracle" "$work/oracle.c"
gcc -std=c11 -O1 -Wall -Wextra -Werror -I "$work" -o "$work/endian" "$work/endian.c"
"$work/oracle" > "$work/oracle_repack.txt"
"$work/endian" > "$work/oracle_endian.txt"

(cd "$work/parity" && CARGO_TARGET_DIR="$work/target" cargo run --quiet 2>/dev/null) > "$work/rust.txt"
tail -n +2305 "$work/rust.txt" > "$work/rust_endian.txt"
head -n 2304 "$work/rust.txt" > "$work/rust_repack.txt"

diff -u "$work/oracle_repack.txt" "$work/rust_repack.txt" || {
    echo "FAIL: bitfield repack output diverges from the C oracle" >&2; exit 1; }
diff -u "$work/oracle_endian.txt" "$work/rust_endian.txt" || {
    echo "FAIL: endian output diverges from the C oracle" >&2; exit 1; }

echo "PARITY OK: $(wc -l < "$work/oracle_repack.txt") repack lines + $(wc -l < "$work/oracle_endian.txt") endian lines match the C oracle"
