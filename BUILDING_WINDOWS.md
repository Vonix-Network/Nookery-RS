# Windows (MinGW-w64) build notes for Nookery-RS

Native Windows builds are supported with MinGW-w64 + the `x86_64-pc-windows-gnu`
Rust target. The Linux and macOS builds are unchanged.

## Toolchain

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt-get install -y --no-install-recommends \
    mingw-w64 g++-mingw-w64-x86-64 protobuf-compiler libz-mingw-w64-dev
```

`protoc` is needed because Dawn's build generates code with the host compiler
while targeting Windows. `libz-mingw-w64-dev` supplies a Windows-targeted zlib;
the host one is the wrong architecture.

## Configure

`cmake/toolchain-mingw-w64.cmake` is provided. Two details matter and are easy
to get wrong:

- `CMAKE_SYSTEM_PROCESSOR` must be `AMD64`, not `x86_64`. Aurora's packaged
  dependency resolution keys off the Windows spelling, and `x86_64` makes it
  report "no prebuilt package for Windows/x86_64".
- `CMAKE_C_COMPILER_TARGET` must be `x86_64-pc-windows-gnu`. It drives
  `NOOKERY_RUNTIME_RUST_TARGET`, and without it Cargo emits host artifacts into
  `release/` while CMake looks for target ones.

```bash
cmake -S . -B build-win -G Ninja \
  -DCMAKE_TOOLCHAIN_FILE=cmake/toolchain-mingw-w64.cmake \
  -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  -DAURORA_DAWN_PROVIDER=vendor \
  -DAURORA_SDL3_PROVIDER=vendor \
  -DAURORA_NOD_PROVIDER=vendor \
  -DPROTOC_EXECUTABLE=/usr/bin/protoc \
  -DZLIB_LIBRARY=/usr/x86_64-w64-mingw32/lib/libz.a \
  -DZLIB_INCLUDE_DIR=/usr/x86_64-w64-mingw32/include \
  -DDAWN_SUPPORTS_CXX_MODULES=OFF
```

`AURORA_NOD_PROVIDER=vendor` is required for MinGW: the published nod Windows
packages are MSVC-built and carry `.lib` import libraries that MinGW cannot
consume. `DAWN_SUPPORTS_CXX_MODULES=OFF` is required because MinGW-w64's GCC
reports C++20 module support that CMake cannot drive (no import-graph scanner),
which fails the generate step in Dawn's `dawncpp_module` target.

## Source changes this required

All are genuine portability defects in the tree, not test hacks:

1. `CMakeLists.txt` gated the debug-symbol install on `WIN32` but used
   `$<TARGET_PDB_FILE>`, which only exists for MSVC. MinGW puts DWARF inside the
   PE, so the install is now gated on `MSVC` with a `WIN32` fallback.
2. The `nookery-audio` and `nookery-runtime` Rust images were consumed as shared
   libraries on Windows. MinGW-w64 refuses to link a DLL with unresolved
   imports, and both images reference host seams (`nookery_audio_host_*`,
   `pc_audio_process_frame`, the runtime callback table) that live in the
   executable. They are now built as static archives on Windows and linked in,
   with no linker suppression, no weak symbols and no stubs.
3. `nookery-audio` declares `crate-type = ["cdylib", "rlib", "staticlib"]`, so a
   plain `cargo build` also attempts the cdylib. On Windows the build uses
   `cargo rustc --lib --crate-type staticlib` to produce only the archive.
4. `include/types.h` included `<alloca.h>` for every non-MSVC compiler. MinGW has
   no `alloca.h` (it is in `<malloc.h>`), so the guard now covers `__MINGW32__`.

## Vendored dependency patch

Dawn vendors abseil, whose `time_zone_info.cc` gates its Windows `FOpen` branch
on `_MSC_VER` only and otherwise uses `O_NONBLOCK`, which MinGW does not define.
`cmake/patches/abseil-mingw-fopen.patch` is applied to the fetched abseil tree
before building. It extends that guard to `__MINGW32__`. Upstream fix pending.

## Verify

```bash
ninja -C build-win acgc
file build-win/bin/Nookery.exe          # PE32+ executable (GUI) x86-64
```

`bin/Nookery.exe` runs on Windows with a user-supplied disc image:

```
Nookery.exe --user-dir <dir> <disc image>
```
