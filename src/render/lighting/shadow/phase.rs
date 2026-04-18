use std::hash::{Hash, Hasher};

use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use crate::render::gpu::{RenderTarget, DEFAULT_DEPTH_FORMAT};
use crate::render::graph::{ImportedTexture, RenderGraphError};
use crate::render::phase::{MeshDrawData, OpaquePhase};
use crate::render::pipeline::{RenderPhase, RenderPhaseExecuteContext, RenderPhaseSetupContext};
use crate::render::resources::mesh::{VertexLayout, VertexSemantic};
use crate::render::view::SceneView;

use super::view::{ShadowViewBinding, IDENTITY_MATRIX};
use super::ShadowPassBindingLayout;

pub struct DirectionalShadowPhase {
    pipelines: FxHashMap<u64, wgpu::RenderPipeline>,
}

impl DirectionalShadowPhase {
    #[inline]
    pub fn new() -> Self {
        Self {
            pipelines: FxHashMap::default(),
        }
    }

    fn pipeline_for(
        &mut self,
        device: &wgpu::Device,
        shadow_layout: &wgpu::BindGroupLayout,
        _model_layout: &wgpu::BindGroupLayout,
        mesh_layout: &VertexLayout,
    ) -> Result<&wgpu::RenderPipeline, RenderGraphError> {
        let mut hasher = rustc_hash::FxHasher::default();
        mesh_layout.hash(&mut hasher);
        let key = hasher.finish();
        if !self.pipelines.contains_key(&key) {
            let position = mesh_layout
                .attributes()
                .iter()
                .find(|attribute| attribute.semantic == VertexSemantic::Position)
                .ok_or_else(|| {
                    RenderGraphError::ExecutionFailed(
                        "shadow caster mesh is missing a position attribute".into(),
                    )
                })?;
            if position.format != wgpu::VertexFormat::Float32x3 {
                return Err(RenderGraphError::ExecutionFailed(format!(
                    "shadow caster position attribute must be Float32x3, got {:?}",
                    position.format
                )));
            }

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("directional_shadow_shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../../shaders/shadow_depth.wgsl").into(),
                ),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("directional_shadow_pipeline_layout"),
                bind_group_layouts: &[shadow_layout],
                push_constant_ranges: &[],
            });
            let attributes = [wgpu::VertexAttribute {
                format: position.format,
                offset: position.offset as u64,
                shader_location: 0,
            }];
            let instance_attributes = [
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ];
            let vertex_buffers = [
                wgpu::VertexBufferLayout {
                    array_stride: mesh_layout.stride() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 16]>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &instance_attributes,
                },
            ];
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("directional_shadow_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: None,
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEFAULT_DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            self.pipelines.insert(key, pipeline);
        }
        Ok(self
            .pipelines
            .get(&key)
            .expect("directional shadow pipeline inserted for mesh layout"))
    }
}

impl Default for DirectionalShadowPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderPhase for DirectionalShadowPhase {
    fn name(&self) -> &'static str {
        "directional_shadow"
    }

    fn is_enabled(
        &self,
        _frame: &crate::render::execution::PreparedFrame<'_>,
        view: &crate::render::execution::PreparedView<'_>,
    ) -> bool {
        view.payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
            && view
                .payload::<ShadowViewBinding>()
                .is_some_and(ShadowViewBinding::enabled)
    }

    fn setup(&mut self, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
        let Some(scene_view) = ctx.view().payload::<SceneView>() else {
            return;
        };
        if !scene_view.is_shadow() {
            return;
        }
        let Some(shadow_view) = ctx.view().payload::<ShadowViewBinding>() else {
            return;
        };
        if !shadow_view.enabled() {
            return;
        }
        let handle = ctx
            .state()
            .texture_slot("directional_shadow_target")
            .map(|slot| slot.handle())
            .unwrap_or_else(|| {
                let handle = ctx.graph().create_texture(|builder| {
                    builder
                        .name("directional_shadow_target")
                        .import_external(import_shadow_target(shadow_view.target()));
                });
                ctx.state().set_texture_slot(
                    "directional_shadow_target",
                    handle,
                    shadow_view.target().format(),
                );
                handle
            });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.set_depth_stencil(handle);
        });
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (
            gpu,
            pass,
            resources,
            execution,
            _draw_functions,
            _material_registry,
            mesh_registry,
            _fallback_texture,
        ) = ctx.split();
        let shadow_pass_layout = execution
            .frame_payload::<ShadowPassBindingLayout>()
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed("missing shadow pass layout payload".into())
            })?;
        let shadow_view = execution
            .view_payload::<ShadowViewBinding>()
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed("missing shadow view payload".into())
            })?;
        if !shadow_view.enabled() {
            return Ok(());
        }
        let scene_view = execution
            .view_payload::<SceneView>()
            .ok_or_else(|| RenderGraphError::ExecutionFailed("missing SceneView payload".into()))?;
        if !scene_view.is_shadow() {
            return Ok(());
        }
        let opaque_phase = execution.view_payload::<OpaquePhase>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("missing opaque phase payload".into())
        })?;
        let gpu_scene = execution
            .frame_payload::<crate::render::GpuScene>()
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed("missing GpuScene frame payload".into())
            })?;
        let model_matrices = execution.frame_payload::<Vec<[f32; 16]>>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("missing model matrix payload".into())
        })?;
        let depth_handle = pass
            .depth_stencil
            .as_ref()
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed(
                    "directional shadow pass missing depth target".into(),
                )
            })?
            .handle;

        let device = gpu.device().clone();
        let mut frame = gpu.frame();
        let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("directional_shadow"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.view(depth_handle),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        let mut cursor = 0usize;
        while cursor < opaque_phase.items().len() {
            let base = *opaque_phase.items()[cursor].data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let sub_mesh_index = base.sub_mesh_index();
            let mut batch_end = cursor + 1;
            while batch_end < opaque_phase.items().len() {
                let next = *opaque_phase.items()[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle || next.sub_mesh_index() != sub_mesh_index {
                    break;
                }
                batch_end += 1;
            }

            let Some(mesh) = mesh_registry.get(mesh_handle) else {
                cursor = batch_end;
                continue;
            };
            let pipeline = self.pipeline_for(
                &device,
                shadow_pass_layout.bind_group_layout(),
                gpu_scene.model_bind_group_layout(),
                mesh.vertex_layout(),
            )?;
            let instances: Vec<[f32; 16]> = opaque_phase.items()[cursor..batch_end]
                .iter()
                .map(|item| {
                    let mesh_data = *item.data::<MeshDrawData>();
                    model_matrices
                        .get(mesh_data.model_slot() as usize)
                        .copied()
                        .unwrap_or(IDENTITY_MATRIX)
                })
                .collect();
            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("directional_shadow_instances"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, shadow_view.shadow_pass_bind_group(), &[]);
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            render_pass.set_vertex_buffer(1, instance_buffer.slice(..));

            let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                cursor = batch_end;
                continue;
            };
            if mesh.has_indices() {
                let Some(index_buffer) = mesh.index_buffer() else {
                    cursor = batch_end;
                    continue;
                };
                let index_count = if sub_mesh.index_count == 0 {
                    mesh.index_count()
                } else {
                    sub_mesh.index_count
                };
                let index_offset = if sub_mesh.index_count == 0 {
                    0
                } else {
                    sub_mesh.index_offset
                };
                render_pass.set_index_buffer(
                    index_buffer.slice(..),
                    mesh.index_format()
                        .expect("indexed meshes provide an index format"),
                );
                render_pass.draw_indexed(
                    index_offset..(index_offset + index_count),
                    sub_mesh.vertex_offset,
                    0..instances.len() as u32,
                );
            } else {
                render_pass.draw(0..mesh.vertex_count(), 0..instances.len() as u32);
            }

            cursor = batch_end;
        }

        Ok(())
    }

    fn draw_calls(&self, execution: &crate::render::execution::ViewExecutionContext<'_>) -> usize {
        if !execution
            .view_payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
        {
            return 0;
        }
        let Some(shadow_view) = execution.view_payload::<ShadowViewBinding>() else {
            return 0;
        };
        if !shadow_view.enabled() {
            return 0;
        }
        let Some(opaque_phase) = execution.view_payload::<OpaquePhase>() else {
            return 0;
        };

        let mut draws = 0usize;
        let mut cursor = 0usize;
        while cursor < opaque_phase.items().len() {
            let base = *opaque_phase.items()[cursor].data::<MeshDrawData>();
            let mut batch_end = cursor + 1;
            while batch_end < opaque_phase.items().len() {
                let next = *opaque_phase.items()[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != base.mesh_handle()
                    || next.sub_mesh_index() != base.sub_mesh_index()
                {
                    break;
                }
                batch_end += 1;
            }
            draws += 1;
            cursor = batch_end;
        }
        draws
    }
}

fn import_shadow_target(target: &RenderTarget) -> ImportedTexture {
    ImportedTexture {
        texture: std::sync::Arc::new(target.texture().clone()),
        view: std::sync::Arc::new(target.view().clone()),
        size: [target.width(), target.height()],
        format: target.format(),
        sample_count: target.sample_count(),
        mip_level_count: target.mip_level_count(),
    }
}
