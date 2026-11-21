# Port wave 3 — dispatch record

Wave base: `05167b482a2cd3d5634c01e288412df18fd9b594` (tree `75c75739f6790bb99ba958e878cefd688880aef7`).
Risk class: **R3** (save bytes, emulator output, network/auth surface).
Integration rule: each lane returns a candidate only. The parent exports an exact patch, applies it to the canonical tree with `git apply --index`, runs the parent-owned gates on the exact staged tree, then records a successor pin. Independent read-only review is obtained before any acceptance, and acceptance never authorizes push or deployment.

## Why this wave

The workspace is 15 bounded crates and 29,147 Rust lines against 5,885 tracked C/C++ files and ~1,004,742 lines. Every crate currently proves a *primitive*, not a user path. Wave 3 targets the four places where a real user path is closest and where the oracle is small enough to port completely rather than approximate:

| Lane | Oracle | Why it is next |
|---|---|---|
| `save-card-v3` | `pc/src/pc_m_card.c` (1,133 lines), `include/m_card.h` | Decides which town loads, Card B visitor towns, ARAM staging, reset codes, legacy migration. This is the last save-layer gap between "validator + typed swaps" and "the game can load a village". |
| `settings-abi-v3` | `pc/src/acgc_settings.cpp` (458), `pc/include/pc_settings.h` | `PCSettings` is read by C consumers through `g_pc_settings`; without a `#[repr(C)]` mirror and a real `config.json` codec the model can never replace the C path. |
| `famicom-ppu-v3` | `pc/lib/fixnes/ppu.c` (2,124), `ppu.h`, `mem.c` | The Rust Famicom core can execute 6502 code but renders no picture. Without PPU the emulator slice cannot satisfy any visual gate. |
| `social-transport-v3` | existing social crates | Hosted towns have framing and authority logic but no socket, no authentication, and no negative security tests — so no hosted-town user path exists. |

## Lane contracts

Each brief is in `~/.hermes/profiles/oro/cache/scratch/nookery-rs-<lane>-codex.md`. Every lane:

- works in `/home/vibegonex/DEV/.nookery-rs-workers/<lane>` at the wave base, detached;
- must not push, deploy, contact upstream, delete C/C++, or add assets/ROMs/fixtures;
- must use an external `CARGO_TARGET_DIR` and leave no `target/` or new `Cargo.lock`;
- must run a **mutation check** per claimed behavior and report the concrete failing assertion;
- must list explicit `NOT_PORTED` gaps rather than implying completion.

## Status

| Lane | Process | State |
|---|---|---|
| `save-card-v3` | `proc_16ede28f2cd6` | dispatched |
| `settings-abi-v3` | `proc_69e22fd82386` | dispatched |
| `famicom-ppu-v3` | `proc_45b0316eb4e6` | dispatched |
| `social-transport-v3` | `proc_b09f274118d7` | dispatched |

This wave is **ASSIGNED / NOT ACCEPTED**. No external effects are authorized by this record.
