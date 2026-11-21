# Nookery-RS port wave 4 — RARC, data ABI activation, Famicom, Save/GCI

**State:** ASSIGNED / candidate lanes running; not accepted.

**Pinned base:** `c38c4e154c1e77b0beaae4eaa8916c1534bd33cd`
**Base tree:** `ad97bf20e90bbed7afe9add8e2bcf8cdd72d1f3c`
**Route:** Codex CLI `gpt-5.6-luna` via the owner's ChatGPT/Codex subscription.
**Parent pin:** `/home/vibegonex/.hermes/profiles/oro/cache/scratch/nookery-rs-wave4-pin-20261121.md`

## Why this wave

The current Rust workspace has 15 bounded crates and a green workspace gate, but the
port ledger still has four important gaps that can be advanced without touching the
shared C/C++ implementation:

- `nookery-disc` has a verified Yaz0 decoder but no RARC reader;
- `nookery-data-ffi` has an ABI candidate and harness but no CMake activation/parity
  target;
- `nookery-famicom` remains a bounded emulator slice with explicit CPU/mapper/interrupt
  gaps;
- `nookery-save` has GCI/card-manager seams but not the remaining composite Save/GCI
  byte-layout behavior.

Each lane owns disjoint Rust/tool paths. The C/C++ tree remains the oracle and is not
deleted or edited by workers. User-supplied disc/ROM/save data remains outside Git.

## Lanes and ownership

| Lane | Owned paths | Intended gate |
|---|---|---|
| RARC | `rust/nookery-disc/**`, `tools/rarc-oracle/**` | Checked RARC header/node/entry/string/data bounds, nested lookup, malformed input rejection, external-fixture manifest comparison |
| Data ABI | `rust/nookery-data-ffi/**`, `tools/data-ffi/**`, minimal `CMakeLists.txt` wiring | Rust/C ABI harness, C++/Rust temp-root parity, minimal host CMake target, mutation failure |
| Famicom | `rust/nookery-famicom/**` | One complete source-grounded CPU/mapper/interrupt/APU/PPU slice, focused edge tests, mutation failure, explicit gaps |
| Save/GCI | `rust/nookery-save/**` | One complete source-grounded composite Save/GCI/ARAM slice, byte-level malformed/round-trip tests, mutation failure, explicit gaps |

## Parent-owned acceptance sequence

1. Reconcile each lane's exact changed-path set against this table and the pinned base.
2. Export each candidate with an isolated Git index; reject generated files, assets,
   nested locks, C/C++ edits, or scope drift.
3. Apply only the union of valid lane patches to a canonical successor.
4. Run fresh workspace `fmt`, `test`, `clippy`, `doc`, CMake/build, and every existing
   bswap/arena/data/settings/Yaz0 gate on the integrated candidate.
5. Independently replay every lane mutation claim; an un-applied or inert mutation is
   unproven and does not count.
6. Recompute Rust/C/C++ measurements and refresh the port ledgers without stale counts.
7. Commit and push only a fully gated parent candidate; effects remain
   `NOT_PERFORMED` beyond the authorized GitHub push.

A lane completion message, clean exit, or isolated green suite is not acceptance. A
bounded lane may close an item while the overall port remains `IN_PROGRESS`.
