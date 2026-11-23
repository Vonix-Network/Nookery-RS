//! Rust model of the `src/data/scene/*.c` scene-word tables.
//!
//! The 51 C files in `src/data/scene` are fully-specified literal initializers
//! with no payload includes and no function definitions, so the C text in the
//! tree is the complete oracle. [`generated`] holds one module per source file,
//! produced by `tools/generate_scene_data.py`.
//!
//! ## Pointer caveat
//!
//! A relocatable Rust `const` cannot hold a host address. A scene word that
//! carries a pointer therefore records the *target symbol name* in
//! [`SceneWord::ptr`] and stores no address. The four scalar words and the
//! packed field-construct word are exact; the pointer is compared by symbol
//! identity rather than by value. `SOURCE_MANIFEST.toml` records
//! `pointer_parity = "symbol-identity"` on every table for the same reason.
//!
//! ## Regeneration
//!
//! ```text
//! python3 tools/generate_scene_data.py           # rewrite
//! python3 tools/generate_scene_data.py --check   # fail when stale
//! ```

pub mod generated;

pub use generated::{ActorData, SceneWord, SceneWordKind};
