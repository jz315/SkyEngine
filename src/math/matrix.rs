use std::ops::Mul;

use super::{
    quaternion::Quat,
    vector::{Vec3, Vec4},
};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct Mat4(pub(crate) glam::Mat4);

impl Mat4 {
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
    pub fn transform_point3(self, point: Vec3) -> Vec3 {
        Vec3::from_glam(self.0.transform_point3(point.as_glam()))
    }

    #[inline]
    pub fn transform_vector3(self, vector: Vec3) -> Vec3 {
        Vec3::from_glam(self.0.transform_vector3(vector.as_glam()))
    }

    #[inline]
    pub fn from_quat(quat: Quat) -> Self {
        Self(glam::Mat4::from_quat(quat.as_glam()))
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
        Vec4::from_glam(self.0 * rhs.as_glam())
    }
}
