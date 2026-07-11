use rustc_hash::FxHashMap;

use crate::ecs::EntityId;
use crate::render::phase::{
    DrawContext, DrawError, DrawFunction, PhaseItem, PhasePayload, PhasePayloadKind,
};
use crate::render::resources::mesh::MeshHandle;
use crate::render::view::ResolvedSceneTransforms;
use crate::render::ModelMatrixTable;

use super::{SharedTilemapFrameCache, TilemapInstance};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct TilemapDrawData {
    payload_index: u32,
    first_instance: u32,
    instance_count: u32,
    model_slot: u32,
}

impl TilemapDrawData {
    #[inline]
    pub(crate) const fn new_chunk_range(
        chunk_index: u32,
        first_instance: u32,
        instance_count: u32,
    ) -> Self {
        Self {
            payload_index: chunk_index,
            first_instance,
            instance_count,
            model_slot: 0,
        }
    }

    #[inline]
    pub(crate) const fn payload_index(self) -> u32 {
        self.payload_index
    }

    #[inline]
    pub(crate) const fn model_slot(self) -> u32 {
        self.model_slot
    }

    #[inline]
    pub(crate) fn set_model_slot(&mut self, model_slot: u32) {
        self.model_slot = model_slot;
    }

    #[inline]
    pub(crate) fn instance_range(self, chunk_instance_count: u32) -> std::ops::Range<u32> {
        let start = self.first_instance.min(chunk_instance_count);
        let count = if self.instance_count == u32::MAX {
            chunk_instance_count.saturating_sub(start)
        } else {
            self.instance_count
        };
        let end = start.saturating_add(count).min(chunk_instance_count);
        start..end
    }
}

impl PhasePayload for TilemapDrawData {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TilemapPipelineKey {
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    view_layout_ptr: usize,
    model_layout_ptr: usize,
}

struct DrawTilemapRuntime {
    shader: wgpu::ShaderModule,
    texture_bgl: wgpu::BindGroupLayout,
    pipelines: FxHashMap<TilemapPipelineKey, wgpu::RenderPipeline>,
}

impl DrawTilemapRuntime {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tilemap_phase_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../shaders/tilemap/tilemap_draw.wgsl").into(),
            ),
        });
        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tilemap_phase_texture_bgl"),
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
        model_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> &wgpu::RenderPipeline {
        let key = TilemapPipelineKey {
            target_format,
            depth_format,
            view_layout_ptr: std::ptr::from_ref(view_layout) as usize,
            model_layout_ptr: std::ptr::from_ref(model_layout) as usize,
        };
        if !self.pipelines.contains_key(&key) {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("tilemap_phase_pipeline_layout"),
                bind_group_layouts: &[
                    Some(view_layout),
                    Some(model_layout),
                    Some(&self.texture_bgl),
                ],
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
                    array_stride: std::mem::size_of::<TilemapInstance>() as u64,
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
                        wgpu::VertexAttribute {
                            offset: 96,
                            shader_location: 8,
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
                label: Some("tilemap_phase_pipeline"),
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
            .expect("tilemap phase pipeline cached for key")
    }
}

pub(crate) struct DrawTilemap {
    runtime: Option<DrawTilemapRuntime>,
    cache: SharedTilemapFrameCache,
}

impl DrawTilemap {
    #[inline]
    pub(crate) fn new(cache: SharedTilemapFrameCache) -> Self {
        Self {
            runtime: None,
            cache,
        }
    }
}

impl DrawFunction for DrawTilemap {
    #[inline]
    fn payload_kind(&self) -> PhasePayloadKind {
        PhasePayloadKind::of::<TilemapDrawData>()
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
        if items.is_empty() {
            return Ok(());
        }

        let device = ctx.device().clone();
        let sampler_nearest = ctx.sampler_nearest().clone();
        let view_bind_group = ctx.view_bind_group().clone();
        let view_bind_group_layout = ctx.view_bind_group_layout().clone();
        let model_bind_group_layout = ctx.model_bind_group_layout().clone();
        let (model_bind_group, model_stride) = {
            let model_table = ctx
                .gpu_scene()
                .map(|scene| scene.table::<ModelMatrixTable>())
                .ok_or(DrawError::MissingFramePayload {
                    type_name: "ModelMatrixTable",
                })?;
            (model_table.bind_group().clone(), model_table.stride())
        };
        let fallback_texture = ctx.fallback_texture().cloned();
        let target_format = ctx.target_format();
        let depth_format = ctx.depth_format();
        let (vertex_buffer, index_buffer, index_format, index_count) = {
            let mesh = ctx.mesh_registry().get(MeshHandle::BUILTIN_QUAD).ok_or(
                DrawError::MissingMesh {
                    handle: MeshHandle::BUILTIN_QUAD,
                },
            )?;
            (
                mesh.vertex_buffer().clone(),
                mesh.index_buffer().cloned(),
                mesh.index_format()
                    .expect("builtin quad uses indexed drawing"),
                mesh.index_count(),
            )
        };

        let runtime = self
            .runtime
            .get_or_insert_with(|| DrawTilemapRuntime::new(&device));
        let cache = self.cache.lock().expect("tilemap frame cache poisoned");
        let first = *items[0].data::<TilemapDrawData>();
        let (first_prepared, _) =
            cache
                .chunk(first.payload_index())
                .ok_or(DrawError::MissingFramePayload {
                    type_name: "TilemapFrameCache",
                })?;
        let texture_key = first_prepared.texture_key;
        let first_texture = first_prepared.texture.clone();
        let texture_layout = runtime.texture_bgl.clone();
        let texture = first_texture
            .or_else(|| fallback_texture.clone())
            .expect("tilemap draw requires a texture or fallback texture");
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tilemap_phase_texture_bg"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler_nearest),
                },
            ],
        });
        let pipeline = runtime.pipeline_for(
            &device,
            &view_bind_group_layout,
            &model_bind_group_layout,
            target_format,
            depth_format,
        );

        ctx.pass().set_pipeline(pipeline);
        ctx.pass().set_bind_group(0, &view_bind_group, &[]);
        ctx.pass().set_bind_group(2, &texture_bind_group, &[]);
        ctx.pass().set_vertex_buffer(0, vertex_buffer.slice(..));
        if let Some(index_buffer) = index_buffer {
            ctx.pass()
                .set_index_buffer(index_buffer.slice(..), index_format);
        }

        for item in items {
            let draw_data = *item.data::<TilemapDrawData>();
            let Some((prepared, gpu_chunk)) = cache.chunk(draw_data.payload_index()) else {
                continue;
            };
            if prepared.texture_key != texture_key {
                return Err(DrawError::MissingMaterial {
                    type_name: "tilemap batch contains mixed textures",
                });
            }
            let instance_range = draw_data.instance_range(gpu_chunk.instance_count);
            if instance_range.is_empty() {
                continue;
            }
            let model_offset = draw_data.model_slot().saturating_mul(model_stride);
            ctx.pass()
                .set_bind_group(1, &model_bind_group, &[model_offset]);
            ctx.pass().set_vertex_buffer(1, gpu_chunk.buffer.slice(..));
            ctx.pass().draw_indexed(0..index_count, 0, instance_range);
        }

        Ok(())
    }

    fn assign_model_matrix(
        &mut self,
        item: &mut PhaseItem,
        transforms: &ResolvedSceneTransforms,
        entity_to_slot: &mut FxHashMap<EntityId, u32>,
        model_matrices: &mut Vec<[f32; 16]>,
    ) {
        let slot = if let Some(slot) = entity_to_slot.get(&item.entity).copied() {
            slot
        } else if let Some(transform) = transforms.get(item.entity) {
            let slot = model_matrices.len() as u32;
            model_matrices.push(transform.to_matrix4().to_cols_array());
            entity_to_slot.insert(item.entity, slot);
            slot
        } else {
            0
        };
        item.data_mut::<TilemapDrawData>().set_model_slot(slot);
    }

    fn draw_call_count(&self, items: &[PhaseItem]) -> usize {
        items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::phase::create_model_bind_group_layout;

    fn create_test_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("test adapter should be available");

        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("tilemap_draw_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("test device should be available");
        device
    }

    fn create_view_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tilemap_draw_test_view_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<
                            crate::render::view::ViewUniform,
                        >() as u64)
                        .expect("view uniform has non-zero size"),
                    ),
                },
                count: None,
            }],
        })
    }

    #[test]
    fn tilemap_pipeline_accepts_model_matrix_layout() {
        let device = create_test_device();
        let view_layout = create_view_bind_group_layout(&device);
        let model_layout = create_model_bind_group_layout(&device);
        let mut runtime = DrawTilemapRuntime::new(&device);

        let _pipeline = runtime.pipeline_for(
            &device,
            &view_layout,
            &model_layout,
            wgpu::TextureFormat::Rgba8Unorm,
            None,
        );
    }
}
