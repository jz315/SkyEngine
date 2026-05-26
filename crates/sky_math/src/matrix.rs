use std::ops::Mul;

use super::{
    quaternion::Quat,
    vector::{Vec3, Vec4},
};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct Mat4(pub(crate) glam::Mat4);

impl Mat4 {
    pub const ZERO: Self = Self(glam::Mat4::ZERO);
    pub const IDENTITY: Self = Self(glam::Mat4::IDENTITY);

    #[inline]
    pub fn from_cols_array(value: [f32; 16]) -> Self {
        Self(glam::Mat4::from_cols_array(&value))
    }

    #[inline]
    pub fn to_cols_array(self) -> [f32; 16] {
        self.0.to_cols_array()
    }

    #[inline]
    pub fn inverse(self) -> Self {
        Self(self.0.inverse())
    }

    #[inline]
    pub fn transpose(self) -> Self {
        Self(self.0.transpose())
    }

    #[inline]
    pub fn determinant(self) -> f32 {
        self.0.determinant()
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    #[inline]
    pub fn transform_point3(self, point: Vec3) -> Vec3 {
        Vec3::from_glam(self.0.transform_point3(point.as_glam()))
    }

    #[inline]
    pub fn transform_vector3(self, vector: Vec3) -> Vec3 {
        Vec3::from_glam(self.0.transform_vector3(vector.as_glam()))
    }

    #[inline]
    pub fn transform_vec4(self, vector: Vec4) -> Vec4 {
        Vec4::from_glam(self.0 * vector.as_glam())
    }

    #[inline]
    pub fn from_quat(quat: Quat) -> Self {
        Self(glam::Mat4::from_quat(quat.as_glam()))
    }

    #[inline]
    pub fn from_rotation_x(radians: f32) -> Self {
        Self(glam::Mat4::from_rotation_x(radians))
    }

    #[inline]
    pub fn from_rotation_y(radians: f32) -> Self {
        Self(glam::Mat4::from_rotation_y(radians))
    }

    #[inline]
    pub fn from_rotation_z(radians: f32) -> Self {
        Self(glam::Mat4::from_rotation_z(radians))
    }

    #[inline]
    pub fn from_translation(translation: Vec3) -> Self {
        Self(glam::Mat4::from_translation(translation.as_glam()))
    }

    #[inline]
    pub fn from_scale(scale: Vec3) -> Self {
        Self(glam::Mat4::from_scale(scale.as_glam()))
    }

    #[inline]
    pub fn from_scale_rotation_translation(scale: Vec3, rotation: Quat, translation: Vec3) -> Self {
        Self(glam::Mat4::from_scale_rotation_translation(
            scale.as_glam(),
            rotation.as_glam(),
            translation.as_glam(),
        ))
    }

    #[inline]
    pub fn from_rotation_translation(rotation: Quat, translation: Vec3) -> Self {
        Self(glam::Mat4::from_rotation_translation(
            rotation.as_glam(),
            translation.as_glam(),
        ))
    }

    #[inline]
    pub fn to_scale_rotation_translation(self) -> (Vec3, Quat, Vec3) {
        let (scale, rotation, translation) = self.0.to_scale_rotation_translation();
        (
            Vec3::from_glam(scale),
            Quat(rotation).normalized(),
            Vec3::from_glam(translation),
        )
    }

    #[inline]
    pub fn orthographic_rh(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Self {
        Self(glam::Mat4::orthographic_rh(
            left, right, bottom, top, near, far,
        ))
    }

    #[inline]
    pub fn perspective_rh(vertical_fov_radians: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self(glam::Mat4::perspective_rh(
            vertical_fov_radians.max(0.001),
            aspect.max(f32::EPSILON),
            near,
            far,
        ))
    }

    #[inline]
    pub fn look_to_rh(origin: Vec3, direction: Vec3, up: Vec3) -> Self {
        Self(glam::Mat4::look_to_rh(
            origin.as_glam(),
            direction.as_glam(),
            up.as_glam(),
        ))
    }

    #[inline]
    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        Self(glam::Mat4::look_at_rh(
            eye.as_glam(),
            target.as_glam(),
            up.as_glam(),
        ))
    }
}

impl Mul for Mat4 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;

    fn mul(self, rhs: Vec4) -> Self::Output {
        self.transform_vec4(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::Mat4;
    use crate::{Quat, Vec3};

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        for (actual, expected) in actual.to_array().into_iter().zip(expected.to_array()) {
            assert!(
                (actual - expected).abs() <= 1.0e-5,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn rotation_z_matches_quaternion_rotation() {
        let matrix = Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
        assert_vec3_close(matrix.transform_vector3(Vec3::X), Vec3::Y);
    }

    #[test]
    fn scale_rotation_translation_round_trips_common_transform_parts() {
        let scale = Vec3::new(2.0, 3.0, 4.0);
        let rotation = Quat::from_rotation_y(0.25);
        let translation = Vec3::new(5.0, 6.0, 7.0);
        let matrix = Mat4::from_scale_rotation_translation(scale, rotation, translation);
        let (actual_scale, actual_rotation, actual_translation) =
            matrix.to_scale_rotation_translation();

        assert_vec3_close(actual_scale, scale);
        assert_vec3_close(actual_translation, translation);
        assert!((actual_rotation.dot(rotation) - 1.0).abs() <= 1.0e-5);
    }

    #[test]
    fn look_at_places_target_on_negative_z_axis() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let target_in_view = view.transform_point3(Vec3::ZERO);

        assert_vec3_close(target_in_view, Vec3::new(0.0, 0.0, -5.0));
    }
}
