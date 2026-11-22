//! Source-located differential vectors for `src/actor/ac_balloon.c`.
//!
//! These vectors compare the Rust lifecycle trace and state against the
//! production oracle's literal ordering: request/setup/main in
//! `ac_balloon.c:44-143`, movement in `:100-104`, and rendering callbacks in
//! `:149-183`/`:217-223`. The C actor is not independently linkable without
//! the game's engine; the expected trace is therefore fixed from those source
//! statements rather than from a duplicate C wrapper.

use nookery_actor::{
    AnimationInit, BalloonActor, BalloonCallbacks, JointRenderCallback, Position, ShortAngles,
};

#[derive(Default)]
struct Trace {
    player: Option<Position>,
    events: Vec<String>,
}

impl BalloonCallbacks for Trace {
    fn player_position(&mut self) -> Option<Position> {
        self.player
    }

    fn keyframe_construct(&mut self, skeleton_id: u16, joint_count: usize) {
        self.events
            .push(format!("construct:{skeleton_id}:{joint_count}"));
    }

    fn keyframe_init(&mut self, init: AnimationInit) {
        self.events.push(format!(
            "init:{}:{:.1}:{:.1}:{:.1}",
            init.animation_id, init.start_frame, init.speed, init.morph
        ));
    }

    fn keyframe_play(&mut self) {
        self.events.push("play".to_string());
    }

    fn joint_render_callback(&mut self, callback: JointRenderCallback) {
        self.events.push(format!(
            "draw:{}:{}",
            callback.joint, callback.tex_edge_alpha
        ));
    }
}

#[test]
fn release_vector_matches_setup_play_move_and_draw_order() {
    let mut actor = BalloonActor::new(
        Position {
            x: 4.0,
            y: 5.0,
            z: 6.0,
        },
        ShortAngles { x: 0, y: 0, z: 0 },
    );
    actor.request_fly(
        3,
        ShortAngles {
            x: 100,
            y: 0,
            z: -100,
        },
        40,
        Position {
            x: 10.0,
            y: 20.0,
            z: 30.0,
        },
        -1.0,
        7.0,
    );
    let mut trace = Trace {
        player: Some(Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }),
        ..Trace::default()
    };

    actor.tick(1.0, &mut trace);
    actor.draw(&mut trace);

    assert_eq!(
        trace.events,
        [
            "construct:26:8",
            "init:31:-1.0:0.0:0.0",
            "play",
            "init:31:1.0:0.5:-5.0",
            "play",
            "draw:1:80",
            "draw:2:80",
            "draw:3:80",
            "draw:1:144",
            "draw:2:144",
            "draw:3:144",
        ]
    );
    assert_eq!(actor.saved_type(), 3);
    assert_eq!(actor.shape_rotation().y, 0);
    assert!((actor.world_position().z - 33.5).abs() < 0.0001);
    assert!((actor.world_position().y - 20.05).abs() < 0.0001);
}

#[test]
fn threshold_request_is_deferred_until_the_next_dispatch_pass() {
    let mut actor = BalloonActor::new(Position::default(), ShortAngles::default());
    actor.request_fly(
        0,
        ShortAngles::default(),
        0,
        Position {
            x: 0.0,
            y: 210.0,
            z: 0.0,
        },
        -1.0,
        7.0,
    );
    let mut trace = Trace {
        player: Some(Position::default()),
        ..Trace::default()
    };

    actor.tick(1.0, &mut trace);
    assert_eq!(
        actor.setup_mode_raw(),
        0,
        "request_hide only sets setup_mode"
    );
    assert_eq!(actor.mode(), Some(nookery_actor::Mode::Fly));

    actor.tick(1.0, &mut trace);
    assert_eq!(actor.mode(), Some(nookery_actor::Mode::Hide));
    assert_eq!(actor.world_position(), Position::default());
}

#[test]
fn missing_player_preserves_hidden_position_and_prevents_threshold_check() {
    let mut actor = BalloonActor::new(
        Position {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        ShortAngles::default(),
    );
    let mut trace = Trace::default();
    actor.tick(1.0, &mut trace);
    assert_eq!(
        actor.world_position(),
        Position {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }
    );
    assert!(trace.events.is_empty());
}
