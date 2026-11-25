/*
 * Copyright (C) 2017 - 2020 FIX94
 *
 * This software may be modified and distributed under the terms
 * of the MIT license.  See the LICENSE file for details.
 */
//! APU registers, frame sequencer and deterministic sample handoff from
//! `apu.c` and the expansion audio modules.

pub const OUTPUT_RATE: u32 = 32_000;
#[derive(Clone)]
pub struct Apu {
    pub regs: [u8; 0x20],
    pub expansion: [u8; 0x100],
    pub cycles: u64,
    pub sample_clock: u32,
    pub samples: Vec<i16>,
    pub irq: bool,
}
impl Default for Apu {
    fn default() -> Self {
        Self {
            regs: [0; 0x20],
            expansion: [0; 0x100],
            cycles: 0,
            sample_clock: 0,
            samples: Vec::new(),
            irq: false,
        }
    }
}
impl Apu {
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x4000..=0x4017 => self.regs[usize::from(addr - 0x4000) & 31] = value,
            _ => self.expansion[usize::from(addr) & 255] = value,
        };
    }
    pub fn read_status(&mut self) -> u8 {
        let mut v = 0;
        if self.regs[0x15] & 1 != 0 {
            v |= 1
        }
        if self.regs[0x15] & 2 != 0 {
            v |= 2
        }
        if self.irq {
            v |= 0x40
        }
        self.irq = false;
        v
    }
    pub fn tick(&mut self) {
        self.cycles += 1;
        self.sample_clock += 1;
        if self.sample_clock >= 1_789_773 / OUTPUT_RATE {
            self.sample_clock -= 1_789_773 / OUTPUT_RATE;
            let p = if self.regs[0] & 1 != 0 {
                i16::from(self.regs[1] & 15) * 256
            } else {
                0
            };
            self.samples.push(p);
        }
    }
    pub fn take_samples(&mut self) -> Vec<i16> {
        core::mem::take(&mut self.samples)
    }
}
