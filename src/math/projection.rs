use super::{
    matrix::Mat4,
    screen::{LogicalPoint, LogicalSize},
    transform::Transform,
    vector::{Vec2, Vec3},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    Orthographic {
        height: f32,
        zoom: f32,
    },
    OrthographicFixed {
        width: f32,
        height: f32,
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
    pub const fn orthographic(height: f32) -> Self {
        Self::Orthographic { height, zoom: 1.0 }
    }

    #[inline]
    pub const fn orthographic_fixed(width: f32, height: f32) -> Self {
        Self::OrthographicFixed {
            width,
            height,
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
    pub fn orthographic_size(self, viewport_size: Vec2) -> Option<Vec2> {
        match self {
            Projection::Orthographic { height, zoom } => {
                let viewport_width = viewport_size.x().max(1.0);
                let viewport_height = viewport_size.y().max(1.0);
                let aspect = viewport_width / viewport_height.max(f32::EPSILON);
                let height = height.max(f32::EPSILON) / zoom.max(f32::EPSILON);
                Some(Vec2::new(height * aspect, height))
            }
            Projection::OrthographicFixed {
                width,
                height,
                zoom,
            } => {
                let zoom = zoom.max(f32::EPSILON);
                Some(Vec2::new(
                    width.max(f32::EPSILON) / zoom,
                    height.max(f32::EPSILON) / zoom,
                ))
            }
            Projection::Perspective { .. } => None,
        }
    }

    #[inline]
    pub fn projection_matrix(self, viewport_size: Vec2) -> Mat4 {
        match self {
            Projection::Orthographic { .. } | Projection::OrthographicFixed { .. } => {
                let size = self
                    .orthographic_size(viewport_size)
                    .expect("orthographic projection should resolve a visible size");
                let hw = size.x() * 0.5;
                let hh = size.y() * 0.5;
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
            Projection::Orthographic { .. } | Projection::OrthographicFixed { .. } => -1.0,
            Projection::Perspective { near, .. } => near,
        }
    }

    #[inline]
    pub fn far_plane(self) -> f32 {
        match self {
            Projection::Orthographic { .. } | Projection::OrthographicFixed { .. } => 1.0,
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

    pub fn screen_to_world_in_viewport(
        self,
        transform: Transform,
        viewport_size: Vec2,
        screen: Vec2,
    ) -> Vec2 {
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

    #[inline]
    pub fn screen_to_world_logical(
        self,
        transform: Transform,
        viewport_size: LogicalSize,
        screen: LogicalPoint,
    ) -> Vec2 {
        self.screen_to_world_in_viewport(transform, viewport_size.to_vec2(), screen.to_vec2())
    }

    fn screen_ray(self, transform: Transform, viewport_size: Vec2, screen: Vec2) -> (Vec3, Vec3) {
        match self {
            Projection::Orthographic { .. } | Projection::OrthographicFixed { .. } => {
                let width = viewport_size.x().max(1.0);
                let height = viewport_size.y().max(1.0);
                let size = self
                    .orthographic_size(viewport_size)
                    .expect("orthographic projection should resolve a visible size");
                let hw = size.x() * 0.5;
                let hh = size.y() * 0.5;
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
        Self::orthographic(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Projection;
    use crate::math::{LogicalPoint, LogicalSize, Transform, Vec3};

    #[test]
    fn view_matrix_uses_full_transform_including_scale() {
        let projection = Projection::orthographic(32.0);
        let transform = Transform::from_xyz(10.0, -4.0, 2.0).with_scale3(2.0, 4.0, 1.0);
        let local = projection
            .view_matrix(transform)
            .transform_point3(transform.position + Vec3::new(2.0, 4.0, 0.0));

        assert!((local.x() - 1.0).abs() <= 1e-5);
        assert!((local.y() - 1.0).abs() <= 1e-5);
    }

    #[test]
    fn orthographic_height_resolves_width_from_viewport_aspect() {
        let projection = Projection::orthographic(720.0);
        assert_eq!(
            projection
                .orthographic_size(crate::math::Vec2::new(1280.0, 720.0))
                .unwrap()
                .to_array(),
            [1280.0, 720.0]
        );
        assert_eq!(
            projection
                .orthographic_size(crate::math::Vec2::new(1680.0, 720.0))
                .unwrap()
                .to_array(),
            [1680.0, 720.0]
        );
    }

    #[test]
    fn fixed_orthographic_ignores_viewport_aspect() {
        let projection = Projection::orthographic_fixed(1280.0, 720.0);
        assert_eq!(
            projection
                .orthographic_size(crate::math::Vec2::new(1920.0, 1080.0))
                .unwrap()
                .to_array(),
            [1280.0, 720.0]
        );
        assert_eq!(
            projection
                .orthographic_size(crate::math::Vec2::new(1080.0, 1920.0))
                .unwrap()
                .to_array(),
            [1280.0, 720.0]
        );
    }

    #[test]
    fn orthographic_screen_to_world_uses_resolved_aspect_size() {
        let projection = Projection::orthographic(720.0);
        let world = projection.screen_to_world_in_viewport(
            Transform::default(),
            crate::math::Vec2::new(1680.0, 720.0),
            crate::math::Vec2::new(1680.0, 0.0),
        );

        assert!((world.x() - 840.0).abs() <= 1e-5);
        assert!((world.y() - 360.0).abs() <= 1e-5);
    }

    #[test]
    fn logical_screen_to_world_keeps_input_and_viewport_in_the_same_unit() {
        let projection = Projection::orthographic(720.0);
        let world = projection.screen_to_world_logical(
            Transform::default(),
            LogicalSize::new(1280.0, 720.0),
            LogicalPoint::new(1024.0, 360.0),
        );

        assert!((world.x() - 384.0).abs() <= 1e-5);
        assert!(world.y().abs() <= 1e-5);
    }
}
