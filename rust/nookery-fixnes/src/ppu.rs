/*
 * Copyright (C) 2017 - 2020 FIX94
 *
 * This software may be modified and distributed under the terms
 * of the MIT license.  See the LICENSE file for details.
 */
//! PPU register, VRAM, OAM and indexed-picture logic from `ppu.c`.

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;

#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub pixels: [u8; WIDTH * HEIGHT],
    pub number: u64,
}
impl Frame {
    pub fn hash(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for b in self.pixels {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

#[derive(Clone)]
pub struct Ppu {
    pub vram: [u8; 0x800],
    pub palette: [u8; 32],
    pub oam: [u8; 256],
    pub regs: [u8; 8],
    pub addr: u16,
    pub latch: bool,
    pub read_buffer: u8,
    pub dot: u16,
    pub line: i16,
    pub frame: u64,
    pub nmi: bool,
    pub odd_frame: bool,
}
impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: [0; 0x800],
            palette: [0; 32],
            oam: [0; 256],
            regs: [0; 8],
            addr: 0,
            latch: false,
            read_buffer: 0,
            dot: 0,
            line: -1,
            frame: 0,
            nmi: false,
            odd_frame: false,
        }
    }
}
impl Ppu {
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr & 7 {
            2 => {
                let v = self.regs[2] & 0xe0;
                self.regs[2] &= !0x80;
                self.latch = false;
                v
            }
            4 => self.oam[usize::from(self.regs[3])],
            7 => {
                let a = self.addr;
                self.addr = self
                    .addr
                    .wrapping_add(if self.regs[0] & 4 != 0 { 32 } else { 1 });
                if a & 0x3fff >= 0x3f00 {
                    let v = self.ppu_read(a);
                    self.read_buffer = self.ppu_read(a & 0x2fff);
                    v
                } else {
                    let v = self.read_buffer;
                    self.read_buffer = self.ppu_read(a);
                    v
                }
            }
            _ => 0,
        }
    }
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 7 {
            0 => self.regs[0] = value,
            1 => self.regs[1] = value,
            3 => self.regs[3] = value,
            4 => {
                self.oam[usize::from(self.regs[3])] = value;
                self.regs[3] = self.regs[3].wrapping_add(1)
            }
            5 => self.latch = !self.latch,
            6 => {
                if !self.latch {
                    self.addr = (u16::from(value & 0x3f)) << 8;
                } else {
                    self.addr = (self.addr & 0xff00) | u16::from(value);
                }
                self.latch = !self.latch
            }
            7 => {
                self.ppu_write(self.addr, value);
                self.addr = self
                    .addr
                    .wrapping_add(if self.regs[0] & 4 != 0 { 32 } else { 1 });
            }
            _ => {}
        }
    }
    pub fn ppu_read(&self, addr: u16) -> u8 {
        let a = addr & 0x3fff;
        match a {
            0..=0x1fff => 0,
            0x2000..=0x3eff => self.vram[usize::from(a - 0x2000) & 0x7ff],
            _ => self.palette[usize::from(a - 0x3f00) & 31],
        }
    }
    pub fn ppu_write(&mut self, addr: u16, value: u8) {
        let a = addr & 0x3fff;
        match a {
            0..=0x1fff => {}
            0x2000..=0x3eff => self.vram[usize::from(a - 0x2000) & 0x7ff] = value,
            _ => self.palette[usize::from(a - 0x3f00) & 31] = value & 0x3f,
        }
    }
    pub fn tick(&mut self) {
        self.dot += 1;
        if self.dot >= 341 {
            self.dot = 0;
            self.line += 1;
            if self.line == 241 && self.regs[1] & 0x18 != 0 {
                self.regs[2] |= 0x80;
                if self.regs[0] & 0x80 != 0 {
                    self.nmi = true;
                }
            }
            if self.line >= 261 {
                self.line = -1;
                self.frame += 1;
                self.odd_frame = !self.odd_frame;
                self.regs[2] &= !0x80;
            }
        }
    }
    pub fn render(&self, chr: &[u8]) -> Frame {
        let mut f = Frame {
            pixels: [0; WIDTH * HEIGHT],
            number: self.frame,
        };
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let tile_x = x / 8;
                let tile_y = y / 8;
                let tile = self.vram[(tile_y * 32 + tile_x) & 0x7ff] as usize;
                let row = y & 7;
                let p0 = chr[(tile * 16 + row) % chr.len()];
                let p1 = chr[(tile * 16 + row + 8) % chr.len()];
                let bit = 7 - (x & 7);
                let color = ((p0 >> bit) & 1) | (((p1 >> bit) & 1) << 1);
                let pal = if color == 0 {
                    self.palette[0]
                } else {
                    self.palette[color as usize]
                };
                f.pixels[y * WIDTH + x] = pal & 0x3f;
            }
        }
        f
    }
}
