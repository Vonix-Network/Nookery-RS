/*
 * Copyright (C) 2017 - 2020 FIX94
 *
 * This software may be modified and distributed under the terms
 * of the MIT license.  See the LICENSE file for details.
 */
//! A native, allocation-stable Rust port of the fixNES execution core.
//!
//! The C implementation uses global callback tables.  This port keeps the
//! same bus ordering and register-visible state but makes the machine an owned
//! value, which lets ROM vectors run in parallel without cross-test state.

#![forbid(unsafe_code)]

pub mod apu;
pub mod cpu;
pub mod input;
pub mod mapper;
pub mod ppu;

use apu::Apu;
use cpu::{Bus, Cpu};
use input::Input;
use mapper::{Cartridge, MapperError};
use ppu::{Frame, Ppu};

pub const INES_HEADER_BYTES: usize = 16;
pub const PRG_BANK: usize = 0x4000;
pub const CHR_BANK: usize = 0x2000;
pub const CPU_RAM_BYTES: usize = 0x800;
pub const NTSC_CPU_CYCLES_PER_FRAME: usize = 29_781;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomError {
    TooShort(usize),
    BadMagic,
    Mapper(MapperError),
    Truncated { required: usize, actual: usize },
}
impl core::fmt::Display for RomError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort(n) => write!(f, "iNES header is short: {n} bytes"),
            Self::BadMagic => f.write_str("iNES magic is missing"),
            Self::Mapper(e) => write!(f, "mapper error: {e:?}"),
            Self::Truncated { required, actual } => {
                write!(f, "ROM needs {required} bytes, only {actual} supplied")
            }
        }
    }
}
impl core::error::Error for RomError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RomInfo {
    pub mapper: u16,
    pub prg_banks: u8,
    pub chr_banks: u8,
    pub trainer: bool,
    pub battery: bool,
}
pub fn inspect_rom(bytes: &[u8]) -> Result<RomInfo, RomError> {
    if bytes.len() < 16 {
        return Err(RomError::TooShort(bytes.len()));
    }
    if bytes[..4] != *b"NES\x1a" {
        return Err(RomError::BadMagic);
    }
    let f6 = bytes[6];
    let f7 = bytes[7];
    let required = 16
        + if f6 & 4 != 0 { 512 } else { 0 }
        + usize::from(bytes[4]) * PRG_BANK
        + usize::from(bytes[5]) * CHR_BANK;
    if bytes.len() < required {
        return Err(RomError::Truncated {
            required,
            actual: bytes.len(),
        });
    }
    Ok(RomInfo {
        mapper: u16::from((f6 >> 4) | (f7 & 0xf0)),
        prg_banks: bytes[4],
        chr_banks: bytes[5],
        trainer: f6 & 4 != 0,
        battery: f6 & 2 != 0,
    })
}

#[derive(Clone)]
pub struct Nes {
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub apu: Apu,
    pub input: Input,
    pub cartridge: Cartridge,
    pub ram: [u8; CPU_RAM_BYTES],
    pub frame_number: u64,
    pub irq: bool,
}
impl Nes {
    pub fn load(bytes: &[u8]) -> Result<Self, RomError> {
        let _ = inspect_rom(bytes)?;
        let cart = Cartridge::from_ines(bytes).map_err(RomError::Mapper)?;
        let mut n = Self {
            cpu: Cpu::default(),
            ppu: Ppu::default(),
            apu: Apu::default(),
            input: Input::default(),
            cartridge: cart,
            ram: [0; CPU_RAM_BYTES],
            frame_number: 0,
            irq: false,
        };
        n.reset();
        Ok(n)
    }
    pub fn reset(&mut self) {
        let mut c = self.cpu;
        c.reset(self);
        self.cpu = c;
        self.ppu = Ppu::default();
        self.apu = Apu::default();
        self.frame_number = 0;
    }
    pub fn set_controller(&mut self, port: usize, buttons: u8) {
        self.input.set(port, buttons)
    }
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        self.read(addr)
    }
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        self.write(addr, value)
    }
    pub fn step_instruction(&mut self) -> u8 {
        let mut c = self.cpu;
        let cycles = c.step(self);
        self.cpu = c;
        for _ in 0..cycles {
            self.apu.tick();
            self.ppu.tick();
            self.ppu.tick();
            self.ppu.tick();
        }
        cycles
    }
    pub fn run_frame(&mut self) -> Frame {
        let target = self.frame_number + 1;
        while self.ppu.frame < target {
            self.step_instruction();
        }
        self.frame_number = target;
        self.ppu.render(&self.cartridge.chr)
    }
    pub fn audio_samples(&mut self) -> Vec<i16> {
        self.apu.take_samples()
    }
}
impl Bus for Nes {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1fff => self.ram[usize::from(addr) & 0x7ff],
            0x2000..=0x3fff => self.ppu.cpu_read(addr),
            0x4015 => self.apu.read_status(),
            0x4016 => self.input.read(0),
            0x4017 => self.input.read(1),
            0x4020..=0xffff => self.cartridge.cpu_read(addr),
            _ => 0,
        }
    }
    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1fff => self.ram[usize::from(addr) & 0x7ff] = value,
            0x2000..=0x3fff => self.ppu.cpu_write(addr, value),
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, value),
            0x4014 => {
                let base = u16::from(value) << 8;
                for i in 0..=255 {
                    let v = self.read(base + i);
                    self.ppu.oam[i as usize] = v;
                }
            }
            0x4016 => self.input.write(value),
            0x4020..=0xffff => self.cartridge.cpu_write(addr, value),
            _ => {}
        }
    }
    fn irq(&self) -> bool {
        self.irq || self.apu.irq
    }
    fn nmi(&mut self) -> bool {
        let v = self.ppu.nmi;
        self.ppu.nmi = false;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rom(program: &[u8]) -> Vec<u8> {
        let mut r = vec![0; 16 + 0x4000];
        r[..4].copy_from_slice(b"NES\x1a");
        r[4] = 1;
        r[16..16 + program.len()].copy_from_slice(program);
        r[16 + 0x3ffc] = 0;
        r[16 + 0x3ffd] = 0x80;
        r
    }
    #[test]
    fn reset_vector_and_cpu_store_are_deterministic() {
        let image = rom(&[0xa9, 0x5a, 0x8d, 0x00, 0x02, 0x4c, 0x05, 0x80]);
        let mut n = Nes::load(&image).unwrap();
        for _ in 0..3 {
            n.step_instruction();
        }
        assert_eq!((n.cpu.pc, n.ram[0x200]), (0x8005, 0x5a));
    }
    #[test]
    fn controllers_shift_lsb_first() {
        let mut i = Input::default();
        i.set(0, 0b1010_0101);
        i.write(1);
        i.write(0);
        let got = (0..8).map(|_| i.read(0) & 1).collect::<Vec<_>>();
        assert_eq!(got, vec![1, 0, 1, 0, 0, 1, 0, 1]);
    }
    #[test]
    fn malformed_roms_keep_error_categories() {
        assert_eq!(inspect_rom(&[]), Err(RomError::TooShort(0)));
        assert_eq!(inspect_rom(b"badbadbadbadbad!"), Err(RomError::BadMagic));
    }

    #[test]
    fn indexed_picture_vector_is_stable() {
        let mut image = rom(&[0x4c, 0x00, 0x80]);
        image[5] = 1;
        image.resize(16 + 0x4000 + 0x2000, 0);
        for row in 0..8 {
            image[16 + 0x4000 + 16 + row] = 0xff;
        }
        let mut n = Nes::load(&image).unwrap();
        n.ppu.vram[0] = 1;
        n.ppu.palette[1] = 0x21;
        let frame = n.ppu.render(&n.cartridge.chr);
        assert_eq!(frame.pixels[0], 0x21);
        assert_eq!(frame.pixels[7], 0x21);
        assert_eq!(frame.pixels[8], 0);
        assert_eq!(frame.hash(), 9_347_079_390_592_419_685);
    }
}
