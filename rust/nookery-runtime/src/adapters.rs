use crate::error::RuntimeError;

/// Finite host failure codes keep propagation bounded at the Rust/C boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostErrorCode {
    Unavailable,
    Failed,
    InvalidInput,
    Unsupported,
}

/// Host services required by the PC single-thread application path.
///
/// Default methods are deliberate no-ops for a small adapter: each maps to a
/// named C boot seam and can be overridden by the parent application. Clock
/// access is the sole required method because tick must be driven by a host
/// time source; deterministic callers can use run_frame instead.
pub trait PlatformAdapter {
    fn set_screen_size(&mut self, _width: u32, _height: u32) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn frame_begin(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn frame_end(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn initialize_card(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn create_irq_manager(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn create_input_manager(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn initialize_display(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn initialize_messages(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn enable_land_start(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn mount_archive(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn dump_aram_heap(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn monotonic_seconds(&mut self) -> Result<f64, HostErrorCode> {
        Err(HostErrorCode::Unavailable)
    }
    fn reset_requested(&mut self) -> Result<bool, HostErrorCode> {
        Ok(false)
    }

    /// The C game_resize_hyral allocator seam. The adapter owns allocation;
    /// the runtime only preserves the signed request and returned size.
    fn resize_game_memory(&mut self, _requested_size: i64) -> Result<usize, HostErrorCode> {
        Err(HostErrorCode::Unsupported)
    }

    fn shutdown(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
}

/// Renderer boundary corresponding to graph display-list preparation,
/// first draw, display-list finish, and task submission.
pub trait RendererAdapter {
    fn prepare_frame(
        &mut self,
        _context: &crate::runtime::FrameContext,
    ) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn draw_first(&mut self, _context: &crate::runtime::FrameContext) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn finish_frame(
        &mut self,
        _context: &crate::runtime::FrameContext,
    ) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn submit_frame(
        &mut self,
        _context: &crate::runtime::FrameContext,
    ) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn shutdown(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
}

/// Audio boundary corresponding to BGM initialization/update/cleanup and the
/// 60 Hz game-audio ticks in graph_audio_gameframe.
pub trait AudioAdapter {
    fn initialize(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn bgm_update(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn game_tick(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn reset(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn cleanup(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
    fn shutdown(&mut self) -> Result<(), HostErrorCode> {
        Ok(())
    }
}

pub(crate) fn host_error(operation: crate::error::Operation, code: HostErrorCode) -> RuntimeError {
    RuntimeError::Host { operation, code }
}
