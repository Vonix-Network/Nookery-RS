/*
 * Copyright (C) 2017 FIX94
 *
 * This software may be modified and distributed under the terms
 * of the MIT license.  See the LICENSE file for details.
 */
//! Controller shift registers from `input.c`.

#[derive(Clone, Copy, Default)]
pub struct Input {
    pub buttons: [u8; 2],
    pub shift: [u8; 2],
    pub strobe: bool,
}
impl Input {
    pub fn set(&mut self, port: usize, buttons: u8) {
        self.buttons[port & 1] = buttons;
        if self.strobe {
            self.shift = self.buttons;
        }
    }
    pub fn write(&mut self, value: u8) {
        let old = self.strobe;
        self.strobe = value & 1 != 0;
        if self.strobe || (old && !self.strobe) {
            self.shift = self.buttons;
        }
    }
    pub fn read(&mut self, port: usize) -> u8 {
        let i = port & 1;
        let v = if self.strobe {
            self.buttons[i] & 1
        } else {
            let b = self.shift[i] & 1;
            self.shift[i] = (self.shift[i] >> 1) | 0x80;
            b
        };
        v | 0x40
    }
}
