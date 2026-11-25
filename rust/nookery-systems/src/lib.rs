/*
 * Nookery system math port.  The original files retain their upstream
 * headers in the source tree; these Rust modules keep the same scalar and
 * matrix conventions used by sys_math.c, sys_math3d.c, sys_math_atan.c and
 * sys_matrix.c.
 */
#![forbid(unsafe_code)]

use core::f32::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn normalized(self) -> Self {
        let n = self.length();
        if n == 0.0 {
            Self::default()
        } else {
            Self::new(self.x / n, self.y / n, self.z / n)
        }
    }
}
impl core::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl core::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl core::ops::Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, v: f32) -> Self {
        Self::new(self.x * v, self.y * v, self.z * v)
    }
}

pub fn sin_s16(angle: i16) -> f32 {
    (f32::from(angle) * PI / 32768.0_f32).sin()
}
pub fn cos_s16(angle: i16) -> f32 {
    (f32::from(angle) * PI / 32768.0).cos()
}
pub fn atan2_s16(y: f32, x: f32) -> i16 {
    let a = y.atan2(x) * 32768.0 / PI;
    a.clamp(-32768.0, 32767.0) as i16
}
pub fn atan_s16(v: f32) -> i16 {
    atan2_s16(v, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}
impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}
impl Mat4 {
    pub const fn identity() -> Self {
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        let mut r = Self::identity();
        r.m[3][0] = x;
        r.m[3][1] = y;
        r.m[3][2] = z;
        r
    }
    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        let mut r = Self::identity();
        r.m[0][0] = x;
        r.m[1][1] = y;
        r.m[2][2] = z;
        r
    }
    pub fn rotation_x(a: i16) -> Self {
        let (s, c) = (sin_s16(a), cos_s16(a));
        Self {
            m: [
                [1., 0., 0., 0.],
                [0., c, s, 0.],
                [0., -s, c, 0.],
                [0., 0., 0., 1.],
            ],
        }
    }
    pub fn rotation_y(a: i16) -> Self {
        let (s, c) = (sin_s16(a), cos_s16(a));
        Self {
            m: [
                [c, 0., -s, 0.],
                [0., 1., 0., 0.],
                [s, 0., c, 0.],
                [0., 0., 0., 1.],
            ],
        }
    }
    pub fn rotation_z(a: i16) -> Self {
        let (s, c) = (sin_s16(a), cos_s16(a));
        Self {
            m: [
                [c, s, 0., 0.],
                [-s, c, 0., 0.],
                [0., 0., 1., 0.],
                [0., 0., 0., 1.],
            ],
        }
    }
    pub fn multiply(self, o: Self) -> Self {
        let mut r = [[0.; 4]; 4];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                for k in 0..4 {
                    *cell += self.m[i][k] * o.m[k][j];
                }
            }
        }
        Self { m: r }
    }
    pub fn transform(self, v: Vec3) -> Vec3 {
        Vec3::new(
            v.x * self.m[0][0] + v.y * self.m[1][0] + v.z * self.m[2][0] + self.m[3][0],
            v.x * self.m[0][1] + v.y * self.m[1][1] + v.z * self.m[2][1] + self.m[3][1],
            v.x * self.m[0][2] + v.y * self.m[1][2] + v.z * self.m[2][2] + self.m[3][2],
        )
    }
}

#[derive(Clone, Debug)]
pub struct MatrixStack {
    current: Mat4,
    stack: Vec<Mat4>,
}
impl MatrixStack {
    pub fn new() -> Self {
        Self {
            current: Mat4::identity(),
            stack: Vec::new(),
        }
    }
    pub fn push(&mut self) {
        self.stack.push(self.current)
    }
    pub fn pull(&mut self) {
        if let Some(m) = self.stack.pop() {
            self.current = m
        }
    }
    pub fn load(&mut self, m: Mat4) {
        self.current = m
    }
    pub fn mult(&mut self, m: Mat4) {
        self.current = self.current.multiply(m)
    }
    pub fn current(&self) -> Mat4 {
        self.current
    }
    pub fn translate(&mut self, x: f32, y: f32, z: f32) {
        self.mult(Mat4::translation(x, y, z))
    }
    pub fn rotate_xyz(&mut self, x: i16, y: i16, z: i16) {
        self.mult(
            Mat4::rotation_x(x)
                .multiply(Mat4::rotation_y(y))
                .multiply(Mat4::rotation_z(z)),
        )
    }
    pub fn scale(&mut self, x: f32, y: f32, z: f32) {
        self.mult(Mat4::scale(x, y, z))
    }
}

impl Default for MatrixStack {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub point: Vec3,
    pub normal: Vec3,
}
impl Plane {
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        if self.normal.y.abs() < f32::EPSILON {
            None
        } else {
            Some(
                self.point.y
                    - (self.normal.x * (x - self.point.x) + self.normal.z * (z - self.point.z))
                        / self.normal.y,
            )
        }
    }
    pub fn segment_hit(&self, a: Vec3, b: Vec3) -> Option<Vec3> {
        let da = (a - self.point).dot(self.normal);
        let db = (b - self.point).dot(self.normal);
        if (da > 0.0 && db > 0.0) || (da < 0.0 && db < 0.0) {
            return None;
        }
        let d = da - db;
        if d.abs() < f32::EPSILON {
            return Some(a);
        }
        let t = da / d;
        Some(a + (b - a) * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matrix_stack_matches_push_pull() {
        let mut s = MatrixStack::new();
        s.translate(1., 2., 3.);
        s.push();
        s.scale(2., 2., 2.);
        s.pull();
        assert_eq!(
            s.current().transform(Vec3::default()),
            Vec3::new(1., 2., 3.)
        );
    }
    #[test]
    fn plane_intersection_preserves_height() {
        let p = Plane {
            point: Vec3::new(0., 5., 0.),
            normal: Vec3::new(0., 1., 0.),
        };
        assert_eq!(p.height_at(20., -3.), Some(5.));
        assert_eq!(
            p.segment_hit(Vec3::new(0., 0., 0.), Vec3::new(0., 10., 0.)),
            Some(Vec3::new(0., 5., 0.))
        );
    }
}
