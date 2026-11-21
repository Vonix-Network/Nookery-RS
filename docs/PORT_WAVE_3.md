# Port wave 3 — dispatch record

Wave base: `05167b482a2cd3d5634c01e288412df18fd9b594` (tree `75c75739f6790bb99ba958e878cefd688880aef7`).
Risk class: **R3** (save bytes, emulator output, network/auth surface).
Integration rule: each lane returns a candidate only. The parent exports an exact patch, applies it to the canonical tree with `git apply --index`, runs the parent-owned gates on the exact staged tree, then records a successor pin. Independent read-only review is obtained before any acceptance, and acceptance never authorizes push or deployment.

## Why this wave

The workspace was 15 bounded crates and 29,147 Rust lines when this wave was dispatched, against 5,885 tracked C/C++ files and ~1,004,742 lines. Every crate currently proves a *primitive*, not a user path. Wave 3 targets the four places where a real user path is closest and where the oracle is small enough to port completely rather than approximate:

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

| Lane | State |
|---|---|
| `save-card-v3` | **Integrated** `3b4638c5` — `rust/nookery-save/src/manager.rs` (+1069), `tests/card_manager.rs` (+246). Parent gates: fmt, 65 save tests, clippy, doc, bswap, arena, data-FFI all PASS; 443 workspace tests. Parent negative controls: reverting the same-town skip and dropping the 64-entry cap each failed their named assertion. |
| `settings-abi-v3` | **Integrated** `b73529d3` — `PCSettings` `#[repr(C)]` ABI + `config.json` codec + `tools/settings-abi` harness (70 checks). Parent cross-checked all 16 offsets/size/alignment against the real `pc_settings.h` by compiling each header separately; identical. Repair round v3a fixed a blocker: the crate header defined a duplicate `PCSettings`, so no TU could include both headers. |
| `famicom-ppu-v3` | **Integrated** `b73529d3` — `rust/nookery-famicom/src/ppu.rs` + `tests/ppu.rs`: registers/latches/buffering/increment/mirroring/palette/OAM, background, sprites, frame hash. 68 Famicom tests. |
| `social-transport-v3` | **HELD BACK — not integrated.** Parent ran the two loopback tests the lane's sandbox skipped; both failed. (1) Encoder emits a 17-byte `Accepted` payload while the client decoder requires exactly 9 → `PARENT_OBSERVED_CREATE_TOWN=Err(Protocol("snapshot payload"))`. (2) An idle authenticated peer is refused with a typed `Error` frame (code 10), not the `TimedOut` IO error the test asserts → `PARENT_OBSERVED_IDLE_RESULT=Ok(Frame{message_type: Error, payload: [1, 10]})`. Repair lane `social-transport-v3a` dispatched; also required to make sandbox skips explicit so a real failure cannot masquerade as a skip. |

Wave 3 is **PARTIALLY INTEGRATED**. The hosted-town transport lane remains unaccepted pending repair and fresh gates. No external effects are authorized by this record; nothing is pushed.

## Host gate resolved (post-wave)

The host blocker that prevented phase 0 from completing is now closed. `cmake --preset relwithdebinfo` configures successfully and `ninja` links `bin/Nookery` (5,027/5,027 targets, zero errors). Root cause was SDL3's development dependencies, discovered iteratively from real CMake errors rather than guessed: missing X11/Wayland headers first, then `XTEST` specifically.

Verified end-to-end with the owner-supplied USA/Canada disc (`GAFE01`, CISO):

- `wit` (Wiimms ISO Tool) reads the CISO as a valid `CISO/GC GameCube` image: `disc=GAFE01`, `AnimalCrossing`, 16 files.
- `Nookery <ciso>` runs the app's own `acgc::disc::inspect` gate and proceeds to launch: zero "not a supported Animal Crossing image" warnings, zero prelaunch-menu entries.
- The application reaches real game boot code (`sound_initial`, `initial_menu_init`, `dvderr_init`, `sound_initial2`, COPYDATE, string-table load) and initializes Vulkan.

This validates the *build and launch* host path only. Phases 7 and 8 remain unported: the runtime is still the C implementation. Frame capture under Xvfb is black because Vulkan presentation needs DRI3, so visual gates are not yet achievable headlessly on this host.
