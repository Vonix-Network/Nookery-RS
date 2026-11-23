# nookery-scene-data

Rust port of the scene-word tables in `src/data/scene/*.c` — 51 files, all
fully-specified literal initializers with no function definitions and no
`#include "assets/*.inc"` payload includes, so the C text in the tree is the
complete oracle.

Each C file becomes one module in `src/generated/scene_words.rs` exposing the
same symbols in the same order.

## What parity means here

`tests/parity.rs` is a **second, independent reader** of the C sources. It does
not call the generator, does not share its parser, and re-derives every expected
value from the raw `.c` bytes plus the repository headers (`include/m_actor.h`
for the actor-profile and object-bank enums). It then compares those values
element by element against the integers in the generated Rust.

A tampered generated value fails the gate. This was verified by planting one,
watching the gate fail, restoring the file, and watching it pass again — twice,
including once on the final formatted tree.

## The pointer caveat, stated plainly

A relocatable Rust `const` cannot hold a host address. Scene words that carry a
pointer (`mSc_DATA_PLAYER`, `mSc_DATA_CTRL_ACTORS`, `mSc_DATA_ACTORS`,
`mSc_DATA_OBJ_BANK`, `mSc_DATA_DOOR_DATA`) therefore record the **target symbol
name** in `SceneWord::ptr` and store no address.

The four scalar words and the packed field-construct word are exact. Pointer
identity is compared by symbol name, not by value. `SOURCE_MANIFEST.toml`
records `pointer_parity = "symbol-identity"` on every table so this limitation
cannot be mistaken for full byte parity.

## Not ported, recorded rather than faked

`Door_data_c` initializers in this tree use a layout that disagrees with the
single `Door_data_c` definition in `include/m_scene.h` (for example `buggy.c`
passes `SCENE_FG` where the header declares `int next_scene_id`). That mismatch
needs a per-version header audit, so door tables are **not** emitted. They are
absent, not emulated.

## Regeneration

```text
python3 tools/generate_scene_data.py           # rewrite
python3 tools/generate_scene_data.py --check   # fail when stale
```

The generator aborts with file and line on any construct it does not
understand. It never guesses and never silently skips a file.
