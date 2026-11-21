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
    assert_eq!(core.ram()[0x10], 1, "controller shift register read");
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
}
