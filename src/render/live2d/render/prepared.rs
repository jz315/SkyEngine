//! Prepared per-frame Live2D render data.
//!
//! This is the execution boundary between higher-level Live2D frame
//! preparation and the actual GPU pass submission performed by
//! [`super::renderer::Live2DRenderer`].

use crate::gpu::UploadSlice;

/// Frame-prepared Live2D work ready for GPU pass submission.
pub struct PreparedLive2DFrame {
    target_format: wgpu::TextureFormat,
    passes: Vec<PreparedTargetPass>,
    final_root_color: [f32; 4],
}

impl PreparedLive2DFrame {
    #[inline]
    pub(crate) fn new(
        target_format: wgpu::TextureFormat,
        passes: Vec<PreparedTargetPass>,
        final_root_color: [f32; 4],
    ) -> Self {
        Self {
            target_format,
            passes,
            final_root_color,
        }
    }

    #[inline]
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    #[inline]
    pub fn mask_draw_count(&self) -> usize {
        self.passes.iter().map(|pass| pass.mask_draws.len()).sum()
    }

    #[inline]
    pub fn model_draw_count(&self) -> usize {
        self.passes.iter().map(|pass| pass.items.len()).sum()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }

    #[inline]
    pub(crate) fn passes(&self) -> &[PreparedTargetPass] {
        &self.passes
    }

    #[inline]
    pub(crate) fn final_root_color(&self) -> [f32; 4] {
        self.final_root_color
    }

    #[inline]
    pub(crate) fn has_offscreen_passes(&self) -> bool {
        self.passes
            .iter()
            .any(|pass| matches!(pass.target, PreparedPassTarget::Offscreen(_)))
    }

    #[inline]
    pub(crate) fn has_backdrop_draws(&self) -> bool {
        self.passes
            .iter()
            .any(PreparedTargetPass::has_backdrop_draws)
    }
}

pub(crate) enum PreparedPassTarget {
    Root,
    Offscreen(usize),
}

pub(crate) struct PreparedTargetPass {
    pub(crate) target: PreparedPassTarget,
    pub(crate) clear: bool,
    pub(crate) requires_mask: bool,
    pub(crate) mask_draws: Vec<PreparedMaskDraw>,
    pub(crate) items: Vec<PreparedTargetItem>,
}

impl PreparedTargetPass {
    pub(crate) fn has_backdrop_draws(&self) -> bool {
        self.items.iter().any(PreparedTargetItem::requires_backdrop)
    }
}

pub(crate) enum PreparedTargetItem {
    Drawable(PreparedModelDraw),
    Composite(PreparedCompositeDraw),
}

pub(crate) struct PreparedMaskDraw {
    pub(crate) pipeline_kind: PreparedMaskPipelineKind,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) vertex_upload: UploadSlice,
    pub(crate) index_upload: UploadSlice,
    pub(crate) index_count: u32,
    pub(crate) uniform_offset: u32,
    pub(crate) texture_bind_group_key: [usize; 2],
    pub(crate) texture_bg: wgpu::BindGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedMaskPipelineKind {
    Unculled,
    Culled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedModelPipelineKind {
    Normal,
    NormalCulled,
    Additive,
    AdditiveCulled,
    Multiplicative,
    MultiplicativeCulled,
    Overlap,
    OverlapCulled,
}

pub(crate) struct PreparedModelDraw {
    pub(crate) pipeline_kind: PreparedModelPipelineKind,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) vertex_upload: UploadSlice,
    pub(crate) index_upload: UploadSlice,
    pub(crate) index_count: u32,
    pub(crate) uniform_offset: u32,
    pub(crate) uses_mask: bool,
    pub(crate) requires_backdrop: bool,
    pub(crate) texture_bind_group_key: [usize; 3],
    pub(crate) texture_bg: wgpu::BindGroup,
}

pub(crate) struct PreparedCompositeDraw {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) uniform_offset: u32,
    pub(crate) uses_mask: bool,
    pub(crate) source_texture_id: usize,
    pub(crate) source_view: wgpu::TextureView,
}

impl PreparedTargetItem {
    pub(crate) fn uses_mask(&self) -> bool {
        match self {
            Self::Drawable(draw) => draw.uses_mask,
            Self::Composite(draw) => draw.uses_mask,
        }
    }

    pub(crate) fn requires_backdrop(&self) -> bool {
        match self {
            Self::Drawable(draw) => draw.requires_backdrop,
            Self::Composite(_) => true,
        }
    }
}
