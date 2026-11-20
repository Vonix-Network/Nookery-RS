//! Behavior-preserving Rust primitives for the Nookery port.
//!
//! This crate is intentionally dependency-free at the bootstrap stage.  The
//! first slice mirrors the data-folder contract in `pc/src/acgc_data.cpp` and
//! `pc/include/acgc/data.hpp`, keeping policy (layout, migration candidates,
//! status text) separate from platform I/O so later platform ports can share
//! the same allocation-light path logic.
//!
//! Scope boundary: `extern/borealis` is an unpopulated submodule at this pin, so
//! the Borealis-owned parts of the contract are modeled from their call sites
//! rather than reimplemented:
//!
//! | Upstream | Here |
//! |---|---|
//! | `Manager::initialize` platform discovery | caller supplies the root ([`DataLayout`]) |
//! | `set_custom_data_path` / `reset_data_path` restart flow | not ported (runtime state) |
//! | `open_active_data_path` (window system) | not ported ([`ErrorCode::Unsupported`] exists) |
//! | `base_path_relative` executable discovery | [`BasePath::Unavailable`] |
//! | migration copying | [`migration::plan`] only; copying is Borealis' job |
//!
//! Everything below the boundary is real: directories are created on disk and
//! every failure is reported as a [`Status`].

pub mod base;
pub mod init;
pub mod layout;
pub mod migration;
pub mod status;

#[cfg(test)]
mod testing;

pub use base::BasePath;
pub use init::{Initialized, ensure_directory, initialize};
pub use layout::{DataLayout, PathError};
pub use migration::{MigrationPlan, MigrationRules, plan as plan_migration};
pub use status::{ErrorCode, Status};
