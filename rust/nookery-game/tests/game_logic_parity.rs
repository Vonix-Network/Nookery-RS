use std::mem::size_of;

use nookery_game::{
    ACTOR_PART_NPC, CHAR_SPACE, DEFAULT_MELODY, DIARY_ENTRY_SIZE, DiaryEntry, Matrix4,
    actor_make_melody, actor_make_this_melody, diary_strlen, get_melody, init_diary,
    melody_from_u64, melody_to_u64, mul_matrix, project_mul_vector, radian_xy_by_2pos,
    sangle_xy_by_2pos, set_default_melody, set_rotate_xyz_s, set_scale, set_translate,
};

#[test]
fn c_m_mld_packs_notes_in_source_order_and_masks_each_nibble() {
    assert_eq!(melody_to_u64(&DEFAULT_MELODY), 0x7cf7_6bf9_aede_3fee);
    assert_eq!(
        melody_from_u64(0x0123_4567_89ab_cdef),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,]
    );

    let notes = [
        0x10, 0x21, 0x32, 0x43, 0x54, 0x65, 0x76, 0x87, 0x98, 0xa9, 0xba, 0xcb, 0xdc, 0xed, 0xfe,
        0xff,
    ];
    assert_eq!(
        melody_from_u64(melody_to_u64(&notes)),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,]
    );

    let mut save = 0;
    set_default_melody(&mut save);
    assert_eq!(get_melody(save), DEFAULT_MELODY);

    let mut calls = Vec::new();
    actor_make_this_melody(
        &DEFAULT_MELODY,
        Some((ACTOR_PART_NPC, 7)),
        |inst, melody| {
            calls.push((inst, *melody));
        },
    );
    actor_make_this_melody(&DEFAULT_MELODY, Some((3, 7)), |inst, melody| {
        calls.push((inst, *melody));
    });
    actor_make_melody(save, None, |inst, melody| calls.push((inst, *melody)));
    actor_make_melody(save, Some((ACTOR_PART_NPC, 9)), |inst, melody| {
        calls.push((inst, *melody));
    });
    assert_eq!(calls, vec![(7, DEFAULT_MELODY), (9, DEFAULT_MELODY)]);
}

#[test]
fn c_m_di_strlen_scans_backwards_and_init_fills_spaces() {
    assert_eq!(diary_strlen(&[1, 2, 0, 0], 4, 0), 2);
    assert_eq!(diary_strlen(&[0, 0, 0], 3, 0), 0);
    assert_eq!(diary_strlen(&[1, 2, 3], 0, 0), 0);

    let mut entry = DiaryEntry {
        text: [0; DIARY_ENTRY_SIZE],
    };
    init_diary(&mut entry);
    assert!(entry.text.iter().all(|byte| *byte == CHAR_SPACE));
    assert_eq!(size_of::<DiaryEntry>(), DIARY_ENTRY_SIZE);
}

#[test]
fn c_skin_matrix_projection_translation_and_multiplication_preserve_layout() {
    let mut scale = [0.0; 16];
    set_scale(&mut scale, 2.0, 3.0, 4.0);
    assert_eq!(
        project_mul_vector(&scale, [1.0, 2.0, 3.0]),
        ([2.0, 6.0, 12.0], 1.0)
    );

    let mut translate = [0.0; 16];
    set_translate(&mut translate, 5.0, 6.0, 7.0);
    assert_eq!(
        project_mul_vector(&translate, [1.0, 2.0, 3.0]),
        ([6.0, 8.0, 10.0], 1.0)
    );

    let mut product = [0.0; 16];
    mul_matrix(&translate, &scale, &mut product);
    assert_eq!(
        product,
        [
            2.0, 0.0, 0.0, 5.0, 0.0, 3.0, 0.0, 6.0, 0.0, 0.0, 4.0, 7.0, 0.0, 0.0, 0.0, 1.0,
        ]
    );
}

#[test]
fn c_skin_matrix_short_rotation_uses_n64_binary_angles() {
    let mut matrix = [0.0; 16];
    set_rotate_xyz_s(&mut matrix, 0, 0, 0x4000);
    assert!(matrix[0].abs() < 1e-6);
    assert!((matrix[1] + 1.0).abs() < 1e-6);
    assert!((matrix[4] - 1.0).abs() < 1e-6);
    assert!(matrix[5].abs() < 1e-6);

    let identity: Matrix4 = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut zero_rotation = [9.0; 16];
    set_rotate_xyz_s(&mut zero_rotation, 0, 0, 0);
    assert_eq!(zero_rotation, identity);
}

#[test]
fn c_m_olib_computes_xz_relative_radians_and_binary_angles() {
    let sub = [0.0, 0.0, 0.0];
    let min = [0.0, 0.0, 1.0];
    let radians = radian_xy_by_2pos(sub, min);
    assert!((radians[0] - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    assert_eq!(radians[1], 0.0);
    assert_eq!(sangle_xy_by_2pos(sub, min), [0x4000, 0, 0]);
}
