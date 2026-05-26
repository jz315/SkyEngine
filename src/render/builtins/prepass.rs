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
use crate::render::phase::{
    MeshDrawData, OpaquePhase, SceneMaterialPrepassContext, SceneMaterialPrepassPipelineCache,
};
use crate::render::pipeline::RenderPhase;
use crate::render::resources::mesh::{VertexAttribute, VertexLayout, VertexSemantic};
use crate::render::runtime::PreviousModelMatrices;
use crate::render::{SceneView, DEFAULT_DEPTH_FORMAT};

const SCENE_NORMAL_FORMAT: wgpu::TextureFormat = SceneTexture::Normal.modern_3d_format();
const SCENE_ALBEDO_FORMAT: wgpu::TextureFormat = SceneTexture::Albedo.modern_3d_format();
const SCENE_MATERIAL_FORMAT: wgpu::TextureFormat = SceneTexture::Material.modern_3d_format();
const SCENE_EMISSIVE_FORMAT: wgpu::TextureFormat = SceneTexture::Emissive.modern_3d_format();
const SCENE_VELOCITY_FORMAT: wgpu::TextureFormat = SceneTexture::Velocity.modern_3d_format();
const SCENE_VELOCITY_CLEAR: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const SCENE_MATERIAL_VELOCITY_CLEAR_PASS: &str = "scene_material_prepass_velocity_clear";
const SCENE_NORMAL_SHADER: &str = include_str!("../shaders/prepass/scene_normal_prepass.wgsl");

const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct NormalPrepassInstance {
    model_col0: [f32; 4],
    model_col1: [f32; 4],
    model_col2: [f32; 4],
    model_col3: [f32; 4],
    prev_model_col0: [f32; 4],
    prev_model_col1: [f32; 4],
    prev_model_col2: [f32; 4],
    prev_model_col3: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PreviousViewProjUniform {
    prev_view_proj: [f32; 16],
}

impl NormalPrepassInstance {
    #[inline]
    fn from_models(model: [f32; 16], previous: [f32; 16]) -> Self {
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

#[derive(Default)]
pub struct SceneMaterialPrepass {
    pipeline_cache: SceneMaterialPrepassPipelineCache,
}

impl SceneMaterialPrepass {
    fn is_view_enabled(view: &PreparedView<'_>) -> bool {
        !view
            .payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
            && view
                .payload::<OpaquePhase>()
                .is_some_and(|phase| !phase.is_empty())
    }
}

impl RenderPhase for SceneMaterialPrepass {
    fn name(&self) -> &'static str {
        "scene_material_prepass"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        Self::is_view_enabled(view)
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        if !Self::is_view_enabled(ctx.view()) {
            return;
        }

        let target_size = ctx.view().target_size();
        let (
            albedo,
            material,
            emissive,
            normal,
            velocity,
            depth,
            existing_depth,
            existing_normal,
            existing_velocity,
        ) = {
            let (graph, state) = ctx.graph_and_state();
            let albedo = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Albedo,
                SCENE_ALBEDO_FORMAT,
                "scene_albedo",
            );
            let material = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Material,
                SCENE_MATERIAL_FORMAT,
                "scene_material",
            );
            let emissive = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Emissive,
                SCENE_EMISSIVE_FORMAT,
                "scene_emissive",
            );
            let existing_normal = state.scene_normal().is_some();
            let normal = ensure_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Normal,
                SCENE_NORMAL_FORMAT,
                "scene_normal",
            );
            let existing_velocity = state.scene_velocity().is_some();
            let velocity = ensure_scene_texture(
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
            (
                albedo,
                material,
                emissive,
                normal,
                velocity,
                depth,
                existing_depth,
                existing_normal,
                existing_velocity,
            )
        };

        if !existing_velocity {
            ctx.graph()
                .add_render_pass(SCENE_MATERIAL_VELOCITY_CLEAR_PASS, |setup| {
                    setup.write_color_cleared(0, velocity.handle(), SCENE_VELOCITY_CLEAR);
                });
        }

        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_cleared(0, albedo.handle(), [0.0, 0.0, 0.0, 0.0]);
            setup.write_color_cleared(1, material.handle(), [1.0, 0.0, 1.0, 0.0]);
            setup.write_color_cleared(2, emissive.handle(), [0.0, 0.0, 0.0, 0.0]);
            if existing_normal {
                setup.write_color_loaded(3, normal.handle());
            } else {
                setup.write_color_cleared(3, normal.handle(), [0.5, 0.5, 1.0, 1.0]);
            }
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
        let (draw_functions, materials, mesh_registry, _fallback_texture) = draw_services.split();
        if pass.name == SCENE_MATERIAL_VELOCITY_CLEAR_PASS {
            let velocity_handle = pass_first_write_texture(pass, self.name(), "scene_velocity");
            let velocity_rt =
                require_render_target(resources, velocity_handle, self.name(), "scene_velocity");
            let mut frame = gpu.frame();
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
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
            })];
            let _clear_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(SCENE_MATERIAL_VELOCITY_CLEAR_PASS),
                color_attachments: &color_attachments,
                ..Default::default()
            });
            return Ok(());
        }

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

        let albedo_handle = pass_first_write_texture(pass, self.name(), "scene_albedo");
        let material_handle = pass_nth_write_texture(pass, 1, self.name(), "scene_material");
        let emissive_handle = pass_nth_write_texture(pass, 2, self.name(), "scene_emissive");
        let normal_handle = pass_nth_write_texture(pass, 3, self.name(), "scene_normal");
        let albedo_rt =
            require_render_target(resources, albedo_handle, self.name(), "scene_albedo");
        let material_rt =
            require_render_target(resources, material_handle, self.name(), "scene_material");
        let emissive_rt =
            require_render_target(resources, emissive_handle, self.name(), "scene_emissive");
        let normal_rt =
            require_render_target(resources, normal_handle, self.name(), "scene_normal");
        let Some(depth_output) = pass.depth_stencil.as_ref() else {
            return Ok(());
        };
        let depth_rt = require_render_target(resources, depth_output.handle, self.name(), "depth");

        let color_load = |index: usize| match pass.color_outputs[index].load {
            crate::render::graph::LoadOp::Clear([r, g, b, a]) => wgpu::LoadOp::Clear(wgpu::Color {
                r: r as f64,
                g: g as f64,
                b: b as f64,
                a: a as f64,
            }),
            crate::render::graph::LoadOp::Load => wgpu::LoadOp::Load,
            crate::render::graph::LoadOp::DontCare => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        };
        let color_attachments = [
            Some(wgpu::RenderPassColorAttachment {
                view: albedo_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(0),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: material_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(1),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: emissive_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(2),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: normal_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(3),
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
        let device = gpu.device().clone();
        let mut frame = gpu.frame();
        let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: depth_attachment,
            ..Default::default()
        });
        let mut prepass_ctx = SceneMaterialPrepassContext::new(
            &device,
            &mut render_pass,
            gpu_scene.view_bind_group(),
            gpu_scene.view_bind_group_layout(),
            model_matrices,
            materials,
            mesh_registry,
            &mut self.pipeline_cache,
            albedo_rt.format(),
            material_rt.format(),
            emissive_rt.format(),
            normal_rt.format(),
            depth_rt.format(),
        );
        let mut cursor = 0usize;
        while cursor < phase_items.len() {
            let draw_function_id = phase_items[cursor].draw_function_id;
            let mut batch_end = cursor + 1;
            while batch_end < phase_items.len()
                && phase_items[batch_end].draw_function_id == draw_function_id
            {
                batch_end += 1;
            }
            if draw_functions.supports_scene_material_prepass(draw_function_id) {
                draw_functions
                    .draw_scene_material_prepass_batch(
                        draw_function_id,
                        &mut prepass_ctx,
                        &phase_items[cursor..batch_end],
                    )
                    .map_err(|error| RenderGraphError::ExecutionFailed(error.to_string()))?;
            }
            cursor = batch_end;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EntityId;
    use crate::render::execution::{PhaseState, TextureFormat};
    use crate::render::graph::{LoadOp, RenderGraph, ResourceRef, TargetSize};
    use crate::render::phase::{DrawFunctionId, MeshDrawData, PhaseItem};
    use crate::render::view::{Projection, ProjectionViewUniformExt, ViewportRect};

    fn test_scene_view() -> SceneView {
        let target_size = [64, 64];
        let projection = Projection::orthographic_fixed(64.0, 64.0);
        let transform = crate::render::Transform::default();
        let view_uniform = projection.view_uniform(transform, target_size);
        SceneView::new(
            0,
            ViewportRect::new(0, 0, target_size[0], target_size[1]),
            target_size,
            false,
            u32::MAX,
            transform,
            projection,
            view_uniform,
            true,
        )
    }

    fn test_opaque_phase() -> OpaquePhase {
        let mut phase = OpaquePhase::new();
        phase.add_item(PhaseItem::new(
            0,
            DrawFunctionId::from_raw(0),
            EntityId::new(1, 0),
            0,
            MeshDrawData::default(),
        ));
        phase
    }

    fn setup_normal_prepass(graph: &mut RenderGraph, state: &mut PhaseState) {
        let scene_view = test_scene_view();
        let opaque = test_opaque_phase();
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let mut view = PreparedView::new(0, scene_view.viewport, scene_view.target_size, false);
        let _ = view.insert_payload(&scene_view);
        let _ = view.insert_payload(&opaque);
        let mut pass = SceneNormalPrepass::default();
        let mut ctx = PhaseSetupContext::new(graph, state, &frame, &view);
        pass.setup(&mut ctx);
    }

    fn setup_material_prepass(graph: &mut RenderGraph, state: &mut PhaseState) {
        let scene_view = test_scene_view();
        let opaque = test_opaque_phase();
        let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let mut view = PreparedView::new(0, scene_view.viewport, scene_view.target_size, false);
        let _ = view.insert_payload(&scene_view);
        let _ = view.insert_payload(&opaque);
        let mut pass = SceneMaterialPrepass::default();
        let mut ctx = PhaseSetupContext::new(graph, state, &frame, &view);
        pass.setup(&mut ctx);
    }

    fn keep_velocity_alive(
        graph: &mut RenderGraph,
        velocity: crate::render::execution::TextureSlot,
    ) {
        let sink = graph.create_texture(|builder| {
            builder
                .name("velocity_test_sink")
                .size(TargetSize::Exact(64, 64))
                .format(velocity.format())
                .persistent();
        });
        graph.add_render_pass("velocity_test_sink", |setup| {
            setup.read(velocity.handle());
            setup.write_color(0, sink);
        });
    }

    #[test]
    fn modern_3d_material_prepass_publishes_all_gbuffer_slots() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);

        setup_material_prepass(&mut graph, &mut state);

        for texture in [
            SceneTexture::Depth,
            SceneTexture::Normal,
            SceneTexture::Velocity,
            SceneTexture::Albedo,
            SceneTexture::Material,
            SceneTexture::Emissive,
        ] {
            let slot = state
                .scene_texture(texture)
                .unwrap_or_else(|| panic!("material prepass should publish {}", texture.label()));
            assert_eq!(slot.format(), texture.modern_3d_format());
        }

        let material = state
            .scene_material()
            .expect("material prepass should publish material target");
        let sink = graph.create_texture(|builder| {
            builder
                .name("material_contract_sink")
                .size(TargetSize::Exact(64, 64))
                .format(material.format())
                .persistent();
        });
        graph.add_render_pass("material_contract_sink", |setup| {
            setup.read(material.handle());
            setup.write_color(0, sink);
        });
        let compiled = graph.compile().expect("material graph should compile");
        let material_pass = compiled
            .iter()
            .find(|pass| pass.name == "scene_material_prepass")
            .expect("material prepass should stay alive");
        assert_eq!(
            material_pass.color_outputs[1].load,
            LoadOp::Clear([1.0, 0.0, 1.0, 0.0])
        );
    }

    #[test]
    fn scene_material_prepass_writes_velocity() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);

        setup_material_prepass(&mut graph, &mut state);

        let velocity = state
            .scene_velocity()
            .expect("material prepass should publish scene velocity");
        assert_eq!(velocity.format(), SCENE_VELOCITY_FORMAT);
        keep_velocity_alive(&mut graph, velocity);
        let compiled = graph.compile().expect("velocity graph should compile");
        let clear_pass = compiled
            .iter()
            .find(|pass| pass.name == SCENE_MATERIAL_VELOCITY_CLEAR_PASS)
            .expect("material prepass should clear velocity when no normal prepass wrote it");
        assert_eq!(
            clear_pass.color_outputs[0].target,
            ResourceRef::Texture(velocity.handle())
        );
        assert_eq!(
            clear_pass.color_outputs[0].load,
            LoadOp::Clear(SCENE_VELOCITY_CLEAR)
        );
    }

    #[test]
    fn scene_material_prepass_preserves_existing_velocity() {
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);

        setup_normal_prepass(&mut graph, &mut state);
        let velocity = state
            .scene_velocity()
            .expect("normal prepass should publish scene velocity");
        setup_material_prepass(&mut graph, &mut state);

        assert_eq!(
            state
                .scene_velocity()
                .expect("material prepass should keep scene velocity")
                .handle(),
            velocity.handle()
        );
        keep_velocity_alive(&mut graph, velocity);
        let compiled = graph.compile().expect("velocity graph should compile");
        assert!(
            compiled
                .iter()
                .all(|pass| pass.name != SCENE_MATERIAL_VELOCITY_CLEAR_PASS),
            "material prepass should not overwrite motion vectors from normal prepass"
        );
    }

    #[test]
    fn static_mesh_velocity_is_zero() {
        let instance = NormalPrepassInstance::from_models(IDENTITY_MATRIX, IDENTITY_MATRIX);
        let current = clip_uv(transform_instance_position(
            [
                instance.model_col0,
                instance.model_col1,
                instance.model_col2,
                instance.model_col3,
            ],
            [0.0, 0.0, 0.0],
        ));
        let previous = clip_uv(transform_instance_position(
            [
                instance.prev_model_col0,
                instance.prev_model_col1,
                instance.prev_model_col2,
                instance.prev_model_col3,
            ],
            [0.0, 0.0, 0.0],
        ));

        assert_eq!(
            [previous[0] - current[0], previous[1] - current[1]],
            [0.0, 0.0]
        );
        assert_eq!(SCENE_VELOCITY_CLEAR, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn moving_mesh_velocity_is_nonzero() {
        let current_model = translated_model(1.0, 0.0, 0.0);
        let instance = NormalPrepassInstance::from_models(current_model, IDENTITY_MATRIX);
        let current = clip_uv(transform_instance_position(
            [
                instance.model_col0,
                instance.model_col1,
                instance.model_col2,
                instance.model_col3,
            ],
            [0.0, 0.0, 0.0],
        ));
        let previous = clip_uv(transform_instance_position(
            [
                instance.prev_model_col0,
                instance.prev_model_col1,
                instance.prev_model_col2,
                instance.prev_model_col3,
            ],
            [0.0, 0.0, 0.0],
        ));
        let velocity = [previous[0] - current[0], previous[1] - current[1]];

        assert!(velocity[0].abs() > 0.0 || velocity[1].abs() > 0.0);
    }

    fn translated_model(x: f32, y: f32, z: f32) -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            x, y, z, 1.0,
        ]
    }

    fn transform_instance_position(cols: [[f32; 4]; 4], position: [f32; 3]) -> [f32; 4] {
        let [x, y, z] = position;
        [
            cols[0][0] * x + cols[1][0] * y + cols[2][0] * z + cols[3][0],
            cols[0][1] * x + cols[1][1] * y + cols[2][1] * z + cols[3][1],
            cols[0][2] * x + cols[1][2] * y + cols[2][2] * z + cols[3][2],
            cols[0][3] * x + cols[1][3] * y + cols[2][3] * z + cols[3][3],
        ]
    }

    fn clip_uv(clip: [f32; 4]) -> [f32; 2] {
        let inv_w = clip[3].recip();
        let ndc = [clip[0] * inv_w, clip[1] * inv_w];
        [ndc[0] * 0.5 + 0.5, ndc[1] * -0.5 + 0.5]
    }
}
