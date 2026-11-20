# nookery-bswap

Bounded endian and `u8` bitfield primitives for the Nookery port, translated
from Nookery's PC port layer.

## Provenance and licensing

| Oracle | Contents |
|---|---|
| `pc/include/pc_bswap.h` | `pc_bswap16/32/64`, `pc_load_be16/32`, `pc_store_be16/32`, `pc_bswap16/32_array` |
| `pc/src/pc_save_bswap.c` (lines 45–178) | `reverse_bits8` and the `repack_u8_*` family |

Both files are part of the PC port layer, which the root `LICENSE` places under
the MIT License (© 2026 FlyingMeta); the decompilation it accompanies is CC0 1.0.
This crate carries the same MIT terms and adds no game assets or save data.

## Surface

- `endian`: `swap_u16/32/64` (self-inverse), `load_be16/32`, `store_be16/32`,
  `swap_u16_array`, `swap_u32_array`.
- `bitfield`: `reverse_bits8`, the eight named directional repackers (`1_7`,
  `7_1`, `5_3`, `2222`, `1_1_6`, `11111_3`, `111_5`, `4_1_3`), `Direction`, and a
  width-driven `repack` engine.

## Deliberate divergences from C

Every difference is a hardening of an unchecked access; nothing observable on
valid input changes.

1. **No out-of-bounds access.** `pc_load_be16(&x[1])` and
   `pc_bswap16_array(p, n)` for too large `n` read/write past the buffer. Here
   they return `BswapError::TooShort`, `NotMultipleOf`, or `CountExceeds`.
2. **Odd byte lengths are rejected.** The C array helpers index through
   `u16*`/`u32*`, so a partial trailing element was never meaningful.
3. **Bitfield layouts are validated.** The generic `repack` rejects a width list
   that does not sum to exactly 8 bits; the named helpers carry fixed, checked
   layouts.
4. **Zero-length/zero-count is an explicit no-op**, tested rather than implicit.

## Directions

Byte swaps are involutions. The bitfield repackers are **not**: they take a
`Direction` mirroring `pc_bswap_dir_t`. `FromBe` then `ToBe` of the same repacker
is the identity for every byte value; using the wrong direction silently
corrupts a byte.

## Verification

```
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
tools/oracle_parity.sh
```

`tools/oracle_parity.sh` compiles the *verbatim* oracle excerpt and diffs its
output against the crate: all 256 byte values × 8 repackers × 2 directions, plus
`reverse_bits8` and the endian/load/store/array vectors — 2 324 lines,
byte-identical. It locates the helper block by pattern, so oracle edits do not
silently shrink the comparison.

## Not implemented here

- Typed `Save_t` / ARAM struct traversal (the `swap_*` walkers in
  `pc/src/pc_save_bswap.c`). That is a separate typed-layout phase.
- GCI container, checksum, and card I/O (see `nookery-save`).
- Any C/C++ ABI shim or CMake wiring.
- C-vs-Rust parity on real `.gci` saves.
