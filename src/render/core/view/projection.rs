use crate::math::{Projection, Transform, Vec2};
use crate::render::view::ViewUniform;

pub(crate) trait ProjectionViewUniformExt {
    fn view_uniform(self, transform: Transform, viewport: [u32; 2]) -> ViewUniform;
}

impl ProjectionViewUniformExt for Projection {
    fn view_uniform(self, transform: Transform, viewport: [u32; 2]) -> ViewUniform {
        let viewport_size = Vec2::new(viewport[0].max(1) as f32, viewport[1].max(1) as f32);
        let view = self.view_matrix(transform);
        let projection = self.projection_matrix(viewport_size);
        let view_proj = projection * view;

        ViewUniform {
            view_proj: view_proj.to_cols_array(),
            camera: [transform.x(), transform.y(), transform.z(), 1.0],
            viewport: [
                viewport_size.x(),
                viewport_size.y(),
                viewport_size.x().recip(),
                viewport_size.y().recip(),
            ],
            view: view.to_cols_array(),
            projection: projection.to_cols_array(),
            inverse_view: transform.to_matrix4().to_cols_array(),
            camera_position: [transform.x(), transform.y(), transform.z(), 1.0],
            near_far_time_delta: [self.near_plane(), self.far_plane(), 0.0, 0.0],
        }
    }
}
