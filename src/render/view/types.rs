use crate::render::runtime::RenderTimingStats;

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    pub step_count: usize,
    pub view_count: usize,
    pub sprite_count: usize,
    pub light_count: usize,
    pub draw_calls: usize,
    pub passes: usize,
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
