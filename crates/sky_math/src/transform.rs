use super::{matrix::Mat4, quaternion::Quat, vector::Vec3};

/// Shared 3D TRS transform used by engine systems.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    pub scale: Vec3,
    pub rotation: Quat,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        scale: Vec3::ONE,
        rotation: Quat::IDENTITY,
    };

    #[inline]
    pub fn from_xy(x: f32, y: f32) -> Self {
        Self::from_xyz(x, y, 0.0)
    }

    #[inline]
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            position: Vec3::new(x, y, z),
            scale: Vec3::ONE,
            rotation: Quat::IDENTITY,
        }
    }

    #[inline]
    pub fn from_position(position: Vec3) -> Self {
        Self {
            position,
            scale: Vec3::ONE,
            rotation: Quat::IDENTITY,
        }
    }

    #[inline]
    pub fn from_matrix4(matrix: Mat4) -> Self {
        let (scale, rotation, position) = matrix.to_scale_rotation_translation();
        Self {
            position,
            scale,
            rotation,
        }
    }

    #[inline]
    pub fn with_position(mut self, position: Vec3) -> Self {
        self.position = position;
        self
    }

    #[inline]
    pub fn with_z(mut self, z: f32) -> Self {
        self.position[2] = z;
        self
    }

    #[inline]
    pub fn with_scale(mut self, scale_x: f32, scale_y: f32) -> Self {
        self.scale[0] = scale_x;
        self.scale[1] = scale_y;
        self
    }

    #[inline]
    pub fn with_scale_uniform(mut self, scale: f32) -> Self {
        self.scale = Vec3::splat(scale);
        self
    }

    #[inline]
    pub fn with_scale3(mut self, scale_x: f32, scale_y: f32, scale_z: f32) -> Self {
        self.scale = Vec3::new(scale_x, scale_y, scale_z);
        self
    }

    #[inline]
    pub fn with_rotation_quat(mut self, rotation: Quat) -> Self {
        self.rotation = rotation.normalized();
        self
    }

    #[inline]
    pub fn with_rotation(mut self, rotation: f32) -> Self {
        self.rotation = Quat::from_rotation_z(rotation);
        self
    }

    #[inline]
    pub fn with_euler_angles(mut self, pitch: f32, yaw: f32, roll: f32) -> Self {
        self.rotation = Quat::from_euler_angles(pitch, yaw, roll);
        self
    }

    #[inline]
    pub fn transform_vector(self, vector: Vec3) -> Vec3 {
        let scaled = self.scale * vector;
        self.rotation.rotate_vec3(scaled)
    }

    #[inline]
    pub fn transform_point(self, point: Vec3) -> Vec3 {
        self.position + self.transform_vector(point)
    }

    #[inline]
    pub fn try_inverse_transform_vector(self, vector: Vec3) -> Option<Vec3> {
        let unrotated = self.rotation.inverse().rotate_vec3(vector);
        Some(Vec3::new(
            checked_div(unrotated.x(), self.scale.x())?,
            checked_div(unrotated.y(), self.scale.y())?,
            checked_div(unrotated.z(), self.scale.z())?,
        ))
    }

    #[inline]
    pub fn try_inverse_transform_point(self, point: Vec3) -> Option<Vec3> {
        self.try_inverse_transform_vector(point - self.position)
    }

    #[inline]
    pub fn mul_transform(self, local: Self) -> Self {
        Self::from_parts(
            self.position + self.transform_vector(local.position),
            (self.rotation * local.rotation).normalized(),
            self.scale * local.scale,
        )
    }

    #[inline]
    pub fn right(self) -> Vec3 {
        self.rotation.rotate_vec3(Vec3::X)
    }

    #[inline]
    pub fn up(self) -> Vec3 {
        self.rotation.rotate_vec3(Vec3::Y)
    }

    #[inline]
    pub fn forward(self) -> Vec3 {
        self.rotation.rotate_vec3(-Vec3::Z)
    }

    #[inline]
    pub fn x(self) -> f32 {
        self.position[0]
    }

    #[inline]
    pub fn y(self) -> f32 {
        self.position[1]
    }

    #[inline]
    pub fn z(self) -> f32 {
        self.position[2]
    }

    #[inline]
    pub fn scale_x(self) -> f32 {
        self.scale[0]
    }

    #[inline]
    pub fn scale_y(self) -> f32 {
        self.scale[1]
    }

    #[inline]
    pub fn scale_z(self) -> f32 {
        self.scale[2]
    }

    #[inline]
    pub fn rotation_z(self) -> f32 {
        self.rotation.roll()
    }

    #[inline]
    pub fn from_parts(position: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            position,
            rotation,
            scale,
        }
    }

    #[inline]
    pub fn set_rotation_z(&mut self, radians: f32) {
        self.rotation = Quat::from_rotation_z(radians);
    }

    #[inline]
    pub fn rotate_z(&mut self, radians: f32) {
        self.rotation = (Quat::from_rotation_z(radians) * self.rotation).normalized();
    }

    #[inline]
    pub fn to_matrix4(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }

    #[inline]
    pub fn is_planar_2d(self) -> bool {
        self.rotation.is_planar_2d()
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[inline]
fn checked_div(value: f32, divisor: f32) -> Option<f32> {
    if divisor.abs() <= f32::EPSILON {
        None
    } else {
        Some(value / divisor)
    }
}

#[cfg(test)]
mod tests {
    use super::Transform;

    #[test]
    fn from_xy_is_the_canonical_2d_convenience_constructor() {
        assert_eq!(Transform::from_xy(3.0, 4.0).z(), 0.0);
    }

    #[test]
    fn inverse_transform_point_undoes_transform_point() {
        let transform = Transform::from_xyz(10.0, -4.0, 2.0)
            .with_scale3(2.0, 3.0, 4.0)
            .with_rotation(std::f32::consts::FRAC_PI_2);
        let local = crate::Vec3::new(2.0, 3.0, 4.0);
        let world = transform.transform_point(local);
        let restored = transform.try_inverse_transform_point(world).unwrap();

        for (actual, expected) in restored.to_array().into_iter().zip(local.to_array()) {
            assert!(
                (actual - expected).abs() <= 1.0e-5,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn zero_scale_has_no_inverse_transform() {
        let transform = Transform::default().with_scale3(1.0, 0.0, 1.0);
        assert!(transform
            .try_inverse_transform_point(crate::Vec3::ONE)
            .is_none());
    }

    #[test]
    fn matrix_round_trip_preserves_common_trs_parts() {
        let transform = Transform::from_xyz(1.0, 2.0, 3.0)
            .with_scale_uniform(2.0)
            .with_euler_angles(0.1, 0.2, 0.3);
        let round_trip = Transform::from_matrix4(transform.to_matrix4());

        assert!((round_trip.position.distance(transform.position)) <= 1.0e-5);
        assert!((round_trip.scale.distance(transform.scale)) <= 1.0e-5);
        assert!((round_trip.rotation.dot(transform.rotation) - 1.0).abs() <= 1.0e-5);
    }
}
