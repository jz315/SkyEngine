use std::any::TypeId;
use std::hash::{Hash, Hasher};

use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use crate::render::execution::{PhaseExecuteContext, PhaseSetupContext};
use crate::render::gpu::DEFAULT_DEPTH_FORMAT;
use crate::render::graph::{ImportedTexture, LoadOp, RenderGraphError, ResourceRef};
use crate::render::phase::{
    DrawFunctionRegistry, MeshDrawData, OpaquePhase, PhaseItem, TransparentPhase,
};
use crate::render::pipeline::RenderPhase;
use crate::render::resources::material::{MaterialError, MaterialHandle, MaterialRegistry};
use crate::render::resources::mesh::{VertexLayout, VertexSemantic};
use crate::render::view::SceneView;
use crate::render::StandardMaterial;

use super::view::{ShadowRasterBias, ShadowViewBinding, IDENTITY_MATRIX};
use super::{
    SceneShadowGraphResources, SceneShadowResources, ShadowPassBindingLayout,
    ShadowSceneBindingLayout, TRANSPARENT_SHADOW_FORMAT,
};

pub struct DirectionalShadowPhase {
    pipelines: FxHashMap<u64, wgpu::RenderPipeline>,
    clear_pipeline: Option<wgpu::RenderPipeline>,
    transparent_clear_pipeline: Option<wgpu::RenderPipeline>,
}

const SHADOW_ATLAS_CLEAR_SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let xy = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(xy[vertex_index], 1.0, 1.0);
}
"#;

const TRANSPARENT_SHADOW_ATLAS_CLEAR_SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let xy = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(xy[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 0.0);
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ShadowPipelineKind {
    Opaque,
    AlphaTest,
    Transparent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShadowCasterKind {
    Opaque,
    AlphaTest(MaterialHandle),
}

const TRANSPARENT_SHADOW_BLEND_STATE: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::Src,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Max,
    },
};

impl ShadowCasterKind {
    #[inline]
    const fn pipeline_kind(self) -> ShadowPipelineKind {
        match self {
            Self::Opaque => ShadowPipelineKind::Opaque,
            Self::AlphaTest(_) => ShadowPipelineKind::AlphaTest,
        }
    }
}

impl DirectionalShadowPhase {
    #[inline]
    pub fn new() -> Self {
        Self {
            pipelines: FxHashMap::default(),
            clear_pipeline: None,
            transparent_clear_pipeline: None,
        }
    }

    fn clear_pipeline_for(&mut self, device: &wgpu::Device) -> &wgpu::RenderPipeline {
        self.clear_pipeline.get_or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("directional_shadow_atlas_rect_clear_shader"),
                source: wgpu::ShaderSource::Wgsl(SHADOW_ATLAS_CLEAR_SHADER.into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("directional_shadow_atlas_rect_clear_pipeline_layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("directional_shadow_atlas_rect_clear_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: None,
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEFAULT_DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    fn transparent_clear_pipeline_for(&mut self, device: &wgpu::Device) -> &wgpu::RenderPipeline {
        self.transparent_clear_pipeline.get_or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("directional_transparent_shadow_atlas_rect_clear_shader"),
                source: wgpu::ShaderSource::Wgsl(TRANSPARENT_SHADOW_ATLAS_CLEAR_SHADER.into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("directional_transparent_shadow_atlas_rect_clear_pipeline_layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("directional_transparent_shadow_atlas_rect_clear_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TRANSPARENT_SHADOW_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEFAULT_DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    fn pipeline_for(
        &mut self,
        device: &wgpu::Device,
        shadow_layout: &wgpu::BindGroupLayout,
        material_layout: Option<&wgpu::BindGroupLayout>,
        mesh_layout: &VertexLayout,
        raster_bias: ShadowRasterBias,
        kind: ShadowPipelineKind,
    ) -> Result<&wgpu::RenderPipeline, RenderGraphError> {
        if matches!(
            kind,
            ShadowPipelineKind::AlphaTest | ShadowPipelineKind::Transparent
        ) && material_layout.is_none()
        {
            return Err(RenderGraphError::ExecutionFailed(
                "material-aware shadow pipeline requires a material bind-group layout".into(),
            ));
        }
        let mut hasher = rustc_hash::FxHasher::default();
        kind.hash(&mut hasher);
        mesh_layout.hash(&mut hasher);
        raster_bias.constant.hash(&mut hasher);
        raster_bias.slope_scale.to_bits().hash(&mut hasher);
        raster_bias.clamp.to_bits().hash(&mut hasher);
        if let Some(material_layout) = material_layout {
            (std::ptr::from_ref(material_layout) as usize).hash(&mut hasher);
        }
        let key = hasher.finish();
        if !self.pipelines.contains_key(&key) {
            let vertex_attributes = shadow_vertex_attributes(mesh_layout, kind)
                .map_err(shadow_pipeline_material_error)?;
            let (shader_label, shader_source) = match kind {
                ShadowPipelineKind::Opaque => (
                    "directional_shadow_shader",
                    include_str!("../../shaders/lighting/shadow_depth.wgsl"),
                ),
                ShadowPipelineKind::AlphaTest => (
                    "directional_shadow_alpha_test_shader",
                    include_str!("../../shaders/lighting/shadow_depth_alpha_test.wgsl"),
                ),
                ShadowPipelineKind::Transparent => (
                    "directional_transparent_shadow_shader",
                    include_str!("../../shaders/lighting/shadow_transparent.wgsl"),
                ),
            };
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(shader_label),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });
            let material_layouts = material_layout.into_iter();
            let bind_group_layouts: Vec<Option<&wgpu::BindGroupLayout>> =
                std::iter::once(shadow_layout)
                    .chain(material_layouts)
                    .map(Some)
                    .collect();
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("directional_shadow_pipeline_layout"),
                bind_group_layouts: &bind_group_layouts,
                immediate_size: 0,
            });
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
                    attributes: &vertex_attributes,
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
                fragment: match kind {
                    ShadowPipelineKind::Opaque => Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[],
                        compilation_options: Default::default(),
                    }),
                    ShadowPipelineKind::AlphaTest => Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[],
                        compilation_options: Default::default(),
                    }),
                    ShadowPipelineKind::Transparent => Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: TRANSPARENT_SHADOW_FORMAT,
                            blend: Some(TRANSPARENT_SHADOW_BLEND_STATE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: if kind == ShadowPipelineKind::Transparent {
                        None
                    } else {
                        Some(wgpu::Face::Back)
                    },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEFAULT_DEPTH_FORMAT,
                    depth_write_enabled: Some(kind != ShadowPipelineKind::Transparent),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState {
                        constant: raster_bias.constant,
                        slope_scale: raster_bias.slope_scale,
                        clamp: raster_bias.clamp,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
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

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
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
        if let Some(scene_layout) = ctx.frame_payload::<ShadowSceneBindingLayout>() {
            let _ = ctx.publish_scene_shadows(SceneShadowResources::from_directional_shadow(
                scene_layout,
                shadow_view,
            ));
        }
        let cascade_index = scene_view.shadow_cascade();
        let slot_name = format!(
            "directional_shadow_atlas_{}",
            scene_view.shadow_binding().unwrap_or(0)
        );
        let graph_texture_name = format!("{slot_name}_cascade_{cascade_index}");
        let depth_handle = ctx
            .state()
            .texture_slot(&slot_name)
            .map(|slot| slot.handle())
            .unwrap_or_else(|| {
                let handle = ctx.graph().create_texture(|builder| {
                    builder
                        .name(graph_texture_name)
                        .import_external(import_shadow_target(shadow_view));
                });
                ctx.state()
                    .set_texture_slot(slot_name, handle, shadow_view.target().format());
                handle
            });
        let transparent_slot_name = format!(
            "directional_transparent_shadow_atlas_{}",
            scene_view.shadow_binding().unwrap_or(0)
        );
        let transparent_graph_texture_name =
            format!("{transparent_slot_name}_cascade_{cascade_index}");
        let transparent_handle = ctx
            .state()
            .texture_slot(&transparent_slot_name)
            .map(|slot| slot.handle())
            .unwrap_or_else(|| {
                let handle = ctx.graph().create_texture(|builder| {
                    builder
                        .name(transparent_graph_texture_name)
                        .import_external(import_transparent_shadow_target(shadow_view));
                });
                ctx.state().set_texture_slot(
                    transparent_slot_name,
                    handle,
                    shadow_view.transparent_target().format(),
                );
                handle
            });
        if let Some(resources) = SceneShadowGraphResources::from_directional_shadow(
            shadow_view,
            depth_handle,
            transparent_handle,
        ) {
            let key =
                SceneShadowGraphResources::blackboard_key(scene_view.shadow_binding().unwrap_or(0));
            ctx.blackboard_set(key, resources);
        }
        if !shadow_view.should_update_cascade(cascade_index) {
            return;
        }
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.set_depth_stencil_loaded(depth_handle);
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, transparent_handle);
            setup.set_depth_stencil_loaded(depth_handle);
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution, draw_services) = ctx.split();
        let (draw_functions, material_registry, mesh_registry, _fallback_texture) =
            draw_services.split();
        let draw_functions = &*draw_functions;
        let material_registry = &*material_registry;
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
        if !shadow_view.should_update_cascade(scene_view.shadow_cascade()) {
            return Ok(());
        }
        let opaque_phase = execution.view_payload::<OpaquePhase>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("missing opaque phase payload".into())
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
        let standard_material_layout = material_registry
            .pipeline_cache()
            .layout::<StandardMaterial>()
            .cloned();
        let cascade_index = scene_view.shadow_cascade();
        let rect = shadow_view.atlas_layout().cascade_rect(cascade_index);
        if !pass.color_outputs.is_empty() {
            let transparent_phase =
                execution
                    .view_payload::<TransparentPhase>()
                    .ok_or_else(|| {
                        RenderGraphError::ExecutionFailed(
                            "missing transparent phase payload".into(),
                        )
                    })?;
            let color_handle = match pass.color_outputs[0].target {
                ResourceRef::Texture(handle) => handle,
                _ => {
                    return Err(RenderGraphError::ExecutionFailed(
                        "directional transparent shadow pass target must be a texture".into(),
                    ));
                }
            };
            let color_load = match pass.color_outputs[0].load {
                LoadOp::Clear(color) => wgpu::LoadOp::Clear(wgpu::Color {
                    r: color[0] as f64,
                    g: color[1] as f64,
                    b: color[2] as f64,
                    a: color[3] as f64,
                }),
                LoadOp::Load => wgpu::LoadOp::Load,
                LoadOp::DontCare => wgpu::LoadOp::Load,
            };
            let depth_load = pass
                .depth_stencil
                .as_ref()
                .and_then(|depth| depth.clear_depth)
                .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
            let depth_store = pass
                .depth_stencil
                .as_ref()
                .is_none_or(|depth| depth.depth_store);
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: resources.view(color_handle),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut frame = gpu.frame();
            let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("directional_transparent_shadow"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: resources.view(depth_handle),
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load,
                        store: if depth_store {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            render_pass.set_viewport(
                rect.x as f32,
                rect.y as f32,
                rect.width as f32,
                rect.height as f32,
                0.0,
                1.0,
            );
            render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
            {
                let clear_pipeline = self.transparent_clear_pipeline_for(&device);
                render_pass.set_pipeline(clear_pipeline);
                render_pass.draw(0..3, 0..1);
            }

            let material_layout = standard_material_layout.as_ref().ok_or_else(|| {
                RenderGraphError::ExecutionFailed(
                    "transparent shadow caster requires registered StandardMaterial layout".into(),
                )
            })?;
            if !material_registry.is_registered::<StandardMaterial>() {
                return Err(RenderGraphError::ExecutionFailed(
                    "transparent shadow caster requires registered StandardMaterial".into(),
                ));
            }
            let mut bind_group_keepalive = Vec::new();
            let mut cursor = 0usize;
            while cursor < transparent_phase.items().len() {
                let base_item = &transparent_phase.items()[cursor];
                let base = *base_item.data::<MeshDrawData>();
                let mesh_handle = base.mesh_handle();
                let sub_mesh_index = base.sub_mesh_index();
                let base_draw_function = base_item.draw_function_id;
                let base_material = transparent_shadow_material_handle(
                    base_item,
                    draw_functions,
                    material_registry,
                );
                let mut batch_end = cursor + 1;
                while batch_end < transparent_phase.items().len() {
                    let next_item = &transparent_phase.items()[batch_end];
                    let next = *next_item.data::<MeshDrawData>();
                    if next.mesh_handle() != mesh_handle
                        || next.sub_mesh_index() != sub_mesh_index
                        || next_item.draw_function_id != base_draw_function
                        || transparent_shadow_material_handle(
                            next_item,
                            draw_functions,
                            material_registry,
                        ) != base_material
                    {
                        break;
                    }
                    batch_end += 1;
                }

                let Some(material_handle) = base_material else {
                    cursor = batch_end;
                    continue;
                };
                if material_registry
                    .get_erased::<StandardMaterial>(material_handle)
                    .is_err()
                {
                    cursor = batch_end;
                    continue;
                }
                let Some(mesh) = mesh_registry.get(mesh_handle) else {
                    cursor = batch_end;
                    continue;
                };
                let pipeline = self.pipeline_for(
                    &device,
                    shadow_pass_layout.bind_group_layout(),
                    Some(material_layout),
                    mesh.vertex_layout(),
                    shadow_view.raster_bias(),
                    ShadowPipelineKind::Transparent,
                )?;
                bind_group_keepalive.push(
                    material_registry
                        .prepared(material_handle)
                        .map_err(shadow_pipeline_material_error)?
                        .bind_group()
                        .clone(),
                );
                let instances: Vec<[f32; 16]> = transparent_phase.items()[cursor..batch_end]
                    .iter()
                    .map(|item| {
                        let mesh_data = *item.data::<MeshDrawData>();
                        model_matrices
                            .get(mesh_data.model_slot() as usize)
                            .copied()
                            .unwrap_or(IDENTITY_MATRIX)
                    })
                    .collect();
                let instance_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("directional_transparent_shadow_instances"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });

                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(
                    0,
                    shadow_view.shadow_pass_bind_group(scene_view.shadow_cascade()),
                    &[],
                );
                render_pass.set_bind_group(
                    1,
                    bind_group_keepalive
                        .last()
                        .expect("transparent shadow caster should cache material bind group"),
                    &[],
                );
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

            return Ok(());
        }
        let mut frame = gpu.frame();
        let depth_load = pass
            .depth_stencil
            .as_ref()
            .and_then(|depth| depth.clear_depth)
            .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let depth_store = pass
            .depth_stencil
            .as_ref()
            .is_none_or(|depth| depth.depth_store);
        let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("directional_shadow"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.view(depth_handle),
                depth_ops: Some(wgpu::Operations {
                    load: depth_load,
                    store: if depth_store {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        render_pass.set_viewport(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
            0.0,
            1.0,
        );
        render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
        {
            let clear_pipeline = self.clear_pipeline_for(&device);
            render_pass.set_pipeline(clear_pipeline);
            render_pass.draw(0..3, 0..1);
        }
        let mut bind_group_keepalive = Vec::new();
        let mut cursor = 0usize;
        while cursor < opaque_phase.items().len() {
            let base_item = &opaque_phase.items()[cursor];
            let base = *base_item.data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let sub_mesh_index = base.sub_mesh_index();
            let caster_kind = shadow_caster_kind(base_item, draw_functions, material_registry);
            let mut batch_end = cursor + 1;
            while batch_end < opaque_phase.items().len() {
                let next_item = &opaque_phase.items()[batch_end];
                let next = *next_item.data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle
                    || next.sub_mesh_index() != sub_mesh_index
                    || shadow_caster_kind(next_item, draw_functions, material_registry)
                        != caster_kind
                {
                    break;
                }
                batch_end += 1;
            }

            let Some(mesh) = mesh_registry.get(mesh_handle) else {
                cursor = batch_end;
                continue;
            };
            let material_layout = match caster_kind {
                ShadowCasterKind::Opaque => None,
                ShadowCasterKind::AlphaTest(_) => {
                    Some(standard_material_layout.as_ref().ok_or_else(|| {
                        RenderGraphError::ExecutionFailed(
                            "alpha-test shadow caster requires registered StandardMaterial layout"
                                .into(),
                        )
                    })?)
                }
            };
            let pipeline = self.pipeline_for(
                &device,
                shadow_pass_layout.bind_group_layout(),
                material_layout,
                mesh.vertex_layout(),
                shadow_view.raster_bias(),
                caster_kind.pipeline_kind(),
            )?;
            if let ShadowCasterKind::AlphaTest(material_handle) = caster_kind {
                let _ = material_registry
                    .get_erased::<StandardMaterial>(material_handle)
                    .map_err(|_| {
                        RenderGraphError::ExecutionFailed(
                            "alpha-test shadow caster material handle no longer resolves".into(),
                        )
                    })?;
                bind_group_keepalive.push(
                    material_registry
                        .prepared(material_handle)
                        .map_err(shadow_pipeline_material_error)?
                        .bind_group()
                        .clone(),
                );
            }
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
            render_pass.set_bind_group(
                0,
                shadow_view.shadow_pass_bind_group(scene_view.shadow_cascade()),
                &[],
            );
            if matches!(caster_kind, ShadowCasterKind::AlphaTest(_)) {
                render_pass.set_bind_group(
                    1,
                    bind_group_keepalive
                        .last()
                        .expect("alpha-test shadow caster should cache material bind group"),
                    &[],
                );
            }
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
        let Some(scene_view) = execution.view_payload::<SceneView>() else {
            return 0;
        };
        if !shadow_view.should_update_cascade(scene_view.shadow_cascade()) {
            return 0;
        }

        let opaque_draws = execution
            .view_payload::<OpaquePhase>()
            .map_or(0, |phase| count_phase_shadow_batches(phase.items()));
        let transparent_draws = execution
            .view_payload::<TransparentPhase>()
            .map_or(0, |phase| count_phase_shadow_batches(phase.items()));
        opaque_draws + transparent_draws
    }
}

fn shadow_vertex_attributes(
    mesh_layout: &VertexLayout,
    kind: ShadowPipelineKind,
) -> Result<Vec<wgpu::VertexAttribute>, MaterialError> {
    let mut attributes = vec![shadow_vertex_attribute(
        mesh_layout,
        VertexSemantic::Position,
        wgpu::VertexFormat::Float32x3,
        0,
    )?];
    if matches!(
        kind,
        ShadowPipelineKind::AlphaTest | ShadowPipelineKind::Transparent
    ) {
        attributes.push(shadow_vertex_attribute(
            mesh_layout,
            VertexSemantic::UV0,
            wgpu::VertexFormat::Float32x2,
            1,
        )?);
    }
    Ok(attributes)
}

fn shadow_vertex_attribute(
    mesh_layout: &VertexLayout,
    semantic: VertexSemantic,
    expected: wgpu::VertexFormat,
    shader_location: u32,
) -> Result<wgpu::VertexAttribute, MaterialError> {
    let actual = mesh_layout
        .attributes()
        .iter()
        .find(|attribute| attribute.semantic == semantic)
        .ok_or(MaterialError::MissingVertexAttribute { semantic })?;
    if actual.format != expected {
        return Err(MaterialError::VertexAttributeFormatMismatch {
            semantic,
            expected,
            actual: actual.format,
        });
    }
    Ok(wgpu::VertexAttribute {
        format: actual.format,
        offset: actual.offset as u64,
        shader_location,
    })
}

fn shadow_caster_kind(
    item: &PhaseItem,
    draw_functions: &DrawFunctionRegistry,
    material_registry: &MaterialRegistry,
) -> ShadowCasterKind {
    if draw_functions.material_type_id(item.draw_function_id)
        != Some(TypeId::of::<StandardMaterial>())
    {
        return ShadowCasterKind::Opaque;
    }

    let draw = *item.data::<MeshDrawData>();
    let raw_material_handle = draw.material_handle::<StandardMaterial>();
    if !material_registry.is_registered::<StandardMaterial>() {
        return ShadowCasterKind::Opaque;
    }
    let Ok(material) = material_registry.get_erased::<StandardMaterial>(raw_material_handle) else {
        return ShadowCasterKind::Opaque;
    };
    let Some(model_id) = material_registry.model_id::<StandardMaterial>() else {
        return ShadowCasterKind::Opaque;
    };
    let material_handle =
        crate::render::resources::material::TypedMaterialHandle::<StandardMaterial>::new(
            model_id,
            raw_material_handle.id(),
        );
    if material.casts_alpha_test_shadow() {
        ShadowCasterKind::AlphaTest(material_handle.into())
    } else {
        ShadowCasterKind::Opaque
    }
}

fn transparent_shadow_material_handle(
    item: &PhaseItem,
    draw_functions: &DrawFunctionRegistry,
    material_registry: &MaterialRegistry,
) -> Option<MaterialHandle> {
    if draw_functions.material_type_id(item.draw_function_id)
        != Some(TypeId::of::<StandardMaterial>())
    {
        return None;
    }

    let draw = *item.data::<MeshDrawData>();
    let raw_material_handle = draw.material_handle::<StandardMaterial>();
    let material = material_registry
        .get_erased::<StandardMaterial>(raw_material_handle)
        .ok()?;
    let model_id = material_registry.model_id::<StandardMaterial>()?;
    let material_handle =
        crate::render::resources::material::TypedMaterialHandle::<StandardMaterial>::new(
            model_id,
            raw_material_handle.id(),
        );
    material
        .alpha_mode
        .is_transparent()
        .then_some(material_handle.into())
}

fn count_phase_shadow_batches(items: &[PhaseItem]) -> usize {
    let mut draws = 0usize;
    let mut cursor = 0usize;
    while cursor < items.len() {
        let base = *items[cursor].data::<MeshDrawData>();
        let base_draw_function = items[cursor].draw_function_id;
        let base_material = base.material_handle::<StandardMaterial>();
        let mut batch_end = cursor + 1;
        while batch_end < items.len() {
            let next = *items[batch_end].data::<MeshDrawData>();
            let next_draw_function = items[batch_end].draw_function_id;
            if next.mesh_handle() != base.mesh_handle()
                || next.sub_mesh_index() != base.sub_mesh_index()
                || next_draw_function != base_draw_function
                || next.material_handle::<StandardMaterial>() != base_material
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

fn shadow_pipeline_material_error(error: MaterialError) -> RenderGraphError {
    RenderGraphError::ExecutionFailed(format!("failed to build shadow pipeline: {error}"))
}

fn import_shadow_target(shadow: &ShadowViewBinding) -> ImportedTexture {
    let target = shadow.target();
    import_render_target(target)
}

fn import_transparent_shadow_target(shadow: &ShadowViewBinding) -> ImportedTexture {
    import_render_target(shadow.transparent_target())
}

fn import_render_target(target: &crate::render::gpu::RenderTarget) -> ImportedTexture {
    ImportedTexture {
        texture: std::sync::Arc::new(target.texture().clone()),
        view: std::sync::Arc::new(target.view().clone()),
        size: [target.width(), target.height()],
        format: target.format(),
        usage: target.usage(),
        sample_count: target.sample_count(),
        mip_level_count: target.mip_level_count(),
        array_layer_count: target.array_layer_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::{EntityId, World};
    use crate::gpu::GpuContext;
    use crate::render::execution::{PhaseState, PreparedFrame, PreparedView};
    use crate::render::graph::{RenderGraph, ResourceRef, TargetSize};
    use crate::render::lighting::shadow::{
        append_directional_shadow_views, create_shadow_compare_sampler, sync_shadow_views,
        ShadowResourceKind,
    };
    use crate::render::phase::{DrawFunctionId, MeshDrawData, PhaseItem};
    use crate::render::pipeline::RenderPhase;
    use crate::render::resources::mesh::VertexAttribute;
    use crate::render::view::{Projection, ViewportRect};
    use crate::render::{DirectionalLight, GpuScene, LightTable, MaterialHandle, Transform};

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for shadow phase tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("shadow_phase_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn opaque_shadow_vertex_layout_remains_position_only() {
        let layout = VertexLayout::new(
            20,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
            ],
        );

        let attributes = shadow_vertex_attributes(&layout, ShadowPipelineKind::Opaque)
            .expect("opaque shadow layout should require only position");

        assert_eq!(attributes.len(), 1);
        assert_eq!(attributes[0].shader_location, 0);
        assert_eq!(attributes[0].offset, 0);
        assert_eq!(attributes[0].format, wgpu::VertexFormat::Float32x3);
    }

    #[test]
    fn alpha_test_shadow_vertex_layout_requires_uv0() {
        let layout = VertexLayout::new(
            12,
            [VertexAttribute::new(
                VertexSemantic::Position,
                wgpu::VertexFormat::Float32x3,
                0,
            )],
        );

        let error = shadow_vertex_attributes(&layout, ShadowPipelineKind::AlphaTest)
            .expect_err("alpha-test shadow layout should require uv0");

        assert_eq!(
            error,
            MaterialError::MissingVertexAttribute {
                semantic: VertexSemantic::UV0
            }
        );
    }

    #[test]
    fn alpha_test_shadow_pipeline_compiles_with_standard_material_layout() {
        let (device, _queue) = create_test_device();
        let shadow_layout = ShadowPassBindingLayout::new(&device);
        let material_layout =
            <StandardMaterial as crate::render::resources::material::MaterialModel>::interface()
                .bindings
                .create_bind_group_layout(&device, "standard_material_shadow_test_bgl");
        let layout = VertexLayout::new(
            20,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
            ],
        );
        let mut phase = DirectionalShadowPhase::new();

        let pipeline = phase.pipeline_for(
            &device,
            shadow_layout.bind_group_layout(),
            Some(&material_layout),
            &layout,
            ShadowRasterBias::new(0, 0.0, 0.0),
            ShadowPipelineKind::AlphaTest,
        );

        assert!(pipeline.is_ok());
    }

    #[test]
    fn transparent_shadow_pipeline_compiles_with_standard_material_layout() {
        let (device, _queue) = create_test_device();
        let shadow_layout = ShadowPassBindingLayout::new(&device);
        let material_layout =
            <StandardMaterial as crate::render::resources::material::MaterialModel>::interface()
                .bindings
                .create_bind_group_layout(&device, "standard_material_shadow_test_bgl");
        let layout = VertexLayout::new(
            20,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
            ],
        );
        let mut phase = DirectionalShadowPhase::new();

        let pipeline = phase.pipeline_for(
            &device,
            shadow_layout.bind_group_layout(),
            Some(&material_layout),
            &layout,
            ShadowRasterBias::new(0, 0.0, 0.0),
            ShadowPipelineKind::Transparent,
        );

        assert!(pipeline.is_ok());
    }

    #[test]
    fn transparent_shadow_blend_state_matches_wicked() {
        let blend = TRANSPARENT_SHADOW_BLEND_STATE;

        assert_eq!(blend.color.src_factor, wgpu::BlendFactor::Zero);
        assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::Src);
        assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
        assert_eq!(blend.alpha.src_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.alpha.dst_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.alpha.operation, wgpu::BlendOperation::Max);
    }

    #[test]
    fn shadow_depth_shaders_keep_raster_and_depth_projection_separate() {
        for source in [
            include_str!("../../shaders/lighting/shadow_depth.wgsl"),
            include_str!("../../shaders/lighting/shadow_depth_alpha_test.wgsl"),
            include_str!("../../shaders/lighting/shadow_transparent.wgsl"),
        ] {
            assert!(source.contains("raster_view_proj: mat4x4<f32>"));
            assert!(source.contains("depth_view_proj: mat4x4<f32>"));
            assert!(source.contains("shadow_pass.raster_view_proj * world_position"));
            assert!(source.contains("shadow_pass.depth_view_proj * world_position"));
            assert!(
                !source.contains("clip_position.z = clamp"),
                "vertex-stage depth clamping bends shadow triangles and can stamp bands into the atlas"
            );
        }

        let opaque = include_str!("../../shaders/lighting/shadow_depth.wgsl");
        let alpha_test = include_str!("../../shaders/lighting/shadow_depth_alpha_test.wgsl");
        let transparent = include_str!("../../shaders/lighting/shadow_transparent.wgsl");
        assert!(opaque.contains("@builtin(frag_depth)"));
        assert!(alpha_test.contains("@builtin(frag_depth)"));
        assert!(transparent.contains("@builtin(frag_depth)"));
    }

    #[test]
    fn standard_mask_material_routes_to_alpha_test_shadow_caster() {
        let (device, _queue) = create_test_device();
        let mut draw_functions = DrawFunctionRegistry::new();
        let draw_mesh =
            draw_functions.register(crate::render::phase::DrawMesh::<StandardMaterial>::new());
        let mut material_registry = MaterialRegistry::new();
        material_registry
            .register_model::<StandardMaterial>(&device)
            .expect("standard material should register");
        let opaque = material_registry
            .insert_material::<StandardMaterial>(StandardMaterial::default())
            .expect("opaque material should insert");
        let mask = material_registry
            .insert_material::<StandardMaterial>(StandardMaterial::default().alpha_mask(0.35))
            .expect("mask material should insert");
        let opaque_item = PhaseItem::new(
            0,
            draw_mesh,
            EntityId::new(0, 0),
            0,
            MeshDrawData::new(crate::render::expert::Mesh::QUAD, opaque, 0),
        );
        let mask_item = PhaseItem::new(
            0,
            draw_mesh,
            EntityId::new(1, 0),
            0,
            MeshDrawData::new(crate::render::expert::Mesh::QUAD, mask, 0),
        );

        assert_eq!(
            shadow_caster_kind(&opaque_item, &draw_functions, &material_registry),
            ShadowCasterKind::Opaque
        );
        assert_eq!(
            shadow_caster_kind(&mask_item, &draw_functions, &material_registry),
            ShadowCasterKind::AlphaTest(mask.into())
        );
    }

    #[test]
    fn standard_blend_material_routes_to_transparent_shadow_caster() {
        let (device, _queue) = create_test_device();
        let mut draw_functions = DrawFunctionRegistry::new();
        let draw_mesh =
            draw_functions.register(crate::render::phase::DrawMesh::<StandardMaterial>::new());
        let mut material_registry = MaterialRegistry::new();
        material_registry
            .register_model::<StandardMaterial>(&device)
            .expect("standard material should register");
        let blend = material_registry
            .insert_material::<StandardMaterial>(
                StandardMaterial::default().alpha_mode(crate::render::AlphaMode::Blend),
            )
            .expect("blend material should insert");
        let item = PhaseItem::new(
            0,
            draw_mesh,
            EntityId::new(0, 0),
            0,
            MeshDrawData::new(crate::render::expert::Mesh::QUAD, blend, 0),
        );

        assert_eq!(
            transparent_shadow_material_handle(&item, &draw_functions, &material_registry),
            Some(blend.into())
        );
    }

    #[test]
    fn directional_shadow_phase_setup_imports_enabled_shadow_view() {
        let (device, queue) = create_test_device();
        let gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let projection = Projection::orthographic_fixed(16.0, 16.0);
        let main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([64, 64]),
            [64, 64],
            false,
            u32::MAX,
            Transform::default(),
            projection,
            projection.view_uniform(Transform::default(), [64, 64]),
            true,
        );
        let mut world = World::new();
        world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
            .shadow_map_size(64)
            .shadow_bias(0.002)
            .radius(0.04)
            .shadow_depth_bias(5)
            .shadow_slope_bias(1.25)
            .shadow_normal_bias(0.03)
            .shadow_filter_radius(0.06),));

        let mut scene_views = vec![main_view];
        let shadow_setups = append_directional_shadow_views(&world, &mut scene_views);
        assert_eq!(scene_views.len(), 2);
        assert!(scene_views[1].is_shadow());

        let mut opaque_phases = vec![OpaquePhase::new(), OpaquePhase::new()];
        opaque_phases[1].add_item(PhaseItem::new(
            0,
            DrawFunctionId::from_raw(0),
            EntityId::new(0, 0),
            0,
            MeshDrawData::new(
                crate::render::expert::Mesh::QUAD,
                MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
                0,
            ),
        ));
        let transparent_phases = vec![TransparentPhase::new(), TransparentPhase::new()];

        let mut gpu_scene = GpuScene::new(&gpu);
        gpu_scene.table_mut::<LightTable>().set_all(&gpu, &[]);
        gpu_scene.upload_all(gpu.queue());
        let scene_layout = ShadowSceneBindingLayout::new(gpu.device());
        let pass_layout = ShadowPassBindingLayout::new(gpu.device());
        let sampler = create_shadow_compare_sampler(gpu.device());
        let mut shadow_bindings = Vec::new();
        sync_shadow_views(
            &mut shadow_bindings,
            &gpu,
            &scene_views,
            &opaque_phases,
            &transparent_phases,
            &[IDENTITY_MATRIX],
            &shadow_setups,
            &scene_layout,
            &pass_layout,
            &sampler,
            gpu_scene.table::<LightTable>(),
            crate::render::component::RenderDebugView::DirectionalShadowCoverage,
        );
        assert_eq!(shadow_bindings.len(), 1);
        assert!(shadow_bindings[0].enabled());
        assert_eq!(shadow_bindings[0].radius(), 0.06);
        assert_eq!(
            shadow_bindings[0].raster_bias(),
            ShadowRasterBias::new(5, 1.25, 0.0)
        );
        assert_eq!(shadow_bindings[0].normal_bias(), 0.03);
        assert_eq!(shadow_bindings[0].debug_mode(), 1.0);

        let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
        frame.insert_payload(&scene_layout);
        let prepared_view = PreparedView::new(
            scene_views[1].order,
            scene_views[1].viewport,
            scene_views[1].target_size,
            scene_views[1].clear_surface,
        )
        .with_payload(&scene_views[1])
        .with_payload(&shadow_bindings[0]);
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(frame.surface_format(), frame.has_surface());
        let mut phase = DirectionalShadowPhase::new();

        assert!(phase.is_enabled(&frame, &prepared_view));
        {
            let mut setup = PhaseSetupContext::new(&mut graph, &mut state, &frame, &prepared_view);
            phase.setup(&mut setup);
        }

        let slot = state
            .texture_slot("directional_shadow_atlas_0")
            .expect("enabled shadow setup should publish the imported depth atlas");
        assert_eq!(slot.format(), DEFAULT_DEPTH_FORMAT);
        let transparent_slot = state
            .texture_slot("directional_transparent_shadow_atlas_0")
            .expect("enabled shadow setup should publish the imported transparent atlas");
        assert_eq!(transparent_slot.format(), TRANSPARENT_SHADOW_FORMAT);
        let graph_resources_key = SceneShadowGraphResources::blackboard_key(0);
        let graph_resources = graph
            .blackboard_ref()
            .get::<SceneShadowGraphResources>(&graph_resources_key)
            .expect("enabled shadow setup should publish graph handles for sampling passes");
        assert_eq!(graph_resources.directional_shadow_atlas(), slot.handle());
        assert_eq!(
            graph_resources.directional_transparent_shadow_atlas(),
            transparent_slot.handle()
        );
        let scene_shadows = state
            .scene_shadows()
            .expect("enabled shadow setup should publish scene shadow resources");
        assert_eq!(
            scene_shadows.kind(),
            ShadowResourceKind::DirectionalCascades
        );
        assert!(scene_shadows.enabled());
        assert!(scene_shadows.bind_group().is_some());
        assert_eq!(graph.pass_count(), 2);
        let passes = graph.compile().expect("shadow phase graph should compile");
        let depth_pass = passes
            .iter()
            .find(|pass| pass.color_outputs.is_empty())
            .expect("shadow phase should declare a depth-only pass");
        let transparent_pass = passes
            .iter()
            .find(|pass| !pass.color_outputs.is_empty())
            .expect("shadow phase should declare a transparent color pass");
        let depth = depth_pass
            .depth_stencil
            .expect("shadow pass should declare depth");
        assert!(
            depth.clear_depth.is_none(),
            "directional shadow cascades clear their own atlas rects while preserving the rest"
        );
        assert_eq!(transparent_pass.color_outputs.len(), 1);
        assert!(
            transparent_pass.depth_stencil.is_some(),
            "transparent shadow pass should depth-test against the directional atlas"
        );
    }

    #[test]
    fn opaque_phase_reads_exact_shadow_graph_handles() {
        let (device, queue) = create_test_device();
        let gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let projection = Projection::orthographic_fixed(16.0, 16.0);
        let main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([64, 64]),
            [64, 64],
            false,
            u32::MAX,
            Transform::default(),
            projection,
            projection.view_uniform(Transform::default(), [64, 64]),
            true,
        );
        let mut world = World::new();
        world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]).shadow_map_size(64),));

        let mut scene_views = vec![main_view];
        let shadow_setups = append_directional_shadow_views(&world, &mut scene_views);
        assert_eq!(scene_views.len(), 2);
        assert_eq!(scene_views[0].shadow_binding(), Some(0));
        assert!(scene_views[1].is_shadow());

        let mut opaque_phases = vec![OpaquePhase::new(), OpaquePhase::new()];
        opaque_phases[0].add_item(PhaseItem::new(
            0,
            DrawFunctionId::from_raw(0),
            EntityId::new(0, 0),
            0,
            MeshDrawData::new(
                crate::render::expert::Mesh::QUAD,
                MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
                0,
            ),
        ));
        opaque_phases[1].add_item(PhaseItem::new(
            0,
            DrawFunctionId::from_raw(0),
            EntityId::new(1, 0),
            0,
            MeshDrawData::new(
                crate::render::expert::Mesh::QUAD,
                MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
                0,
            ),
        ));
        let transparent_phases = vec![TransparentPhase::new(), TransparentPhase::new()];

        let mut gpu_scene = GpuScene::new(&gpu);
        gpu_scene.table_mut::<LightTable>().set_all(&gpu, &[]);
        gpu_scene.upload_all(gpu.queue());
        let scene_layout = ShadowSceneBindingLayout::new(gpu.device());
        let pass_layout = ShadowPassBindingLayout::new(gpu.device());
        let sampler = create_shadow_compare_sampler(gpu.device());
        let mut shadow_bindings = Vec::new();
        sync_shadow_views(
            &mut shadow_bindings,
            &gpu,
            &scene_views,
            &opaque_phases,
            &transparent_phases,
            &[IDENTITY_MATRIX],
            &shadow_setups,
            &scene_layout,
            &pass_layout,
            &sampler,
            gpu_scene.table::<LightTable>(),
            crate::render::component::RenderDebugView::None,
        );
        assert_eq!(shadow_bindings.len(), 1);
        assert!(shadow_bindings[0].enabled());

        let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
        frame.insert_payload(&scene_layout);
        let shadow_prepared_view = PreparedView::new(
            scene_views[1].order,
            scene_views[1].viewport,
            scene_views[1].target_size,
            scene_views[1].clear_surface,
        )
        .with_payload(&scene_views[1])
        .with_payload(&shadow_bindings[0]);
        let main_prepared_view = PreparedView::new(
            scene_views[0].order,
            scene_views[0].viewport,
            scene_views[0].target_size,
            scene_views[0].clear_surface,
        )
        .with_payload(&scene_views[0])
        .with_payload(&opaque_phases[0]);
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(frame.surface_format(), frame.has_surface());
        let mut shadow_phase = DirectionalShadowPhase::new();

        {
            let mut setup =
                PhaseSetupContext::new(&mut graph, &mut state, &frame, &shadow_prepared_view);
            shadow_phase.setup(&mut setup);
        }

        let graph_resources_key = SceneShadowGraphResources::blackboard_key(0);
        let graph_resources = graph
            .blackboard_ref()
            .get::<SceneShadowGraphResources>(&graph_resources_key)
            .cloned()
            .expect("shadow setup should publish graph handles before opaque setup");
        let scene_color = graph.create_texture(|builder| {
            builder
                .name("opaque_phase_shadow_dependency_color")
                .size(TargetSize::Exact(64, 64))
                .format(frame.surface_format())
                .persistent();
        });
        state.set_current_color(scene_color, frame.surface_format());

        let mut opaque_phase = OpaquePhase::new();
        {
            let mut setup =
                PhaseSetupContext::new(&mut graph, &mut state, &frame, &main_prepared_view);
            opaque_phase.setup(&mut setup);
        }

        let passes = graph
            .compile()
            .expect("shadow and opaque dependency graph should compile");
        let opaque_pass = passes
            .iter()
            .find(|pass| pass.name.as_ref() == "opaque_phase")
            .expect("opaque setup should declare a live opaque pass");
        assert!(
            opaque_pass.reads.contains(&ResourceRef::Texture(
                graph_resources.directional_shadow_atlas()
            )),
            "opaque pass must read the exact directional shadow atlas handle from shadow setup"
        );
        assert!(
            opaque_pass.reads.contains(&ResourceRef::Texture(
                graph_resources.directional_transparent_shadow_atlas()
            )),
            "opaque pass must read the exact transparent shadow atlas handle from shadow setup"
        );
    }

    #[test]
    fn static_shadow_phase_setup_skips_clean_cascade_but_keeps_resource() {
        let (device, queue) = create_test_device();
        let gpu =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 64.0);
        let main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([64, 64]),
            [64, 64],
            false,
            u32::MAX,
            Transform::default(),
            projection,
            projection.view_uniform(Transform::default(), [64, 64]),
            false,
        );
        let mut world = World::new();
        world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
            .cascade_count(2)
            .cascade_distances([16.0, 48.0, 0.0, 0.0])
            .shadow_map_size(64)
            .static_shadows_when_unchanged(),));

        let mut scene_views = vec![main_view];
        let shadow_setups = append_directional_shadow_views(&world, &mut scene_views);
        assert_eq!(scene_views.len(), 3);

        let mut opaque_phases = (0..scene_views.len())
            .map(|_| OpaquePhase::new())
            .collect::<Vec<_>>();
        let transparent_phases = (0..scene_views.len())
            .map(|_| TransparentPhase::new())
            .collect::<Vec<_>>();
        for phase in opaque_phases.iter_mut().skip(1) {
            phase.add_item(PhaseItem::new(
                0,
                DrawFunctionId::from_raw(0),
                EntityId::new(0, 0),
                0,
                MeshDrawData::new(
                    crate::render::expert::Mesh::QUAD,
                    MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
                    0,
                ),
            ));
        }

        let mut gpu_scene = GpuScene::new(&gpu);
        gpu_scene.table_mut::<LightTable>().set_all(&gpu, &[]);
        gpu_scene.upload_all(gpu.queue());
        let scene_layout = ShadowSceneBindingLayout::new(gpu.device());
        let pass_layout = ShadowPassBindingLayout::new(gpu.device());
        let sampler = create_shadow_compare_sampler(gpu.device());
        let mut shadow_bindings = Vec::new();
        sync_shadow_views(
            &mut shadow_bindings,
            &gpu,
            &scene_views,
            &opaque_phases,
            &transparent_phases,
            &[IDENTITY_MATRIX],
            &shadow_setups,
            &scene_layout,
            &pass_layout,
            &sampler,
            gpu_scene.table::<LightTable>(),
            crate::render::component::RenderDebugView::None,
        );
        assert!(shadow_bindings[0].should_update_cascade(0));
        assert!(shadow_bindings[0].should_update_cascade(1));

        sync_shadow_views(
            &mut shadow_bindings,
            &gpu,
            &scene_views,
            &opaque_phases,
            &transparent_phases,
            &[IDENTITY_MATRIX],
            &shadow_setups,
            &scene_layout,
            &pass_layout,
            &sampler,
            gpu_scene.table::<LightTable>(),
            crate::render::component::RenderDebugView::None,
        );
        assert!(!shadow_bindings[0].should_update_cascade(0));
        assert!(!shadow_bindings[0].should_update_cascade(1));

        let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
        frame.insert_payload(&scene_layout);
        let prepared_view = PreparedView::new(
            scene_views[1].order,
            scene_views[1].viewport,
            scene_views[1].target_size,
            scene_views[1].clear_surface,
        )
        .with_payload(&scene_views[1])
        .with_payload(&shadow_bindings[0]);
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(frame.surface_format(), frame.has_surface());
        let mut phase = DirectionalShadowPhase::new();

        assert!(phase.is_enabled(&frame, &prepared_view));
        {
            let mut setup = PhaseSetupContext::new(&mut graph, &mut state, &frame, &prepared_view);
            phase.setup(&mut setup);
        }

        assert_eq!(graph.pass_count(), 0);
        assert!(
            state.texture_slot("directional_shadow_atlas_0").is_some(),
            "clean static cascades still publish the reusable imported atlas"
        );
        assert!(
            state
                .texture_slot("directional_transparent_shadow_atlas_0")
                .is_some(),
            "clean static cascades still publish the reusable transparent shadow atlas"
        );
        assert!(state
            .scene_shadows()
            .expect("clean static cascades should still publish shadow resources")
            .enabled());
    }
}
