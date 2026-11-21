//! Synthetic mapper-0 picture and PPU-bus fixtures.
//!
//! These ROMs contain no external assets. Their PRG and CHR arrays are
//! assembled below, and each expected value is tied to the fixNES source
//! ranges documented beside the assertion.

use nookery_famicom::core::{NesCore, RomImage};

fn rom(program: &[u8], chr: &[u8], flags6: u8) -> Vec<u8> {
    let mut image = vec![0; 16 + 0x4000 + 0x2000];
    image[..4].copy_from_slice(b"NES\x1a");
    image[4] = 1;
    image[5] = 1;
    image[6] = flags6;
    image[16..16 + program.len()].copy_from_slice(program);
    image[16 + 0x4000..].copy_from_slice(chr);
    image[16 + 0x3ffc] = 0;
    image[16 + 0x3ffd] = 0x80;
    image
}

fn instruction_writes(writes: &[(u16, u8)]) -> Vec<u8> {
    let mut program = Vec::with_capacity(writes.len() * 5 + 3);
    for &(address, value) in writes {
        program.extend([0xa9, value, 0x8d, address as u8, (address >> 8) as u8]);
    }
    finish(program)
}

fn ppu_memory_writes(program: &mut Vec<u8>, writes: &[(u16, u8)]) {
    for &(address, value) in writes {
        program.extend([
            0xa9,
            (address >> 8) as u8,
            0x8d,
            0x06,
            0x20,
            0xa9,
            address as u8,
            0x8d,
            0x06,
            0x20,
            0xa9,
            value,
            0x8d,
            0x07,
            0x20,
        ]);
    }
}

fn finish(mut program: Vec<u8>) -> Vec<u8> {
    let loop_address = 0x8000 + program.len() as u16;
    program.extend([0x4c, loop_address as u8, (loop_address >> 8) as u8]);
    program
}

fn chr_with_tile(tile: usize, low: u8, high: u8) -> [u8; 0x2000] {
    let mut chr = [0; 0x2000];
    for row in 0..8 {
        chr[tile * 16 + row] = low;
        chr[tile * 16 + row + 8] = high;
    }
    chr
}

#[test]
fn background_fixture_matches_attribute_pattern_and_hash() {
    let mut chr = chr_with_tile(1, 0xff, 0);
    // Tile 2 supplies a distinct second pattern for the neighboring tile.
    chr[2 * 16..2 * 16 + 8].fill(0x00);
    chr[2 * 16 + 8..2 * 16 + 16].fill(0xff);
    let register_writes = [
        (0x2000, 0x00), // background pattern table 0
        (0x2001, 0x0a), // background enabled; leftmost 8 pixels clipped
        (0x2005, 0x00),
        (0x2005, 0x00),
    ];
    let mut program = instruction_writes(&register_writes);
    program.truncate(program.len() - 3);
    ppu_memory_writes(
        &mut program,
        &[(0x3f00, 0x0f), (0x3f01, 0x16), (0x3f05, 0x27)],
    );
    ppu_memory_writes(&mut program, &[(0x2000, 0x01), (0x2001, 0x02)]);
    ppu_memory_writes(&mut program, &[(0x23c0, 0x01)]);
    program.extend(instruction_writes(&[(0x2005, 0), (0x2005, 0)]));
    program.truncate(program.len() - 3);
    let image = RomImage::parse(rom(&finish(program), &chr, 0)).unwrap();
    let mut core = NesCore::new(&image);
    let frame = core.frame().unwrap().picture;

    // fixNES ppu.c:808-818 draws BGTiles through PALRAM2; ppu.c:1980-1984
    // masks palette entries to six bits. The first 8 pixels are clipped by
    // PPU_BG_8PX (ppu.c:40-42), then tile 1's low plane makes color 1.
    assert_eq!(frame.pixels[0], 0x27);
    assert_eq!(frame.pixels[7], 0x27);
    assert_eq!(frame.pixels[8], 0x00);
    assert_eq!(frame.pixels[15], 0x00);
    // ppu.c:469-474 selects the attribute quadrant; tile 2's high plane
    // therefore selects color 2 from palette 1 at x=16..23.
    assert_eq!(frame.pixels[16], 0x0f);
    assert_eq!(frame.pixels[256 + 8], 0x00);
    // ppu.c:763-818 writes one indexed output per visible dot in the 256x240
    // surface; Frame::hash hashes those same dimensions and bytes.
    assert_eq!(frame.hash(), 0x51877313a0b2cd9a);
}

#[test]
fn sprite_fixture_exercises_priority_flip_8x16_hit_and_overflow() {
    let mut chr = chr_with_tile(1, 0xff, 0);
    chr[2 * 16..2 * 16 + 8].fill(0x80);
    chr[2 * 16 + 8..2 * 16 + 16].fill(0);
    chr[3 * 16..3 * 16 + 8].fill(0x01);
    chr[3 * 16 + 8..3 * 16 + 16].fill(0);
    let register_writes = [
        (0x2000, 0x20), // 8x16 sprites
        (0x2001, 0x1e), // background/sprites, including left 8 pixels
        (0x2005, 0x00),
        (0x2005, 0x00),
        (0x2003, 0x00),
        (0x2004, 0x00),
        (0x2004, 0x02), // sprite 0: visible, hit, normal (8x16 table 0)
        (0x2004, 0x00),
        (0x2004, 0x08),
        (0x2004, 0x00),
        (0x2004, 0x02), // sprite 1: priority behind background
        (0x2004, SPRITE_PRIORITY),
        (0x2004, 0x10),
        (0x2004, 0x00),
        (0x2004, 0x02),
        (0x2004, SPRITE_FLIP_X | SPRITE_FLIP_Y),
        (0x2004, 0x18), // sprite 2: both flips
        (0x2004, 0x00),
        (0x2004, 0x02),
        (0x2004, 0x00),
        (0x2004, 0x28), // sprite 3: 8x16 bottom half
    ];
    let mut program = instruction_writes(&register_writes);
    program.truncate(program.len() - 3);
    ppu_memory_writes(
        &mut program,
        &[
            (0x3f00, 0x0f),
            (0x3f01, 0x11),
            (0x3f11, 0x22),
            (0x2000, 0x01),
            (0x2001, 0x01),
            (0x2002, 0x01),
        ],
    );
    let scroll_reset = instruction_writes(&[(0x2005, 0), (0x2005, 0)]);
    program.extend(&scroll_reset[..scroll_reset.len() - 3]);
    // Six additional overlapping sprites make fixNES set overflow after the
    // first eight entries (ppu.c:626-630 and ppu.c:661-665).
    for x in [0x38, 0x40, 0x48, 0x50, 0x58, 0x60] {
        program.extend(instruction_writes(&[
            (0x2004, 0),
            (0x2004, 2),
            (0x2004, 0),
            (0x2004, x),
        ]));
        program.truncate(program.len() - 3);
    }
    let image = RomImage::parse(rom(&finish(program), &chr, 0)).unwrap();
    let mut core = NesCore::new(&image);
    let frame = core.frame().unwrap();
    // ppu.c:1122-1140 gives sprite pixels priority only when background is
    // transparent or the priority bit is clear; sprite 0 also sets hit at
    // ppu.c:1130-1134 when x<255.
    assert_eq!(frame.picture.pixels[8], 0x22);
    assert_eq!(frame.picture.pixels[16], 0x11);
    // ppu.c:469-474 selects the 8x16 tile half and ppu.c:450-488 applies
    // horizontal/vertical flips.
    assert_eq!(frame.picture.pixels[40], 0x22);
    assert_eq!(frame.picture.pixels[24], 0x22);
    assert_eq!(frame.picture.pixels[8 * 256 + 47], 0x22);
    assert_eq!(core.ppu_register(2), Some(0x60));
}

#[test]
fn scroll_fixture_moves_the_first_background_sample() {
    let chr = chr_with_tile(1, 0xff, 0);
    let register_writes = [
        (0x2000, 0x08), // show left edge and pattern table 0
        (0x2001, 0x0a),
        (0x2005, 0x08), // coarse X = 1
        (0x2005, 0x00),
    ];
    let mut program = instruction_writes(&register_writes);
    program.truncate(program.len() - 3);
    ppu_memory_writes(
        &mut program,
        &[(0x3f00, 0x0f), (0x3f01, 0x19), (0x2000, 0), (0x2001, 1)],
    );
    let image = RomImage::parse(rom(&finish(program), &chr, 0)).unwrap();
    let mut core = NesCore::new(&image);
    let frame = core.frame().unwrap().picture;
    // ppu.c:1762-1780 builds TmpVramAddr from the two $2005 writes; the
    // visible fetch at ppu.c:812 then starts at the resulting coarse tile.
    assert_eq!(frame.pixels[0], 0x19);
    assert_eq!(frame.pixels[7], 0x19);
}

#[test]
fn register_fixture_checks_status_buffer_increment_and_mirroring() {
    let mut program = vec![
        0xad, 0x02, 0x20, 0x85, 0x10, // first status read sees initial vblank
        0xa9, 0x12, 0x8d, 0x05, 0x20, // set w, then status clears w
        0xad, 0x02, 0x20, 0xa9, 0x04, 0x8d, 0x00, 0x20, // $2007 increment = 32
        0xa9, 0x20, 0x8d, 0x06, 0x20, 0xa9, 0x20, 0x8d, 0x06, 0x20, 0xa9, 0xbb, 0x8d, 0x07,
        0x20, // $2020 = $bb
        0xa9, 0x20, 0x8d, 0x06, 0x20, 0xa9, 0x00, 0x8d, 0x06, 0x20, 0xa9, 0xaa, 0x8d, 0x07,
        0x20, // writes $2000 then increments by 32
        0xa9, 0x20, 0x8d, 0x06, 0x20, 0xa9, 0x00, 0x8d, 0x06, 0x20, 0xad, 0x07, 0x20, 0x85,
        0x11, // buffered, old value
        0xad, 0x07, 0x20, 0x85, 0x12, // buffered $aa
        0xad, 0x07, 0x20, 0x85, 0x15, // buffered $bb after the +32 step
        0xa9, 0x00, 0x8d, 0x00, 0x20, // restore increment = 1 for mirroring checks
        0xa9, 0x55, 0x8d, 0x07, 0x20, // current $2002, then read $2400
        0xa9, 0x24, 0x8d, 0x06, 0x20, 0xa9, 0x00, 0x8d, 0x06, 0x20, 0xad, 0x07, 0x20, 0xad, 0x07,
        0x20, 0x85, 0x13,
    ];
    program.extend([
        0xa9, 0x3f, 0x8d, 0x06, 0x20, 0xa9, 0x00, 0x8d, 0x06, 0x20, 0xa9, 0x33, 0x8d, 0x07, 0x20,
        0xa9, 0x3f, 0x8d, 0x06, 0x20, 0xa9, 0x10, 0x8d, 0x06, 0x20, 0xad, 0x07, 0x20, 0x85, 0x14,
    ]);
    let program = finish(program);
    let chr = [0; 0x2000];
    let horizontal = RomImage::parse(rom(&program, &chr, 0)).unwrap();
    let mut hcore = NesCore::new(&horizontal);
    hcore.frame().unwrap();
    assert_eq!(hcore.ram()[0x10], 0x80);
    // ppu.c:1820-1849 returns the old buffer first, fills it from VRAM, and
    // increments by one because $2000 bit 2 was clear.
    assert_eq!(hcore.ram()[0x11], 0);
    assert_eq!(hcore.ram()[0x12], 0xaa);
    assert_eq!(hcore.ram()[0x15], 0xbb);
    // ppu.c:1919-1929 maps $2000/$2400 to one physical bank horizontally.
    assert_eq!(hcore.ram()[0x13], 0xaa);

    let vertical = RomImage::parse(rom(&program, &chr, 1)).unwrap();
    let mut vcore = NesCore::new(&vertical);
    vcore.frame().unwrap();
    // ppu.c:1919-1929 maps $2000 and $2400 to different banks vertically.
    assert_eq!(vcore.ram()[0x13], 0);
    // ppu.c:1980-2001 mirrors universal background entries at $3f10.
    assert_eq!(hcore.ram()[0x14], 0x33);
}

#[test]
fn nmi_edge_is_raised_once_after_vblank_enable_and_status_read_suppresses_it() {
    let mut ppu = nookery_famicom::Ppu::new(&[], true, nookery_famicom::Mirroring::Horizontal);
    ppu.write_register(0x2000, 0x80);
    ppu.add_dots(241 * 341 + 5);
    assert!(ppu.poll_nmi(), "ppuNMI raises one edge after vblank");
    assert!(!ppu.poll_nmi(), "ppuNMI does not repeat the same edge");

    let mut suppressed =
        nookery_famicom::Ppu::new(&[], true, nookery_famicom::Mirroring::Horizontal);
    suppressed.write_register(0x2000, 0x80);
    suppressed.add_dots(241 * 341 + 3);
    assert_eq!(suppressed.read_register(0x2002), 0x80);
    suppressed.add_dots(2);
    assert!(
        !suppressed.poll_nmi(),
        "ppu.c status read clears the NMI window"
    );

    let mut early_read =
        nookery_famicom::Ppu::new(&[], true, nookery_famicom::Mirroring::Horizontal);
    early_read.write_register(0x2000, 0x80);
    assert_eq!(early_read.read_register(0x2002), 0x80);
    early_read.add_dots(241 * 341 + 5);
    assert!(
        early_read.poll_nmi(),
        "an early status read does not leak into the next vblank"
    );
}

#[test]
fn short_chr_surface_is_zero_filled_without_out_of_bounds_access() {
    let mut ppu = nookery_famicom::Ppu::new(&[0xa5], true, nookery_famicom::Mirroring::Vertical);
    ppu.write_register(0x2000, 0x00);
    ppu.write_register(0x2001, 0x08);
    let frame = ppu.render();
    assert_eq!(frame.pixels.len(), 256 * 240);
    assert_eq!(frame.pixels[0], 0);
}

const SPRITE_PRIORITY: u8 = 1 << 5;
const SPRITE_FLIP_X: u8 = 1 << 6;
const SPRITE_FLIP_Y: u8 = 1 << 7;
