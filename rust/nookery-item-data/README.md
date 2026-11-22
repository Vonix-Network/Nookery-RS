# nookery-item-data

This crate ports the bounded item-price family from the actual C oracle:

- `src/data/item/md_price.c`
- `src/data/item/plant_price.c`
- `src/data/item/insect_price.c`
- `src/data/item/fish_price.c`
- `src/data/item/food_price.c`

`tools/generate_item_prices.py` parses those exact declarations and emits
`src/generated.rs` plus `SOURCE_MANIFEST.toml`. The generated `u16` slices keep
every value and duplicate in source order; each C `-1` sentinel is represented
as `65535` (`0xFFFF`). No C wrapping or production FFI is used.

The remaining item-price sources are deliberately outside this bounded slice:
`binsen_price.c`, `carpet_price.c`, `cloth_price.c`, `diary_price.c`,
`ftr_price.c`, `tool_price.c`, and `wall_price.c`.

From the repository root:

```text
python3 tools/generate_item_prices.py --check
git diff --check
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo test --manifest-path rust/Cargo.toml -p nookery-item-data
cargo clippy --manifest-path rust/Cargo.toml -p nookery-item-data --all-targets -- -D warnings
cargo doc --manifest-path rust/Cargo.toml -p nookery-item-data --no-deps --document-private-items
python3 tools/test_item_prices_mutation.py
```
