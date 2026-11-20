# Nookery-RS source cleanup ledger

This ledger governs removal of the original C/C++ source as the Rust port reaches full behavioral parity. The upstream repository remains available, and Git history preserves any deleted paths, but upstream availability is not itself proof that a local deletion is safe.

## Deletion rule

A source unit may be removed only in a dedicated cleanup commit after all of these are true for the exact unit and every production caller it serves:

1. The complete source surface is mapped to Rust, not only a bounded helper or model.
2. All production callers are migrated or removed intentionally; no CMake target, link rule, runtime loader, or platform cell still depends on the unit.
3. The C/C++ oracle and Rust replacement pass source-grounded differential/behavioral gates, including failure paths and persisted bytes where applicable.
4. The Rust replacement is exercised through the real application path, not only unit tests or an isolated crate.
5. The required platform/build matrix and user-path gates pass, or an exact environment blocker is recorded.
6. The deletion is reviewed against the exact candidate commit, then performed with `git rm`, followed by fresh workspace/build tests and remote read-back.

Until all six conditions pass, the original source is **KEEP — ORACLE/UNPORTED**, even when a Rust candidate compiles and its local tests pass.

## Current source-unit disposition

| Original source unit | Rust candidate | Current disposition | Why it remains |
|---|---|---|---|
| `pc/src/acgc_data.cpp` | `rust/nookery-core`, `rust/nookery-data-ffi` | **KEEP — partial** | Bounded data-folder ABI candidate exists; C++ parity, CMake wiring, and application activation are unproven. |
| `pc/src/pc_gci.c` | `rust/nookery-save` | **KEEP — partial** | GCI validation and recovery seams exist; writing, import, complete card behavior, and runtime save flow remain C-backed. |
| `pc/src/pc_save_bswap.c` | `rust/nookery-bswap`, `rust/nookery-save` | **KEEP — partial** | Primitive swaps, five leaf mirrors, and a Mail slice exist; composite `Save_t`, quest, ARAM, and full walker parity remain open. |
| `pc/src/acgc_settings.cpp` | `rust/nookery-config` | **KEEP — partial** | Settings model and migration exist; C ABI/runtime consumers and real-install behavior remain. |
| `pc/src/acgc_input_profiles.cpp` | `rust/nookery-input` | **KEEP — partial** | Defaults/edit/conflict/legacy model exists; live SDL/Aurora/PADStatus integration remains. |
| `pc/src/acgc_controller_ports.cpp` | `rust/nookery-input` | **KEEP — unported** | Controller-port runtime assignment and migration behavior are not ported. |
| `pc/src/acgc_legacy_input.cpp` | `rust/nookery-input` | **KEEP — partial** | Pure migration logic is modeled; application integration and complete serialization remain. |
| `pc/src/acgc_disc.cpp` | `rust/nookery-disc` | **KEEP — partial** | Catalog/header/hash seam exists; Aurora image decoding and launch path remain. |
| `pc/src/acgc_texture_replacements.cpp` | `rust/nookery-texture` | **KEEP — partial** | Path precedence exists; registration, decoding, GPU integration, reload, and user-path behavior remain. |
| `pc/src/pc_nes_fixnes.c` and `src/famicom_emu.c` | `rust/nookery-famicom` | **KEEP — partial** | Host/ROM/exit seams exist; emulator and fixNES runtime parity remain. |
| `src/gamealloc.c`, `src/TwoHeadArena.c`, `src/THA_GA.c`, related allocator callers | `rust/nookery-arena` | **KEEP — partial** | Bounded model/parity fixture exists; live layout/ABI/address behavior is not integrated. |
| `src/`, `pc/`, and remaining host/game units | None or bounded candidates | **KEEP — unported** | The shipped application is still C/C++; most game, actor, UI, boot, and platform behavior has no Rust replacement. |

## First eligible deletion

**None is currently eligible.** Removing any listed file now would delete an active oracle or an unported production path, not clean up a completed port.

The first deletion candidate will be recorded here only after its full replacement is integrated, activated, and passed through the deletion rule above. Cleanup commits must remain separate from feature-port commits so a regression can be reverted without recovering unrelated work.

## Effects and preservation

- No source deletion is performed by this ledger update.
- Original C/C++ files remain available as local oracles until their row closes.
- GitHub `Vonix-Network/Nookery-RS` and the upstream `flyngmt/Nookery` lineage remain unchanged by this record.
- A future deletion will be a separate owner-authorized Git commit and will be read back from `origin/master` after pushing.
