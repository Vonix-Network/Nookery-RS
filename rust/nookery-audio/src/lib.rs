//! Bounded audio handoff primitives for the PC audio boundary.
//!
//! This crate deliberately does not open or claim an audio device.  The C
//! implementation produces 32 kHz, interleaved stereo `s16` data and hands it
//! to SDL; this crate models the bounded handoff before a backend is attached.

/// The sample rate used by `pc/src/pc_audio.c`.
pub const SAMPLE_RATE_HZ: u32 = 32_000;

/// One interleaved stereo frame: left sample followed by right sample.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StereoFrame {
    pub left: i16,
    pub right: i16,
}

impl StereoFrame {
    pub const SILENCE: Self = Self { left: 0, right: 0 };
}

/// Accounting for one bounded push into a [`StereoRing`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PushReport {
    pub accepted_frames: usize,
    pub dropped_frames: usize,
}

/// A fixed-capacity FIFO for 32 kHz interleaved stereo frames.
///
/// `push` never overwrites frames already in the ring: when there is not
/// enough room, the newest incoming frames are dropped. `pop` always writes
/// the requested number of frames, supplying silence after the FIFO empties.
/// Both loss paths are accounted for cumulatively.
#[derive(Debug)]
pub struct StereoRing {
    frames: Vec<StereoFrame>,
    read_index: usize,
    write_index: usize,
    len: usize,
    underflow_frames: u64,
    dropped_frames: u64,
}

impl StereoRing {
    /// Creates a ring with exactly `capacity_frames` slots.
    pub fn new(capacity_frames: usize) -> Self {
        Self {
            frames: vec![StereoFrame::SILENCE; capacity_frames],
            read_index: 0,
            write_index: 0,
            len: 0,
            underflow_frames: 0,
            dropped_frames: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.frames.len()
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == self.frames.len()
    }

    pub const fn underflow_frames(&self) -> u64 {
        self.underflow_frames
    }

    pub const fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Pushes as many frames as fit and drops the newest frames that do not.
    pub fn push(&mut self, input: &[StereoFrame]) -> PushReport {
        let available = self.capacity().saturating_sub(self.len);
        let accepted_frames = input.len().min(available);
        let dropped_frames = input.len() - accepted_frames;

        for &frame in &input[..accepted_frames] {
            if self.capacity() != 0 {
                self.frames[self.write_index] = frame;
                self.write_index = (self.write_index + 1) % self.capacity();
            }
        }
        self.len += accepted_frames;
        self.dropped_frames += dropped_frames as u64;

        PushReport {
            accepted_frames,
            dropped_frames,
        }
    }

    /// Pops exactly `output.len()` frames, padding with silence on underflow.
    pub fn pop(&mut self, output: &mut [StereoFrame]) {
        let available = output.len().min(self.len);

        for slot in &mut output[..available] {
            *slot = self.frames[self.read_index];
            self.read_index = (self.read_index + 1) % self.capacity();
        }
        self.len -= available;

        let underflow_frames = output.len() - available;
        for slot in &mut output[available..] {
            *slot = StereoFrame::SILENCE;
        }
        self.underflow_frames += underflow_frames as u64;
    }
}

/// Commands crossing the audio producer/consumer ownership boundary.
///
/// These are intentionally typed equivalents of the source's audio messages:
/// update the DAC, process DSP synchronization, and complete NEOS
/// synchronization. Lifecycle commands make reset and shutdown explicit to a
/// caller that owns the queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioCommand {
    UpdateDac,
    DspSync,
    NeosSync,
    StartDma,
    StopDma,
    SetPaused(bool),
    SetMasterVolume(u8),
    Reset,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueState {
    Open,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    Full,
    Empty,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueueStats {
    pub dropped_commands: u64,
    pub cleared_commands: u64,
}

/// A fixed-capacity, non-blocking FIFO for typed audio commands.
///
/// `try_push` and `try_pop` never wait. A full queue rejects the new command,
/// matching `Z_osSendMesg(..., OS_MESG_NOBLOCK)` in the source. Shutdown
/// clears pending commands and rejects future commands until `reset` is called.
#[derive(Debug)]
pub struct AudioCommandQueue {
    commands: Vec<Option<AudioCommand>>,
    read_index: usize,
    write_index: usize,
    len: usize,
    state: QueueState,
    stats: QueueStats,
}

impl AudioCommandQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            commands: vec![None; capacity],
            read_index: 0,
            write_index: 0,
            len: 0,
            state: QueueState::Open,
            stats: QueueStats::default(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.commands.len()
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn state(&self) -> QueueState {
        self.state
    }

    pub const fn stats(&self) -> QueueStats {
        self.stats
    }

    pub fn try_push(&mut self, command: AudioCommand) -> Result<(), QueueError> {
        if self.state == QueueState::Shutdown {
            return Err(QueueError::Shutdown);
        }
        if self.len == self.capacity() {
            self.stats.dropped_commands += 1;
            return Err(QueueError::Full);
        }

        if self.capacity() != 0 {
            self.commands[self.write_index] = Some(command);
            self.write_index = (self.write_index + 1) % self.capacity();
        }
        self.len += 1;
        Ok(())
    }

    pub fn try_pop(&mut self) -> Result<AudioCommand, QueueError> {
        if self.len == 0 {
            return Err(if self.state == QueueState::Shutdown {
                QueueError::Shutdown
            } else {
                QueueError::Empty
            });
        }

        let command = self.commands[self.read_index]
            .take()
            .expect("a non-empty queue slot must contain a command");
        self.read_index = (self.read_index + 1) % self.capacity();
        self.len -= 1;
        Ok(command)
    }

    /// Transitions to shutdown and discards all pending commands.
    pub fn shutdown(&mut self) {
        self.clear_pending();
        self.state = QueueState::Shutdown;
    }

    /// Reopens the queue and clears any stale state without allocating.
    pub fn reset(&mut self) {
        self.clear_pending();
        self.state = QueueState::Open;
    }

    fn clear_pending(&mut self) {
        for slot in &mut self.commands {
            if slot.take().is_some() {
                self.stats.cleared_commands += 1;
            }
        }
        self.read_index = 0;
        self.write_index = 0;
        self.len = 0;
    }
}
