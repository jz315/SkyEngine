use crate::math::{Mat4, Vec3};
use crate::render::component::{CameraViewport, Transform};
use crate::render::view::ViewUniform;
use crate::render::ViewportRect;

use super::frustum::Frustum;
use super::Projection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneViewKind {
    Main,
    DirectionalShadow,
}

#[derive(Debug, Clone, Copy)]
pub struct SceneView {
    pub order: i32,
    execution_order: i32,
    pub viewport: ViewportRect,
    pub target_size: [u32; 2],
    pub clear_surface: bool,
    pub kind: SceneViewKind,
    shadow_binding: Option<usize>,
    pub layer_mask: u32,
    pub camera_transform: Transform,
    pub projection: Projection,
    pub view_matrix: [f32; 16],
    pub projection_matrix: [f32; 16],
    pub inverse_view: [f32; 16],
    pub camera_position: [f32; 3],
    pub near: f32,
    pub far: f32,
    pub time: f32,
    pub delta_time: f32,
    pub view_uniform: ViewUniform,
    pub frustum: Frustum,
    pub is_planar_2d: bool,
}

impl SceneView {
    #[inline]
    pub fn new(
        order: i32,
        viewport: ViewportRect,
        target_size: [u32; 2],
        clear_surface: bool,
        layer_mask: u32,
        camera_transform: Transform,
        projection: Projection,
        view_uniform: ViewUniform,
        is_planar_2d: bool,
    ) -> Self {
        Self::from_parts(
            order,
            viewport,
            target_size,
            clear_surface,
            SceneViewKind::Main,
            None,
            layer_mask,
            camera_transform,
            projection,
            view_uniform,
            is_planar_2d,
        )
    }

    #[inline]
    pub fn from_parts(
        order: i32,
        viewport: ViewportRect,
        target_size: [u32; 2],
        clear_surface: bool,
        kind: SceneViewKind,
        shadow_binding: Option<usize>,
        layer_mask: u32,
        camera_transform: Transform,
        projection: Projection,
        view_uniform: ViewUniform,
        is_planar_2d: bool,
    ) -> Self {
        let frustum = Frustum::from_view_proj(view_uniform.view_proj);
        Self {
            order,
            execution_order: order,
            viewport,
            target_size,
            clear_surface,
            kind,
            shadow_binding,
            layer_mask,
            camera_transform,
            projection,
            view_matrix: view_uniform.view,
            projection_matrix: view_uniform.projection,
            inverse_view: view_uniform.inverse_view,
            camera_position: [
                view_uniform.camera_position[0],
                view_uniform.camera_position[1],
                view_uniform.camera_position[2],
            ],
            near: view_uniform.near_far_time_delta[0],
            far: view_uniform.near_far_time_delta[1],
            time: view_uniform.near_far_time_delta[2],
            delta_time: view_uniform.near_far_time_delta[3],
            view_uniform,
            frustum,
            is_planar_2d,
        }
    }

    #[inline]
    pub fn world_to_view(self, world: [f32; 3]) -> [f32; 3] {
        Mat4::from_cols_array(self.view_matrix)
            .transform_point3(Vec3::from_array(world))
            .to_array()
    }

    #[inline]
    pub fn view_depth(self, world: [f32; 3]) -> f32 {
        -self.world_to_view(world)[2]
    }

    #[inline]
    pub fn frustum(self) -> Frustum {
        self.frustum
    }

    #[inline]
    pub fn execution_order(&self) -> i32 {
        self.execution_order
    }

    #[inline]
    pub fn set_execution_order(&mut self, execution_order: i32) {
        self.execution_order = execution_order;
    }

    #[inline]
    pub fn shadow_binding(&self) -> Option<usize> {
        self.shadow_binding
    }

    #[inline]
    pub fn with_shadow_binding(mut self, shadow_binding: usize) -> Self {
        self.shadow_binding = Some(shadow_binding);
        self
    }

    #[inline]
    pub fn presents_to_surface(&self) -> bool {
        matches!(self.kind, SceneViewKind::Main)
    }

    #[inline]
    pub fn is_shadow(&self) -> bool {
        matches!(self.kind, SceneViewKind::DirectionalShadow)
    }
}

pub(crate) fn fallback_scene_view(surface_size: [u32; 2]) -> SceneView {
    build_scene_view(
        Transform::default(),
        Some(Projection::orthographic(surface_size[1].max(1) as f32)),
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
    let projection =
        projection.unwrap_or_else(|| Projection::orthographic(target_size[1].max(1) as f32));
    let view_uniform = projection.view_uniform(transform, target_size);
    let is_planar_2d = transform.is_planar_2d()
        && matches!(
            projection,
            Projection::Orthographic { .. } | Projection::OrthographicFixed { .. }
        );
    SceneView::new(
        viewport_desc.order,
        viewport_rect,
        target_size,
        false,
        viewport_desc.layer_mask,
        transform,
        projection,
        view_uniform,
        is_planar_2d,
    )
}
