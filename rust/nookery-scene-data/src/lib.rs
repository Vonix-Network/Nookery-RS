//! Source-grounded scene and furniture data tables.
//!
//! The furniture module is a data-only candidate. Pointer fields preserve C
//! target identity as symbol names because Rust constants cannot contain the
//! original relocatable host addresses.

pub mod furniture_part3;

pub use furniture_part3::{FURNITURE_PROFILES, FurnitureProfile, FurnitureSource, SOURCE_MANIFEST};
