use std::ops::Mul;

use super::{matrix::Mat4, vector::Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct Quat(pub(crate) glam::Quat);

impl Quat {
    pub const IDENTITY: Self = Self(glam::Quat::IDENTITY);

    #[inline]
    pub fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self(glam::Quat::from_xyzw(x, y, z, w))
    }

    #[inline]
    pub fn from_xyzw_array(value: [f32; 4]) -> Self {
        Self::from_xyzw(value[0], value[1], value[2], value[3])
    }

    #[inline]
    pub fn to_xyzw_array(self) -> [f32; 4] {
        [self.0.x, self.0.y, self.0.z, self.0.w]
    }

    #[inline]
    pub fn x(self) -> f32 {
        self.0.x
    }

    #[inline]
    pub fn y(self) -> f32 {
        self.0.y
    }

    #[inline]
    pub fn z(self) -> f32 {
        self.0.z
    }

    #[inline]
    pub fn w(self) -> f32 {
        self.0.w
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.0.length_squared()
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.0.dot(rhs.0)
    }

    #[inline]
    pub fn normalized(self) -> Self {
        if self.length_squared() <= f32::EPSILON {
            Self::IDENTITY
        } else {
            Self(self.0.normalize())
        }
    }

    #[inline]
    pub fn conjugate(self) -> Self {
        Self(self.0.conjugate())
    }

    #[inline]
    pub fn inverse(self) -> Self {
        self.normalized().conjugate()
    }

    #[inline]
    pub fn from_axis_angle(axis: Vec3, radians: f32) -> Self {
        match axis.try_normalized() {
            Some(axis) => Self(glam::Quat::from_axis_angle(axis.as_glam(), radians)),
            None => Self::IDENTITY,
        }
    }

    #[inline]
    pub fn from_rotation_x(radians: f32) -> Self {
        Self(glam::Quat::from_rotation_x(radians))
    }

    #[inline]
    pub fn from_rotation_y(radians: f32) -> Self {
        Self(glam::Quat::from_rotation_y(radians))
    }

    #[inline]
    pub fn from_rotation_z(radians: f32) -> Self {
        Self(glam::Quat::from_rotation_z(radians))
    }

    #[inline]
    pub fn from_euler_angles(pitch: f32, yaw: f32, roll: f32) -> Self {
        (Self::from_rotation_z(roll) * Self::from_rotation_y(yaw) * Self::from_rotation_x(pitch))
            .normalized()
    }

    #[inline]
    pub fn rotate_vec3(self, vector: Vec3) -> Vec3 {
        Vec3::from_glam(self.normalized().0.mul_vec3(vector.as_glam()))
    }

    #[inline]
    pub fn slerp(self, rhs: Self, t: f32) -> Self {
        Self(self.normalized().0.slerp(rhs.normalized().0, t)).normalized()
    }

    #[inline]
    pub fn to_matrix4(self) -> Mat4 {
        Mat4::from_quat(self.normalized())
    }

    #[inline]
    pub fn roll(self) -> f32 {
        let q = self.normalized();
        (2.0 * (q.w() * q.z() + q.x() * q.y())).atan2(1.0 - 2.0 * (q.y() * q.y() + q.z() * q.z()))
    }

    #[inline]
    pub fn is_planar_2d(self) -> bool {
        let q = self.normalized();
        q.x().abs() <= 1e-6 && q.y().abs() <= 1e-6
    }
    #[inline]
    pub(crate) fn as_glam(self) -> glam::Quat {
        self.0
    }
}

impl Mul for Quat {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Quat;
    use crate::Vec3;

    fn assert_close(actual: Vec3, expected: Vec3) {
        for (actual, expected) in actual.to_array().into_iter().zip(expected.to_array()) {
            assert!(
                (actual - expected).abs() <= 1.0e-5,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn axis_angle_handles_zero_axis_as_identity() {
        let q = Quat::from_axis_angle(Vec3::ZERO, 1.0);
        assert_eq!(q, Quat::IDENTITY);
    }

    #[test]
    fn inverse_undoes_rotation() {
        let q = Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2);
        let rotated = q.rotate_vec3(Vec3::X);
        let restored = q.inverse().rotate_vec3(rotated);

        assert_close(rotated, Vec3::Y);
        assert_close(restored, Vec3::X);
    }

    #[test]
    fn slerp_returns_unit_rotation_between_endpoints() {
        let start = Quat::IDENTITY;
        let end = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let halfway = start.slerp(end, 0.5);

        assert!((halfway.length_squared() - 1.0).abs() <= 1.0e-5);
        let expected = Vec3::new(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        );
        assert_close(halfway.rotate_vec3(Vec3::X), expected);
    }
}
