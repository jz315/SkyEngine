#[cfg(feature = "live2d")]
use crate::math::{Mat4, Transform};

#[cfg(feature = "live2d")]
#[inline]
pub(crate) fn column_major_mul(lhs: [f32; 16], rhs: [f32; 16]) -> [f32; 16] {
    (Mat4::from_cols_array(lhs) * Mat4::from_cols_array(rhs)).to_cols_array()
}

#[cfg(feature = "live2d")]
pub(crate) fn scene_transform_matrix(transform: Transform) -> [f32; 16] {
    transform.to_matrix4().to_cols_array()
}
