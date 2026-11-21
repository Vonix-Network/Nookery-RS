#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SOURCE="$SCRIPT_DIR/../../rust/nookery-data-ffi/src/abi.rs"
BACKUP=$(mktemp "${TMPDIR:-/tmp}/nookery-data-ffi-abi.XXXXXX")
LOG=$(mktemp "${TMPDIR:-/tmp}/nookery-data-ffi-mutation.XXXXXX")

cleanup() {
    cp "$BACKUP" "$SOURCE"
    rm -f "$BACKUP" "$LOG"
}
trap cleanup EXIT HUP INT TERM

cp "$SOURCE" "$BACKUP"
sed -i 's/2 => Self::TexturePack,/2 => Self::NesRoms,/; s/3 => Self::NesRoms,/3 => Self::TexturePack,/' "$SOURCE"

if make -C "$SCRIPT_DIR" run >"$LOG" 2>&1; then
    echo "mutation unexpectedly passed"
    cat "$LOG"
    exit 1
fi
if ! grep -q 'HARNESS_RESULT=FAIL' "$LOG"; then
    echo "mutation failed outside the parity assertion"
    cat "$LOG"
    exit 1
fi

echo "MUTATION_RESULT=PASS"
echo "mutation: swapped Rust directory selectors 2 and 3"
echo "observed: tools/data-ffi harness returned nonzero with HARNESS_RESULT=FAIL"
