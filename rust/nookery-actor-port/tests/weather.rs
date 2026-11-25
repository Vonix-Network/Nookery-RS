use nookery_actor_port::weather::{
    PRIVATE_COUNT, Particle, Profile, Vec3, WeatherField, decide_rain_count, should_spawn,
};

#[test]
fn first_free_and_allocation_match_weather_clip() {
    let mut field = WeatherField::default();
    assert_eq!(field.first_free(), 0);
    let particle = field
        .allocate(
            2,
            280.0,
            Some(Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            None,
            0,
        )
        .expect("slot 0 is free");
    assert_eq!(particle.id, 0);
    assert_eq!(particle.status, 2);
    assert_eq!(particle.timer, 280.0);
    assert_eq!(
        particle.pos,
        Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }
    );
    assert!(field.allocate(1, 8.0, None, None, 0).is_none());
    assert_eq!(field.first_free(), 1);
    field.remove(0);
    assert_eq!(field.first_free(), 0);
    assert_eq!(field.particles.len(), PRIVATE_COUNT);
}

#[test]
fn failed_allocation_does_not_mutate_an_occupied_slot() {
    let mut field = WeatherField::default();
    field.particles[3] = Particle {
        use_flag: true,
        id: 3,
        pos: Vec3 {
            x: 7.0,
            y: 8.0,
            z: 9.0,
        },
        ..Particle::default()
    };
    let before = field.particles[3];
    assert!(
        field
            .allocate(4, 99.0, Some(Vec3::default()), Some(Vec3::default()), 3)
            .is_none()
    );
    assert_eq!(field.particles[3], before);
    assert!(field.allocate(4, 99.0, None, None, -1).is_none());
    assert!(
        field
            .allocate(4, 99.0, None, None, PRIVATE_COUNT as i32)
            .is_none()
    );
}

#[test]
fn rain_fractional_carry_and_intensity_follow_source() {
    let mut field = WeatherField {
        horizontal_scale: 0.5,
        current_level: 1,
        ..WeatherField::default()
    };
    assert_eq!(decide_rain_count(&mut field), 0);
    assert_eq!(decide_rain_count(&mut field), 1);
    field.current_level = 3;
    assert_eq!(decide_rain_count(&mut field), 1);
    assert!((field.spawn_accum - 0.5).abs() < f32::EPSILON);
}

#[test]
fn particle_motion_uses_dt_and_wraps_the_same_edges() {
    let mut field = WeatherField {
        origin: Vec3 {
            x: 10.0,
            y: 5.0,
            z: 20.0,
        },
        wind: Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        player_y: 0.0,
        ..WeatherField::default()
    };
    field.allocate(
        2,
        280.0,
        Some(Vec3 {
            x: -95.0,
            y: 45.0,
            z: 205.0,
        }),
        Some(Vec3 {
            x: 2.0,
            y: -1.0,
            z: 4.0,
        }),
        0,
    );
    field.particles[0].work[1] = 10;
    field.particles[0].current_y = 235.0;
    field.move_snow(0, 1.0);
    assert_eq!(field.particles[0].pos.x, 108.0);
    assert_eq!(field.particles[0].pos.z, -168.0);
    assert_eq!(field.particles[0].pos.y, 46.0);

    field.allocate(4, 280.0, Some(Vec3::default()), Some(Vec3::default()), 1);
    field.particles[1].work[4] = 10;
    field.current_level = 2;
    assert!(should_spawn(&mut field, Profile::Leaf, 8.0));
    assert!(!should_spawn(&mut field, Profile::Fine, 1.0));
}
