use crate::error::{CallbackKind, RuntimeError};

type Hook = Box<dyn FnMut() -> Result<(), RuntimeError>>;
type UnresolvedHook = Box<dyn FnMut(UnresolvedReport) -> Result<(), RuntimeError>>;

/// One bounded stack-frame record supplied by a host diagnostic adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackFrame {
    pub address: u32,
    pub back_chain: u32,
    pub link_register: u32,
}

/// The unresolved-function report is capped at the sixteen frames used by C.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedReport {
    pub frames: Vec<StackFrame>,
}

/// Executor result counters, useful for call-path verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExecutorReport {
    pub constructors_run: usize,
    pub main_run: bool,
    pub destructors_run: usize,
}

/// Rust boundary for _prolog, _epilog, and _unresolved.
pub struct Executor {
    construct_skip: bool,
    constructors: Vec<Hook>,
    main: Option<Hook>,
    destructors: Vec<Hook>,
    unresolved: Option<UnresolvedHook>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorError {
    Callback(CallbackKind),
    MissingMain,
}

impl Executor {
    pub fn new(construct_skip: bool) -> Self {
        Self {
            construct_skip,
            constructors: Vec::new(),
            main: None,
            destructors: Vec::new(),
            unresolved: None,
        }
    }

    pub fn register_constructor<F>(&mut self, hook: F)
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        self.constructors.push(Box::new(hook));
    }

    pub fn register_main<F>(&mut self, hook: F)
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        self.main = Some(Box::new(hook));
    }

    pub fn register_destructor<F>(&mut self, hook: F)
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        self.destructors.push(Box::new(hook));
    }

    pub fn register_unresolved<F>(&mut self, hook: F)
    where
        F: FnMut(UnresolvedReport) -> Result<(), RuntimeError> + 'static,
    {
        self.unresolved = Some(Box::new(hook));
    }

    pub fn prolog(&mut self) -> Result<ExecutorReport, ExecutorError> {
        let mut report = ExecutorReport::default();
        if !self.construct_skip {
            for constructor in &mut self.constructors {
                constructor().map_err(|_| ExecutorError::Callback(CallbackKind::Constructor))?;
                report.constructors_run += 1;
            }
        }
        let main = self.main.as_mut().ok_or(ExecutorError::MissingMain)?;
        main().map_err(|_| ExecutorError::Callback(CallbackKind::Main))?;
        report.main_run = true;
        Ok(report)
    }

    pub fn epilog(&mut self) -> Result<ExecutorReport, ExecutorError> {
        let mut report = ExecutorReport::default();
        for destructor in &mut self.destructors {
            destructor().map_err(|_| ExecutorError::Callback(CallbackKind::Destructor))?;
            report.destructors_run += 1;
        }
        Ok(report)
    }

    pub fn unresolved(&mut self, frames: &[StackFrame]) -> Result<(), ExecutorError> {
        let Some(unresolved) = self.unresolved.as_mut() else {
            return Ok(());
        };
        let report = UnresolvedReport {
            frames: frames.iter().copied().take(16).collect(),
        };
        unresolved(report).map_err(|_| ExecutorError::Callback(CallbackKind::Unresolved))
    }
}
