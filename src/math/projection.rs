use super::{
    matrix::Mat4,
    transform::Transform,
    vector::{Vec2, Vec3},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    Orthographic {
        viewport_width: f32,
        viewport_height: f32,
        zoom: f32,
    },
    Perspective {
        vertical_fov_radians: f32,
        near: f32,
        far: f32,
    },
}

impl Projection {
    #[inline]
    pub const fn orthographic(viewport_width: f32, viewport_height: f32) -> Self {
        Self::Orthographic {
            viewport_width,
            viewport_height,
            zoom: 1.0,
        }
    }

    #[inline]
    pub const fn perspective(vertical_fov_radians: f32, near: f32, far: f32) -> Self {
        Self::Perspective {
            vertical_fov_radians,
            near,
            far,
        }
    }

    #[inline]
    pub fn projection_matrix(self, viewport_size: Vec2) -> Mat4 {
        match self {
            Projection::Orthographic {
                viewport_width,
                viewport_height,
                zoom,
            } => {
                let hw = viewport_width.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                let hh = viewport_height.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                Mat4::from_cols_array([
                    hw.recip(),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    hh.recip(),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ])
            }
            Projection::Perspective {
                vertical_fov_radians,
                near,
                far,
            } => {
                let width = viewport_size.x().max(1.0);
                let height = viewport_size.y().max(1.0);
                Mat4::perspective_rh(
                    vertical_fov_radians,
                    width / height.max(f32::EPSILON),
                    near,
                    far,
                )
            }
        }
    }

    #[inline]
    pub fn near_plane(self) -> f32 {
        match self {
            Projection::Orthographic { .. } => -1.0,
            Projection::Perspective { near, .. } => near,
        }
    }

    #[inline]
    pub fn far_plane(self) -> f32 {
        match self {
            Projection::Orthographic { .. } => 1.0,
            Projection::Perspective { far, .. } => far,
        }
    }

    #[inline]
    pub fn view_matrix(self, transform: Transform) -> Mat4 {
        let _ = self;
        transform.to_matrix4().inverse()
    }

    #[inline]
    pub fn world_to_view(self, transform: Transform, world: Vec3) -> Vec3 {
        self.view_matrix(transform).transform_point3(world)
    }

    #[inline]
    pub fn view_depth(self, transform: Transform, world: Vec3) -> f32 {
        -self.world_to_view(transform, world).z()
    }

    pub fn screen_to_world(self, transform: Transform, viewport_size: Vec2, screen: Vec2) -> Vec2 {
        let (origin, direction) = self.screen_ray(transform, viewport_size, screen);
        if direction.z().abs() <= f32::EPSILON {
            return Vec2::new(origin.x(), origin.y());
        }
        let distance = -origin.z() / direction.z();
        Vec2::new(
            origin.x() + direction.x() * distance,
            origin.y() + direction.y() * distance,
        )
    }

    fn screen_ray(self, transform: Transform, viewport_size: Vec2, screen: Vec2) -> (Vec3, Vec3) {
        match self {
            Projection::Orthographic {
                viewport_width,
                viewport_height,
                zoom,
            } => {
                let width = viewport_size.x().max(1.0);
                let height = viewport_size.y().max(1.0);
                let hw = viewport_width.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                let hh = viewport_height.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                let local = Vec3::new(
                    (screen.x() / width - 0.5) * 2.0 * hw,
                    -(screen.y() / height - 0.5) * 2.0 * hh,
                    0.0,
                );
                (
                    transform.transform_point(local),
                    transform
                        .rotation
                        .to_matrix4()
                        .transform_vector3(Vec3::new(0.0, 0.0, -1.0)),
                )
            }
            Projection::Perspective {
                vertical_fov_radians,
                ..
            } => {
                let width = viewport_size.x().max(1.0);
                let height = viewport_size.y().max(1.0);
                let aspect = width / height.max(f32::EPSILON);
                let ndc_x = (screen.x() / width) * 2.0 - 1.0;
                let ndc_y = 1.0 - (screen.y() / height) * 2.0;
                let tan_half_fov = (vertical_fov_radians.max(0.001) * 0.5).tan();
                let local_dir =
                    Vec3::new(ndc_x * aspect * tan_half_fov, ndc_y * tan_half_fov, -1.0);
                (
                    transform.position,
                    transform.rotation.to_matrix4().transform_vector3(local_dir),
                )
            }
        }
    }
}

impl Default for Projection {
    fn default() -> Self {
        Self::orthographic(1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Projection;
    use crate::math::{Transform, Vec3};

    #[test]
    fn view_matrix_uses_full_transform_including_scale() {
        let projection = Projection::orthographic(32.0, 32.0);
        let transform = Transform::from_xyz(10.0, -4.0, 2.0).with_scale3(2.0, 4.0, 1.0);
        let local = projection
            .view_matrix(transform)
            .transform_point3(transform.position + Vec3::new(2.0, 4.0, 0.0));

        assert!((local.x() - 1.0).abs() <= 1e-5);
        assert!((local.y() - 1.0).abs() <= 1e-5);
    }
}
