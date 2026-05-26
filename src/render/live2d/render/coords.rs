use std::marker::PhantomData;

use crate::math::{Mat4, Vec4};

/// Live2D model-local logical coordinates before view/projection transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpace;
/// Homogeneous clip/NDC-oriented coordinates after the model projection matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipSpace;
/// Mask render target clip coordinates used while rasterizing mask drawables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskClipSpace;
/// Mask texture UV coordinates used while sampling the packed mask atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskUvSpace;

pub type ModelToClip = CoordTransform<ModelSpace, ClipSpace>;
pub type ClipToModel = CoordTransform<ClipSpace, ModelSpace>;
pub type ModelToMaskClip = CoordTransform<ModelSpace, MaskClipSpace>;
pub type MaskClipToMaskUv = CoordTransform<MaskClipSpace, MaskUvSpace>;
pub type ModelToMaskUv = CoordTransform<ModelSpace, MaskUvSpace>;
pub type ClipToMaskUv = CoordTransform<ClipSpace, MaskUvSpace>;

#[derive(Debug, PartialEq)]
pub struct CoordTransform<From, To> {
    matrix: Mat4,
    _marker: PhantomData<fn(From) -> To>,
}

impl<From, To> Clone for CoordTransform<From, To> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<From, To> Copy for CoordTransform<From, To> {}

impl<From, To> CoordTransform<From, To> {
    #[inline]
    pub fn from_cols_array(value: [f32; 16]) -> Self {
        Self {
            matrix: Mat4::from_cols_array(value),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub(super) fn from_mat4(matrix: Mat4) -> Self {
        Self {
            matrix,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_cols_array(self) -> [f32; 16] {
        self.matrix.to_cols_array()
    }

    #[inline]
    pub fn then<Next>(self, next: CoordTransform<To, Next>) -> CoordTransform<From, Next> {
        CoordTransform {
            matrix: next.matrix * self.matrix,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn inverse(self) -> CoordTransform<To, From> {
        CoordTransform {
            matrix: self.matrix.inverse(),
            _marker: PhantomData,
        }
    }

    #[cfg(test)]
    pub fn transform_xy(self, x: f32, y: f32) -> [f32; 2] {
        let transformed = self.matrix.transform_vec4(Vec4::new(x, y, 0.0, 1.0));
        [
            transformed.x() / transformed.w(),
            transformed.y() / transformed.w(),
        ]
    }
}
