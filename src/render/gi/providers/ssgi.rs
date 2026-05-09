//! WickedEngine-inspired screen-space diffuse GI.
//!
//! The pass follows the practical shape of Wicked's SSGI path: deinterleave
//! depth/color into 2x/4x/8x/16x atlas resources, compute a low-resolution
//! diffuse bounce, then bilateral-upsample it into scene color. WickedEngine is
//! MIT licensed; this module is a SkyEngine WGSL/Rust implementation inspired
//! by that design, not a vendored C++ dependency.

use crate::gpu::GpuContext;
use crate::math::Mat4;
use crate::render::component::GlobalIllumination;
use crate::render::execution::{
    pass_first_write_texture, pass_nth_read_texture, require_render_target,
};
use crate::render::gi::{
    downcast_settings, GiCompositeDescriptor, GiProviderFactory, GiProviderId, GiProviderRuntime,
    GiSamplingBinding, GiSceneInput, GiSettings, GiShaderDescriptor,
};
use crate::render::gpu::{ComputePipelineCache, FullscreenPass, FullscreenPipeline};
use crate::render::graph::{
    CompiledPass, PassFlags, RenderGraph, RenderGraphError, ResourceRef, TargetSize, TextureHandle,
    TextureSubresource,
};
use crate::render::pipeline::{PostFxPassExecuteContext, PostFxPassSetupContext, TextureSpec};
use crate::render::view::SceneView;
use std::sync::OnceLock;

pub const SSGI_PROVIDER_ID: GiProviderId = "sky.ssgi";

const SSGI_FINAL_SHADER: &str = include_str!("../../shaders/gi/ssgi_final.wgsl");
const SSGI_DEINTERLEAVE_COMPUTE_SHADER: &str =
    include_str!("../../shaders/gi/ssgi_deinterleave_compute.wgsl");
const SSGI_COMPUTE_SHADER: &str = include_str!("../../shaders/gi/ssgi_compute.wgsl");
const SSGI_UPSAMPLE_COMPUTE_SHADER: &str =
    include_str!("../../shaders/gi/ssgi_upsample_compute.wgsl");
const SSGI_MIP_COUNT: usize = 4;
const SSGI_ATLAS_LAYERS: u32 = 16;
const SSGI_INTERNAL_ALIGNMENT: u32 = 64;
const SSGI_COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SSGI_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
const SSGI_NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SSGI_COMPUTE_TEXTURE_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::TEXTURE_BINDING
    .union(wgpu::TextureUsages::STORAGE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);
pub(crate) const SSGI_COMPUTE_RESOURCES_BLACKBOARD: &str = "ssgi_compute_resources";
const SSGI_TEXTURE_ATLAS_COLOR: &str = "ssgi_texture_atlas_color";
const SSGI_TEXTURE_ATLAS_DEPTH: &str = "ssgi_texture_atlas_depth";
const SSGI_TEXTURE_DEPTH_MIPS: &str = "ssgi_texture_depth_mips";
const SSGI_TEXTURE_NORMAL_MIPS: &str = "ssgi_texture_normal_mips";
const SSGI_TEXTURE_DIFFUSE_MIPS: &str = "ssgi_texture_diffuse_mips";
const SSGI_FINAL_PASS: &str = "ssgi_final_upsample";

/// Screen-space GI controls for the Wicked-inspired provider.
#[derive(Clone, Copy, Debug)]
pub struct SsgiSettings {
    /// Final composite strength. Wicked applies SSGI as a separate indirect term;
    /// `1.0` preserves that energy in SkyEngine's current post-composite path.
    pub intensity: f32,
    /// Public radius hint. The current WGSL pass maps `8.0` to Wicked's narrow
    /// `range = 2, spread = 2` SSGI sampling pass.
    pub radius_pixels: f32,
    /// Wicked's SSGI depth rejection distance; the shader uses its reciprocal.
    pub depth_rejection: f32,
    /// Wicked's bilateral normal threshold for SSGI upsample-style rejection.
    pub normal_power: f32,
}

impl Default for SsgiSettings {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            radius_pixels: 8.0,
            depth_rejection: 8.0,
            normal_power: 64.0,
        }
    }
}

#[inline]
pub fn global_illumination(settings: SsgiSettings) -> GlobalIllumination {
    GlobalIllumination::provider(crate::render::gi::GiProviderConfig::new(
        SSGI_PROVIDER_ID,
        settings,
    ))
}

const SSGI_COMPUTE_DEINTERLEAVE_PASSES: [&str; SSGI_MIP_COUNT] = [
    "ssgi_compute_deinterleave_2x",
    "ssgi_compute_deinterleave_4x",
    "ssgi_compute_deinterleave_8x",
    "ssgi_compute_deinterleave_16x",
];
const SSGI_COMPUTE_DIFFUSE_PASSES: [&str; SSGI_MIP_COUNT] = [
    "ssgi_compute_diffuse_2x",
    "ssgi_compute_diffuse_4x",
    "ssgi_compute_diffuse_8x",
    "ssgi_compute_diffuse_16x",
];
const SSGI_COMPUTE_UPSAMPLE_PASSES: [&str; SSGI_MIP_COUNT - 1] = [
    "ssgi_compute_upsample_16x_to_8x",
    "ssgi_compute_upsample_8x_to_4x",
    "ssgi_compute_upsample_4x_to_2x",
];
const SSGI_DIFFUSE_PARAMS: [SsgiSampleParams; SSGI_MIP_COUNT] = [
    SsgiSampleParams {
        range: 2.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 2.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 4.0,
        spread: 4.0,
    },
    SsgiSampleParams {
        range: 8.0,
        spread: 2.0,
    },
];
const SSGI_UPSAMPLE_PARAMS: [SsgiSampleParams; SSGI_MIP_COUNT] = [
    SsgiSampleParams {
        range: 3.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 2.0,
        spread: 3.0,
    },
    SsgiSampleParams {
        range: 1.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 2.0,
        spread: 1.0,
    },
];

fn render_debug_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SKY_RENDER_DEBUG_LOG")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn should_log_scene_view(scene_view: &SceneView) -> bool {
    render_debug_log_enabled() && scene_view.temporal.frame_index % 120 == 0
}

#[derive(Clone, Copy)]
struct SsgiSampleParams {
    range: f32,
    spread: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SsgiUniform {
    params0: [f32; 4],
    params1: [f32; 4],
    params2: [f32; 4],
    inverse_projection: [f32; 16],
}

/// Small runtime resource tracker for the SSGI pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SsgiResources {
    target_size: [u32; 2],
    aligned_size: [u32; 2],
    atlas_size: [u32; 2],
    mip_chain: [SsgiMipLevel; SSGI_MIP_COUNT],
}

/// Wicked-style SSGI mip dimensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SsgiMipLevel {
    pub scale: u32,
    pub atlas_size: [u32; 2],
    pub regular_size: [u32; 2],
}

/// Texture contract for the upcoming compute-backed SSGI path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SsgiComputeTextureLayout {
    pub atlas_size: [u32; 2],
    pub regular_mip_size: [u32; 2],
    pub mip_level_count: u32,
    pub atlas_layer_count: u32,
    pub atlas_color_format: wgpu::TextureFormat,
    pub atlas_depth_format: wgpu::TextureFormat,
    pub depth_mip_format: wgpu::TextureFormat,
    pub normal_mip_format: wgpu::TextureFormat,
    pub diffuse_mip_format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
}

#[derive(Clone, Debug)]
struct SsgiComputeTextureSpecs {
    atlas_color: TextureSpec,
    atlas_depth: TextureSpec,
    depth_mips: TextureSpec,
    normal_mips: TextureSpec,
    diffuse_mips: TextureSpec,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SsgiComputeGraphResources {
    pub(crate) atlas_color: TextureHandle,
    pub(crate) atlas_depth: TextureHandle,
    pub(crate) depth_mips: TextureHandle,
    pub(crate) normal_mips: TextureHandle,
    pub(crate) diffuse_mips: TextureHandle,
}

#[allow(dead_code)]
impl SsgiComputeGraphResources {
    #[inline]
    pub(crate) fn atlas_color_layer(self, mip_level: u32, layer: u32) -> TextureSubresource {
        TextureSubresource::new(self.atlas_color, mip_level, 1, layer, 1)
    }

    #[inline]
    pub(crate) fn atlas_depth_layer(self, mip_level: u32, layer: u32) -> TextureSubresource {
        TextureSubresource::new(self.atlas_depth, mip_level, 1, layer, 1)
    }

    #[inline]
    pub(crate) fn depth_mip(self, mip_level: u32) -> TextureSubresource {
        TextureSubresource::new(self.depth_mips, mip_level, 1, 0, 1)
    }

    #[inline]
    pub(crate) fn normal_mip(self, mip_level: u32) -> TextureSubresource {
        TextureSubresource::new(self.normal_mips, mip_level, 1, 0, 1)
    }

    #[inline]
    pub(crate) fn diffuse_mip(self, mip_level: u32) -> TextureSubresource {
        TextureSubresource::new(self.diffuse_mips, mip_level, 1, 0, 1)
    }
}

impl SsgiResources {
    #[inline]
    pub const fn target_size(&self) -> [u32; 2] {
        self.target_size
    }

    #[inline]
    pub const fn aligned_size(&self) -> [u32; 2] {
        self.aligned_size
    }

    #[inline]
    pub const fn atlas_size(&self) -> [u32; 2] {
        self.atlas_size
    }

    #[inline]
    pub const fn atlas_layers(&self) -> u32 {
        SSGI_ATLAS_LAYERS
    }

    #[inline]
    pub const fn mip_level(&self, index: usize) -> Option<SsgiMipLevel> {
        if index < SSGI_MIP_COUNT {
            Some(self.mip_chain[index])
        } else {
            None
        }
    }

    #[inline]
    pub fn compute_texture_layout(&self) -> SsgiComputeTextureLayout {
        let regular_mip_size = self
            .mip_chain
            .first()
            .map(|mip| [mip.regular_size[0].max(1), mip.regular_size[1].max(1)])
            .unwrap_or([1, 1]);
        SsgiComputeTextureLayout {
            atlas_size: [self.atlas_size[0].max(1), self.atlas_size[1].max(1)],
            regular_mip_size,
            mip_level_count: SSGI_MIP_COUNT as u32,
            atlas_layer_count: SSGI_ATLAS_LAYERS,
            atlas_color_format: SSGI_COLOR_FORMAT,
            atlas_depth_format: SSGI_DEPTH_FORMAT,
            depth_mip_format: SSGI_DEPTH_FORMAT,
            normal_mip_format: SSGI_NORMAL_FORMAT,
            diffuse_mip_format: SSGI_COLOR_FORMAT,
            usage: SSGI_COMPUTE_TEXTURE_USAGE,
        }
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.target_size = [width.max(1), height.max(1)];
        self.aligned_size = [
            align_to(self.target_size[0], SSGI_INTERNAL_ALIGNMENT),
            align_to(self.target_size[1], SSGI_INTERNAL_ALIGNMENT),
        ];
        self.atlas_size = [
            self.aligned_size[0].div_ceil(8).max(1),
            self.aligned_size[1].div_ceil(8).max(1),
        ];
        let regular_base = [
            self.aligned_size[0].div_ceil(2).max(1),
            self.aligned_size[1].div_ceil(2).max(1),
        ];
        let mut levels = [SsgiMipLevel::default(); SSGI_MIP_COUNT];
        for (index, level) in levels.iter_mut().enumerate() {
            let scale = 1u32 << index;
            *level = SsgiMipLevel {
                scale: scale * 2,
                atlas_size: [
                    (self.atlas_size[0] / scale).max(1),
                    (self.atlas_size[1] / scale).max(1),
                ],
                regular_size: [
                    (regular_base[0] / scale).max(1),
                    (regular_base[1] / scale).max(1),
                ],
            };
        }
        self.mip_chain = levels;
    }
}

impl SsgiComputeTextureSpecs {
    fn from_layout(layout: SsgiComputeTextureLayout) -> Self {
        let usage = layout.usage;
        Self {
            atlas_color: TextureSpec::new(SSGI_TEXTURE_ATLAS_COLOR, layout.atlas_color_format)
                .exact(layout.atlas_size[0], layout.atlas_size[1])
                .usage(usage)
                .mips(layout.mip_level_count)
                .array_layers(layout.atlas_layer_count),
            atlas_depth: TextureSpec::new(SSGI_TEXTURE_ATLAS_DEPTH, layout.atlas_depth_format)
                .exact(layout.atlas_size[0], layout.atlas_size[1])
                .usage(usage)
                .mips(layout.mip_level_count)
                .array_layers(layout.atlas_layer_count),
            depth_mips: TextureSpec::new(SSGI_TEXTURE_DEPTH_MIPS, layout.depth_mip_format)
                .exact(layout.regular_mip_size[0], layout.regular_mip_size[1])
                .usage(usage)
                .mips(layout.mip_level_count),
            normal_mips: TextureSpec::new(SSGI_TEXTURE_NORMAL_MIPS, layout.normal_mip_format)
                .exact(layout.regular_mip_size[0], layout.regular_mip_size[1])
                .usage(usage)
                .mips(layout.mip_level_count),
            diffuse_mips: TextureSpec::new(SSGI_TEXTURE_DIFFUSE_MIPS, layout.diffuse_mip_format)
                .exact(layout.regular_mip_size[0], layout.regular_mip_size[1])
                .usage(usage)
                .mips(layout.mip_level_count),
        }
    }
}

fn ssgi_compute_texture_specs(resources: SsgiResources) -> SsgiComputeTextureSpecs {
    SsgiComputeTextureSpecs::from_layout(resources.compute_texture_layout())
}

fn create_ssgi_compute_resources(
    graph: &mut RenderGraph,
    resources: SsgiResources,
) -> SsgiComputeGraphResources {
    let specs = ssgi_compute_texture_specs(resources);
    SsgiComputeGraphResources {
        atlas_color: specs.atlas_color.create(graph),
        atlas_depth: specs.atlas_depth.create(graph),
        depth_mips: specs.depth_mips.create(graph),
        normal_mips: specs.normal_mips.create(graph),
        diffuse_mips: specs.diffuse_mips.create(graph),
    }
}

fn declare_ssgi_compute_passes(
    graph: &mut RenderGraph,
    scene_color: TextureHandle,
    scene_depth: TextureHandle,
    scene_normal: TextureHandle,
    resources: SsgiComputeGraphResources,
) {
    for mip_index in 0..SSGI_MIP_COUNT {
        let mip = mip_index as u32;
        graph.add_compute_pass(SSGI_COMPUTE_DEINTERLEAVE_PASSES[mip_index], |setup| {
            setup.read(scene_color);
            setup.read(scene_depth);
            setup.read(scene_normal);
            setup.write_subresource(TextureSubresource::new(
                resources.atlas_depth,
                mip,
                1,
                0,
                SSGI_ATLAS_LAYERS,
            ));
            setup.write_subresource(TextureSubresource::new(
                resources.atlas_color,
                mip,
                1,
                0,
                SSGI_ATLAS_LAYERS,
            ));
            setup.write_subresource(resources.depth_mip(mip));
            setup.write_subresource(resources.normal_mip(mip));
            setup.with_flags(PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::BANDWIDTH_INTENSIVE);
        });
    }

    for mip_index in (0..SSGI_MIP_COUNT).rev() {
        let mip = mip_index as u32;
        graph.add_compute_pass(SSGI_COMPUTE_DIFFUSE_PASSES[mip_index], |setup| {
            setup.read_subresource(TextureSubresource::new(
                resources.atlas_depth,
                mip,
                1,
                0,
                SSGI_ATLAS_LAYERS,
            ));
            setup.read_subresource(TextureSubresource::new(
                resources.atlas_color,
                mip,
                1,
                0,
                SSGI_ATLAS_LAYERS,
            ));
            setup.read_subresource(resources.normal_mip(mip));
            setup.write_subresource(resources.diffuse_mip(mip));
            setup.with_flags(PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::COMPUTE_INTENSIVE);
        });
    }

    for pass_index in 0..SSGI_COMPUTE_UPSAMPLE_PASSES.len() {
        let source_mip = (SSGI_MIP_COUNT - 1 - pass_index) as u32;
        let target_mip = source_mip - 1;
        graph.add_compute_pass(SSGI_COMPUTE_UPSAMPLE_PASSES[pass_index], |setup| {
            setup.read_subresource(resources.depth_mip(source_mip));
            setup.read_subresource(resources.normal_mip(source_mip));
            setup.read_subresource(resources.diffuse_mip(source_mip));
            setup.read_subresource(resources.depth_mip(target_mip));
            setup.read_subresource(resources.normal_mip(target_mip));
            setup.write_subresource(resources.diffuse_mip(target_mip));
            setup.with_flags(PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::BANDWIDTH_INTENSIVE);
        });
    }
}

/// Simplified SSGI post effect for `RenderPipelineAsset::modern_3d()`.
#[derive(Default)]
pub struct SsgiPass {
    resources: SsgiResources,
    final_bgl: Option<wgpu::BindGroupLayout>,
    uniform_bgl: Option<wgpu::BindGroupLayout>,
    compute_scene_bgl: Option<wgpu::BindGroupLayout>,
    compute_deinterleave_output_bgl: Option<wgpu::BindGroupLayout>,
    compute_diffuse_input_bgl: Option<wgpu::BindGroupLayout>,
    compute_diffuse_output_bgl: Option<wgpu::BindGroupLayout>,
    compute_upsample_input_bgl: Option<wgpu::BindGroupLayout>,
    compute_upsample_output_bgl: Option<wgpu::BindGroupLayout>,
    uniform_buffer: Option<wgpu::Buffer>,
    final_pipeline: Option<FullscreenPipeline>,
    compute_deinterleave_pipeline: Option<ComputePipelineCache>,
    compute_diffuse_pipeline: Option<ComputePipelineCache>,
    compute_upsample_pipeline: Option<ComputePipelineCache>,
}

impl SsgiPass {
    #[inline]
    pub const fn resources(&self) -> SsgiResources {
        self.resources
    }

    fn ensure_final_gpu_objects(&mut self, gpu: &GpuContext, target_format: wgpu::TextureFormat) {
        if self.final_bgl.is_none() {
            self.final_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_final_bgl"),
                    entries: &[
                        texture_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(3, wgpu::TextureSampleType::Depth),
                        texture_entry(4, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(5, wgpu::TextureSampleType::Float { filterable: false }),
                    ],
                },
            ));
        }

        if self.uniform_bgl.is_none() {
            self.uniform_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_uniform_bgl"),
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
                },
            ));
        }

        if self.uniform_buffer.is_none() {
            self.uniform_buffer = Some(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("ssgi_uniform"),
                size: std::mem::size_of::<SsgiUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if self.final_pipeline.is_none() {
            let texture_bgl = self
                .final_bgl
                .as_ref()
                .expect("SSGI final bind group layout should exist");
            let uniform_bgl = self
                .uniform_bgl
                .as_ref()
                .expect("SSGI uniform bind group layout should exist");
            self.final_pipeline = Some(FullscreenPipeline::new(
                gpu,
                SSGI_FINAL_SHADER,
                "fs_final_upsample",
                &[texture_bgl, uniform_bgl],
                target_format,
                None,
                "ssgi_final_pipeline",
            ));
        }
    }

    fn ensure_compute_gpu_objects(&mut self, gpu: &GpuContext) {
        if self.uniform_bgl.is_none() {
            self.uniform_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_uniform_bgl"),
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
                },
            ));
        }

        if self.uniform_buffer.is_none() {
            self.uniform_buffer = Some(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("ssgi_uniform"),
                size: std::mem::size_of::<SsgiUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if self.compute_scene_bgl.is_none() {
            self.compute_scene_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_scene_bgl"),
                    entries: &[
                        compute_texture_entry(
                            0,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            1,
                            wgpu::TextureSampleType::Depth,
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            2,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_deinterleave_output_bgl.is_none() {
            self.compute_deinterleave_output_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_deinterleave_output_bgl"),
                    entries: &[
                        compute_storage_texture_entry(
                            0,
                            SSGI_DEPTH_FORMAT,
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_storage_texture_entry(
                            1,
                            SSGI_COLOR_FORMAT,
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_storage_texture_entry(
                            2,
                            SSGI_DEPTH_FORMAT,
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_storage_texture_entry(
                            3,
                            SSGI_NORMAL_FORMAT,
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_diffuse_input_bgl.is_none() {
            self.compute_diffuse_input_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_diffuse_input_bgl"),
                    entries: &[
                        compute_texture_entry(
                            0,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_texture_entry(
                            1,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_texture_entry(
                            2,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_diffuse_output_bgl.is_none() {
            self.compute_diffuse_output_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_diffuse_output_bgl"),
                    entries: &[compute_storage_texture_entry(
                        0,
                        SSGI_COLOR_FORMAT,
                        wgpu::TextureViewDimension::D2,
                    )],
                },
            ));
        }

        if self.compute_upsample_input_bgl.is_none() {
            self.compute_upsample_input_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_upsample_input_bgl"),
                    entries: &[
                        compute_texture_entry(
                            0,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            1,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            2,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            3,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            4,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_upsample_output_bgl.is_none() {
            self.compute_upsample_output_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_upsample_output_bgl"),
                    entries: &[compute_storage_texture_entry(
                        0,
                        SSGI_COLOR_FORMAT,
                        wgpu::TextureViewDimension::D2,
                    )],
                },
            ));
        }

        if self.compute_deinterleave_pipeline.is_none() {
            self.compute_deinterleave_pipeline = Some(ComputePipelineCache::new(
                gpu,
                SSGI_DEINTERLEAVE_COMPUTE_SHADER,
                "cs_main",
                &[
                    self.compute_scene_bgl
                        .as_ref()
                        .expect("SSGI compute scene layout should exist"),
                    self.uniform_bgl
                        .as_ref()
                        .expect("SSGI uniform layout should exist"),
                    self.compute_deinterleave_output_bgl
                        .as_ref()
                        .expect("SSGI deinterleave output layout should exist"),
                ],
                "ssgi_compute_deinterleave",
            ));
        }

        if self.compute_diffuse_pipeline.is_none() {
            self.compute_diffuse_pipeline = Some(ComputePipelineCache::new(
                gpu,
                SSGI_COMPUTE_SHADER,
                "cs_main",
                &[
                    self.compute_diffuse_input_bgl
                        .as_ref()
                        .expect("SSGI diffuse input layout should exist"),
                    self.uniform_bgl
                        .as_ref()
                        .expect("SSGI uniform layout should exist"),
                    self.compute_diffuse_output_bgl
                        .as_ref()
                        .expect("SSGI diffuse output layout should exist"),
                ],
                "ssgi_compute_diffuse",
            ));
        }

        if self.compute_upsample_pipeline.is_none() {
            self.compute_upsample_pipeline = Some(ComputePipelineCache::new(
                gpu,
                SSGI_UPSAMPLE_COMPUTE_SHADER,
                "cs_main",
                &[
                    self.compute_upsample_input_bgl
                        .as_ref()
                        .expect("SSGI upsample input layout should exist"),
                    self.uniform_bgl
                        .as_ref()
                        .expect("SSGI uniform layout should exist"),
                    self.compute_upsample_output_bgl
                        .as_ref()
                        .expect("SSGI upsample output layout should exist"),
                ],
                "ssgi_compute_upsample",
            ));
        }
    }

    fn create_final_bind_group_views(
        &self,
        gpu: &GpuContext,
        low_depth: &wgpu::TextureView,
        low_normal: &wgpu::TextureView,
        low_diffuse: &wgpu::TextureView,
        scene_depth: &wgpu::TextureView,
        scene_normal: &wgpu::TextureView,
        scene_color: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_final_bg"),
            layout: self
                .final_bgl
                .as_ref()
                .expect("SSGI final bind group layout should exist"),
            entries: &[
                texture_view_binding(0, low_depth),
                texture_view_binding(1, low_normal),
                texture_view_binding(2, low_diffuse),
                texture_view_binding(3, scene_depth),
                texture_view_binding(4, scene_normal),
                texture_view_binding(5, scene_color),
            ],
        })
    }

    fn create_uniform_bind_group(&self, gpu: &GpuContext) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_uniform_bg"),
            layout: self
                .uniform_bgl
                .as_ref()
                .expect("SSGI uniform bind group layout should exist"),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self
                    .uniform_buffer
                    .as_ref()
                    .expect("SSGI uniform buffer should exist")
                    .as_entire_binding(),
            }],
        })
    }

    fn create_compute_scene_bind_group(
        &self,
        gpu: &GpuContext,
        current: &crate::render::gpu::RenderTarget,
        depth: &crate::render::gpu::RenderTarget,
        normal: &crate::render::gpu::RenderTarget,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_compute_scene_bg"),
            layout: self
                .compute_scene_bgl
                .as_ref()
                .expect("SSGI compute scene layout should exist"),
            entries: &[
                texture_binding(0, current),
                texture_binding(1, depth),
                texture_binding(2, normal),
            ],
        })
    }

    fn create_compute_deinterleave_output_bind_group(
        &self,
        gpu: &GpuContext,
        atlas_depth: &wgpu::TextureView,
        atlas_color: &wgpu::TextureView,
        regular_depth: &wgpu::TextureView,
        regular_normal: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_compute_deinterleave_output_bg"),
            layout: self
                .compute_deinterleave_output_bgl
                .as_ref()
                .expect("SSGI deinterleave output layout should exist"),
            entries: &[
                texture_view_binding(0, atlas_depth),
                texture_view_binding(1, atlas_color),
                texture_view_binding(2, regular_depth),
                texture_view_binding(3, regular_normal),
            ],
        })
    }

    fn create_compute_diffuse_input_bind_group(
        &self,
        gpu: &GpuContext,
        atlas_depth: &wgpu::TextureView,
        atlas_color: &wgpu::TextureView,
        normal: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_compute_diffuse_input_bg"),
            layout: self
                .compute_diffuse_input_bgl
                .as_ref()
                .expect("SSGI diffuse input layout should exist"),
            entries: &[
                texture_view_binding(0, atlas_depth),
                texture_view_binding(1, atlas_color),
                texture_view_binding(2, normal),
            ],
        })
    }

    fn create_compute_diffuse_output_bind_group(
        &self,
        gpu: &GpuContext,
        output: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_compute_diffuse_output_bg"),
            layout: self
                .compute_diffuse_output_bgl
                .as_ref()
                .expect("SSGI diffuse output layout should exist"),
            entries: &[texture_view_binding(0, output)],
        })
    }

    fn create_compute_upsample_input_bind_group(
        &self,
        gpu: &GpuContext,
        depth_low: &wgpu::TextureView,
        normal_low: &wgpu::TextureView,
        diffuse_low: &wgpu::TextureView,
        depth_high: &wgpu::TextureView,
        normal_high: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_compute_upsample_input_bg"),
            layout: self
                .compute_upsample_input_bgl
                .as_ref()
                .expect("SSGI upsample input layout should exist"),
            entries: &[
                texture_view_binding(0, depth_low),
                texture_view_binding(1, normal_low),
                texture_view_binding(2, diffuse_low),
                texture_view_binding(3, depth_high),
                texture_view_binding(4, normal_high),
            ],
        })
    }

    fn create_compute_upsample_output_bind_group(
        &self,
        gpu: &GpuContext,
        output: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssgi_compute_upsample_output_bg"),
            layout: self
                .compute_upsample_output_bgl
                .as_ref()
                .expect("SSGI upsample output layout should exist"),
            entries: &[texture_view_binding(0, output)],
        })
    }

    fn execute_compute_pass(
        &mut self,
        gpu: &mut GpuContext,
        pass: &CompiledPass,
        resources: &crate::render::graph::PhysicalResources<'_>,
        execution: &crate::render::execution::ViewExecutionContext<'_>,
        settings: SsgiSettings,
        pass_kind: SsgiComputePassKind,
    ) -> Result<(), RenderGraphError> {
        self.ensure_compute_gpu_objects(gpu);
        let scene_view = execution.view_payload::<SceneView>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("ssgi compute missing SceneView payload".into())
        })?;
        let inverse_projection = Mat4::from_cols_array(scene_view.unjittered_projection_matrix)
            .inverse()
            .to_cols_array();
        let pass_settings = pass_kind.settings();
        let range = pass_settings
            .range
            .max((settings.radius_pixels * 0.25).clamp(1.0, 3.0));
        let spread = pass_settings.spread;
        let range_spread = (range * spread).max(1.0);
        let uniform = SsgiUniform {
            params0: [
                settings.intensity.max(0.0),
                range,
                spread,
                settings.depth_rejection.max(0.001).recip(),
            ],
            params1: [
                range_spread.recip() * range_spread.recip(),
                settings.normal_power.max(0.001),
                0.96,
                1.0,
            ],
            params2: [
                pass_kind.output_scale() as f32,
                pass_kind.source_scale() as f32,
                0.0,
                0.0,
            ],
            inverse_projection,
        };
        gpu.queue().write_buffer(
            self.uniform_buffer
                .as_ref()
                .expect("SSGI uniform buffer should exist"),
            0,
            bytemuck::bytes_of(&uniform),
        );
        let uniform_bg = self.create_uniform_bind_group(gpu);

        let (input_bg, output_bg, pipeline, dispatch_size) = match pass_kind {
            SsgiComputePassKind::Deinterleave { .. } => {
                let current_handle = pass_nth_read_texture(pass, 0, self.name(), "current color");
                let depth_handle = pass_nth_read_texture(pass, 1, self.name(), "scene depth");
                let normal_handle = pass_nth_read_texture(pass, 2, self.name(), "scene normal");
                let current =
                    require_render_target(resources, current_handle, self.name(), "current");
                let depth = require_render_target(resources, depth_handle, self.name(), "depth");
                let normal = require_render_target(resources, normal_handle, self.name(), "normal");
                let atlas_depth = pass_nth_write_subresource(pass, 0, self.name(), "atlas depth");
                let atlas_color = pass_nth_write_subresource(pass, 1, self.name(), "atlas color");
                let regular_depth =
                    pass_nth_write_subresource(pass, 2, self.name(), "regular depth");
                let regular_normal =
                    pass_nth_write_subresource(pass, 3, self.name(), "regular normal");
                let atlas_depth_view = resources
                    .storage_texture_view(atlas_depth, wgpu::TextureViewDimension::D2Array);
                let atlas_color_view = resources
                    .storage_texture_view(atlas_color, wgpu::TextureViewDimension::D2Array);
                let regular_depth_view =
                    resources.storage_texture_view(regular_depth, wgpu::TextureViewDimension::D2);
                let regular_normal_view =
                    resources.storage_texture_view(regular_normal, wgpu::TextureViewDimension::D2);
                (
                    self.create_compute_scene_bind_group(gpu, current, depth, normal),
                    self.create_compute_deinterleave_output_bind_group(
                        gpu,
                        &atlas_depth_view,
                        &atlas_color_view,
                        &regular_depth_view,
                        &regular_normal_view,
                    ),
                    self.compute_deinterleave_pipeline
                        .as_mut()
                        .expect("SSGI deinterleave compute pipeline should exist")
                        .pipeline(gpu),
                    subresource_extent(resources, regular_depth),
                )
            }
            SsgiComputePassKind::Diffuse { .. } => {
                let atlas_depth = pass_nth_read_subresource(pass, 0, self.name(), "atlas depth");
                let atlas_color = pass_nth_read_subresource(pass, 1, self.name(), "atlas color");
                let normal = pass_nth_read_subresource(pass, 2, self.name(), "normal");
                let output = pass_nth_write_subresource(pass, 0, self.name(), "diffuse output");
                let atlas_depth_view = resources
                    .texture_subresource_view(atlas_depth, wgpu::TextureViewDimension::D2Array);
                let atlas_color_view = resources
                    .texture_subresource_view(atlas_color, wgpu::TextureViewDimension::D2Array);
                let normal_view =
                    resources.texture_subresource_view(normal, wgpu::TextureViewDimension::D2);
                let output_view =
                    resources.storage_texture_view(output, wgpu::TextureViewDimension::D2);
                (
                    self.create_compute_diffuse_input_bind_group(
                        gpu,
                        &atlas_depth_view,
                        &atlas_color_view,
                        &normal_view,
                    ),
                    self.create_compute_diffuse_output_bind_group(gpu, &output_view),
                    self.compute_diffuse_pipeline
                        .as_mut()
                        .expect("SSGI diffuse compute pipeline should exist")
                        .pipeline(gpu),
                    subresource_extent(resources, output),
                )
            }
            SsgiComputePassKind::Upsample { .. } => {
                let depth_low = pass_nth_read_subresource(pass, 0, self.name(), "low depth");
                let normal_low = pass_nth_read_subresource(pass, 1, self.name(), "low normal");
                let diffuse_low = pass_nth_read_subresource(pass, 2, self.name(), "low diffuse");
                let depth_high = pass_nth_read_subresource(pass, 3, self.name(), "high depth");
                let normal_high = pass_nth_read_subresource(pass, 4, self.name(), "high normal");
                let output = pass_nth_write_subresource(pass, 0, self.name(), "upsample output");
                let depth_low_view =
                    resources.texture_subresource_view(depth_low, wgpu::TextureViewDimension::D2);
                let normal_low_view =
                    resources.texture_subresource_view(normal_low, wgpu::TextureViewDimension::D2);
                let diffuse_low_view =
                    resources.texture_subresource_view(diffuse_low, wgpu::TextureViewDimension::D2);
                let depth_high_view =
                    resources.texture_subresource_view(depth_high, wgpu::TextureViewDimension::D2);
                let normal_high_view =
                    resources.texture_subresource_view(normal_high, wgpu::TextureViewDimension::D2);
                let output_view =
                    resources.storage_texture_view(output, wgpu::TextureViewDimension::D2);
                (
                    self.create_compute_upsample_input_bind_group(
                        gpu,
                        &depth_low_view,
                        &normal_low_view,
                        &diffuse_low_view,
                        &depth_high_view,
                        &normal_high_view,
                    ),
                    self.create_compute_upsample_output_bind_group(gpu, &output_view),
                    self.compute_upsample_pipeline
                        .as_mut()
                        .expect("SSGI upsample compute pipeline should exist")
                        .pipeline(gpu),
                    subresource_extent(resources, output),
                )
            }
        };

        if dispatch_size[0] == 0 || dispatch_size[1] == 0 {
            return Ok(());
        }
        if should_log_scene_view(scene_view) {
            eprintln!(
                "[ssgi][compute][frame={}] pass={} kind={:?} dispatch={:?} settings={:?} uniform.params0={:?} params1={:?} params2={:?}",
                scene_view.temporal.frame_index,
                pass.name,
                pass_kind,
                dispatch_size,
                settings,
                uniform.params0,
                uniform.params1,
                uniform.params2
            );
        }
        let mut frame = gpu.frame();
        let mut compute = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(pass.name.as_ref()),
            ..Default::default()
        });
        compute.set_pipeline(&pipeline);
        compute.set_bind_group(0, &input_bg, &[]);
        compute.set_bind_group(1, &uniform_bg, &[]);
        compute.set_bind_group(2, &output_bg, &[]);
        compute.dispatch_workgroups(
            dispatch_size[0].div_ceil(8),
            dispatch_size[1].div_ceil(8),
            1,
        );
        Ok(())
    }
}

impl SsgiPass {
    fn name(&self) -> &'static str {
        "ssgi"
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup_with_settings(
        &mut self,
        ctx: &mut PostFxPassSetupContext<'_, '_>,
        settings: SsgiSettings,
    ) {
        let Some(current) = ctx.state().current_color() else {
            return;
        };
        let Some(depth) = ctx.state().scene_depth() else {
            return;
        };
        let Some(normal) = ctx.state().scene_normal() else {
            return;
        };

        let target_size = ctx.view().target_size();
        self.resources.resize(target_size[0], target_size[1]);
        if let Some(scene_view) = ctx.view_payload::<SceneView>() {
            if should_log_scene_view(scene_view) {
                eprintln!(
                    "[ssgi][setup][frame={}] target={:?} current={:?} depth={:?} normal={:?} resources={:?} settings={:?} jitter={:?} prev_jitter={:?}",
                    scene_view.temporal.frame_index,
                    target_size,
                    current.format(),
                    depth.format(),
                    normal.format(),
                    self.resources,
                    settings,
                    scene_view.temporal.jitter,
                    scene_view.temporal.previous_jitter
                );
            }
        }
        let compute_resources = create_ssgi_compute_resources(ctx.graph(), self.resources);
        declare_ssgi_compute_passes(
            ctx.graph(),
            current.handle(),
            depth.handle(),
            normal.handle(),
            compute_resources,
        );
        ctx.blackboard_set(SSGI_COMPUTE_RESOURCES_BLACKBOARD, compute_resources);

        let output = ctx.graph().create_texture(|builder| {
            builder
                .name("ssgi_scene_color")
                .size(TargetSize::Exact(target_size[0], target_size[1]))
                .format(current.format());
        });
        ctx.state().set_current_color(output, current.format());
        ctx.state().set_scene_color(output, current.format());

        ctx.graph().add_render_pass(SSGI_FINAL_PASS, |setup| {
            setup.read_subresource(compute_resources.depth_mip(0));
            setup.read_subresource(compute_resources.normal_mip(0));
            setup.read_subresource(compute_resources.diffuse_mip(0));
            setup.read(depth.handle());
            setup.read(normal.handle());
            setup.read(current.handle());
            setup.write_color_cleared(0, output, [0.0, 0.0, 0.0, 1.0]);
        });
    }

    fn execute_with_settings(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
        settings: SsgiSettings,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution) = ctx.split();
        if let Some(pass_kind) = ssgi_compute_pass_kind(pass.name.as_ref()) {
            return self.execute_compute_pass(gpu, pass, resources, execution, settings, pass_kind);
        }
        if pass.name.as_ref() != SSGI_FINAL_PASS {
            return Err(RenderGraphError::ExecutionFailed(format!(
                "unknown SSGI pass `{}`",
                pass.name
            )));
        }
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let output = require_render_target(resources, output_handle, self.name(), "output");

        self.ensure_final_gpu_objects(gpu, output.format());
        let scene_view = execution.view_payload::<SceneView>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("ssgi missing SceneView payload".into())
        })?;
        let inverse_projection = Mat4::from_cols_array(scene_view.unjittered_projection_matrix)
            .inverse()
            .to_cols_array();
        let pass_settings = SSGI_UPSAMPLE_PARAMS[SSGI_MIP_COUNT - 1];
        let range = pass_settings.range;
        let spread = pass_settings.spread;
        let range_spread = (range * spread).max(1.0);
        let uniform = SsgiUniform {
            params0: [
                settings.intensity.max(0.0),
                range,
                spread,
                settings.depth_rejection.max(0.001).recip(),
            ],
            params1: [
                range_spread.recip() * range_spread.recip(),
                settings.normal_power.max(0.001),
                0.96,
                1.0,
            ],
            params2: [1.0, 2.0, 1.0, 0.0],
            inverse_projection,
        };
        gpu.queue().write_buffer(
            self.uniform_buffer
                .as_ref()
                .expect("SSGI uniform buffer should exist"),
            0,
            bytemuck::bytes_of(&uniform),
        );

        let uniform_bg = self.create_uniform_bind_group(gpu);

        let low_depth = pass_nth_read_subresource(pass, 0, self.name(), "low depth");
        let low_normal = pass_nth_read_subresource(pass, 1, self.name(), "low normal");
        let low_diffuse = pass_nth_read_subresource(pass, 2, self.name(), "low diffuse");
        let scene_depth_handle = pass_nth_read_texture(pass, 0, self.name(), "scene depth");
        let scene_normal_handle = pass_nth_read_texture(pass, 1, self.name(), "scene normal");
        let scene_color_handle = pass_nth_read_texture(pass, 2, self.name(), "scene color");
        let low_depth_view =
            resources.texture_subresource_view(low_depth, wgpu::TextureViewDimension::D2);
        let low_normal_view =
            resources.texture_subresource_view(low_normal, wgpu::TextureViewDimension::D2);
        let low_diffuse_view =
            resources.texture_subresource_view(low_diffuse, wgpu::TextureViewDimension::D2);
        let scene_depth =
            require_render_target(resources, scene_depth_handle, self.name(), "scene depth");
        let scene_normal =
            require_render_target(resources, scene_normal_handle, self.name(), "scene normal");
        let scene_color =
            require_render_target(resources, scene_color_handle, self.name(), "scene color");
        let texture_bg = self.create_final_bind_group_views(
            gpu,
            &low_depth_view,
            &low_normal_view,
            &low_diffuse_view,
            scene_depth.view(),
            scene_normal.view(),
            scene_color.view(),
        );
        if should_log_scene_view(scene_view) {
            eprintln!(
                "[ssgi][final][frame={}] output={}x{} format={:?} scene_color={:?} scene_depth={:?} scene_normal={:?} settings={:?} uniform.params0={:?} params1={:?} params2={:?}",
                scene_view.temporal.frame_index,
                output.width(),
                output.height(),
                output.format(),
                scene_color.format(),
                scene_depth.format(),
                scene_normal.format(),
                settings,
                uniform.params0,
                uniform.params1,
                uniform.params2
            );
        }
        let pipeline = self
            .final_pipeline
            .as_mut()
            .expect("SSGI final pipeline should exist")
            .pipeline(gpu, output.format());
        let color_attachments = vec![Some(color_attachment(output))];
        let mut frame = gpu.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &texture_bg, &[]);
        pass.set_bind_group(1, &uniform_bg, &[]);
        FullscreenPass::draw(&mut pass);
        Ok(())
    }
}

pub struct SsgiProviderFactory;

impl GiProviderFactory for SsgiProviderFactory {
    fn id(&self) -> GiProviderId {
        SSGI_PROVIDER_ID
    }

    fn create(&self, _gpu: &GpuContext) -> Box<dyn GiProviderRuntime> {
        Box::new(SsgiRuntime {
            pass: SsgiPass::default(),
            settings: SsgiSettings::default(),
            layout: None,
            bind_group: None,
        })
    }
}

struct SsgiRuntime {
    pass: SsgiPass,
    settings: SsgiSettings,
    layout: Option<wgpu::BindGroupLayout>,
    bind_group: Option<wgpu::BindGroup>,
}

impl SsgiRuntime {
    fn ensure_sampling_binding(&mut self, gpu: &GpuContext) {
        if self.layout.is_none() {
            self.layout = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_sampling_bgl"),
                    entries: &[],
                },
            ));
        }
        if self.bind_group.is_none() {
            let layout = self
                .layout
                .as_ref()
                .expect("SSGI sampling layout should exist");
            self.bind_group = Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ssgi_sampling_bg"),
                layout,
                entries: &[],
            }));
        }
    }
}

impl GiProviderRuntime for SsgiRuntime {
    fn prepare(&mut self, gpu: &GpuContext, _scene: &GiSceneInput<'_>, settings: &dyn GiSettings) {
        if let Some(settings) = downcast_settings::<SsgiSettings>(settings, SSGI_PROVIDER_ID) {
            self.settings = *settings;
        }
        self.ensure_sampling_binding(gpu);
    }

    fn setup_composite(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        self.pass.setup_with_settings(ctx, self.settings);
    }

    fn execute_composite(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.pass.execute_with_settings(ctx, self.settings)
    }

    fn sampling_binding(&self) -> GiSamplingBinding {
        GiSamplingBinding {
            layout: self
                .layout
                .as_ref()
                .expect("SSGI sampling layout should be initialized during prepare")
                .clone(),
            bind_group: self
                .bind_group
                .as_ref()
                .expect("SSGI sampling bind group should be initialized during prepare")
                .clone(),
        }
    }

    fn shader_descriptor(&self) -> GiShaderDescriptor {
        GiShaderDescriptor {
            key: "ssgi",
            source: crate::render::gi::NULL_GI_SHADER,
        }
    }

    fn composite_descriptor(&self) -> Option<GiCompositeDescriptor> {
        Some(GiCompositeDescriptor {
            label: "gi_composite",
            requires_hdr_input: self.pass.requires_hdr_input(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum SsgiComputePassKind {
    Deinterleave {
        mip_index: usize,
    },
    Diffuse {
        mip_index: usize,
    },
    Upsample {
        source_mip_index: usize,
        target_mip_index: usize,
        pass_index: usize,
    },
}

impl SsgiComputePassKind {
    fn settings(self) -> SsgiSampleParams {
        match self {
            Self::Deinterleave { .. } => SsgiSampleParams {
                range: 1.0,
                spread: 1.0,
            },
            Self::Diffuse { mip_index } => SSGI_DIFFUSE_PARAMS[mip_index],
            Self::Upsample { pass_index, .. } => SSGI_UPSAMPLE_PARAMS[pass_index],
        }
    }

    fn output_scale(self) -> u32 {
        match self {
            Self::Deinterleave { mip_index } | Self::Diffuse { mip_index } => {
                1u32 << (mip_index + 1)
            }
            Self::Upsample {
                target_mip_index, ..
            } => 1u32 << (target_mip_index + 1),
        }
    }

    fn source_scale(self) -> u32 {
        match self {
            Self::Deinterleave { mip_index } | Self::Diffuse { mip_index } => {
                1u32 << (mip_index + 1)
            }
            Self::Upsample {
                source_mip_index, ..
            } => 1u32 << (source_mip_index + 1),
        }
    }
}

fn ssgi_compute_pass_kind(name: &str) -> Option<SsgiComputePassKind> {
    if let Some(mip_index) = SSGI_COMPUTE_DEINTERLEAVE_PASSES
        .iter()
        .position(|pass_name| *pass_name == name)
    {
        return Some(SsgiComputePassKind::Deinterleave { mip_index });
    }

    if let Some(mip_index) = SSGI_COMPUTE_DIFFUSE_PASSES
        .iter()
        .position(|pass_name| *pass_name == name)
    {
        return Some(SsgiComputePassKind::Diffuse { mip_index });
    }

    let pass_index = SSGI_COMPUTE_UPSAMPLE_PASSES
        .iter()
        .position(|pass_name| *pass_name == name)?;
    let source_mip_index = SSGI_MIP_COUNT - 1 - pass_index;
    let target_mip_index = source_mip_index.checked_sub(1)?;
    Some(SsgiComputePassKind::Upsample {
        source_mip_index,
        target_mip_index,
        pass_index,
    })
}

#[inline]
fn align_to(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value.max(1);
    }
    value.max(1).div_ceil(alignment) * alignment
}

fn color_attachment(
    target: &crate::render::gpu::RenderTarget,
) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view: target.view(),
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn texture_binding<'a>(
    binding: u32,
    target: &'a crate::render::gpu::RenderTarget,
) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(target.view()),
    }
}

fn texture_view_binding<'a>(binding: u32, view: &'a wgpu::TextureView) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn compute_texture_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn compute_storage_texture_entry(
    binding: u32,
    format: wgpu::TextureFormat,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
            view_dimension,
        },
        count: None,
    }
}

fn pass_nth_read_subresource(
    pass: &CompiledPass,
    index: usize,
    node_name: &str,
    label: &str,
) -> TextureSubresource {
    pass.reads
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::TextureSubresource(subresource) => Some(*subresource),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("{node_name} should read {label} texture subresource"))
}

fn pass_nth_write_subresource(
    pass: &CompiledPass,
    index: usize,
    node_name: &str,
    label: &str,
) -> TextureSubresource {
    pass.writes
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::TextureSubresource(subresource) => Some(*subresource),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("{node_name} should write {label} texture subresource"))
}

fn subresource_extent(
    resources: &crate::render::graph::PhysicalResources<'_>,
    subresource: TextureSubresource,
) -> [u32; 2] {
    let texture = resources.texture_ref(subresource.texture);
    let divisor = 1u32
        .checked_shl(subresource.base_mip_level)
        .unwrap_or(u32::MAX);
    [
        texture.size[0].div_ceil(divisor).max(1),
        texture.size[1].div_ceil(divisor).max(1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("ssgi_shader_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    fn assert_wgsl_module_is_valid(device: &wgpu::Device, label: &'static str, source: &str) {
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        device.poll(wgpu::Maintain::Wait);
        let error = pollster::block_on(device.pop_error_scope());
        assert!(error.is_none(), "{label} should validate: {error:?}");
    }

    fn decode_scene_normal_like_ssgi(encoded: [f32; 3]) -> [f32; 3] {
        let normal = [
            encoded[0] * 2.0 - 1.0,
            encoded[1] * 2.0 - 1.0,
            encoded[2] * 2.0 - 1.0,
        ];
        let len_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
        if len_sq <= 0.000001 {
            return [0.0, 0.0, -1.0];
        }
        let inv_len = len_sq.sqrt().recip();
        [
            normal[0] * inv_len,
            normal[1] * inv_len,
            -normal[2] * inv_len,
        ]
    }

    fn reconstruct_positive_view_z_like_ssgi(projection: Mat4, depth: f32) -> f32 {
        let view = projection.inverse() * crate::math::Vec4::new(0.0, 0.0, depth, 1.0);
        let view = view.to_array();
        -(view[2] / view[3])
    }

    #[test]
    fn ssgi_resources_clamp_zero_size_to_one() {
        let mut resources = SsgiResources::default();
        resources.resize(0, 0);
        assert_eq!(resources.target_size(), [1, 1]);
    }

    #[test]
    fn ssgi_resources_track_requested_size() {
        let mut resources = SsgiResources::default();
        resources.resize(640, 360);
        assert_eq!(resources.target_size(), [640, 360]);
    }

    #[test]
    fn ssgi_resources_match_wicked_aligned_atlas_dimensions() {
        let mut resources = SsgiResources::default();
        resources.resize(641, 359);

        assert_eq!(resources.target_size(), [641, 359]);
        assert_eq!(resources.aligned_size(), [704, 384]);
        assert_eq!(resources.atlas_size(), [88, 48]);
        assert_eq!(resources.atlas_layers(), 16);
        assert_eq!(
            resources.mip_level(0),
            Some(SsgiMipLevel {
                scale: 2,
                atlas_size: [88, 48],
                regular_size: [352, 192],
            })
        );
        assert_eq!(
            resources.mip_level(3),
            Some(SsgiMipLevel {
                scale: 16,
                atlas_size: [11, 6],
                regular_size: [44, 24],
            })
        );
    }

    #[test]
    fn ssgi_compute_layout_matches_wicked_texture2d_array_contract() {
        let mut resources = SsgiResources::default();
        resources.resize(641, 359);

        let layout = resources.compute_texture_layout();

        assert_eq!(layout.atlas_size, [88, 48]);
        assert_eq!(layout.regular_mip_size, [352, 192]);
        assert_eq!(layout.mip_level_count, 4);
        assert_eq!(layout.atlas_layer_count, 16);
        assert_eq!(layout.atlas_color_format, wgpu::TextureFormat::Rgba16Float);
        assert_eq!(layout.atlas_depth_format, wgpu::TextureFormat::R32Float);
        assert_eq!(layout.depth_mip_format, wgpu::TextureFormat::R32Float);
        assert_eq!(layout.normal_mip_format, wgpu::TextureFormat::Rgba16Float);
        assert_eq!(layout.diffuse_mip_format, wgpu::TextureFormat::Rgba16Float);
        assert!(layout.usage.contains(wgpu::TextureUsages::STORAGE_BINDING));
        assert!(layout.usage.contains(wgpu::TextureUsages::TEXTURE_BINDING));
        assert!(!layout
            .usage
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
    }

    #[test]
    fn ssgi_compute_texture_specs_allocate_array_atlas_and_mip_chains() {
        let mut resources = SsgiResources::default();
        resources.resize(1280, 720);

        let specs = ssgi_compute_texture_specs(resources);

        assert_eq!(specs.atlas_color.name(), SSGI_TEXTURE_ATLAS_COLOR);
        assert_eq!(specs.atlas_color.size(), TargetSize::Exact(160, 96));
        assert_eq!(specs.atlas_color.mip_level_count(), 4);
        assert_eq!(specs.atlas_color.array_layer_count(), 16);
        assert_eq!(specs.atlas_color.format(), wgpu::TextureFormat::Rgba16Float);
        assert!(specs
            .atlas_color
            .usage_flags()
            .contains(wgpu::TextureUsages::STORAGE_BINDING));

        assert_eq!(specs.atlas_depth.name(), SSGI_TEXTURE_ATLAS_DEPTH);
        assert_eq!(specs.atlas_depth.size(), TargetSize::Exact(160, 96));
        assert_eq!(specs.atlas_depth.mip_level_count(), 4);
        assert_eq!(specs.atlas_depth.array_layer_count(), 16);
        assert_eq!(specs.atlas_depth.format(), wgpu::TextureFormat::R32Float);

        assert_eq!(specs.depth_mips.name(), SSGI_TEXTURE_DEPTH_MIPS);
        assert_eq!(specs.depth_mips.size(), TargetSize::Exact(640, 384));
        assert_eq!(specs.depth_mips.mip_level_count(), 4);
        assert_eq!(specs.depth_mips.array_layer_count(), 1);
        assert_eq!(specs.depth_mips.format(), wgpu::TextureFormat::R32Float);

        assert_eq!(specs.normal_mips.name(), SSGI_TEXTURE_NORMAL_MIPS);
        assert_eq!(specs.normal_mips.size(), TargetSize::Exact(640, 384));
        assert_eq!(specs.normal_mips.format(), wgpu::TextureFormat::Rgba16Float);

        assert_eq!(specs.diffuse_mips.name(), SSGI_TEXTURE_DIFFUSE_MIPS);
        assert_eq!(specs.diffuse_mips.size(), TargetSize::Exact(640, 384));
        assert_eq!(
            specs.diffuse_mips.format(),
            wgpu::TextureFormat::Rgba16Float
        );
    }

    #[test]
    fn ssgi_compute_resources_register_named_graph_textures() {
        let mut resources = SsgiResources::default();
        resources.resize(320, 180);
        let mut graph = RenderGraph::new();

        let handles = create_ssgi_compute_resources(&mut graph, resources);

        assert_eq!(
            graph.get_texture(SSGI_TEXTURE_ATLAS_COLOR),
            Some(handles.atlas_color)
        );
        assert_eq!(
            graph.get_texture(SSGI_TEXTURE_ATLAS_DEPTH),
            Some(handles.atlas_depth)
        );
        assert_eq!(
            graph.get_texture(SSGI_TEXTURE_DEPTH_MIPS),
            Some(handles.depth_mips)
        );
        assert_eq!(
            graph.get_texture(SSGI_TEXTURE_NORMAL_MIPS),
            Some(handles.normal_mips)
        );
        assert_eq!(
            graph.get_texture(SSGI_TEXTURE_DIFFUSE_MIPS),
            Some(handles.diffuse_mips)
        );
        assert_eq!(
            handles.atlas_color_layer(2, 7),
            TextureSubresource::new(handles.atlas_color, 2, 1, 7, 1)
        );
        assert_eq!(
            handles.diffuse_mip(3),
            TextureSubresource::new(handles.diffuse_mips, 3, 1, 0, 1)
        );
    }

    #[test]
    fn ssgi_compute_pass_declarations_use_subresource_dependencies() {
        let mut resources = SsgiResources::default();
        resources.resize(320, 180);
        let mut graph = RenderGraph::new();
        let scene_color = graph.create_texture(|builder| {
            builder
                .name("ssgi_test_scene_color")
                .format(wgpu::TextureFormat::Rgba16Float)
                .persistent();
        });
        let scene_depth = graph.create_texture(|builder| {
            builder
                .name("ssgi_test_scene_depth")
                .format(wgpu::TextureFormat::Depth32Float)
                .persistent();
        });
        let scene_normal = graph.create_texture(|builder| {
            builder
                .name("ssgi_test_scene_normal")
                .format(wgpu::TextureFormat::Rgba16Float)
                .persistent();
        });
        let handles = create_ssgi_compute_resources(&mut graph, resources);

        declare_ssgi_compute_passes(&mut graph, scene_color, scene_depth, scene_normal, handles);

        assert_eq!(
            graph.pass_count(),
            SSGI_COMPUTE_DEINTERLEAVE_PASSES.len()
                + SSGI_COMPUTE_DIFFUSE_PASSES.len()
                + SSGI_COMPUTE_UPSAMPLE_PASSES.len()
        );
        let compiled = graph
            .compile()
            .expect("SSGI compute subresource graph should compile");
        assert!(
            compiled.is_empty(),
            "compute SSGI declarations stay culled until wired to the final output"
        );
    }

    #[test]
    fn ssgi_compute_wgsl_sources_validate() {
        let (device, _queue) = create_test_device();

        assert_wgsl_module_is_valid(
            &device,
            "ssgi_deinterleave_compute_test",
            SSGI_DEINTERLEAVE_COMPUTE_SHADER,
        );
        assert_wgsl_module_is_valid(&device, "ssgi_compute_test", SSGI_COMPUTE_SHADER);
        assert_wgsl_module_is_valid(
            &device,
            "ssgi_upsample_compute_test",
            SSGI_UPSAMPLE_COMPUTE_SHADER,
        );
    }

    #[test]
    fn ssgi_decode_flips_normal_z_to_match_positive_view_z() {
        let facing_camera = decode_scene_normal_like_ssgi([0.5, 0.5, 1.0]);
        assert_eq!(facing_camera, [0.0, 0.0, -1.0]);

        let facing_away = decode_scene_normal_like_ssgi([0.5, 0.5, 0.0]);
        assert_eq!(facing_away, [0.0, 0.0, 1.0]);

        for source in [
            SSGI_COMPUTE_SHADER,
            SSGI_UPSAMPLE_COMPUTE_SHADER,
            SSGI_FINAL_SHADER,
        ] {
            assert!(
                source.contains("unit.x, unit.y, -unit.z"),
                "SSGI normal decode must mirror z because reconstructed positions use Wicked-style positive view z"
            );
        }
    }

    #[test]
    fn ssgi_depth_reconstruct_matches_wicked_positive_view_z() {
        let projection = Mat4::perspective_rh(60.0_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
        let view_position = crate::math::Vec4::new(0.0, 0.0, -8.0, 1.0);
        let clip = projection * view_position;
        let depth = clip.z() / clip.w();

        let reconstructed_z = reconstruct_positive_view_z_like_ssgi(projection, depth);

        assert!((reconstructed_z - 8.0).abs() < 0.001);
    }

    #[test]
    fn ssgi_compute_pass_kind_recognizes_wicked_compute_chain() {
        assert!(matches!(
            ssgi_compute_pass_kind("ssgi_compute_deinterleave_16x"),
            Some(SsgiComputePassKind::Deinterleave { mip_index: 3 })
        ));
        assert!(matches!(
            ssgi_compute_pass_kind("ssgi_compute_diffuse_16x"),
            Some(SsgiComputePassKind::Diffuse { mip_index: 3 })
        ));
        assert!(matches!(
            ssgi_compute_pass_kind("ssgi_compute_upsample_16x_to_8x"),
            Some(SsgiComputePassKind::Upsample {
                source_mip_index: 3,
                target_mip_index: 2,
                pass_index: 0,
            })
        ));
    }
}
