//! ECS-facing 2D rendering components and settings.

use crate::ecs::EntityId;
pub use crate::render::core::viewport::ViewportRect;
use crate::render::{Color, Texture};
#[cfg(feature = "live2d")]
use std::path::PathBuf;

/// Quaternion rotation used by the unified scene transform model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quaternion {
    pub const IDENTITY: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    #[inline]
    pub fn normalized(self) -> Self {
        let len_sq = self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w;
        if len_sq <= f32::EPSILON {
            return Self::IDENTITY;
        }
        let inv_len = len_sq.sqrt().recip();
        Self::new(
            self.x * inv_len,
            self.y * inv_len,
            self.z * inv_len,
            self.w * inv_len,
        )
    }

    #[inline]
    pub const fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    #[inline]
    pub fn from_rotation_x(radians: f32) -> Self {
        let half = radians * 0.5;
        let (sin_h, cos_h) = half.sin_cos();
        Self::new(sin_h, 0.0, 0.0, cos_h)
    }

    #[inline]
    pub fn from_rotation_y(radians: f32) -> Self {
        let half = radians * 0.5;
        let (sin_h, cos_h) = half.sin_cos();
        Self::new(0.0, sin_h, 0.0, cos_h)
    }

    #[inline]
    pub fn from_rotation_z(radians: f32) -> Self {
        let half = radians * 0.5;
        let (sin_h, cos_h) = half.sin_cos();
        Self::new(0.0, 0.0, sin_h, cos_h)
    }

    #[inline]
    pub fn from_euler_angles(pitch: f32, yaw: f32, roll: f32) -> Self {
        (Self::from_rotation_z(roll) * Self::from_rotation_y(yaw) * Self::from_rotation_x(pitch))
            .normalized()
    }

    #[inline]
    pub fn rotate_vector(self, vector: [f32; 3]) -> [f32; 3] {
        let q = self.normalized();
        let v = Quaternion::new(vector[0], vector[1], vector[2], 0.0);
        let rotated = q * v * q.conjugate();
        [rotated.x, rotated.y, rotated.z]
    }

    #[inline]
    pub fn to_matrix4(self) -> [f32; 16] {
        let q = self.normalized();
        let xx = q.x * q.x;
        let yy = q.y * q.y;
        let zz = q.z * q.z;
        let xy = q.x * q.y;
        let xz = q.x * q.z;
        let yz = q.y * q.z;
        let wx = q.w * q.x;
        let wy = q.w * q.y;
        let wz = q.w * q.z;
        [
            1.0 - 2.0 * (yy + zz),
            2.0 * (xy + wz),
            2.0 * (xz - wy),
            0.0,
            2.0 * (xy - wz),
            1.0 - 2.0 * (xx + zz),
            2.0 * (yz + wx),
            0.0,
            2.0 * (xz + wy),
            2.0 * (yz - wx),
            1.0 - 2.0 * (xx + yy),
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ]
    }

    #[inline]
    pub fn roll(self) -> f32 {
        let q = self.normalized();
        (2.0 * (q.w * q.z + q.x * q.y)).atan2(1.0 - 2.0 * (q.y * q.y + q.z * q.z))
    }

    #[inline]
    pub fn is_planar_2d(self) -> bool {
        let q = self.normalized();
        q.x.abs() <= 1e-6 && q.y.abs() <= 1e-6
    }
}

impl Default for Quaternion {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl std::ops::Mul for Quaternion {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self::new(
            self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
            self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
        )
    }
}

/// Shared scene transform used by the high-level renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: Quaternion,
}

impl Transform {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self::from_xyz(x, y, 0.0)
    }

    #[inline]
    pub const fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            position: [x, y, z],
            scale: [1.0, 1.0, 1.0],
            rotation: Quaternion::IDENTITY,
        }
    }

    #[inline]
    pub const fn from_position(position: [f32; 3]) -> Self {
        Self {
            position,
            scale: [1.0, 1.0, 1.0],
            rotation: Quaternion::IDENTITY,
        }
    }

    #[inline]
    pub const fn with_position(mut self, position: [f32; 3]) -> Self {
        self.position = position;
        self
    }

    #[inline]
    pub const fn with_z(mut self, z: f32) -> Self {
        self.position[2] = z;
        self
    }

    #[inline]
    pub const fn with_scale(mut self, scale_x: f32, scale_y: f32) -> Self {
        self.scale[0] = scale_x;
        self.scale[1] = scale_y;
        self
    }

    #[inline]
    pub const fn with_scale3(mut self, scale_x: f32, scale_y: f32, scale_z: f32) -> Self {
        self.scale = [scale_x, scale_y, scale_z];
        self
    }

    #[inline]
    pub fn with_rotation_quat(mut self, rotation: Quaternion) -> Self {
        self.rotation = rotation.normalized();
        self
    }

    #[inline]
    pub fn with_rotation(mut self, rotation: f32) -> Self {
        self.rotation = Quaternion::from_rotation_z(rotation);
        self
    }

    #[inline]
    pub fn with_euler_angles(mut self, pitch: f32, yaw: f32, roll: f32) -> Self {
        self.rotation = Quaternion::from_euler_angles(pitch, yaw, roll);
        self
    }

    #[inline]
    pub fn transform_vector(self, vector: [f32; 3]) -> [f32; 3] {
        self.rotation.rotate_vector([
            self.scale[0] * vector[0],
            self.scale[1] * vector[1],
            self.scale[2] * vector[2],
        ])
    }

    #[inline]
    pub fn transform_point(self, point: [f32; 3]) -> [f32; 3] {
        let rotated = self.transform_vector(point);
        [
            self.position[0] + rotated[0],
            self.position[1] + rotated[1],
            self.position[2] + rotated[2],
        ]
    }

    #[inline]
    pub fn mul_transform(self, local: Self) -> Self {
        Self {
            position: self.transform_point(local.position),
            scale: [
                self.scale[0] * local.scale[0],
                self.scale[1] * local.scale[1],
                self.scale[2] * local.scale[2],
            ],
            rotation: (self.rotation * local.rotation).normalized(),
        }
    }

    #[inline]
    pub const fn x(self) -> f32 {
        self.position[0]
    }

    #[inline]
    pub const fn y(self) -> f32 {
        self.position[1]
    }

    #[inline]
    pub const fn z(self) -> f32 {
        self.position[2]
    }

    #[inline]
    pub const fn scale_x(self) -> f32 {
        self.scale[0]
    }

    #[inline]
    pub const fn scale_y(self) -> f32 {
        self.scale[1]
    }

    #[inline]
    pub const fn scale_z(self) -> f32 {
        self.scale[2]
    }

    #[inline]
    pub fn rotation_z(self) -> f32 {
        self.rotation.roll()
    }

    #[inline]
    pub fn set_rotation_z(&mut self, radians: f32) {
        self.rotation = Quaternion::from_rotation_z(radians);
    }

    #[inline]
    pub fn rotate_z(&mut self, radians: f32) {
        self.rotation = (Quaternion::from_rotation_z(radians) * self.rotation).normalized();
    }

    #[inline]
    pub fn is_planar_2d(self) -> bool {
        self.rotation.is_planar_2d()
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

/// Scene hierarchy link used by the unified renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Parent(pub EntityId);

impl Parent {
    #[inline]
    pub const fn new(entity: EntityId) -> Self {
        Self(entity)
    }

    #[inline]
    pub const fn entity(self) -> EntityId {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_z_rotation_preserves_2d_helpers() {
        let angle = std::f32::consts::FRAC_PI_2;
        let quaternion = Quaternion::from_rotation_z(angle);
        let rotated = quaternion.rotate_vector([1.0, 0.0, 0.0]);

        assert!(quaternion.is_planar_2d());
        assert!((quaternion.roll() - angle).abs() <= 0.0001);
        assert!(rotated[0].abs() <= 0.0001);
        assert!((rotated[1] - 1.0).abs() <= 0.0001);
        assert!(rotated[2].abs() <= 0.0001);
    }

    #[test]
    fn pitched_quaternion_is_not_planar_2d() {
        let quaternion = Quaternion::from_euler_angles(0.35, 0.1, 0.0);
        assert!(!quaternion.is_planar_2d());
    }
}

/// High-level scene camera marker/config.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Camera {
    pub enabled: bool,
}

impl Camera {
    #[inline]
    pub const fn new() -> Self {
        Self { enabled: true }
    }

    #[inline]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// ECS/manual render-view descriptor for multi-camera rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraViewport {
    pub order: i32,
    pub viewport: ViewportRect,
    pub layer_mask: u32,
}

impl CameraViewport {
    #[inline]
    pub const fn new(viewport: ViewportRect) -> Self {
        Self {
            order: 0,
            viewport,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub const fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    #[inline]
    pub const fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }
}

impl Default for CameraViewport {
    fn default() -> Self {
        Self::new(ViewportRect::default())
    }
}

/// Shared render-layer mask used by the unified scene renderer.
///
/// When attached to a renderable entity, this overrides the layer mask stored
/// inside high-level renderer-specific components such as [`SpriteRenderer`] and
/// [`PointLight2D`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderLayerMask(pub u32);

impl RenderLayerMask {
    #[inline]
    pub const fn all() -> Self {
        Self(u32::MAX)
    }
}

impl Default for RenderLayerMask {
    fn default() -> Self {
        Self::all()
    }
}

/// Coarse scene ordering layer shared across renderable content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortingLayer(pub i32);

/// Stable order within a [`SortingLayer`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderInLayer(pub i32);

/// High-level sprite component consumed by the default scene renderer.
#[derive(Clone)]
pub struct SpriteRenderer {
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub uv: [f32; 4],
    pub texture: Option<Texture>,
    pub visible: bool,
    pub layer_mask: u32,
}

impl SpriteRenderer {
    #[inline]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            color: Color::WHITE,
            uv: [0.0, 0.0, 1.0, 1.0],
            texture: None,
            visible: true,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn texture(mut self, texture: Texture) -> Self {
        self.texture = Some(texture);
        self
    }

    #[inline]
    pub fn clear_texture(mut self) -> Self {
        self.texture = None;
        self
    }

    #[inline]
    pub fn uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.uv = [u_min, v_min, u_max, v_max];
        self
    }

    #[inline]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }
}

impl Default for SpriteRenderer {
    fn default() -> Self {
        Self::new(1.0, 1.0)
    }
}

impl std::fmt::Debug for SpriteRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpriteRenderer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("color", &self.color)
            .field("uv", &self.uv)
            .field("textured", &self.texture.is_some())
            .field("visible", &self.visible)
            .field("layer_mask", &self.layer_mask)
            .finish()
    }
}

/// High-level 2D point light component.
#[derive(Clone, Copy, Debug)]
pub struct PointLight2D {
    pub radius: f32,
    pub intensity: f32,
    pub color: Color,
    pub temperature: f32,
    pub falloff: f32,
    pub visible: bool,
    pub layer_mask: u32,
}

impl PointLight2D {
    #[inline]
    pub const fn new(radius: f32) -> Self {
        Self {
            radius,
            intensity: 1.0,
            color: Color::WHITE,
            temperature: 6500.0,
            falloff: 2.0,
            visible: true,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub const fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    #[inline]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub const fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    #[inline]
    pub const fn falloff(mut self, falloff: f32) -> Self {
        self.falloff = falloff;
        self
    }

    #[inline]
    pub const fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub const fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }
}

impl Default for PointLight2D {
    fn default() -> Self {
        Self::new(128.0)
    }
}

/// Marker component identifying the primary camera used by the high-level renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct MainCamera;

/// Bloom controls for the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct BloomSettings {
    pub enabled: bool,
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
}

impl Default for BloomSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.55,
            intensity: 0.45,
            radius: 1.15,
        }
    }
}

/// Tonemap controls for the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct ToneMapSettings {
    pub enabled: bool,
    pub exposure: f32,
    pub gamma: f32,
}

impl Default for ToneMapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            exposure: 1.35,
            gamma: 2.2,
        }
    }
}

/// Vignette controls for the high-level renderer.
#[derive(Clone, Copy, Debug)]
pub struct VignetteSettings {
    pub enabled: bool,
    pub intensity: f32,
    pub smoothness: f32,
}

impl Default for VignetteSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 0.35,
            smoothness: 0.28,
        }
    }
}

/// Per-frame render settings consumed from the [`crate::ecs::World`].
#[derive(Clone, Copy, Debug)]
pub struct RenderSettings {
    pub clear_color: Color,
    pub ambient_color: Color,
    pub bloom: BloomSettings,
    pub tonemap: ToneMapSettings,
    pub vignette: VignetteSettings,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            clear_color: Color::new(0.015, 0.016, 0.02, 1.0),
            ambient_color: Color::new(0.07, 0.075, 0.09, 1.0),
            bloom: BloomSettings::default(),
            tonemap: ToneMapSettings::default(),
            vignette: VignetteSettings::default(),
        }
    }
}

#[cfg(feature = "live2d")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Live2DModelInstance {
    pub model_path: PathBuf,
    pub visible: bool,
}

#[cfg(feature = "live2d")]
impl Live2DModelInstance {
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: path.into(),
            visible: true,
        }
    }

    #[inline]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}
