# nookery-field-data

This crate is the portable Rust data plane for the field translation units
under `src/data/field/` that contain no generated `assets/*.inc` payload.
Animation, move-actor, field, and background records are generated from the
checked-in C initializers into typed Rust statics. `SOURCE_MANIFEST.toml`
records the source hash and conversion kind for audit and parity tooling.

Relocatable C pointers are represented by their target symbol names, matching
the scene-data crate's symbol-identity rule. The `TARGET_PC` raw-actable endian
mutation is ported as `swap_actable_once`/`init_actable_endian`; the six input
tables remain disc-derived and are not fabricated here.

The 274 excluded translation units are disc-derived payload sources and are
reported as `GENERATED_FROM_DISC`; they are intentionally not copied into the
crate.

Regenerate the tables with `python3 tools/generate.py` from this crate's
directory.
