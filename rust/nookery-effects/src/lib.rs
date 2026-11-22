//! Source-grounded Rust slice for Nookery's `ASE` effect pair.
//!
//! This crate ports only `src/effect/ef_ase.c` and `src/effect/ef_ase_ch.c`,
//! plus the controller operations those files use from
//! `src/effect/ef_effect_control.c`. The C sources remain the oracle; no other
//! effect profile is represented here.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

mod ase;

pub use ase::{
    AudioEvent, DrawState, EffectController, EffectHandle, EffectId, EffectSnapshot, EffectState,
    EffectVec3, SpawnInput, SpawnOutcome, UpdateError, ACTIVE_EFFECT_MAX, EFFECT_ASE,
    EFFECT_ASE_CH, EFFECT_NUM,
};
