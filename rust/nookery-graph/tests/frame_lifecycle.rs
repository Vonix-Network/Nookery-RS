use nookery_graph::{
    graph_dt_60hz_ticks, graph_dt_frame_phase, graph_dt_frame_time, graph_dt_period_elapsed,
    Executor, ExecutorCallbackPhase, ExecutorCallbacks, ExecutorError, ExecutorLifecycle, Graph,
    GraphCallbackStage, GraphDoingPoint, GraphExecutionError, GraphFrameCallbacks, GraphLifecycle,
    GraphLifecycleError, GRAPH_BASE_FRAME_RATE,
};

// SOURCE_PARITY (machine-readable by inspection):
// graph.c:105-121  graph_ct -> zero/60Hz/total defaults and CT point
// graph.c:124-137  graph_dt_60hz_ticks -> f32 accumulation, truncation, remainder, cap 4
// graph.c:140-148  graph_dt_period_elapsed -> threshold, one subtraction, >4-period reset
// graph.c:150-160  frame time/phase -> total, non-positive guard, integer remainder
// executor.c:11-21 _prolog -> optional forward _ctors then main
// executor.c:23-30 _epilog -> forward _dtors
// executor.c:32-44 _unresolved -> explicit reporting callback boundary

#[test]
fn frame_lifecycle_preserves_source_order_and_counter() {
    let mut graph = Graph::default();

    assert_eq!(graph.state.lifecycle(), GraphLifecycle::New);
    graph.begin().unwrap();
    assert_eq!(graph.state.doing_point, GraphDoingPoint::Ct);
    assert_eq!(graph.state.frame_counter, 0);

    graph.proc(1.0 / 60.0).unwrap();
    assert_eq!(graph.state.doing_point, GraphDoingPoint::GameMain);
    assert_eq!(graph.state.frame_counter, 0);
    assert_eq!(graph.state.lifecycle(), GraphLifecycle::Processing);

    graph.finish_frame(true).unwrap();
    assert_eq!(graph.state.doing_point, GraphDoingPoint::TaskSetFinished);
    assert_eq!(graph.state.frame_counter, 1);
    assert_eq!(graph.state.lifecycle(), GraphLifecycle::Running);

    graph.end().unwrap();
    assert_eq!(graph.state.doing_point, GraphDoingPoint::Dt);
    assert_eq!(graph.state.lifecycle(), GraphLifecycle::CleanedUp);

    // Mutation-sensitive: cleanup does not rewrite the source counter, and a
    // second end cannot silently reorder or duplicate the transition.
    assert_eq!(graph.state.frame_counter, 1);
    assert_eq!(graph.end(), Err(GraphLifecycleError::EndAlreadyComplete));
    assert_eq!(graph.state.frame_counter, 1);
}

#[test]
fn clock_accumulates_60fps_units_without_loss() {
    let mut graph = Graph::default();
    graph.begin().unwrap();

    for _ in 0..3 {
        graph.proc(1.0 / 120.0).unwrap();
        graph.finish_frame(true).unwrap();
    }

    assert_eq!(graph.clock.dt, 1.0 / 120.0);
    assert_eq!(graph.clock.dt_num_60fps_frames, 0.5);
    assert_eq!(graph.clock.dt_total_60fps_frames, 1.5);

    // The source's >4-frame stall clamp is part of the clock seam.
    graph.proc(1.0).unwrap();
    graph.finish_frame(true).unwrap();
    assert_eq!(graph.clock.dt_num_60fps_frames, 4.0);
    assert_eq!(graph.clock.dt, 4.0 / 60.0);
    assert_eq!(graph.clock.dt_total_60fps_frames, 5.5);
}

#[test]
fn invalid_transition_is_rejected() {
    let mut graph = Graph::default();

    assert_eq!(
        graph.proc(1.0 / 60.0),
        Err(GraphLifecycleError::ProcessBeforeBegin)
    );
    assert_eq!(
        graph.finish_frame(true),
        Err(GraphLifecycleError::FinishBeforeProcess)
    );
    assert_eq!(graph.end(), Err(GraphLifecycleError::EndBeforeBegin));

    graph.begin().unwrap();
    assert_eq!(graph.begin(), Err(GraphLifecycleError::BeginWhileRunning));
    graph.proc(1.0 / 60.0).unwrap();
    assert_eq!(
        graph.proc(1.0 / 60.0),
        Err(GraphLifecycleError::ProcessWhileProcessing)
    );
    assert_eq!(graph.end(), Err(GraphLifecycleError::EndWhileProcessing));
    assert_eq!(graph.state.frame_counter, 0);

    graph.finish_frame(false).unwrap();
    assert_eq!(graph.state.doing_point, GraphDoingPoint::GameMainFinished);
    graph.end().unwrap();
    assert_eq!(graph.begin(), Err(GraphLifecycleError::BeginAfterCleanup));
    graph.reset();
    graph.begin().unwrap();
    assert_eq!(graph.state.frame_counter, 0);
}

#[test]
fn graph_ct_defaults_and_reset_match_source_fields() {
    let mut graph = Graph::default();
    assert_eq!(graph.clock.dt, 1.0 / GRAPH_BASE_FRAME_RATE);
    assert_eq!(graph.clock.dt_num_60fps_frames, 1.0);
    assert_eq!(graph.clock.dt_total_60fps_frames, 0.0);
    assert_eq!(graph.state.frame_counter, 0);
    assert_eq!(graph.state.doing_point, GraphDoingPoint::Zero);

    graph.begin().unwrap();
    graph.proc(0.25).unwrap();
    graph.finish_frame(false).unwrap();
    graph.reset();
    assert_eq!(graph.clock.dt, 1.0 / GRAPH_BASE_FRAME_RATE);
    assert_eq!(graph.clock.dt_num_60fps_frames, 1.0);
    assert_eq!(graph.clock.dt_total_60fps_frames, 0.0);
    assert_eq!(graph.state.frame_counter, 0);
}

#[test]
fn dt_ticks_preserve_fractional_accumulation_and_cap_four() {
    let mut accum = 0.0_f32;
    assert_eq!(graph_dt_60hz_ticks(0.25, &mut accum), 0);
    assert_eq!(graph_dt_60hz_ticks(0.25, &mut accum), 0);
    assert_eq!(graph_dt_60hz_ticks(0.5, &mut accum), 1);
    assert_eq!(accum, 0.0);

    accum = 0.5;
    assert_eq!(graph_dt_60hz_ticks(6.0, &mut accum), 4);
    assert_eq!(accum, 0.5); // cap occurs after truncation/remainder subtraction
}

#[test]
fn period_elapsed_honors_threshold_and_stale_accumulator_reset() {
    let mut accum = 0.75_f32;
    assert_eq!(graph_dt_period_elapsed(0.24, &mut accum, 1.0), 0);
    assert!(accum < 1.0);
    assert_eq!(graph_dt_period_elapsed(0.01, &mut accum, 1.0), 1);
    assert_eq!(accum, 0.0); // exactly one period after f32 addition

    accum = 0.0;
    assert_eq!(graph_dt_period_elapsed(4.2, &mut accum, 1.0), 1);
    assert!((accum - 3.2).abs() <= 3.0 * f32::EPSILON); // below the > period * 4 reset

    accum = 0.0;
    assert_eq!(graph_dt_period_elapsed(5.1, &mut accum, 1.0), 1);
    assert_eq!(accum, 0.0); // > period * 4 resets after one subtraction
}

#[test]
fn frame_time_and_phase_handle_invalid_periods_and_large_totals() {
    let mut graph = Graph::default();
    graph.clock.dt_total_60fps_frames = 1_000_000_003.75;
    assert_eq!(graph_dt_frame_time(&graph.clock), 1_000_000_003.75);
    assert_eq!(graph_dt_frame_phase(&graph.clock, 0), 0);
    assert_eq!(graph_dt_frame_phase(&graph.clock, -7), 0);
    assert_eq!(graph_dt_frame_phase(&graph.clock, 10), 3);

    graph.clock.dt_total_60fps_frames = -13.75;
    assert_eq!(graph_dt_frame_phase(&graph.clock, 5), -3);
}

struct FrameHooks {
    events: Vec<&'static str>,
    fail_at: Option<&'static str>,
}

impl FrameHooks {
    fn event(&mut self, event: &'static str) -> Result<(), &'static str> {
        self.events.push(event);
        if self.fail_at == Some(event) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

impl GraphFrameCallbacks for FrameHooks {
    type Error = &'static str;

    fn game_main(&mut self) -> Result<(), Self::Error> {
        self.event("main")
    }

    fn draw_finish(&mut self) -> Result<bool, Self::Error> {
        self.event("draw")?;
        Ok(true)
    }

    fn task_set(&mut self) -> Result<(), Self::Error> {
        self.event("task")
    }

    fn wait_task(&mut self) -> Result<(), Self::Error> {
        self.event("wait")
    }
}

#[test]
fn graph_callback_order_and_error_surface_are_explicit() {
    let mut graph = Graph::default();
    graph.begin().unwrap();
    let mut hooks = FrameHooks {
        events: Vec::new(),
        fail_at: None,
    };
    graph.run_frame(1.0 / 60.0, &mut hooks).unwrap();
    assert_eq!(hooks.events, ["main", "draw", "task", "wait"]);
    assert_eq!(graph.state.frame_counter, 1);

    graph.reset();
    graph.begin().unwrap();
    let mut hooks = FrameHooks {
        events: Vec::new(),
        fail_at: Some("draw"),
    };
    assert_eq!(
        graph.run_frame(1.0 / 60.0, &mut hooks),
        Err(GraphExecutionError::Callback {
            stage: GraphCallbackStage::DrawFinish,
            source: "draw",
        })
    );
    assert_eq!(hooks.events, ["main", "draw"]);
}

struct ExecutorHooks {
    events: Vec<String>,
    fail_at: Option<String>,
}

impl ExecutorHooks {
    fn event(&mut self, event: String) -> Result<(), String> {
        if self.fail_at.as_ref() == Some(&event) {
            self.events.push(event.clone());
            Err(event)
        } else {
            self.events.push(event);
            Ok(())
        }
    }
}

impl ExecutorCallbacks for ExecutorHooks {
    type Error = String;

    fn constructor(&mut self, index: usize) -> Result<(), Self::Error> {
        self.event(format!("ctor{index}"))
    }

    fn main(&mut self) -> Result<(), Self::Error> {
        self.event("main".to_string())
    }

    fn destructor(&mut self, index: usize) -> Result<(), Self::Error> {
        self.event(format!("dtor{index}"))
    }

    fn unresolved(&mut self) -> Result<(), Self::Error> {
        self.event("unresolved".to_string())
    }
}

#[test]
fn executor_construct_skip_order_main_and_forward_dtors_match_source() {
    let mut hooks = ExecutorHooks {
        events: Vec::new(),
        fail_at: None,
    };
    let mut executor = Executor::new(false, 3, 3);
    executor.prolog(&mut hooks).unwrap();
    executor.epilog(&mut hooks).unwrap();
    assert_eq!(
        hooks.events,
        ["ctor0", "ctor1", "ctor2", "main", "dtor0", "dtor1", "dtor2"]
    );
    assert_eq!(executor.state.lifecycle(), ExecutorLifecycle::Exited);

    let mut hooks = ExecutorHooks {
        events: Vec::new(),
        fail_at: None,
    };
    let mut skipped = Executor::new(true, 3, 0);
    skipped.prolog(&mut hooks).unwrap();
    assert_eq!(hooks.events, ["main"]);
    assert!(!skipped.state.constructors_called);

    let mut hooks = ExecutorHooks {
        events: Vec::new(),
        fail_at: None,
    };
    let mut reverse_registration = Executor::with_destructor_order(false, 0, vec![2, 1, 0]);
    reverse_registration.prolog(&mut hooks).unwrap();
    reverse_registration.epilog(&mut hooks).unwrap();
    assert_eq!(hooks.events, ["main", "dtor2", "dtor1", "dtor0"]);
}

#[test]
fn executor_callback_errors_and_unresolved_are_typed() {
    let mut hooks = ExecutorHooks {
        events: Vec::new(),
        fail_at: Some("ctor1".to_string()),
    };
    let mut executor = Executor::new(false, 3, 2);
    assert_eq!(
        executor.prolog(&mut hooks),
        Err(ExecutorError::Callback {
            phase: ExecutorCallbackPhase::Constructor,
            index: Some(1),
            source: "ctor1".to_string(),
        })
    );
    assert_eq!(hooks.events, ["ctor0", "ctor1"]);
    assert_eq!(executor.state.lifecycle(), ExecutorLifecycle::Failed);

    let mut hooks = ExecutorHooks {
        events: Vec::new(),
        fail_at: Some("unresolved".to_string()),
    };
    let executor = Executor::new(true, 0, 0);
    assert_eq!(
        executor.unresolved(&mut hooks),
        Err(ExecutorError::Callback {
            phase: ExecutorCallbackPhase::Unresolved,
            index: None,
            source: "unresolved".to_string(),
        })
    );
}
