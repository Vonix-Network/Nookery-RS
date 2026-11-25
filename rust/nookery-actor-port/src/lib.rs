//! Portable behavior from `src/actor`, covering the sources that do not require
//! disc-generated assets or engine-owned global state.
//!
//! The original actor files are engine translation units: most reach into
//! global clips, display-list builders, save data, or generated disc assets.
//! This crate exposes the behaviour-bearing seams that stand on their own. The
//! C sources remain the active implementation and the oracle; this crate is a
//! candidate replacement for the documented seams.
//!
//! Source grounding:
//!
//! * [`airplane`] follows `src/actor/ac_airplane.c` and `include/ac_airplane.h`.
//! * [`insect`] follows `src/actor/ac_insect_move.c_inc` and
//!   `include/ac_insect_h.h`.
//! * [`profiles`] holds the actor profile tables.
//! * [`balloon`] re-exports the independently source-grounded port of
//!   `src/actor/ac_balloon.c`.
//! * [`weather`] ports `ac_weather_fine.c`, `ac_weather_rain.c`,
//!   `ac_weather_snow.c`, `ac_weather_sakura.c` and `ac_weather_leaf.c`.
//!   Rendering and actor-list ownership stay at the game boundary; particle
//!   allocation, movement, scrolling and profile selection are ordinary Rust
//!   state.
//! * [`tunahiki`] ports `src/actor/ac_tunahiki.c`.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod airplane;
pub mod insect;
pub mod profiles;
pub mod tunahiki;
pub mod weather;

/// The full player-balloon behavior port, shared with the earlier standalone
/// actor candidate rather than duplicated in this crate.
pub mod balloon {
    pub use nookery_actor::*;
}
