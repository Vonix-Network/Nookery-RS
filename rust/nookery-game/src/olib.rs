//! Coordinate-angle helpers from `src/game/m_olib.c`.

use std::f32::consts::PI;

/// C `xyz_t`, represented as positional components for this standalone math
/// slice.
pub type Vector3 = [f32; 3];
/// C `s_xyz`, represented as positional components for this standalone slice.
pub type ShortVector3 = [i16; 3];

/// Port of `radianxy_by_2pos`.
pub fn radian_xy_by_2pos(sub: Vector3, min: Vector3) -> Vector3 {
    [
        (min[2] - sub[2]).atan2(min[1] - sub[1]),
        (min[0] - sub[0]).atan2(min[2] - sub[2]),
        0.0,
    ]
}

/// Port of `sanglexy_by_2pos` and its `CAM_DEG_TO_BINANG` conversion.
pub fn sangle_xy_by_2pos(sub: Vector3, min: Vector3) -> ShortVector3 {
    let angle = radian_xy_by_2pos(sub, min);
    [degrees_to_binang(angle[0]), degrees_to_binang(angle[1]), 0]
}

fn degrees_to_binang(radians: f32) -> i16 {
    // TRUNCF_BINANG is a truncating signed 32-bit conversion after the
    // source's positive .5 bias.
    ((radians * (180.0 / PI) * 182.04167 + 0.5) as i32) as i16
}
