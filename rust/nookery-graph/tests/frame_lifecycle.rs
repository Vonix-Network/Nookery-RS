use nookery_graph::{Graph, GraphDoingPoint, GraphLifecycle, GraphLifecycleError};

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
fn display_failure_does_not_commit_task_or_frame_counter() {
    let mut graph = Graph::default();
    graph.begin().unwrap();
    graph.proc(1.0 / 60.0).unwrap();

    // This is the source branch after graph_draw_finish reports an error:
    // GAME_MAIN_FINISHED is observable, but TASK_SET and the frame counter
    // are not committed.
    graph.finish_frame(false).unwrap();
    assert_eq!(graph.state.doing_point, GraphDoingPoint::GameMainFinished);
    assert_eq!(graph.state.frame_counter, 0);
    assert_eq!(graph.state.lifecycle(), GraphLifecycle::Running);

    // Mutation guard: a later successful frame commits exactly one task set
    // and one graph frame, rather than replaying the failed frame.
    graph.proc(1.0 / 60.0).unwrap();
    graph.finish_frame(true).unwrap();
    assert_eq!(graph.state.doing_point, GraphDoingPoint::TaskSetFinished);
    assert_eq!(graph.state.frame_counter, 1);
}
