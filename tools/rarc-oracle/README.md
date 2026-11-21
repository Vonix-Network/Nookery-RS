# RARC differential helper

This directory contains a standalone Rust-only manifest extractor. It has no
dependency on the `nookery-disc` crate, Python, C, or game data. The helper
accepts a raw big-endian RARC path and prints one `FILE` record per reachable
file with its original path, name, byte size, and FNV-1a64 digest.

Build it with:

```sh
make -C tools/rarc-oracle
```

The runtime differential test uses `NOOKERY_RARC_FIXTURES` as a platform path
list. Fixture files remain outside Git:

```sh
NOOKERY_RARC_FIXTURES=/path/to/forest_1st.arc:/path/to/forest_2nd.arc \
  CARGO_TARGET_DIR=/tmp/nookery-rs-wave4-target \
  cargo test -p nookery-disc --test rarc_differential -- --nocapture
```

Yaz0 wrapping is intentionally explicit in the Rust API. The helper compares
raw RARC fixtures; a Yaz0-wrapped fixture must be unwrapped before invoking
the helper and parsed with `RarcArchive::parse_yaz0`.
