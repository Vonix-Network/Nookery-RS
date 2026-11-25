/*
 * Copyright (C) 2017 - 2018 FIX94
 *
 * This software may be modified and distributed under the terms
 * of the MIT license.  See the LICENSE file for details.
 */
//! 6502 instruction and interrupt state from `cpu.c`.

pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);
    fn irq(&self) -> bool {
        false
    }
    fn nmi(&mut self) -> bool {
        false
    }
}
#[derive(Clone, Copy, Default)]
pub struct Cpu {
    pub a: u8,
    p: u8,
    x: u8,
    y: u8,
    s: u8,
    pub pc: u16,
    pub cycles: u64,
    pub stopped: bool,
}
const C: u8 = 1;
const Z: u8 = 2;
const I: u8 = 4;
const V: u8 = 64;
const N: u8 = 128;
impl Cpu {
    pub fn reset<B: Bus>(&mut self, b: &mut B) {
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.s = 0xfd;
        self.p = I | 0x20;
        self.pc = u16::from_le_bytes([b.read(0xfffc), b.read(0xfffd)]);
        self.cycles = 7;
        self.stopped = false;
    }
    fn set_nz(&mut self, v: u8) {
        if v == 0 {
            self.p |= Z
        } else {
            self.p &= !Z
        }
        if v & 0x80 != 0 {
            self.p |= N
        } else {
            self.p &= !N
        }
    }
    fn push<B: Bus>(&mut self, b: &mut B, v: u8) {
        b.write(0x100 | u16::from(self.s), v);
        self.s = self.s.wrapping_sub(1)
    }
    fn pop<B: Bus>(&mut self, b: &mut B) -> u8 {
        self.s = self.s.wrapping_add(1);
        b.read(0x100 | u16::from(self.s))
    }
    fn imm<B: Bus>(&mut self, b: &mut B) -> u8 {
        let v = b.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn abs<B: Bus>(&mut self, b: &mut B) -> u16 {
        let lo = self.imm(b);
        let hi = self.imm(b);
        u16::from_le_bytes([lo, hi])
    }
    fn branch<B: Bus>(&mut self, b: &mut B, cond: bool) {
        let off = self.imm(b) as i8;
        if cond {
            self.pc = self.pc.wrapping_add_signed(i16::from(off));
        }
    }
    fn adc(&mut self, v: u8) {
        let sum = u16::from(self.a) + u16::from(v) + u16::from(self.p & C);
        let r = sum as u8;
        if (!(self.a ^ v) & (self.a ^ r) & 0x80) != 0 {
            self.p |= V
        } else {
            self.p &= !V
        }
        if sum > 255 {
            self.p |= C
        } else {
            self.p &= !C
        }
        self.a = r;
        self.set_nz(r)
    }
    pub fn step<B: Bus>(&mut self, b: &mut B) -> u8 {
        if self.stopped {
            return 0;
        }
        if b.nmi() {
            self.interrupt(b, 0xfffa);
            return 7;
        }
        if b.irq() && self.p & I == 0 {
            self.interrupt(b, 0xfffe);
            return 7;
        }
        let op = self.imm(b);
        let before = self.pc;
        let mut cycles = 2;
        match op {
            0x00 => {
                self.pc = self.pc.wrapping_add(1);
                self.interrupt(b, 0xfffe);
                cycles = 7
            }
            0xEA => cycles = 2,
            0xA9 => {
                self.a = self.imm(b);
                self.set_nz(self.a);
                cycles = 2
            }
            0xA2 => {
                self.x = self.imm(b);
                self.set_nz(self.x);
                cycles = 2
            }
            0xA0 => {
                self.y = self.imm(b);
                self.set_nz(self.y);
                cycles = 2
            }
            0xA5 => {
                let a = u16::from(self.imm(b));
                self.a = b.read(a);
                self.set_nz(self.a);
                cycles = 3
            }
            0xB5 => {
                let a = u16::from(self.imm(b).wrapping_add(self.x));
                self.a = b.read(a);
                self.set_nz(self.a);
                cycles = 4
            }
            0xAD => {
                let a = self.abs(b);
                self.a = b.read(a);
                self.set_nz(self.a);
                cycles = 4
            }
            0xBD => {
                let a = self.abs(b).wrapping_add(u16::from(self.x));
                self.a = b.read(a);
                self.set_nz(self.a);
                cycles = 4
            }
            0xB9 => {
                let a = self.abs(b).wrapping_add(u16::from(self.y));
                self.a = b.read(a);
                self.set_nz(self.a);
                cycles = 4
            }
            0x85 => {
                let a = u16::from(self.imm(b));
                b.write(a, self.a);
                cycles = 3
            }
            0x95 => {
                let a = u16::from(self.imm(b).wrapping_add(self.x));
                b.write(a, self.a);
                cycles = 4
            }
            0x8D => {
                let a = self.abs(b);
                b.write(a, self.a);
                cycles = 4
            }
            0x9D => {
                let a = self.abs(b).wrapping_add(u16::from(self.x));
                b.write(a, self.a);
                cycles = 5
            }
            0x99 => {
                let a = self.abs(b).wrapping_add(u16::from(self.y));
                b.write(a, self.a);
                cycles = 5
            }
            0x86 => {
                let a = u16::from(self.imm(b));
                b.write(a, self.x);
                cycles = 3
            }
            0x8E => {
                let a = self.abs(b);
                b.write(a, self.x);
                cycles = 4
            }
            0x84 => {
                let a = u16::from(self.imm(b));
                b.write(a, self.y);
                cycles = 3
            }
            0x8C => {
                let a = self.abs(b);
                b.write(a, self.y);
                cycles = 4
            }
            0xAA => {
                self.x = self.a;
                self.set_nz(self.x)
            }
            0xA8 => {
                self.y = self.a;
                self.set_nz(self.y)
            }
            0x8A => {
                self.a = self.x;
                self.set_nz(self.a)
            }
            0x98 => {
                self.a = self.y;
                self.set_nz(self.a)
            }
            0xBA => {
                self.x = self.s;
                self.set_nz(self.x)
            }
            0x9A => self.s = self.x,
            0xE8 => {
                self.x = self.x.wrapping_add(1);
                self.set_nz(self.x)
            }
            0xC8 => {
                self.y = self.y.wrapping_add(1);
                self.set_nz(self.y)
            }
            0xCA => {
                self.x = self.x.wrapping_sub(1);
                self.set_nz(self.x)
            }
            0x88 => {
                self.y = self.y.wrapping_sub(1);
                self.set_nz(self.y)
            }
            0x69 => {
                let v = self.imm(b);
                self.adc(v);
                cycles = 2
            }
            0xE9 | 0xEB => {
                let v = !self.imm(b);
                self.adc(v);
                cycles = 2
            }
            0x29 => {
                self.a &= self.imm(b);
                self.set_nz(self.a)
            }
            0x09 => {
                self.a |= self.imm(b);
                self.set_nz(self.a)
            }
            0x49 => {
                self.a ^= self.imm(b);
                self.set_nz(self.a)
            }
            0xC9 => {
                let v = self.imm(b);
                let r = self.a.wrapping_sub(v);
                if self.a >= v {
                    self.p |= C
                } else {
                    self.p &= !C
                }
                self.set_nz(r)
            }
            0x4C => {
                self.pc = self.abs(b);
                cycles = 3
            }
            0x6C => {
                let a = self.abs(b);
                let lo = b.read(a);
                let hi = b.read((a & 0xff00) | ((a + 1) & 0xff));
                self.pc = u16::from_le_bytes([lo, hi]);
                cycles = 5
            }
            0x20 => {
                let a = self.abs(b);
                let r = self.pc.wrapping_sub(1);
                self.push(b, (r >> 8) as u8);
                self.push(b, r as u8);
                self.pc = a;
                cycles = 6
            }
            0x60 => {
                let lo = self.pop(b);
                let hi = self.pop(b);
                self.pc = u16::from_le_bytes([lo, hi]).wrapping_add(1);
                cycles = 6
            }
            0x48 => {
                self.push(b, self.a);
                cycles = 3
            }
            0x68 => {
                self.a = self.pop(b);
                self.set_nz(self.a);
                cycles = 4
            }
            0x08 => {
                self.push(b, self.p | 0x30);
                cycles = 3
            }
            0x28 => {
                self.p = (self.pop(b) & !0x30) | 0x20;
                cycles = 4
            }
            0xD0 => {
                self.branch(b, self.p & Z == 0);
                cycles = 2
            }
            0xF0 => {
                self.branch(b, self.p & Z != 0);
                cycles = 2
            }
            0x90 => {
                self.branch(b, self.p & C == 0);
                cycles = 2
            }
            0xB0 => {
                self.branch(b, self.p & C != 0);
                cycles = 2
            }
            0x10 => {
                self.branch(b, self.p & N == 0);
                cycles = 2
            }
            0x30 => {
                self.branch(b, self.p & N != 0);
                cycles = 2
            }
            0x18 => self.p &= !C,
            0x38 => self.p |= C,
            0x58 => self.p &= !I,
            0x78 => self.p |= I,
            0xB8 => self.p &= !V,
            0xD8 => self.p &= !8,
            0xF8 => self.p |= 8,
            0x40 => {
                self.p = self.pop(b);
                let lo = self.pop(b);
                let hi = self.pop(b);
                self.pc = u16::from_le_bytes([lo, hi]);
                cycles = 6
            }
            0x4A => {
                if self.a & 1 != 0 {
                    self.p |= C
                } else {
                    self.p &= !C
                }
                self.a >>= 1;
                self.set_nz(self.a)
            }
            _ => {
                let _ = before;
                cycles = 2
            }
        };
        self.cycles += u64::from(cycles);
        cycles
    }
    fn interrupt<B: Bus>(&mut self, b: &mut B, vector: u16) {
        self.push(b, (self.pc >> 8) as u8);
        self.push(b, self.pc as u8);
        self.push(b, self.p | 0x20);
        self.p |= I;
        self.pc = u16::from_le_bytes([b.read(vector), b.read(vector + 1)]);
    }
}
