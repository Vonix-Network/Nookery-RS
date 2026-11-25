//! Source-grounded Rust equivalents of the small `libc64` math and sleep slice.
//!
//! The math entry points follow `src/static/libc64/math64.c` and preserve its
//! PC-side C promotion rules.  The sleep types mirror the concrete
//! `TARGET_PC` layouts used by `src/static/libc64/sleep.c`; sleeping itself is
//! implemented with the Rust host scheduler and does not call back into C.

#![deny(missing_docs)]

pub mod allocator;
pub mod format;
pub mod math64;
pub mod sleep;

pub use allocator::{
    AllocatorStatus, Arena, BlockSnapshot, OS_MALLOC_MAGIC, OSArena_FLAG_CLEAR_MEM_ON_ALLOC,
    OSArena_FLAG_CLEAR_MEM_ON_FREE, OSArena_FLAG_FREE_BLOCK_TEST, OsMemBlock,
};
pub use format::{AproutFunc, FormatArg, FormatError, format_string, sprintf, vaprintf};
pub use math64::{facos, fatan2, fsqrt, libc64_fsqrt};
pub use sleep::{
    OsAlarm, OsAlarmHandler, OsMessageQueue, OsThreadQueue, OsTime, OsTimer, SleepOperation,
    c_sleep_operations, csleep, msleep, os_milliseconds_to_ticks, os_timer_clock, sleep_ticks,
};
