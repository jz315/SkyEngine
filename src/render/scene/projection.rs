use crate::render::core::camera::{Camera2D, ViewUniform};
use crate::render::ecs::Transform;

use super::math::{
    column_major_mul, scene_rotation_matrix, scene_view_matrix, transform_direction,
    transform_local_point, transform_point,
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

    pub fn projection_matrix(self, viewport: [u32; 2]) -> [f32; 16] {
        let width = viewport[0].max(1) as f32;
        let height = viewport[1].max(1) as f32;
        match self {
            Projection::Orthographic {
                viewport_width,
                viewport_height,
                zoom,
            } => {
                let hw = viewport_width.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                let hh = viewport_height.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                [
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
                ]
            }
            Projection::Perspective {
                vertical_fov_radians,
                near,
                far,
            } => {
                let aspect = width / height.max(f32::EPSILON);
                let f = 1.0 / (0.5 * vertical_fov_radians.max(0.001)).tan();
                let range_inv = 1.0 / (near - far).min(-f32::EPSILON);
                [
                    f / aspect,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    f,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    (near + far) * range_inv,
                    -1.0,
                    0.0,
                    0.0,
                    (2.0 * near * far) * range_inv,
                    0.0,
                ]
            }
        }
    }

    pub fn view_matrix(self, transform: Transform) -> [f32; 16] {
        let _ = self;
        scene_view_matrix(transform)
    }

    pub fn world_to_view(self, transform: Transform, world: [f32; 3]) -> [f32; 3] {
        let _ = self;
        transform_point(self.view_matrix(transform), world)
    }

    #[inline]
    pub fn view_depth(self, transform: Transform, world: [f32; 3]) -> f32 {
        -self.world_to_view(transform, world)[2]
    }

    pub fn view_uniform(self, transform: Transform, viewport: [u32; 2]) -> ViewUniform {
        let width = viewport[0].max(1) as f32;
        let height = viewport[1].max(1) as f32;
        let inv_width = width.recip();
        let inv_height = height.recip();
        let view_proj = column_major_mul(
            self.projection_matrix(viewport),
            self.view_matrix(transform),
        );

        ViewUniform {
            view_proj,
            camera: [transform.x(), transform.y(), transform.z(), 1.0],
            viewport: [width, height, inv_width, inv_height],
        }
    }

    pub fn screen_to_world(
        self,
        transform: Transform,
        viewport: [u32; 2],
        screen: [f32; 2],
    ) -> [f32; 2] {
        let (origin, direction) = self.screen_ray(transform, viewport, screen);
        if direction[2].abs() <= f32::EPSILON {
            return [origin[0], origin[1]];
        }
        let distance = -origin[2] / direction[2];
        [
            origin[0] + direction[0] * distance,
            origin[1] + direction[1] * distance,
        ]
    }

    fn screen_ray(
        self,
        transform: Transform,
        viewport: [u32; 2],
        screen: [f32; 2],
    ) -> ([f32; 3], [f32; 3]) {
        match self {
            Projection::Orthographic {
                viewport_width,
                viewport_height,
                zoom,
            } => {
                let width = viewport[0].max(1) as f32;
                let height = viewport[1].max(1) as f32;
                let hw = viewport_width.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                let hh = viewport_height.max(f32::EPSILON) * 0.5 / zoom.max(f32::EPSILON);
                let rotation = scene_rotation_matrix(transform.rotation);
                let local = [
                    (screen[0] / width - 0.5) * 2.0 * hw,
                    -(screen[1] / height - 0.5) * 2.0 * hh,
                    0.0,
                ];
                (
                    transform_local_point(rotation, local, transform),
                    transform_direction(rotation, [0.0, 0.0, -1.0]),
                )
            }
            Projection::Perspective {
                vertical_fov_radians,
                ..
            } => {
                let width = viewport[0].max(1) as f32;
                let height = viewport[1].max(1) as f32;
                let aspect = width / height.max(f32::EPSILON);
                let ndc_x = (screen[0] / width) * 2.0 - 1.0;
                let ndc_y = 1.0 - (screen[1] / height) * 2.0;
                let tan_half_fov = (vertical_fov_radians.max(0.001) * 0.5).tan();
                let local_dir = [ndc_x * aspect * tan_half_fov, ndc_y * tan_half_fov, -1.0f32];
                let rotation = scene_rotation_matrix(transform.rotation);
                (transform.position, transform_direction(rotation, local_dir))
            }
        }
    }
}

impl Default for Projection {
    fn default() -> Self {
        Self::orthographic(1.0, 1.0)
    }
}

pub(crate) fn orthographic_cull_camera(
    transform: Transform,
    projection: Projection,
) -> Option<Camera2D> {
    let Projection::Orthographic {
        viewport_width,
        viewport_height,
        zoom,
    } = projection
    else {
        return None;
    };
    if !transform.is_planar_2d() {
        return None;
    }
    let mut camera = Camera2D::new(viewport_width, viewport_height);
    camera.position = [transform.x(), transform.y()];
    camera.rotation = transform.rotation_z();
    camera.zoom = zoom.max(f32::EPSILON);
    Some(camera)
}
