# nookery-actor

This crate ports the player-owned balloon actor from the C oracle:

- `src/actor/ac_balloon.c:8-143` — hide/fly dispatch, request semantics,
  asset selection, animation setup, upward movement, and the 200-unit hide
  request;
- `src/actor/ac_balloon.c:149-223` — joint render callbacks and hidden draw
  suppression;
- `include/ac_balloon.h:29-47` — `BALLOON_ACTOR` field order and allocation;
- `include/m_actor.h:790-1021,1052-1066,1104-1152` — profile identifiers,
  flags, and shared actor byte span;
- `src/game/m_actor.c:180-237,496-570,768-850` — engine constructor,
  update/draw ordering, and actor allocation;
- `src/game/m_player.c:513-526` — one hidden balloon actor is created for the
  player;
- `src/game/m_player_main_release_creature.c_inc:55-88` and
  `src/game/m_player_main_tumble_getup.c_inc:10-35` — the two live flight
  request callers.

The Rust `BalloonActorLayout` is `#[repr(C)]`, 0x478 bytes, and keeps the
shared 0x174-byte actor area plus the opaque keyframe/matrix allocations and
all named balloon fields. Unknown bytes are retained rather than replaced by a
smaller model. Profile ID 230, `ITM_BALLOON_START` 0x2244, BG part 3, keep
object bank 3, flags 0x30, eight balloon types, and data IDs 23/31/32 are
source-preserved constants.

Deletion remains source-defined: `Actor_dt` calls the save callback first
(this profile has none), then `Ac_Balloon_dt` (empty), then detaches parent and
child links and decrements the object-bank count (`src/game/m_actor.c:237-270`).
The Rust candidate does not invent teardown callbacks or claim ownership of
that engine-level list/accounting sequence.

## Activation status

The C actor is still active and remains the oracle. `configure.py:991` still
builds `src/actor/ac_balloon.c`; the Rust workspace is not referenced by
CMake or the shipped application. No C/C++/CMake/source assets were changed.
The candidate is pure Rust and exposes a callback trait for player position,
keyframe/animation events, and the three-joint before/after render hooks; it
does not wrap C.

Run the focused mutation gate with:

```sh
rust/nookery-actor/tools/mutation-test.sh
```

It changes the two-entry dispatch bound to one in a temporary source copy and
requires the source-located lifecycle tests to fail.
