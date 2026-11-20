#!/bin/sh
# Regenerate parity/expected.txt from the UNMODIFIED upstream sources.
#
#   sh rust/nookery-arena/parity/run.sh
#
# The harness compiles src/TwoHeadArena.c and src/gamealloc.c at 32-bit width
# with a freestanding libc shim (no 32-bit glibc needed) and dumps one line per
# observable step.  tests/parity.rs replays the same steps through the crate and
# requires byte-identical output.
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cc=${CC:-gcc}

# The copies below must stay byte-identical to the pin's sources; refresh and
# verify instead of trusting a stale copy.
repo=$(CDPATH= cd -- "$here/../../.." && pwd)
for f in TwoHeadArena.c gamealloc.c; do
    cmp -s "$repo/src/$f" "$here/$f" || {
        echo "parity: $here/$f is stale relative to $repo/src/$f" >&2
        echo "parity: re-copy and re-run" >&2
        exit 1
    }
done

"$cc" -m32 -nostdlib -static -ffreestanding -fno-builtin -fno-stack-protector \
    -fno-pic -I"$here/include" -o "$here/driver32" \
    "$here/driver.c" "$here/TwoHeadArena.c" "$here/gamealloc.c"
"$here/driver32" > "$here/expected.txt"
rm -f "$here/driver32"
echo "parity: wrote $here/expected.txt"
