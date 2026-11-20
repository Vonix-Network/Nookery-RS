# `nookery-data-ffi` — data-folder C ABI candidate

Status: **candidate, not integrated.**  `pc/src/acgc_data.cpp` is still the
application's default path and nothing links this library.  The porting ledger
(Phase 1) requires a C-vs-Rust parity gate that this candidate does **not**
claim; see [What is not proven](#what-is-not-proven).

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
| `src/ffi.rs` | the 18 `extern "C"` entry points |
| `tests/abi.rs` | 23 boundary tests through raw pointers |
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
```

`make` builds `libnookery_data_ffi.a` into `tools/data-ffi/build/` (gitignored),
compiles `harness.c` with `-std=c11 -Wall -Wextra -Werror -pedantic`, and runs
it.  The harness prints `219 checks, 0 failures` and then
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

## What is proven

- Handles/plans own their state; null `*_destroy` is accepted; creation does no
  filesystem work.
- Initialization creates exactly `save/card_a`, `save/card_b`, `texture_pack`,
  `nes_roms`, `cache` in upstream order, is idempotent, preserves existing
  content, and reports a file blocker as `NOT_DIRECTORY` while still creating
  the remaining directories.
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
- Mutation checks, run manually and reverted: changing a harness expectation
  fails the harness (`218 checks, 1 failures`, before that check existed); renumbering a header status
  fails `abi_header`; altering one message byte fails two Rust tests.

## What is not proven

- **No C++ oracle run.**  `pc/src/acgc_data.cpp` was never executed side by
  side with this ABI.  The blocker observed on this host is *not* X11/Wayland —
  both are installed (`pkg-config --exists x11 wayland-client` succeeds) — but
  the two submodules are unpopulated: `extern/aurora/` and `extern/borealis/`
  are empty directories, so `cmake -S . -B …` fails at
  `include(extern/borealis/cmake/DetectVersion.cmake)` before any target is
  configured.  `SDL2` development files are also absent.  The Phase 1 parity gate
  is therefore open.  All parity claims above are about the Rust text/number
  tables matching what the C++ *source* declares, not about observed C++ runtime
  output.
- **No CMake integration** (and no configure evidence on this host: see above).
  The crate is its own workspace (`[workspace]` at the
  top of `Cargo.toml`) because the lane may not edit the root workspace
  manifest.  Parent integration is: add `nookery-data-ffi` to
  `rust/Cargo.toml` members, drop this crate's `[workspace]`, and wire the
  staticlib into the `acgc` target.
- **No application linkage.**  Nothing calls these symbols yet, so no
  end-to-end or platform-matrix evidence exists.
- **Behavioural parity for migration and legacy folders** is out of scope, as
  listed under divergences.
- **Threading.**  One handle per thread; the ABI does no locking and documents
  that requirement rather than enforcing it.
