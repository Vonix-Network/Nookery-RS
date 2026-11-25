# nookery-scene-data

This standalone crate contains source-grounded data tables from the C scene
and furniture sources. Lane 05 adds the final alphabetical third of the
portable `src/furniture/*.c` files.

The generated furniture profiles preserve every `aFTR_PROFILE` initializer
field in source order. Pointer fields use the exact C symbol name, which keeps
identity and ordering without introducing an unsafe host pointer. The custom
callback implementations in files with `aFTR_vtable_c` remain in C; those
files are listed in the lane result rather than represented by a signature-only
placeholder.

Regenerate or validate the lane slice with:

```text
python3 tools/generate_furniture_data.py
python3 tools/generate_furniture_data.py --check
cargo fmt --manifest-path rust/nookery-scene-data/Cargo.toml -- --check
cargo test --manifest-path rust/nookery-scene-data/Cargo.toml --all-targets
cargo clippy --manifest-path rust/nookery-scene-data/Cargo.toml --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path rust/nookery-scene-data/Cargo.toml --no-deps
```
