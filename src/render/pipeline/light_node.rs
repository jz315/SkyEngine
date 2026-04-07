use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::core::camera::{CameraUniform, RenderView};
use crate::render::core::target::RenderTarget;
use crate::render::core::texture::Texture;
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    pos: [f32; 2],
}

const QUAD_VERTICES: [QuadVertex; 4] = [
    QuadVertex { pos: [0.0, 0.0] },
    QuadVertex { pos: [1.0, 0.0] },
    QuadVertex { pos: [1.0, 1.0] },
    QuadVertex { pos: [0.0, 1.0] },
];

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

struct LightNodeResources {
    shader: wgpu::ShaderModule,
    pipelines: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bgl: wgpu::BindGroupLayout,
    scene_bgl: wgpu::BindGroupLayout,
    normal_bgl: wgpu::BindGroupLayout,
    normal_bind_group: wgpu::BindGroup,
    _flat_normal: Texture,
    cached_scene_bind_group: Option<wgpu::BindGroup>,
    cached_scene_buffer_version: u64,
}

impl LightNodeResources {
    fn new(ctx: &GpuContext) -> Self {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("light_scene_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/light_scene.wgsl").into(),
                ),
            });

        let vertex_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_scene_quad_vb"),
            size: std::mem::size_of_val(&QUAD_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&QUAD_VERTICES));

        let index_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_scene_quad_ib"),
            size: std::mem::size_of_val(&QUAD_INDICES) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

        let camera_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("light_scene_camera_buf"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("light_scene_camera_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let scene_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("light_scene_storage_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let normal_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("light_scene_normal_bgl"),
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

        let camera_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light_scene_camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let flat_normal = Texture::flat_normal(ctx);
        let normal_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light_scene_normal_bg"),
            layout: &normal_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(flat_normal.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        });

        Self {
            shader,
            pipelines: FxHashMap::default(),
            vertex_buffer,
            index_buffer,
            camera_buffer,
            camera_bind_group,
            camera_bgl,
            scene_bgl,
            normal_bgl,
            normal_bind_group,
            _flat_normal: flat_normal,
            cached_scene_bind_group: None,
            cached_scene_buffer_version: 0,
        }
    }

    fn pipeline_for(
        &mut self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> Arc<wgpu::RenderPipeline> {
        if let Some(existing) = self.pipelines.get(&target_format) {
            return existing.clone();
        }

        let layout = ctx
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("light_scene_pipeline_layout"),
                bind_group_layouts: &[&self.camera_bgl, &self.scene_bgl, &self.normal_bgl],
                push_constant_ranges: &[],
            });

        let pipeline = Arc::new(ctx.device().create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(&format!("light_scene_pipeline_{target_format:?}")),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<QuadVertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float32x2,
                            }],
                        },
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<u32>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &[wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Uint32,
                            }],
                        },
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState {
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
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));
        self.pipelines.insert(target_format, pipeline.clone());
        pipeline
    }

    fn scene_bind_group(
        &mut self,
        ctx: &GpuContext,
        light_table_buffer: &wgpu::Buffer,
        light_table_version: u64,
    ) -> &wgpu::BindGroup {
        if self.cached_scene_bind_group.is_none()
            || self.cached_scene_buffer_version != light_table_version
        {
            self.cached_scene_bind_group =
                Some(ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("light_scene_storage_bg"),
                    layout: &self.scene_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: light_table_buffer.as_entire_binding(),
                    }],
                }));
            self.cached_scene_buffer_version = light_table_version;
        }

        self.cached_scene_bind_group
            .as_ref()
            .expect("light scene bind group should be cached")
    }
}

pub struct LightNode {
    resources: Option<LightNodeResources>,
    #[cfg(test)]
    last_prepared_ambient: Option<[f32; 4]>,
}

impl LightNode {
    pub fn new(ctx: &GpuContext) -> Self {
        let mut node = Self {
            resources: None,
            #[cfg(test)]
            last_prepared_ambient: None,
        };
        node.initialize(ctx);
        node
    }

    fn initialize(&mut self, ctx: &GpuContext) {
        if self.resources.is_none() {
            self.resources = Some(LightNodeResources::new(ctx));
        }
    }

    fn resources_mut(&mut self, ctx: &GpuContext) -> &mut LightNodeResources {
        self.resources
            .get_or_insert_with(|| LightNodeResources::new(ctx))
    }

    fn validate_targets(normal_target: Option<&RenderTarget>, lightmap: &RenderTarget) {
        if let Some(normal_target) = normal_target {
            assert!(
                !std::ptr::eq(normal_target.texture(), lightmap.texture()),
                "LightNode requires distinct normal and lightmap targets",
            );
        }
    }
}

impl RenderFeature2D for LightNode {
    fn name(&self) -> &'static str {
        "lights"
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let light_tex = graph.create_texture(|b| {
            b.name("light")
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("lights", |s| {
            s.write_color(0, light_tex);
        });
        state.set_lightmap(light_tex, HDR_FORMAT);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError> {
        #[cfg(test)]
        {
            self.last_prepared_ambient = Some(execution.state().ambient_color().to_array());
        }

        let write_handle = pass.writes.iter().find_map(|w| {
            if let ResourceRef::Texture(th) = w {
                Some(*th)
            } else {
                None
            }
        });
        if let Some(handle) = write_handle {
            if let Some(target) = resources.render_target(handle) {
                Self::validate_targets(None, target);

                let resources_mut = self.resources_mut(ctx);
                let pipeline = resources_mut.pipeline_for(ctx, target.format());
                ctx.queue().write_buffer(
                    &resources_mut.camera_buffer,
                    0,
                    bytemuck::bytes_of(&execution.camera().view_uniform()),
                );
                let scene_bg = resources_mut
                    .scene_bind_group(
                        ctx,
                        execution.light_table_buffer(),
                        execution.light_table_version(),
                    )
                    .clone();
                let normal_bg = resources_mut.normal_bind_group.clone();

                ctx.with_render_pass(
                    &wgpu::RenderPassDescriptor {
                        label: Some("light_scene_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target.view(),
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: execution.state().ambient_color().r as f64,
                                    g: execution.state().ambient_color().g as f64,
                                    b: execution.state().ambient_color().b as f64,
                                    a: execution.state().ambient_color().a as f64,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        ..Default::default()
                    },
                    |render_pass| {
                        render_pass.set_pipeline(pipeline.as_ref());
                        render_pass.set_bind_group(0, &resources_mut.camera_bind_group, &[]);
                        render_pass.set_bind_group(1, &scene_bg, &[]);
                        render_pass.set_bind_group(2, &normal_bg, &[]);
                        render_pass.set_vertex_buffer(0, resources_mut.vertex_buffer.slice(..));
                        render_pass
                            .set_vertex_buffer(1, execution.visible_light_index_buffer().slice(..));
                        render_pass.set_index_buffer(
                            resources_mut.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        let instance_range = execution.visible_light_range();
                        if instance_range.start < instance_range.end {
                            render_pass.draw_indexed(0..6, 0, instance_range);
                        }
                    },
                );
            }
        }
        Ok(())
    }

    fn is_enabled(&self, _settings: &RenderSettings2D, _has_surface: bool) -> bool {
        true
    }

    fn draw_calls(&self, execution: &FeatureExecutionContext2D<'_>) -> usize {
        usize::from(!execution.visible_light_range().is_empty())
    }

    #[cfg(test)]
    fn debug_last_light_ambient(&self) -> Option<[f32; 4]> {
        self.last_prepared_ambient
    }
}
