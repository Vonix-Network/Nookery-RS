# Wave 15 — graph/executor production route integrated into the host candidate

Status: **CANDIDATE_ONLY**. No acceptance, deployment, publication, release or service change is implied by this record.

## Lineage

| Item | Value |
|---|---|
| Canonical repo | `Vonix-Network/Nookery-RS` (fork of `flyngmt/Nookery`, ACGC PC port decomp) |
| Base commit | `8de320b1316a8bc7726c23024b00289144e5a330` |
| Graph lane commit | `b867b3346d46ec051529843b2a16d745fad0fafb` (`pure-rust/wave14-graph-production`) |
| Common merge base | `f68ca4cbdb5eb7e8c7adbb4b9d467a573c24cefa` |
| Graph merge checkpoint | `9fbb6d56e5962bbd8717be9be52a8f225d403ae5` (`pure-rust/wave15-graph-merged`) |
| Host build repair | this record's parent commit — same branch |

## What the graph lane contributes

- `rust/nookery-graph` — graph class lifecycle, display-list buffers, `graph_proc`/`graph_main` frame ordering, `graph_dt` accumulation, all derived from `src/graph.c`.
- `rust/nookery-game` — `FrameClock` tick maths (`MAX_DT_60FPS_FRAMES`, accumulator drain) derived from `src/game.c`.
- `rust/nookery-runtime` — the runtime ABI gains a graph member (`ffi.rs: Graph`), so one `nookery_runtime_frame` call drives the Rust graph and the Rust game clock in source order (`executor.rs`, `runtime.rs`, `schedule.rs`).
- `pc/include/acgc/nookery_runtime_adapter.h`, `pc/include/acgc/nookery_runtime.h`, `pc/src/nookery_runtime_adapter.cpp` — the C ABI surface and host callbacks that let the C host hand frames to the Rust route.
- `CMakeLists.txt` — `nookery-game` and `nookery-graph` sources join the shared-image build, so the production executable links the Rust route rather than a stub.
- `src/graph.c`, `src/main.c`, `include/graph.h` — the C path keeps the oracle and the retained hardware seams; it is no longer the only route.

## Integration conflicts resolved

Thirteen paths conflicted at the three-way merge because the graph lane and the host candidate both descend from `f68ca4cb` but each had since changed shared runtime, CMake and manifest surfaces. Each was resolved by union against that common base rather than by taking either side:

`CMakeLists.txt`, `pc/include/acgc/nookery_runtime.h`, `pc/include/acgc/nookery_runtime_adapter.h`, `pc/src/nookery_runtime_adapter.cpp`, `rust/Cargo.lock`, `rust/Cargo.toml`, `rust/nookery-game/src/lib.rs`, `rust/nookery-game/tests/parity.rs`, `rust/nookery-graph/src/lib.rs`, `rust/nookery-graph/tests/frame_lifecycle.rs`, `rust/nookery-runtime/Cargo.toml`, `rust/nookery-runtime/src/lib.rs`, `rust/nookery-social-server/src/lib.rs`.

`rust/Cargo.toml` keeps every workspace member from both sides (25 crates) and `rust/Cargo.lock` was regenerated offline from the merged manifests instead of being hand-edited.

## Host build repair

The merged candidate did not build natively. Two defects, both in `pc/src/nookery_runtime_adapter.cpp`:

1. **`include/main.h:23: error: '::main' must return 'int'`.** The header declares the console entry point as `void main(void)`. That declaration is only meaningful when CMake renames the symbol (`main=ac_entry` / `main=boot_main`), which it does for the C translation units but not for this C++ adapter. Fixed by wrapping the include in the same documented rename `pc/src/aurora_main.cpp` already uses:

   ```cpp
   #define main acgc_legacy_main
   #include "main.h"
   #undef main
   ```

2. **`conflicting C language linkage declaration 'void {anonymous}::graph_setup_double_buffer(RuntimeHost*)'`.** An anonymous-namespace host helper shared its name with the exported C symbol `graph_setup_double_buffer(GRAPH*)` declared by `include/graph.h`. Two different signatures under one C linkage name is ill-formed. The helper is not an ABI entry point, so it was renamed to `apply_double_buffer_setup`; the C symbol keeps its name and its caller in the graph route.

No oracle source, test, assertion or CMake production marker was changed to achieve the build. The `${GAME_C_SOURCES}` set is still compiled and still carries the legacy game path.

## Gates on these exact bytes

Rust workspace (`rust/`, `CARGO_TARGET_DIR` isolated outside the candidate):

| Gate | Exit |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo test --workspace --all-targets` | 0 — 64 test binaries, 631 tests passed, 0 failed |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | 0 |

Native host (Ninja, `RelWithDebInfo`, `-j2`):

| Gate | Result |
|---|---|
| `cmake` configure | exit 0 |
| `ninja acgc` | exit 0 — 5025/5025 targets, 0 `FAILED:` |
| `nookery-runtime-route-assertion` (build dependency of `acgc`) | passed |
| `bin/Nookery` | ELF 64-bit LSB pie executable, x86-64, BuildID `17da7d7e…` |
| `bin/libnookery_audio.so` | ELF 64-bit LSB shared object, BuildID `6df4d6f1…` |
| `bin/libnookery_runtime.so` | ELF 64-bit LSB shared object, BuildID `389d1261…` |
| `readelf -d bin/Nookery` | `NEEDED libnookery_audio.so`, `NEEDED libnookery_runtime.so` |
| `ldd bin/Nookery` | no `not found` |
| `nm -D --defined-only libnookery_runtime.so` | 8 `T` symbols: `nookery_runtime_{boot,create,destroy,frame,prenmi,reset,resize,shutdown}` |
| `nm -D --defined-only libnookery_audio.so` | 41 `T` symbols (`AI*`, `DSP*`, `nookery_audio_refill`, `pc_audio_*`) |
| `rust_eh_personality` ownership | defined in `bin/Nookery` only; 0 definitions in either shared image |
| `bin/Nookery --help` | exit 0, prints the documented option set |

## What these gates do not prove

Recorded so the record cannot be read as more than it is:

- **No game display evidence.** The host needs a video device. Run from this environment it reaches `[ERROR aurora::window] No available video device` → `Error initializing window` → `SIGABRT` (`NOARGS_EXIT=134`) before any frame is presented. That is an environment gate failure, not a candidate failure, and it is also not a pass statement: the earlier `weston --backend=headless` and `Xvfb` attempts are the record of why a presented frame has not been captured here.
- **No disc image in this tree.** The console prints `Usage: Nookery [OPTION...] [disc image]`; a boot beyond window creation needs a user-supplied image outside the repository. None was used and none was added.
- **No gameplay or save-format evidence** was produced in this phase beyond the unit-level parity tests listed above.
- **No multiplayer evidence.** `nookery-social-client` still does not exist; hosted towns are not reachable end to end.
- **The port is not complete.** `${GAME_C_SOURCES}` is still compiled and `src/` still holds the game. `tools/pure-rust-production-gate.py` is expected to FAIL on `${GAME_C_SOURCES}` — and it does; that failure is the honest ledger row, not a regression.

## Effects

None. Nothing was deployed, published, released, restarted, messaged or migrated. The candidate is a dirty worktree plus pushed candidate branches; the canonical `master` at `f68ca4cb` was not advanced.
