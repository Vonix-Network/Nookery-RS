//! Source-grounded Rust model for Nookery's effect controller and profiles.
//!
//! The C sources remain the oracle. The controller keeps the fixed pool,
//! dispatch table, profile lifecycle policies, smooth-motion pass, and the
//! source-defined initialization values in one portable Rust implementation.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

mod ase;

pub use ase::*;
