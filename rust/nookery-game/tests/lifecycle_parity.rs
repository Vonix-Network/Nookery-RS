use nookery_game::{
    FirstGameInitializer, GAME_HYRAL_SIZE, GAME_SECOND_SIZE, Game, GameGlobals, GameLifecycleHooks,
    GameVersion, GraphState, LifecycleError, LifecyclePhase, NextGame, SaveChannelCallback,
};

// Independent oracle constants copied from include/game.h and
// include/second_game.h, rather than imported from nookery_game.
const ORACLE_HYRAL_SIZE: usize = 0x100000;
const ORACLE_GAME_FRAME: i32 = 1;
const ORACLE_SECOND_SIZE: usize = 0xE0;

#[derive(Default)]
struct Trace {
    events: Vec<String>,
    allocation_succeeds: bool,
}

impl Trace {
    fn event(&mut self, name: &str) {
        self.events.push(name.to_owned());
    }
}

impl GameLifecycleHooks for Trace {
    fn mcon_ct(&mut self) {
        self.event("mCon_ct");
    }

    fn mcon_dt(&mut self) {
        self.event("mCon_dt");
    }

    fn gamealloc_init(&mut self) {
        self.event("gamealloc_init");
    }

    fn gamealloc_malloc(&mut self, size: usize) -> bool {
        self.events.push(format!("gamealloc_malloc({size:#x})"));
        self.allocation_succeeds
    }

    fn gamealloc_cleanup(&mut self) {
        self.event("gamealloc_cleanup");
    }

    fn tha_ct(&mut self, size: usize) {
        self.events.push(format!("THA_ct({size:#x})"));
    }

    fn tha_dt(&mut self) {
        self.event("THA_dt");
    }

    fn display_set_frame_rate(&mut self, frame: i32) {
        self.events
            .push(format!("JC_JFWDisplay_setFrameRate({frame})"));
    }

    fn gfxprint_enable_highres(&mut self) {
        self.event("gfxprint_highres");
    }

    fn mbgm_init(&mut self) {
        self.event("mBGM_init");
    }

    fn mbgm_cleanup(&mut self) {
        self.event("mBGM_cleanup");
    }

    fn mbgm_ct(&mut self) {
        self.event("mBGM_ct");
    }

    fn mvibctl_init(&mut self) {
        self.event("mVibctl_init");
    }

    fn mvibctl_cleanup(&mut self) {
        self.event("mVibctl_cleanup");
    }

    fn mvibctl_ct(&mut self) {
        self.event("mVibctl_ct");
    }

    fn gba_init(&mut self) {
        self.event("GBAInit");
    }

    fn sys_romcheck(&mut self) {
        self.event("sys_romcheck");
    }

    fn mcd_save_data_aram_malloc(&mut self) {
        self.event("mCD_save_data_aram_malloc");
    }

    fn famicom_set_save_channel(&mut self, callback: SaveChannelCallback) {
        assert_eq!(callback, SaveChannelCallback::McdGetThisLandSlotNo);
        self.event("famicom_setCallback_getSaveChan");
    }

    fn common_data_init(&mut self) {
        self.event("common_data_init");
    }

    fn first_game_cleanup(&mut self) {
        self.event("first_game_cleanup");
    }
}

fn oracle_first_game_ct_events() -> Vec<String> {
    vec![
        "mCon_ct".into(),
        "gamealloc_init".into(),
        format!("gamealloc_malloc({ORACLE_HYRAL_SIZE:#x})"),
        format!("THA_ct({ORACLE_HYRAL_SIZE:#x})"),
        format!("JC_JFWDisplay_setFrameRate({ORACLE_GAME_FRAME})"),
        "gfxprint_highres".into(),
        "sys_romcheck".into(),
        "mCD_save_data_aram_malloc".into(),
        "famicom_setCallback_getSaveChan".into(),
        "mBGM_ct".into(),
        "mVibctl_ct".into(),
        "common_data_init".into(),
        "mBGM_init".into(),
        "GBAInit".into(),
        "mVibctl_init".into(),
    ]
}

#[test]
fn mutation_game_constant_and_callback_order_are_constrained() {
    let mut graph = GraphState::default();
    let mut globals = GameGlobals::default();
    let mut hooks = Trace {
        allocation_succeeds: true,
        ..Trace::default()
    };
    let mut game = Game::default();

    game.ct(
        &mut graph,
        &mut globals,
        GameVersion::Gafe01_00,
        FirstGameInitializer,
        &mut hooks,
    )
    .unwrap();

    assert_eq!(hooks.events, oracle_first_game_ct_events());
    assert!(graph.need_viupdate);
    assert!(game.graph_attached);
    assert_eq!(game.frame_counter, 0);
    assert!(game.pad_initialized);
    assert!(!game.exec_installed);
    assert!(!game.doing);
    assert_eq!(game.next_game, Some(NextGame::Second));
    assert_eq!(game.next_game_class_size, ORACLE_SECOND_SIZE);
    assert_eq!(game.phase(), LifecyclePhase::Active);
    assert_eq!(game.tha_size, ORACLE_HYRAL_SIZE);
    assert_eq!(globals.game_frame, ORACLE_GAME_FRAME as u8);
    assert_eq!(globals.game_frame_f, 1.0);
    assert_eq!(globals.game_frame_2f, 0.5);
    assert_eq!(globals.game_frame_inverse, 1.0);

    // This deliberately fixes the oracle's constant and callback order. A
    // mutation of 0x100000, 0xE0, or the mBGM/mVibctl transition order fails.
    assert_eq!(GAME_HYRAL_SIZE, ORACLE_HYRAL_SIZE);
    assert_eq!(GAME_SECOND_SIZE, ORACLE_SECOND_SIZE);
}

#[test]
fn game_dt_preserves_source_cleanup_order_and_invokes_empty_first_cleanup() {
    let mut graph = GraphState::default();
    let mut globals = GameGlobals::default();
    let mut hooks = Trace {
        allocation_succeeds: true,
        ..Trace::default()
    };
    let mut game = Game::default();
    game.ct(
        &mut graph,
        &mut globals,
        GameVersion::Gafu01_00,
        FirstGameInitializer,
        &mut hooks,
    )
    .unwrap();

    hooks.events.clear();
    game.dt(&mut hooks).unwrap();

    assert_eq!(
        hooks.events,
        [
            "mCon_dt",
            "mBGM_cleanup",
            "first_game_cleanup",
            "mVibctl_cleanup",
            "THA_dt",
            "gamealloc_cleanup",
        ]
    );
    assert!(!game.current_game);
    assert_eq!(game.phase(), LifecyclePhase::Cleaned);
}

#[test]
fn hyral_failure_keeps_null_arena_state_and_stops_before_init_callback() {
    let mut graph = GraphState::default();
    let mut globals = GameGlobals::default();
    let mut hooks = Trace::default();
    let mut game = Game::default();

    assert_eq!(
        game.ct(
            &mut graph,
            &mut globals,
            GameVersion::Gafe01_00,
            FirstGameInitializer,
            &mut hooks,
        ),
        Err(LifecycleError::HyralAllocationFailed {
            requested: ORACLE_HYRAL_SIZE,
        })
    );
    assert_eq!(
        hooks.events,
        [
            "mCon_ct",
            "gamealloc_init",
            "gamealloc_malloc(0x100000)",
            "THA_ct(0x0)",
        ]
    );
    assert!(graph.need_viupdate);
    assert!(game.current_game);
    assert!(game.doing);
    assert!(!game.hyral_allocated);
    assert_eq!(game.tha_size, 0);
    assert_eq!(game.phase(), LifecyclePhase::AllocationFailed);
}

#[test]
fn repeated_lifecycle_calls_are_typed_failures_at_the_unsafe_boundaries() {
    let mut graph = GraphState::default();
    let mut globals = GameGlobals::default();
    let mut hooks = Trace {
        allocation_succeeds: true,
        ..Trace::default()
    };
    let mut game = Game::default();
    game.ct(
        &mut graph,
        &mut globals,
        GameVersion::Gafu01_00,
        FirstGameInitializer,
        &mut hooks,
    )
    .unwrap();
    game.dt(&mut hooks).unwrap();

    assert_eq!(
        game.dt(&mut hooks),
        Err(LifecycleError::InvalidPhase {
            operation: "destruct",
            actual: LifecyclePhase::Cleaned,
        })
    );
}
