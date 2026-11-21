use crate::adapters::HostErrorCode;
use crate::runtime::Lifecycle;

/// A bounded operation identifier used for host failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    SetScreenSize,
    InitialFrame,
    InitializeCard,
    CreateIrqManager,
    CreateInputManager,
    RuntimeFrame,
    InitializeDisplay,
    InitializeMessages,
    EnableLandStart,
    MountArchive,
    DumpAramHeap,
    ReadClock,
    PrepareFrame,
    PollInput,
    DrawFirst,
    GameTime,
    Execute,
    Bgm,
    FinishFrame,
    SubmitFrame,
    AudioTick,
    Reset,
    ResizeGameMemory,
    GameInit,
    GameCleanup,
    AudioInit,
    AudioCleanup,
    RendererShutdown,
    PlatformShutdown,
}

/// Which registered callback failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackKind {
    GameInit,
    Input,
    Time,
    Update,
    Execute,
    Bgm,
    GameCleanup,
    Reset,
    Prenmi,
    Resize,
    Shutdown,
    Constructor,
    Main,
    Destructor,
    Unresolved,
}

/// Bounded error returned by the runtime boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeError {
    InvalidState {
        expected: Lifecycle,
        actual: Lifecycle,
    },
    InvalidInput,
    Host {
        operation: Operation,
        code: HostErrorCode,
    },
    Callback(CallbackKind),
    ArithmeticOverflow,
    GameStopped,
}

impl RuntimeError {
    pub(crate) fn invalid_state(expected: Lifecycle, actual: Lifecycle) -> Self {
        Self::InvalidState { expected, actual }
    }
}
