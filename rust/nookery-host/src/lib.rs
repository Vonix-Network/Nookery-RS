//! Portable behavior from the PC host adaptation layer.
//!
//! The modules here are deliberately limited to decisions that do not require
//! Aurora, SDL, Dolphin, RmlUi, or the decompiled game's C object layouts.
//! They mirror the corresponding production sources exactly enough to be
//! tested without a graphics or windowing backend:
//!
//! * [`game_text`] is the Animal Crossing font codec from
//!   `pc/src/acgc_game_text.cpp`.
//! * [`touch_layout`] is the placement model, JSON format, and geometry from
//!   `pc/src/ui/touch_layout.cpp`.
//! * [`widescreen`] contains the aspect, projection, and viewport decisions
//!   from `pc/src/aurora_widescreen.cpp`; the final GX calls remain host glue.
//! * [`clock`] contains the RTC/override selection from `pc/src/pc_clock.c`;
//!   obtaining the two host clock fields remains host glue.
//!
//! This is a candidate crate. The existing CMake target still compiles the
//! original files, so no duplicate symbols are introduced into the production
//! executable.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod clock;
pub mod game_text;
pub mod touch_layout;
pub mod widescreen;
