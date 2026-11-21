//! Native deterministic NES core surface used by [`crate::runtime`].
//!
//! The implementation ports the highest-value PC fixNES contracts without
//! calling the C emulator: iNES validation and mapper-0 PRG mapping, the
//! reset-vector/6502 instruction path needed by synthetic fixtures, the
//! controller shift register, NTSC frame timing, and the fixed 32 kHz/60
//! audio handoff size.  Full mapper, pixel PPU, and APU waveform behavior are
//! represented by typed gaps in [`CoreGap`].

use core::fmt;

/// iNES header length used by `pc_fixnes_init`.
pub const INES_HEADER_BYTES: usize = 16;
/// PRG bank size from the iNES format.
pub const PRG_BANK_BYTES: usize = 0x4000;
/// CHR bank size from the iNES format.
pub const CHR_BANK_BYTES: usize = 0x2000;
/// CPU addressable internal RAM, mirrored through `$1fff`.
pub const CPU_RAM_BYTES: usize = 0x800;
/// NTSC PPU dots in one line, from fixNES `DOTS`.
pub const NTSC_DOTS_PER_LINE: u32 = 341;
/// NTSC PPU lines in one frame, from fixNES `LINES_NTSC`.
pub const NTSC_LINES_PER_FRAME: u32 = 262;
/// Output rate selected by `fixnes_capture_audio`.
pub const OUTPUT_AUDIO_RATE: u32 = 32_000;
/// Integer stereo frames requested by `FIXNES_OUTPUT_RATE / 60`.
pub const AUDIO_FRAMES_PER_VIDEO_FRAME: usize = (OUTPUT_AUDIO_RATE / 60) as usize;
/// Interleaved stereo sample count in one output handoff.
pub const AUDIO_SAMPLES_PER_VIDEO_FRAME: usize = AUDIO_FRAMES_PER_VIDEO_FRAME * 2;

/// Why a provided ROM cannot be loaded by this candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomError {
    /// Fewer than the 16 bytes required for an iNES header were provided.
    TooShort {
        /// Number of bytes supplied.
        actual: usize,
    },
    /// The iNES magic was not present.
    BadMagic,
    /// The ROM declares a mapper not implemented by this candidate.
    UnsupportedMapper {
        /// Mapper number encoded by flags 6 and 7.
        mapper: u16,
    },
    /// The trainer bit is set but its 512-byte payload is absent.
    MissingTrainer,
    /// Declared PRG or CHR data runs past the supplied bytes.
    TruncatedData {
        /// First byte required after the header/trainer.
        offset: usize,
        /// Number of bytes declared by the header.
        required: usize,
        /// Number of bytes supplied.
        available: usize,
    },
}

impl fmt::Display for RomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual } => write!(f, "iNES header is short: {actual} bytes"),
            Self::BadMagic => f.write_str("iNES magic is missing"),
            Self::UnsupportedMapper { mapper } => write!(f, "mapper {mapper} is not implemented"),
            Self::MissingTrainer => f.write_str("iNES trainer is truncated"),
            Self::TruncatedData {
                offset,
                required,
                available,
            } => write!(
                f,
                "iNES data at {offset:#x} needs {required} bytes, only {available} remain"
            ),
        }
    }
}

impl core::error::Error for RomError {}

/// A validated, owned iNES image with mapper-0 offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomImage {
    bytes: Vec<u8>,
    prg_offset: usize,
    prg_len: usize,
    chr_offset: usize,
    chr_len: usize,
    chr_ram: bool,
    mapper: u16,
    trainer: bool,
    prg_ram_size: usize,
}

impl RomImage {
    /// Parses a mapper-0 iNES image using the same header fields as fixNES.
    pub fn parse(bytes: Vec<u8>) -> Result<Self, RomError> {
        if bytes.len() < INES_HEADER_BYTES {
            return Err(RomError::TooShort {
                actual: bytes.len(),
            });
        }
        if bytes[..4] != *b"NES\x1a" {
            return Err(RomError::BadMagic);
        }

        let flags6 = bytes[6];
        let flags7 = bytes[7];
        let mapper = u16::from((flags6 >> 4) | (flags7 & 0xf0));
        if mapper != 0 {
            return Err(RomError::UnsupportedMapper { mapper });
        }

        let trainer = flags6 & 0x04 != 0;
        let data_offset = INES_HEADER_BYTES + if trainer { 512 } else { 0 };
        if trainer && bytes.len() < data_offset {
            return Err(RomError::MissingTrainer);
        }

        let prg_len = usize::from(bytes[4]) * PRG_BANK_BYTES;
        let chr_len = usize::from(bytes[5]) * CHR_BANK_BYTES;
        let declared = prg_len + chr_len;
        let available = bytes.len().saturating_sub(data_offset);
        if declared == 0 || declared > available {
            return Err(RomError::TruncatedData {
                offset: data_offset,
                required: declared,
                available,
            });
        }

        let prg_ram_size = if bytes[8] == 0 {
            0x2000
        } else {
            usize::from(bytes[8]) * 0x2000
        };
        Ok(Self {
            bytes,
            prg_offset: data_offset,
            prg_len,
            chr_offset: data_offset + prg_len,
            chr_len,
            chr_ram: chr_len == 0,
            mapper,
            trainer,
            prg_ram_size,
        })
    }

    /// Mapper number parsed from the iNES flags.
    #[must_use]
    pub const fn mapper(&self) -> u16 {
        self.mapper
    }

    /// Whether the optional 512-byte trainer was present.
    #[must_use]
    pub const fn has_trainer(&self) -> bool {
        self.trainer
    }

    /// PRG ROM bytes, excluding the header and trainer.
    #[must_use]
    pub fn prg(&self) -> &[u8] {
        &self.bytes[self.prg_offset..self.prg_offset + self.prg_len]
    }

    /// CHR ROM bytes, or an empty slice when the cartridge declares CHR RAM.
    #[must_use]
    pub fn chr(&self) -> &[u8] {
        &self.bytes[self.chr_offset..self.chr_offset + self.chr_len]
    }

    /// Whether the cartridge uses the default 8 KiB CHR RAM surface.
    #[must_use]
    pub const fn has_chr_ram(&self) -> bool {
        self.chr_ram
    }

    /// PRG RAM size selected by `pc_fixnes_prg_ram_size`.
    #[must_use]
    pub const fn prg_ram_size(&self) -> usize {
        self.prg_ram_size
    }

    /// Reads a mapper-0 CPU PRG address, mirroring a 16 KiB bank.
    #[must_use]
    pub fn prg_read(&self, address: u16) -> u8 {
        let offset = (usize::from(address).saturating_sub(0x8000)) % self.prg_len;
        self.prg()[offset]
    }
}

/// Typed surfaces that remain outside this candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreGap {
    /// The PPU advances frame timing but does not render NES pixels.
    PpuRendering,
    /// The APU emits a correctly sized silent handoff, not channel waveforms.
    ApuWaveforms,
}

/// A deterministic core error after ROM validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreError {
    /// The synthetic/native CPU reached an opcode not yet ported.
    UnsupportedCpuOpcode {
        /// Program counter at opcode fetch.
        pc: u16,
        /// Opcode byte that has no native implementation yet.
        opcode: u8,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCpuOpcode { pc, opcode } => {
                write!(
                    f,
                    "6502 opcode {opcode:#04x} at {pc:#06x} is not implemented"
                )
            }
        }
    }
}

impl core::error::Error for CoreError {}

/// The serialized controller state used by `$4016` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Controller {
    buttons: u8,
    strobe: bool,
    position: u8,
}

impl Controller {
    /// Creates an empty controller.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buttons: 0,
            strobe: false,
            position: 0,
        }
    }

    /// Sets fixNES-format bits: A, B, Select, Start, Up, Down, Left, Right.
    pub fn set_buttons(&mut self, buttons: u8) {
        self.buttons = buttons;
    }

    /// Implements the `$4016` write behavior: low strobe resets the shift.
    pub fn write_strobe(&mut self, value: u8) {
        self.strobe = value & 1 != 0;
        if !self.strobe {
            self.position = 0;
        }
    }

    /// Implements `inputGetP1`, including opposing-direction suppression.
    #[must_use]
    pub fn read(&mut self) -> u8 {
        let value = if self.strobe {
            self.buttons & 1
        } else if self.position < 8 {
            let bit = 1u8 << self.position;
            let opposing = match self.position {
                5 => self.buttons & (1 << 4) != 0,
                7 => self.buttons & (1 << 6) != 0,
                _ => false,
            };
            if opposing || self.buttons & bit == 0 {
                0
            } else {
                1
            }
        } else {
            0
        };
        if !self.strobe {
            self.position = self.position.saturating_add(1);
        }
        value
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

/// CPU-visible result of one native core frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreFrame {
    /// One-based frame count after the completed frame.
    pub frame_number: u64,
    /// CPU cycles consumed to cross one NTSC PPU frame boundary.
    pub cpu_cycles: u32,
    /// Audio handoff captured before the cadence reset.
    pub audio: AudioFrame,
}

/// One fixed-cadence stereo audio handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrame {
    /// Interleaved signed 16-bit stereo samples.
    pub samples: Vec<i16>,
}

impl AudioFrame {
    fn silent() -> Self {
        Self {
            samples: vec![0; AUDIO_SAMPLES_PER_VIDEO_FRAME],
        }
    }

    /// Number of interleaved stereo samples.
    #[must_use]
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Number of stereo frames in this handoff.
    #[must_use]
    pub fn stereo_frames(&self) -> usize {
        self.samples.len() / 2
    }
}

#[derive(Debug, Clone)]
struct Bus {
    ram: [u8; CPU_RAM_BYTES],
    prg: Vec<u8>,
    ppu_registers: [u8; 8],
    apu_registers: [u8; 0x18],
    controller: Controller,
}

impl Bus {
    fn new(rom: &RomImage) -> Self {
        Self {
            ram: [0; CPU_RAM_BYTES],
            prg: rom.prg().to_vec(),
            ppu_registers: [0; 8],
            apu_registers: [0; 0x18],
            controller: Controller::new(),
        }
    }

    fn read(&mut self, address: u16) -> u8 {
        match address {
            0x0000..=0x1fff => self.ram[usize::from(address) & 0x07ff],
            0x2000..=0x3fff => self.ppu_registers[usize::from(address) & 7],
            0x4015 => self.apu_registers[0x15],
            0x4016 => self.controller.read(),
            0x4017 => 0,
            0x6000..=0x7fff => 0,
            0x8000..=0xffff => {
                let offset = (usize::from(address) - 0x8000) % self.prg.len();
                self.prg[offset]
            }
            _ => 0,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x1fff => self.ram[usize::from(address) & 0x07ff] = value,
            0x2000..=0x3fff => self.ppu_registers[usize::from(address) & 7] = value,
            0x4000..=0x4017 => {
                if address == 0x4016 {
                    self.controller.write_strobe(value);
                } else {
                    self.apu_registers[usize::from(address - 0x4000) & 0x17] = value;
                }
            }
            0x6000..=0x7fff => {}
            0x8000..=0xffff => {}
            _ => {}
        }
    }

    fn read_u16(&mut self, address: u16) -> u16 {
        let lo = u16::from(self.read(address));
        let hi = u16::from(self.read(address.wrapping_add(1)));
        lo | (hi << 8)
    }
}

const FLAG_CARRY: u8 = 0x01;
const FLAG_ZERO: u8 = 0x02;
const FLAG_INTERRUPT: u8 = 0x04;
const FLAG_BREAK: u8 = 0x10;
const FLAG_UNUSED: u8 = 0x20;
const FLAG_OVERFLOW: u8 = 0x40;
const FLAG_NEGATIVE: u8 = 0x80;

#[derive(Debug, Clone, Copy)]
struct Cpu {
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    sp: u8,
    pc: u16,
    halted: bool,
}

impl Cpu {
    fn reset(bus: &mut Bus) -> Self {
        let pc = bus.read_u16(0xfffc);
        Self {
            a: 0,
            x: 0,
            y: 0,
            p: FLAG_INTERRUPT | FLAG_UNUSED,
            sp: 0xfd,
            pc,
            halted: false,
        }
    }

    fn set_zn(&mut self, value: u8) {
        if value == 0 {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }
        if value & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }
    }

    fn push(&mut self, bus: &mut Bus, value: u8) {
        bus.write(0x0100 | u16::from(self.sp), value);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop(&mut self, bus: &mut Bus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | u16::from(self.sp))
    }

    fn adc(&mut self, value: u8) {
        let carry = u16::from((self.p & FLAG_CARRY != 0) as u8);
        let sum = u16::from(self.a) + u16::from(value) + carry;
        let result = sum as u8;
        if sum > 0xff {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        if (!(self.a ^ value) & (self.a ^ result) & 0x80) != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
        self.a = result;
        self.set_zn(self.a);
    }

    fn branch(&mut self, bus: &mut Bus, condition: bool) -> u8 {
        let offset = bus.read(self.pc) as i8;
        self.pc = self.pc.wrapping_add(1);
        if condition {
            self.pc = self.pc.wrapping_add_signed(i16::from(offset));
            3
        } else {
            2
        }
    }

    fn step(&mut self, bus: &mut Bus) -> Result<u8, CoreError> {
        if self.halted {
            return Ok(1);
        }
        let pc = self.pc;
        let opcode = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        let cycles = match opcode {
            0x00 => {
                self.halted = true;
                7
            }
            0x08 => {
                self.push(bus, self.p | FLAG_BREAK | FLAG_UNUSED);
                3
            }
            0x09 => {
                self.a |= bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.set_zn(self.a);
                2
            }
            0x0a => {
                let carry = self.a & 0x80 != 0;
                self.a <<= 1;
                if carry {
                    self.p |= FLAG_CARRY;
                } else {
                    self.p &= !FLAG_CARRY;
                }
                self.set_zn(self.a);
                2
            }
            0x10 => self.branch(bus, self.p & FLAG_NEGATIVE == 0),
            0x18 => {
                self.p &= !FLAG_CARRY;
                2
            }
            0x20 => {
                let target = bus.read_u16(self.pc);
                self.pc = self.pc.wrapping_add(2);
                let return_pc = self.pc.wrapping_sub(1);
                self.push(bus, (return_pc >> 8) as u8);
                self.push(bus, return_pc as u8);
                self.pc = target;
                6
            }
            0x24 => {
                let address = u16::from(bus.read(self.pc));
                let value = bus.read(address);
                self.pc = self.pc.wrapping_add(1);
                if self.a & value == 0 {
                    self.p |= FLAG_ZERO;
                } else {
                    self.p &= !FLAG_ZERO;
                }
                self.p = (self.p & !(FLAG_NEGATIVE | FLAG_OVERFLOW))
                    | (value & (FLAG_NEGATIVE | FLAG_OVERFLOW));
                3
            }
            0x29 => {
                self.a &= bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.set_zn(self.a);
                2
            }
            0x30 => self.branch(bus, self.p & FLAG_NEGATIVE != 0),
            0x38 => {
                self.p |= FLAG_CARRY;
                2
            }
            0x40 => {
                self.p = (self.pop(bus) | FLAG_UNUSED) & !FLAG_BREAK;
                let lo = u16::from(self.pop(bus));
                let hi = u16::from(self.pop(bus));
                self.pc = lo | (hi << 8);
                6
            }
            0x48 => {
                self.push(bus, self.a);
                3
            }
            0x4c => {
                self.pc = bus.read_u16(self.pc);
                3
            }
            0x60 => {
                let lo = u16::from(self.pop(bus));
                let hi = u16::from(self.pop(bus));
                self.pc = (lo | (hi << 8)).wrapping_add(1);
                6
            }
            0x68 => {
                self.a = self.pop(bus);
                self.set_zn(self.a);
                4
            }
            0x69 => {
                let value = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.adc(value);
                2
            }
            0x78 => {
                self.p |= FLAG_INTERRUPT;
                2
            }
            0x81 => {
                let zp = bus.read(self.pc).wrapping_add(self.x);
                self.pc = self.pc.wrapping_add(1);
                bus.write(u16::from(zp), self.a);
                6
            }
            0x85 => {
                let address = u16::from(bus.read(self.pc));
                self.pc = self.pc.wrapping_add(1);
                bus.write(address, self.a);
                3
            }
            0x88 => {
                self.y = self.y.wrapping_sub(1);
                self.set_zn(self.y);
                2
            }
            0x8a => {
                self.a = self.x;
                self.set_zn(self.a);
                2
            }
            0x8d => {
                let address = bus.read_u16(self.pc);
                self.pc = self.pc.wrapping_add(2);
                bus.write(address, self.a);
                4
            }
            0x98 => {
                self.a = self.y;
                self.set_zn(self.a);
                2
            }
            0x9a => {
                self.sp = self.x;
                2
            }
            0xa0 => {
                self.y = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.set_zn(self.y);
                2
            }
            0xa2 => {
                self.x = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.set_zn(self.x);
                2
            }
            0xa8 => {
                self.y = self.a;
                self.set_zn(self.y);
                2
            }
            0xa9 => {
                self.a = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.set_zn(self.a);
                2
            }
            0xaa => {
                self.x = self.a;
                self.set_zn(self.x);
                2
            }
            0xad => {
                let address = bus.read_u16(self.pc);
                self.pc = self.pc.wrapping_add(2);
                self.a = bus.read(address);
                self.set_zn(self.a);
                4
            }
            0xc8 => {
                self.y = self.y.wrapping_add(1);
                self.set_zn(self.y);
                2
            }
            0xca => {
                self.x = self.x.wrapping_sub(1);
                self.set_zn(self.x);
                2
            }
            0xd0 => self.branch(bus, self.p & FLAG_ZERO == 0),
            0xe8 => {
                self.x = self.x.wrapping_add(1);
                self.set_zn(self.x);
                2
            }
            0xea => 2,
            0xf0 => self.branch(bus, self.p & FLAG_ZERO != 0),
            _ => return Err(CoreError::UnsupportedCpuOpcode { pc, opcode }),
        };
        Ok(cycles)
    }
}

/// Native mapper-0 core with deterministic frame and handoff behavior.
#[derive(Debug, Clone)]
pub struct NesCore {
    bus: Bus,
    cpu: Cpu,
    ppu_dots: u32,
    frame_number: u64,
}

impl NesCore {
    /// Creates a core from a validated iNES image.
    #[must_use]
    pub fn new(rom: &RomImage) -> Self {
        let mut bus = Bus::new(rom);
        let cpu = Cpu::reset(&mut bus);
        Self {
            bus,
            cpu,
            ppu_dots: 0,
            frame_number: 0,
        }
    }

    /// Sets the current player-1 fixNES-format input byte.
    pub fn set_input(&mut self, buttons: u8) {
        self.bus.controller.set_buttons(buttons);
    }

    /// Reads CPU-visible internal RAM for deterministic fixture assertions.
    #[must_use]
    pub fn ram(&self) -> &[u8; CPU_RAM_BYTES] {
        &self.bus.ram
    }

    /// Current CPU program counter.
    #[must_use]
    pub const fn program_counter(&self) -> u16 {
        self.cpu.pc
    }

    /// Current frame number.
    #[must_use]
    pub const fn frame_number(&self) -> u64 {
        self.frame_number
    }

    /// Reports the intentionally incomplete surfaces.
    #[must_use]
    pub const fn gaps() -> [CoreGap; 2] {
        [CoreGap::PpuRendering, CoreGap::ApuWaveforms]
    }

    /// Advances until one NTSC PPU frame boundary, then captures audio.
    pub fn frame(&mut self) -> Result<CoreFrame, CoreError> {
        let target = NTSC_DOTS_PER_LINE * NTSC_LINES_PER_FRAME;
        let mut cpu_cycles = 0;
        while self.ppu_dots < target {
            let cycles = u32::from(self.cpu.step(&mut self.bus)?);
            cpu_cycles += cycles;
            self.ppu_dots = self.ppu_dots.saturating_add(cycles * 3);
            if self.cpu.halted {
                break;
            }
        }
        self.ppu_dots %= target;
        self.frame_number += 1;
        Ok(CoreFrame {
            frame_number: self.frame_number,
            cpu_cycles,
            audio: AudioFrame::silent(),
        })
    }
}
