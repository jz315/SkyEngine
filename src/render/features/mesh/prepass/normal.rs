use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use crate::render::execution::{
    create_scene_texture, ensure_scene_texture, pass_first_write_texture, pass_nth_write_texture,
    require_render_target, PreparedFrame, PreparedView, SceneTexture,
};
use crate::render::execution::{PhaseExecuteContext, PhaseSetupContext};
use crate::render::gpu::GpuScene;
use crate::render::graph::RenderGraphError;
use crate::render::phase::{MeshDrawData, OpaquePhase};
use crate::render::pipeline::RenderPhase;
use crate::render::resources::mesh::{VertexAttribute, VertexLayout, VertexSemantic};
use crate::render::runtime::PreviousModelMatrices;
use crate::render::{SceneView, DEFAULT_DEPTH_FORMAT};

pub(super) const SCENE_NORMAL_FORMAT: wgpu::TextureFormat = SceneTexture::Normal.modern_3d_format();
pub(super) const SCENE_VELOCITY_FORMAT: wgpu::TextureFormat =
    SceneTexture::Velocity.modern_3d_format();
pub(super) const SCENE_VELOCITY_CLEAR: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const SCENE_NORMAL_SHADER: &str =
    include_str!("../../../shaders/prepass/scene_normal_prepass.wgsl");

pub(super) const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct NormalPrepassInstance {
    pub(super) model_col0: [f32; 4],
    pub(super) model_col1: [f32; 4],
    pub(super) model_col2: [f32; 4],
    pub(super) model_col3: [f32; 4],
    pub(super) prev_model_col0: [f32; 4],
    pub(super) prev_model_col1: [f32; 4],
    pub(super) prev_model_col2: [f32; 4],
    pub(super) prev_model_col3: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PreviousViewProjUniform {
    prev_view_proj: [f32; 16],
}

impl NormalPrepassInstance {
    #[inline]
    pub(super) fn from_models(model: [f32; 16], previous: [f32; 16]) -> Self {
        Self {
            model_col0: [model[0], model[1], model[2], model[3]],
            model_col1: [model[4], model[5], model[6], model[7]],
            model_col2: [model[8], model[9], model[10], model[11]],
            model_col3: [model[12], model[13], model[14], model[15]],
            prev_model_col0: [previous[0], previous[1], previous[2], previous[3]],
            prev_model_col1: [previous[4], previous[5], previous[6], previous[7]],
            prev_model_col2: [previous[8], previous[9], previous[10], previous[11]],
            prev_model_col3: [previous[12], previous[13], previous[14], previous[15]],
        }
    }
}

fn normal_prepass_required_layout() -> VertexLayout {
    VertexLayout::new(
        24,
        [
            VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
            VertexAttribute::new(VertexSemantic::Normal, wgpu::VertexFormat::Float32x3, 12),
        ],
    )
}

fn normal_prepass_vertex_attributes(
    mesh_layout: &VertexLayout,
) -> Option<Vec<wgpu::VertexAttribute>> {
    normal_prepass_required_layout()
        .attributes()
        .iter()
        .enumerate()
        .map(|(shader_location, required)| {
            let actual = mesh_layout
                .attributes()
                .iter()
                .find(|candidate| candidate.semantic == required.semantic)?;
            if actual.format != required.format {
                return None;
            }
            Some(wgpu::VertexAttribute {
                format: actual.format,
                offset: actual.offset as u64,
                shader_location: shader_location as u32,
            })
        })
        .collect()
}

fn normal_prepass_instance_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    const ATTRS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        8 => Float32x4,
        9 => Float32x4,
        10 => Float32x4,
        11 => Float32x4,
        12 => Float32x4,
        13 => Float32x4,
        14 => Float32x4,
        15 => Float32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<NormalPrepassInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}

fn normal_prepass_instance_buffer(
    device: &wgpu::Device,
    models: &[[f32; 16]],
    previous_models: &[[f32; 16]],
) -> wgpu::Buffer {
    let instances: Vec<NormalPrepassInstance> = models
        .iter()
        .cloned()
        .zip(previous_models.iter().cloned())
        .map(|(model, previous)| NormalPrepassInstance::from_models(model, previous))
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("scene_normal_prepass_instances"),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

#[derive(Default)]
pub struct SceneNormalPrepass {
    shader: Option<Arc<wgpu::ShaderModule>>,
    pipelines: FxHashMap<u64, Arc<wgpu::RenderPipeline>>,
    prev_view_proj_layout: Option<wgpu::BindGroupLayout>,
    prev_view_proj_buffer: Option<wgpu::Buffer>,
    prev_view_proj_bind_group: Option<wgpu::BindGroup>,
}

impl SceneNormalPrepass {
    fn is_view_enabled(view: &PreparedView<'_>) -> bool {
        !view
            .payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
            && view
                .payload::<OpaquePhase>()
                .is_some_and(|phase| !phase.is_empty())
    }

    fn ensure_shader(&mut self, device: &wgpu::Device) -> Arc<wgpu::ShaderModule> {
        if let Some(shader) = &self.shader {
            return shader.clone();
        }
        let shader = Arc::new(device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene_normal_prepass_shader"),
            source: wgpu::ShaderSource::Wgsl(SCENE_NORMAL_SHADER.into()),
        }));
        self.shader = Some(shader.clone());
        shader
    }

    fn ensure_previous_view_proj_resources(&mut self, device: &wgpu::Device) {
        if self.prev_view_proj_layout.is_none() {
            self.prev_view_proj_layout = Some(device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("scene_normal_prepass_prev_view_proj_bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
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
        if self.prev_view_proj_buffer.is_none() {
            self.prev_view_proj_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene_normal_prepass_prev_view_proj"),
                size: std::mem::size_of::<PreviousViewProjUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if self.prev_view_proj_bind_group.is_none() {
            self.prev_view_proj_bind_group = Some(
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("scene_normal_prepass_prev_view_proj_bg"),
                    layout: self
                        .prev_view_proj_layout
                        .as_ref()
                        .expect("previous-view-proj layout should exist"),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self
                            .prev_view_proj_buffer
                            .as_ref()
                            .expect("previous-view-proj buffer should exist")
                            .as_entire_binding(),
                    }],
                }),
            );
        }
    }

    fn pipeline_key(
        mesh_layout: &VertexLayout,
        view_layout: &wgpu::BindGroupLayout,
        normal_format: wgpu::TextureFormat,
        velocity_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        mesh_layout.hash(&mut hasher);
        (std::ptr::from_ref(view_layout) as usize).hash(&mut hasher);
        normal_format.hash(&mut hasher);
        velocity_format.hash(&mut hasher);
        depth_format.hash(&mut hasher);
        hasher.finish()
    }

    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        view_layout: &wgpu::BindGroupLayout,
        mesh_layout: &VertexLayout,
        normal_format: wgpu::TextureFormat,
        velocity_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Option<Arc<wgpu::RenderPipeline>> {
        self.ensure_previous_view_proj_resources(device);
        let key = Self::pipeline_key(
            mesh_layout,
            view_layout,
            normal_format,
            velocity_format,
            depth_format,
        );
        if let Some(pipeline) = self.pipelines.get(&key) {
            return Some(pipeline.clone());
        }

        let attributes = normal_prepass_vertex_attributes(mesh_layout)?;
        let vertex_layouts = [
            wgpu::VertexBufferLayout {
                array_stride: mesh_layout.stride() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            },
            normal_prepass_instance_layout(),
        ];
        let shader = self.ensure_shader(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene_normal_prepass_layout"),
            bind_group_layouts: &[
                Some(view_layout),
                Some(
                    self.prev_view_proj_layout
                        .as_ref()
                        .expect("previous-view-proj layout should exist"),
                ),
            ],
            immediate_size: 0,
        });
        let pipeline = Arc::new(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scene_normal_prepass_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &vertex_layouts,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: normal_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(wgpu::ColorTargetState {
                            format: velocity_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth_format,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            }),
        );
        self.pipelines.insert(key, pipeline.clone());
        Some(pipeline)
    }
}

impl RenderPhase for SceneNormalPrepass {
    fn name(&self) -> &'static str {
        "scene_normal_prepass"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        Self::is_view_enabled(view)
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        if !Self::is_view_enabled(ctx.view()) {
            return;
        }

        let target_size = ctx.view().target_size();
        let (normal, velocity, depth, existing_depth) = {
            let (graph, state) = ctx.graph_and_state();
            let normal = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Normal,
                SCENE_NORMAL_FORMAT,
                "scene_normal",
            );
            let velocity = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Velocity,
                SCENE_VELOCITY_FORMAT,
                "scene_velocity",
            );
            let existing_depth = state.scene_depth().is_some();
            let depth = ensure_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Depth,
                DEFAULT_DEPTH_FORMAT,
                "scene_depth",
            );
            (normal, velocity, depth, existing_depth)
        };

        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_cleared(0, normal.handle(), [0.5, 0.5, 1.0, 1.0]);
            setup.write_color_cleared(1, velocity.handle(), SCENE_VELOCITY_CLEAR);
            if existing_depth {
                setup.set_depth_stencil_loaded(depth.handle());
            } else {
                setup.set_depth_stencil(depth.handle());
            }
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution, draw_services) = ctx.split();
        let mesh_registry = draw_services.mesh_registry();
        let Some(scene_view) = execution.view_payload::<SceneView>() else {
            return Ok(());
        };
        if scene_view.is_shadow() {
            return Ok(());
        }
        let Some(opaque_phase) = execution.view_payload::<OpaquePhase>() else {
            return Ok(());
        };
        let phase_items = opaque_phase.items();
        if phase_items.is_empty() {
            return Ok(());
        }

        let Some(gpu_scene) = execution.frame_payload::<GpuScene>() else {
            return Ok(());
        };
        gpu_scene.write_view_uniform(gpu.queue(), &scene_view.view_uniform);

        let normal_handle = pass_first_write_texture(pass, self.name(), "scene_normal");
        let normal_rt =
            require_render_target(resources, normal_handle, self.name(), "scene_normal");
        let velocity_handle = pass_nth_write_texture(pass, 1, self.name(), "scene_velocity");
        let velocity_rt =
            require_render_target(resources, velocity_handle, self.name(), "scene_velocity");
        let Some(depth_output) = pass.depth_stencil.as_ref() else {
            return Ok(());
        };
        let depth_rt = require_render_target(resources, depth_output.handle, self.name(), "depth");

        let color_attachments = [
            Some(wgpu::RenderPassColorAttachment {
                view: normal_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.5,
                        g: 0.5,
                        b: 1.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: velocity_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: SCENE_VELOCITY_CLEAR[0] as f64,
                        g: SCENE_VELOCITY_CLEAR[1] as f64,
                        b: SCENE_VELOCITY_CLEAR[2] as f64,
                        a: SCENE_VELOCITY_CLEAR[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            }),
        ];
        let depth_attachment = Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth_rt.view(),
            depth_ops: Some(wgpu::Operations {
                load: match depth_output.clear_depth {
                    Some(value) => wgpu::LoadOp::Clear(value),
                    None => wgpu::LoadOp::Load,
                },
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        });

        let model_matrices = execution
            .frame_payload::<Vec<[f32; 16]>>()
            .map(std::vec::Vec::as_slice);
        let previous_model_matrices = execution
            .frame_payload::<PreviousModelMatrices>()
            .map(|matrices| matrices.0.as_slice());
        let device = gpu.device().clone();
        let previous_view_proj = scene_view.temporal.previous_view_proj;
        self.ensure_previous_view_proj_resources(&device);
        gpu.queue().write_buffer(
            self.prev_view_proj_buffer
                .as_ref()
                .expect("previous-view-proj buffer should exist"),
            0,
            bytemuck::bytes_of(&PreviousViewProjUniform {
                prev_view_proj: previous_view_proj,
            }),
        );
        let mut frame = gpu.frame();
        let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: depth_attachment,
            ..Default::default()
        });

        let mut cursor = 0usize;
        while cursor < phase_items.len() {
            let base = *phase_items[cursor].data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let sub_mesh_index = base.sub_mesh_index();
            let mut batch_end = cursor + 1;
            while batch_end < phase_items.len() {
                let next = *phase_items[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle || next.sub_mesh_index() != sub_mesh_index {
                    break;
                }
                batch_end += 1;
            }

            let Some(mesh) = mesh_registry.get(mesh_handle) else {
                cursor = batch_end;
                continue;
            };
            let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                cursor = batch_end;
                continue;
            };
            let Some(pipeline) = self.ensure_pipeline(
                &device,
                gpu_scene.view_bind_group_layout(),
                mesh.vertex_layout(),
                normal_rt.format(),
                velocity_rt.format(),
                depth_rt.format(),
            ) else {
                cursor = batch_end;
                continue;
            };

            let models: Vec<[f32; 16]> = phase_items[cursor..batch_end]
                .iter()
                .map(|item| {
                    let draw = *item.data::<MeshDrawData>();
                    model_matrices
                        .and_then(|matrices| matrices.get(draw.model_slot() as usize))
                        .cloned()
                        .unwrap_or(IDENTITY_MATRIX)
                })
                .collect();
            let previous_models: Vec<[f32; 16]> = phase_items[cursor..batch_end]
                .iter()
                .map(|item| {
                    let draw = *item.data::<MeshDrawData>();
                    previous_model_matrices
                        .and_then(|matrices| matrices.get(draw.model_slot() as usize))
                        .cloned()
                        .unwrap_or_else(|| {
                            model_matrices
                                .and_then(|matrices| matrices.get(draw.model_slot() as usize))
                                .cloned()
                                .unwrap_or(IDENTITY_MATRIX)
                        })
                })
                .collect();
            let instance_buffer =
                normal_prepass_instance_buffer(&device, &models, &previous_models);

            render_pass.set_pipeline(pipeline.as_ref());
            render_pass.set_bind_group(0, gpu_scene.view_bind_group(), &[]);
            render_pass.set_bind_group(
                1,
                self.prev_view_proj_bind_group
                    .as_ref()
                    .expect("previous-view-proj bind group should exist"),
                &[],
            );
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            render_pass.set_vertex_buffer(1, instance_buffer.slice(..));

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
                        .expect("indexed mesh should have format"),
                );
                render_pass.draw_indexed(
                    index_offset..(index_offset + index_count),
                    sub_mesh.vertex_offset,
                    0..models.len() as u32,
                );
            } else {
                render_pass.draw(0..mesh.vertex_count(), 0..models.len() as u32);
            }

            cursor = batch_end;
        }

        Ok(())
    }
}
