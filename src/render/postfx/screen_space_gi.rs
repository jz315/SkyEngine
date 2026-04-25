//! Screen-space diffuse GI with temporal reprojection and spatial filtering.

use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::math::Mat4;
use crate::render::component::ScreenSpaceGiSettings;
use crate::render::gpu::{
    FullscreenPass, FullscreenPipeline, RenderTarget, RenderTargetDescriptor,
};
use crate::render::postfx::PostFx;
use crate::render::view::{Color, SceneView};

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenSpaceGiUniform {
    inverse_projection: [f32; 16],
    inverse_view: [f32; 16],
    prev_view_proj: [f32; 16],
    viewport: [f32; 4],
    history_viewport: [f32; 4],
    ultra_viewport: [f32; 4],
    super_viewport: [f32; 4],
    hyper_viewport: [f32; 4],
    sky_color: [f32; 4],
    ground_color: [f32; 4],
    params0: [f32; 4], // intensity, resolve_radius_px, depth_reject, normal_reject
    params1: [f32; 4], // falloff, hemisphere_strength, temporal_blend, disocclusion
    params2: [f32; 4], // spatial_radius_px, spatial_depth_reject, spatial_normal_reject, history_valid
}

const SCREEN_SPACE_GI_SHADER: &str =
    include_str!("../shaders/postfx/screen_space_gi/screen_space_gi.wgsl");
const SCREEN_SPACE_GI_PREPROCESS_SHADER: &str =
    include_str!("../shaders/postfx/screen_space_gi/screen_space_gi_preprocess.wgsl");
const SCREEN_SPACE_GI_DIFFUSE_SHADER: &str =
    include_str!("../shaders/postfx/screen_space_gi/screen_space_gi_diffuse.wgsl");
const SCREEN_SPACE_GI_FILTER_SHADER: &str =
    include_str!("../shaders/postfx/screen_space_gi/screen_space_gi_filter.wgsl");
const SCREEN_SPACE_GI_UPSAMPLE_SHADER: &str =
    include_str!("../shaders/postfx/screen_space_gi/screen_space_gi_upsample.wgsl");
const SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE: u32 = 8;
const SCREEN_SPACE_GI_MIP_LEVEL_COUNT: u32 = 4;

fn screen_space_gi_internal_usage() -> wgpu::TextureUsages {
    wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::STORAGE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST
}

fn screen_space_gi_internal_format() -> wgpu::TextureFormat {
    wgpu::TextureFormat::Rgba16Float
}

struct ViewHistory {
    geometry_a: RenderTarget,
    geometry_b: RenderTarget,
    radiance: RenderTarget,
    diffuse_base: RenderTarget,
    diffuse: RenderTarget,
    raw: RenderTarget,
    history_a: RenderTarget,
    history_b: RenderTarget,
    prev_is_a: bool,
    prev_view_proj: [f32; 16],
    valid: bool,
}

impl ViewHistory {
    #[inline]
    fn low_res_extent(width: u32, height: u32) -> (u32, u32) {
        (width.div_ceil(2).max(1), height.div_ceil(2).max(1))
    }

    fn new_mipped_storage_target(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        mip_level_count: u32,
        label: impl Into<Cow<'static, str>>,
    ) -> RenderTarget {
        RenderTarget::from_descriptor(
            ctx,
            RenderTargetDescriptor::new(width, height, format)
                .usage(screen_space_gi_internal_usage())
                .mip_level_count(mip_level_count)
                .label(label),
        )
    }

    fn new_storage_target(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: impl Into<Cow<'static, str>>,
    ) -> RenderTarget {
        RenderTarget::from_descriptor(
            ctx,
            RenderTargetDescriptor::new(width, height, format)
                .usage(screen_space_gi_internal_usage())
                .label(label),
        )
    }

    fn resize_mipped_storage_target(
        target: &mut RenderTarget,
        ctx: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        mip_level_count: u32,
    ) {
        target.resize_with(
            ctx,
            RenderTargetDescriptor::new(width, height, format)
                .usage(screen_space_gi_internal_usage())
                .mip_level_count(mip_level_count)
                .label(target.label().to_owned()),
        );
    }

    fn resize_storage_target(
        target: &mut RenderTarget,
        ctx: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) {
        target.resize_with(
            ctx,
            RenderTargetDescriptor::new(width, height, format)
                .usage(screen_space_gi_internal_usage())
                .label(target.label().to_owned()),
        );
    }

    fn new(ctx: &GpuContext, width: u32, height: u32, _format: wgpu::TextureFormat) -> Self {
        let (width, height) = Self::low_res_extent(width, height);
        Self {
            geometry_a: Self::new_mipped_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
                "screen_space_gi_geometry_a",
            ),
            geometry_b: Self::new_mipped_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
                "screen_space_gi_geometry_b",
            ),
            radiance: Self::new_mipped_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
                "screen_space_gi_radiance",
            ),
            diffuse_base: Self::new_mipped_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
                "screen_space_gi_diffuse_base",
            ),
            diffuse: Self::new_mipped_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
                "screen_space_gi_diffuse",
            ),
            raw: Self::new_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                "screen_space_gi_raw",
            ),
            history_a: Self::new_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                "screen_space_gi_history_a",
            ),
            history_b: Self::new_storage_target(
                ctx,
                width,
                height,
                screen_space_gi_internal_format(),
                "screen_space_gi_history_b",
            ),
            prev_is_a: true,
            prev_view_proj: Mat4::IDENTITY.to_cols_array(),
            valid: false,
        }
    }

    fn resize_if_needed(
        &mut self,
        ctx: &GpuContext,
        width: u32,
        height: u32,
        _format: wgpu::TextureFormat,
    ) {
        let (width, height) = Self::low_res_extent(width, height);
        let size_changed = self.raw.width() != width || self.raw.height() != height;
        Self::resize_mipped_storage_target(
            &mut self.geometry_a,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
            SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
        );
        Self::resize_mipped_storage_target(
            &mut self.geometry_b,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
            SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
        );
        Self::resize_mipped_storage_target(
            &mut self.radiance,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
            SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
        );
        Self::resize_mipped_storage_target(
            &mut self.diffuse_base,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
            SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
        );
        Self::resize_mipped_storage_target(
            &mut self.diffuse,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
            SCREEN_SPACE_GI_MIP_LEVEL_COUNT,
        );
        Self::resize_storage_target(
            &mut self.raw,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
        );
        Self::resize_storage_target(
            &mut self.history_a,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
        );
        Self::resize_storage_target(
            &mut self.history_b,
            ctx,
            width,
            height,
            screen_space_gi_internal_format(),
        );
        if size_changed {
            self.valid = false;
        }
    }

    fn prev_history(&self) -> &RenderTarget {
        if self.prev_is_a {
            &self.history_a
        } else {
            &self.history_b
        }
    }

    fn curr_history(&self) -> &RenderTarget {
        if self.prev_is_a {
            &self.history_b
        } else {
            &self.history_a
        }
    }

    fn prev_geometry(&self) -> &RenderTarget {
        if self.prev_is_a {
            &self.geometry_a
        } else {
            &self.geometry_b
        }
    }

    fn curr_geometry(&self) -> &RenderTarget {
        if self.prev_is_a {
            &self.geometry_b
        } else {
            &self.geometry_a
        }
    }

    fn advance(&mut self, current_view_proj: [f32; 16]) {
        self.prev_is_a = !self.prev_is_a;
        self.prev_view_proj = current_view_proj;
        self.valid = true;
    }
}

pub struct ScreenSpaceGi {
    temporal_pipeline: wgpu::ComputePipeline,
    spatial_pipeline: wgpu::ComputePipeline,
    composite_pipeline: FullscreenPipeline,
    geometry_mips_pipeline: wgpu::ComputePipeline,
    radiance_mips_pipeline: wgpu::ComputePipeline,
    diffuse_mips_pipeline: wgpu::ComputePipeline,
    upsample_mip3_to_2_pipeline: wgpu::ComputePipeline,
    upsample_mip2_to_1_pipeline: wgpu::ComputePipeline,
    upsample_mip1_to_0_pipeline: wgpu::ComputePipeline,
    scene_bgl: wgpu::BindGroupLayout,
    pair_texture_bgl: wgpu::BindGroupLayout,
    diffuse_input_bgl: wgpu::BindGroupLayout,
    filter_output_bgl: wgpu::BindGroupLayout,
    upsample_input_bgl: wgpu::BindGroupLayout,
    mip_output_bgl: wgpu::BindGroupLayout,
    storage_output_bgl: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    view_histories: Vec<ViewHistory>,
}

impl ScreenSpaceGi {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let scene_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("screen_space_gi_scene_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let pair_texture_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_space_gi_pair_texture_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let diffuse_input_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_space_gi_diffuse_input_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let filter_output_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_space_gi_filter_output_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: screen_space_gi_internal_format(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });

        let upsample_input_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_space_gi_upsample_input_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let params_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("screen_space_gi_params_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let mip_output_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_space_gi_mip_output_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });

        let storage_output_bgl =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_space_gi_storage_output_bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba16Float,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    }],
                });

        let params_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen_space_gi_params"),
            size: std::mem::size_of::<ScreenSpaceGiUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen_space_gi_params_bg"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let composite_pipeline = FullscreenPipeline::new(
            ctx,
            SCREEN_SPACE_GI_SHADER,
            "fs_composite",
            &[
                &scene_bgl,
                &params_bgl,
                &pair_texture_bgl,
                &pair_texture_bgl,
            ],
            target_format,
            None,
            "screen_space_gi_composite",
        );

        let preprocess_shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("screen_space_gi_preprocess_shader"),
                source: wgpu::ShaderSource::Wgsl(SCREEN_SPACE_GI_PREPROCESS_SHADER.into()),
            });
        let preprocess_pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen_space_gi_preprocess_layout"),
                    bind_group_layouts: &[
                        &scene_bgl,
                        &params_bgl,
                        &pair_texture_bgl,
                        &mip_output_bgl,
                    ],
                    push_constant_ranges: &[],
                });
        let geometry_mips_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_geometry_mips_pipeline"),
                    layout: Some(&preprocess_pipeline_layout),
                    module: &preprocess_shader,
                    entry_point: Some("cs_build_geometry_mips"),
                    cache: None,
                    compilation_options: Default::default(),
                });
        let radiance_mips_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_radiance_mips_pipeline"),
                    layout: Some(&preprocess_pipeline_layout),
                    module: &preprocess_shader,
                    entry_point: Some("cs_build_radiance_mips"),
                    cache: None,
                    compilation_options: Default::default(),
                });
        let diffuse_shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("screen_space_gi_diffuse_shader"),
                source: wgpu::ShaderSource::Wgsl(SCREEN_SPACE_GI_DIFFUSE_SHADER.into()),
            });
        let filter_shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("screen_space_gi_filter_shader"),
                source: wgpu::ShaderSource::Wgsl(SCREEN_SPACE_GI_FILTER_SHADER.into()),
            });
        let upsample_shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("screen_space_gi_upsample_shader"),
                source: wgpu::ShaderSource::Wgsl(SCREEN_SPACE_GI_UPSAMPLE_SHADER.into()),
            });
        let diffuse_pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen_space_gi_diffuse_layout"),
                    bind_group_layouts: &[&params_bgl, &diffuse_input_bgl, &mip_output_bgl],
                    push_constant_ranges: &[],
                });
        let diffuse_mips_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_diffuse_mips_pipeline"),
                    layout: Some(&diffuse_pipeline_layout),
                    module: &diffuse_shader,
                    entry_point: Some("cs_build_diffuse_mips"),
                    cache: None,
                    compilation_options: Default::default(),
                });
        let temporal_pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen_space_gi_temporal_layout"),
                    bind_group_layouts: &[
                        &scene_bgl,
                        &params_bgl,
                        &pair_texture_bgl,
                        &filter_output_bgl,
                    ],
                    push_constant_ranges: &[],
                });
        let temporal_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_temporal_pipeline"),
                    layout: Some(&temporal_pipeline_layout),
                    module: &filter_shader,
                    entry_point: Some("cs_temporal"),
                    cache: None,
                    compilation_options: Default::default(),
                });
        let spatial_pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen_space_gi_spatial_layout"),
                    bind_group_layouts: &[
                        &scene_bgl,
                        &params_bgl,
                        &pair_texture_bgl,
                        &filter_output_bgl,
                    ],
                    push_constant_ranges: &[],
                });
        let spatial_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_spatial_pipeline"),
                    layout: Some(&spatial_pipeline_layout),
                    module: &filter_shader,
                    entry_point: Some("cs_spatial"),
                    cache: None,
                    compilation_options: Default::default(),
                });
        let upsample_pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen_space_gi_upsample_layout"),
                    bind_group_layouts: &[&params_bgl, &upsample_input_bgl, &storage_output_bgl],
                    push_constant_ranges: &[],
                });
        let upsample_mip3_to_2_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_upsample_mip3_to_2_pipeline"),
                    layout: Some(&upsample_pipeline_layout),
                    module: &upsample_shader,
                    entry_point: Some("cs_upsample_mip3_to_2"),
                    cache: None,
                    compilation_options: Default::default(),
                });
        let upsample_mip2_to_1_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_upsample_mip2_to_1_pipeline"),
                    layout: Some(&upsample_pipeline_layout),
                    module: &upsample_shader,
                    entry_point: Some("cs_upsample_mip2_to_1"),
                    cache: None,
                    compilation_options: Default::default(),
                });
        let upsample_mip1_to_0_pipeline =
            ctx.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("screen_space_gi_upsample_mip1_to_0_pipeline"),
                    layout: Some(&upsample_pipeline_layout),
                    module: &upsample_shader,
                    entry_point: Some("cs_upsample_mip1_to_0"),
                    cache: None,
                    compilation_options: Default::default(),
                });

        Self {
            temporal_pipeline,
            spatial_pipeline,
            composite_pipeline,
            geometry_mips_pipeline,
            radiance_mips_pipeline,
            diffuse_mips_pipeline,
            upsample_mip3_to_2_pipeline,
            upsample_mip2_to_1_pipeline,
            upsample_mip1_to_0_pipeline,
            scene_bgl,
            pair_texture_bgl,
            diffuse_input_bgl,
            filter_output_bgl,
            upsample_input_bgl,
            mip_output_bgl,
            storage_output_bgl,
            params_buffer,
            params_bind_group,
            view_histories: Vec::new(),
        }
    }

    #[inline]
    pub fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}

    fn ensure_history(
        &mut self,
        ctx: &GpuContext,
        view_index: usize,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> &mut ViewHistory {
        while self.view_histories.len() <= view_index {
            self.view_histories
                .push(ViewHistory::new(ctx, width, height, format));
        }
        let history = &mut self.view_histories[view_index];
        history.resize_if_needed(ctx, width, height, format);
        history
    }

    fn make_pair_bg(
        &self,
        ctx: &GpuContext,
        first_view: &wgpu::TextureView,
        second_view: &wgpu::TextureView,
        label: &'static str,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.pair_texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(first_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(second_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }

    fn make_diffuse_input_bg(
        &self,
        ctx: &GpuContext,
        radiance_view: &wgpu::TextureView,
        geometry_view: &wgpu::TextureView,
        label: &'static str,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.diffuse_input_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(radiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(geometry_view),
                },
            ],
        })
    }

    fn make_upsample_bg(
        &self,
        ctx: &GpuContext,
        low_geometry_view: &wgpu::TextureView,
        low_diffuse_view: &wgpu::TextureView,
        high_geometry_view: &wgpu::TextureView,
        high_diffuse_view: &wgpu::TextureView,
        label: &'static str,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.upsample_input_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(low_geometry_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(low_diffuse_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(high_geometry_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(high_diffuse_view),
                },
            ],
        })
    }

    fn make_filter_output_bg(
        &self,
        ctx: &GpuContext,
        first_view: &wgpu::TextureView,
        second_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        label: &'static str,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.filter_output_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(first_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(second_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
            ],
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn make_scene_bg(
        &self,
        ctx: &GpuContext,
        depth: &RenderTarget,
        normal: &RenderTarget,
        albedo: &RenderTarget,
        material: &RenderTarget,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen_space_gi_scene_bg"),
            layout: &self.scene_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(normal.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(albedo.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(material.view()),
                },
            ],
        })
    }

    fn make_mip_output_bg(
        &self,
        ctx: &GpuContext,
        first: &wgpu::TextureView,
        second: &wgpu::TextureView,
        third: &wgpu::TextureView,
        fourth: &wgpu::TextureView,
        label: &'static str,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.mip_output_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(first),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(second),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(third),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(fourth),
                },
            ],
        })
    }

    fn make_storage_output_bg(
        &self,
        ctx: &GpuContext,
        view: &wgpu::TextureView,
        label: &'static str,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.storage_output_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            }],
        })
    }

    fn write_uniform(
        &self,
        ctx: &GpuContext,
        scene_view: &SceneView,
        settings: ScreenSpaceGiSettings,
        ambient_color: Color,
        prev_view_proj: [f32; 16],
        history_valid: bool,
        history_size: [u32; 2],
        ultra_size: [u32; 2],
        super_size: [u32; 2],
        hyper_size: [u32; 2],
    ) {
        let inverse_projection = Mat4::from_cols_array(scene_view.projection_matrix)
            .inverse()
            .to_cols_array();
        let ground_color = [
            ambient_color.r * 0.38,
            ambient_color.g * 0.34,
            ambient_color.b * 0.30,
            ambient_color.a,
        ];
        let uniform = ScreenSpaceGiUniform {
            inverse_projection,
            inverse_view: scene_view.inverse_view,
            prev_view_proj,
            viewport: [
                scene_view.target_size[0] as f32,
                scene_view.target_size[1] as f32,
                1.0 / scene_view.target_size[0].max(1) as f32,
                1.0 / scene_view.target_size[1].max(1) as f32,
            ],
            history_viewport: [
                history_size[0] as f32,
                history_size[1] as f32,
                1.0 / history_size[0].max(1) as f32,
                1.0 / history_size[1].max(1) as f32,
            ],
            ultra_viewport: [
                ultra_size[0] as f32,
                ultra_size[1] as f32,
                1.0 / ultra_size[0].max(1) as f32,
                1.0 / ultra_size[1].max(1) as f32,
            ],
            super_viewport: [
                super_size[0] as f32,
                super_size[1] as f32,
                1.0 / super_size[0].max(1) as f32,
                1.0 / super_size[1].max(1) as f32,
            ],
            hyper_viewport: [
                hyper_size[0] as f32,
                hyper_size[1] as f32,
                1.0 / hyper_size[0].max(1) as f32,
                1.0 / hyper_size[1].max(1) as f32,
            ],
            sky_color: ambient_color.to_array(),
            ground_color,
            params0: [
                settings.intensity.max(0.0),
                settings.radius_px.max(1.0),
                settings.depth_reject.max(0.001),
                settings.normal_reject.max(0.001),
            ],
            params1: [settings.falloff.max(0.001), 0.65, 0.74, 8.0],
            params2: [1.2, 6.5, 18.0, history_valid as u32 as f32],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn render_fullscreen(
        ctx: &mut GpuContext,
        pipeline: &wgpu::RenderPipeline,
        output: &RenderTarget,
        bind_groups: &[&wgpu::BindGroup],
        label: &'static str,
    ) {
        Self::render_fullscreen_region(
            ctx,
            pipeline,
            output,
            bind_groups,
            label,
            None,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );
    }

    fn render_fullscreen_region(
        ctx: &mut GpuContext,
        pipeline: &wgpu::RenderPipeline,
        output: &RenderTarget,
        bind_groups: &[&wgpu::BindGroup],
        label: &'static str,
        viewport: Option<[u32; 4]>,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        if let Some([x, y, width, height]) = viewport {
            pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, width, height);
        }
        for (index, bind_group) in bind_groups.iter().enumerate() {
            pass.set_bind_group(index as u32, *bind_group, &[]);
        }
        FullscreenPass::draw(&mut pass);
    }

    fn dispatch_preprocess_compute(
        &self,
        ctx: &mut GpuContext,
        pipeline: &wgpu::ComputePipeline,
        scene_bg: &wgpu::BindGroup,
        input_bg: &wgpu::BindGroup,
        output_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
        label: &'static str,
    ) {
        let mut frame = ctx.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, scene_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        pass.set_bind_group(2, input_bg, &[]);
        pass.set_bind_group(3, output_bg, &[]);
        pass.dispatch_workgroups(
            width.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            height.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            1,
        );
    }

    fn dispatch_geometry_mips_compute(
        &self,
        ctx: &mut GpuContext,
        scene_bg: &wgpu::BindGroup,
        input_bg: &wgpu::BindGroup,
        output_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
    ) {
        self.dispatch_preprocess_compute(
            ctx,
            &self.geometry_mips_pipeline,
            scene_bg,
            input_bg,
            output_bg,
            width,
            height,
            "screen_space_gi_geometry_mips_compute",
        );
    }

    fn dispatch_radiance_mips_compute(
        &self,
        ctx: &mut GpuContext,
        scene_bg: &wgpu::BindGroup,
        input_bg: &wgpu::BindGroup,
        output_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
    ) {
        self.dispatch_preprocess_compute(
            ctx,
            &self.radiance_mips_pipeline,
            scene_bg,
            input_bg,
            output_bg,
            width,
            height,
            "screen_space_gi_radiance_mips_compute",
        );
    }

    fn dispatch_diffuse_mips_compute(
        &self,
        ctx: &mut GpuContext,
        input_bg: &wgpu::BindGroup,
        output_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
    ) {
        let mut frame = ctx.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("screen_space_gi_diffuse_mips_compute"),
            ..Default::default()
        });
        pass.set_pipeline(&self.diffuse_mips_pipeline);
        pass.set_bind_group(0, &self.params_bind_group, &[]);
        pass.set_bind_group(1, input_bg, &[]);
        pass.set_bind_group(2, output_bg, &[]);
        pass.dispatch_workgroups(
            width.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            height.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            1,
        );
    }

    fn dispatch_upsample_compute(
        &self,
        ctx: &mut GpuContext,
        pipeline: &wgpu::ComputePipeline,
        input_bg: &wgpu::BindGroup,
        output_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
        label: &'static str,
    ) {
        let mut frame = ctx.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.params_bind_group, &[]);
        pass.set_bind_group(1, input_bg, &[]);
        pass.set_bind_group(2, output_bg, &[]);
        pass.dispatch_workgroups(
            width.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            height.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            1,
        );
    }

    fn dispatch_temporal_compute(
        &self,
        ctx: &mut GpuContext,
        scene_bg: &wgpu::BindGroup,
        input_bg: &wgpu::BindGroup,
        filter_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
    ) {
        let mut frame = ctx.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("screen_space_gi_temporal_compute"),
            ..Default::default()
        });
        pass.set_pipeline(&self.temporal_pipeline);
        pass.set_bind_group(0, scene_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        pass.set_bind_group(2, input_bg, &[]);
        pass.set_bind_group(3, filter_bg, &[]);
        pass.dispatch_workgroups(
            width.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            height.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            1,
        );
    }

    fn dispatch_spatial_compute(
        &self,
        ctx: &mut GpuContext,
        scene_bg: &wgpu::BindGroup,
        input_bg: &wgpu::BindGroup,
        filter_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
    ) {
        let mut frame = ctx.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("screen_space_gi_spatial_compute"),
            ..Default::default()
        });
        pass.set_pipeline(&self.spatial_pipeline);
        pass.set_bind_group(0, scene_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        pass.set_bind_group(2, input_bg, &[]);
        pass.set_bind_group(3, filter_bg, &[]);
        pass.dispatch_workgroups(
            width.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            height.div_ceil(SCREEN_SPACE_GI_PREPROCESS_WORKGROUP_SIZE),
            1,
        );
    }

    pub fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        depth: &RenderTarget,
        normal: &RenderTarget,
        velocity: &RenderTarget,
        albedo: &RenderTarget,
        material: &RenderTarget,
        _emissive: &RenderTarget,
        output: &RenderTarget,
        scene_view: &SceneView,
        settings: ScreenSpaceGiSettings,
        ambient_color: Color,
        view_index: usize,
    ) {
        self.ensure_history(
            ctx,
            view_index,
            output.width(),
            output.height(),
            output.format(),
        );

        let scene_bg = self.make_scene_bg(ctx, depth, normal, albedo, material);
        let radiance_bg = self.make_pair_bg(
            ctx,
            input.view(),
            velocity.view(),
            "screen_space_gi_radiance_bg",
        );

        let (prev_view_proj, history_valid, history_size, ultra_size, super_size, hyper_size) = {
            let history = &self.view_histories[view_index];
            let history_size = [
                history.curr_geometry().width(),
                history.curr_geometry().height(),
            ];
            let (ultra_width, ultra_height) = history.curr_geometry().mip_extent(1);
            let (super_width, super_height) = history.curr_geometry().mip_extent(2);
            let (hyper_width, hyper_height) = history.curr_geometry().mip_extent(3);
            (
                history.prev_view_proj,
                history.valid,
                history_size,
                [ultra_width, ultra_height],
                [super_width, super_height],
                [hyper_width, hyper_height],
            )
        };
        self.write_uniform(
            ctx,
            scene_view,
            settings,
            ambient_color,
            prev_view_proj,
            history_valid,
            history_size,
            ultra_size,
            super_size,
            hyper_size,
        );

        {
            let history = &self.view_histories[view_index];
            let geometry_mips_bg = self.make_mip_output_bg(
                ctx,
                &history.curr_geometry().create_mip_view(0),
                &history.curr_geometry().create_mip_view(1),
                &history.curr_geometry().create_mip_view(2),
                &history.curr_geometry().create_mip_view(3),
                "screen_space_gi_geometry_mips_bg",
            );
            self.dispatch_geometry_mips_compute(
                ctx,
                &scene_bg,
                &radiance_bg,
                &geometry_mips_bg,
                history.curr_geometry().width(),
                history.curr_geometry().height(),
            );

            let radiance_mips_bg = self.make_mip_output_bg(
                ctx,
                &history.radiance.create_mip_view(0),
                &history.radiance.create_mip_view(1),
                &history.radiance.create_mip_view(2),
                &history.radiance.create_mip_view(3),
                "screen_space_gi_radiance_mips_bg",
            );
            self.dispatch_radiance_mips_compute(
                ctx,
                &scene_bg,
                &radiance_bg,
                &radiance_mips_bg,
                history.radiance.width(),
                history.radiance.height(),
            );
        }

        {
            let history = &self.view_histories[view_index];
            let diffuse_input_bg = self.make_diffuse_input_bg(
                ctx,
                history.radiance.view(),
                history.curr_geometry().view(),
                "screen_space_gi_diffuse_input_bg",
            );
            let diffuse_mips_bg = self.make_mip_output_bg(
                ctx,
                &history.diffuse_base.create_mip_view(0),
                &history.diffuse_base.create_mip_view(1),
                &history.diffuse_base.create_mip_view(2),
                &history.diffuse_base.create_mip_view(3),
                "screen_space_gi_diffuse_mips_bg",
            );
            self.dispatch_diffuse_mips_compute(
                ctx,
                &diffuse_input_bg,
                &diffuse_mips_bg,
                history.diffuse_base.width(),
                history.diffuse_base.height(),
            );
        }

        {
            let history = &self.view_histories[view_index];
            let upsample_bg = self.make_upsample_bg(
                ctx,
                &history.curr_geometry().create_mip_view(3),
                &history.diffuse_base.create_mip_view(3),
                &history.curr_geometry().create_mip_view(2),
                &history.diffuse_base.create_mip_view(2),
                "screen_space_gi_upsample_mip3_to_2_bg",
            );
            let output_bg = self.make_storage_output_bg(
                ctx,
                &history.diffuse.create_mip_view(2),
                "screen_space_gi_upsample_mip3_to_2_output_bg",
            );
            let (width, height) = history.diffuse.mip_extent(2);
            self.dispatch_upsample_compute(
                ctx,
                &self.upsample_mip3_to_2_pipeline,
                &upsample_bg,
                &output_bg,
                width,
                height,
                "screen_space_gi_upsample_mip3_to_2",
            );
        }

        {
            let history = &self.view_histories[view_index];
            let upsample_bg = self.make_upsample_bg(
                ctx,
                &history.curr_geometry().create_mip_view(2),
                &history.diffuse.create_mip_view(2),
                &history.curr_geometry().create_mip_view(1),
                &history.diffuse_base.create_mip_view(1),
                "screen_space_gi_upsample_mip2_to_1_bg",
            );
            let output_bg = self.make_storage_output_bg(
                ctx,
                &history.diffuse.create_mip_view(1),
                "screen_space_gi_upsample_mip2_to_1_output_bg",
            );
            let (width, height) = history.diffuse.mip_extent(1);
            self.dispatch_upsample_compute(
                ctx,
                &self.upsample_mip2_to_1_pipeline,
                &upsample_bg,
                &output_bg,
                width,
                height,
                "screen_space_gi_upsample_mip2_to_1",
            );
        }

        {
            let history = &self.view_histories[view_index];
            let upsample_bg = self.make_upsample_bg(
                ctx,
                &history.curr_geometry().create_mip_view(1),
                &history.diffuse.create_mip_view(1),
                &history.curr_geometry().create_mip_view(0),
                &history.diffuse_base.create_mip_view(0),
                "screen_space_gi_upsample_mip1_to_0_bg",
            );
            let output_bg = self.make_storage_output_bg(
                ctx,
                &history.diffuse.create_mip_view(0),
                "screen_space_gi_upsample_mip1_to_0_output_bg",
            );
            let (width, height) = history.diffuse.mip_extent(0);
            self.dispatch_upsample_compute(
                ctx,
                &self.upsample_mip1_to_0_pipeline,
                &upsample_bg,
                &output_bg,
                width,
                height,
                "screen_space_gi_upsample_mip1_to_0",
            );
        }

        {
            let history = &self.view_histories[view_index];
            let temporal_input_bg = self.make_pair_bg(
                ctx,
                history.diffuse.view(),
                velocity.view(),
                "screen_space_gi_temporal_input_bg",
            );
            let filter_bg = self.make_filter_output_bg(
                ctx,
                history.prev_history().view(),
                history.prev_geometry().view(),
                history.curr_history().view(),
                "screen_space_gi_temporal_filter_bg",
            );
            self.dispatch_temporal_compute(
                ctx,
                &scene_bg,
                &temporal_input_bg,
                &filter_bg,
                history.curr_history().width(),
                history.curr_history().height(),
            );
        }

        {
            let history = &self.view_histories[view_index];
            let spatial_bg = self.make_pair_bg(
                ctx,
                history.curr_history().view(),
                history.curr_geometry().view(),
                "screen_space_gi_spatial_bg",
            );
            let filter_bg = self.make_filter_output_bg(
                ctx,
                history.curr_history().view(),
                history.curr_geometry().view(),
                history.raw.view(),
                "screen_space_gi_spatial_filter_bg",
            );
            self.dispatch_spatial_compute(
                ctx,
                &scene_bg,
                &spatial_bg,
                &filter_bg,
                history.raw.width(),
                history.raw.height(),
            );
        }

        {
            let history = &self.view_histories[view_index];
            let composite_bg = self.make_pair_bg(
                ctx,
                input.view(),
                history.raw.view(),
                "screen_space_gi_composite_bg",
            );
            let composite_normal_bg = self.make_pair_bg(
                ctx,
                history.curr_geometry().view(),
                albedo.view(),
                "screen_space_gi_composite_normal_bg",
            );
            let composite_pipeline = self.composite_pipeline.pipeline(ctx, output.format());
            Self::render_fullscreen(
                ctx,
                composite_pipeline.as_ref(),
                output,
                &[
                    &scene_bg,
                    &self.params_bind_group,
                    &composite_bg,
                    &composite_normal_bg,
                ],
                "screen_space_gi_composite",
            );
        }

        self.view_histories[view_index].advance(scene_view.view_uniform.view_proj);
    }
}

impl PostFx for ScreenSpaceGi {
    fn apply_to_target(
        &mut self,
        _ctx: &mut GpuContext,
        _input: &RenderTarget,
        _output: &RenderTarget,
    ) {
        panic!("ScreenSpaceGi requires depth + scene-view context; call apply_to_target directly");
    }
}
