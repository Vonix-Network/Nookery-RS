//! Native runtime candidate: synthetic ROM vectors and negative lifecycle paths.

use nookery_famicom::core::{
    AUDIO_FRAMES_PER_VIDEO_FRAME, AUDIO_SAMPLES_PER_VIDEO_FRAME, Controller, CoreError, NesCore,
    RomError, RomImage,
};
use nookery_famicom::input::{BUTTON_L, BUTTON_R, BUTTON_X, BUTTON_Y, BUTTON_Z};
use nookery_famicom::{
    FamicomRuntime, FrameAction, HostSeam, RomId, RuntimeError, RuntimeState, controller_byte,
};

fn fixture_rom(program: &[u8]) -> Vec<u8> {
    let mut rom = vec![0; 16 + 0x4000];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    let prg = 16;
    rom[prg..prg + program.len()].copy_from_slice(program);
    rom[prg + 0x3ffc] = 0;
    rom[prg + 0x3ffd] = 0x80;
    rom
}

fn increment_fixture() -> Vec<u8> {
    // LDA #1; STA $0000; INX; JMP $8005.
    fixture_rom(&[0xa9, 1, 0x8d, 0, 0, 0xe8, 0x4c, 0x05, 0x80])
}

fn fixture_with_reset(program: &[(usize, u8)], reset: u16) -> Vec<u8> {
    let mut rom = vec![0; 16 + 0x4000];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    let prg = 16;
    for &(offset, value) in program {
        rom[prg + offset] = value;
    }
    rom[prg + 0x3ffc] = reset as u8;
    rom[prg + 0x3ffd] = (reset >> 8) as u8;
    rom
}

struct RecordingHost {
    events: Vec<&'static str>,
}

impl HostSeam for RecordingHost {
    fn pump_audio(&mut self) {
        self.events.push("audio");
    }

    fn advance_famicom_frame(&mut self) {
        self.events.push("frame");
    }

    fn present_black(&mut self) {
        self.events.push("black");
    }

    fn return_to_game(&mut self) {
        self.events.push("return");
    }
}

#[test]
fn synthetic_vector_has_deterministic_cpu_frame_and_audio_counts() {
    let image = RomImage::parse(increment_fixture()).expect("fixture");
    let mut core = NesCore::new(&image);
    let first = core.frame().expect("supported fixture");
    let second = core.frame().expect("supported fixture");

    assert_eq!(first.frame_number, 1);
    assert_eq!(second.frame_number, 2);
    assert_eq!(first.cpu_cycles, 29_781);
    assert_eq!(second.cpu_cycles, 29_782);
    assert_eq!(first.audio.stereo_frames(), AUDIO_FRAMES_PER_VIDEO_FRAME);
    assert_eq!(first.audio.sample_count(), AUDIO_SAMPLES_PER_VIDEO_FRAME);
    assert_eq!(first.audio.samples, vec![0; AUDIO_SAMPLES_PER_VIDEO_FRAME]);
    assert_eq!(core.ram()[0], 1);
}

#[test]
fn official_cpu_addressing_stack_rmw_and_register_boundaries_run_end_to_end() {
    let mut program = vec![
        0xa2, 0x03, // LDX #3
        0xca, // DEX
        0xd0, 0xfd, // BNE $8002
        0x8e, 0x00, 0x20, // STX $2000: PPU register boundary
        0xa9, 0x5a, 0x8d, 0x00, 0x40, // LDA #$5a; STA $4000: APU boundary
        0xa9, 0xa5, 0x8d, 0x08, 0x40, // STA $4008: distinct APU register
        0xa9, 0x3c, 0x8d, 0x10, 0x40, // STA $4010: distinct APU register
        0xa9, 0x77, 0x8d, 0x17, 0x40, // STA $4017: distinct APU register
        0xa9, 0x00, 0x85, 0x20, // Set ($20),Y pointer low
        0xa9, 0x00, 0x85, 0x21, // Set ($20),Y pointer high
        0xa2, 0x00, // LDX #0
        0xa9, 0x42, 0x81, 0x20, // STA ($20,X)
        0xa0, 0x01, 0xa9, 0x33, 0x91, 0x20, // LDY #1; STA ($20),Y
        0xa9, 0x01, 0x8d, 0x16, 0x40, // Strobe controller high
        0xa9, 0x00, 0x8d, 0x16, 0x40, // Then latch it low
        0xad, 0x16, 0x40, 0x85, 0x10, // Read A and retain it
        0x20, 0x60, 0x80, // JSR $8060
        0xa9, 0x70, 0x85, 0x30, 0xa9, 0x80, 0x85, 0x31, // JMP ($0030)
        0x6c, 0x30, 0x00,
    ];
    program.resize(0x60, 0xea);
    program.extend([
        0xa9, 0x7f, 0x85, 0x11, // LDA #$7f; STA $11
        0x06, 0x11, // ASL $11 -> $fe
        0xe6, 0x11, // INC $11 -> $ff
        0xa5, 0x11, // LDA $11
        0x48, 0xa9, 0x01, 0x68, 0x85, 0x12, // PHA; LDA #1; PLA; STA $12
        0x60, // RTS
    ]);
    program.resize(0x70, 0xea);
    program.extend([0xa9, 0x99, 0x85, 0x13, 0x4c, 0x70, 0x80]);

    let image = RomImage::parse(fixture_rom(&program)).expect("fixture");
    let mut core = NesCore::new(&image);
    core.set_input(0x01);
    core.frame()
        .expect("all official instructions are supported");

    assert_eq!(core.ram()[0x00], 0x42, "indexed-indirect store");
    assert_eq!(core.ram()[0x01], 0x33, "indirect-indexed store");
    assert_eq!(core.ram()[0x10], 0x41, "controller open-bus read");
    assert_eq!(core.ram()[0x11], 0xff, "zero-page read-modify-write");
    assert_eq!(core.ram()[0x12], 0xff, "stack push/pop and PLA");
    assert_eq!(core.ram()[0x13], 0x99, "JMP indirect target");
    assert_eq!(core.ppu_register(0), Some(0), "PPU bus write");
    assert_eq!(core.apu_register(0), Some(0x5a), "APU bus write");
    assert_eq!(core.apu_register(8), Some(0xa5), "APU $4008 bus write");
    assert_eq!(core.apu_register(0x10), Some(0x3c), "APU $4010 bus write");
    assert_eq!(core.apu_register(0x17), Some(0x77), "APU $4017 bus write");
}

#[test]
fn zero_prg_with_chr_uses_remaining_bytes_as_prg_without_panicking() {
    let mut rom = vec![0xea; 16 + 0x2000];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 0;
    rom[5] = 1;
    rom[6] = 0;
    rom[7] = 0;
    let prg = 16;
    rom[prg + 0x1ffc] = 0;
    rom[prg + 0x1ffd] = 0x80;

    let image = RomImage::parse(rom).expect("zero-PRG fallback is valid");
    assert_eq!(image.prg().len(), 0x2000);
    assert!(image.chr().is_empty());
    assert!(image.has_chr_ram());

    let mut core = NesCore::new(&image);
    core.frame()
        .expect("fallback PRG executes without divide by zero");
    assert_eq!(core.frame_number(), 1);
}

#[test]
fn nonzero_prg_is_clamped_to_supplied_payload_before_chr() {
    let mut rom = vec![0; 16 + 0x2000];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 2;
    rom[5] = 1;
    rom[16] = 0xa5;

    let image = RomImage::parse(rom).expect("fixNES clamps oversized PRG");
    assert_eq!(image.prg().len(), 0x2000);
    assert!(image.chr().is_empty());
}

#[test]
fn chr_is_clamped_after_a_fitting_prg() {
    let mut rom = vec![0; 16 + 0x4000 + 0x1000];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    rom[5] = 1;
    rom[16 + 0x4000] = 0x5a;

    let image = RomImage::parse(rom).expect("fixNES clamps oversized CHR");
    assert_eq!(image.prg().len(), 0x4000);
    assert_eq!(image.chr().len(), 0x1000);
    assert_eq!(image.chr()[0], 0x5a);
}

#[test]
fn trainer_is_skipped_while_zero_prg_uses_remaining_payload_safely() {
    let mut rom = vec![0; 16 + 512 + 0x2000];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 0;
    rom[5] = 1;
    rom[6] = 0x04;
    rom[16] = 0xee;
    rom[16 + 512] = 0xa5;

    let image = RomImage::parse(rom).expect("trainer and zero PRG are valid");
    assert!(image.has_trainer());
    assert_eq!(image.prg().len(), 0x2000);
    assert_eq!(image.prg()[0], 0xa5, "trainer is not PRG data");
    assert!(image.chr().is_empty());
}

#[test]
fn clamping_to_empty_prg_is_a_typed_error() {
    let mut rom = vec![0; 16];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    assert_eq!(
        RomImage::parse(rom),
        Err(RomError::TruncatedData {
            offset: 16,
            required: 1,
            available: 0,
        })
    );
}

#[test]
fn mapper_zero_prg_ram_is_backed_and_4015_is_status_not_write_echo() {
    let mut program = vec![
        0xa9, 0x5a, 0x8d, 0x00, 0x60, // $6000 = $5a
        0xad, 0x00, 0x60, 0x85, 0x20, // read it back
        0xa9, 0x1f, 0x8d, 0x15, 0x40, // write APU enable bits
        0xad, 0x15, 0x40, 0x85, 0x21, // status has no active channels
        0x4c, 0x00, 0x80,
    ];
    program.resize(0x30, 0xea);
    let mut rom = fixture_rom(&program);
    rom[8] = 2;
    let image = RomImage::parse(rom).expect("mapper-0 fixture");
    assert_eq!(image.prg_ram_size(), 0x4000);
    let mut core = NesCore::new(&image);
    core.frame().expect("RAM/status fixture executes");
    assert_eq!(core.prg_ram()[0], 0x5a);
    assert_eq!(core.ram()[0x20], 0x5a);
    assert_eq!(core.ram()[0x21], 0);
}

#[test]
fn oam_dma_copies_a_cpu_page_through_ppu_oam() {
    let program = [
        0xa9, 0x00, 0x8d, 0x03, 0x20, // OAMADDR = 0
        0xa9, 0xa5, 0x8d, 0x00, 0x02, // CPU page $02, byte 0 = $a5
        0xa9, 0x02, 0x8d, 0x14, 0x40, // OAMDMA page $02
        0x4c, 0x00, 0x80,
    ];
    let image = RomImage::parse(fixture_rom(&program)).expect("OAM DMA fixture");
    let mut core = NesCore::new(&image);
    core.frame().expect("OAM DMA executes");
    assert_eq!(core.ppu_oam()[0], 0xa5);
    assert_eq!(core.ppu_oam()[1], 0);
}

#[test]
fn controller_reads_preserve_fixnes_open_bus_upper_bits_through_cpu_bus() {
    let program = [
        0xa9, 0x01, 0x8d, 0x16, 0x40, // strobe high
        0xa9, 0x00, 0x8d, 0x16, 0x40, // then low, reset controller position
        0xad, 0x16, 0x40, 0x85, 0x20, // A=0, upper bits must remain $40
        0xad, 0x16, 0x40, 0x85, 0x21, // next bit remains open-bus $40
        0x4c, 0x0a, 0x80,
    ];
    let image = RomImage::parse(fixture_rom(&program)).expect("controller fixture");
    let mut core = NesCore::new(&image);
    core.set_input(0x00);
    core.frame().expect("controller fixture executes");
    assert_eq!(core.ram()[0x20], 0x40);
    assert_eq!(core.ram()[0x21], 0x40);
}

#[test]
fn player_two_register_returns_fixnes_open_bus_and_leaves_p1_shift_untouched() {
    // C oracle: inputGetP2 (pc/lib/fixnes/input.c:65-70) has no player-2
    // state, so it returns `(memLastVal & ~0x1F)` and never advances `inPos`
    // or reads `inValReads`.  Each `LDA $4017` operand fetch leaves the high
    // operand byte $40 in memLastVal, so the read yields $40 with bits 0..4
    // cleared.  Reading $4017 must not consume a player-1 shift position.
    let program = [
        0xa9, 0x01, 0x8d, 0x16, 0x40, // strobe high
        0xa9, 0x00, 0x8d, 0x16, 0x40, // strobe low, P1 position = 0
        0xad, 0x17, 0x40, 0x85, 0x20, // LDA $4017 -> open bus $40
        0xad, 0x17, 0x40, 0x85, 0x23, // second $4017 read, still $40
        0xad, 0x16, 0x40, 0x85, 0x21, // P1 bit 0 (A) is still first
        0xad, 0x16, 0x40, 0x85, 0x22, // P1 bit 1 (B)
        0x4c, 0x1e, 0x80, // JMP self; JMP starts at $801e
    ];
    let image = RomImage::parse(fixture_rom(&program)).expect("controller fixture");
    let mut core = NesCore::new(&image);
    core.set_input(0x01); // A only
    core.frame().expect("controller fixture executes");

    assert_eq!(core.ram()[0x20], 0x40, "$4017 keeps open-bus upper bits");
    assert_eq!(core.ram()[0x23], 0x40, "$4017 read is not a shift step");
    assert_eq!(core.ram()[0x21], 0x41, "P1 A bit not consumed by $4017");
    assert_eq!(core.ram()[0x22], 0x40, "P1 B bit follows A");
}

#[test]
fn oam_dma_total_stall_does_not_claim_per_cycle_timing_parity() {
    assert!(NesCore::gaps().contains(&nookery_famicom::core::CoreGap::OamDmaTiming));
}

#[test]
fn oam_dma_adds_the_hardware_stall_to_frame_cadence() {
    let no_dma = [
        0xa9, 0x00, 0x8d, 0x03, 0x20, // OAMADDR = 0
        0xa9, 0xa5, 0x8d, 0x00, 0x02, // CPU page $02, byte 0 = $a5
        0xa9, 0x02, 0x8d, 0x13, 0x40, // Same-cycle non-DMA register write
        0x4c, 0x12, 0x80, // Enter the post-DMA loop once
        0xe6, 0x10, // INC $10
        0x4c, 0x12, 0x80,
    ];
    let with_dma = [
        0xa9, 0x00, 0x8d, 0x03, 0x20, // OAMADDR = 0
        0xa9, 0xa5, 0x8d, 0x00, 0x02, // CPU page $02, byte 0 = $a5
        0xa9, 0x02, 0x8d, 0x14, 0x40, // OAMDMA page $02
        0x4c, 0x12, 0x80, // Enter the post-DMA loop once
        0xe6, 0x10, // INC $10
        0x4c, 0x12, 0x80,
    ];
    let mut baseline = NesCore::new(&RomImage::parse(fixture_rom(&no_dma)).unwrap());
    let mut dma = NesCore::new(&RomImage::parse(fixture_rom(&with_dma)).unwrap());
    let baseline_frame = baseline.frame().expect("baseline fixture executes");
    let dma_frame = dma.frame().expect("DMA fixture executes");

    assert_eq!(baseline_frame.oam_dma_cycles, 0);
    assert_eq!(dma_frame.oam_dma_cycles, 513);
    assert!(
        dma.ram()[0x10] < baseline.ram()[0x10],
        "the DMA stall must reduce post-DMA loop iterations"
    );
    assert_eq!(dma.ppu_oam()[0], 0xa5);
}

#[test]
fn oam_dma_uses_the_extra_alignment_cycle_on_an_odd_boundary() {
    let program = [
        0xd0, 0x00, // Taken branch: make the $4014 write end on an odd cycle.
        0xa9, 0x00, 0x8d, 0x03, 0x20, // OAMADDR = 0
        0xa9, 0xa5, 0x8d, 0x00, 0x02, // CPU page $02, byte 0 = $a5
        0xa9, 0x02, 0x8d, 0x14, 0x40, // OAMDMA page $02
        0x4c, 0x14, 0x80, // Enter a loop without another DMA request
        0xe6, 0x10, // INC $10
        0x4c, 0x14, 0x80,
    ];
    let image = RomImage::parse(fixture_rom(&program)).expect("odd-cycle DMA fixture");
    let mut core = NesCore::new(&image);
    let frame = core.frame().expect("odd-cycle DMA executes");

    assert_eq!(frame.oam_dma_cycles, 514);
    assert_eq!(core.ppu_oam()[0], 0xa5);
}

#[test]
fn oam_dma_wraps_oam_address_at_the_256_byte_boundary() {
    let program = [
        0xa9, 0xff, 0x8d, 0x03, 0x20, // OAMADDR = $ff
        0xa9, 0x5a, 0x8d, 0xff, 0x00, // CPU page $00, byte $ff = $5a
        0xa9, 0x00, 0x8d, 0x14, 0x40, // OAMDMA page $00
        0x4c, 0x0f, 0x80, // Loop without another DMA request
    ];
    let image = RomImage::parse(fixture_rom(&program)).expect("OAM wrap fixture");
    let mut core = NesCore::new(&image);
    core.frame().expect("OAM wrap executes");

    assert_eq!(core.ppu_oam()[0xff], 0);
    // Source $00ff lands at OAM $fe; fixNES masks attribute-byte bits there.
    assert_eq!(core.ppu_oam()[0xfe], 0x42);
}

#[test]
fn indirect_jmp_wraps_high_byte_within_page_during_execution() {
    let rom = fixture_with_reset(
        &[
            (0x00, 0x80), // Wrapped high byte for JMP ($80ff).
            (0x03, 0x6c),
            (0x04, 0xff),
            (0x05, 0x80),
            (0x10, 0xa9), // Target at $8010.
            (0x11, 0x99),
            (0x12, 0x85),
            (0x13, 0x20),
            (0x14, 0x4c),
            (0x15, 0x10),
            (0x16, 0x80),
            (0xff, 0x10), // Low byte for the wrapped vector.
        ],
        0x8003,
    );
    let image = RomImage::parse(rom).expect("fixture");
    let mut core = NesCore::new(&image);
    core.frame().expect("JMP indirect is supported");
    assert_eq!(core.ram()[0x20], 0x99, "wrapped indirect JMP target");
}

#[test]
fn ppu_nmi_pushes_return_state_and_runs_the_fffa_handler() {
    let mut program = vec![
        0xa9, 0x80, 0x8d, 0x00, 0x20, // enable PPU NMI
        0x4c, 0x05, 0x80, // wait in a stable loop
    ];
    program.resize(0x1000, 0xea);
    program.extend([
        0xad, 0xfd, 0x01, 0x85, 0x21, // saved PC high
        0xad, 0xfc, 0x01, 0x85, 0x22, // saved PC low
        0xad, 0xfb, 0x01, 0x85, 0x23, // saved status, B clear and U set
        0xa9, 0x77, 0x85, 0x20, // NMI handler marker
        0x40, // RTI restores PC and P
    ]);
    let mut rom = fixture_rom(&program);
    let prg = 16;
    rom[prg + 0x3ffa] = 0x00;
    rom[prg + 0x3ffb] = 0x90;
    let image = RomImage::parse(rom).expect("NMI fixture");
    let mut core = NesCore::new(&image);
    let frame = core.frame().expect("NMI handler executes");

    assert_eq!(core.ram()[0x20], 0x77);
    assert_eq!(core.ram()[0x21], 0x80);
    assert_eq!(core.ram()[0x22], 0x05);
    assert_eq!(core.ram()[0x23], 0xa4);
    assert_eq!(core.program_counter(), 0x8005);
    assert!(
        frame.cpu_cycles >= 29_781,
        "NMI does not shorten the frame cadence: {}",
        frame.cpu_cycles,
    );
}

#[test]
fn taken_branch_crosses_page_and_pays_the_penalty() {
    let rom = fixture_with_reset(
        &[
            (0xfd, 0xd0), // BNE from $80fd to $8100.
            (0xfe, 0x01),
            (0x100, 0xa9), // Branch target at $8100.
            (0x101, 0x77),
            (0x102, 0x85),
            (0x103, 0x22),
            (0x104, 0x4c),
            (0x105, 0x00),
            (0x106, 0x81),
        ],
        0x80fd,
    );
    let image = RomImage::parse(rom).expect("fixture");
    let mut core = NesCore::new(&image);
    let frame = core.frame().expect("branch is supported");
    assert_eq!(core.ram()[0x22], 0x77, "cross-page branch target");
    assert_eq!(frame.cpu_cycles, 29_782, "taken page-cross branch penalty");
}

#[test]
fn unsupported_cpu_opcode_is_a_typed_gap() {
    let image = RomImage::parse(fixture_rom(&[0x02])).expect("fixture");
    let mut core = NesCore::new(&image);
    assert_eq!(
        core.frame(),
        Err(CoreError::UnsupportedCpuOpcode {
            pc: 0x8000,
            opcode: 0x02
        })
    );
}

#[test]
fn unofficial_nop_family_matches_two_cycle_nop_without_changing_cpu_state() {
    let family = [0x1a, 0x3a, 0x5a, 0x7a, 0xda, 0xfa];
    let official = [0xea; 6];
    let suffix = [
        0x85, 0x20, // STA $20: capture A
        0x86, 0x21, // STX $21: capture X
        0x84, 0x22, // STY $22: capture Y
        0x08, // PHP
        0x68, // PLA: capture P through A
        0x85, 0x23, // STA $23
        0x4c, 0x00, 0x80, // stable loop
    ];
    let mut family_program = vec![
        0xa9, 0x81, // LDA #$81
        0xa2, 0x37, // LDX #$37
        0xa0, 0x42, // LDY #$42
        0x38, // SEC
        0xf8, // SED
        0x78, // SEI
    ];
    family_program.extend(family);
    family_program.extend(suffix);
    let mut official_program = vec![
        0xa9, 0x81, // LDA #$81
        0xa2, 0x37, // LDX #$37
        0xa0, 0x42, // LDY #$42
        0x38, // SEC
        0xf8, // SED
        0x78, // SEI
    ];
    official_program.extend(official);
    official_program.extend(suffix);

    let family_image = RomImage::parse(fixture_rom(&family_program)).expect("family fixture");
    let official_image =
        RomImage::parse(fixture_rom(&official_program)).expect("official NOP fixture");
    let mut family_core = NesCore::new(&family_image);
    let mut official_core = NesCore::new(&official_image);
    let family_frame = family_core.frame().expect("unofficial NOPs are supported");
    let official_frame = official_core.frame().expect("official NOPs are supported");

    assert_eq!(family_frame.cpu_cycles, official_frame.cpu_cycles);
    assert_eq!(
        &family_core.ram()[0x20..0x24],
        &official_core.ram()[0x20..0x24]
    );
    assert_eq!(&family_core.ram()[0x20..0x24], &[0x81, 0x37, 0x42, 0x3d]);
}

#[test]
fn invalid_and_unsupported_roms_fail_before_core_creation() {
    let id = RomId::new(0).expect("catalog id");
    let mut runtime = FamicomRuntime::new_pc();
    assert_eq!(
        runtime.load_rom(id, vec![0; 15]),
        Err(RuntimeError::InvalidRom(RomError::TooShort { actual: 15 }))
    );

    let mut bad_magic = fixture_rom(&[0xea]);
    bad_magic[..4].copy_from_slice(b"BAD!");
    assert_eq!(
        runtime.load_rom(id, bad_magic),
        Err(RuntimeError::InvalidRom(RomError::BadMagic))
    );

    let mut unsupported = fixture_rom(&[0xea]);
    unsupported[6] = 0x10;
    assert_eq!(
        runtime.load_rom(id, unsupported),
        Err(RuntimeError::InvalidRom(RomError::UnsupportedMapper {
            mapper: 1
        }))
    );

    assert_eq!(
        runtime.load_rom(RomId::new(8).unwrap(), fixture_rom(&[0xea])),
        Err(RuntimeError::NoRomResource { id: 8 })
    );
}

#[test]
fn runtime_rejects_frame_before_load_after_exit_and_double_cleanup() {
    let mut runtime = FamicomRuntime::new_pc();
    let mut host = RecordingHost { events: Vec::new() };
    assert_eq!(
        runtime.frame(&mut host, [0; 4]),
        Err(RuntimeError::RomNotLoaded)
    );
    runtime
        .load_rom(RomId::new(0).unwrap(), increment_fixture())
        .unwrap();
    let combo = BUTTON_L | BUTTON_R | BUTTON_Z;
    for _ in 0..60 {
        assert_eq!(
            runtime.frame(&mut host, [combo, 0, 0, 0]).unwrap().action,
            FrameAction::PresentBlack
        );
    }
    assert_eq!(runtime.state(), RuntimeState::Fading { remaining: 0 });
    assert_eq!(
        runtime.frame(&mut host, [0; 4]).unwrap().action,
        FrameAction::ReturnToGame
    );
    assert_eq!(
        runtime.frame(&mut host, [0; 4]),
        Err(RuntimeError::AlreadyExited)
    );
    runtime.cleanup().unwrap();
    assert_eq!(runtime.cleanup(), Err(RuntimeError::AlreadyCleaned));
}

#[test]
fn both_supported_combos_request_exit_and_audio_precedes_core_frame() {
    for combo in [
        BUTTON_L | BUTTON_R | BUTTON_Z,
        BUTTON_L | BUTTON_R | BUTTON_X | BUTTON_Y,
    ] {
        let mut runtime = FamicomRuntime::new_pc();
        runtime
            .load_rom(RomId::new(0).unwrap(), increment_fixture())
            .unwrap();
        let mut host = RecordingHost { events: Vec::new() };
        let report = runtime.frame(&mut host, [combo, 0, 0, 0]).unwrap();
        assert_eq!(report.action, FrameAction::PresentBlack);
        assert_eq!(runtime.state(), RuntimeState::Fading { remaining: 59 });
        assert_eq!(host.events, vec!["black"]);

        let mut running = FamicomRuntime::new_pc();
        running
            .load_rom(RomId::new(0).unwrap(), increment_fixture())
            .unwrap();
        let mut running_host = RecordingHost { events: Vec::new() };
        let report = running.frame(&mut running_host, [0; 4]).unwrap();
        assert_eq!(
            report.action,
            FrameAction::AdvanceFrame { pump_audio: true }
        );
        assert_eq!(report.audio_pumps, 1);
        assert_eq!(report.audio_samples, AUDIO_SAMPLES_PER_VIDEO_FRAME);
        assert_eq!(running_host.events, vec!["audio", "frame"]);
    }
}

#[test]
fn controller_mapping_and_shift_order_match_fixnes_input() {
    assert_eq!(controller_byte(0x8000), 0x01);
    assert_eq!(controller_byte(0x0100), 0x80);

    let mut controller = Controller::new();
    controller.set_buttons(0b1101_0011);
    controller.write_strobe(0);
    let reads: Vec<u8> = (0..8).map(|_| controller.read()).collect();
    // A, B, Select, Start, Up, Down, Left, Right; opposing directions read 0.
    assert_eq!(reads, vec![1, 1, 0, 0, 1, 0, 1, 0]);
    controller.write_strobe(1);
    assert_eq!(controller.read(), 1);
    assert_eq!(controller.read(), 1);

    let mut opposing_vertical = Controller::new();
    opposing_vertical.set_buttons(0b0011_0011);
    opposing_vertical.write_strobe(0);
    let reads: Vec<u8> = (0..8).map(|_| opposing_vertical.read()).collect();
    assert_eq!(reads, vec![1, 1, 0, 0, 1, 0, 0, 0]);
}
