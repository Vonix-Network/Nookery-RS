//! The source-level sleep contract and its host implementation.
//!
//! `sleep.c` builds a one-message queue, arms a one-shot `OSTimer`, and blocks
//! receiving from that queue.  The operation description below keeps that
//! sequence testable without making production Rust call an FFI shim.  The
//! exported `csleep` uses the same one-shot duration contract with the host
//! scheduler.  `msleep` follows the PC platform helper's millisecond contract.

use std::ffi::c_void;
use std::thread;
use std::time::Duration;

/// Dolphin's signed 64-bit time type.
pub type OsTime = i64;

/// An opaque Dolphin thread object, referenced but not embedded by queues.
#[repr(C)]
pub struct OsThread {
    _private: [u8; 0],
}

/// A Dolphin thread queue, as embedded in `OSMessageQueue`.
#[repr(C)]
pub struct OsThreadQueue {
    /// First waiting thread.
    pub head: *mut OsThread,
    /// Last waiting thread.
    pub tail: *mut OsThread,
}

/// A `TARGET_PC`-layout Dolphin message queue.
#[repr(C)]
pub struct OsMessageQueue {
    /// Threads blocked sending to this queue.
    pub queue_send: OsThreadQueue,
    /// Threads blocked receiving from this queue.
    pub queue_receive: OsThreadQueue,
    /// Message storage supplied by the caller.
    pub msg_array: *mut *mut c_void,
    /// Number of message slots.
    pub msg_count: i32,
    /// Index of the first queued message.
    pub first_index: i32,
    /// Number of queued messages.
    pub used_count: i32,
}

/// The callback pointer stored at the head of Dolphin's `OSAlarm`.
pub type OsAlarmHandler = extern "C" fn(*mut OsAlarm, *mut c_void);

/// A `TARGET_PC`-layout Dolphin alarm.
#[repr(C)]
pub struct OsAlarm {
    /// Alarm callback.
    pub handler: Option<OsAlarmHandler>,
    /// Absolute fire time.
    pub fire: OsTime,
    /// Previous alarm in the queue.
    pub prev: *mut OsAlarm,
    /// Next alarm in the queue.
    pub next: *mut OsAlarm,
    /// Period for a periodic alarm.
    pub period: OsTime,
    /// Start time for a periodic alarm.
    pub start: OsTime,
}

/// A `TARGET_PC`-layout libultra timer.
#[repr(C)]
pub struct OsTimer {
    /// Embedded Dolphin alarm.
    pub alarm: OsAlarm,
    /// Previous timer in libultra's timer list.
    pub next: *mut OsTimer,
    /// Next timer in libultra's timer list.
    pub prev: *mut OsTimer,
    /// Repeat interval.
    pub interval: OsTime,
    /// One-shot countdown.
    pub value: OsTime,
    /// Destination message queue.
    pub mq: *mut OsMessageQueue,
    /// Message delivered when the timer fires.
    pub msg: *mut c_void,
}

/// The default GameCube bus clock used by the host-compatible `msleep` entry.
pub const DEFAULT_BUS_CLOCK_HZ: u32 = 162_000_000;

/// The default GameCube timer clock (`OS_BUS_CLOCK / 4`).
pub const DEFAULT_TIMER_CLOCK_HZ: u32 = DEFAULT_BUS_CLOCK_HZ / 4;

/// Compute Dolphin's integer timer clock from a bus clock.
pub const fn os_timer_clock(bus_clock_hz: u32) -> u32 {
    bus_clock_hz / 4
}

/// Compute `OSMillisecondsToTicks((u64)milliseconds)` with C unsigned wrapping.
pub const fn os_milliseconds_to_ticks(milliseconds: u64, bus_clock_hz: u32) -> OsTime {
    let ticks_per_millisecond = (os_timer_clock(bus_clock_hz) / 1000) as u64;
    milliseconds.wrapping_mul(ticks_per_millisecond) as OsTime
}

/// One observable operation made by `csleep` in the C source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SleepOperation {
    /// `osCreateMesgQueue(&mq, &msg, 1)`.
    CreateMessageQueue {
        /// Number of queue slots.
        msg_count: i32,
        /// Whether the source supplied a message buffer.
        message_buffered: bool,
    },
    /// `osSetTimer(&timer, countdown, 0, &mq, NULL)`.
    SetTimer {
        /// One-shot countdown.
        countdown: OsTime,
        /// Repetition interval, always zero for this source slice.
        interval: OsTime,
        /// The source passes a null timer message.
        message_is_null: bool,
    },
    /// `osRecvMesg(&mq, NULL, 1)`.
    ReceiveMessage {
        /// Receive flags.
        flags: i32,
        /// Whether the source discarded the received message.
        output_is_null: bool,
    },
}

/// Return the exact queue/timer/receive sequence emitted by `sleep.c`.
pub fn c_sleep_operations(countdown: OsTime) -> [SleepOperation; 3] {
    [
        SleepOperation::CreateMessageQueue {
            msg_count: 1,
            message_buffered: true,
        },
        SleepOperation::SetTimer {
            countdown,
            interval: 0,
            message_is_null: true,
        },
        SleepOperation::ReceiveMessage {
            flags: 1,
            output_is_null: true,
        },
    ]
}

/// Sleep for a positive number of timer ticks at `timer_clock_hz`.
pub fn sleep_ticks(countdown: OsTime, timer_clock_hz: u64) {
    if countdown <= 0 || timer_clock_hz == 0 {
        return;
    }

    let nanos =
        (u128::try_from(countdown).unwrap_or(0) * 1_000_000_000) / u128::from(timer_clock_hz);
    let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
    thread::sleep(Duration::from_nanos(nanos));
}

/// Source-compatible one-shot sleep entry point.
#[unsafe(no_mangle)]
pub extern "C" fn csleep(countdown: OsTime) {
    sleep_ticks(countdown, u64::from(DEFAULT_TIMER_CLOCK_HZ));
}

/// PC platform millisecond sleep entry point.
#[unsafe(no_mangle)]
pub extern "C" fn msleep(milliseconds: u32) {
    thread::sleep(Duration::from_millis(u64::from(milliseconds)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_conversion_keeps_the_source_integer_order() {
        assert_eq!(os_timer_clock(DEFAULT_BUS_CLOCK_HZ), 40_500_000);
        assert_eq!(os_milliseconds_to_ticks(1, DEFAULT_BUS_CLOCK_HZ), 40_500);
        assert_eq!(
            os_milliseconds_to_ticks(u64::MAX, DEFAULT_BUS_CLOCK_HZ),
            u64::MAX.wrapping_mul(40_500) as i64
        );
    }

    #[test]
    fn c_sleep_operation_order_is_source_order() {
        assert_eq!(
            c_sleep_operations(7),
            [
                SleepOperation::CreateMessageQueue {
                    msg_count: 1,
                    message_buffered: true,
                },
                SleepOperation::SetTimer {
                    countdown: 7,
                    interval: 0,
                    message_is_null: true,
                },
                SleepOperation::ReceiveMessage {
                    flags: 1,
                    output_is_null: true,
                },
            ]
        );
    }
}
