//! Host clock selection from `pc/src/pc_clock.c`.

/// Inputs available after the host has produced its calendar minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockInput {
    /// Whether the game's RTC is enabled.
    pub rtc_enabled: bool,
    /// Whether the game's RTC has entered its crashed state.
    pub rtc_crashed: bool,
    /// RTC hour, used only when the game RTC is healthy.
    pub rtc_hour: i32,
    /// RTC minute, used only when the game RTC is healthy.
    pub rtc_minute: i32,
    /// Host calendar hour.
    pub host_hour: i32,
    /// Host calendar minute.
    pub host_minute: i32,
    /// PC hour override; negative means unset.
    pub hour_override: i32,
    /// PC minute override; negative means unset.
    pub minute_override: i32,
}

/// Select the game's clock minute, preserving the C branch order.
///
/// A healthy game RTC wins over both PC overrides. Otherwise each host field
/// is independently replaced when its corresponding override is non-negative.
#[must_use]
pub fn minutes(input: ClockInput) -> i32 {
    if input.rtc_enabled && !input.rtc_crashed {
        return input.rtc_hour * 60 + input.rtc_minute;
    }
    let hour = if input.hour_override >= 0 {
        input.hour_override
    } else {
        input.host_hour
    };
    let minute = if input.minute_override >= 0 {
        input.minute_override
    } else {
        input.host_minute
    };
    hour * 60 + minute
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ClockInput {
        ClockInput {
            rtc_enabled: false,
            rtc_crashed: false,
            rtc_hour: 1,
            rtc_minute: 2,
            host_hour: 10,
            host_minute: 20,
            hour_override: -1,
            minute_override: -1,
        }
    }

    #[test]
    fn healthy_rtc_has_priority_over_overrides() {
        let mut value = input();
        value.rtc_enabled = true;
        value.hour_override = 23;
        value.minute_override = 59;
        assert_eq!(minutes(value), 62);
    }

    #[test]
    fn crashed_rtc_uses_independent_overrides() {
        let mut value = input();
        value.rtc_enabled = true;
        value.rtc_crashed = true;
        value.hour_override = 4;
        assert_eq!(minutes(value), 4 * 60 + 20);
    }
}
