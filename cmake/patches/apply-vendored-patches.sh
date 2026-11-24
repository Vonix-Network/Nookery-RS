#!/usr/bin/env bash
# Re-apply the vendored-dependency patches required for a MinGW-w64 cross build.
#
# Dawn fetches its own copy of abseil, tint and the DXC headers into each build
# directory, so every patch below is applied to a *fetched* tree and is lost on a
# clean reconfigure. Run this after configuring and before building.
#
# Each patch is idempotent: it checks for its own marker first, so running twice
# is safe.
#
# Usage: cmake/patches/apply-vendored-patches.sh <build-dir>

set -uo pipefail

build_dir=${1:?usage: apply-vendored-patches.sh <build-dir>}
dawn="$build_dir/_deps/dawn-src"

if [ ! -d "$dawn" ]; then
    printf 'REFUSED: %s does not exist; configure first\n' "$dawn" >&2
    exit 2
fi

applied=0
skipped=0

patch_file() { # patch_file <path> <marker> <python-snippet-file>
    local path=$1 marker=$2 snippet=$3
    if [ ! -f "$path" ]; then
        printf 'MISSING %s\n' "$path"
        return
    fi
    if grep -qF "$marker" "$path"; then
        printf 'already %s\n' "$marker"
        skipped=$((skipped + 1))
        return
    fi
    python3 "$snippet" "$path"
    applied=$((applied + 1))
}

# 1. abseil: FOpen gates its Windows branch on _MSC_VER only, but MinGW has
#    fopen_s and no O_NONBLOCK.
absl="$dawn/third_party/abseil-cpp/absl/time/internal/cctz/src/time_zone_info.cc"
if [ -f "$absl" ] && ! grep -q '__MINGW32__' "$absl"; then
    python3 - "$absl" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); t = p.read_text()
old = 'inline FilePtr FOpen(const char* path) {\n#if defined(_MSC_VER)\n'
new = ('inline FilePtr FOpen(const char* path) {\n'
       '// MinGW-w64 has fopen_s and does not define O_NONBLOCK.\n'
       '#if defined(_MSC_VER) || defined(__MINGW32__)\n')
assert old in t, "abseil FOpen anchor missing"
p.write_text(t.replace(old, new, 1))
print("patched abseil FOpen")
PY
    applied=$((applied + 1))
else
    printf 'already abseil FOpen\n'
    skipped=$((skipped + 1))
fi

# 2. Windows.h capitalisation. Resolves on Windows and case-insensitive macOS
#    but not when cross-compiling on Linux.
upper=$(grep -rl '#include <Windows\.h>' "$dawn" 2>/dev/null | wc -l)
if [ "$upper" -gt 0 ]; then
    grep -rl '#include <Windows\.h>' "$dawn" 2>/dev/null | while read -r f; do
        sed -i 's/#include <Windows\.h>/#include <windows.h>/g' "$f"
    done
    printf 'lowercased %s Windows.h includes\n' "$upper"
    applied=$((applied + 1))
else
    printf 'already lowercased Windows.h\n'
    skipped=$((skipped + 1))
fi

# 3. RenderDoc: DAWN_ENABLE_RENDERDOC needs <DXProgrammableCapture.h> from the
#    Windows SDK, which is not vendored. Define the macro only when reachable.
rd="$dawn/third_party/CMakeLists.txt"
if [ -f "$rd" ] && ! grep -q 'DAWN_HAVE_DXPROGRAMMABLECAPTURE' "$rd"; then
    python3 - "$rd" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); t = p.read_text()
old = '''if (NOT TARGET renderdoc-api)
    add_library(RenderDocApi INTERFACE)
    target_include_directories(RenderDocApi INTERFACE "renderdoc")
    target_compile_definitions(RenderDocApi INTERFACE "DAWN_ENABLE_RENDERDOC")
endif()'''
new = '''if (NOT TARGET renderdoc-api)
    add_library(RenderDocApi INTERFACE)
    target_include_directories(RenderDocApi INTERFACE "renderdoc")
    # DAWN_ENABLE_RENDERDOC gates the D3D capture path, which needs
    # <DXProgrammableCapture.h> from the Windows SDK. Only the vendored
    # renderdoc_app.h is present, so define the macro only when the SDK header
    # is reachable (MSVC yes, MinGW-w64 no).
    include(CheckIncludeFileCXX)
    check_include_file_cxx("DXProgrammableCapture.h" DAWN_HAVE_DXPROGRAMMABLECAPTURE)
    if (DAWN_HAVE_DXPROGRAMMABLECAPTURE)
        target_compile_definitions(RenderDocApi INTERFACE "DAWN_ENABLE_RENDERDOC")
    else ()
        message(STATUS "Dawn: DXProgrammableCapture.h unavailable; D3D RenderDoc capture disabled")
    endif ()
endif()'''
assert old in t, "renderdoc anchor missing"
p.write_text(t.replace(old, new, 1))
print("patched RenderDocApi")
PY
    applied=$((applied + 1))
else
    printf 'already RenderDocApi conditional\n'
    skipped=$((skipped + 1))
fi

# 4. DXC headers, only when the D3D12 backend is compiled. Dawn dlopen()s
#    dxcompiler.dll at runtime, so only the headers are compile-time; do NOT
#    enable DAWN_USE_BUILT_DXC (~8,974 extra targets) for three header lines.
nat="$dawn/src/dawn/native/CMakeLists.txt"
if [ -f "$nat" ] && ! grep -q '_dawn_dxc_include_dir' "$nat"; then
    python3 - "$nat" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); t = p.read_text()
anchor = 'if (DAWN_ENABLE_D3D12)\n    if (DAWN_USE_BUILT_DXC)'
add = '''if (DAWN_ENABLE_D3D12)
    # d3d_platform.h includes <dxcapi.h> when the D3D12 backend is enabled.
    # Dawn resolves dxcompiler.dll at runtime, so only the headers are needed at
    # compile time. Wire them in independently of DAWN_USE_BUILT_DXC.
    set(_dawn_dxc_include_dir
        "${CMAKE_CURRENT_SOURCE_DIR}/../../../third_party/directx-shader-compiler/src/include/dxc")
    if (EXISTS "${_dawn_dxc_include_dir}/dxcapi.h")
        target_include_directories(dawn_native_objects PRIVATE "${_dawn_dxc_include_dir}")
        target_include_directories(dawn_native PRIVATE "${_dawn_dxc_include_dir}")
    endif ()
    if (DAWN_USE_BUILT_DXC)'''
if anchor in t:
    p.write_text(t.replace(anchor, add, 1))
    print("wired DXC headers")
PY
    applied=$((applied + 1))
else
    printf 'already DXC headers wired\n'
    skipped=$((skipped + 1))
fi

# 5. MinGW SAL macro: dxcapi.h pulls WinAdapter.h (which defines _Maybenull_)
#    only on the non-_WIN32 path, but MinGW defines _WIN32.
dxc="$dawn/third_party/directx-shader-compiler/src/include/dxc/dxcapi.h"
if [ -f "$dxc" ] && ! grep -q '__MINGW32__' "$dxc"; then
    python3 - "$dxc" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); t = p.read_text()
old = '#ifdef _WIN32\n\n#ifndef CROSS_PLATFORM_UUIDOF'
new = ('#ifdef _WIN32\n\n'
       '// MinGW-w64 defines _WIN32 but lacks the Windows SDK SAL macros used\n'
       '// below, and WinAdapter.h (which defines them) is only included on the\n'
       '// non-Windows path.\n'
       '#if defined(__MINGW32__) && !defined(_Maybenull_)\n'
       '#define _Maybenull_\n'
       '#endif\n\n'
       '#ifndef CROSS_PLATFORM_UUIDOF')
if old in t:
    p.write_text(t.replace(old, new, 1))
    print("patched dxcapi SAL")
PY
    applied=$((applied + 1))
else
    printf 'already dxcapi SAL\n'
    skipped=$((skipped + 1))
fi

# 6. D3DError.h: winerror.h uses HRESULT without defining it; MSVC gets the
#    typedef from the SDK include chain, MinGW does not.
d3de="$dawn/src/dawn/native/d3d/D3DError.h"
if [ -f "$d3de" ] && ! grep -q 'wtypesbase.h' "$d3de"; then
    python3 - "$d3de" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); t = p.read_text()
old = '#include <winerror.h>\n'
new = ('// winerror.h uses HRESULT but does not define it; MSVC gets the typedef\n'
       '// from the SDK include chain, MinGW does not.\n'
       '#include <wtypesbase.h>\n'
       '#include <winerror.h>\n')
if old in t:
    p.write_text(t.replace(old, new, 1))
    print("patched D3DError HRESULT")
PY
    applied=$((applied + 1))
else
    printf 'already D3DError HRESULT\n'
    skipped=$((skipped + 1))
fi

# 7. Submodule patches (aurora, borealis). These live in checked-out submodules,
#    not the fetched _deps tree, so they survive a reconfigure but not a
#    `git submodule update`. Apply them from the captured patch files.
root=$(cd "$(dirname "$0")/../.." && pwd)
for spec in "aurora:extern/aurora" "borealis:extern/borealis"; do
    name=${spec%%:*}; dir=${spec##*:}
    patch="$root/cmake/patches/submodules/${name}-mingw.patch"
    [ -f "$patch" ] || continue
    if git -C "$root/$dir" diff --quiet 2>/dev/null; then
        git -C "$root/$dir" apply "$patch" && printf 'applied %s submodule patch\n' "$name"
        applied=$((applied + 1))
    else
        printf 'already %s submodule patched\n' "$name"
        skipped=$((skipped + 1))
    fi
done

printf '\nAPPLIED=%s ALREADY=%s\n' "$applied" "$skipped"
printf 'Re-run cmake on %s so vendored CMake edits take effect.\n' "$build_dir"
