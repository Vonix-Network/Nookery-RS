# Nookery-RS port ledger

Nookery-RS is a Rust reimplementation of [Nookery](https://github.com/flyngmt/Nookery), maintained as a fork so the original project and contributors remain credited. The fork is not a replacement for the upstream license or attribution notices.

This file is the **architecture map**: what exists, where it lives, and what is already ported. The phase-by-phase plan with acceptance gates is in [`PORTING_LEDGER.md`](./PORTING_LEDGER.md).

## How to read this document

Every claim below is tagged:

- **OBSERVED** — read from the Rust-slice integration pin `7921579223b231e929435de571542e67edf83848`.
- **INFERRED** — a conclusion drawn from observed call sites, not from a specification.
- **UNKNOWN** — not determinable at this pin (usually because a submodule is unpopulated).

When a statement has no tag it restates the project constraints in [Scope](#scope), which are policy, not measurements.

## Scope

- Preserve the observable behavior and supported platform matrix of Nookery.
- Permit only measured RAM/CPU optimizations that do not change gameplay, save formats, input behavior, rendering behavior, or supported user workflows.
- Keep the existing C/C++ implementation available while Rust slices are migrated and verified; remove a source unit only when the dedicated cleanup gate in [`PORT_CLEANUP_LEDGER.md`](./PORT_CLEANUP_LEDGER.md) closes. **A Rust crate compiling is not evidence that the corresponding user path has been ported.**
- Do not include or distribute game assets; users must provide a legally obtained supported disc image, as in upstream. `assets/` (980 KiB, OBSERVED) holds only port-owned resources, not game data.

## Measured scale

OBSERVED at the pinned commit (`du -sh`, `find … | wc -l`, `cat | wc -l`):

| Layer | Size | Files | Lines | Role |
|---|---:|---:|---:|---|
| `src/` | 51 MiB | 5 284 | 814 605 | Decompiled GameCube game code (C/C++) |
| `pc/` | 11 MiB | 236 | 66 645 | Handwritten PC host layer (C/C++) |
| `include/` | 4.7 MiB | 754 | 85 119 | Decomp headers plus Famicom/GBA/Dolphin shims |
| `libs/dolphin/` | 1.6 MiB | — | — | Dolphin SDK reimplementation (gx, mtx, pad, dvd, card, dsp) |
| `extern/aurora/` | — | — | — | Aurora submodule, checked out at the pinned gitlink |
| `extern/borealis/` | — | — | — | Borealis submodule, checked out at the pinned gitlink |
| `assets/` | 980 KiB | — | — | Port-owned assets only |
| `rust/` | — | 139 | 32 360 | Rust workspace (`nookery-core`, `nookery-save`, `nookery-input`, `nookery-config`, `nookery-famicom`, `nookery-bswap`, `nookery-arena`, `nookery-disc`, `nookery-texture`, `nookery-data-ffi`, `nookery-social-proto`, `nookery-social-model`, `nookery-runtime`, `nookery-social-persistence`, `nookery-social-server`) |

The current verification checkout reports Aurora `7c8c6117eb4061d35aacd10613ed5a0f0ef02727` and Borealis `8e9427fab92c1935a5d6bacadc6053c687f10c1e`, both checked out. The original `nookery-core` data model was authored from call sites; checked-out dependency source is now available for a later reconciliation pass.

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
| **Data folders** | `pc/src/acgc_data.cpp` (239), `pc/include/acgc/data.hpp`; `rust/nookery-data-ffi` | Front end over `borealis::data::Manager`; Rust `#[repr(C)]`/staticlib ABI candidate covers bounded initialization, status/message, directory and migration-plan calls; CMake wiring and C++ parity remain open |
| **Disc validation** | `pc/src/acgc_disc.cpp` (80), `pc/include/acgc/disc.hpp`; `rust/nookery-disc` | Bounded Rust catalog/header/xxh3-128 seam; `AcceptedDiscs` = `GAFE01` rev 0 with xxh3-128 `9ffba17f08aa24e8a084ba30e038cef3`; recognises `GAFJ01`, `GAFP01`; Aurora format decoding remains outside the crate |
| **Texture paths** | `pc/src/acgc_texture_replacements.cpp` (path seam), `pc/include/acgc/texture_replacements.hpp`; `rust/nookery-texture` | Exact primary → cwd → base-relative directory selection; Aurora texture registration, decoding, GPU integration, and reload remain open |
| **Save / GCI** | `pc/src/pc_gci.c` (600), `pc/include/pc_gci.h`, `pc_gci_dir.h` | GCI container: 0x2000-block `CARDDir` header, `DobutsunomoriP_MURA.gci`, rotation suffixes (`.bak1..3`, `.temp`, `.write`) |
| **Memory card** | `pc/src/pc_m_card.c` (1 133) | Card A/B mounts, village generation, ARAM blocks |
| **Save byteswap** | `pc/src/pc_save_bswap.c` (1 180), `pc/include/pc_save_bswap.h`; `rust/nookery-bswap`, `rust/nookery-save/src/typed_swap.rs` | Rust primitive endian/bitfield repackers are C-oracle checked; five `#[repr(C)]` leaf mirrors plus `Mail_nm_c`, `Mail_hdr_c`, and `Mail_c` have tested bidirectional LE↔BE swaps; composite `Save_t`, quest, ARAM walkers, and card I/O remain unported |
| **Settings** | `pc/src/acgc_settings.cpp` (458), `pc/include/pc_settings.h`, `pc/include/acgc/settings.hpp` | `PCSettings` struct (`window_width`, `vsync`, `max_fps`, deadzones, `nes_aspect`, …); Aurora/Borealis config codec |
| **Input** | `pc/src/acgc_input_profiles.cpp` (97), `acgc_controller_ports.cpp`, `acgc_legacy_input.cpp`, `pc/include/acgc/input_profiles.hpp`, `controller_ports.hpp`, `legacy_input.hpp`; `rust/nookery-input` | Bounded Rust default maps, direction editing, conflict validation, SDL-name translation, and legacy INI migration; live SDL/Aurora/PADStatus/controller-port integration remains open |
| **Famicom** | `src/famicom_emu.c`, `src/static/Famicom/`, `pc/src/pc_nes_fixnes.c` (454), `pc/lib/fixnes/`; `rust/nookery-famicom/src/{core,runtime}.rs` | Rust candidate now validates mapper-0 iNES images, executes an expanded official 6502 opcode/addressing subset with stack/control-flow/RMW/page-crossing/indirect-JMP behavior, preserves controller mapping, audio-before-frame ordering, 60-frame exit fade, CPU-path PPU/APU register boundaries, and cleanup semantics; remaining opcodes, mappers, PPU/APU/DMA/NMI/IRQ/fixNES parity remain open |
| **Assets** | `pc/src/pc_assets.c`, `pc/tools/gen_runtime_assets.py` | Runtime asset generation |
| **UI** | `pc/src/ui/*.cpp` (17), `pc/include/acgc/ui/*.hpp` | RmlUi screens: settings, touch controls + editor, theme, town select, overlays |
| **Entry** | `pc/src/aurora_main.cpp` (666+) | Host `int main` → `boot_main`; wires Aurora window, disc, data, settings, prelaunch |
| **Platform** | `pc/src/aurora_platform.cpp`, `pc/src/aurora_vi.cpp`, `pc_clock.c`, `pc_audio.c` | Window, VI, clock, audio adapters |

### 3. External dependencies (OBSERVED state)

- `extern/aurora` → `https://github.com/encounter/aurora.git` at `7c8c6117…`, checked out in the verification tree.
- `extern/borealis` → `https://github.com/encounter/borealis.git` at `8e9427fa…`, checked out in the verification tree.
- `libs/dolphin/` is in-tree and contains `libs/dolphin/include/dolphin/` plus `libs/dolphin/src/` (`gx`, `mtx`, `pad`, `dvd`, `card`, `dsp`) (OBSERVED).
- Aurora adds a Cargo/Corrosion build of an internal `nod` crate; `CMakeLists.txt` only patches it for Android and Apple (`corrosion_set_env_vars(nod …)`, `target nod-static`). That is Aurora's Rust, **not** `rust/nookery-core` (OBSERVED).

### 4. Rust bootstrap — `rust/` (OBSERVED)

Workspace at `rust/Cargo.toml`: edition 2024, `rust-version = "1.85"`, release profile `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`.

The workspace currently contains fifteen bounded crates. The crates are not wired into the CMake application.

| Module | Public surface |
|---|---|
| `layout.rs` | `DataLayout`, `PathError`; `save_card_a/b`, `texture_pack`, `nes_roms`, `required_directories` (4), `relative`, `resolve` |
| `migration.rs` | `MigrationRules` (verbatim upstream lists), `MigrationPlan`, `plan()` — plans only, never copies |
| `init.rs` | `initialize`, `ensure_directory`, `Initialized` (aggregates `Status` failures) |
| `status.rs` | `ErrorCode` (13 variants) with byte-for-byte upstream messages, `Status` |
| `base.rs` | `BasePath` — `Unavailable` by design (executable discovery not ported) |
| `nookery-input` | `Binding`, `ProfileStore`, exact keyboard defaults, direction editing, conflict validation, SDL name translation, and legacy INI migration; no live SDL/Aurora/PADStatus integration |
| `nookery-save` | GCI directory/record/checksum validation, bounded composite `Save_t`/keep-mail layout and swaps, recovery/rotation precedence, staged synthetic writes; no full production card-manager ABI |
| `nookery-famicom` | ROM catalog, exit/countdown/frame seam, bounded mapper-0 iNES + expanded official 6502 opcode/addressing subset with stack/control-flow/RMW behavior, controller/audio/cleanup behavior; no full emulator/fixNES parity |
| `nookery-disc` | Accepted-disc catalog, header/hash classification, bounded RVZ/CISO metadata and raw ISO/GCM stream verification; no Aurora/NOD decoder |
| `nookery-texture` | `resolve_texture_pack_path` plus bounded PNG/DDS texture indexing; no Aurora texture registration/reload/GPU runtime |
| `nookery-runtime` | Deterministic boot/frame/shutdown lifecycle, callback ordering, dt/audio scheduling, reset and bounded host-adapter errors; no real platform/renderer/audio callbacks |
| `nookery-social-proto` | Opaque versioned frame envelope and incremental stream decoder; no sockets, authentication, cryptography, persistence, or game/save payload interpretation |
| `nookery-social-model` | Bounded clean-room hosted-town authority state with owner/visitor/moderator roles, revisions, presence, public items, and request replay checks |
| `nookery-social-persistence` | Bounded checksummed snapshots/command log, torn-tail recovery, corruption refusal, and atomic snapshot writes; no durable production store or deployment |
| `nookery-social-server` | Bounded local frame-command server over the protocol/model with admission, leases, reconnect, idempotency, and offline regression tests; no transport, authentication, client/game integration, or deployment |

`nookery-save` contains the GCI directory/record/checksum validator, bounded save-file helpers, five source-grounded `#[repr(C)]` typed byte-swap leaf mirrors, Mail name/header/content swaps, composite `Save_t`/keep-mail walkers, recovery/rotation precedence, and staged synthetic writes. `nookery-input` contains the bounded input-profile, default-binding, direction-editing, conflict-validation, SDL-name, legacy-INI, controller-port, PAD mask, deadzone, threshold, hotplug, and snapshot model. `nookery-config` contains scalar settings defaults/ranges, Borealis-style constraint behavior, legacy INI import, and JSON migration helpers. `nookery-famicom` contains the source-verified ROM catalog, exit/countdown state machine, frame-action ordering, PC arena/cleanup contract, and a bounded native mapper-0 runtime with expanded official 6502 opcode/addressing, stack/control-flow, RMW, page-crossing branch, and indirect-JMP behavior plus explicit emulator gaps. `nookery-bswap` contains primitive endian/bitfield helpers with a C oracle harness; `nookery-arena` contains bounded GameAlloc/TwoHeadArena models with a fixed-seed C parity fixture; `nookery-disc` contains bounded disc catalog/header/hash verification seams, RVZ/CISO metadata inspection, and raw ISO/GCM stream verification without Aurora image decoding; `nookery-texture` contains exact texture-pack path selection and bounded PNG/DDS indexing without Aurora runtime behavior; `nookery-data-ffi` contains a C ABI/staticlib candidate over the data-folder primitives with a 219-check C harness; `nookery-social-proto` contains an opaque versioned frame envelope/incremental decoder; `nookery-social-model` contains bounded hosted-town authority state, roles, revisions, presence, public items, and request replay checks; `nookery-runtime` contains a deterministic host-adapter boot/frame/shutdown boundary; `nookery-social-persistence` contains checksummed snapshot/command-log recovery; and `nookery-social-server` contains a bounded local frame-command vertical slice. The current workspace gates pass for 18 `nookery-core` tests, 65 save tests, 45 input tests, 27 `nookery-config` tests, 68 Famicom tests, 21 bswap tests, 56 arena tests, 60 disc tests, 10 texture tests, 37 data-FFI tests, 10 social-proto tests, 12 social-model tests, 7 runtime tests, 3 social-persistence tests, and 4 social-server tests plus the 219-check harness and the 70-check settings ABI harness (443 listed Rust tests total).

The data crate models `borealis::data` from its call sites in `acgc_data.cpp` and `pc/src/ui/settings.cpp` (`data_folder_error_message`). Documented divergences in `layout.rs`: `relative()` refuses absolute/`..` paths where upstream's `pc_platform_user_path` joins blindly — a deliberate hardening marked as such in the code.

**Integration status (OBSERVED):** no CMake file references any crate in `rust/`. `grep -rn 'nookery' CMakeLists.txt CMakePresets.json configure.py BUILDING.md` matches only the Windows manifest/Rc filenames and the macOS bundle identifier `com.flyngmt.nookery`. All fifteen crates build and test in the Rust workspace; none ships in the application binary.

## Subsystem boundaries the port must respect

Grouped by what a Rust slice can own without touching the C ABI.

| Subsystem | Owns (C side) | Boundary the Rust slice must not cross |
|---|---|---|
| **Data folders** | `pc/src/acgc_data.cpp` | Directory creation + path resolution only; window-system handoff (`open_data_path`) stays host-side |
| **Disc** | `pc/src/acgc_disc.cpp` | Read-only catalog + hash; ISO/RVZ/CISO/GCM parsing stays in Aurora `dvd` |
| **Save/GCI** | `pc/src/pc_gci.c`, `pc_save_bswap.c`, `pc_m_card.c` | Byte layout is a compatibility contract; must round-trip real `.gci` byte-for-byte |
| **Settings** | `pc/src/acgc_settings.cpp`, `pc/include/pc_settings.h` | On-disk `config.json` keys and `PCSettings` field order are frozen |
| **Input** | `pc/src/acgc_input_profiles.cpp`, `src/padmgr.c` | `PADStatus` semantics and deadzone math observable to gameplay |
| **Allocator** | `src/TwoHeadArena.c`, `THA_GA.c`, `gamealloc.c`, `gfxalloc.c`, `PreRender.c`; `rust/nookery-arena` | Bounded Rust GameAlloc/TwoHeadArena models and fixed-seed C parity fixture exist; address/size relationships are assumed by decomp callers and no runtime ABI integration is claimed |
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
- The Rust workspace now exposes the `nookery-data-ffi` `extern "C"` candidate surface; the other fourteen crates remain pure Rust libraries. It is not yet linked into the application, so introducing production FFI wiring is still a new, reviewable step.
- Types crossing the boundary must be `#[repr(C)]` and own no Rust-only layout; `Status`/`PathError` are Rust enums and cannot be passed by value.

## Current candidate status

**What is real:** the `nookery-core` data-folder primitive and `nookery-data-ffi` ABI candidate; the `nookery-save` GCI directory/record/checksum validator, typed leaf/Mail swaps, bounded composite `Save_t`/keep-mail walkers, recovery precedence, and staged write slice; the bounded `nookery-input` profile/default/edit/conflict/legacy model plus controller/PAD runtime math and snapshot model; the bounded `nookery-texture` path resolver and PNG/DDS index; the `nookery-config` scalar settings, legacy import, and JSON migration model; the `nookery-famicom` host seam plus expanded native mapper-0/official-6502 opcode-addressing runtime with stack/control-flow/RMW behavior and explicit emulator gaps; the primitive `nookery-bswap` C-oracle slice; the bounded `nookery-arena` model/parity fixture; the bounded `nookery-disc` catalog/header/hash and stream-verification slice; the opaque `nookery-social-proto` frame codec; the bounded `nookery-social-model` hosted-town authority slice; the deterministic `nookery-runtime` lifecycle boundary; and the bounded social persistence/server local vertical slice. Their bounded workspace tests, Clippy/doc gates, bswap/arena oracle harnesses, and data-FFI C harness pass. They build and test in the Rust workspace but remain unwired to CMake/the shipped application.

**What is not:** no CMake integration or C++ parity for the data-manager ABI, the remaining `Save_t`/quest/ARAM/card-manager surfaces, live SDL/Aurora/PADStatus wiring, runtime-compatible game allocator, full NES emulator/fixNES behavior, Aurora texture registration/decoding/GPU/reload behavior, UI, boot code, or hosted-town transport/authentication/client-game integration is ported. The shipped application is still entirely C/C++. The Famicom candidate explicitly leaves full 6502 opcode coverage, mappers, PPU rendering, APU waveforms, DMA/NMI/IRQ/DMC, and the fixNES surfaces open. The social persistence/server crates are bounded clean-room local candidates, not a runnable deployed hosted-town feature and not compatible with Nintendo services. The next acceptance work is C/C++ differential save/input/content parity, CMake activation, completing the Famicom core, real platform/runtime callbacks, and extending the separate hosted-town track through a real authenticated multi-client vertical slice.

## Attribution and license

See the root `LICENSE` for the upstream dual-license terms and the original ACreTeam and PC-port attribution. `LICENSE` (OBSERVED) states: the decompilation code from [ACreTeam/ac-decomp](https://github.com/ACreTeam/ac-decomp) is CC0 1.0; the PC port layer (`pc/` and `#ifdef TARGET_PC` additions) is MIT, © 2026 FlyingMeta.

New Rust code in this fork remains MIT-licensed unless a source file states otherwise. Vendored third-party code keeps its own license — `pc/lib/fixnes/LICENSE` is fixNES (MIT, FIX94). Do not remove upstream copyright or license text when translating code.
