use crate::render::core::camera::{Camera2D, ViewUniform};
use crate::render::ecs::{CameraViewport, Transform};
use crate::render::frame_pipeline::PreparedView;
use crate::render::ViewportRect;

use super::projection::{orthographic_cull_camera, Projection};

#[derive(Debug, Clone, Copy)]
pub struct SceneView {
    pub order: i32,
    pub viewport: ViewportRect,
    pub target_size: [u32; 2],
    pub clear_surface: bool,
    pub layer_mask: u32,
    pub camera_transform: Transform,
    pub projection: Projection,
    pub view_uniform: ViewUniform,
    pub cull_camera_2d: Option<Camera2D>,
}

impl SceneView {
    #[inline]
    pub const fn new(
        order: i32,
        viewport: ViewportRect,
        target_size: [u32; 2],
        clear_surface: bool,
        layer_mask: u32,
        camera_transform: Transform,
        projection: Projection,
        view_uniform: ViewUniform,
        cull_camera_2d: Option<Camera2D>,
    ) -> Self {
        Self {
            order,
            viewport,
            target_size,
            clear_surface,
            layer_mask,
            camera_transform,
            projection,
            view_uniform,
            cull_camera_2d,
        }
    }

    #[inline]
    pub fn world_to_view(self, world: [f32; 3]) -> [f32; 3] {
        self.projection.world_to_view(self.camera_transform, world)
    }

    #[inline]
    pub fn view_depth(self, world: [f32; 3]) -> f32 {
        self.projection.view_depth(self.camera_transform, world)
    }
}

pub(crate) fn default_scene_view_from_prepared(view: &PreparedView<'_>) -> SceneView {
    let projection =
        Projection::orthographic(view.target_size()[0] as f32, view.target_size()[1] as f32);
    let cull_camera = orthographic_cull_camera(Transform::default(), projection);
    SceneView::new(
        view.order(),
        view.viewport(),
        view.target_size(),
        view.clear_surface(),
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), view.target_size()),
        cull_camera,
    )
}

pub(crate) fn fallback_scene_view(surface_size: [u32; 2]) -> SceneView {
    build_scene_view(
        Transform::default(),
        Some(Projection::orthographic(
            surface_size[0].max(1) as f32,
            surface_size[1].max(1) as f32,
        )),
        Some(CameraViewport::new(ViewportRect::from_surface_size(
            surface_size,
        ))),
        surface_size,
    )
}

pub(crate) fn build_scene_view(
    transform: Transform,
    projection: Option<Projection>,
    viewport: Option<CameraViewport>,
    surface_size: [u32; 2],
) -> SceneView {
    let viewport_desc = viewport
        .unwrap_or_else(|| CameraViewport::new(ViewportRect::from_surface_size(surface_size)));
    let viewport_rect = viewport_desc.viewport.clamp_to_surface(surface_size);
    let target_size = viewport_rect.size();
    let projection = projection.unwrap_or_else(|| {
        Projection::orthographic(target_size[0].max(1) as f32, target_size[1].max(1) as f32)
    });
    let view_uniform = projection.view_uniform(transform, target_size);
    let cull_camera_2d = orthographic_cull_camera(transform, projection);
    SceneView::new(
        viewport_desc.order,
        viewport_rect,
        target_size,
        false,
        viewport_desc.layer_mask,
        transform,
        projection,
        view_uniform,
        cull_camera_2d,
    )
}
