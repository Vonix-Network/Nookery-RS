#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SOURCE="$SCRIPT_DIR/../src/lib.rs"
BACKUP=$(mktemp "${TMPDIR:-/tmp}/nookery-actor-lib.XXXXXX")
TARGET=$(mktemp -d "${TMPDIR:-/tmp}/nookery-actor-target.XXXXXX")
LOG=$(mktemp "${TMPDIR:-/tmp}/nookery-actor-mutation.XXXXXX")

cleanup() {
    cp "$BACKUP" "$SOURCE"
    rm -f "$BACKUP" "$LOG"
    rm -rf "$TARGET"
}
trap cleanup EXIT HUP INT TERM

cp "$SOURCE" "$BACKUP"
sed -i 's/const MODE_NUM: i32 = 2;/const MODE_NUM: i32 = 1;/' "$SOURCE"

if CARGO_TARGET_DIR="$TARGET" cargo test --manifest-path "$SCRIPT_DIR/../Cargo.toml" --all-targets >"$LOG" 2>&1; then
    echo "mutation unexpectedly passed"
    cat "$LOG"
    exit 1
fi

echo "MUTATION_RESULT=PASS"
echo "mutation: changed MODE_NUM from 2 to 1"
echo "observed: lifecycle tests rejected the mutated dispatch bound"
