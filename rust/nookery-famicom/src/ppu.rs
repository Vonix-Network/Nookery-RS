//! Mapper-0 PPU bus and deterministic indexed picture renderer.
//!
//! The register and memory rules in this module follow the corresponding
//! fixNES paths in `pc/lib/fixnes/ppu.c`: register writes (`1699..1805`),
//! register reads (`1807..1856`), nametable mapping (`1907..1943`), palette
//! mirroring (`1980..2001`), and the bounded `ppuNMI` latch (`1859..1877`).
//! Rendering is intentionally sampled at the frame boundary; mid-scanline
//! rendering remains outside this candidate.

/// Visible NES picture width.
pub const FRAME_WIDTH: usize = 256;
/// Visible NES picture height.
pub const FRAME_HEIGHT: usize = 240;
const FRAME_PIXELS: usize = FRAME_WIDTH * FRAME_HEIGHT;

const CTRL_INCREMENT: u8 = 1 << 2;
const CTRL_SPRITE_TABLE: u8 = 1 << 3;
const CTRL_BACKGROUND_TABLE: u8 = 1 << 4;
const CTRL_SPRITE_8X16: u8 = 1 << 5;
const MASK_GREYSCALE: u8 = 1;
const MASK_BG_LEFT: u8 = 1 << 1;
const MASK_SPRITE_LEFT: u8 = 1 << 2;
const MASK_BG_ENABLE: u8 = 1 << 3;
const MASK_SPRITE_ENABLE: u8 = 1 << 4;
const STATUS_OVERFLOW: u8 = 1 << 5;
const STATUS_SPRITE_ZERO: u8 = 1 << 6;
const STATUS_VBLANK: u8 = 1 << 7;
const SPRITE_PRIORITY: u8 = 1 << 5;
const SPRITE_FLIP_X: u8 = 1 << 6;
const SPRITE_FLIP_Y: u8 = 1 << 7;

/// Cartridge nametable wiring used by the mapper-0 board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mirroring {
    /// A and B share the first physical nametable, C and D share the second.
    Horizontal,
    /// A and C share the first physical nametable, B and D share the second.
    Vertical,
    /// All four nametables have independent 1 KiB storage.
    FourScreen,
}

/// The stable picture returned by [`Ppu::render`].  Pixels are one byte each,
/// row-major, and contain the effective NES palette entry `0..=63` after the
/// grayscale mask.  The palette RGB conversion used by fixNES is deliberately
/// left to the host; this makes hashes independent of host pixel endianness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Picture width, always [`FRAME_WIDTH`].
    pub width: usize,
    /// Picture height, always [`FRAME_HEIGHT`].
    pub height: usize,
    /// Row-major indexed pixels, length `width * height`.
    pub pixels: Vec<u8>,
}

impl Frame {
    /// FNV-1a hash of dimensions followed by row-major pixels.
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in (self.width as u64)
            .to_le_bytes()
            .into_iter()
            .chain((self.height as u64).to_le_bytes())
            .chain(self.pixels.iter().copied())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}

/// Stateful mapper-0 PPU surface.
#[derive(Debug, Clone)]
pub struct Ppu {
    chr: [u8; 0x2000],
    chr_writable: bool,
    nametable: [u8; 0x1000],
    palette: [u8; 0x20],
    oam: [u8; 0x100],
    regs: [u8; 8],
    v: u16,
    t: u16,
    fine_x: u8,
    w: bool,
    read_buffer: u8,
    oam_addr: u8,
    last_value: u8,
    mirroring: Mirroring,
    dot: u16,
    line: u16,
    nmi_allowed: bool,
    nmi_triggered: bool,
    nmi_pending: bool,
    vblank_read: bool,
}

impl Ppu {
    /// Creates a PPU with the cartridge CHR surface and initial mirroring.
    #[must_use]
    pub fn new(chr: &[u8], chr_writable: bool, mirroring: Mirroring) -> Self {
        let mut chr_surface = [0; 0x2000];
        let copy_len = chr.len().min(chr_surface.len());
        chr_surface[..copy_len].copy_from_slice(&chr[..copy_len]);
        let mut ppu = Self {
            chr: chr_surface,
            chr_writable,
            nametable: [0; 0x1000],
            palette: [0; 0x20],
            oam: [0; 0x100],
            regs: [0; 8],
            v: 0,
            t: 0,
            fine_x: 0,
            w: false,
            read_buffer: 0,
            oam_addr: 0,
            last_value: 0,
            mirroring,
            dot: 0,
            line: 0,
            nmi_allowed: false,
            nmi_triggered: false,
            nmi_pending: false,
            vblank_read: false,
        };
        // fixNES ppuInit starts with the vblank status bit set (ppu.c:292).
        ppu.regs[2] = STATUS_VBLANK;
        ppu
    }

    /// Current register value for diagnostic assertions.
    #[must_use]
    pub fn register(&self, index: usize) -> Option<u8> {
        self.regs.get(index).copied()
    }

    /// Current VRAM address, useful for bus-side tests.
    #[must_use]
    pub const fn vram_address(&self) -> u16 {
        self.v
    }

    /// Current OAM bytes, exposed read-only for deterministic fixtures.
    #[must_use]
    pub const fn oam(&self) -> &[u8; 0x100] {
        &self.oam
    }

    /// Returns an effective palette entry for deterministic fixture inspection.
    #[must_use]
    pub fn palette_entry(&self, index: usize) -> u8 {
        self.palette_value(index)
    }

    /// Reads a mapped nametable byte for synthetic fixture inspection.
    #[must_use]
    pub fn nametable_entry(&self, address: u16) -> u8 {
        self.nametable[self.map_nametable(address & 0x0fff)]
    }

    /// Advances the visible timing state by PPU dots.
    pub fn add_dots(&mut self, dots: u32) {
        for _ in 0..dots {
            if self.line == 241 && self.dot == 0 {
                self.vblank_read = false;
            }
            if self.line == 241 && self.dot == 2 {
                self.nmi_triggered = false;
                if !self.vblank_read {
                    self.regs[2] |= STATUS_VBLANK;
                }
                self.vblank_read = false;
            }
            if self.line == 241 && self.dot == 4 && self.regs[2] & STATUS_VBLANK != 0 {
                self.nmi_allowed = true;
                self.update_nmi_pending();
            }
            if self.line == 261 && self.dot == 2 {
                self.regs[2] &= !(STATUS_VBLANK | STATUS_SPRITE_ZERO | STATUS_OVERFLOW);
                self.nmi_allowed = false;
                self.nmi_pending = false;
                self.vblank_read = false;
            }
            self.dot += 1;
            if self.dot == 341 {
                self.dot = 0;
                self.line += 1;
                if self.line == 262 {
                    self.line = 0;
                }
            }
        }
    }

    /// Reads one CPU-visible PPU register, including latch and buffer effects.
    pub fn read_register(&mut self, address: u16) -> u8 {
        let value = match address & 7 {
            2 => {
                let value = self.regs[2];
                self.regs[2] &= !STATUS_VBLANK;
                self.w = false;
                if value & STATUS_VBLANK != 0 {
                    self.vblank_read = true;
                    self.nmi_allowed = false;
                    self.nmi_pending = false;
                }
                value
            }
            4 => self.oam[usize::from(self.oam_addr)],
            7 => {
                let address = self.v & 0x3fff;
                let value = if address >= 0x3f00 {
                    let value = self.ppu_read(address);
                    self.read_buffer = self.ppu_read(address & 0x2fff);
                    value
                } else {
                    let value = self.read_buffer;
                    self.read_buffer = self.ppu_read(address);
                    value
                };
                self.increment_vram_address();
                value
            }
            _ => self.last_value,
        };
        self.last_value = value;
        value
    }

    /// Writes one CPU-visible PPU register, matching fixNES register decode.
    pub fn write_register(&mut self, address: u16, value: u8) {
        self.last_value = value;
        match address & 7 {
            0 => {
                self.regs[0] = value;
                self.t = (self.t & !0x0c00) | (u16::from(value & 3) << 10);
                self.update_nmi_pending();
            }
            1 => self.regs[1] = value,
            3 => self.oam_addr = value,
            4 => {
                let index = usize::from(self.oam_addr);
                self.oam[index] = if index & 3 == 2 { value & 0xe3 } else { value };
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            5 => {
                if !self.w {
                    self.w = true;
                    self.fine_x = value & 7;
                    self.t = (self.t & !0x001f) | u16::from(value >> 3 & 0x1f);
                } else {
                    self.w = false;
                    self.t = (self.t & !0x73e0)
                        | (u16::from(value & 7) << 12)
                        | (u16::from(value >> 3) << 5);
                }
            }
            6 => {
                if !self.w {
                    self.w = true;
                    self.t = (self.t & 0x00ff) | (u16::from(value & 0x3f) << 8);
                } else {
                    self.w = false;
                    self.t = (self.t & 0xff00) | u16::from(value);
                    self.v = self.t;
                }
            }
            7 => {
                self.ppu_write(self.v & 0x3fff, value);
                self.increment_vram_address();
            }
            _ => {}
        }
    }

    fn update_nmi_pending(&mut self) {
        if self.nmi_allowed
            && self.regs[0] & (1 << 7) != 0
            && self.regs[2] & STATUS_VBLANK != 0
            && !self.nmi_triggered
        {
            self.nmi_pending = true;
            self.nmi_triggered = true;
        }
    }

    /// Consumes one edge-triggered NMI request from the C `ppuNMI` contract.
    pub fn poll_nmi(&mut self) -> bool {
        let pending = self.nmi_pending;
        self.nmi_pending = false;
        pending
    }

    fn increment_vram_address(&mut self) {
        self.v = self.v.wrapping_add(if self.regs[0] & CTRL_INCREMENT != 0 {
            32
        } else {
            1
        }) & 0x7fff;
    }

    fn map_nametable(&self, address: u16) -> usize {
        let table = usize::from((address >> 10) & 3);
        let offset = usize::from(address & 0x03ff);
        let physical = match self.mirroring {
            Mirroring::Horizontal => [0, 0, 1, 1][table],
            Mirroring::Vertical => [0, 1, 0, 1][table],
            Mirroring::FourScreen => table,
        };
        physical * 0x400 + offset
    }

    fn ppu_read(&self, address: u16) -> u8 {
        let address = address & 0x3fff;
        match address {
            0x0000..=0x1fff => self.chr[usize::from(address)],
            0x2000..=0x3eff => self.nametable[self.map_nametable(address - 0x2000)],
            _ => self.palette[palette_index(address)],
        }
    }

    fn ppu_write(&mut self, address: u16, value: u8) {
        let address = address & 0x3fff;
        match address {
            0x0000..=0x1fff if self.chr_writable => self.chr[usize::from(address)] = value,
            0x2000..=0x3eff => {
                let index = self.map_nametable(address - 0x2000);
                self.nametable[index] = value;
            }
            0x3f00..=0x3fff => {
                let index = palette_index(address);
                self.palette[index] = value;
                if index & 3 == 0 {
                    self.palette[index ^ 0x10] = value;
                }
            }
            _ => {}
        }
    }

    fn palette_value(&self, address: usize) -> u8 {
        self.palette[address & 0x1f]
            & if self.regs[1] & MASK_GREYSCALE != 0 {
                0x30
            } else {
                0x3f
            }
    }

    /// Renders the current PPU surfaces to the stable indexed frame format.
    #[must_use]
    pub fn render(&mut self) -> Frame {
        let mut pixels = vec![0; FRAME_PIXELS];
        let bg_enabled = self.regs[1] & MASK_BG_ENABLE != 0;
        let sprites_enabled = self.regs[1] & MASK_SPRITE_ENABLE != 0;
        let bg_left = self.regs[1] & MASK_BG_LEFT != 0;
        let sprite_left = self.regs[1] & MASK_SPRITE_LEFT != 0;
        let bg_base = if self.regs[0] & CTRL_BACKGROUND_TABLE != 0 {
            0x1000
        } else {
            0
        };
        let sprite_height = if self.regs[0] & CTRL_SPRITE_8X16 != 0 {
            16
        } else {
            8
        };
        self.regs[2] &= !(STATUS_SPRITE_ZERO | STATUS_OVERFLOW);

        for y in 0..FRAME_HEIGHT {
            let mut sprites = Vec::with_capacity(8);
            if sprites_enabled {
                for index in 0..64 {
                    let oam = index * 4;
                    let sprite_y = usize::from(self.oam[oam]);
                    if y >= sprite_y && y < sprite_y + sprite_height {
                        if sprites.len() < 8 {
                            sprites.push(index);
                        } else {
                            self.regs[2] |= STATUS_OVERFLOW;
                        }
                    }
                }
            }
            for x in 0..FRAME_WIDTH {
                let (bg_color, bg_opaque) = if bg_enabled && (x >= 8 || bg_left) {
                    let scroll_x = usize::from(self.t & 0x1f) * 8 + usize::from(self.fine_x);
                    let scroll_y =
                        usize::from((self.t >> 5) & 0x1f) * 8 + usize::from((self.t >> 12) & 7);
                    let world_x = scroll_x + x;
                    let world_y = scroll_y + y;
                    let table_x = usize::from((self.t >> 10) & 1) ^ ((world_x / 256) & 1);
                    let table_y = usize::from((self.t >> 11) & 1) ^ ((world_y / 240) & 1);
                    let table = table_x | (table_y << 1);
                    let tile_x = (world_x / 8) % 32;
                    let tile_y = (world_y / 8) % 30;
                    let nt = self.nametable[self.map_nametable(
                        (u16::from(table as u8) << 10) | ((tile_y * 32 + tile_x) as u16),
                    )];
                    let attr_address = (u16::from(table as u8) << 10)
                        | 0x03c0
                        | ((tile_y / 4 * 8 + tile_x / 4) as u16);
                    let attr = self.nametable[self.map_nametable(attr_address)];
                    let shift = ((tile_y & 2) << 1) | (tile_x & 2);
                    let palette_number = (attr >> shift) & 3;
                    let row = world_y & 7;
                    let plane0 =
                        self.chr[usize::from((bg_base + u16::from(nt) * 16 + row as u16) & 0x1fff)];
                    let plane1 = self.chr
                        [usize::from((bg_base + u16::from(nt) * 16 + row as u16 + 8) & 0x1fff)];
                    let bit = 7 - (world_x & 7);
                    let color = ((plane0 >> bit) & 1) | (((plane1 >> bit) & 1) << 1);
                    // fixNES loadTiles/TILELUT emits zero for color 0, so
                    // transparent background pixels always use universal
                    // entry 0 rather than the attribute palette's entry 4.
                    let address = if color == 0 {
                        0
                    } else {
                        usize::from(palette_number) * 4 + usize::from(color)
                    };
                    (self.palette_value(address), color != 0)
                } else {
                    (self.palette_value(0), false)
                };
                let mut color = bg_color;
                if sprites_enabled && (x >= 8 || sprite_left) {
                    for &sprite_index in &sprites {
                        let oam = sprite_index * 4;
                        let sprite_x = usize::from(self.oam[oam + 3]);
                        if x < sprite_x || x >= sprite_x + 8 {
                            continue;
                        }
                        let attributes = self.oam[oam + 2];
                        let mut row = y - usize::from(self.oam[oam]);
                        if attributes & SPRITE_FLIP_Y != 0 {
                            row = sprite_height - 1 - row;
                        }
                        let mut tile = self.oam[oam + 1];
                        let base;
                        if sprite_height == 16 {
                            base = if tile & 1 != 0 { 0x1000 } else { 0 };
                            tile &= 0xfe;
                            if row >= 8 {
                                tile = tile.wrapping_add(1);
                                row -= 8;
                            }
                        } else {
                            base = if self.regs[0] & CTRL_SPRITE_TABLE != 0 {
                                0x1000
                            } else {
                                0
                            };
                        }
                        let column = if attributes & SPRITE_FLIP_X != 0 {
                            x - sprite_x
                        } else {
                            7 - (x - sprite_x)
                        };
                        let tile_address = base + usize::from(tile) * 16 + row;
                        let low = self.chr[tile_address & 0x1fff];
                        let high = self.chr[(tile_address + 8) & 0x1fff];
                        let bit = column;
                        let sprite_color = ((low >> bit) & 1) | (((high >> bit) & 1) << 1);
                        if sprite_color == 0 {
                            continue;
                        }
                        if sprite_index == 0 && bg_opaque && x < 255 {
                            self.regs[2] |= STATUS_SPRITE_ZERO;
                        }
                        if !bg_opaque || attributes & SPRITE_PRIORITY == 0 {
                            color = self.palette_value(
                                0x10 + usize::from(attributes & 3) * 4 + usize::from(sprite_color),
                            );
                        }
                        break;
                    }
                }
                pixels[y * FRAME_WIDTH + x] = color;
            }
        }
        Frame {
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            pixels,
        }
    }
}

fn palette_index(address: u16) -> usize {
    // fixNES ppuGetPALRAM uses addr&0x1f; the $00/$10 aliases are made
    // equal by ppuSetPALRAMMirror on writes, not by the read address decode.
    usize::from((address - 0x3f00) & 0x1f)
}
