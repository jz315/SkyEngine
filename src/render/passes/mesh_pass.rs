//! Custom mesh rendering pass built on top of [`MaterialPipelineCache`].

use std::ops::Range;
use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::render::core::camera::{RenderView, ViewUniform};
use crate::render::core::color::Color;
use crate::render::core::target::RenderTarget;
use crate::render::resources::material::{
    MaterialError, MaterialInstance, MaterialPipelineCache, MaterialPipelineDesc,
};
use crate::render::resources::mesh::Mesh;

const VIEW_BIND_GROUP_SLOT: u32 = 0;

/// Errors returned by [`MeshPass`] rendering APIs.
#[derive(Debug, Clone, PartialEq)]
pub enum MeshPassError {
    Material(MaterialError),
    MissingMaterial {
        pipeline: String,
    },
    TargetSampleCountMismatch {
        pipeline: String,
        pipeline_samples: u32,
        target_samples: u32,
    },
    SurfaceSampleCountMismatch {
        pipeline: String,
        pipeline_samples: u32,
    },
    MissingDepthTarget {
        pipeline: String,
    },
    UnexpectedDepthTarget {
        pipeline: String,
    },
    DepthFormatMismatch {
        pipeline: String,
        pipeline_format: wgpu::TextureFormat,
        target_format: wgpu::TextureFormat,
    },
    DepthSampleCountMismatch {
        color_samples: u32,
        depth_samples: u32,
    },
    AliasingTargets,
    VertexRangeOutOfBounds {
        mesh: String,
        start: u32,
        end: u32,
        vertex_count: u32,
    },
    IndexRangeOutOfBounds {
        mesh: String,
        start: u32,
        end: u32,
        index_count: u32,
    },
    MissingIndexBuffer {
        mesh: String,
    },
    InvalidInstanceRange {
        start: u32,
        end: u32,
    },
}

impl std::fmt::Display for MeshPassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Material(err) => write!(f, "{err}"),
            Self::MissingMaterial { pipeline } => {
                write!(f, "Mesh pipeline \"{pipeline}\" requires a material instance")
            }
            Self::TargetSampleCountMismatch {
                pipeline,
                pipeline_samples,
                target_samples,
            } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" uses sample_count={}, but the color target uses {}",
                pipeline_samples, target_samples
            ),
            Self::SurfaceSampleCountMismatch {
                pipeline,
                pipeline_samples,
            } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" uses sample_count={}, but surface rendering only supports 1x",
                pipeline_samples
            ),
            Self::MissingDepthTarget { pipeline } => {
                write!(f, "Mesh pipeline \"{pipeline}\" requires a depth target")
            }
            Self::UnexpectedDepthTarget { pipeline } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" does not declare depth-stencil state"
            ),
            Self::DepthFormatMismatch {
                pipeline,
                pipeline_format,
                target_format,
            } => write!(
                f,
                "Mesh pipeline \"{pipeline}\" expects depth format {pipeline_format:?}, got {target_format:?}"
            ),
            Self::DepthSampleCountMismatch {
                color_samples,
                depth_samples,
            } => write!(
                f,
                "Color/depth sample counts must match, got color={} depth={}",
                color_samples, depth_samples
            ),
            Self::AliasingTargets => {
                write!(f, "MeshPass requires distinct color and depth targets")
            }
            Self::VertexRangeOutOfBounds {
                mesh,
                start,
                end,
                vertex_count,
            } => write!(
                f,
                "Vertex range {start}..{end} is out of bounds for mesh \"{mesh}\" with {} vertices",
                vertex_count
            ),
            Self::IndexRangeOutOfBounds {
                mesh,
                start,
                end,
                index_count,
            } => write!(
                f,
                "Index range {start}..{end} is out of bounds for mesh \"{mesh}\" with {} indices",
                index_count
            ),
            Self::MissingIndexBuffer { mesh } => {
                write!(f, "Mesh \"{mesh}\" has no index buffer")
            }
            Self::InvalidInstanceRange { start, end } => {
                write!(f, "Invalid instance range {start}..{end}")
            }
        }
    }
}

impl std::error::Error for MeshPassError {}

impl From<MaterialError> for MeshPassError {
    fn from(value: MaterialError) -> Self {
        Self::Material(value)
    }
}

/// One mesh draw submitted to [`MeshPass`].
pub struct MeshDraw<'a> {
    mesh: &'a Mesh,
    pipeline: &'a mut MaterialPipelineCache,
    material: Option<&'a mut MaterialInstance>,
    vertex_range: Option<Range<u32>>,
    index_range: Option<Range<u32>>,
    base_vertex: i32,
    instances: Range<u32>,
}

impl<'a> MeshDraw<'a> {
    pub fn new(mesh: &'a Mesh, pipeline: &'a mut MaterialPipelineCache) -> Self {
        Self {
            mesh,
            pipeline,
            material: None,
            vertex_range: None,
            index_range: None,
            base_vertex: 0,
            instances: 0..1,
        }
    }

    #[inline]
    pub fn material(mut self, material: &'a mut MaterialInstance) -> Self {
        self.material = Some(material);
        self
    }

    #[inline]
    pub fn vertices(mut self, range: Range<u32>) -> Self {
        self.vertex_range = Some(range);
        self
    }

    #[inline]
    pub fn indices(mut self, range: Range<u32>) -> Self {
        self.index_range = Some(range);
        self
    }

    #[inline]
    pub fn base_vertex(mut self, base_vertex: i32) -> Self {
        self.base_vertex = base_vertex;
        self
    }

    #[inline]
    pub fn instances(mut self, range: Range<u32>) -> Self {
        self.instances = range;
        self
    }
}

struct PreparedMeshDraw {
    pipeline: Arc<wgpu::RenderPipeline>,
    vertex_range: Range<u32>,
    index_range: Option<Range<u32>>,
    base_vertex: i32,
    instances: Range<u32>,
}

/// Generic mesh renderer using the shared material system.
pub struct MeshPass {
    view_buffer: wgpu::Buffer,
    view_bind_group: wgpu::BindGroup,
    view_bind_group_layout: wgpu::BindGroupLayout,
}

impl MeshPass {
    pub fn new(ctx: &GpuContext) -> Self {
        let view_bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("mesh_view_bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size:
                                Some(
                                    std::num::NonZeroU64::new(
                                        std::mem::size_of::<ViewUniform>() as u64
                                    )
                                    .expect("ViewUniform has non-zero size"),
                                ),
                        },
                        count: None,
                    }],
                });

        let view_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh_view_buffer"),
            size: std::mem::size_of::<ViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let view_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh_view_bg"),
            layout: &view_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buffer.as_entire_binding(),
            }],
        });

        Self {
            view_buffer,
            view_bind_group,
            view_bind_group_layout,
        }
    }

    #[inline]
    pub fn view_layout(&self) -> &wgpu::BindGroupLayout {
        &self.view_bind_group_layout
    }

    pub fn create_pipeline_cache(
        &self,
        ctx: &GpuContext,
        desc: MaterialPipelineDesc,
        properties_layout: Option<&wgpu::BindGroupLayout>,
        bindings_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Result<MaterialPipelineCache, MaterialError> {
        MaterialPipelineCache::try_new_with_fixed_layouts(
            ctx,
            desc,
            &[(VIEW_BIND_GROUP_SLOT, &self.view_bind_group_layout)],
            properties_layout,
            bindings_layout,
        )
    }

    pub fn render_to_target(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        view: &impl RenderView,
        clear: Option<Color>,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<(), MeshPassError> {
        self.upload_view(ctx, view);
        let prepared = self.prepare_draws(
            ctx,
            target.format(),
            target.sample_count(),
            None,
            false,
            draws,
        )?;

        let color_load = clear
            .map(Color::to_wgpu)
            .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("mesh_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |pass| {
                self.record_draws(pass, draws, &prepared);
            },
        );
        Ok(())
    }

    pub fn render_to_target_with_depth(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        depth_target: &RenderTarget,
        view: &impl RenderView,
        clear: Option<Color>,
        clear_depth: Option<f32>,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<(), MeshPassError> {
        if std::ptr::eq(target.texture(), depth_target.texture()) {
            return Err(MeshPassError::AliasingTargets);
        }
        if target.sample_count() != depth_target.sample_count() {
            return Err(MeshPassError::DepthSampleCountMismatch {
                color_samples: target.sample_count(),
                depth_samples: depth_target.sample_count(),
            });
        }

        self.upload_view(ctx, view);
        let prepared = self.prepare_draws(
            ctx,
            target.format(),
            target.sample_count(),
            Some(depth_target),
            false,
            draws,
        )?;

        let color_load = clear
            .map(Color::to_wgpu)
            .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let depth_load = clear_depth.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("mesh_pass_depth"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_target.view(),
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            },
            |pass| {
                self.record_draws(pass, draws, &prepared);
            },
        );
        Ok(())
    }

    pub fn render_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        view: &impl RenderView,
        clear: Option<Color>,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<(), MeshPassError> {
        self.upload_view(ctx, view);
        let prepared = self.prepare_draws(ctx, ctx.surface_format(), 1, None, true, draws)?;

        ctx.with_surface_pass("mesh_pass", clear.map(Color::to_wgpu), |pass| {
            self.record_draws(pass, draws, &prepared);
        });
        Ok(())
    }

    fn upload_view(&self, ctx: &GpuContext, view: &impl RenderView) {
        ctx.queue().write_buffer(
            &self.view_buffer,
            0,
            bytemuck::bytes_of(&view.view_uniform()),
        );
    }

    fn prepare_draws(
        &self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
        target_samples: u32,
        depth_target: Option<&RenderTarget>,
        is_surface: bool,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<Vec<PreparedMeshDraw>, MeshPassError> {
        let mut prepared = Vec::with_capacity(draws.len());

        for draw in draws.iter_mut() {
            let (pipeline_label, pipeline_samples, pipeline_depth) = {
                let pipeline_desc = draw.pipeline.desc();
                (
                    pipeline_desc.label.to_string(),
                    pipeline_desc.multisample.count.max(1),
                    pipeline_desc.depth_stencil.clone(),
                )
            };

            if is_surface {
                if pipeline_samples != 1 {
                    return Err(MeshPassError::SurfaceSampleCountMismatch {
                        pipeline: pipeline_label,
                        pipeline_samples,
                    });
                }
            } else if pipeline_samples != target_samples {
                return Err(MeshPassError::TargetSampleCountMismatch {
                    pipeline: pipeline_label,
                    pipeline_samples,
                    target_samples,
                });
            }

            match (pipeline_depth.as_ref(), depth_target) {
                (Some(depth_state), Some(depth_target)) => {
                    if depth_state.format != depth_target.format() {
                        return Err(MeshPassError::DepthFormatMismatch {
                            pipeline: pipeline_label,
                            pipeline_format: depth_state.format,
                            target_format: depth_target.format(),
                        });
                    }
                    if pipeline_samples != depth_target.sample_count() {
                        return Err(MeshPassError::DepthSampleCountMismatch {
                            color_samples: target_samples,
                            depth_samples: depth_target.sample_count(),
                        });
                    }
                }
                (Some(_), None) => {
                    return Err(MeshPassError::MissingDepthTarget {
                        pipeline: pipeline_label,
                    });
                }
                (None, Some(_)) => {
                    return Err(MeshPassError::UnexpectedDepthTarget {
                        pipeline: pipeline_label,
                    });
                }
                (None, None) => {}
            }

            let requires_material =
                draw.pipeline.property_slot().is_some() || draw.pipeline.resource_slot().is_some();
            if requires_material && draw.material.is_none() {
                return Err(MeshPassError::MissingMaterial {
                    pipeline: draw.pipeline.desc().label.to_string(),
                });
            }

            if let Some(material) = draw.material.as_deref_mut() {
                material.upload(ctx);
                if draw.pipeline.resource_slot().is_some() {
                    material.try_resource_bind_group()?;
                }
            }

            let instances = validate_instances(draw.instances.clone())?;
            let pipeline = draw.pipeline.try_pipeline_arc(ctx, target_format)?;

            let index_range = if draw.mesh.has_indices() || draw.index_range.is_some() {
                if !draw.mesh.has_indices() {
                    return Err(MeshPassError::MissingIndexBuffer {
                        mesh: draw.mesh.label().to_string(),
                    });
                }
                Some(validate_index_range(
                    draw.mesh,
                    draw.index_range
                        .clone()
                        .unwrap_or(0..draw.mesh.index_count()),
                )?)
            } else {
                None
            };

            let vertex_range = validate_vertex_range(
                draw.mesh,
                draw.vertex_range
                    .clone()
                    .unwrap_or(0..draw.mesh.vertex_count()),
            )?;

            prepared.push(PreparedMeshDraw {
                pipeline,
                vertex_range,
                index_range,
                base_vertex: draw.base_vertex,
                instances,
            });
        }

        Ok(prepared)
    }

    fn record_draws(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        draws: &mut [MeshDraw<'_>],
        prepared: &[PreparedMeshDraw],
    ) {
        for (draw, prepared) in draws.iter_mut().zip(prepared.iter()) {
            if prepared.instances.is_empty() {
                continue;
            }

            pass.set_pipeline(prepared.pipeline.as_ref());
            pass.set_bind_group(VIEW_BIND_GROUP_SLOT, &self.view_bind_group, &[]);

            if let Some(material) = draw.material.as_deref() {
                if let Some(slot) = draw.pipeline.property_slot() {
                    pass.set_bind_group(slot, material.property_bind_group(), &[]);
                }
                if let Some(slot) = draw.pipeline.resource_slot() {
                    pass.set_bind_group(
                        slot,
                        material
                            .try_resource_bind_group()
                            .expect("resource bind group validated before recording"),
                        &[],
                    );
                }
            }

            pass.set_vertex_buffer(0, draw.mesh.vertex_buffer().slice(..));
            if let Some(index_range) = &prepared.index_range {
                pass.set_index_buffer(
                    draw.mesh
                        .index_buffer()
                        .expect("indexed draw validated before recording")
                        .slice(..),
                    draw.mesh
                        .index_format()
                        .expect("indexed draw validated before recording"),
                );
                pass.draw_indexed(
                    index_range.clone(),
                    prepared.base_vertex,
                    prepared.instances.clone(),
                );
            } else {
                pass.draw(prepared.vertex_range.clone(), prepared.instances.clone());
            }
        }
    }
}

fn validate_instances(range: Range<u32>) -> Result<Range<u32>, MeshPassError> {
    if range.start > range.end {
        return Err(MeshPassError::InvalidInstanceRange {
            start: range.start,
            end: range.end,
        });
    }
    Ok(range)
}

fn validate_vertex_range(mesh: &Mesh, range: Range<u32>) -> Result<Range<u32>, MeshPassError> {
    if range.start > range.end || range.end > mesh.vertex_count() {
        return Err(MeshPassError::VertexRangeOutOfBounds {
            mesh: mesh.label().to_string(),
            start: range.start,
            end: range.end,
            vertex_count: mesh.vertex_count(),
        });
    }
    Ok(range)
}

fn validate_index_range(mesh: &Mesh, range: Range<u32>) -> Result<Range<u32>, MeshPassError> {
    if range.start > range.end || range.end > mesh.index_count() {
        return Err(MeshPassError::IndexRangeOutOfBounds {
            mesh: mesh.label().to_string(),
            start: range.start,
            end: range.end,
            index_count: mesh.index_count(),
        });
    }
    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::core::camera::Camera2D;
    use crate::render::core::target::RenderTargetDescriptor;
    use crate::render::resources::mesh::MeshIndexData;

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
                label: Some("mesh_pass_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        pos: [f32; 2],
        color: [f32; 4],
    }

    const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn mesh_shader() -> &'static str {
        r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: ViewUniform;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = view.view_proj * vec4<f32>(input.pos, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#
    }

    fn basic_pipeline_desc() -> MaterialPipelineDesc {
        MaterialPipelineDesc {
            label: "mesh_test".into(),
            shader_source: mesh_shader().into(),
            vs_entry: "vs_main",
            fs_entry: "fs_main",
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            material_properties_slot: None,
            material_resources_slot: None,
            vertex_buffers: vec![wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRIBUTES,
            }],
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            color_write_mask: wgpu::ColorWrites::ALL,
        }
    }

    #[test]
    fn mesh_pass_renders_indexed_mesh_to_target() {
        let (device, queue) = create_test_device();
        let mut ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [32, 32],
        );
        let camera = Camera2D::new(32.0, 32.0);
        let mut mesh_pass = MeshPass::new(&ctx);
        let mut pipeline = mesh_pass
            .create_pipeline_cache(&ctx, basic_pipeline_desc(), None, None)
            .expect("mesh pipeline should build");
        let mesh = Mesh::from_vertices_indices(
            &ctx,
            &[
                Vertex {
                    pos: [-8.0, -8.0],
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                Vertex {
                    pos: [8.0, -8.0],
                    color: [0.0, 1.0, 0.0, 1.0],
                },
                Vertex {
                    pos: [0.0, 8.0],
                    color: [0.0, 0.0, 1.0, 1.0],
                },
            ],
            MeshIndexData::U16(&[0, 1, 2]),
            "triangle",
        );
        let target = RenderTarget::new(&ctx, 32, 32, wgpu::TextureFormat::Rgba8Unorm, "mesh");
        let mut draws = [MeshDraw::new(&mesh, &mut pipeline)];

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        mesh_pass
            .render_to_target(&mut ctx, &target, &camera, Some(Color::BLACK), &mut draws)
            .expect("mesh pass should render");
        ctx.end_frame();
    }

    #[test]
    fn mesh_pass_rejects_sample_count_mismatch() {
        let (device, queue) = create_test_device();
        let mut ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [16, 16],
        );
        let camera = Camera2D::new(16.0, 16.0);
        let mut mesh_pass = MeshPass::new(&ctx);
        let mut pipeline = mesh_pass
            .create_pipeline_cache(&ctx, basic_pipeline_desc(), None, None)
            .expect("mesh pipeline should build");
        let mesh = Mesh::from_vertices(
            &ctx,
            &[Vertex {
                pos: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            }],
            "point",
        );
        let target = RenderTarget::from_descriptor(
            &ctx,
            RenderTargetDescriptor::new(16, 16, wgpu::TextureFormat::Rgba8Unorm).sample_count(4),
        );
        let mut draws = [MeshDraw::new(&mesh, &mut pipeline)];

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        let err = mesh_pass
            .render_to_target(&mut ctx, &target, &camera, None, &mut draws)
            .expect_err("sample mismatch should fail");
        ctx.end_frame();

        assert!(matches!(
            err,
            MeshPassError::TargetSampleCountMismatch {
                target_samples: 4,
                ..
            }
        ));
    }

    #[test]
    fn mesh_pass_rejects_depth_format_mismatch() {
        let (device, queue) = create_test_device();
        let mut ctx = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            wgpu::TextureFormat::Bgra8Unorm,
            [16, 16],
        );
        let camera = Camera2D::new(16.0, 16.0);
        let mut mesh_pass = MeshPass::new(&ctx);
        let mut desc = basic_pipeline_desc();
        desc.depth_stencil = Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let mut pipeline = mesh_pass
            .create_pipeline_cache(&ctx, desc, None, None)
            .expect("mesh pipeline should build");
        let mesh = Mesh::from_vertices(
            &ctx,
            &[Vertex {
                pos: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            }],
            "point",
        );
        let color = RenderTarget::new(&ctx, 16, 16, wgpu::TextureFormat::Rgba8Unorm, "color");
        let depth = RenderTarget::new(&ctx, 16, 16, wgpu::TextureFormat::Depth32Float, "depth");
        let mut draws = [MeshDraw::new(&mesh, &mut pipeline)];

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        let err = mesh_pass
            .render_to_target_with_depth(
                &mut ctx,
                &color,
                &depth,
                &camera,
                None,
                Some(1.0),
                &mut draws,
            )
            .expect_err("depth mismatch should fail");
        ctx.end_frame();

        assert!(matches!(
            err,
            MeshPassError::DepthFormatMismatch {
                pipeline_format: wgpu::TextureFormat::Depth24Plus,
                target_format: wgpu::TextureFormat::Depth32Float,
                ..
            }
        ));
    }
}
