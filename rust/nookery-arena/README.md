# nookery-arena

Rust port of Nookery's two bounded allocation primitives, at upstream pin
`a81c1aaf0d169a49d1044eb5bce9bbf699da64d9`:

| Upstream | Here |
|---|---|
| `include/gamealloc.h`, `src/gamealloc.c` | [`GameAlloc`] |
| `include/TwoHeadArena.h`, `src/TwoHeadArena.c` | [`TwoHeadArena`], [`Arena`] |
| `malloc` / `free` from `src/static/libc64/malloc.c` | [`BlockStore`], [`SlabStore`], [`BumpStore`] |

This crate is a **compatibility candidate**. It does not replace, wrap, or link
against the shipped C/C++ runtime; the parent integrates it by explicit
reconciliation. Nothing here has been benchmarked against a game workload, so
this crate makes no memory or CPU claim.

## Status

* `#![forbid(unsafe_code)]` — the port contains no `unsafe`. Addresses are
  integers and memory is reached only through checked slice indexing.
* `#![deny(missing_docs)]`.
* Standalone: it declares its own empty `[workspace]`, so the repository's
  `rust/Cargo.toml` is untouched.

## Ported behavior

`gamealloc_init`, `gamealloc_malloc`, `gamealloc_free`, `gamealloc_cleanup`,
`THA_ct`, `THA_dt`, `THA_alloc16`, `THA_allocAlign`, `THA_getFreeBytes`,
`THA_getFreeBytes16`, `THA_getFreeBytesAlign`, `THA_isCrash`, `THA_init`.

The six `/* @fabricated */` functions that are commented out in
`src/TwoHeadArena.c` (`THA_alloc`, `THA_getHeadPtr`, `THA_setHeadPtr`,
`THA_getTailPtr`, `THA_nextPtrN`, `THA_nextPtr1`) are **not** active upstream
behavior and are not ported.

Preserved details worth naming:

* Node headers are 16 bytes with `next`/`prev`/`alloc_size`/`pad` at offsets
  `0/4/8/12`, matching the header's `sizeof == 0x10` assertion. `pad` is never
  initialized, as upstream's `malloc` does not clear it.
* `gamealloc_malloc` stores `prev` and `alloc_size`, then writes
  `tail->next`, moves `tail`, writes the sentinel link, and finally sets
  `head.prev` — same order, same links.
* `gamealloc_free` recomputes `tail` from `head.prev` *after* unlinking, even
  when the freed node was not the tail.
* `NULL` free is a no-op; a failed allocation leaves the ring untouched.
* `gamealloc_cleanup` walks the ring forwards and re-seats it as an empty ring
  with `tail == head`.
* `THA_alloc16` masks the tail *before* subtracting the size, then masks again:
  `((tail & ~15) - siz) & ~15`.
* `THA_getFreeBytesAlign` aligns the head with `~mask` and the tail with `mask`,
  and truncates the difference to `int`.
* `THA_dt` zeroes every field, so a detached arena reports zero free bytes.

## Semantic boundaries

The port keeps upstream's arithmetic but refuses to produce an invalid state.

| Boundary | Upstream | This crate |
|---|---|---|
| 32-bit address width | `uintptr_t` is 4 bytes on console, 8 on the PC build | [`Addr`] is `u32`; identical results below 4 GiB |
| `size + sizeof(header)` overflow | wraps, hands out a short block | [`AllocError::SizeOverflow`] |
| Tail subtraction underflow | wraps to a high address | `try_*` returns [`ArenaError::Underflow`] |
| Allocation crossing the head | silently collides; `THA_isCrash` notices later | `try_*` returns [`ArenaError::Collision`] |
| `buf_p + size` overflow | wraps | [`ArenaError::AddressOverflow`] |
| Invalid mask | undefined alignment movement | [`ArenaError::InvalidMask`] |
| Foreign / stale / double free | writes through a stale address (UB) | [`AllocError::NotOwned`] |
| Pointer stability | live node address is stable and dereferenceable | offset of a live block never moves; **not** a host pointer, never dereferenced, must not cross into C |

Two things the crate deliberately does **not** claim:

* **Not a drop-in replacement for the C runtime.** `GameAlloc<S>` owns its
  store and an ownership set, so it is not layout-compatible with
  `struct gameAlloc_s` (`0x14` bytes) and cannot be handed to pointer-heavy
  decomp callers (`src/game.c`, `src/game/m_field_make.c`). Integration is a
  parent-level decision, not something this crate asserts.
* **No performance claim.** Nothing here is benchmarked against a game
  workload, and the `SlabStore`/`BumpStore` models are not `__osMalloc`.

The raw functions ([`TwoHeadArena::alloc16`], [`TwoHeadArena::alloc_align`],
[`TwoHeadArena::alloc16`]'s siblings, [`TwoHeadArena::ct`],
[`TwoHeadArena::get_free_bytes_align`]) reproduce upstream's wrapping exactly, so
differential tests can compare against C. The `try_*` forms are the checked API.

`THA_GA` (`src/THA_GA.c`) is a thin `Gfx*`-typed wrapper over the same
`TwoHeadArena` fields; it adds no arithmetic of its own, so it needs no separate
port. Its only distinct entry points (`THA_GA_ct`, `THA_GA_isCrash`,
`THA_GA_getFreeBytes`) are the ported `THA_ct`/`THA_isCrash`/`THA_getFreeBytes`.

## Verification

```sh
cargo fmt --manifest-path rust/nookery-arena/Cargo.toml -- --check
cargo test --manifest-path rust/nookery-arena/Cargo.toml
cargo clippy --manifest-path rust/nookery-arena/Cargo.toml --all-targets -- -D warnings
sh rust/nookery-arena/parity/run.sh   # regenerate the C transcript, then re-test
```

`tests/arena.rs` covers the listed behaviors with constants derived by hand from
the C source (not from this port's output), plus regressions for defects the
suite originally missed: the tail pre-mask in `try_alloc_align`, `SlabStore`
dropping sub-alignment split slack, `init` over a dirty ring, and `malloc`
leaving the node `pad` word uninitialized.

`tests/parity.rs` replays a transcript — including a 24-step fixed-seed
interleaving of sizes and masks — and requires byte-identical output with the
unmodified `src/TwoHeadArena.c` and `src/gamealloc.c` compiled at 32-bit width;
see `parity/README.md` for that harness and how to regenerate it. 

## License and attribution

MIT, following the repository's `LICENSE` and the upstream project
(https://github.com/flyngmt/Nookery). The ported logic is derived from the two
source files named above; `parity/` redistributes unmodified copies of those
files solely so the differential transcript stays reproducible.
