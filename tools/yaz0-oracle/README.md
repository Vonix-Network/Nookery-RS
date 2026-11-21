# Yaz0 differential harness

Compares the Rust Yaz0 decoder (`rust/nookery-disc/src/yaz0.rs`) against the C
implementation it must match, on real game data.

## Why

`pc/src/pc_assets.c:47-94` (`aurora_yaz0_decode`) unpacks `/foresta.rel.szs`
before the REL is mounted. A port that only "looks right" is worthless here: a
single wrong back-reference distance silently corrupts the REL and every
subsequent game behavior. So the port is checked byte-for-byte against the C
code on an actual disc image.

## The oracle

`yaz0_oracle.c` contains `aurora_yaz0_decode` **copied verbatim** from
`pc/src/pc_assets.c`, plus `pc_load_be32` from `pc/include/pc_bswap.h`. It is
the reference, not a paraphrase. The only addition is a failure-reason code so
Rust can be compared rejection-for-rejection, not just success-for-success.

Genuine upstream quirks it preserves deliberately:

- a source shorter than 16 bytes is rejected *before* the magic is examined;
- a back-reference whose distance reaches before the output start is a hard
  failure, not a clamp;
- running out of source mid-stream ends decoding, and a short result is
  rejected rather than returned partially.

## Build and run

```sh
make -C tools/yaz0-oracle all

# Compare one or more user-supplied .szs files.  Game data is never committed;
# supply paths at run time.
NOOKERY_YAZ0_FIXTURES=/path/to/foresta.rel.szs \
  cargo test --manifest-path rust/Cargo.toml -p nookery-disc --test yaz0_differential -- --nocapture
```

Both tests skip loudly when the oracle binary or the fixtures are absent, so a
bare `cargo test --workspace` stays green without game data while never
pretending the comparison ran.

## Observed result

On the USA/Canada disc (`GAFE01`) `files/foresta.rel.szs`:

```text
input      6,137,393 bytes
decoded   15,640,056 bytes
fnv1a64   322cd015b52e4005   (identical from C and Rust)
```

## Extracting a disc image

A CISO is compressed, so tooling is needed rather than hand-parsing the
container header. `wit` (Wiimms ISO Tool) reads it and can produce a raw image:

```sh
wit DUMP  game.ciso          # confirms CISO/GC, disc id, size
wit FILES game.ciso          # lists archive members
wit COPY  game.ciso game.iso # reconstruct the full image
wit EXTRACT game.iso -d out/ # extract files for use as fixtures
```

The reconstruction is the full virtual size (~4.5 GiB for `GAFE01`), which is
much larger than the compressed CISO.

## Next targets

`RARC` archive parsing (`forest_1st.arc`, `famicom.arc`) is the natural
successor: the port already carries the `JKRArchive*` oracles, and the same
verbatim-extraction plus digest-comparison pattern applies.
