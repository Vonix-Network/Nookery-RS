# Nookery-RS port wave 5 — executable runtime and user-path closure

**State:** PLANNED / not accepted.

**Purpose:** close the next source-grounded Rust verticals required before the final boot → title → village → save → reload and hosted-town user-path gates. This wave does not claim a complete port, game-resource parity, or multiplayer deployment.

## Contract

- The C/C++ tree remains the source oracle and is not deleted or edited by workers.
- Rust behavior remains 1:1; only measured RAM/CPU optimizations may be proposed after parity.
- Workers edit isolated worktrees and return candidates. Oro owns integration, fresh gates, acceptance, and the separate GitHub push effect.
- Shared `Cargo.toml`, `Cargo.lock`, CMake, runtime registries, and this document remain parent-owned integration surfaces.
- No game assets, ROMs, disc images, extracted proprietary data, credentials, deployment, publication, or public network exposure may enter the wave.

## Pinned base

The implementation pin is recorded outside the repository in the Hermes scratch evidence directory. Every successor candidate must name its exact base commit/tree and changed-path manifest.

## Disjoint lanes

| Lane | Owned paths | Required candidate outcome |
|---|---|---|
| Runtime boundary | `rust/nookery-runtime/**` | Expand the deterministic runtime into a source-grounded executable boundary for boot/frame/reset/shutdown, adapter error propagation, and a stable host-call seam. Tests must prove the named production-call contract without pretending that CMake activation already exists. |
| Famicom completion | `rust/nookery-famicom/**` | Port the next complete source-grounded CPU, mapper, APU, DMA, interrupt, or PPU behavior slice from the fixNES/Famicom oracle, with differential/edge tests and mutation evidence. |
| Content path | `rust/nookery-disc/**`, `rust/nookery-texture/**` | Close a real disc/content or texture replacement path beyond RARC/Yaz0, including precedence/error behavior and synthetic or owner-supplied fixture tests. |
| Input path | `rust/nookery-input/**` | Close controller-port assignment, PAD/deadzone/hotplug behavior and legacy migration seams against the C input oracle, with boundary vectors and failure tests. |
| Save path | `rust/nookery-save/**` | Close the next composite save/GCI/ARAM byte-layout vertical, including malformed data, round-trip bytes, and failure-mode parity. |
| Hosted town | `rust/nookery-social-model/**`, `rust/nookery-social-persistence/**`, `rust/nookery-social-proto/**`, `rust/nookery-social-server/**` | Strengthen clean-room multi-client behavior: in-memory handler coverage, authorization negatives, revision/idempotency/reconnect, crash/recovery, and loud capability-free tests. No deployment or public service. |

## Parent integration gates

1. Reconcile each candidate against the exact wave pin and reject scope drift, generated artifacts, resources, C/C++ edits, and shared-surface edits.
2. Apply changed-path unions only; preserve all sibling lane hunks and verify each lane's distinctive tests/symbols remain registered.
3. Run fresh Rust formatting, all-target workspace tests, Clippy with `-D warnings`, and docs on the exact integrated candidate.
4. Run each lane's source-grounded differential, negative, and mutation gate again from the parent tree; capability-dependent tests must not silently skip.
5. Wire only after the component candidate is accepted; CMake/host entrypoint activation is a separate parent-owned successor candidate with its own build and real-user-path gate.
6. The final completion gate remains open until a legal owner-supplied disc can exercise boot/title/village/save/reload and the hosted-town multi-client path; unavailable platform/display cells remain explicit blockers.

**Wave 5 is not completion by itself.** A green crate, source presence, process status, or worker report is candidate evidence only.