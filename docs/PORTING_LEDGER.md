# Nookery-RS phased ledger

Execution plan for the Rust port. The architecture map — what exists and where — is in [`PORTING_RUST.md`](./PORTING_RUST.md).

## Rules of engagement

These hold for every phase. A phase that violates one is not done, regardless of what compiles.

1. **Compile at every phase.** The full application builds on all eight CI cells (five platform families, see the map) after every merge. A phase that only builds in isolation is not merged.
2. **Observable gate, not a vibe.** Each phase lists the exact command or user action that proves it, and what output counts as pass. "Tests pass" alone is insufficient; state what the test would fail on.
3. **No assets.** Never add, modify, or commit game data, ROMs, disc images, or extracted textures. Fixtures for save-format tests are synthetic or user-supplied at test time only.
4. **Upstream credits and licenses stay.** CC0 decompilation, MIT port layer, MIT fixNES, and the Aurora/Borealis licenses are preserved in every touched file.
5. **C/C++ stays available.** No phase deletes a C implementation before the Rust replacement passes the same gate. The C path is the oracle, not dead code, until parity is signed off.
6. **Optimize only after parity, only with measurements.** A change must come with before/after numbers on the same workload and must not alter save bytes, input timing, or frame output.
7. **Panic never crosses FFI.** `panic = "abort"`; every `extern "C"` entry validates and returns a status. No `unwrap`/index on untrusted input.
8. **Truthful status.** A slice compiles ≠ ported. Ledger state moves only when its gate is observed to pass on the listed platforms.

## Phase 0 — Baseline

Keep the fork's starting point measurable so later phases have an oracle.

| Item | Detail |
|---|---|
| Deliverable | Documented pin, build matrix, existing Rust crate |
| Gate | `git rev-parse HEAD` = pinned base; `cargo test --offline` in `rust/` → 18 passed; CMake configures on at least one cell |
| Observed | The Rust baseline test passed at `a81c1aaf`; the local CMake configure gate is blocked by missing X11/Wayland development libraries on the current host. |
| Status | **Partial — host CMake gate blocked** |

## Phase 1 — Data-folder boundary behind a C ABI

The `nookery-core` logic is real but unreachable from the application. Make it callable and prove equivalence.

| | |
|---|---|
| **Target** | `rust/nookery-core` + a thin `extern "C"` shim; CMake target linking it |
| **Slices** | (a) `#[repr(C)]` layout/status structs; (b) `nookery_data_initialize` / `nookery_data_plan_migration`; (c) CMake `add_library(nookery_core …)` wired into `acgc` |
| **Non-goals** | Window-system handoff (`open_data_path`), restart flow, `set_custom_data_path` |
| **Gate — build** | All eight CI cells still produce a binary |
| **Gate — behavior** | A parity harness runs the C++ `acgc::data::initialize` and the Rust shim against the same temp root; directory sets, `required_directories` order, and every `ErrorCode` message string are byte-identical. Mutation check: corrupt one Rust message string → harness fails |
| **Fallback** | `acgc_data.cpp` unchanged and still the default path until the gate passes |

## Phase 2 — Save/GCI and byteswap (compatibility-critical)

Highest-risk-for-correctness slice: user saves must round-trip.

| | |
|---|---|
| **Target** | `rust/nookery-save` first; `pc/src/pc_gci.c`, `pc/src/pc_save_bswap.c`, `pc/include/pc_gci.h`, `pc_save_bswap.h` remain the oracle |
| **Slices** | (a) GCI header (`CARDDir`, 0x2000 blocks, rotation suffixes); (b) `Save_t`/ARAM LE↔BE swap; (c) card A/B directory resolution |
| **Gate — current candidate** | GCI directory/record/checksum validation and bounded path/read tests pass; this does not yet prove full `Save_t` byteswap or card I/O parity. |
| **Gate — round-trip** | For N real `.gci` files (user-supplied, never committed): C reads → Rust byteswaps → C reads must produce identical `Save_t`; and Rust write → C read → Rust read is stable. Any byte diff fails |
| **Gate — failure modes** | `pc_save_result` states `PC_SAVE_MISSING` / `PC_SAVE_INVALID` / `PC_SAVE_IO_ERROR` reproduced for truncated, wrong-hash, and unreadable files |
| **Gate — regression** | Existing C save path still loads a previously saved village (the C oracle is unchanged) |
| **Risk** | Struct packing/alignment differences between Rust and the decomp; mitigate with `#[repr(C)]` mirrors generated from headers and a size/offset assertion test per struct |

## Phase 3 — Settings and input

Frozen on-disk and in-memory contracts.

| | |
|---|---|
| **Target** | `rust/nookery-config` first; `pc/src/acgc_settings.cpp`, `pc/include/pc_settings.h`, `pc/src/acgc_input_profiles.cpp`, `src/padmgr.c` remain the oracle |
| **Slices** | (a) `PCSettings` field order + `config.json` codec keys; (b) keyboard/gamepad default maps; (c) deadzone math |
| **Gate — current candidate** | Scalar defaults/ranges, Borealis-style constraints, legacy INI import, and JSON migration tests pass. Real-install round-trip and C ABI parity are not yet proven. |
| **Gate — config** | Round-trip an existing `config.json` and `settings.ini` from a real install: every key and value preserved; unknown keys retained, not dropped |
| **Gate — input** | Same controller input sequence yields identical `PADStatus` bytes from C and Rust across the deadzone boundary (0, at-threshold, above-threshold) |
| **Gate — platform** | Touch-control layouts on Android/iOS still render and the layout editor still saves |
| **Risk** | `PCSettings` is read by decomp C via `g_pc_settings`; the struct ABI is load-bearing. Rust must not own it until the C consumers are migrated or a frozen C struct is shared |

## Phase 4 — Allocator and fixed arenas

Only after Phases 1–3 prove the harness works.

| | |
|---|---|
| **Target** | `src/gamealloc.c`, `src/gfxalloc.c`, `src/TwoHeadArena.c`, `src/THA_GA.c`, `src/PreRender.c` |
| **Slices** | (a) `GameAlloc` list semantics (size header, tail bookkeeping); (b) `THA` double-ended arena; (c) `gfxalloc` 8-byte alignment + `gSPBranchList` |
| **Gate — invariant** | Randomized allocation/free sequences over a fixed seed produce the same live-set and tail pointer as C; free-of-null and cleanup-while-nonempty behave identically |
| **Gate — memory** | Measured RSS and peak arena usage within noise of the C build on the same boot-to-title workload; no address assumptions violated |
| **Non-goal** | Replacing `malloc` with a Rust allocator, or changing arena sizes — forbidden by the optimize-after-parity rule |
| **Risk** | Very high. Decomp callers assume pointer arithmetic and no relocation. This phase may legitimately end as "measured, keep C" |

## Phase 5 — Famicom / fixNES boundary

Third-party emulator; integrate, do not rewrite.

| | |
|---|---|
| **Target** | `pc/src/pc_nes_fixnes.c`, `src/famicom_emu.c`, `src/static/Famicom/` |
| **Slices** | (a) frame-buffer contract (256×240 RGB565); (b) AI DMA audio cadence; (c) save-header (`0x40`) and internal-ROM table |
| **Gate — visual** | A known NES ROM renders an identical frame hash (per-frame CRC over the RGB565 buffer) as the C path for a fixed input script |
| **Gate — audio** | Sample sequence bit-identical for the same script; no underruns introduced |
| **Constraint** | fixNES is MIT (FIX94) — keep `pc/lib/fixnes/LICENSE` and attribution; do not vendor a different emulator |

## Phase 6 — Disc and content loading

| | |
|---|---|
| **Target** | `pc/src/acgc_disc.cpp`, `pc/src/acgc_texture_replacements.cpp`, asset pipeline |
| **Slices** | (a) accepted-disc catalog + xxh3-128 check; (b) HD texture pack indexing |
| **Gate** | All four image formats (RVZ, ISO, CISO, GCM) accepted; wrong-hash disc rejected with the same error; texture replacement for a known pack resolves identically |
| **Constraint** | Never ship a disc image or extracted assets; tests use user-supplied input or a synthetic hash fixture |

## Phase 7 — UI and platform services

| | |
|---|---|
| **Target** | `pc/src/ui/*.cpp`, `pc/src/aurora_platform.cpp`, `aurora_vi.cpp`, `pc_clock.c`, `pc_audio.c` |
| **Slices** | One screen at a time (settings, touch controls, town select, overlays) behind the existing Borealis trait boundary |
| **Gate — UI** | Render the real page in a window; assert on the DOM/control, not on the presence of a label string (a `[role="switch"]` must actually toggle the setting it names) |
| **Gate — platform** | Same screen behaves on all five families; touch editor works on Android/iOS |
| **Risk** | Borealis is unpopulated at this pin, so its trait shapes are `INFERRED` from call sites; confirm against a checked-out submodule before starting |

## Phase 8 — Boot and integration

Last, because it couples everything.

| | |
|---|---|
| **Target** | `src/main.c`, `src/graph.c`, `src/executor.c`, `pc/src/aurora_main.cpp` |
| **Gate — end-to-end** | A scripted session: boot → title → load village → save → reload produces a byte-identical save and identical frame hashes over a recorded input trace, on all five platform families |
| **Gate — no regression** | The C build of the same commit passes the same script, proving the harness itself |
| **Freeze** | Only after this gate does any C implementation become eligible for removal |

## Parity and performance ledger

Record measured, not claimed:

```text
workload: boot-to-title / 60 s town walk / save+reload
metric:   wall time, peak RSS, frame-hash match, save-byte match
baseline: C build at the pin
```

An optimization is accepted only with a row here showing the metric, the before/after numbers, and confirmation that save bytes and frame hashes are unchanged. No row → not merged.

## Status table

| Phase | Scope | State |
|---|---|---|
| 0 | Baseline pin + crate tests | **Partial — host CMake gate blocked** |
| 1 | Data-folder C ABI + parity harness | Candidate (crate logic exists, unwired) |
| 2 | Save/GCI + byteswap | **Candidate — GCI validator only; byteswap/card I/O not started** |
| 3 | Settings + input | **Candidate — settings model only; input not started** |
| 4 | Allocator / arenas | Not started |
| 5 | Famicom boundary | Not started |
| 6 | Disc + content | Not started |
| 7 | UI + platform services | Not started |
| 8 | Boot + integration | Not started |

“Candidate” means the primitive exists and passes its own unit tests; it does **not** mean the user path is ported. See [`PORTING_RUST.md`](./PORTING_RUST.md) for the measured evidence behind each row.
