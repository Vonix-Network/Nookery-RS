# Nookery-RS phased ledger

Execution plan for the Rust port. The architecture map — what exists and where — is in [`PORTING_RUST.md`](./PORTING_RUST.md). Source-removal decisions are tracked separately in [`PORT_CLEANUP_LEDGER.md`](./PORT_CLEANUP_LEDGER.md).

## Rules of engagement

These hold for every phase. A phase that violates one is not done, regardless of what compiles.

1. **Compile at every phase.** The full application builds on all eight CI cells (five platform families, see the map) after every merge. A phase that only builds in isolation is not merged.
2. **Observable gate, not a vibe.** Each phase lists the exact command or user action that proves it, and what output counts as pass. "Tests pass" alone is insufficient; state what the test would fail on.
3. **No assets.** Never add, modify, or commit game data, ROMs, disc images, or extracted textures. Fixtures for save-format tests are synthetic or user-supplied at test time only.
4. **Upstream credits and licenses stay.** CC0 decompilation, MIT port layer, MIT fixNES, and the Aurora/Borealis licenses are preserved in every touched file.
5. **C/C++ stays until the deletion gate.** No phase deletes a C implementation while the Rust work is only a bounded helper, isolated model, or unactivated crate. The C path remains the oracle until the complete replacement, production callers, platform/user-path gates, and dedicated cleanup commit satisfy [`PORT_CLEANUP_LEDGER.md`](./PORT_CLEANUP_LEDGER.md).
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
| **Target** | `rust/nookery-core` + `rust/nookery-data-ffi` thin `extern "C"` shim; CMake target linking it |
| **Slices** | (a) `#[repr(C)]` layout/status structs; (b) `nookery_data_initialize` / `nookery_data_plan_migration`; (c) CMake `add_library(nookery_core …)` wired into `acgc` |
| **Non-goals** | Window-system handoff (`open_data_path`), restart flow, `set_custom_data_path` |
| **Gate — build** | All eight CI cells still produce a binary |
| **Gate — current candidate** | The ABI header/layout and Rust-to-C harness pass 37 Rust tests plus 219 checks; the C++ `acgc::data::initialize` parity run and CMake application linkage are not yet proven. |
| **Gate — behavior** | A parity harness runs the C++ `acgc::data::initialize` and the Rust shim against the same temp root; directory sets, `required_directories` order, and every `ErrorCode` message string are byte-identical. Mutation check: corrupt one Rust message string → harness fails |
| **Fallback** | `acgc_data.cpp` unchanged and still the default path until the gate passes |

## Phase 2 — Save/GCI and byteswap (compatibility-critical)

Highest-risk-for-correctness slice: user saves must round-trip.

| | |
|---|---|
| **Target** | `rust/nookery-save` + primitive `rust/nookery-bswap` first; `pc/src/pc_gci.c`, `pc/src/pc_save_bswap.c`, `pc/include/pc_gci.h`, `pc_save_bswap.h` remain the oracle |
| **Slices** | (a) GCI header (`CARDDir`, 0x2000 blocks, rotation suffixes); (b) five typed `#[repr(C)]` LE↔BE leaf mirrors; (c) Mail name/header/content swaps; (d) composite `Save_t`/ARAM LE↔BE swap; (e) card A/B directory resolution |
| **Gate — current candidate** | GCI directory/record/checksum validation, primitive endian/bitfield repackers, five typed save-byte-swap leaf mirrors, Mail swaps, bounded composite `Save_t`/keep-mail walkers, recovery/rotation precedence, staged synthetic writes, and the PC card-manager layer (save-path resolution, town and Card-B visitor-town scanning/selection, ARAM block staging, reset codes, pre-write side effects, legacy migration, write gating) pass 65 save tests; full `Save_t` construction/byte swapping in `pc_gci_write`, the game-global travel lifecycle (`mCD_InitGameStart_bg`, `mCD_SaveStation_NextLand_bg`, `mCD_toNextLand`), and runtime-only cleanup/time calls remain open. |
| **Gate — round-trip** | For N real `.gci` files (user-supplied, never committed): C reads → Rust byteswaps → C reads must produce identical `Save_t`; and Rust write → C read → Rust read is stable. Any byte diff fails |
| **Gate — failure modes** | `pc_save_result` states `PC_SAVE_MISSING` / `PC_SAVE_INVALID` / `PC_SAVE_IO_ERROR` reproduced for truncated, wrong-hash, and unreadable files |
| **Gate — regression** | Existing C save path still loads a previously saved village (the C oracle is unchanged) |
| **Risk** | Struct packing/alignment differences between Rust and the decomp; mitigate with `#[repr(C)]` mirrors generated from headers and a size/offset assertion test per struct |

## Phase 3 — Settings and input

Frozen on-disk and in-memory contracts.

| | |
|---|---|
| **Target** | `rust/nookery-config` and bounded `rust/nookery-input` first; `pc/src/acgc_settings.cpp`, `pc/include/pc_settings.h`, `pc/src/acgc_input_profiles.cpp`, `src/padmgr.c` remain the oracle |
| **Slices** | (a) `PCSettings` field order + `config.json` codec keys; (b) keyboard/gamepad default maps; (c) direction editing, conflict validation, and legacy migration; (d) live deadzone/PADStatus math |
| **Gate — current candidate** | Scalar defaults/ranges, Borealis-style constraints, legacy INI import, JSON migration, exact input defaults, direction editing, conflict validation, SDL-name translation, controller ports, PAD masks, deadzone/threshold math, hotplug, snapshot bounds, and malformed-input handling pass 27 config plus 45 input tests; the published `#[repr(C)] PCSettings` ABI is offset/size/alignment-identical to `pc_settings.h` and coexists with it in one translation unit, with a 70-check C harness. Real-install round-trip, live SDL/Aurora/PADStatus behavior, and C++ consumer migration are not yet proven. |
| **Gate — config** | Round-trip an existing `config.json` and `settings.ini` from a real install: every key and value preserved; unknown keys retained, not dropped |
| **Gate — input** | Same controller input sequence yields identical `PADStatus` bytes from C and Rust across the deadzone boundary (0, at-threshold, above-threshold) |
| **Gate — platform** | Touch-control layouts on Android/iOS still render and the layout editor still saves |
| **Risk** | `PCSettings` is read by decomp C via `g_pc_settings`; the struct ABI is load-bearing. Rust must not own it until the C consumers are migrated or a frozen C struct is shared |

## Phase 4 — Allocator and fixed arenas

Only after Phases 1–3 prove the harness works.

| | |
|---|---|
| **Target** | `src/gamealloc.c`, `src/gfxalloc.c`, `src/TwoHeadArena.c`, `src/THA_GA.c`, `src/PreRender.c`; bounded model in `rust/nookery-arena` |
| **Slices** | (a) `GameAlloc` list semantics (size header, tail bookkeeping); (b) `THA` double-ended arena; (c) `gfxalloc` 8-byte alignment + `gSPBranchList` |
| **Gate — current candidate** | Bounded GameAlloc/TwoHeadArena models pass 56 Rust tests and a fixed-seed C transcript; the model is not layout-compatible with the live C structs and is not runtime integrated. |
| **Gate — invariant** | Randomized allocation/free sequences over a fixed seed produce the same live-set and tail pointer as C; free-of-null and cleanup-while-nonempty behave identically |
| **Gate — memory** | Measured RSS and peak arena usage within noise of the C build on the same boot-to-title workload; no address assumptions violated |
| **Non-goal** | Replacing `malloc` with a Rust allocator, or changing arena sizes — forbidden by the optimize-after-parity rule |
| **Risk** | Very high. Decomp callers assume pointer arithmetic and no relocation. This phase may legitimately end as "measured, keep C" |

## Phase 5 — Famicom / fixNES boundary

Third-party emulator; integrate, do not rewrite.

| | |
|---|---|
| **Target** | `rust/nookery-famicom` host seam first; `pc/src/pc_nes_fixnes.c`, `src/famicom_emu.c`, `src/static/Famicom/` remain the oracle |
| **Slices** | (a) frame-buffer contract (256×240 RGB565); (b) AI DMA audio cadence; (c) save-header (`0x40`) and internal-ROM table |
| **Gate — current candidate** | ROM catalog, exact exit combos/countdown, frame-action ordering, PC arena/cleanup contract, mapper-0 iNES validation, expanded official 6502 opcode/addressing subset with stack/control-flow/RMW/page-crossing/indirect-JMP behavior, controller shift behavior, CPU-path PPU/APU register boundaries, audio cadence, PPU register/memory and background+sprite rendering, iNES clamp parity for over-declared headers, mapper-0 PRG RAM backing, `$4014` OAM copy, and runtime cleanup pass 74 Famicom tests; remaining opcodes, mappers, full emulator/fixNES parity, and application activation remain open. |
| **Gate — visual** | A known NES ROM renders an identical frame hash (per-frame CRC over the RGB565 buffer) as the C path for a fixed input script |
| **Gate — audio** | Sample sequence bit-identical for the same script; no underruns introduced |
| **Constraint** | fixNES is MIT (FIX94) — keep `pc/lib/fixnes/LICENSE` and attribution; do not vendor a different emulator |

## Phase 6 — Disc and content loading

| | |
|---|---|
| **Target** | `pc/src/acgc_disc.cpp`, `pc/src/acgc_texture_replacements.cpp`, asset pipeline; bounded seams in `rust/nookery-disc` and `rust/nookery-texture` |
| **Slices** | (a) accepted-disc catalog + xxh3-128 check; (b) exact texture-pack primary/cwd/base-relative path selection; (c) HD texture pack indexing |
| **Gate — current candidate** | Rust catalog/header/hash classification, bounded RVZ/CISO metadata inspection, raw ISO/GCM stream verification, and bounded read/error behavior pass 60 disc tests; texture path precedence, non-directory handling, and deterministic PNG/DDS indexing pass 10 texture tests; no successful Aurora RVZ/ISO/CISO/GCM decode, texture registration, or user-path launch is claimed. |
| **Gate** | All four image formats (RVZ, ISO, CISO, GCM) accepted; wrong-hash disc rejected with the same error; texture replacement for a known pack resolves identically |
| **Constraint** | Never ship a disc image or extracted assets; tests use user-supplied input or a synthetic hash fixture |

## Phase 7 — UI and platform services

| | |
|---|---|
| **Target** | `pc/src/ui/*.cpp`, `pc/src/aurora_platform.cpp`, `aurora_vi.cpp`, `pc_clock.c`, `pc_audio.c` |
| **Slices** | One screen at a time (settings, touch controls, town select, overlays) behind the existing Borealis trait boundary |
| **Gate — UI** | Render the real page in a window; assert on the DOM/control, not on the presence of a label string (a `[role="switch"]` must actually toggle the setting it names) |
| **Gate — platform** | Same screen behaves on all five families; touch editor works on Android/iOS |
| **Risk** | UI trait shapes must be confirmed against the checked-out Borealis submodule before starting; the existing Rust host seams do not replace platform services |

## Phase 8 — Boot and integration

Last, because it couples everything.

| | |
|---|---|
| **Target** | `src/main.c`, `src/graph.c`, `src/executor.c`, `pc/src/aurora_main.cpp` |
| **Gate — end-to-end** | A scripted session: boot → title → load village → save → reload produces a byte-identical save and identical frame hashes over a recorded input trace, on all five platform families |
| **Gate — no regression** | The C build of the same commit passes the same script, proving the harness itself |
| **Freeze** | Only after this gate does any C implementation become eligible for removal |

## Additional criterion — hosted-town multiplayer

This is a separate R3 feature track in addition to the 1:1 Rust port. It must add a clean-room, Nookery-compatible hosted-town/server experience with New Leaf-style social visits while preserving the existing offline/single-player path. It must not claim compatibility with Nintendo's proprietary network services or require distributing game assets.

| Item | Current state / required evidence |
|---|---|
| **Status** | **Bounded candidates integrated — source and security architecture advisories complete**; `nookery-social-proto`, `nookery-social-model`, `nookery-social-persistence`, and `nookery-social-server` pass local gates for offline/local behavior, but no network transport, authentication, client/game integration, or deployed service exists in the current tree |
| **Required surface** | Hosted towns, session discovery/join/leave, host/resident/visitor permissions, synchronized gameplay state, persistence/checkpoints, reconnect/recovery, and a client integration path through the real frame/input/save boundaries |
| **Required gates** | Runnable client/server vertical slice; deterministic multi-client simulation; authorization and hostile-packet negative tests; crash-safe persistence/recovery; reconnect and duplicate-delivery tests; offline regression; platform/user-path checks; no asset or proprietary-protocol dependency |
| **Acceptance boundary** | Architecture prose or a compiling server crate is not completion. Acceptance requires an exercised hosted-town user path with exact candidate identity, fresh security gates, and separately authorized deployment/effect records |

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
| 1 | Data-folder C ABI + parity harness | **Candidate — ABI + Rust/C harness pass; C++ parity and CMake linkage unproven** |
| 2 | Save/GCI + byteswap | **Candidate — GCI validator + primitive bswap + typed leaf/Mail + composite/recovery/write slices + PC card-manager layer (paths, town/Card-B selection, ARAM staging, reset codes, legacy migration); `pc_gci_write` Save_t construction and game-global travel lifecycle open** |
| 3 | Settings + input | **Candidate — settings model + `#[repr(C)]` PCSettings ABI with oracle-identical layout and `config.json` codec (70-check C harness) + profile/legacy/controller/PAD runtime slice; live SDL/PADStatus wiring and C++ consumer migration open** |
| 4 | Allocator / arenas | **Candidate — bounded model + fixed-seed C parity; runtime ABI and workload gates not started** |
| 5 | Famicom boundary | **Candidate — host seam plus native mapper-0 runtime with official 6502 opcode/addressing, stack/control-flow, RMW, page-crossing, indirect-JMP behavior, and PPU register/memory plus background+sprite rendering; APU waveforms, DMA cycle stealing, NMI/IRQ timing, fixNES dot-cadence parity, remaining opcodes/mappers, and application activation remain open** |
| 6 | Disc + content | **Candidate — catalog/header/hash + bounded RVZ/CISO/raw stream verification + texture path/index seams; Aurora format decoding, texture runtime, and user path open** |
| 7 | UI + platform services | Not started |
| 8 | Boot + integration | Not started |
| 9 | Hosted-town multiplayer | **Candidate — bounded clean-room frame codec + authority model + local persistence/server vertical slice; transport, auth, reconnect over real clients, client/game integration, and multi-client runtime remain open** |

“Candidate” means the primitive exists and passes its own unit tests; it does **not** mean the user path is ported. See [`PORTING_RUST.md`](./PORTING_RUST.md) for the measured evidence behind each row.
