//! Controller slice ported from `src/game/m_controller.c`.
//!
//! The public state intentionally follows `MCON` in `include/m_controller.h`
//! field-for-field.  The C implementation remains the oracle; this module is
//! not an alternate input model.

use std::f32::consts::PI;

pub const STICK_MIN: f32 = 9.899_495;
pub const STICK_MAX: f32 = 61.0;
pub const STICK_UNCORRECTED_SCALE: f32 = 1.0 / STICK_MAX;
pub const STICK_CORRECTED_SCALE: f32 = 1.0 / (STICK_MAX - STICK_MIN);

/// Rust layout of the C `MCON` object (`sizeof(MCON) == 0x38`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControllerState {
    pub move_px: f32,
    pub move_py: f32,
    pub move_pr: f32,
    pub move_angle: i16,
    pub last_move_px: f32,
    pub last_move_py: f32,
    pub last_move_pr: f32,
    pub last_move_angle: i16,
    pub adjusted_px: f32,
    pub adjusted_py: f32,
    pub adjusted_pr: f32,
    pub last_adjusted_px: f32,
    pub last_adjusted_py: f32,
    pub last_adjusted_pr: f32,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
            move_px: 0.0,
            move_py: 0.0,
            move_pr: 0.0,
            move_angle: 0,
            last_move_px: 0.0,
            last_move_py: 0.0,
            last_move_pr: 0.0,
            last_move_angle: 0,
            adjusted_px: 0.0,
            adjusted_py: 0.0,
            adjusted_pr: 0.0,
            last_adjusted_px: 0.0,
            last_adjusted_py: 0.0,
            last_adjusted_pr: 0.0,
        }
    }
}

impl ControllerState {
    /// Port of empty C `mCon_ct`.
    pub fn mcon_ct() {}

    /// Port of empty C `mCon_dt`.
    pub fn mcon_dt() {}

    /// Port of C `mCon_calc`: snapshot the previous values, then process one
    /// joystick sample in the same dead-zone/clamp/scale order.
    pub fn mcon_calc(&mut self, stick_x: f32, stick_y: f32) {
        self.last_move_px = self.move_px;
        self.last_move_py = self.move_py;
        self.last_move_pr = self.move_pr;
        self.last_move_angle = self.move_angle;

        self.last_adjusted_px = self.adjusted_px;
        self.last_adjusted_py = self.adjusted_py;
        self.last_adjusted_pr = self.adjusted_pr;

        let mut x = stick_x;
        let mut y = stick_y;
        let mut magnitude = (x * x + y * y).sqrt();

        if magnitude <= STICK_MIN {
            self.move_px = 0.0;
            self.move_py = 0.0;
            self.move_pr = 0.0;
            self.adjusted_px = 0.0;
            self.adjusted_py = 0.0;
            self.adjusted_pr = 0.0;
        } else {
            let stick_angle = atan_short(x, y);
            if magnitude > STICK_MAX {
                x = cos_short(stick_angle) * STICK_MAX;
                y = sin_short(stick_angle) * STICK_MAX;
                magnitude = STICK_MAX;
            }

            self.move_angle = stick_angle;
            self.move_px =
                check_percent_abs(x, STICK_MIN, STICK_MAX, STICK_UNCORRECTED_SCALE, false);
            self.move_py =
                check_percent_abs(y, STICK_MIN, STICK_MAX, STICK_UNCORRECTED_SCALE, false);
            self.move_pr = check_percent_abs(
                magnitude,
                STICK_MIN,
                STICK_MAX,
                STICK_UNCORRECTED_SCALE,
                false,
            );
            self.adjusted_px =
                check_percent_abs(x, STICK_MIN, STICK_MAX, STICK_CORRECTED_SCALE, true);
            self.adjusted_py =
                check_percent_abs(y, STICK_MIN, STICK_MAX, STICK_CORRECTED_SCALE, true);
            self.adjusted_pr =
                check_percent_abs(magnitude, STICK_MIN, STICK_MAX, STICK_CORRECTED_SCALE, true);
        }
    }
}

/// Input values consumed by the non-mouse C controller path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ControllerInput {
    pub title_demo: bool,
    pub now_buttons: u16,
    pub triggered_buttons: u16,
    pub stick_x: i16,
    pub stick_y: i16,
}

impl ControllerInput {
    /// Port of C `getButton` without the `MOUSE_INPUT` augmentation.
    pub fn get_button(self) -> u16 {
        if self.title_demo { 0 } else { self.now_buttons }
    }

    /// Port of C `getTrigger` without the `MOUSE_INPUT` augmentation.
    pub fn get_trigger(self) -> u16 {
        if self.title_demo {
            0
        } else {
            self.triggered_buttons
        }
    }

    /// Port of C `chkButton`'s complete-mask check.
    pub fn chk_button(self, mask: u16) -> bool {
        !self.title_demo && (mask & self.get_button()) == mask
    }

    /// Port of C `chkTrigger`'s complete-mask check.
    pub fn chk_trigger(self, mask: u16) -> bool {
        !self.title_demo && (mask & self.get_trigger()) == mask
    }

    /// Port of C `getJoystick_X`.
    pub fn get_joystick_x(self) -> i16 {
        if self.title_demo { 0 } else { self.stick_x }
    }

    /// Port of C `getJoystick_Y`.
    pub fn get_joystick_y(self) -> i16 {
        if self.title_demo { 0 } else { self.stick_y }
    }

    /// Port of the non-mouse branch of C `mCon_main`.
    pub fn mcon_main(self, state: &mut ControllerState) {
        state.mcon_calc(self.get_joystick_x() as f32, self.get_joystick_y() as f32);
    }
}

fn check_percent_abs(x: f32, min: f32, max: f32, scale: f32, shift_by_min: bool) -> f32 {
    if (-min <= x) && (x <= min) {
        0.0
    } else if x >= max {
        1.0
    } else if x <= -max {
        -1.0
    } else if shift_by_min {
        if x > 0.0 {
            (x - min) * scale
        } else {
            (x + min) * scale
        }
    } else {
        x * scale
    }
}

fn atan_table(y: f32, x: f32) -> i32 {
    if x == 0.0 {
        return 0x2000;
    }
    let index = ((y / x) * 1024.0 + 0.5) as i32;
    let index = index.clamp(0, 0x400);
    ((index as f32 / 1024.0).atan() * 32768.0 / PI).round() as i32
}

/// Port of C `atans_table`, including its `(x, y)` argument order.
fn atan_short(x: f32, y: f32) -> i16 {
    let table = if y >= 0.0 {
        if x >= 0.0 {
            if x >= y {
                if y != 0.0 { atan_table(y, x) } else { 0 }
            } else {
                0x4000 - atan_table(x, y)
            }
        } else if -x < y {
            atan_table(-x, y) + 0x4000
        } else {
            0x8000 - atan_table(y, -x)
        }
    } else if x < 0.0 {
        if x <= y {
            atan_table(-y, -x) + 0x8000
        } else {
            0xC000 - atan_table(-x, -y)
        }
    } else if x < -y {
        atan_table(x, -y) + 0xC000
    } else {
        -atan_table(-y, x)
    };
    table as i16
}

fn cos_short(angle: i16) -> f32 {
    (angle as f32 * (2.0 * PI / 65536.0)).cos()
}

fn sin_short(angle: i16) -> f32 {
    (angle as f32 * (2.0 * PI / 65536.0)).sin()
}
