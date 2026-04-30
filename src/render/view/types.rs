use crate::render::runtime::RenderTimingStats;

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    pub step_count: usize,
    pub view_count: usize,
    pub sprite_count: usize,
    pub light_count: usize,
    pub draw_calls: usize,
    pub shadow_cascade_count: usize,
    pub shadow_caster_count: usize,
    pub shadow_caster_count_by_cascade:
        [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES],
    pub shadow_draw_calls: usize,
    pub shadow_draw_calls_by_cascade:
        [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES],
    pub shadow_atlas_width: u32,
    pub shadow_atlas_height: u32,
    pub shadow_atlas_rect_count: usize,
    pub shadow_atlas_used_pixel_ratio: f32,
    pub shadow_atlas_guard_band_texels: f32,
    pub passes: usize,
    pub resident_render_assets: usize,
    pub uploaded_render_assets: usize,
    pub loading_render_assets: usize,
    pub missing_render_assets: usize,
    pub failed_render_assets: usize,
    pub dirty_sprite_slots: usize,
    pub dirty_light_slots: usize,
    pub visible_sprite_upload_count: usize,
    pub visible_light_upload_count: usize,
    pub timings: RenderTimingStats,
}

pub(crate) const SCENE_HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderQueueSort {
    OpaqueDepthFrontToBack,
    TransparentScene,
    OverlayStable,
}
