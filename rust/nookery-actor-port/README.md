# nookery-actor-port

This candidate crate ports the portable behavior seams from the first
alphabetical half of `src/actor/`:

* `ac_airplane.c`: frame-rate rubber movement, fly selection, ground friction,
  dust timer mutation, wall transition, and background-check offset;
* `ac_insect_move.c_inc`: shared patience, lifetime expiry, and alpha fade
  mutation from the insect actor family;
* `ac_balloon.c`: re-export of the existing source-grounded Rust lifecycle
  implementation;
* the empty lifecycle units in `ac_dummy.c`, `ac_boxManager.c`,
  `ac_boxMove.c`, and `ac_boxTrick01.c`: profile metadata and explicit no-op
  lifecycle calls.

`parity/oracle.c` is a test-only C driver. It is compiled with
`rust/nookery-libc64/parity/include` before all other includes so `u32` and
related source types retain their 32-bit widths on the host.
