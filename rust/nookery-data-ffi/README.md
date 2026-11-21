# `nookery-data-ffi` — data-folder C ABI candidate

Status: **candidate, opt-in parity target.**  `pc/src/acgc_data.cpp` remains the
application's default path. CMake can build and run a separate C++/Rust parity
gate with `-DENABLE_DATA_FFI_PARITY=ON`; it does not replace the production C
path.

## What this is

A total `extern "C"` surface over the parts of `nookery-core` that are already
real: required-directory creation, layout enumeration, the two
`pc_platform_*_path` resolvers, and migration *planning*.  The ABI is versioned
(`NOOKERY_DATA_ABI_VERSION`), every type is `#[repr(C)]`, and every entry point
validates its inputs and returns a status instead of unwinding — `panic =
"abort"` makes a panic an undeclared process kill, so there is no
`catch_unwind` net by design.

| File | Role |
|---|---|
| `include/nookery_data.h` | the contract a C caller compiles against; ownership, buffer and versioning rules |
| `src/abi.rs` | status numbers, `#[repr(C)]` buffer, selector enum, status text |
| `src/handle.rs` | owned handle/plan allocation with reported `OUT_OF_MEMORY` |
| `src/ffi.rs` | the 19 `extern "C"` entry points |
| `tests/abi.rs` | 24 boundary tests through raw pointers |
| `tests/abi_header.rs` | parses the real header and fails on any drift |

## Build and verify

All commands run from the repository root.  `CARGO_TARGET_DIR` is external so
no build output lands in the worktree.

```sh
# Rust candidate
CARGO_TARGET_DIR=/tmp/nookery-data-ffi-target cargo fmt --check \
  --manifest-path rust/nookery-data-ffi/Cargo.toml
CARGO_TARGET_DIR=/tmp/nookery-data-ffi-target cargo test \
  --manifest-path rust/nookery-data-ffi/Cargo.toml
CARGO_TARGET_DIR=/tmp/nookery-data-ffi-target cargo clippy \
  --manifest-path rust/nookery-data-ffi/Cargo.toml --all-targets -- -D warnings

# C harness against the real static library
make -C tools/data-ffi run

# Negative mutation: the harness must reject swapped Rust directory selectors
make -C tools/data-ffi mutation-test

# CMake parity gate, in a configured Linux host build with Borealis/Aurora
# populated (the target is intentionally opt-in)
cmake -S . -B build/data-ffi-parity -DENABLE_DATA_FFI_PARITY=ON
cmake --build build/data-ffi-parity --target nookery-data-ffi-parity-test
```

`make` builds `libnookery_data_ffi.a` into `tools/data-ffi/build/` (gitignored),
compiles `harness.c` with `-std=c11 -Wall -Wextra -Werror -pedantic`, and runs
it.  The harness prints `220 checks, 0 failures` and then
`HARNESS_RESULT=PASS`; it inspects every returned status, `length`, buffer byte
and on-disk directory, so a clean exit code alone would not be evidence.

## Status model

`0..=12` are `nookery_core::ErrorCode` in declaration order — the same order as
upstream `borealis::data::ErrorCode` — so a code compares directly against the
C++ oracle.  `13..=18` are ABI-level outcomes that upstream expresses as a bare
`0`/`false` return and therefore have no upstream number or user-facing string:

| Value | Name | Upstream equivalent |
|---:|---|---|
| 0–12 | `OK` … `OPEN_FOLDER_FAILED` | `ErrorCode`, messages byte-identical |
| 13 | `INVALID_ARGUMENT` | null/bad `struct_size`/non-UTF-8/relative root — upstream would crash or accept blindly |
| 14 | `BUFFER_TOO_SMALL` | upstream `pc_platform_*_path` returns `0` |
| 15 | `PATH_REJECTED` | upstream joins blindly, so it has no refusal code |
| 16 | `BASE_PATH_UNAVAILABLE` | upstream `pc_platform_base_path` returns `0` |
| 17 | `INDEX_OUT_OF_RANGE` | no upstream analogue (C has no enumeration) |
| 18 | `OUT_OF_MEMORY` | no upstream analogue |

`nookery_data_status_message` returns `""` for `13..=18` rather than inventing
user-facing text: those paths never produced localized or logged wording
upstream, so a string here would be a fabrication.

## Deliberate divergences from upstream

1. **Path escapes are refused.**  `pc_platform_user_path` does
   `active_data_path() / relative`; for an absolute operand
   `std::filesystem::operator/` *replaces* the root, and `..` walks out.  The
   ABI returns `PATH_REJECTED` instead.  This is hardening, not parity.
2. **Legacy-folder skipping is not reproduced.**  Upstream `ensure_folders`
   skips `texture_pack`/`nes_roms` when a legacy copy exists next to the
   executable; that check needs CWD plus executable-directory discovery, which
   this adapter does not own.  It always creates both.
3. **Migration plans, never copies.**  Copying is `borealis::data::Manager`'s
   job.  Only the rule lists and candidate selection are ported.
4. **No restart/session state.**  `set_custom_data_path`,
   `reset_data_path`, `is_data_path_restart_pending`, `is_default_data_path`,
   `abbreviated_path_string`, and `open_data_path` are host-side and absent.
5. **Non-UTF-8 paths are reported, not rewritten.**  Upstream returns
   `u8string()` bytes; choosing a lossy conversion here would silently change
   paths a C caller could act on.
6. **Cache injection is explicit.** `nookery_data_set_cache_root` lets the
   host pass Borealis' `Paths::cachePath`; without it, the ABI keeps its
   historical `<root>/cache` default.

## What is proven

- Handles/plans own their state; null `*_destroy` is accepted; creation does no
  filesystem work.
- Initialization creates exactly `save/card_a`, `save/card_b`, `texture_pack`,
  `nes_roms`, and the configured cache directory in upstream order, is
  idempotent, preserves existing content, and reports a file blocker as
  `NOT_DIRECTORY` while still creating the remaining directories.
- Every null pointer, bad `struct_size`, bad ABI version, non-UTF-8 byte
  sequence, relative/escaping root, out-of-range selector and out-of-range plan
  index returns a status; none is dereferenced.
- Undersized output writes nothing and reports the exact requirement including
  the NUL; a null pointer with nonzero capacity is rejected; an exactly-sized
  buffer is NUL-terminated and reports a length excluding the terminator.
- Every status the header defines equals the library's number, checked by
  parsing `include/nookery_data.h` in `tests/abi_header.rs`.
- Every core message equals `nookery_core::ErrorCode::message` byte for byte
  (`c_string_messages_are_byte_identical_to_the_core_messages`).
- The mutation runner swaps Rust directory selectors 2 and 3 and requires the
  C harness to report `HARNESS_RESULT=FAIL`; the source is restored afterward.
- Renumbering a header status fails `abi_header`; altering one message byte
  fails the Rust message-parity tests.

## What is not proven

- **C++ oracle execution is environment-dependent.** The parity executable
  compiles the unchanged `pc/src/acgc_data.cpp` and compares its initialized
  required directories, order, status numbers, and status messages with Rust.
  On this checkout the Borealis and Aurora submodules are unpopulated, so CMake
  cannot configure the target.
- **Production activation remains open.** The parity gate is a separate
  opt-in target; no Rust symbols are linked into `acgc`, and no application
  runtime smoke can be claimed until a populated host build passes the gate.
- **No application linkage.**  Nothing calls these symbols yet, so no
  end-to-end or platform-matrix evidence exists.
- **Behavioural parity for migration and legacy folders** is out of scope, as
  listed under divergences.
- **Threading.**  One handle per thread; the ABI does no locking and documents
  that requirement rather than enforcing it.
