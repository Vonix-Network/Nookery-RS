# Nookery-RS port wave 7 — host/runtime seam activation

This candidate activates only the narrow `nookery-runtime` C ABI seam. It is
not a complete Rust game port and does not replace the existing C/C++ gameplay
path.

## Activated

- `rust/nookery-runtime` now emits a release `cdylib` as well as its Rust
  library. The shared image is required because Aurora/Nod already contributes
  Rust `std` to the host through `libnod.a`; a static Rust archive would define
  `rust_eh_personality` a second time. CMake invokes Cargo with `--release
  --locked --package nookery-runtime`; `NOOKERY_RUNTIME_RUST_TARGET` selects an
  explicit Rust target triple, or an empty value uses Cargo's host target.
- `pc/include/acgc/nookery_runtime.h` mirrors the existing `#[repr(C)]`
  callback/config/status ABI using opaque handles and fixed-width C values.
  It does not expose Rust references or Rust unwinding.
- `pc/src/nookery_runtime_adapter.cpp` supplies real C++ callbacks and a
  deterministic diagnostic path for create → boot → frame → reset → PRENMI →
  shutdown → destroy. It checks and prints every status code and callback
  order, and uses no disc image or game resource.
- With `-DENABLE_NOOKERY_RUNTIME_SMOKE=ON`, CMake builds the Rust shared image,
  links the adapter into the existing PC target, builds
  `nookery-runtime-smoke`, and exposes the `nookery-runtime-smoke-test` target.
  The exact Cargo image is copied beside each opt-in host binary and resolved
  through a relative build/install RPATH; it is also installed beside the
  application. The same opt-in host binary accepts `--runtime-smoke` and exits
  before Aurora/disc setup after running that adapter path.

## Still open

The live application continues to enter through `boot_main` and the existing
C graph/game loop. The adapter's callbacks are diagnostic only; they do not
initialize Aurora, the renderer, audio, disc/data loading, title/village
state, save/reload, or any complete game boot path. The runtime's documented
automatic source reset-frame transition remains an explicit parent scheduler
gap. Cross-platform target triples still need to be selected and exercised in
each platform build cell before this seam can be considered broadly accepted.
