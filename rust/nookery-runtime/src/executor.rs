use crate::error::{CallbackKind, RuntimeError};
use nookery_game::ExecutorPlan;
use std::cell::Cell;

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
        let main = self.main.as_mut().ok_or(ExecutorError::MissingMain)?;
        let error = Cell::new(None);
        let constructors_run = Cell::new(0);
        let main_run = Cell::new(false);
        let error_ref = &error;
        let main_run_ref = &main_run;
        let mut plan = ExecutorPlan::new(move || {
            if error_ref.get().is_none() {
                if main().is_err() {
                    error_ref.set(Some(ExecutorError::Callback(CallbackKind::Main)));
                } else {
                    main_run_ref.set(true);
                }
            }
        });
        for constructor in &mut self.constructors {
            let error_ref = &error;
            let constructors_run_ref = &constructors_run;
            plan = plan.constructor(move || {
                if error_ref.get().is_none() {
                    if constructor().is_err() {
                        error_ref.set(Some(ExecutorError::Callback(CallbackKind::Constructor)));
                    } else {
                        constructors_run_ref.set(constructors_run_ref.get() + 1);
                    }
                }
            });
        }
        plan.run(self.construct_skip);
        if let Some(error) = error.get() {
            Err(error)
        } else {
            Ok(ExecutorReport {
                constructors_run: constructors_run.get(),
                main_run: main_run.get(),
                destructors_run: 0,
            })
        }
    }

    pub fn epilog(&mut self) -> Result<ExecutorReport, ExecutorError> {
        let error = Cell::new(None);
        let destructors_run = Cell::new(0);
        let mut plan = ExecutorPlan::new(|| {});
        for destructor in &mut self.destructors {
            let error_ref = &error;
            let destructors_run_ref = &destructors_run;
            plan = plan.destructor(move || {
                if error_ref.get().is_none() {
                    if destructor().is_err() {
                        error_ref.set(Some(ExecutorError::Callback(CallbackKind::Destructor)));
                    } else {
                        destructors_run_ref.set(destructors_run_ref.get() + 1);
                    }
                }
            });
        }
        plan.run(true);
        if let Some(error) = error.get() {
            Err(error)
        } else {
            Ok(ExecutorReport {
                constructors_run: 0,
                main_run: false,
                destructors_run: destructors_run.get(),
            })
        }
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
