//! Native deterministic NES core surface used by [`crate::runtime`].
//!
//! The implementation ports the highest-value PC fixNES contracts without
//! calling the C emulator: iNES validation and mapper-0 PRG mapping, the
//! reset-vector/6502 instruction and PPU-NMI path needed by synthetic fixtures, the
//! controller shift register and the `$4016`/`$4017` fixNES open-bus reads, a frame-sampled mapper-0 PPU picture, NTSC
//! frame timing, and the fixed 32 kHz/60 audio handoff size. Full mapper and
//! APU waveform behavior, plus exact mid-scanline PPU timing, remain typed
//! gaps in [`CoreGap`].

use core::fmt;

use crate::ppu::{Frame, Mirroring, Ppu};

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
/// OAM DMA length when the `$4014` write ends on an even CPU cycle.
pub const OAM_DMA_EVEN_CYCLES: u32 = 513;
/// OAM DMA length when the `$4014` write ends on an odd CPU cycle.
pub const OAM_DMA_ODD_CYCLES: u32 = 514;

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

        // Keep this arithmetic aligned with pc_nes_fixnes.c: ROMsize starts
        // after the header, so trainer bytes remain part of the clamp pool.
        // The pointer into the payload skips the trainer separately below.
        let mut rom_size = bytes.len() - INES_HEADER_BYTES;
        let declared_prg_len = usize::from(bytes[4]) * PRG_BANK_BYTES;
        let declared_chr_len = usize::from(bytes[5]) * CHR_BANK_BYTES;
        let mut prg_len = declared_prg_len.min(rom_size);
        rom_size -= prg_len;
        let mut chr_len = declared_chr_len;
        if prg_len == 0 {
            // Match pc_fixnes_init: a zero-PRG header treats all remaining
            // bytes as PRG and discards the declared CHR surface.
            prg_len = rom_size;
        } else {
            chr_len = chr_len.min(rom_size);
        }

        // fixNES advances the PRG pointer past the trainer after calculating
        // ROMsize.  Bound the resulting lengths against that actual payload
        // so malformed-but-clamped inputs cannot create an out-of-bounds
        // slice.  A zero-length PRG remains a typed parse error.
        let available = bytes.len().saturating_sub(data_offset);
        prg_len = prg_len.min(available);
        if prg_len == 0 {
            return Err(RomError::TruncatedData {
                offset: data_offset,
                required: 1,
                available,
            });
        }
        chr_len = chr_len.min(available - prg_len);

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
    /// The PPU renderer is frame-sampled; exact mid-scanline rendering is not
    /// ported. The bounded vblank NMI edge and CPU entry sequence are ported.
    PpuRendering,
    /// The APU emits a correctly sized silent handoff, not channel waveforms.
    ApuWaveforms,
    /// DMC sample DMA and its interaction with CPU bus ownership are not
    /// modeled.
    DmcDma,
    /// Unofficial or otherwise unported 6502 opcodes return a typed error.
    UnsupportedCpuOpcodes,
    /// Only mapper 0 is implemented by this native core.
    UnsupportedMappers,
    /// OAM DMA copies the page and charges the total stall, but does not yet
    /// expose fixNES's per-cycle CPU read/PPU-dot interleaving.
    OamDmaTiming,
}

/// Exact fixNES C symbols and behaviors still outside this candidate.
///
/// This is intentionally a source-surface ledger, not a claim of emulator
/// completeness. The names below are the remaining C entry points or helper
/// families that the mapper-0/native path does not implement.
pub const NOT_PORTED: &[&str] = &[
    "mapperInit / mapperInitNSF / mapperInitFDS (pc/lib/fixnes/mapper.c)",
    "mapperCycle / mapperReset and all pc/lib/fixnes/mapper/*.c bank-switch handlers",
    "apuCycle / apuUpdate / apuGetBuf / apuGetBufSize (pc/lib/fixnes/apu.c)",
    "apuSetReg00..apuSetReg17 channel sequencing and waveform synthesis",
    "apuGetReg15 channel/DMC IRQ status (pc/lib/fixnes/apu.c)",
    "cpuDoDMA / cpuDMATryHalt / cpuTryTakeover DMC path (pc/lib/fixnes/cpu.c)",
    "cpuSoftReset / ppuSoftReset / ppuReset soft-reset request sequencing",
    "cpuKIL, cpuSLO, cpuRLA, cpuSRE, cpuRRA, cpuDCP, cpuISC and other unofficial opcode handlers",
    "ppuCycle / ppuDrawDone scanline rendering and mapper-cycle timing (pc/lib/fixnes/ppu.c)",
    "cpuDoOAM_DMA per-cycle read/write/PPU-dot interleaving (pc/lib/fixnes/cpu.c)",
    "pc_fixnes_render_frame / fixnes_capture_audio host conversion and DMA (pc/src/pc_nes_fixnes.c)",
];

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
    /// CPU cycles spent in `$4014` OAM DMA stalls during this frame.
    pub oam_dma_cycles: u32,
    /// Audio handoff captured before the cadence reset.
    pub audio: AudioFrame,
    /// Indexed 256x240 picture produced at the frame boundary.
    pub picture: Frame,
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
    prg_ram: Vec<u8>,
    ppu: Ppu,
    apu_registers: [u8; 0x18],
    controller: Controller,
    open_bus: u8,
    oam_dma_pending: bool,
}

impl Bus {
    fn new(rom: &RomImage) -> Self {
        let mirroring = if rom.bytes[6] & 8 != 0 {
            Mirroring::FourScreen
        } else if rom.bytes[6] & 1 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };
        Self {
            ram: [0; CPU_RAM_BYTES],
            prg: rom.prg().to_vec(),
            prg_ram: vec![0; rom.prg_ram_size()],
            ppu: Ppu::new(rom.chr(), rom.has_chr_ram(), mirroring),
            apu_registers: [0; 0x18],
            controller: Controller::new(),
            open_bus: 0,
            oam_dma_pending: false,
        }
    }

    fn read(&mut self, address: u16) -> u8 {
        let value = match address {
            0x0000..=0x1fff => self.ram[usize::from(address) & 0x07ff],
            0x2000..=0x3fff => self.ppu.read_register(address),
            // No APU channel length counters or DMC state are modeled, so
            // the status shape for this subset is zero.  This is a status
            // read, not an echo of the last $4015 write.
            0x4015 => 0,
            // fixNES inputGetP1 returns the controller bit over the previous
            // bus value's upper bits: (memLastVal & ~0x1f) | (ret & 1).
            0x4016 => (self.open_bus & !0x1f) | (self.controller.read() & 1),
            // inputGetP2 has no player-2 state yet and returns only
            // (memLastVal & ~0x1f); it never advances the P1 shift position.
            0x4017 => self.open_bus & !0x1f,
            0x6000..=0x7fff => self
                .prg_ram
                .get((usize::from(address) - 0x6000) % self.prg_ram.len())
                .copied()
                .unwrap_or(0),
            0x8000..=0xffff => {
                let offset = (usize::from(address) - 0x8000) % self.prg.len();
                self.prg[offset]
            }
            _ => 0,
        };
        self.open_bus = value;
        value
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x1fff => self.ram[usize::from(address) & 0x07ff] = value,
            0x2000..=0x3fff => self.ppu.write_register(address, value),
            0x4000..=0x4017 => match address {
                0x4014 => self.oam_dma(value),
                0x4016 => self.controller.write_strobe(value),
                _ => self.apu_registers[usize::from(address - 0x4000)] = value,
            },
            0x6000..=0x7fff => {
                let index = (usize::from(address) - 0x6000) % self.prg_ram.len();
                if let Some(byte) = self.prg_ram.get_mut(index) {
                    *byte = value;
                }
            }
            0x8000..=0xffff => {}
            _ => {}
        }
        // memSet8 updates fixNES's memLastVal after the device write, including
        // a $4016 strobe or a $4014 DMA request.
        self.open_bus = value;
    }

    fn oam_dma(&mut self, page: u8) {
        let base = u16::from(page) << 8;
        for offset in 0..=u8::MAX {
            let value = self.read(base | u16::from(offset));
            self.ppu.write_register(0x2004, value);
        }
        self.oam_dma_pending = true;
    }

    fn take_oam_dma(&mut self) -> bool {
        core::mem::take(&mut self.oam_dma_pending)
    }

    fn read_u16(&mut self, address: u16) -> u16 {
        let lo = u16::from(self.read(address));
        let hi = u16::from(self.read(address.wrapping_add(1)));
        lo | (hi << 8)
    }

    fn read_u16_indirect(&mut self, address: u16) -> u16 {
        let lo = u16::from(self.read(address));
        // The NMOS 6502 wraps the high-byte read within the same page for
        // JMP ($xxFF); this is the indirect-address path used by fixNES.
        let high_address = (address & 0xff00) | address.wrapping_add(1) & 0x00ff;
        let hi = u16::from(self.read(high_address));
        lo | (hi << 8)
    }
}

const FLAG_CARRY: u8 = 0x01;
const FLAG_ZERO: u8 = 0x02;
const FLAG_INTERRUPT: u8 = 0x04;
const FLAG_DECIMAL: u8 = 0x08;
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
}

#[derive(Debug, Clone, Copy)]
enum AddressMode {
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    IndirectX,
    IndirectY,
}

#[derive(Debug, Clone, Copy)]
enum ReadOperation {
    Adc,
    And,
    Bit,
    Cmp,
    Cpx,
    Cpy,
    Eor,
    Lda,
    Ldx,
    Ldy,
    Ora,
    Sbc,
}

#[derive(Debug, Clone, Copy)]
enum RmwOperation {
    Asl,
    Lsr,
    Rol,
    Ror,
    Dec,
    Inc,
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

    fn service_nmi(&mut self, bus: &mut Bus) -> u8 {
        self.push(bus, (self.pc >> 8) as u8);
        self.push(bus, self.pc as u8);
        self.push(bus, (self.p & !FLAG_BREAK) | FLAG_UNUSED);
        self.p |= FLAG_INTERRUPT;
        self.pc = bus.read_u16(0xfffa);
        7
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

    fn fetch8(&mut self, bus: &mut Bus) -> u8 {
        let value = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    fn fetch16(&mut self, bus: &mut Bus) -> u16 {
        let lo = u16::from(self.fetch8(bus));
        let hi = u16::from(self.fetch8(bus));
        lo | (hi << 8)
    }

    fn address(&mut self, bus: &mut Bus, mode: AddressMode) -> (u16, bool) {
        match mode {
            AddressMode::Immediate => {
                let address = self.pc;
                self.pc = self.pc.wrapping_add(1);
                (address, false)
            }
            AddressMode::ZeroPage => (u16::from(self.fetch8(bus)), false),
            AddressMode::ZeroPageX => (u16::from(self.fetch8(bus).wrapping_add(self.x)), false),
            AddressMode::ZeroPageY => (u16::from(self.fetch8(bus).wrapping_add(self.y)), false),
            AddressMode::Absolute => (self.fetch16(bus), false),
            AddressMode::AbsoluteX => {
                let base = self.fetch16(bus);
                let address = base.wrapping_add(u16::from(self.x));
                (address, base & 0xff00 != address & 0xff00)
            }
            AddressMode::AbsoluteY => {
                let base = self.fetch16(bus);
                let address = base.wrapping_add(u16::from(self.y));
                (address, base & 0xff00 != address & 0xff00)
            }
            AddressMode::IndirectX => {
                let pointer = self.fetch8(bus).wrapping_add(self.x);
                let lo = u16::from(bus.read(u16::from(pointer)));
                let hi = u16::from(bus.read(u16::from(pointer.wrapping_add(1))));
                (lo | (hi << 8), false)
            }
            AddressMode::IndirectY => {
                let pointer = self.fetch8(bus);
                let lo = u16::from(bus.read(u16::from(pointer)));
                let hi = u16::from(bus.read(u16::from(pointer.wrapping_add(1))));
                let base = lo | (hi << 8);
                let address = base.wrapping_add(u16::from(self.y));
                (address, base & 0xff00 != address & 0xff00)
            }
        }
    }

    fn read_operand(&mut self, bus: &mut Bus, mode: AddressMode) -> (u8, bool) {
        let (address, page_crossed) = self.address(bus, mode);
        (bus.read(address), page_crossed)
    }

    fn read_operation(&mut self, value: u8, operation: ReadOperation) {
        match operation {
            ReadOperation::Adc => self.adc(value),
            ReadOperation::And => {
                self.a &= value;
                self.set_zn(self.a);
            }
            ReadOperation::Bit => {
                if self.a & value == 0 {
                    self.p |= FLAG_ZERO;
                } else {
                    self.p &= !FLAG_ZERO;
                }
                self.p = (self.p & !(FLAG_NEGATIVE | FLAG_OVERFLOW))
                    | (value & (FLAG_NEGATIVE | FLAG_OVERFLOW));
            }
            ReadOperation::Cmp => self.compare(self.a, value),
            ReadOperation::Cpx => self.compare(self.x, value),
            ReadOperation::Cpy => self.compare(self.y, value),
            ReadOperation::Eor => {
                self.a ^= value;
                self.set_zn(self.a);
            }
            ReadOperation::Lda => {
                self.a = value;
                self.set_zn(self.a);
            }
            ReadOperation::Ldx => {
                self.x = value;
                self.set_zn(self.x);
            }
            ReadOperation::Ldy => {
                self.y = value;
                self.set_zn(self.y);
            }
            ReadOperation::Ora => {
                self.a |= value;
                self.set_zn(self.a);
            }
            ReadOperation::Sbc => self.adc(!value),
        }
    }

    fn compare(&mut self, register: u8, value: u8) {
        if register >= value {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        self.set_zn(register.wrapping_sub(value));
    }

    fn rmw_value(&mut self, value: u8, operation: RmwOperation) -> u8 {
        match operation {
            RmwOperation::Asl => {
                if value & 0x80 != 0 {
                    self.p |= FLAG_CARRY;
                } else {
                    self.p &= !FLAG_CARRY;
                }
                let result = value << 1;
                self.set_zn(result);
                result
            }
            RmwOperation::Lsr => {
                if value & 1 != 0 {
                    self.p |= FLAG_CARRY;
                } else {
                    self.p &= !FLAG_CARRY;
                }
                let result = value >> 1;
                self.set_zn(result);
                result
            }
            RmwOperation::Rol => {
                let carry = self.p & FLAG_CARRY != 0;
                if value & 0x80 != 0 {
                    self.p |= FLAG_CARRY;
                } else {
                    self.p &= !FLAG_CARRY;
                }
                let result = (value << 1) | u8::from(carry);
                self.set_zn(result);
                result
            }
            RmwOperation::Ror => {
                let carry = self.p & FLAG_CARRY != 0;
                if value & 1 != 0 {
                    self.p |= FLAG_CARRY;
                } else {
                    self.p &= !FLAG_CARRY;
                }
                let result = (value >> 1) | if carry { 0x80 } else { 0 };
                self.set_zn(result);
                result
            }
            RmwOperation::Dec => {
                let result = value.wrapping_sub(1);
                self.set_zn(result);
                result
            }
            RmwOperation::Inc => {
                let result = value.wrapping_add(1);
                self.set_zn(result);
                result
            }
        }
    }

    fn branch(&mut self, bus: &mut Bus, condition: bool) -> u8 {
        let offset = self.fetch8(bus) as i8;
        if condition {
            let old_page = self.pc & 0xff00;
            self.pc = self.pc.wrapping_add_signed(i16::from(offset));
            if old_page != self.pc & 0xff00 { 4 } else { 3 }
        } else {
            2
        }
    }

    fn step(&mut self, bus: &mut Bus) -> Result<u8, CoreError> {
        let pc = self.pc;
        let opcode = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        let cycles = match opcode {
            0x00 => {
                self.pc = self.pc.wrapping_add(1);
                self.push(bus, (self.pc >> 8) as u8);
                self.push(bus, self.pc as u8);
                self.push(bus, self.p | FLAG_BREAK | FLAG_UNUSED);
                self.p |= FLAG_INTERRUPT;
                self.pc = bus.read_u16(0xfffe);
                7
            }
            0x08 => {
                self.push(bus, self.p | FLAG_BREAK | FLAG_UNUSED);
                3
            }
            0x10 => self.branch(bus, self.p & FLAG_NEGATIVE == 0),
            0x18 => {
                self.p &= !FLAG_CARRY;
                2
            }
            0x90 => self.branch(bus, self.p & FLAG_CARRY == 0),
            0x20 => {
                let target = self.fetch16(bus);
                let return_pc = self.pc.wrapping_sub(1);
                self.push(bus, (return_pc >> 8) as u8);
                self.push(bus, return_pc as u8);
                self.pc = target;
                6
            }
            0x28 => {
                self.p = (self.pop(bus) | FLAG_UNUSED) & !FLAG_BREAK;
                4
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
                self.pc = self.fetch16(bus);
                3
            }
            0x6c => {
                let pointer = self.fetch16(bus);
                self.pc = bus.read_u16_indirect(pointer);
                5
            }
            0x50 => self.branch(bus, self.p & FLAG_OVERFLOW == 0),
            0x58 => {
                self.p &= !FLAG_INTERRUPT;
                2
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
            0x70 => self.branch(bus, self.p & FLAG_OVERFLOW != 0),
            0x78 => {
                self.p |= FLAG_INTERRUPT;
                2
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
            0x98 => {
                self.a = self.y;
                self.set_zn(self.a);
                2
            }
            0x9a => {
                self.sp = self.x;
                2
            }
            0xa8 => {
                self.y = self.a;
                self.set_zn(self.y);
                2
            }
            0xaa => {
                self.x = self.a;
                self.set_zn(self.x);
                2
            }
            0xb0 => self.branch(bus, self.p & FLAG_CARRY != 0),
            0xb8 => {
                self.p &= !FLAG_OVERFLOW;
                2
            }
            0xba => {
                self.x = self.sp;
                self.set_zn(self.x);
                2
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
            0xd8 => {
                self.p &= !FLAG_DECIMAL;
                2
            }
            0xe8 => {
                self.x = self.x.wrapping_add(1);
                self.set_zn(self.x);
                2
            }
            // fixNES maps these six unofficial one-byte NOPs to its direct
            // read path, which consumes two cycles and performs no action.
            0x1a | 0x3a | 0x5a | 0x7a | 0xda | 0xea | 0xfa => 2,
            0xf8 => {
                self.p |= FLAG_DECIMAL;
                2
            }
            0xf0 => self.branch(bus, self.p & FLAG_ZERO != 0),
            0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => {
                self.read_group(bus, ReadOperation::Ora, opcode)?
            }
            0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                self.read_group(bus, ReadOperation::And, opcode)?
            }
            0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
                self.read_group(bus, ReadOperation::Eor, opcode)?
            }
            0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
                self.read_group(bus, ReadOperation::Adc, opcode)?
            }
            0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => {
                self.read_group(bus, ReadOperation::Sbc, opcode)?
            }
            0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                self.read_group(bus, ReadOperation::Cmp, opcode)?
            }
            0x24 | 0x2c => self.read_group(bus, ReadOperation::Bit, opcode)?,
            0xe0 | 0xe4 | 0xec => self.read_group(bus, ReadOperation::Cpx, opcode)?,
            0xc0 | 0xc4 | 0xcc => self.read_group(bus, ReadOperation::Cpy, opcode)?,
            0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                self.read_group(bus, ReadOperation::Lda, opcode)?
            }
            0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => self.read_group(bus, ReadOperation::Ldx, opcode)?,
            0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => self.read_group(bus, ReadOperation::Ldy, opcode)?,
            0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                self.write_group(bus, opcode, self.a)?
            }
            0x86 | 0x96 | 0x8e => self.write_group(bus, opcode, self.x)?,
            0x84 | 0x94 | 0x8c => self.write_group(bus, opcode, self.y)?,
            0x0a => {
                self.a = self.rmw_value(self.a, RmwOperation::Asl);
                2
            }
            0x06 | 0x16 | 0x0e | 0x1e => self.rmw_group(bus, opcode, RmwOperation::Asl)?,
            0x4a => {
                self.a = self.rmw_value(self.a, RmwOperation::Lsr);
                2
            }
            0x46 | 0x56 | 0x4e | 0x5e => self.rmw_group(bus, opcode, RmwOperation::Lsr)?,
            0x2a => {
                self.a = self.rmw_value(self.a, RmwOperation::Rol);
                2
            }
            0x26 | 0x36 | 0x2e | 0x3e => self.rmw_group(bus, opcode, RmwOperation::Rol)?,
            0x6a => {
                self.a = self.rmw_value(self.a, RmwOperation::Ror);
                2
            }
            0x66 | 0x76 | 0x6e | 0x7e => self.rmw_group(bus, opcode, RmwOperation::Ror)?,
            0xc6 | 0xd6 | 0xce | 0xde => self.rmw_group(bus, opcode, RmwOperation::Dec)?,
            0xe6 | 0xf6 | 0xee | 0xfe => self.rmw_group(bus, opcode, RmwOperation::Inc)?,
            0x80 => return Err(CoreError::UnsupportedCpuOpcode { pc, opcode }),
            _ => return Err(CoreError::UnsupportedCpuOpcode { pc, opcode }),
        };
        Ok(cycles)
    }

    fn read_group(
        &mut self,
        bus: &mut Bus,
        operation: ReadOperation,
        opcode: u8,
    ) -> Result<u8, CoreError> {
        let (mode, cycles, page_penalty) = match opcode {
            0x24 => (AddressMode::ZeroPage, 3, false),
            0x2c => (AddressMode::Absolute, 4, false),
            0x01 | 0x21 | 0x41 | 0x61 | 0xa1 | 0xc1 | 0xe1 => (AddressMode::IndirectX, 6, false),
            0x11 | 0x31 | 0x51 | 0x71 | 0xb1 | 0xd1 | 0xf1 => (AddressMode::IndirectY, 5, true),
            0x05 | 0x25 | 0x45 | 0x65 | 0xa5 | 0xc5 | 0xe5 => (AddressMode::ZeroPage, 3, false),
            0x15 | 0x35 | 0x55 | 0x75 | 0xb5 | 0xd5 | 0xf5 => (AddressMode::ZeroPageX, 4, false),
            0x0d | 0x2d | 0x4d | 0x6d | 0xad | 0xcd | 0xed => (AddressMode::Absolute, 4, false),
            0x1d | 0x3d | 0x5d | 0x7d | 0xbd | 0xdd | 0xfd => (AddressMode::AbsoluteX, 4, true),
            0x19 | 0x39 | 0x59 | 0x79 | 0xb9 | 0xd9 | 0xf9 => (AddressMode::AbsoluteY, 4, true),
            0x09 | 0x29 | 0x49 | 0x69 | 0xa9 | 0xc9 | 0xe9 => (AddressMode::Immediate, 2, false),
            0xe0 | 0xc0 => (AddressMode::Immediate, 2, false),
            0xe4 | 0xc4 => (AddressMode::ZeroPage, 3, false),
            0xec | 0xcc => (AddressMode::Absolute, 4, false),
            0xa2 => (AddressMode::Immediate, 2, false),
            0xa6 => (AddressMode::ZeroPage, 3, false),
            0xb6 => (AddressMode::ZeroPageY, 4, false),
            0xae => (AddressMode::Absolute, 4, false),
            0xbe => (AddressMode::AbsoluteY, 4, true),
            0xa0 => (AddressMode::Immediate, 2, false),
            0xa4 => (AddressMode::ZeroPage, 3, false),
            0xb4 => (AddressMode::ZeroPageX, 4, false),
            0xac => (AddressMode::Absolute, 4, false),
            0xbc => (AddressMode::AbsoluteX, 4, true),
            _ => {
                return Err(CoreError::UnsupportedCpuOpcode {
                    pc: self.pc.wrapping_sub(1),
                    opcode,
                });
            }
        };
        let (value, crossed) = self.read_operand(bus, mode);
        self.read_operation(value, operation);
        Ok(cycles + u8::from(page_penalty && crossed))
    }

    fn write_group(&mut self, bus: &mut Bus, opcode: u8, value: u8) -> Result<u8, CoreError> {
        let (mode, cycles) = match opcode {
            0x84..=0x86 => (AddressMode::ZeroPage, 3),
            0x95 | 0x94 => (AddressMode::ZeroPageX, 4),
            0x96 => (AddressMode::ZeroPageY, 4),
            0x8c..=0x8e => (AddressMode::Absolute, 4),
            0x9d => (AddressMode::AbsoluteX, 5),
            0x99 => (AddressMode::AbsoluteY, 5),
            0x81 => (AddressMode::IndirectX, 6),
            0x91 => (AddressMode::IndirectY, 6),
            _ => {
                return Err(CoreError::UnsupportedCpuOpcode {
                    pc: self.pc.wrapping_sub(1),
                    opcode,
                });
            }
        };
        let (address, _) = self.address(bus, mode);
        bus.write(address, value);
        Ok(cycles)
    }

    fn rmw_group(
        &mut self,
        bus: &mut Bus,
        opcode: u8,
        operation: RmwOperation,
    ) -> Result<u8, CoreError> {
        let (mode, cycles) = match opcode {
            0x06 | 0x46 | 0x66 | 0xc6 | 0xe6 => (AddressMode::ZeroPage, 5),
            0x16 | 0x56 | 0x76 | 0xd6 | 0xf6 => (AddressMode::ZeroPageX, 6),
            0x0e | 0x4e | 0x6e | 0xce | 0xee => (AddressMode::Absolute, 6),
            0x1e | 0x5e | 0x7e | 0xde | 0xfe => (AddressMode::AbsoluteX, 7),
            _ => {
                return Err(CoreError::UnsupportedCpuOpcode {
                    pc: self.pc.wrapping_sub(1),
                    opcode,
                });
            }
        };
        let (address, _) = self.address(bus, mode);
        let value = bus.read(address);
        bus.write(address, self.rmw_value(value, operation));
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
    cpu_odd_cycle: bool,
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
            cpu_odd_cycle: false,
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

    /// Reads the mapper-0 PRG RAM backing store.
    #[must_use]
    pub fn prg_ram(&self) -> &[u8] {
        &self.bus.prg_ram
    }

    /// Reads one of the eight CPU-visible PPU register mirrors.
    ///
    /// Rendering, VRAM address latches, and status side effects remain an
    /// explicit [`CoreGap::PpuRendering`] surface; this accessor only exposes
    /// the deterministic register bus boundary exercised by synthetic tests.
    #[must_use]
    pub fn ppu_register(&self, index: usize) -> Option<u8> {
        self.bus.ppu.register(index)
    }

    /// Returns the current PPU VRAM address.
    #[must_use]
    pub fn ppu_vram_address(&self) -> u16 {
        self.bus.ppu.vram_address()
    }

    /// Reads an effective palette entry for synthetic PPU fixtures.
    #[must_use]
    pub fn ppu_palette_entry(&self, index: usize) -> u8 {
        self.bus.ppu.palette_entry(index)
    }

    /// Reads a mapped nametable byte for synthetic fixture inspection.
    #[must_use]
    pub fn ppu_nametable_entry(&self, address: u16) -> u8 {
        self.bus.ppu.nametable_entry(address)
    }

    /// Reads OAM for synthetic sprite-fixture inspection.
    #[must_use]
    pub fn ppu_oam(&self) -> &[u8; 0x100] {
        self.bus.ppu.oam()
    }

    /// Returns the most recently rendered picture, by rendering the current
    /// surfaces without advancing the CPU. This is mainly useful for fixtures.
    #[must_use]
    pub fn render(&mut self) -> Frame {
        self.bus.ppu.render()
    }

    /// Reads one of the CPU-visible APU register bytes.
    ///
    /// Channel synthesis and frame sequencing remain an explicit
    /// [`CoreGap::ApuWaveforms`] surface.
    #[must_use]
    pub fn apu_register(&self, index: usize) -> Option<u8> {
        self.bus.apu_registers.get(index).copied()
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
    pub const fn gaps() -> [CoreGap; 6] {
        [
            CoreGap::PpuRendering,
            CoreGap::ApuWaveforms,
            CoreGap::DmcDma,
            CoreGap::UnsupportedCpuOpcodes,
            CoreGap::UnsupportedMappers,
            CoreGap::OamDmaTiming,
        ]
    }

    /// Advances until one NTSC PPU frame boundary, then captures audio.
    pub fn frame(&mut self) -> Result<CoreFrame, CoreError> {
        let target = NTSC_DOTS_PER_LINE * NTSC_LINES_PER_FRAME;
        let mut cpu_cycles = 0;
        let mut oam_dma_cycles = 0;
        while self.ppu_dots < target {
            let mut cycles = if self.bus.ppu.poll_nmi() {
                u32::from(self.cpu.service_nmi(&mut self.bus))
            } else {
                u32::from(self.cpu.step(&mut self.bus)?)
            };
            self.cpu_odd_cycle ^= cycles & 1 != 0;
            let ppu_dots = cycles * 3;
            self.ppu_dots = self.ppu_dots.saturating_add(ppu_dots);
            self.bus.ppu.add_dots(ppu_dots);

            if self.bus.take_oam_dma() {
                let dma_cycles = if self.cpu_odd_cycle {
                    OAM_DMA_ODD_CYCLES
                } else {
                    OAM_DMA_EVEN_CYCLES
                };
                self.cpu_odd_cycle ^= dma_cycles & 1 != 0;
                cycles += dma_cycles;
                oam_dma_cycles += dma_cycles;
                let dma_dots = dma_cycles * 3;
                self.ppu_dots = self.ppu_dots.saturating_add(dma_dots);
                self.bus.ppu.add_dots(dma_dots);
            }
            cpu_cycles += cycles;
        }
        self.ppu_dots %= target;
        self.frame_number += 1;
        let picture = self.bus.ppu.render();
        Ok(CoreFrame {
            frame_number: self.frame_number,
            cpu_cycles,
            oam_dma_cycles,
            audio: AudioFrame::silent(),
            picture,
        })
    }
}
