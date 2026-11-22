#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
GENERATED="$ROOT/rust/nookery-data-assets/src/generated.rs"
BACKUP=$(mktemp "${TMPDIR:-/tmp}/nookery-titledemo-generated.XXXXXX")
LOG=$(mktemp "${TMPDIR:-/tmp}/nookery-titledemo-mutation.XXXXXX")

cleanup() {
    cp "$BACKUP" "$GENERATED"
    rm -f "$BACKUP" "$LOG"
}
trap cleanup EXIT HUP INT TERM

cp "$GENERATED" "$BACKUP"
sed -i '0,/0x0884/s//0x0885/' "$GENERATED"

TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/target-titledemo-mutation"}
if cargo test --manifest-path "$ROOT/rust/Cargo.toml" --package nookery-data-assets \
    --test source_parity every_titledemo_source_hash_count_and_byte_stream_matches_rust \
    --target-dir "$TARGET_DIR" >"$LOG" 2>&1; then
    echo "mutation unexpectedly passed"
    cat "$LOG"
    exit 1
fi
if ! grep -q 'test result: FAILED' "$LOG"; then
    echo "mutation failed outside the parity assertion"
    cat "$LOG"
    exit 1
fi

echo "MUTATION_RESULT=PASS"
echo "mutation: changed pact0_head_table[0] from 0x0884 to 0x0885"
echo "observed: source-to-Rust parity test failed"
