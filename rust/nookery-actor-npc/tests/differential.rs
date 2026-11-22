//! Differential coverage against the executable C excerpt oracle.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use nookery_actor_npc::{
    ActionDispatcher, ActionPhase, NpcAction, NpcBehavior, action_proc, set_request_act,
};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/actor/npc/ac_npc_action.c_inc"
));
const ORACLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/oracle/npc_action_oracle.c"
));

fn source_block(text: &str) -> &str {
    let start = text
        .find("static void aNPC_chk_request_act")
        .expect("source block start");
    let end = text[start..]
        .find("\nstatic unsigned parse")
        .map_or(text.len(), |offset| start + offset);
    text[start..end].trim_end()
}

fn oracle_path() -> (PathBuf, PathBuf) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("nookery-actor-npc-{stamp}-{}", std::process::id()));
    (base.with_extension("c"), base.with_extension("bin"))
}

fn compile_oracle() -> (PathBuf, PathBuf) {
    let (source_path, binary_path) = oracle_path();
    std::fs::write(&source_path, ORACLE).expect("write C oracle");
    let status = Command::new("cc")
        .args(["-std=c11", "-O1", "-Wall", "-Wextra", "-Werror"])
        .arg(&source_path)
        .arg("-o")
        .arg(&binary_path)
        .status()
        .expect("C compiler is required for the NPC differential test");
    assert!(status.success(), "C oracle compilation failed");
    (source_path, binary_path)
}

struct Recorder {
    events: Vec<String>,
}

impl ActionDispatcher for Recorder {
    fn dispatch(&mut self, index: u8, phase: ActionPhase, action: &mut NpcAction) {
        let phase = match phase {
            ActionPhase::ChangeData => 1,
            ActionPhase::Init => 0,
            ActionPhase::Main => 2,
        };
        self.events.push(format!("D:{index}:{phase}"));
        if phase == 1 {
            action.act_obj = 7;
            action.act_obj_id = 0x1234;
        }
    }

    fn resolve_object(&mut self, _action: &NpcAction) {
        self.events.push("R:0:0".to_owned());
    }
}

fn rust_result(values: [u16; 8]) -> String {
    let mut state = NpcBehavior::default();
    state.action.priority = values[0] as u8;
    state.action.idx = values[1] as u8;
    state.action.step = values[2] as u8;
    state.action.act_obj = values[3] as u8;
    state.action.act_obj_id = values[4];
    state.action.prev_idx = 0;
    state.action.prev_step = 0;
    let args = [11, 22, 33, 44, 55, 66];
    let accepted = set_request_act(
        &mut state,
        values[5] as u8,
        values[6] as u8,
        values[7] as u8,
        Some(&args),
    );
    let mut recorder = Recorder { events: Vec::new() };
    action_proc(&mut state, &mut recorder).expect("valid differential action index");
    format!(
        "accepted={} action={},{},{},{},{},{} request={},{},{} args={},{},{},{},{},{} events={}",
        u8::from(accepted),
        state.action.priority,
        state.action.idx,
        state.action.step,
        state.action.action_type,
        state.action.prev_idx,
        state.action.prev_step,
        state.request.act_priority,
        state.request.act_idx,
        state.request.act_type,
        state.request.act_args[0],
        state.request.act_args[1],
        state.request.act_args[2],
        state.request.act_args[3],
        state.request.act_args[4],
        state.request.act_args[5],
        recorder.events.join(";")
    )
}

fn c_result(binary: &Path, values: [u16; 8]) -> String {
    let args: Vec<String> = values.iter().map(ToString::to_string).collect();
    let output = Command::new(binary)
        .args(args)
        .output()
        .expect("C oracle runs");
    assert!(
        output.status.success(),
        "C oracle failed: {:?}",
        output.status
    );
    String::from_utf8(output.stdout)
        .expect("C oracle output is UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn rust_matches_the_verbatim_c_source_block() {
    assert_eq!(source_block(SOURCE), source_block(ORACLE));
    let (source_path, binary_path) = compile_oracle();
    let cases = [
        [2, 0, 0, 0, 0, 2, 1, 3],
        [4, 1, 0, 7, 0x34, 3, 1, 2],
        [4, 1, 0, 7, 0x1234, 3, 1, 2],
        [4, 1, 0, 0, 0, 1, 2, 2],
        [4, 1, 0, 0, 0, 0, 2, 2],
        [4, 1, 0, 0, 0, 4, 1, 2],
    ];
    for values in cases {
        let c = c_result(&binary_path, values);
        let rust = rust_result(values);
        assert_eq!(rust, c, "differential mismatch for {values:?}");
    }
    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(binary_path);
}
