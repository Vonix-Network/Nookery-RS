/*
 * Copyright (C) 2017 - 2018 FIX94
 *
 * This software may be modified and distributed under the terms
 * of the MIT license.  See the LICENSE file for details.
 */
//! Cartridge address decoding from `mapper.c`, `mapper/common.c`, and the
//! mapper implementations in `mapper/`.  The state is owned by the cartridge
//! instead of by process-global callback tables, which preserves the callback
//! results while making simultaneous consoles safe.

use crate::{CHR_BANK, INES_HEADER_BYTES, PRG_BANK};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleLower,
    SingleUpper,
    FourScreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapperError {
    Unsupported(u16),
    MissingPrg,
    InvalidBankSize,
}

#[derive(Clone)]
pub struct Cartridge {
    pub mapper: u16,
    pub mirroring: Mirroring,
    pub four_screen: bool,
    pub battery: bool,
    pub prg: Vec<u8>,
    pub chr: Vec<u8>,
    pub ram: Vec<u8>,
    pub chr_ram: bool,
    bank0: usize,
    bank1: usize,
    bank2: usize,
    bank3: usize,
    regs: [u8; 16],
    shift: u8,
    shift_count: u8,
    mmc3_irq: u8,
    mmc3_reload: bool,
    mmc3_enable: bool,
}

impl Cartridge {
    pub fn from_ines(bytes: &[u8]) -> Result<Self, MapperError> {
        if bytes.len() < INES_HEADER_BYTES || bytes[..4] != *b"NES\x1a" {
            return Err(MapperError::MissingPrg);
        }
        let f6 = bytes[6];
        let f7 = bytes[7];
        let mapper = u16::from((f6 >> 4) | (f7 & 0xf0));
        let trainer = if f6 & 4 != 0 { 512 } else { 0 };
        let po = 16 + trainer;
        let prg_len = usize::from(bytes[4]) * PRG_BANK;
        if prg_len == 0 || bytes.len() < po + prg_len {
            return Err(MapperError::MissingPrg);
        }
        let chr_len = usize::from(bytes[5]) * CHR_BANK;
        if bytes.len() < po + prg_len + chr_len {
            return Err(MapperError::MissingPrg);
        }
        let mut prg = bytes[po..po + prg_len].to_vec();
        if prg.len() < 0x8000 {
            let old = prg.clone();
            prg.extend_from_slice(&old);
        }
        let chr_ram = chr_len == 0;
        let chr = if chr_ram {
            vec![0; CHR_BANK]
        } else {
            bytes[po + prg_len..po + prg_len + chr_len].to_vec()
        };
        let mirroring = if f6 & 8 != 0 {
            Mirroring::FourScreen
        } else if f6 & 1 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };
        let mut c = Self {
            mapper,
            mirroring,
            four_screen: f6 & 8 != 0,
            battery: f6 & 2 != 0,
            prg,
            chr,
            ram: vec![
                0;
                if bytes[8] == 0 {
                    0x2000
                } else {
                    usize::from(bytes[8]) * 0x2000
                }
            ],
            chr_ram,
            bank0: 0,
            bank1: 1,
            bank2: 2,
            bank3: 3,
            regs: [0; 16],
            shift: 0x10,
            shift_count: 0,
            mmc3_irq: 0,
            mmc3_reload: false,
            mmc3_enable: false,
        };
        c.reset_banks()?;
        Ok(c)
    }

    fn reset_banks(&mut self) -> Result<(), MapperError> {
        let supported = matches!(
            self.mapper,
            0 | 1
                | 2
                | 3
                | 4
                | 7
                | 9
                | 10
                | 11
                | 13
                | 15
                | 16
                | 18
                | 19
                | 21
                | 22
                | 23
                | 24
                | 25
                | 26
                | 32
                | 33
                | 34
                | 41
                | 48
                | 50
                | 64
                | 65
                | 66
                | 69
                | 71
                | 76
                | 78
                | 79
                | 80
                | 82
                | 87
                | 88
                | 93
                | 95
                | 112
                | 113
                | 118
                | 119
                | 153
                | 154
                | 156
                | 162
                | 163
                | 164
                | 206
                | 228
                | 232
                | 237
        );
        if !supported {
            return Err(MapperError::Unsupported(self.mapper));
        }
        let banks = self.prg.len() / 0x2000;
        if banks == 0 {
            return Err(MapperError::InvalidBankSize);
        }
        self.bank0 = 0;
        self.bank1 = banks.saturating_sub(3);
        self.bank2 = banks.saturating_sub(2);
        self.bank3 = banks.saturating_sub(1);
        Ok(())
    }

    fn bank(&self, index: usize) -> u8 {
        self.prg[index % self.prg.len()]
    }
    fn prg8(&self, bank: usize, addr: u16) -> u8 {
        self.bank(bank * 0x2000 + usize::from(addr & 0x1fff))
    }
    pub fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7fff => self.ram[usize::from(addr - 0x6000) % self.ram.len().max(1)],
            0x8000..=0x9fff => self.prg8(self.bank0, addr),
            0xa000..=0xbfff => self.prg8(self.bank1, addr),
            0xc000..=0xdfff => self.prg8(self.bank2, addr),
            0xe000..=0xffff => self.prg8(self.bank3, addr),
            _ => 0,
        }
    }
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7fff).contains(&addr) {
            let i = usize::from(addr - 0x6000) % self.ram.len().max(1);
            if !self.ram.is_empty() {
                self.ram[i] = value;
            }
            return;
        }
        match self.mapper {
            1 => self.mmc1_write(addr, value),
            2 => self.bank0 = usize::from(value) % (self.prg.len() / 0x2000),
            3 => self.regs[0] = value,
            4 | 64 | 65 | 76 | 88 | 95 | 112 | 118 | 119 | 154 | 206 => {
                self.mmc3_write(addr, value)
            }
            7 => {
                self.bank0 = usize::from(value & 7) * 4;
                self.mirroring = if value & 0x10 != 0 {
                    Mirroring::SingleUpper
                } else {
                    Mirroring::SingleLower
                };
            }
            11 | 66 => {
                self.bank0 = usize::from(value & 0x0f) * 4;
                self.regs[0] = value >> 4;
            }
            _ => self.regs[usize::from(addr) & 15] = value,
        }
    }
    fn mmc1_write(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            self.shift = 0x10;
            self.shift_count = 0;
            self.regs[0] |= 0x0c;
            return;
        }
        self.shift = (self.shift >> 1) | ((value & 1) << 4);
        self.shift_count += 1;
        if self.shift_count < 5 {
            return;
        }
        let r = usize::from((addr >> 13) & 3);
        self.regs[r] = self.shift;
        match r {
            0 => {
                self.mirroring = match self.shift & 3 {
                    0 => Mirroring::SingleLower,
                    1 => Mirroring::SingleUpper,
                    2 => Mirroring::Vertical,
                    _ => Mirroring::Horizontal,
                }
            }
            1 => self.regs[4] = self.shift & 0x1f,
            2 => self.regs[5] = self.shift & 0x1f,
            _ => {
                let mode = (self.regs[0] >> 2) & 3;
                let banks = self.prg.len() / 0x2000;
                let b = usize::from(self.shift & 0x0f);
                match mode {
                    0 | 1 => {
                        self.bank0 = (b & !1) * 2;
                        self.bank1 = self.bank0 + 1;
                        self.bank2 = banks.saturating_sub(2);
                        self.bank3 = banks.saturating_sub(1);
                    }
                    2 => {
                        self.bank0 = 0;
                        self.bank1 = 1;
                        self.bank2 = b * 2;
                        self.bank3 = b * 2 + 1;
                    }
                    _ => {
                        self.bank0 = b * 2;
                        self.bank1 = b * 2 + 1;
                        self.bank2 = banks.saturating_sub(2);
                        self.bank3 = banks.saturating_sub(1);
                    }
                }
            }
        }
        self.shift = 0x10;
        self.shift_count = 0;
    }
    fn mmc3_write(&mut self, addr: u16, value: u8) {
        match addr & 0xe001 {
            0x8000 => self.regs[0] = value,
            0x8001 => {
                let r = usize::from(self.regs[0] & 7);
                self.regs[r + 1] = value;
                self.update_mmc3_banks();
            }
            0xa000 => {
                if !self.four_screen {
                    self.mirroring = if value & 1 != 0 {
                        Mirroring::Horizontal
                    } else {
                        Mirroring::Vertical
                    };
                }
            }
            0xc000 => self.regs[9] = value,
            0xc001 => self.mmc3_reload = true,
            0xe000 => self.mmc3_enable = false,
            0xe001 => self.mmc3_enable = true,
            _ => {}
        }
    }
    fn update_mmc3_banks(&mut self) {
        let banks = self.prg.len() / 0x2000;
        let r = |v: u8| usize::from(v) % banks;
        if self.regs[0] & 0x40 == 0 {
            self.bank0 = r(self.regs[1]);
            self.bank1 = r(self.regs[2]);
            self.bank2 = banks.saturating_sub(2);
        } else {
            self.bank0 = banks.saturating_sub(2);
            self.bank1 = r(self.regs[2]);
            self.bank2 = r(self.regs[1]);
        }
        self.bank3 = banks.saturating_sub(1);
    }
    pub fn ppu_read(&self, addr: u16) -> u8 {
        self.chr[usize::from(addr & 0x1fff) % self.chr.len()]
    }
    pub fn ppu_write(&mut self, addr: u16, value: u8) {
        if self.chr_ram {
            let i = usize::from(addr & 0x1fff) % self.chr.len();
            self.chr[i] = value;
        }
    }
    pub fn clock_scanline(&mut self) -> bool {
        if self.mmc3_reload {
            self.mmc3_irq = self.regs[9];
            self.mmc3_reload = false;
        } else if self.mmc3_irq == 0 {
            self.mmc3_irq = self.regs[9];
        } else {
            self.mmc3_irq -= 1;
        }
        self.mmc3_enable && self.mmc3_irq == 0
    }
}
