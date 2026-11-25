use nookery_actor_port::airplane::{self, AirplaneState, AirplaneStatus};
use nookery_actor_port::insect::{self, INSECT_TYPE_FIREFLY, InsectState};
use std::path::PathBuf;
use std::process::Command;

fn oracle_output() -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work =
        std::env::temp_dir().join(format!("nookery-actor-port-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("create oracle work directory");
    let binary = work.join("oracle");
    let status = Command::new("cc")
        .args(["-std=c11", "-O1", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(manifest.join("../nookery-libc64/parity/include"))
        .arg("-o")
        .arg(&binary)
        .arg(manifest.join("parity/oracle.c"))
        .arg("-lm")
        .status()
        .expect("compile C oracle");
    assert!(status.success(), "C oracle compilation failed: {status}");
    let output = Command::new(binary).output().expect("run C oracle");
    assert!(
        output.status.success(),
        "C oracle failed: {}",
        output.status
    );
    String::from_utf8(output.stdout).expect("oracle output is UTF-8")
}

fn fields<'a>(output: &'a str, prefix: &str) -> Vec<&'a str> {
    output
        .lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("oracle output has no {prefix:?} line"))
        .split_whitespace()
        .skip(1)
        .collect()
}

fn close(actual: f32, expected: &str) {
    let expected = expected.parse::<f32>().expect("oracle float");
    assert!(
        (actual - expected).abs() <= 1.0e-5,
        "{actual} != {expected}"
    );
}

#[test]
fn rust_state_transitions_match_the_c_driver() {
    let c = oracle_output();

    let mut airplane_state = AirplaneState {
        speed: 0.5,
        rot_z: 2.0,
        ..AirplaneState::default()
    };
    airplane::free_fly_move(&mut airplane_state, 1.0, true, true);
    assert_eq!(airplane_state.status, AirplaneStatus::FallFlyMove);
    assert_eq!(airplane_state.speed, 0.0);
    assert_eq!(airplane_state.dust_effects, 0);
    let air_state = airplane_state;
    let mut select_state = AirplaneState::default();
    assert!(airplane::select_fly(&mut select_state, 67, 0.99));
    let first_select = select_state.status;
    airplane::select_fly(&mut select_state, 68, 0.0);
    let second_select = select_state.status;
    airplane::select_fly(&mut select_state, 68, 0.5);
    let third_select = select_state.status;
    assert_eq!(first_select, AirplaneStatus::FreeFlyMove);
    assert_eq!(second_select, AirplaneStatus::SomerFlyMove);
    assert_eq!(third_select, AirplaneStatus::FallFlyMove2);

    let mut normal = InsectState {
        patience: 10.0,
        life_time: 0.5,
        alpha_time: 24.0,
        alpha0: 255,
        ..InsectState::default()
    };
    insect::tick(&mut normal, 0.0, 1.0);
    assert_eq!(normal.patience, 9.5);
    assert_eq!(normal.life_time, 0.0);
    assert!(insect::is_lifetime_expired(&normal));

    let mut firefly = InsectState {
        insect_type: INSECT_TYPE_FIREFLY,
        alpha_time: 1.0,
        alpha0: 255,
        alpha1: 255,
        ..InsectState::default()
    };
    insect::calc_life_time(&mut firefly, 1.0);
    insect::calc_alpha_time(&mut firefly, 1.0);
    assert_eq!(firefly.alpha2, 255);
    assert!(insect::is_destructed(&firefly));

    let air = fields(&c, "air ");
    close(air_state.speed, air[0]);
    close(air_state.rot_x, air[1]);
    close(air_state.rot_z, air[2]);
    assert_eq!(air_state.status as i16, air[3].parse::<i16>().unwrap());
    assert_eq!(air_state.dust_effects, air[4].parse::<u32>().unwrap());
    close(airplane::bg_check_offset_y(air_state.status), air[5]);

    let select = fields(&c, "select ");
    assert_eq!(select, ["0", "3", "2"]);
    let c_normal = fields(&c, "ins ");
    close(normal.patience, c_normal[0]);
    close(normal.life_time, c_normal[1]);
    close(normal.alpha_time, c_normal[2]);
    assert_eq!(normal.alpha0, c_normal[3].parse::<i32>().unwrap());
    assert_eq!(normal.alpha1, c_normal[4].parse::<i32>().unwrap());
    assert_eq!(normal.alpha2, c_normal[5].parse::<i32>().unwrap());
    assert_eq!(normal.flags, c_normal[6].parse::<u8>().unwrap());
    let firefly_oracle = c
        .lines()
        .filter(|line| line.starts_with("ins "))
        .nth(1)
        .expect("firefly oracle line");
    let firefly_fields: Vec<_> = firefly_oracle.split_whitespace().skip(1).collect();
    assert_eq!(firefly.alpha2, firefly_fields[5].parse::<i32>().unwrap());
    assert_eq!(firefly.flags, firefly_fields[6].parse::<u8>().unwrap());
    let offset = fields(&c, "offset ");
    close(-5.5, offset[0]);
    close(0.0, offset[1]);
}
