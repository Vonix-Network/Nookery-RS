# Nookery-RS completion wave

Owner directive: **finish the complete Rust port** and the separately tracked clean-room hosted-town multiplayer feature. This record is parent-owned coordination state; it does not claim acceptance, runtime completion, deployment, or publication.

## Baseline

- Repository: `Vonix-Network/Nookery-RS`, upstream: `flyngmt/Nookery`.
- Bootstrap ancestor: `03ff4a17f7f1fa1cc083b6e7c3a0ff21927bb15e`.
- Current Rust workspace: 15 bounded crates, 132 tracked Rust-workspace files, 29,051 lines.
- Remaining oracle surface: 5,885 tracked C/C++ header/source files, approximately 1,004,742 lines.
- CMake application configure is currently blocked on this host by missing X11/Wayland development libraries; this remains an environment blocker, not a port completion claim.

## Non-negotiable contract

- Rust behavior must remain 1:1 with the C/C++ implementation; only measured RAM/CPU optimizations are permitted after parity.
- Keep C/C++ as the source oracle until the six conditions in `PORT_CLEANUP_LEDGER.md` pass for the exact source unit and all production callers.
- Do not add game assets, ROMs, disc images, extracted textures, credentials, or proprietary Nintendo network compatibility.
- Multiplayer is clean-room and must preserve the offline/single-player path.
- Workers edit isolated worktrees and return candidates. Oro integrates candidates, runs fresh workspace gates, records acceptance, and performs no deployment/publication effect.
- Every lane must produce a substantive source candidate or an exact blocker; a survey, plan, or passing isolated stub is not completion.

## Initial disjoint implementation lanes

| Lane | Owned paths | Source contract | Required evidence |
|---|---|---|---|
| Save composite | `rust/nookery-save/**` | Complete the next save vertical slice from `pc/src/pc_gci.c`, `pc/src/pc_save_bswap.c`, `pc/include/pc_gci.h`, and disk-layout headers: composite `Save_t`/quest/ARAM/card I/O, preserving failure modes and persisted bytes. | C-oracle vectors, size/offset assertions, round-trip and failure-path tests; no claim of runtime activation until wired. |
| Famicom runtime | `rust/nookery-famicom/**` | Extend the host seam into a source-grounded executable Rust Famicom path using `src/famicom_emu.c` and `src/static/Famicom/**`; preserve frame/input/audio/exit behavior. | Deterministic frame/input/exit tests and synthetic ROM/state fixtures; no bundled game ROM. |
| Content runtime | `rust/nookery-disc/**`, `rust/nookery-texture/**` | Port the next real content-loading slice from `pc/src/acgc_disc.cpp`, `pc/src/acgc_texture_replacements.cpp`, and Aurora/Borealis interfaces: image-format inspection/verification and texture indexing/path behavior. | User-supplied/synthetic inputs only, error-path vectors, index/path precedence tests, explicit dependency blockers. |
| Platform input | `rust/nookery-input/**` | Port controller-port assignment and live PADStatus/deadzone behavior from `pc/src/acgc_controller_ports.cpp`, `pc/src/acgc_input_profiles.cpp`, and `src/padmgr.c`; retain legacy migration. | C/Rust differential status vectors at deadzone/error/hotplug boundaries; do not alter shared workspace manifests. |
| Game runtime | `rust/nookery-runtime/**` | Create the first executable Rust runtime boundary for `src/main.c`, `src/graph.c`, `src/game.c`, and `src/executor.c`: deterministic boot/frame/tick lifecycle with explicit host adapters. | State-machine, frame timing, reset, and shutdown tests; real call-path hooks named; no fake “full port” claim. |
| Hosted-town server | `rust/nookery-social-persistence/**`, `rust/nookery-social-server/**` | Complete crash-safe persistence plus a bounded clean-room server vertical slice over the integrated protocol/model: town create, discover, join/leave, role authorization, revisioned commands, reconnect/idempotency, offline preservation. | Multi-client simulation, hostile-packet negatives, crash/recovery, duplicate-delivery, reconnect, and offline regression tests; no deployment. |

The parent may add further disjoint game/data/UI lanes after the first cohort is reconciled. Shared manifests, CMake, runtime registries, and this status file remain parent-owned integration surfaces.

## Integration and completion order

1. Freeze this wave's resulting commit as the candidate base and launch only the listed owned paths.
2. Reconcile each candidate against the exact base and its source oracle; reject report-only or scope-drifting output.
3. Integrate by changed-path union, never wholesale worktree copying. Repair workspace manifests and shared seams only on the parent lane.
4. Run fresh formatting, workspace tests, clippy/doc gates, source-grounded differential tests, and activation/call-path checks.
5. Repeat disjoint waves until phases 1–9 have executable user paths. Only then run the deletion ledger for each complete source unit; no blanket C/C++ removal.
6. Finish with the end-to-end boot → title → village → save → reload and hosted-town multi-client gates across the available platform cells. Any unavailable cell remains an explicit blocker.

## Current status

### Reconciled candidate

- Famicom successor lane: base `08a505b035a33f584bcd35b99badcf2cc442eefe`; parent-exported diff `famicom-full-v3.diff` (`2cd9c9c3f1c1d87cfd46f52627a7fc73b18e94317adae97ad871470dafbfe114`) changed only `rust/nookery-famicom/src/core.rs` and `rust/nookery-famicom/tests/runtime.rs`.
- Integrated code commit: `7921579223b231e929435de571542e67edf83848`, tree `87f5b0ed2400e52c97c0ffacfa421fa62667b7cb`; the worker could not update its linked-worktree ref because Git metadata was read-only, so the parent applied the exact checked patch.
- Parent-owned fresh gates: 426 Rust tests passed; formatting, Clippy `-D warnings`, docs, bswap oracle (2,304 + 20), arena parity, and data-FFI harness (219 checks) passed.
- Verified implementation: expanded official 6502 opcode/addressing coverage, stack/control-flow and read-modify-write operations, page-crossing branches, NMOS indirect-JMP wrapping, CPU-path PPU/APU register boundaries, mapper-0 iNES validation, controller shift behavior, audio-before-frame ordering, 60-frame exit fade, and cleanup semantics.
- Explicitly open: remaining 6502 opcodes, non-zero mappers, PPU rendering/VRAM semantics, APU waveforms, DMA/NMI/IRQ/DMC timing, fixNES parity, CMake/application activation, and real ROM visual/audio parity.

This wave is **ASSIGNED / NOT ACCEPTED** until exact candidate identities, changed paths, gates, and blockers are read back. No external effects are authorized by this record.
