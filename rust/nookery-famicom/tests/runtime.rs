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
