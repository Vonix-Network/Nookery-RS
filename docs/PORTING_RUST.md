# Nookery-RS port ledger

Nookery-RS is a Rust reimplementation of [Nookery](https://github.com/flyngmt/Nookery), maintained as a fork so the original project and contributors remain credited. The fork is not a replacement for the upstream license or attribution notices.

This file is the **architecture map**: what exists, where it lives, and what is already ported. The phase-by-phase plan with acceptance gates is in [`PORTING_LEDGER.md`](./PORTING_LEDGER.md).

## How to read this document

Every claim below is tagged:

- **OBSERVED** — read from the pinned tree at `a81c1aaf0d169a49d1044eb5bce9bbf699da64d9`.
- **INFERRED** — a conclusion drawn from observed call sites, not from a specification.
- **UNKNOWN** — not determinable at this pin (usually because a submodule is unpopulated).

When a statement has no tag it restates the project constraints in [Scope](#scope), which are policy, not measurements.

## Scope

- Preserve the observable behavior and supported platform matrix of Nookery.
- Permit only measured RAM/CPU optimizations that do not change gameplay, save formats, input behavior, rendering behavior, or supported user workflows.
- Keep the existing C/C++ implementation available while Rust slices are migrated and verified. **A Rust crate compiling is not evidence that the corresponding user path has been ported.**
- Do not include or distribute game assets; users must provide a legally obtained supported disc image, as in upstream. `assets/` (980 KiB, OBSERVED) holds only port-owned resources, not game data.

## Measured scale

OBSERVED at the pinned commit (`du -sh`, `find … | wc -l`, `cat | wc -l`):

| Layer | Size | Files | Lines | Role |
|---|---:|---:|---:|---|
| `src/` | 51 MiB | 5 284 | 814 605 | Decompiled GameCube game code (C/C++) |
| `pc/` | 11 MiB | 236 | 66 645 | Handwritten PC host layer (C/C++) |
| `include/` | 4.7 MiB | 754 | 85 119 | Decomp headers plus Famicom/GBA/Dolphin shims |
| `libs/dolphin/` | 1.6 MiB | — | — | Dolphin SDK reimplementation (gx, mtx, pad, dvd, card, dsp) |
| `extern/aurora/` | 12 KiB | 0 | — | Submodule, **unpopulated** at this pin |
| `extern/borealis/` | — | 0 | — | Submodule, **unpopulated** at this pin |
| `assets/` | 980 KiB | — | — | Port-owned assets only |
| `rust/` | — | 8 | 925 | Rust workspace (`nookery-core`), 18 unit tests |

`git submodule status` reports leading `-` for both submodules, i.e. neither is checked out. Any claim depending on Aurora or Borealis **source** is therefore `INFERRED` from call sites only.

## Layers

### 1. Decompilation core — `src/` (OBSERVED)

The game itself. Subdirectory ownership, by file count:

| Path | Files | Role |
|---|---:|---|
| `src/data/` | 3 212 | Static game tables: `model/` (15 MiB), `field/` (8.7 MiB), `npc/` (5.5 MiB), `scene/`, `item/`, `font/`, `player/`, `combi/`, `submenu/`, `titledemo/` |
| `src/furniture/` | 696 | Furniture actors |
| `src/actor/` | 603 | NPC/creature actors (`ac_*.c`) |
| `src/game/` | 320 | Gameplay systems (`m_player*`, `m_camera*`, `m_inventory*`, …) |
| `src/static/` | 249 | Vendored middleware: `jaudio_NES/` (64), `JSystem/` (45), `libultra/` (33), `Famicom/` (4) |
| `src/effect/` | 131 | Particle/effect actors |
| `src/bg_item/` | 23 | Background props |
| `src/system/` | 9 | `sys_matrix.c`, `sys_math*.c`, `sys_dynamic.c`, `sys_stacks.c`, `sys_vimgr.c`, `sys_ucode.c` |
| `src/AUS/` | 9 | AU sound wrapper |
| top level | 25 `.c` | Boot and managers: `main.c`, `game.c`, `graph.c`, `executor.c`, `irqmgr.c`, `padmgr.c`, `audio.c`, `zurumode.c`, … |

Top-level boot chain (OBSERVED): `src/main.c` defines `main()` → sets `HotStartEntry = &entry`; `entry()` initialises `padmgr_Init` and calls `mainproc()`; `src/executor.c` supplies `_prolog`/`_epilog`; `src/graph.c` owns the frame/game loop (`graph_ct`, `graph_dt`, double buffering); `src/game.c` drives per-frame move/draw.

### 2. PC host layer — `pc/` (OBSERVED)

The handwritten port. 26 top-level units in `pc/src/` plus 17 UI units in `pc/src/ui/`.

| Area | Exact paths | Role |
|---|---|---|
| **Data folders** | `pc/src/acgc_data.cpp` (239), `pc/include/acgc/data.hpp` | Front end over `borealis::data::Manager`; declares `initialize`, `migration_rules`, `set_custom_data_path`, `reset_data_path`, `base_path_relative`, `open_data_path` |
| **Disc validation** | `pc/src/acgc_disc.cpp` (80), `pc/include/acgc/disc.hpp` | `AcceptedDiscs` = `GAFE01` rev 0 with xxh3-128 `9ffba17f08aa24e8a084ba30e038cef3`; recognises `GAFJ01`, `GAFP01` |
| **Save / GCI** | `pc/src/pc_gci.c` (600), `pc/include/pc_gci.h`, `pc_gci_dir.h` | GCI container: 0x2000-block `CARDDir` header, `DobutsunomoriP_MURA.gci`, rotation suffixes (`.bak1..3`, `.temp`, `.write`) |
| **Memory card** | `pc/src/pc_m_card.c` (1 133) | Card A/B mounts, village generation, ARAM blocks |
| **Save byteswap** | `pc/src/pc_save_bswap.c` (1 180), `pc/include/pc_save_bswap.h` | Bidirectional LE↔BE swap of `Save_t` and ARAM blocks |
| **Settings** | `pc/src/acgc_settings.cpp` (458), `pc/include/pc_settings.h`, `pc/include/acgc/settings.hpp` | `PCSettings` struct (`window_width`, `vsync`, `max_fps`, deadzones, `nes_aspect`, …); Aurora/Borealis config codec |
| **Input** | `pc/src/acgc_input_profiles.cpp` (97), `acgc_controller_ports.cpp`, `acgc_legacy_input.cpp`, `pc/include/acgc/input_profiles.hpp`, `controller_ports.hpp`, `legacy_input.hpp` | Keyboard/gamepad default maps over `aurora::pad::controls()` |
| **Famicom** | `pc/src/pc_nes_fixnes.c` (454), `pc/lib/fixnes/` | fixNES (MIT, FIX94) adapted as a 256×240 RGB565 library; audio via `AIInitDMA` |
| **Assets** | `pc/src/pc_assets.c`, `pc/tools/gen_runtime_assets.py` | Runtime asset generation |
| **UI** | `pc/src/ui/*.cpp` (17), `pc/include/acgc/ui/*.hpp` | RmlUi screens: settings, touch controls + editor, theme, town select, overlays |
| **Entry** | `pc/src/aurora_main.cpp` (666+) | Host `int main` → `boot_main`; wires Aurora window, disc, data, settings, prelaunch |
| **Platform** | `pc/src/aurora_platform.cpp`, `pc/src/aurora_vi.cpp`, `pc_clock.c`, `pc_audio.c` | Window, VI, clock, audio adapters |

### 3. External dependencies (OBSERVED state)

- `extern/aurora` → `https://github.com/encounter/aurora.git` at `7c8c6117…`, **not checked out**.
- `extern/borealis` → `https://github.com/encounter/borealis.git` at `8e9427fa…`, **not checked out**.
- `libs/dolphin/` is in-tree and contains `libs/dolphin/include/dolphin/` plus `libs/dolphin/src/` (`gx`, `mtx`, `pad`, `dvd`, `card`, `dsp`) (OBSERVED).
- Aurora adds a Cargo/Corrosion build of an internal `nod` crate; `CMakeLists.txt` only patches it for Android and Apple (`corrosion_set_env_vars(nod …)`, `target nod-static`). That is Aurora's Rust, **not** `rust/nookery-core` (OBSERVED).

### 4. Rust bootstrap — `rust/` (OBSERVED)

Workspace at `rust/Cargo.toml`: edition 2024, `rust-version = "1.85"`, release profile `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`.

Single crate `nookery-core` (925 lines), dependency-free, module per concern:

| Module | Public surface |
|---|---|
| `layout.rs` | `DataLayout`, `PathError`; `save_card_a/b`, `texture_pack`, `nes_roms`, `required_directories` (4), `relative`, `resolve` |
| `migration.rs` | `MigrationRules` (verbatim upstream lists), `MigrationPlan`, `plan()` — plans only, never copies |
| `init.rs` | `initialize`, `ensure_directory`, `Initialized` (aggregates `Status` failures) |
| `status.rs` | `ErrorCode` (13 variants) with byte-for-byte upstream messages, `Status` |
| `base.rs` | `BasePath` — `Unavailable` by design (executable discovery not ported) |

The crate models `borealis::data` from its call sites in `acgc_data.cpp` and `pc/src/ui/settings.cpp` (`data_folder_error_message`), because Borealis is unpopulated. Documented divergences in `layout.rs`: `relative()` refuses absolute/`..` paths where upstream's `pc_platform_user_path` joins blindly — a deliberate hardening marked as such in the code.

**Integration status (OBSERVED):** no CMake file references `nookery-core`. `grep -rn 'nookery' CMakeLists.txt CMakePresets.json configure.py BUILDING.md` matches only the Windows manifest/Rc filenames and the macOS bundle identifier `com.flyngmt.nookery`. The crate builds and tests standalone (`cargo test` → 18 passed) and ships in no binary.

## Subsystem boundaries the port must respect

Grouped by what a Rust slice can own without touching the C ABI.

| Subsystem | Owns (C side) | Boundary the Rust slice must not cross |
|---|---|---|
| **Data folders** | `pc/src/acgc_data.cpp` | Directory creation + path resolution only; window-system handoff (`open_data_path`) stays host-side |
| **Disc** | `pc/src/acgc_disc.cpp` | Read-only catalog + hash; ISO/RVZ/CISO/GCM parsing stays in Aurora `dvd` |
| **Save/GCI** | `pc/src/pc_gci.c`, `pc_save_bswap.c`, `pc_m_card.c` | Byte layout is a compatibility contract; must round-trip real `.gci` byte-for-byte |
| **Settings** | `pc/src/acgc_settings.cpp`, `pc/include/pc_settings.h` | On-disk `config.json` keys and `PCSettings` field order are frozen |
| **Input** | `pc/src/acgc_input_profiles.cpp`, `src/padmgr.c` | `PADStatus` semantics and deadzone math observable to gameplay |
| **Allocator** | `src/TwoHeadArena.c`, `THA_GA.c`, `gamealloc.c`, `gfxalloc.c`, `PreRender.c` | Address/size relationships are assumed by decomp callers; no relocation |
| **Famicom** | `src/famicom_emu.c`, `src/static/Famicom/`, `pc/src/pc_nes_fixnes.c` | 256×240 RGB565 output + AI DMA audio cadence; fixNES is MIT, third-party |
| **UI** | `pc/src/ui/*.cpp` | RmlUi document structure and localized strings are user-visible |
| **Boot/integration** | `src/main.c`, `src/graph.c`, `pc/src/aurora_main.cpp` | Threading and frame ordering; last to migrate |

Allocator detail (OBSERVED): `gamealloc.c` (55 lines) is a doubly-linked list over libc `malloc`, storing `alloc_size` ahead of user data; `TwoHeadArena.c` (105) is the double-ended arena; `THA_GA.c` (16) wraps `THA_*` for Gfx buffers; `gfxalloc.c` (29) allocates display lists with 8-byte alignment via `gSPBranchList`. These are lock-free and pointer-arithmetic-heavy — the highest-risk slice for any rewrite.

## Supported platform matrix

OBSERVED from `.github/workflows/build.yml` — five platform families, eight build cells:

| Family | Cells | Runner |
|---|---|---|
| Linux (GCC) | x86_64, aarch64 | `ubuntu-24.04`, `ubuntu-24.04-arm` |
| Apple | macOS arm64, macOS x86_64, iOS arm64 | `macos-latest` |
| Android | arm64-v8a | `ubuntu-latest` |
| Windows (MSVC) | x86_64, arm64 | `windows-latest` |

Presets: `x-linux-ci`, `x-macos-ci-{arm64,x86_64}`, `x-ios-ci`, `x-android-ci-arm64`, `x-windows-ci`. Build is CMake ≥ 3.25, C11 + C++20, `project(acgc … LANGUAGES C CXX)`; decompilation code assumes signed `char`, so GNU/MSVC-clang builds add `-fsigned-char`.

Any Rust slice is only “done” when all eight cells still build **and** the user path it touches is exercised on Linux, macOS, Windows, Android and iOS.

## Panic and FFI rules

`panic = "abort"` in the release profile (OBSERVED). Consequences the port must design around:

- A Rust panic kills the process; there is no unwinding across FFI and no `catch_unwind` safety net. Every `extern "C"` function must be total: validate inputs, return a status code, never index/divide/`unwrap` on unvalidated data.
- `nookery-core` currently exposes no `extern "C"` surface at all (OBSERVED) — it is a pure Rust library. Introducing any FFI is a new, reviewable step, not a continuation.
- Types crossing the boundary must be `#[repr(C)]` and own no Rust-only layout; `Status`/`PathError` are Rust enums and cannot be passed by value.

## Current candidate status

**What is real:** the `nookery-core` data-folder primitive — layout, required-directory creation, migration planning, and byte-exact upstream status messages — with 18 passing unit tests against temp directories. It builds standalone and is wired into nothing.

**What is not:** no disc, save/GCI, byteswap, settings, input, allocator, Famicom, UI, or boot code is ported. The shipped application is still entirely C/C++. The next slice is the data-manager I/O boundary behind a C ABI, followed by a fixture-based parity harness that compares Rust and C++ on the same `.gci` bytes.

## Attribution and license

See the root `LICENSE` for the upstream dual-license terms and the original ACreTeam and PC-port attribution. `LICENSE` (OBSERVED) states: the decompilation code from [ACreTeam/ac-decomp](https://github.com/ACreTeam/ac-decomp) is CC0 1.0; the PC port layer (`pc/` and `#ifdef TARGET_PC` additions) is MIT, © 2026 FlyingMeta.

New Rust code in this fork remains MIT-licensed unless a source file states otherwise. Vendored third-party code keeps its own license — `pc/lib/fixnes/LICENSE` is fixNES (MIT, FIX94). Do not remove upstream copyright or license text when translating code.
