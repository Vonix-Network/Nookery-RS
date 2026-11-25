//! Pure matrix operations from `src/game/m_skin_matrix.c`.
//!
//! The C `MtxF` dependency is represented as its row-major sixteen-float
//! storage rather than duplicated as a second named game type.  Indices are
//! `[xx, xy, xz, xw, yx, ... , ww]`.

use std::f32::consts::PI;

pub type Matrix4 = [f32; 16];
pub type Vector3 = [f32; 3];

fn sin_s(angle: i16) -> f32 {
    (f32::from(angle) * PI / 32768.0).sin()
}

fn cos_s(angle: i16) -> f32 {
    (f32::from(angle) * PI / 32768.0).cos()
}

/// Port of `Skin_Matrix_PrjMulVector`.
pub fn project_mul_vector(matrix: &Matrix4, source: Vector3) -> (Vector3, f32) {
    let destination = [
        matrix[3] + ((source[0] * matrix[0]) + (source[1] * matrix[1]) + (source[2] * matrix[2])),
        matrix[7] + ((source[0] * matrix[4]) + (source[1] * matrix[5]) + (source[2] * matrix[6])),
        matrix[11] + ((source[0] * matrix[8]) + (source[1] * matrix[9]) + (source[2] * matrix[10])),
    ];
    let w = matrix[15]
        + ((source[0] * matrix[12]) + (source[1] * matrix[13]) + (source[2] * matrix[14]));
    (destination, w)
}

/// Port of `Skin_Matrix_MulMatrix`, including the source's `A * B` order.
pub fn mul_matrix(a: &Matrix4, b: &Matrix4, destination: &mut Matrix4) {
    let mut result = [0.0; 16];
    for row in 0..4 {
        for column in 0..4 {
            result[row * 4 + column] = (a[row * 4] * b[column])
                + (a[row * 4 + 1] * b[4 + column])
                + (a[row * 4 + 2] * b[8 + column])
                + (a[row * 4 + 3] * b[12 + column]);
        }
    }
    *destination = result;
}

/// Port of `Skin_Matrix_SetScale`.
pub fn set_scale(matrix: &mut Matrix4, x: f32, y: f32, z: f32) {
    *matrix = [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
}

/// Port of `Skin_Matrix_SetRotateXyz_s`.
pub fn set_rotate_xyz_s(matrix: &mut Matrix4, x: i16, y: i16, z: i16) {
    let sin_z = sin_s(z);
    let cos_z = cos_s(z);

    matrix[5] = cos_z;
    matrix[1] = -sin_z;
    matrix[14] = 0.0;
    matrix[13] = 0.0;
    matrix[12] = 0.0;
    matrix[11] = 0.0;
    matrix[9] = 0.0;
    matrix[3] = 0.0;
    matrix[7] = 0.0;
    matrix[15] = 1.0;

    if y != 0 {
        let sin_y = sin_s(y);
        let cos_y = cos_s(y);
        matrix[0] = cos_z * cos_y;
        matrix[2] = cos_z * sin_y;
        matrix[4] = sin_z * cos_y;
        matrix[6] = sin_z * sin_y;
        matrix[8] = -sin_y;
        matrix[10] = cos_y;
    } else {
        matrix[0] = cos_z;
        matrix[4] = sin_z;
        matrix[6] = 0.0;
        matrix[2] = 0.0;
        matrix[8] = 0.0;
        matrix[10] = 1.0;
    }

    if x != 0 {
        let sin_x = sin_s(x);
        let cos_x = cos_s(x);

        let xy = matrix[1];
        let xz = matrix[2];
        matrix[1] = (xy * cos_x) + (xz * sin_x);
        matrix[2] = -(xy * sin_x) + (xz * cos_x);

        let yy = matrix[5];
        let yz = matrix[6];
        matrix[5] = (yy * cos_x) + (yz * sin_x);
        matrix[6] = -(yy * sin_x) + (yz * cos_x);

        let zz = matrix[10];
        matrix[9] = zz * sin_x;
        matrix[10] = zz * cos_x;
    } else {
        matrix[9] = 0.0;
    }
}

/// Port of `Skin_Matrix_SetTranslate`.
pub fn set_translate(matrix: &mut Matrix4, x: f32, y: f32, z: f32) {
    *matrix = [
        1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y, 0.0, 0.0, 1.0, z, 0.0, 0.0, 0.0, 1.0,
    ];
}
