set(CMAKE_SYSTEM_NAME Windows)
# Aurora's package providers key off "AMD64" for Windows, not "x86_64".
set(CMAKE_SYSTEM_PROCESSOR AMD64)
# The -posix variant is a separate GCC/libstdc++ build; the default -win32
# variant ICE-segfaults on several large C++20 template expansions in Dawn.
set(CMAKE_C_COMPILER /usr/bin/x86_64-w64-mingw32-gcc-posix)
set(CMAKE_CXX_COMPILER /usr/bin/x86_64-w64-mingw32-g++-posix)
set(CMAKE_RC_COMPILER /usr/bin/x86_64-w64-mingw32-windres)
# Do NOT set CMAKE_C_COMPILER_TARGET here: CMake turns it into a literal
# `--target=<triple>` argument, which MinGW's GCC rejects. Pass
# -DNOOKERY_RUNTIME_RUST_TARGET=<triple> on the cmake line instead.
set(CMAKE_FIND_ROOT_PATH /usr/x86_64-w64-mingw32)
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
