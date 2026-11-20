# Nookery-RS port ledger

Nookery-RS is a Rust reimplementation of [Nookery](https://github.com/flyngmt/Nookery), maintained as a fork so the original project and contributors remain credited. The fork is not a replacement for the upstream license or attribution notices.

## Scope

- Preserve the observable behavior and supported platform matrix of Nookery.
- Permit only measured RAM/CPU optimizations that do not change gameplay, save formats, input behavior, rendering behavior, or supported user workflows.
- Keep the existing C/C++ implementation available while Rust slices are migrated and verified. A Rust crate compiling is not evidence that the corresponding user path has been ported.
- Do not include or distribute game assets; users must provide a legally obtained supported disc image, as in upstream.

## Source facts from the upstream fork base

- The current upstream default branch is `master` at the fork base commit recorded by Git.
- The application combines roughly 51 MiB of decompilation sources under `src/`, an 11 MiB PC layer under `pc/`, and the Aurora/Borealis submodules under `extern/`.
- The current build is CMake-based, compiles C11/C++20, and uses Aurora/Borealis for platform, UI, and Rust-backed support components.
- The PC data contract is implemented in `pc/src/acgc_data.cpp` and `pc/include/acgc/data.hpp`; the first Rust slice mirrors that contract in `rust/nookery-core` without changing the shipped CMake application.

## Port phases

1. **Inventory and compatibility harness** — enumerate public entry points, data formats, platform adapters, rendering/audio/input boundaries, and existing tests. Establish baseline build and runtime probes for each supported platform.
2. **Leaf, pure-data modules** — port allocation-light path, configuration, serialization, and validation logic first. Compare Rust and upstream outputs on the same fixtures.
3. **Platform services** — port filesystem, threading, timing, input, audio, and window abstractions behind stable Rust traits while retaining adapters for the existing Aurora/Borealis stack.
4. **Game/runtime integration** — migrate one subsystem at a time, preserving ABI boundaries until an end-to-end user-path gate passes.
5. **Performance work** — benchmark matched workloads before and after each optimization. Reject changes that trade correctness or bounded memory for a faster microbenchmark.
6. **Release parity** — build and exercise every supported platform cell, verify saves and user data, and publish only after a separate owner-authorized release decision.

## Current candidate

The bootstrap candidate adds a dependency-free `nookery-core` crate with the upstream data migration constants, required directory layout, and safe relative-path resolution. It is intentionally not labeled as a completed port. The next slice is the data-manager I/O boundary, followed by a fixture-based parity harness against the C++ implementation.

## Attribution and license

See the root `LICENSE` for the upstream dual-license terms and the original ACreTeam and PC-port attribution. New Rust code in this fork remains MIT-licensed unless a source file states otherwise. Do not remove upstream copyright or license text when translating code.
