# nookery-data-assets

This crate is the pure-Rust replacement candidate for the complete
`src/data/titledemo/` subdirectory. It contains the five title-demo recordings
(`pact0` through `pact4`) as checked-in Rust `u16` arrays.

`tools/titledemo/generate_rust.py` reads the real C declarations and emits
`src/generated.rs` plus `source-manifest.txt`. The manifest records raw source
SHA-256 hashes, canonical big-endian data hashes, and word counts. Run:

```text
python3 tools/titledemo/generate_rust.py --check
```

The crate is not wired into the C/C++ production target. The unchanged C
objects remain the production implementation and oracle until a separately
authorized integration gate proves the caller and linker transition.

The source is from ACreTeam/ac-decomp and is covered by the repository's CC0
decompilation attribution. New Rust glue remains under the repository's MIT
terms; see the root `LICENSE`.
