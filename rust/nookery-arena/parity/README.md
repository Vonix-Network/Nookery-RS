# Parity harness

`expected.txt` is produced by the **unmodified** upstream sources, not by this
crate:

| File | Origin |
|---|---|
| `TwoHeadArena.c` | `src/TwoHeadArena.c` at pin `a81c1aaf0d169a49d1044eb5bce9bbf699da64d9` |
| `gamealloc.c` | `src/gamealloc.c` at the same pin |
| `include/` | Minimal stand-ins for `types.h`, `libultra/libultra.h`, `libc64/malloc.h` and the two public headers, reduced to the declarations these two `.c` files need |
| `driver.c` | Harness: drives both upstream implementations and prints one line per observable step |

## What this proves, and what it does not

This is a **fixture/oracle**, not proof that the Rust crate is a drop-in
replacement. It proves that, for the transcript below, the port's address
arithmetic and list bookkeeping agree byte-for-byte with the real
`src/TwoHeadArena.c` and `src/gamealloc.c`. It does **not** cover:

* `THA_GA` (`src/THA_GA.c`) or `gfxalloc.c` — neither is compiled here.
* The real runtime allocator: the harness replaces `malloc`/`free` with a
  never-reusing bump allocator, while the game links `__osMalloc` from
  `src/static/libc64/malloc.c` (block headers, magic values, an arena lock,
  address reuse). The port models both policies (`SlabStore`, `BumpStore`), but
  no transcript here came from `__osMalloc`.
* Pointer-heavy decomp callers (`src/game.c`, `src/game/m_field_make.c`) that
  hold the returned `void*` across allocations. The crate exposes stable
  *offsets*, not C-dereferenceable pointers.

The transcript includes a 24-step fixed-seed interleaving of sizes and all four
mask shapes used in-repo (`-1`, `-16`, `~0x1F`, `-2`), so the arithmetic path is
compared over operand shapes the hand-written cases do not enumerate — including
past the head, where both sides wrap and `THA_isCrash` reports 1.

## Why 32-bit freestanding

`include/gamealloc.h` asserts `sizeof(struct gameAllocList_s) == 0x10` and
`sizeof(struct gameAlloc_s) == 0x14`, and `include/game.h` places `tha` at
`0x78` and `gamealloc` at `0x88`.  Those numbers only hold when pointers and
`size_t` are four bytes, i.e. the shipped console targets.  Building the harness
`-m32` runs the real source at the width the assertions describe.

The functions under test do no I/O and need no allocator beyond `malloc`/`free`,
so the harness is freestanding (`-nostdlib`) with syscall-only printing.  That
avoids needing a 32-bit glibc, which this container does not have.  `bzero`,
`memset` and `malloc`/`free` are provided by the harness:

* `malloc` is a bump allocator that never reuses a released block, so offsets are
  reproducible and identical to `BumpStore` in `src/storage.rs`.
* `free` is a no-op; the port's `BumpStore` models that by not rewinding its
  cursor.

## Regenerate

```sh
sh rust/nookery-arena/parity/run.sh
cargo test --manifest-path rust/nookery-arena/Cargo.toml --test parity
```

`run.sh` refuses to run if its local copies of the two `.c` files differ from the
worktree's `src/` copies, so the transcript cannot silently go stale.
