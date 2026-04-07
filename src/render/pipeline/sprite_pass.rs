use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::core::camera::{CameraUniform, RenderView};
use crate::render::core::color::Color;
use crate::render::ecs::RenderSettings2D;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};
use crate::render::pipeline::{FeatureExecutionContext2D, PipelineState2D, RenderFeature2D};
use crate::render::Texture;

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
static NEXT_SPRITE_SCENE_NAME: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
enum SpriteTargetFormat {
    Fixed(wgpu::TextureFormat),
    Surface,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

const QUAD_VERTICES: [QuadVertex; 4] = [
    QuadVertex {
        position: [0.0, 0.0],
        uv: [0.0, 1.0],
    },
    QuadVertex {
        position: [1.0, 0.0],
        uv: [1.0, 1.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
        uv: [1.0, 0.0],
    },
    QuadVertex {
        position: [0.0, 1.0],
        uv: [0.0, 0.0],
    },
];

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

struct SpritePassResources {
    shader: wgpu::ShaderModule,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bgl: wgpu::BindGroupLayout,
    scene_bgl: wgpu::BindGroupLayout,
    texture_bgl: wgpu::BindGroupLayout,
    pipelines_color: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,
    pipelines_textured: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,
    cached_scene_bind_group: Option<wgpu::BindGroup>,
    cached_scene_buffer_version: u64,
    cached_texture_bind_groups: FxHashMap<usize, wgpu::BindGroup>,
}

impl SpritePassResources {
    fn new(ctx: &GpuContext) -> Self {
        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sprite_scene_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/sprite_scene.wgsl").into(),
                ),
            });

        let vertex_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_scene_quad_vb"),
            size: std::mem::size_of_val(&QUAD_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&QUAD_VERTICES));

        let index_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_scene_quad_ib"),
            size: std::mem::size_of_val(&QUAD_INDICES) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ctx.queue()
            .write_buffer(&index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

        let camera_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_scene_camera_buf"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sprite_scene_camera_bgl"),
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
                label: Some("sprite_scene_storage_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sprite_scene_texture_bgl"),
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
            label: Some("sprite_scene_camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            shader,
            vertex_buffer,
            index_buffer,
            camera_buffer,
            camera_bind_group,
            camera_bgl,
            scene_bgl,
            texture_bgl,
            pipelines_color: FxHashMap::default(),
            pipelines_textured: FxHashMap::default(),
            cached_scene_bind_group: None,
            cached_scene_buffer_version: 0,
            cached_texture_bind_groups: FxHashMap::default(),
        }
    }

    fn pipeline_for(
        &mut self,
        ctx: &GpuContext,
        format: wgpu::TextureFormat,
        textured: bool,
    ) -> Arc<wgpu::RenderPipeline> {
        let map = if textured {
            &mut self.pipelines_textured
        } else {
            &mut self.pipelines_color
        };
        if let Some(existing) = map.get(&format) {
            return existing.clone();
        }

        let layouts: Vec<&wgpu::BindGroupLayout> = if textured {
            vec![&self.camera_bgl, &self.scene_bgl, &self.texture_bgl]
        } else {
            vec![&self.camera_bgl, &self.scene_bgl]
        };
        let layout = ctx
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sprite_scene_pipeline_layout"),
                bind_group_layouts: &layouts,
                push_constant_ranges: &[],
            });

        let alpha_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let pipeline = Arc::new(ctx.device().create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(&format!(
                    "sprite_scene_{}_pipeline_{format:?}",
                    if textured { "textured" } else { "color" }
                )),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<QuadVertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[
                                wgpu::VertexAttribute {
                                    offset: 0,
                                    shader_location: 0,
                                    format: wgpu::VertexFormat::Float32x2,
                                },
                                wgpu::VertexAttribute {
                                    offset: 8,
                                    shader_location: 1,
                                    format: wgpu::VertexFormat::Float32x2,
                                },
                            ],
                        },
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<u32>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &[wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Uint32,
                            }],
                        },
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some(if textured { "fs_main" } else { "fs_color_only" }),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(alpha_blend),
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
        map.insert(format, pipeline.clone());
        pipeline
    }

    fn scene_bind_group(
        &mut self,
        ctx: &GpuContext,
        sprite_table_buffer: &wgpu::Buffer,
        sprite_table_version: u64,
    ) -> &wgpu::BindGroup {
        if self.cached_scene_bind_group.is_none()
            || self.cached_scene_buffer_version != sprite_table_version
        {
            self.cached_scene_bind_group =
                Some(ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sprite_scene_storage_bg"),
                    layout: &self.scene_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: sprite_table_buffer.as_entire_binding(),
                    }],
                }));
            self.cached_scene_buffer_version = sprite_table_version;
        }

        self.cached_scene_bind_group
            .as_ref()
            .expect("sprite scene bind group should be cached")
    }

    fn texture_bind_groups(
        &mut self,
        ctx: &GpuContext,
        textures: &[Texture],
    ) -> Vec<wgpu::BindGroup> {
        let texture_bgl = self.texture_bgl.clone();
        let sampler = ctx.sampler_nearest();
        textures
            .iter()
            .map(|tex| {
                let texture_key = std::ptr::from_ref(tex.texture()) as usize;
                self.cached_texture_bind_groups
                    .entry(texture_key)
                    .or_insert_with(|| {
                        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("sprite_scene_texture_bg"),
                            layout: &texture_bgl,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(tex.view()),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(sampler),
                                },
                            ],
                        })
                    })
                    .clone()
            })
            .collect()
    }
}

pub struct SpritePass {
    resources: Option<SpritePassResources>,
    target_format: SpriteTargetFormat,
}

impl SpritePass {
    pub fn new() -> Self {
        Self {
            resources: None,
            target_format: SpriteTargetFormat::Fixed(HDR_FORMAT),
        }
    }

    pub(crate) fn hdr(ctx: &GpuContext) -> Self {
        let mut pass = Self::new();
        pass.initialize(ctx);
        pass
    }

    pub(crate) fn surface(ctx: &GpuContext) -> Self {
        let mut pass = Self {
            resources: None,
            target_format: SpriteTargetFormat::Surface,
        };
        pass.initialize(ctx);
        pass
    }

    fn initialize(&mut self, ctx: &GpuContext) {
        if self.resources.is_none() {
            self.resources = Some(SpritePassResources::new(ctx));
        }
    }

    fn resources_mut(&mut self, ctx: &GpuContext) -> &mut SpritePassResources {
        self.resources
            .get_or_insert_with(|| SpritePassResources::new(ctx))
    }

    fn target_format(&self, state: &PipelineState2D) -> wgpu::TextureFormat {
        match self.target_format {
            SpriteTargetFormat::Fixed(format) => format,
            SpriteTargetFormat::Surface => state.surface_format(),
        }
    }
}

impl Default for SpritePass {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderFeature2D for SpritePass {
    fn name(&self) -> &'static str {
        "sprites"
    }

    fn setup(&mut self, graph: &mut RenderGraph, state: &mut PipelineState2D) {
        let target_format = self.target_format(state);
        let scene_name = format!(
            "scene_{}",
            NEXT_SPRITE_SCENE_NAME.fetch_add(1, Ordering::Relaxed)
        );
        let scene_tex = graph.create_texture(|b| {
            b.name(scene_name.clone())
                .size(TargetSize::Exact(
                    state.view_size()[0],
                    state.view_size()[1],
                ))
                .format(target_format);
        });
        graph.add_render_pass("sprites", |s| {
            s.write_color(0, scene_tex);
        });
        state.set_scene_color(scene_tex, target_format);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &FeatureExecutionContext2D<'_>,
    ) -> Result<(), RenderGraphError> {
        let target_handle = pass.writes.iter().find_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        });
        let Some(target_handle) = target_handle else {
            return Ok(());
        };
        let Some(target) = resources.render_target(target_handle) else {
            return Ok(());
        };

        let resources_mut = self.resources_mut(ctx);
        ctx.queue().write_buffer(
            &resources_mut.camera_buffer,
            0,
            bytemuck::bytes_of(&execution.camera().view_uniform()),
        );
        let scene_bind_group = resources_mut
            .scene_bind_group(
                ctx,
                execution.sprite_table_buffer(),
                execution.sprite_table_version(),
            )
            .clone();

        let format = target.format();
        let bind_groups = resources_mut.texture_bind_groups(ctx, execution.textures());
        let pip_color = resources_mut.pipeline_for(ctx, format, false);
        let pip_tex = resources_mut.pipeline_for(ctx, format, true);
        let load = Some(execution.state().clear_color())
            .map(|c: Color| wgpu::LoadOp::Clear(c.to_wgpu()))
            .unwrap_or(wgpu::LoadOp::Load);

        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("sprite_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |render_pass| {
                render_pass.set_bind_group(0, &resources_mut.camera_bind_group, &[]);
                render_pass.set_bind_group(1, &scene_bind_group, &[]);
                render_pass.set_vertex_buffer(0, resources_mut.vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, execution.visible_sprite_index_buffer().slice(..));
                render_pass.set_index_buffer(
                    resources_mut.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );

                for draw in execution.draw_spans() {
                    if let Some(texture_index) = draw.texture_index {
                        let Some(bg) = bind_groups.get(texture_index) else {
                            continue;
                        };
                        render_pass.set_pipeline(pip_tex.as_ref());
                        render_pass.set_bind_group(2, bg, &[]);
                    } else {
                        render_pass.set_pipeline(pip_color.as_ref());
                    }
                    render_pass.draw_indexed(
                        0..6,
                        0,
                        draw.first_instance..(draw.first_instance + draw.instance_count),
                    );
                }
            },
        );
        Ok(())
    }

    fn draw_calls(&self, execution: &FeatureExecutionContext2D<'_>) -> usize {
        execution.draw_spans().len()
    }

    fn apply_settings(&mut self, _settings: &RenderSettings2D) {}
}
