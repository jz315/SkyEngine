// GPU renderer for Live2D models.
//
// Follows the `SpriteBatch` pattern: create with `GpuContext`, own pipelines
// and buffers, per-frame upload + draw. Ported from SakuraEngine's
// `Live2DRendererImpl` and the two render passes (mask + model).

use rustc_hash::FxHashMap;

use crate::gpu::{DynamicUniformBuffer, GpuContext};

use super::super::clipping::{ClippingManager, ClippingObjectKind};
use super::super::prepared::{PreparedPassTarget, PreparedTargetItem};
use crate::render::gpu::FullscreenPipeline;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::Texture;

/// Per-drawable uniform data uploaded to the GPU.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Live2DUniforms {
    pub(super) projection_matrix: [f32; 16],
    pub(super) clip_matrix: [f32; 16],
    pub(super) base_color: [f32; 4],
    pub(super) multiply_color: [f32; 4],
    pub(super) screen_color: [f32; 4],
    pub(super) channel_flag: [f32; 4],
    pub(super) use_mask: f32,
    pub(super) inverted_mask: f32,
    pub(super) color_blend_type: u32,
    pub(super) alpha_blend_type: u32,
}

/// Live2D vertex (position + UV).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Live2DVertex {
    pub(super) position: [f32; 2],
    pub(super) uv: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TextureBindGroupKey {
    pub(super) color_texture: usize,
    pub(super) mask_texture: usize,
    pub(super) destination_texture: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct CompositeBindGroupKey {
    pub(super) source_texture: usize,
    pub(super) mask_texture: usize,
    pub(super) destination_texture: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Live2DCompositeUniforms {
    pub(super) clip_matrix: [f32; 16],
    pub(super) base_color: [f32; 4],
    pub(super) multiply_color: [f32; 4],
    pub(super) screen_color: [f32; 4],
    pub(super) channel_flag: [f32; 4],
    pub(super) use_mask: f32,
    pub(super) inverted_mask: f32,
    pub(super) color_blend_type: u32,
    pub(super) alpha_blend_type: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Live2DBlitUniforms {
    pub(super) base_color: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MaskRequest {
    pub(super) kind: ClippingObjectKind,
    pub(super) context_index: usize,
}

pub(super) struct SegmentBuilder {
    pub(super) target: PreparedPassTarget,
    pub(super) clear: bool,
    pub(super) mask_requests: Vec<MaskRequest>,
    pub(super) items: Vec<PreparedTargetItem>,
}

impl SegmentBuilder {
    pub(super) fn new(target: PreparedPassTarget, clear: bool) -> Self {
        Self {
            target,
            clear,
            mask_requests: Vec::new(),
            items: Vec::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(super) fn push_mask_request(&mut self, request: Option<MaskRequest>) {
        let Some(request) = request else {
            return;
        };
        if !self.mask_requests.contains(&request) {
            self.mask_requests.push(request);
        }
    }
}

/// GPU renderer for Live2D models.
///
/// # Usage
/// ```no_run
/// # use sky_engine::gpu::GpuContext;
/// # use sky_engine::render::Texture;
/// # use sky_engine::render::expert::live2d::{Live2DModel, Live2DRenderer};
/// # use sky_engine::render::expert::live2d::render::clipping::ClippingManager;
/// # fn frame(ctx: &mut GpuContext, target: &crate::render::expert::RenderTarget, model: &Live2DModel, textures: &[Texture], clipping_mgr: &mut Option<ClippingManager>) {
/// let mut renderer = Live2DRenderer::new(ctx);
/// let prepared = renderer.prepare_frame_for_target(ctx, target, model, textures, clipping_mgr);
/// renderer.execute_prepared_to_target(ctx, target, &prepared);
/// # }
/// ```
pub struct Live2DRenderer {
    pub(super) shader: wgpu::ShaderModule,
    pub(super) composite_shader: wgpu::ShaderModule,
    pub(super) uniforms: DynamicUniformBuffer<Live2DUniforms>,
    pub(super) composite_uniforms: DynamicUniformBuffer<Live2DCompositeUniforms>,
    pub(super) blit_uniforms: DynamicUniformBuffer<Live2DBlitUniforms>,
    pub(super) uniform_bgl: wgpu::BindGroupLayout,
    pub(super) composite_uniform_bgl: wgpu::BindGroupLayout,
    pub(super) texture_bgl: wgpu::BindGroupLayout,
    pub(super) blend_texture_bgl: wgpu::BindGroupLayout,
    pub(super) composite_texture_bgl: wgpu::BindGroupLayout,
    // 1x1 dummy texture for mask pass (avoids texture usage conflict)
    pub(super) dummy_texture: Texture,
    // Blit infrastructure for complex surface execution.
    pub(super) blit_pipeline: FullscreenPipeline,
    pub(super) blit_texture_bgl: wgpu::BindGroupLayout,
    // Model pass pipelines (per blend mode × format)
    pub(super) model_pipeline_normal: Option<wgpu::RenderPipeline>,
    pub(super) model_pipeline_normal_culled: Option<wgpu::RenderPipeline>,
    pub(super) model_pipeline_additive: Option<wgpu::RenderPipeline>,
    pub(super) model_pipeline_additive_culled: Option<wgpu::RenderPipeline>,
    pub(super) model_pipeline_multiplicative: Option<wgpu::RenderPipeline>,
    pub(super) model_pipeline_multiplicative_culled: Option<wgpu::RenderPipeline>,
    pub(super) model_pipeline_overlap: Option<wgpu::RenderPipeline>,
    pub(super) model_pipeline_overlap_culled: Option<wgpu::RenderPipeline>,
    // Mask pass pipeline
    pub(super) mask_pipeline: Option<wgpu::RenderPipeline>,
    pub(super) mask_pipeline_culled: Option<wgpu::RenderPipeline>,
    // Offscreen composite pipelines.
    pub(super) composite_pipeline_normal: Option<wgpu::RenderPipeline>,
    pub(super) composite_pipeline_additive: Option<wgpu::RenderPipeline>,
    pub(super) composite_pipeline_multiplicative: Option<wgpu::RenderPipeline>,
    pub(super) composite_pipeline_overlap: Option<wgpu::RenderPipeline>,
    // Mask texture (created on first use)
    pub(super) mask_texture: Option<RenderTarget>,
    // Complex offscreen path targets.
    pub(super) root_intermediate_target: Option<RenderTarget>,
    pub(super) blend_backup_target: Option<RenderTarget>,
    pub(super) offscreen_targets: Vec<RenderTarget>,
    // Cached texture bind groups keyed by sampled color/mask views.
    pub(super) cached_texture_bind_groups: FxHashMap<TextureBindGroupKey, wgpu::BindGroup>,
    pub(super) cached_composite_bind_groups: FxHashMap<CompositeBindGroupKey, wgpu::BindGroup>,
    pub(super) cached_blit_bind_groups: FxHashMap<usize, wgpu::BindGroup>,
    pub(super) cached_offscreen_clipping_model: Option<usize>,
    pub(super) cached_offscreen_clipping: Option<ClippingManager>,
    // Cached target format
    pub(super) cached_format: Option<wgpu::TextureFormat>,
    // Reused scratch for per-draw vertex packing.
    pub(super) vertex_scratch: Vec<Live2DVertex>,
}

impl Live2DRenderer {
    pub(crate) fn new_internal(ctx: &GpuContext) -> Self {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("live2d_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../../../shaders/live2d/live2d.wgsl").into(),
                ),
            });
        let composite_shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("live2d_offscreen_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../../../shaders/live2d/live2d_offscreen.wgsl").into(),
                ),
            });
        let uniforms = DynamicUniformBuffer::new(
            ctx,
            "live2d_uniform_buf",
            wgpu::ShaderStages::VERTEX_FRAGMENT,
        );
        let uniform_bgl = uniforms.bind_group_layout().clone();
        let composite_uniforms = DynamicUniformBuffer::new(
            ctx,
            "live2d_composite_uniform_buf",
            wgpu::ShaderStages::VERTEX_FRAGMENT,
        );
        let composite_uniform_bgl = composite_uniforms.bind_group_layout().clone();
        let blit_uniforms = DynamicUniformBuffer::new(
            ctx,
            "live2d_root_blit_uniform_buf",
            wgpu::ShaderStages::FRAGMENT,
        );
        let blit_uniform_bgl = blit_uniforms.bind_group_layout().clone();

        // Texture bind group layout (group 1): color_texture + mask_texture + sampler
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("live2d_texture_bgl"),
                entries: &[
                    // color_texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // mask_texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let blend_texture_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("live2d_blend_texture_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });
        let composite_texture_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("live2d_composite_texture_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });
        let blit_texture_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("live2d_blit_texture_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });
        let blend_normal = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let blit_pipeline = FullscreenPipeline::new(
            ctx,
            "struct RootBlitUniforms {\n    base_color: vec4<f32>,\n};\n@group(0) @binding(0) var<uniform> blit: RootBlitUniforms;\n@group(1) @binding(0) var input_tex: texture_2d<f32>;\n@group(1) @binding(1) var input_sampler: sampler;\n@fragment\nfn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {\n    return textureSample(input_tex, input_sampler, in.uv) * blit.base_color;\n}",
            "fs_main",
            &[&blit_uniform_bgl, &blit_texture_bgl],
            ctx.surface_format(),
            Some(blend_normal),
            "live2d_root_blit",
        );

        // 1x1 white dummy texture for mask-pass bind groups
        let dummy_texture = Texture::from_rgba8_with_format(
            ctx,
            1,
            1,
            &[255, 255, 255, 255],
            wgpu::TextureFormat::Rgba8Unorm,
            "live2d_dummy",
        );

        Self {
            shader,
            composite_shader,
            uniforms,
            composite_uniforms,
            blit_uniforms,
            uniform_bgl,
            composite_uniform_bgl,
            texture_bgl,
            blend_texture_bgl,
            composite_texture_bgl,
            dummy_texture,
            blit_pipeline,
            blit_texture_bgl,
            model_pipeline_normal: None,
            model_pipeline_normal_culled: None,
            model_pipeline_additive: None,
            model_pipeline_additive_culled: None,
            model_pipeline_multiplicative: None,
            model_pipeline_multiplicative_culled: None,
            model_pipeline_overlap: None,
            model_pipeline_overlap_culled: None,
            mask_pipeline: None,
            mask_pipeline_culled: None,
            composite_pipeline_normal: None,
            composite_pipeline_additive: None,
            composite_pipeline_multiplicative: None,
            composite_pipeline_overlap: None,
            mask_texture: None,
            root_intermediate_target: None,
            blend_backup_target: None,
            offscreen_targets: Vec::new(),
            cached_texture_bind_groups: FxHashMap::default(),
            cached_composite_bind_groups: FxHashMap::default(),
            cached_blit_bind_groups: FxHashMap::default(),
            cached_offscreen_clipping_model: None,
            cached_offscreen_clipping: None,
            cached_format: None,
            vertex_scratch: Vec::with_capacity(512),
        }
    }

    // Prepare all Live2D draw data needed to render into the current surface.
}
