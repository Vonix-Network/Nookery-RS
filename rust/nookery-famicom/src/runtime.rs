//! Executable, allocation-free-at-the-frame-boundary Famicom runtime shell.
//!
//! This module owns the runtime lifecycle and the ordering between ROM load,
//! input, audio, emulation, presentation, and cleanup. The CPU/PPU/APU
//! implementation remains behind typed state so host callbacks do not own
//! emulator internals.

use crate::core::{AudioFrame, CoreError, NesCore, RomError};
use crate::exit::{
    EmulatorExit, ExitPhase, FrameAction, HostSeam, LoadResult, PAD_COUNT, Platform, dispatch,
};
use crate::ppu::Frame;
use crate::rom::{ResourcePath, RomId};

/// Runtime lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeState {
    /// No ROM has been loaded yet.
    Created,
    /// A valid ROM is loaded and frames may advance.
    Running,
    /// The host is presenting the 60-frame exit fade.
    Fading {
        /// Black frames still to present before the return frame.
        remaining: u16,
    },
    /// The host has returned to the game scene.
    Exited,
    /// Cleanup has completed; all later operations are rejected.
    Cleaned,
}

/// Errors returned by the runtime lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A frame was requested before a ROM was loaded.
    RomNotLoaded,
    /// A frame was requested after the runtime left the emulator scene.
    AlreadyExited,
    /// Cleanup was requested more than once.
    AlreadyCleaned,
    /// The supplied bytes are not a supported iNES image.
    InvalidRom(RomError),
    /// The selected catalog slot is empty (`NULL` in the C table).
    NoRomResource {
        /// Catalog id that selected the empty slot.
        id: u32,
    },
    /// The native core reached an explicitly unported instruction.
    Core(CoreError),
}

/// Result of one runtime frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameReport {
    /// Host action selected by the upstream loop order.
    pub action: FrameAction,
    /// Number of successfully advanced emulator frames.
    pub frame_number: u64,
    /// Number of audio pumps performed before emulator frames.
    pub audio_pumps: u64,
    /// Interleaved audio samples captured on an advance frame.
    pub audio_samples: usize,
    /// Hash of the indexed picture on an advance frame.
    pub frame_hash: Option<u64>,
}

/// The source-grounded runtime coordinator.
pub struct FamicomRuntime {
    exit: EmulatorExit,
    state: RuntimeState,
    rom: Option<Vec<u8>>,
    rom_path: Option<ResourcePath>,
    core: Option<NesCore>,
    last_audio: Option<AudioFrame>,
    last_frame: Option<Frame>,
    frame_number: u64,
    audio_pumps: u64,
    cleaned: bool,
}

impl FamicomRuntime {
    /// Creates a PC runtime with no loaded ROM.
    #[must_use]
    pub fn new_pc() -> Self {
        Self::new(Platform::Pc)
    }

    /// Creates a runtime for the selected host platform.
    #[must_use]
    pub fn new(platform: crate::exit::Platform) -> Self {
        Self {
            exit: EmulatorExit::new(platform),
            state: RuntimeState::Created,
            rom: None,
            rom_path: None,
            core: None,
            last_audio: None,
            last_frame: None,
            frame_number: 0,
            audio_pumps: 0,
            cleaned: false,
        }
    }

    /// Current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> RuntimeState {
        self.state
    }

    /// Loaded ROM bytes, when a ROM has been accepted.
    #[must_use]
    pub fn rom(&self) -> Option<&[u8]> {
        self.rom.as_deref()
    }

    /// Selected catalog path, when a ROM id has been selected.
    #[must_use]
    pub const fn rom_path(&self) -> Option<ResourcePath> {
        self.rom_path
    }

    /// Loads already-provided ROM bytes and records the catalog selection.
    pub fn load_rom(&mut self, id: RomId, bytes: Vec<u8>) -> Result<(), RuntimeError> {
        if self.cleaned {
            return Err(RuntimeError::AlreadyCleaned);
        }
        let rom_path = id
            .resource_path()
            .ok_or(RuntimeError::NoRomResource { id: id.get() })?;
        let image =
            crate::core::RomImage::parse(bytes.clone()).map_err(RuntimeError::InvalidRom)?;
        self.core = Some(NesCore::new(&image));
        self.rom = Some(bytes);
        self.rom_path = Some(rom_path);
        self.state = RuntimeState::Running;
        Ok(())
    }

    /// Runs one upstream-ordered frame.
    pub fn frame(
        &mut self,
        host: &mut dyn HostSeam,
        pads: [u16; PAD_COUNT],
    ) -> Result<FrameReport, RuntimeError> {
        if self.cleaned {
            return Err(RuntimeError::AlreadyCleaned);
        }
        if self.rom.is_none() {
            return Err(RuntimeError::RomNotLoaded);
        }
        if self.state == RuntimeState::Exited {
            return Err(RuntimeError::AlreadyExited);
        }

        let buttons = controller_byte(pads[0]);
        let mut runtime_host = RuntimeHost {
            host,
            core: self.core.as_mut().expect("ROM/core invariant"),
            buttons,
            core_frame: None,
            core_error: None,
            audio_pumps: 0,
        };
        let action = dispatch(
            &mut self.exit,
            &mut runtime_host,
            Some(LoadResult::Loaded),
            Some(&pads),
        );
        if let Some(error) = runtime_host.core_error {
            return Err(RuntimeError::Core(error));
        }
        if matches!(action, FrameAction::AdvanceFrame { .. }) {
            self.frame_number += 1;
            self.audio_pumps += runtime_host.audio_pumps;
            if let Some(frame) = runtime_host.core_frame {
                self.last_audio = Some(frame.audio);
                self.last_frame = Some(frame.picture);
            }
        }
        self.state = if matches!(action, FrameAction::ReturnToGame) {
            RuntimeState::Exited
        } else {
            match self.exit.phase() {
                ExitPhase::Running => RuntimeState::Running,
                ExitPhase::FadeOut { remaining } => RuntimeState::Fading { remaining },
                ExitPhase::ReadyToReturn => RuntimeState::Fading { remaining: 0 },
                ExitPhase::LoadFailed => RuntimeState::Exited,
            }
        };
        Ok(FrameReport {
            action,
            frame_number: self.frame_number,
            audio_pumps: self.audio_pumps,
            audio_samples: if matches!(action, FrameAction::AdvanceFrame { .. }) {
                self.last_audio.as_ref().map_or(0, AudioFrame::sample_count)
            } else {
                0
            },
            frame_hash: if matches!(action, FrameAction::AdvanceFrame { .. }) {
                self.last_frame.as_ref().map(Frame::hash)
            } else {
                None
            },
        })
    }

    /// Most recent audio handoff, captured before the core cadence reset.
    #[must_use]
    pub fn last_audio(&self) -> Option<&AudioFrame> {
        self.last_audio.as_ref()
    }

    /// Most recent indexed 256x240 picture.
    #[must_use]
    pub fn last_frame(&self) -> Option<&Frame> {
        self.last_frame.as_ref()
    }

    /// Performs idempotent cleanup once the runtime has exited.
    pub fn cleanup(&mut self) -> Result<(), RuntimeError> {
        if self.cleaned {
            return Err(RuntimeError::AlreadyCleaned);
        }
        self.rom = None;
        self.rom_path = None;
        self.core = None;
        self.last_audio = None;
        self.last_frame = None;
        self.cleaned = true;
        self.state = RuntimeState::Cleaned;
        Ok(())
    }
}

/// Converts the Aurora/Gekko high-byte pad order into fixNES A..Right bits.
#[must_use]
pub const fn controller_byte(raw: u16) -> u8 {
    let raw = (raw >> 8) as u8;
    raw.reverse_bits()
}

struct RuntimeHost<'a> {
    host: &'a mut dyn HostSeam,
    core: &'a mut NesCore,
    buttons: u8,
    core_frame: Option<crate::core::CoreFrame>,
    core_error: Option<CoreError>,
    audio_pumps: u64,
}

impl HostSeam for RuntimeHost<'_> {
    fn begin_frame(&mut self) {
        self.host.begin_frame();
    }

    fn pump_audio(&mut self) {
        self.audio_pumps += 1;
        self.host.pump_audio();
    }

    fn advance_famicom_frame(&mut self) {
        self.core.set_input(self.buttons);
        match self.core.frame() {
            Ok(frame) => self.core_frame = Some(frame),
            Err(error) => self.core_error = Some(error),
        }
        self.host.advance_famicom_frame();
    }

    fn present_black(&mut self) {
        self.host.present_black();
    }

    fn return_to_game(&mut self) {
        self.host.return_to_game();
    }

    fn end_frame(&mut self) {
        self.host.end_frame();
    }

    fn request_fade_out(&mut self, frames: u16) {
        self.host.request_fade_out(frames);
    }

    fn report_load_failure(&mut self) {
        self.host.report_load_failure();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::FrameAction;
    use crate::input::{BUTTON_L, BUTTON_R, BUTTON_Z};

    struct NullHost;

    impl HostSeam for NullHost {
        fn advance_famicom_frame(&mut self) {}
    }

    #[test]
    fn runtime_exit_countdown_presents_exactly_sixty_black_frames() {
        let mut runtime = FamicomRuntime::new_pc();
        runtime
            .load_rom(RomId::new(0).expect("catalog id"), fixture_rom())
            .expect("load");
        let mut host = NullHost;
        let combo = BUTTON_L | BUTTON_R | BUTTON_Z;
        let mut black_frames = 0;
        for _ in 0..60 {
            let report = runtime.frame(&mut host, [combo, 0, 0, 0]).expect("frame");
            assert_eq!(report.action, FrameAction::PresentBlack);
            black_frames += 1;
        }
        assert_eq!(black_frames, 60);
        assert_eq!(runtime.state(), RuntimeState::Fading { remaining: 0 });
        let report = runtime.frame(&mut host, [0; PAD_COUNT]).expect("return");
        assert_eq!(report.action, FrameAction::ReturnToGame);
        assert_eq!(runtime.state(), RuntimeState::Exited);
    }

    fn fixture_rom() -> Vec<u8> {
        let mut rom = vec![0; 16 + 0x4000];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        let prg = 16;
        rom[prg..prg + 9].copy_from_slice(&[0xa9, 1, 0x8d, 0, 0, 0xe8, 0x4c, 0x05, 0x80]);
        rom[prg + 0x3ffc] = 0;
        rom[prg + 0x3ffd] = 0x80;
        rom
    }
}
