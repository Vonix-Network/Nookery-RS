#!/bin/sh
# Prove that the source-sensitive timer-clock operator is covered by a test.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
source="$here/../rust/nookery-libc64/src/sleep.rs"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

cp "$source" "$work/sleep.rs"
sed -i 's/bus_clock_hz \/ 4/bus_clock_hz \/ 8/' "$work/sleep.rs"

rustc --edition=2024 --test "$work/sleep.rs" -o "$work/mutated-tests"
if "$work/mutated-tests" --exact tests::c_conversion_keeps_the_source_integer_order; then
    echo "mutation unexpectedly passed" >&2
    exit 1
fi

echo "MUTATION_RESULT=PASS (bus-clock divisor mutation rejected)"
