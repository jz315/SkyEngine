//! Bloom post-processing.

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};
use crate::render::graph::{
    CompiledPass, PassHandle, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
    TextureHandle,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUniform {
    params: [f32; 4],    // intensity, spread, _, _
    texel_dir: [f32; 4], // texel_x, texel_y, dir_x, dir_y
}

const BLOOM_SHADER: &str = include_str!("../shaders/postfx/bloom.wgsl");
const BLOOM_LEVELS: usize = 6;
pub(crate) const BLOOM_GRAPH_PASS_COUNT: usize = 4 * BLOOM_LEVELS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BloomGraphPassKind {
    Downsample { level: usize },
    BlurHorizontal { level: usize },
    BlurVertical { level: usize },
    Upsample { level: usize },
    Combine,
}

#[derive(Debug, Clone, Copy)]
struct BloomGraphPass {
    handle: PassHandle,
    kind: BloomGraphPassKind,
}

#[derive(Debug, Clone)]
pub struct BloomGraph {
    input: TextureHandle,
    output: TextureHandle,
    mips: [TextureHandle; BLOOM_LEVELS],
    scratches: [TextureHandle; BLOOM_LEVELS],
    passes: Vec<BloomGraphPass>,
}

impl BloomGraph {
    #[inline]
    pub fn output(&self) -> TextureHandle {
        self.output
    }

    #[inline]
    fn pass_kind(&self, handle: PassHandle) -> Option<BloomGraphPassKind> {
        self.passes
            .iter()
            .find(|pass| pass.handle == handle)
            .map(|pass| pass.kind)
    }
}

pub struct Bloom {
    downsample_pipeline: FullscreenPipeline,
    blur_pipeline: FullscreenPipeline,
    upsample_pipeline: FullscreenPipeline,
    combine_pipeline: FullscreenPipeline,
    sample_bgl: wgpu::BindGroupLayout,
    dual_bgl: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    pub intensity: f32,
    pub spread: f32,
}

fn texture_sampler_bgl(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
    })
}

fn dual_texture_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bloom_dual_bgl"),
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
    })
}

impl Bloom {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let sample_bgl = texture_sampler_bgl(ctx.device(), "bloom_sample_bgl");
        let dual_bgl = dual_texture_bgl(ctx.device());

        let params_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bloom_params_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let params_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom_params_buf"),
            size: std::mem::size_of::<BloomUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_params_bg"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let additive_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let downsample_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_downsample",
            &[&sample_bgl, &params_bgl],
            target_format,
            None,
            "bloom_downsample",
        );
        let blur_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_blur",
            &[&sample_bgl, &params_bgl],
            target_format,
            None,
            "bloom_blur",
        );
        let upsample_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_upsample",
            &[&sample_bgl, &params_bgl],
            target_format,
            Some(additive_blend),
            "bloom_upsample",
        );
        let combine_pipeline = FullscreenPipeline::new(
            ctx,
            BLOOM_SHADER,
            "fs_combine",
            &[&dual_bgl, &params_bgl],
            target_format,
            None,
            "bloom_combine",
        );

        Self {
            downsample_pipeline,
            blur_pipeline,
            upsample_pipeline,
            combine_pipeline,
            sample_bgl,
            dual_bgl,
            params_buffer,
            params_bind_group,
            intensity: 1.0,
            spread: 1.0,
        }
    }

    fn update_uniform(&self, ctx: &GpuContext, texel: [f32; 2], dir: [f32; 2]) {
        let uniform = BloomUniform {
            params: [self.intensity.max(0.0), self.spread.max(0.0), 0.0, 0.0],
            texel_dir: [texel[0], texel[1], dir[0], dir[1]],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn create_sample_bg(&self, ctx: &GpuContext, target: &RenderTarget) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_sample_bg"),
            layout: &self.sample_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(target.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }

    fn draw_single_input(
        &mut self,
        ctx: &mut GpuContext,
        pipeline_kind: BloomGraphPassKind,
        input: &RenderTarget,
        output: &RenderTarget,
        clear: bool,
        texel_dir: [f32; 2],
    ) {
        let pipeline = match pipeline_kind {
            BloomGraphPassKind::Downsample { .. } => {
                self.downsample_pipeline.pipeline(ctx, output.format())
            }
            BloomGraphPassKind::BlurHorizontal { .. } | BloomGraphPassKind::BlurVertical { .. } => {
                self.blur_pipeline.pipeline(ctx, output.format())
            }
            BloomGraphPassKind::Upsample { .. } => {
                self.upsample_pipeline.pipeline(ctx, output.format())
            }
            BloomGraphPassKind::Combine => unreachable!("combine has two inputs"),
        };
        let input_bg = self.create_sample_bg(ctx, input);
        self.update_uniform(ctx, texel(input), texel_dir);
        self.run_single_input(ctx, pipeline.as_ref(), &input_bg, output, clear);
    }

    fn draw_combine(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        bloom: &RenderTarget,
        output: &RenderTarget,
    ) {
        let combine_pipeline = self.combine_pipeline.pipeline(ctx, output.format());
        self.update_uniform(ctx, [0.0, 0.0], [0.0, 0.0]);
        let combine_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom_combine_bg"),
            layout: &self.dual_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        });
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom_combine_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(combine_pipeline.as_ref());
        pass.set_bind_group(0, &combine_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }

    pub fn setup_graph(
        graph: &mut RenderGraph,
        input: TextureHandle,
        output: TextureHandle,
        size: TargetSize,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> BloomGraph {
        let mips = std::array::from_fn(|level| {
            graph.create_texture(|builder| {
                builder
                    .name(format!("{label}_mip_{level}"))
                    .size(bloom_level_target_size(size, level))
                    .format(format);
            })
        });
        let scratches = std::array::from_fn(|level| {
            graph.create_texture(|builder| {
                builder
                    .name(format!("{label}_scratch_{level}"))
                    .size(bloom_level_target_size(size, level))
                    .format(format);
            })
        });

        let mut passes = Vec::with_capacity(BLOOM_GRAPH_PASS_COUNT);
        let handle = graph.add_render_pass(format!("{label}_downsample_0"), |setup| {
            setup.read(input);
            setup.write_color_cleared(0, mips[0], [0.0, 0.0, 0.0, 1.0]);
        });
        passes.push(BloomGraphPass {
            handle,
            kind: BloomGraphPassKind::Downsample { level: 0 },
        });

        for level in 1..BLOOM_LEVELS {
            let handle = graph.add_render_pass(format!("{label}_downsample_{level}"), |setup| {
                setup.read(mips[level - 1]);
                setup.write_color_cleared(0, mips[level], [0.0, 0.0, 0.0, 1.0]);
            });
            passes.push(BloomGraphPass {
                handle,
                kind: BloomGraphPassKind::Downsample { level },
            });
        }

        for level in 0..BLOOM_LEVELS {
            let handle = graph.add_render_pass(format!("{label}_blur_h_{level}"), |setup| {
                setup.read(mips[level]);
                setup.write_color_cleared(0, scratches[level], [0.0, 0.0, 0.0, 1.0]);
            });
            passes.push(BloomGraphPass {
                handle,
                kind: BloomGraphPassKind::BlurHorizontal { level },
            });

            let handle = graph.add_render_pass(format!("{label}_blur_v_{level}"), |setup| {
                setup.read(scratches[level]);
                setup.write_color_cleared(0, mips[level], [0.0, 0.0, 0.0, 1.0]);
            });
            passes.push(BloomGraphPass {
                handle,
                kind: BloomGraphPassKind::BlurVertical { level },
            });
        }

        for level in (1..BLOOM_LEVELS).rev() {
            let handle = graph.add_render_pass(format!("{label}_upsample_{level}"), |setup| {
                setup.read(mips[level]);
                setup.write_color_loaded(0, mips[level - 1]);
            });
            passes.push(BloomGraphPass {
                handle,
                kind: BloomGraphPassKind::Upsample { level },
            });
        }

        let handle = graph.add_render_pass(format!("{label}_combine"), |setup| {
            setup.read(input);
            setup.read(mips[0]);
            setup.write_color_cleared(0, output, [0.0, 0.0, 0.0, 1.0]);
        });
        passes.push(BloomGraphPass {
            handle,
            kind: BloomGraphPassKind::Combine,
        });

        BloomGraph {
            input,
            output,
            mips,
            scratches,
            passes,
        }
    }

    pub fn execute_graph_pass(
        &mut self,
        ctx: &mut GpuContext,
        graph: &BloomGraph,
        pass: &CompiledPass,
        resources: &PhysicalResources<'_>,
    ) -> Result<bool, RenderGraphError> {
        let Some(kind) = graph.pass_kind(pass.handle) else {
            return Ok(false);
        };

        match kind {
            BloomGraphPassKind::Downsample { level } => {
                let input_handle = if level == 0 {
                    graph.input
                } else {
                    graph.mips[level - 1]
                };
                let input = resources.render_target(input_handle).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing downsample input".into())
                })?;
                let output = resources.render_target(graph.mips[level]).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing downsample output".into())
                })?;
                self.draw_single_input(ctx, kind, input, output, true, [0.0, 0.0]);
            }
            BloomGraphPassKind::BlurHorizontal { level } => {
                let input = resources.render_target(graph.mips[level]).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing horizontal blur input".into())
                })?;
                let output = resources
                    .render_target(graph.scratches[level])
                    .ok_or_else(|| {
                        RenderGraphError::ExecutionFailed(
                            "bloom missing horizontal blur output".into(),
                        )
                    })?;
                self.draw_single_input(ctx, kind, input, output, true, [1.0, 0.0]);
            }
            BloomGraphPassKind::BlurVertical { level } => {
                let input = resources
                    .render_target(graph.scratches[level])
                    .ok_or_else(|| {
                        RenderGraphError::ExecutionFailed(
                            "bloom missing vertical blur input".into(),
                        )
                    })?;
                let output = resources.render_target(graph.mips[level]).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing vertical blur output".into())
                })?;
                self.draw_single_input(ctx, kind, input, output, true, [0.0, 1.0]);
            }
            BloomGraphPassKind::Upsample { level } => {
                let input = resources.render_target(graph.mips[level]).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing upsample input".into())
                })?;
                let output = resources
                    .render_target(graph.mips[level - 1])
                    .ok_or_else(|| {
                        RenderGraphError::ExecutionFailed("bloom missing upsample output".into())
                    })?;
                self.draw_single_input(ctx, kind, input, output, false, [0.0, 0.0]);
            }
            BloomGraphPassKind::Combine => {
                let input = resources.render_target(graph.input).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing combine input".into())
                })?;
                let bloom = resources.render_target(graph.mips[0]).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing combine mip".into())
                })?;
                let output = resources.render_target(graph.output).ok_or_else(|| {
                    RenderGraphError::ExecutionFailed("bloom missing combine output".into())
                })?;
                self.draw_combine(ctx, input, bloom, output);
            }
        }

        Ok(true)
    }

    fn run_single_input(
        &self,
        ctx: &mut GpuContext,
        pipeline: &wgpu::RenderPipeline,
        input_bg: &wgpu::BindGroup,
        output: &RenderTarget,
        clear: bool,
    ) {
        let load = if clear {
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        } else {
            wgpu::LoadOp::Load
        };
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, input_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }
}

fn bloom_level_target_size(size: TargetSize, level: usize) -> TargetSize {
    let scale = 1u32 << (level as u32 + 1);
    match size {
        TargetSize::Surface => TargetSize::Scale(1.0 / scale as f32),
        TargetSize::Scale(base) => TargetSize::Scale(base / scale as f32),
        TargetSize::Exact(width, height) => {
            TargetSize::Exact((width / scale).max(1), (height / scale).max(1))
        }
    }
}

fn texel(target: &RenderTarget) -> [f32; 2] {
    [1.0 / target.width() as f32, 1.0 / target.height() as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("render_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn constructs_all_shader_pipelines() {
        let (device, queue) = create_test_device();
        let ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [64, 64],
        );

        let bloom = Bloom::new(&ctx, wgpu::TextureFormat::Rgba16Float);

        let _ = bloom;
    }

    #[test]
    fn graph_pass_accounting_matches_rebuilt_pass_count() {
        assert_eq!(BLOOM_GRAPH_PASS_COUNT, 24);
        assert_eq!(
            BLOOM_GRAPH_PASS_COUNT,
            BLOOM_LEVELS + BLOOM_LEVELS * 2 + (BLOOM_LEVELS - 1) + 1
        );
    }

    #[test]
    fn graph_execution_runs_in_headless_frame() {
        let (device, queue) = create_test_device();
        let mut ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [64, 64],
        );
        let mut graph = RenderGraph::new();
        let input = graph.create_texture(|builder| {
            builder
                .name("input")
                .size(TargetSize::Exact(64, 64))
                .format(wgpu::TextureFormat::Rgba16Float);
        });
        let output = graph.create_texture(|builder| {
            builder
                .name("output")
                .size(TargetSize::Exact(64, 64))
                .format(wgpu::TextureFormat::Rgba16Float);
        });
        graph.add_render_pass("seed_input", |setup| {
            setup.write_color_cleared(0, input, [1.0, 1.0, 1.0, 1.0]);
        });
        let bloom_graph = Bloom::setup_graph(
            &mut graph,
            input,
            output,
            TargetSize::Exact(64, 64),
            wgpu::TextureFormat::Rgba16Float,
            "bloom",
        );
        graph.add_render_pass("present_output", |setup| {
            setup.read(output);
            setup.write_surface();
        });
        graph.compile().expect("bloom graph should compile");
        assert_eq!(graph.alive_pass_count(), BLOOM_GRAPH_PASS_COUNT + 2);

        let mut bloom = Bloom::new(&ctx, wgpu::TextureFormat::Rgba16Float);

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        graph
            .try_execute(&mut ctx, |pass, gpu, resources| {
                if bloom.execute_graph_pass(gpu, &bloom_graph, pass, resources)? {
                    return Ok(());
                }
                Ok(())
            })
            .expect("bloom graph execution should succeed");
        ctx.end_frame();
    }
}
