# nookery-actor-npc

This crate ports one coherent ordinary-NPC family: action-request arbitration,
action dispatch, and the normal-NPC lifecycle forwarding adapter.

The primary sources are:

- `src/actor/npc/ac_npc_action.c_inc:445-498` — request threshold, state
  transition, dispatch order, request clearing, and end-step reset.
- `src/actor/npc/ac_normal_npc.c:28-64` — normal-NPC birth gate and lifecycle
  forwarding.
- `include/ac_npc.h:693-722` — `aNPC_action_c` and `aNPC_request_c` layouts.
- `src/actor/npc/ac_npc.c:113-130` — the control actor's required NPC behavior
  include family.

`oracle/npc_action_oracle.c` is a test-only executable oracle. Its three
behavior functions are verbatim from the source block above; the differential
test checks that before compiling it. Run it with:

```text
cargo test -p nookery-actor-npc
```

The mutation guard in the unit tests changes the source's inclusive `>=`
priority admission to strict `>` and proves the equal-priority boundary would
be detected.

This is not a complete NPC port. `NPC_CONTROL_ACTOR` remains at
`src/actor/npc/ac_npc.c:52-61`, and the full `NPC_ACTOR` remains at
`include/ac_npc.h:971-1002`; both depend on actor, animation, movement,
schedule, dialogue, cloth, collision, save, and global NPC-clip systems. This
crate uses target-width integer handles and ports only the exact records and
dispatch behavior required by this family; it adds no production FFI or C
wrapper.
