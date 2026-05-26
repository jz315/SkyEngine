use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use crate::ecs::EntityId;
use crate::render::resources::material::{MaterialError, SpriteMaterial};
use crate::render::resources::mesh::MeshHandle;
use crate::render::view::ResolvedSceneTransforms;

use super::{DrawContext, DrawError, DrawFunction, PhaseItem, PhasePayloadKind, SpriteDrawData};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SpritePhaseInstance {
    model_col0: [f32; 4],
    model_col1: [f32; 4],
    model_col2: [f32; 4],
    model_col3: [f32; 4],
    color: [f32; 4],
    uv_rect: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SpritePipelineKey {
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    view_layout_ptr: usize,
}

struct DrawSpriteRuntime {
    shader: wgpu::ShaderModule,
    texture_bgl: wgpu::BindGroupLayout,
    pipelines: FxHashMap<SpritePipelineKey, wgpu::RenderPipeline>,
}

impl DrawSpriteRuntime {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite_phase_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/sprite/sprite_draw.wgsl").into(),
            ),
        });
        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_phase_texture_bgl"),
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
        Self {
            shader,
            texture_bgl,
            pipelines: FxHashMap::default(),
        }
    }

    fn pipeline_for(
        &mut self,
        device: &wgpu::Device,
        view_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> &wgpu::RenderPipeline {
        let key = SpritePipelineKey {
            target_format,
            depth_format,
            view_layout_ptr: std::ptr::from_ref(view_layout) as usize,
        };
        if !self.pipelines.contains_key(&key) {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sprite_phase_pipeline_layout"),
                bind_group_layouts: &[Some(view_layout), Some(&self.texture_bgl)],
                immediate_size: 0,
            });
            let vertex_buffers = [
                wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpritePhaseInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 3,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            offset: 32,
                            shader_location: 4,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            offset: 48,
                            shader_location: 5,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            offset: 64,
                            shader_location: 6,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            offset: 80,
                            shader_location: 7,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                },
            ];
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
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sprite_phase_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(alpha_blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
            self.pipelines.insert(key, pipeline);
        }
        self.pipelines
            .get(&key)
            .expect("sprite phase pipeline cached for key")
    }
}

#[derive(Default)]
pub struct DrawSprite {
    runtime: Option<DrawSpriteRuntime>,
    prepared: FxHashMap<EntityId, SpritePhaseInstance>,
}

impl DrawSprite {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

impl DrawFunction for DrawSprite {
    #[inline]
    fn payload_kind(&self) -> PhasePayloadKind {
        PhasePayloadKind::of::<SpriteDrawData>()
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError> {
        self.draw_batch(ctx, std::slice::from_ref(item))
    }

    fn draw_batch(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        if !ctx.material_registry.is_registered::<SpriteMaterial>() {
            return Err(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<SpriteMaterial>(),
            }
            .into());
        }
        let runtime = self
            .runtime
            .get_or_insert_with(|| DrawSpriteRuntime::new(ctx.device));
        let mesh =
            ctx.mesh_registry
                .get(MeshHandle::BUILTIN_QUAD)
                .ok_or(DrawError::MissingMesh {
                    handle: MeshHandle::BUILTIN_QUAD,
                })?;
        let first_material_handle = items[0].data::<SpriteDrawData>().material_handle();
        let material = ctx
            .material_registry
            .get_erased::<SpriteMaterial>(first_material_handle)
            .map_err(|_| DrawError::MissingMaterial {
                type_name: std::any::type_name::<SpriteMaterial>(),
            })?;
        let texture_layout = runtime.texture_bgl.clone();
        let texture = material
            .texture
            .as_ref()
            .or(ctx.fallback_texture)
            .expect("sprite draw requires a texture or fallback texture");
        let texture_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_phase_texture_bg"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_nearest),
                },
            ],
        });
        let pipeline = runtime.pipeline_for(
            ctx.device,
            ctx.view_bind_group_layout,
            ctx.target_format,
            ctx.depth_format,
        );

        ctx.pass.set_pipeline(pipeline);
        ctx.pass.set_bind_group(0, ctx.view_bind_group, &[]);
        ctx.pass.set_bind_group(1, &texture_bind_group, &[]);
        ctx.pass
            .set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
        if let Some(index_buffer) = mesh.index_buffer() {
            ctx.pass.set_index_buffer(
                index_buffer.slice(..),
                mesh.index_format()
                    .expect("builtin quad uses indexed drawing"),
            );
        }

        let mut instances = Vec::with_capacity(items.len());
        for item in items {
            if item.data::<SpriteDrawData>().material_handle() != first_material_handle {
                return Err(DrawError::MissingMaterial {
                    type_name: "sprite batch contains mixed material handles",
                });
            }
            let Some(instance) = self.prepared.get(&item.entity).copied() else {
                continue;
            };
            instances.push(instance);
        }
        if instances.is_empty() {
            return Ok(());
        }
        let instance_buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sprite_phase_instance_buffer"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        ctx.pass.set_vertex_buffer(1, instance_buffer.slice(..));
        ctx.pass
            .draw_indexed(0..mesh.index_count(), 0, 0..instances.len() as u32);

        Ok(())
    }

    fn assign_model_matrix(
        &mut self,
        item: &mut PhaseItem,
        transforms: &ResolvedSceneTransforms,
        _entity_to_slot: &mut FxHashMap<EntityId, u32>,
        _model_matrices: &mut Vec<[f32; 16]>,
    ) {
        let sprite_data = *item.data::<SpriteDrawData>();
        let Some(mut transform) = transforms.get(item.entity) else {
            return;
        };
        transform.scale[0] *= sprite_data.size()[0];
        transform.scale[1] *= sprite_data.size()[1];
        let model = transform.to_matrix4().to_cols_array();
        let instance = SpritePhaseInstance {
            model_col0: [model[0], model[1], model[2], model[3]],
            model_col1: [model[4], model[5], model[6], model[7]],
            model_col2: [model[8], model[9], model[10], model[11]],
            model_col3: [model[12], model[13], model[14], model[15]],
            color: sprite_data.color(),
            uv_rect: sprite_data.uv_rect(),
        };
        self.prepared.insert(item.entity, instance);
    }

    fn draw_call_count(&self, items: &[PhaseItem]) -> usize {
        usize::from(!items.is_empty())
    }
}
