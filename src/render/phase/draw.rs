use std::any::TypeId;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::ecs::EntityId;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::gpu::GpuScene;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::Texture;
use crate::render::lighting::shadow::SceneShadowResources;
use crate::render::resources::material::{
    Material, MaterialError, MaterialRegistry, SceneBindingKind,
};
use crate::render::resources::mesh::{MeshHandle, MeshRegistry, VertexLayout};
use crate::render::view::ResolvedSceneTransforms;
use wgpu::util::DeviceExt;

use super::{MeshDrawData, PhaseItem, SpriteDrawData};

#[cfg_attr(not(test), allow(dead_code))]
pub fn create_model_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("draw_model_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: Some(
                    std::num::NonZeroU64::new(std::mem::size_of::<[f32; 16]>() as u64)
                        .expect("ModelUniform has non-zero size"),
                ),
            },
            count: None,
        }],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrawFunctionId(usize);

impl DrawFunctionId {
    #[inline]
    pub const fn index(self) -> usize {
        self.0
    }

    #[inline]
    pub(crate) const fn from_raw(index: usize) -> Self {
        Self(index)
    }
}

#[derive(Debug)]
pub enum DrawError {
    MissingDrawFunction {
        id: DrawFunctionId,
    },
    MissingMaterial {
        type_name: &'static str,
    },
    MissingMesh {
        handle: MeshHandle,
    },
    InvalidSubMeshIndex {
        mesh: String,
        sub_mesh_index: u32,
    },
    MissingIndexBuffer {
        mesh: String,
        sub_mesh_index: u32,
    },
    MissingFramePayload {
        type_name: &'static str,
    },
    MissingViewPayload {
        type_name: &'static str,
    },
    MissingPreparedFrame {
        entity: crate::ecs::EntityId,
    },
    MissingSceneBinding {
        type_name: &'static str,
        kind: SceneBindingKind,
    },
    Material(MaterialError),
}

impl std::fmt::Display for DrawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDrawFunction { id } => {
                write!(f, "Draw function {:?} has not been registered", id)
            }
            Self::MissingMaterial { type_name } => {
                write!(f, "Material handle did not resolve to `{type_name}`")
            }
            Self::MissingMesh { handle } => {
                write!(
                    f,
                    "Mesh handle {:?} did not resolve to a registered mesh",
                    handle
                )
            }
            Self::InvalidSubMeshIndex {
                mesh,
                sub_mesh_index,
            } => write!(
                f,
                "Mesh `{mesh}` does not contain sub-mesh index {sub_mesh_index}"
            ),
            Self::MissingIndexBuffer {
                mesh,
                sub_mesh_index,
            } => write!(
                f,
                "Mesh `{mesh}` sub-mesh {sub_mesh_index} requires an index buffer"
            ),
            Self::MissingFramePayload { type_name } => {
                write!(f, "Standalone draw requires frame payload `{type_name}`")
            }
            Self::MissingViewPayload { type_name } => {
                write!(f, "Standalone draw requires view payload `{type_name}`")
            }
            Self::MissingPreparedFrame { entity } => {
                write!(
                    f,
                    "Standalone draw could not resolve prepared frame for entity {entity:?}"
                )
            }
            Self::MissingSceneBinding { type_name, kind } => {
                write!(
                    f,
                    "Material `{type_name}` requires scene binding `{kind:?}` but it is unavailable"
                )
            }
            Self::Material(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DrawError {}

impl From<MaterialError> for DrawError {
    fn from(value: MaterialError) -> Self {
        Self::Material(value)
    }
}

pub struct DrawContext<'ctx, 'pass, 'tex> {
    device: &'ctx wgpu::Device,
    sampler_linear: &'ctx wgpu::Sampler,
    sampler_nearest: &'ctx wgpu::Sampler,
    pass: &'ctx mut wgpu::RenderPass<'pass>,
    view_bind_group: &'ctx wgpu::BindGroup,
    view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    #[allow(dead_code)]
    model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
    gpu_scene: Option<&'ctx GpuScene>,
    scene_shadows: Option<SceneShadowResources>,
    material_registry: &'ctx mut MaterialRegistry,
    mesh_registry: &'ctx MeshRegistry,
    fallback_texture: Option<&'tex Texture>,
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
}

impl<'ctx, 'pass, 'tex> DrawContext<'ctx, 'pass, 'tex> {
    #[inline]
    pub(crate) fn new(
        device: &'ctx wgpu::Device,
        sampler_linear: &'ctx wgpu::Sampler,
        sampler_nearest: &'ctx wgpu::Sampler,
        pass: &'ctx mut wgpu::RenderPass<'pass>,
        view_bind_group: &'ctx wgpu::BindGroup,
        view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
        gpu_scene: Option<&'ctx GpuScene>,
        scene_shadows: Option<SceneShadowResources>,
        material_registry: &'ctx mut MaterialRegistry,
        mesh_registry: &'ctx MeshRegistry,
        fallback_texture: Option<&'tex Texture>,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        Self {
            device,
            sampler_linear,
            sampler_nearest,
            pass,
            view_bind_group,
            view_bind_group_layout,
            model_bind_group_layout,
            cpu_model_matrices,
            gpu_scene,
            scene_shadows,
            material_registry,
            mesh_registry,
            fallback_texture,
            target_format,
            depth_format,
        }
    }

    #[inline]
    pub fn gpu_scene(&self) -> Option<&GpuScene> {
        self.gpu_scene
    }

    #[inline]
    pub fn cpu_model_matrix(&self, slot: u32) -> Option<&[f32; 16]> {
        self.cpu_model_matrices
            .and_then(|matrices| matrices.get(slot as usize))
    }

    #[inline]
    pub(crate) fn device(&self) -> &wgpu::Device {
        self.device
    }

    #[inline]
    pub(crate) fn sampler_nearest(&self) -> &wgpu::Sampler {
        self.sampler_nearest
    }

    #[inline]
    pub(crate) fn pass(&mut self) -> &mut wgpu::RenderPass<'pass> {
        self.pass
    }

    #[inline]
    pub(crate) fn view_bind_group(&self) -> &wgpu::BindGroup {
        self.view_bind_group
    }

    #[inline]
    pub(crate) fn view_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.view_bind_group_layout
    }

    #[inline]
    pub(crate) fn model_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.model_bind_group_layout
    }

    #[inline]
    pub(crate) fn mesh_registry(&self) -> &MeshRegistry {
        self.mesh_registry
    }

    #[inline]
    pub(crate) fn fallback_texture(&self) -> Option<&Texture> {
        self.fallback_texture
    }

    #[inline]
    pub(crate) fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    #[inline]
    pub(crate) fn depth_format(&self) -> Option<wgpu::TextureFormat> {
        self.depth_format
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshPhaseInstance {
    model_col0: [f32; 4],
    model_col1: [f32; 4],
    model_col2: [f32; 4],
    model_col3: [f32; 4],
}

impl MeshPhaseInstance {
    #[inline]
    fn from_model(model: [f32; 16]) -> Self {
        Self {
            model_col0: [model[0], model[1], model[2], model[3]],
            model_col1: [model[4], model[5], model[6], model[7]],
            model_col2: [model[8], model[9], model[10], model[11]],
            model_col3: [model[12], model[13], model[14], model[15]],
        }
    }
}

fn mesh_phase_instance_buffer(
    device: &wgpu::Device,
    label: &'static str,
    models: &[[f32; 16]],
) -> wgpu::Buffer {
    let instances: Vec<MeshPhaseInstance> = models
        .iter()
        .copied()
        .map(MeshPhaseInstance::from_model)
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

fn mesh_phase_instance_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        8 => Float32x4,
        9 => Float32x4,
        10 => Float32x4,
        11 => Float32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<MeshPhaseInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}

fn resolve_scene_prepass_vertex_attributes(
    required_layout: &VertexLayout,
    mesh_layout: &VertexLayout,
) -> Result<Vec<wgpu::VertexAttribute>, MaterialError> {
    required_layout
        .attributes()
        .iter()
        .enumerate()
        .map(|(shader_location, required)| {
            let actual = mesh_layout
                .attributes()
                .iter()
                .find(|candidate| candidate.semantic == required.semantic)
                .ok_or(MaterialError::MissingVertexAttribute {
                    semantic: required.semantic,
                })?;
            if actual.format != required.format {
                return Err(MaterialError::VertexAttributeFormatMismatch {
                    semantic: required.semantic,
                    expected: required.format,
                    actual: actual.format,
                });
            }
            Ok(wgpu::VertexAttribute {
                format: actual.format,
                offset: actual.offset as u64,
                shader_location: shader_location as u32,
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SceneMaterialPipelineKey {
    material_pipeline: u64,
    mesh_layout: u64,
    albedo_format: wgpu::TextureFormat,
    material_format: wgpu::TextureFormat,
    emissive_format: wgpu::TextureFormat,
    normal_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    view_layout_ptr: usize,
    material_layout_ptr: usize,
}

#[derive(Default)]
pub(crate) struct SceneMaterialPrepassPipelineCache {
    shaders: FxHashMap<u64, Arc<wgpu::ShaderModule>>,
    pipelines: FxHashMap<SceneMaterialPipelineKey, Arc<wgpu::RenderPipeline>>,
}

impl SceneMaterialPrepassPipelineCache {
    fn ensure_shader(
        &mut self,
        device: &wgpu::Device,
        shader_source: &crate::render::resources::material::ShaderSource,
    ) -> Arc<wgpu::ShaderModule> {
        let mut hasher = rustc_hash::FxHasher::default();
        shader_source.hash(&mut hasher);
        let key = hasher.finish();
        if let Some(shader) = self.shaders.get(&key) {
            return shader.clone();
        }
        let shader = Arc::new(device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene_material_prepass_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.wgsl_source().into()),
        }));
        self.shaders.insert(key, shader.clone());
        shader
    }

    fn ensure_pipeline<M: Material>(
        &mut self,
        device: &wgpu::Device,
        material: &M,
        view_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        mesh_layout: &VertexLayout,
        albedo_format: wgpu::TextureFormat,
        material_format: wgpu::TextureFormat,
        emissive_format: wgpu::TextureFormat,
        normal_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Result<Option<Arc<wgpu::RenderPipeline>>, MaterialError> {
        let Some(material_pipeline) = material.scene_prepass_pipeline_key() else {
            return Ok(None);
        };
        let mut mesh_hasher = rustc_hash::FxHasher::default();
        mesh_layout.hash(&mut mesh_hasher);
        let key = SceneMaterialPipelineKey {
            material_pipeline,
            mesh_layout: mesh_hasher.finish(),
            albedo_format,
            material_format,
            emissive_format,
            normal_format,
            depth_format,
            view_layout_ptr: std::ptr::from_ref(view_layout) as usize,
            material_layout_ptr: std::ptr::from_ref(material_layout) as usize,
        };
        if let Some(pipeline) = self.pipelines.get(&key) {
            return Ok(Some(pipeline.clone()));
        }

        let Some(shader_source) = material.scene_prepass_shader_source() else {
            return Ok(None);
        };
        let attributes = resolve_scene_prepass_vertex_attributes(
            &material.scene_prepass_vertex_layout(),
            mesh_layout,
        )?;
        let shader = self.ensure_shader(device, &shader_source);
        let vertex_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: mesh_layout.stride() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            },
            mesh_phase_instance_layout(),
        ];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene_material_prepass_layout"),
            bind_group_layouts: &[view_layout, material_layout],
            push_constant_ranges: &[],
        });
        let render_state = material.render_state();
        let pipeline = Arc::new(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scene_material_prepass_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(material.scene_prepass_vertex_entry()),
                    buffers: &vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(material.scene_prepass_fragment_entry()),
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: albedo_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(wgpu::ColorTargetState {
                            format: material_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(wgpu::ColorTargetState {
                            format: emissive_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(wgpu::ColorTargetState {
                            format: normal_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: render_state.cull_mode,
                    polygon_mode: render_state.polygon_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth_format,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            }),
        );
        self.pipelines.insert(key, pipeline.clone());
        Ok(Some(pipeline))
    }
}

pub(crate) struct SceneMaterialPrepassContext<'ctx, 'pass, 'tex> {
    device: &'ctx wgpu::Device,
    sampler_linear: &'ctx wgpu::Sampler,
    sampler_nearest: &'ctx wgpu::Sampler,
    pass: &'ctx mut wgpu::RenderPass<'pass>,
    view_bind_group: &'ctx wgpu::BindGroup,
    view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
    material_registry: &'ctx mut MaterialRegistry,
    mesh_registry: &'ctx MeshRegistry,
    fallback_texture: Option<&'tex Texture>,
    pipeline_cache: &'ctx mut SceneMaterialPrepassPipelineCache,
    albedo_format: wgpu::TextureFormat,
    material_format: wgpu::TextureFormat,
    emissive_format: wgpu::TextureFormat,
    normal_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
}

impl<'ctx, 'pass, 'tex> SceneMaterialPrepassContext<'ctx, 'pass, 'tex> {
    #[inline]
    pub(crate) fn new(
        device: &'ctx wgpu::Device,
        sampler_linear: &'ctx wgpu::Sampler,
        sampler_nearest: &'ctx wgpu::Sampler,
        pass: &'ctx mut wgpu::RenderPass<'pass>,
        view_bind_group: &'ctx wgpu::BindGroup,
        view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
        material_registry: &'ctx mut MaterialRegistry,
        mesh_registry: &'ctx MeshRegistry,
        fallback_texture: Option<&'tex Texture>,
        pipeline_cache: &'ctx mut SceneMaterialPrepassPipelineCache,
        albedo_format: wgpu::TextureFormat,
        material_format: wgpu::TextureFormat,
        emissive_format: wgpu::TextureFormat,
        normal_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            device,
            sampler_linear,
            sampler_nearest,
            pass,
            view_bind_group,
            view_bind_group_layout,
            cpu_model_matrices,
            material_registry,
            mesh_registry,
            fallback_texture,
            pipeline_cache,
            albedo_format,
            material_format,
            emissive_format,
            normal_format,
            depth_format,
        }
    }
}

pub struct StandaloneDrawContext<'ctx, 'frame, 'tex> {
    gpu: &'ctx mut crate::gpu::GpuContext,
    target: &'ctx RenderTarget,
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
    device: &'ctx wgpu::Device,
    sampler_linear: &'ctx wgpu::Sampler,
    sampler_nearest: &'ctx wgpu::Sampler,
    view_bind_group: &'ctx wgpu::BindGroup,
    view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    material_registry: &'ctx mut MaterialRegistry,
    fallback_texture: Option<&'tex Texture>,
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
}

impl<'ctx, 'frame, 'tex> StandaloneDrawContext<'ctx, 'frame, 'tex> {
    #[inline]
    pub fn new(
        gpu: &'ctx mut crate::gpu::GpuContext,
        target: &'ctx RenderTarget,
        frame: &'frame PreparedFrame<'frame>,
        view: &'frame PreparedView<'frame>,
        device: &'ctx wgpu::Device,
        sampler_linear: &'ctx wgpu::Sampler,
        sampler_nearest: &'ctx wgpu::Sampler,
        view_bind_group: &'ctx wgpu::BindGroup,
        view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        material_registry: &'ctx mut MaterialRegistry,
        fallback_texture: Option<&'tex Texture>,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        Self {
            gpu,
            target,
            frame,
            view,
            device,
            sampler_linear,
            sampler_nearest,
            view_bind_group,
            view_bind_group_layout,
            model_bind_group_layout,
            material_registry,
            fallback_texture,
            target_format,
            depth_format,
        }
    }

    #[inline]
    pub fn gpu(&mut self) -> &mut crate::gpu::GpuContext {
        self.gpu
    }

    pub fn gpu_and_target(&mut self) -> (&mut crate::gpu::GpuContext, &RenderTarget) {
        let gpu = self.gpu as *mut crate::gpu::GpuContext;
        let target = self.target as *const RenderTarget;
        // These references point at disjoint fields stored inside the context.
        unsafe { (&mut *gpu, &*target) }
    }

    #[inline]
    pub fn target(&self) -> &RenderTarget {
        self.target
    }

    #[inline]
    pub fn frame_payload<T: std::any::Any>(&self) -> Option<&'frame T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: std::any::Any>(&self) -> Option<&'frame T> {
        self.view.payload::<T>()
    }

    #[inline]
    pub fn material_registry(&mut self) -> &mut MaterialRegistry {
        self.material_registry
    }

    #[inline]
    pub fn device(&self) -> &wgpu::Device {
        self.device
    }

    #[inline]
    pub fn sampler_linear(&self) -> &wgpu::Sampler {
        self.sampler_linear
    }

    #[inline]
    pub fn sampler_nearest(&self) -> &wgpu::Sampler {
        self.sampler_nearest
    }

    #[inline]
    pub fn view_bind_group(&self) -> &wgpu::BindGroup {
        self.view_bind_group
    }

    #[inline]
    pub fn view_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.view_bind_group_layout
    }

    #[inline]
    pub fn model_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.model_bind_group_layout
    }

    #[inline]
    pub fn fallback_texture(&self) -> Option<&Texture> {
        self.fallback_texture
    }

    #[inline]
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    #[inline]
    pub fn depth_format(&self) -> Option<wgpu::TextureFormat> {
        self.depth_format
    }
}

#[allow(private_interfaces)]
pub trait DrawFunction: Send {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError>;

    #[inline]
    fn draw_batch(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        for item in items {
            self.draw(ctx, item)?;
        }
        Ok(())
    }

    #[inline]
    fn assign_model_matrix(
        &mut self,
        _item: &mut PhaseItem,
        _transforms: &ResolvedSceneTransforms,
        _entity_to_slot: &mut FxHashMap<EntityId, u32>,
        _model_matrices: &mut Vec<[f32; 16]>,
    ) {
    }

    #[inline]
    fn is_standalone(&self) -> bool {
        false
    }

    #[inline]
    fn draw_standalone(
        &mut self,
        _ctx: &mut StandaloneDrawContext<'_, '_, '_>,
        _item: &PhaseItem,
    ) -> Result<(), DrawError> {
        unreachable!("draw_standalone called for non-standalone draw function")
    }

    #[inline]
    fn draw_call_count(&self, items: &[PhaseItem]) -> usize {
        items.len()
    }

    #[inline]
    fn material_type_id(&self) -> Option<TypeId> {
        None
    }

    #[inline]
    fn supports_scene_material_prepass(&self) -> bool {
        false
    }

    #[inline]
    fn draw_scene_material_prepass_batch(
        &mut self,
        _ctx: &mut SceneMaterialPrepassContext<'_, '_, '_>,
        _items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct DrawFunctionRegistry {
    functions: Vec<Box<dyn DrawFunction>>,
}

impl DrawFunctionRegistry {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<D: DrawFunction + 'static>(&mut self, draw_function: D) -> DrawFunctionId {
        let id = DrawFunctionId(self.functions.len());
        self.functions.push(Box::new(draw_function));
        id
    }

    pub fn register_boxed(&mut self, draw_function: Box<dyn DrawFunction>) -> DrawFunctionId {
        let id = DrawFunctionId(self.functions.len());
        self.functions.push(draw_function);
        id
    }

    pub fn draw(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut DrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw(ctx, item)
    }

    pub fn draw_batch(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut DrawContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw_batch(ctx, items)
    }

    pub fn draw_standalone(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut StandaloneDrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw_standalone(ctx, item)
    }

    pub fn is_standalone(&self, id: DrawFunctionId) -> bool {
        self.functions
            .get(id.index())
            .is_some_and(|function| function.is_standalone())
    }

    pub fn draw_call_count(&self, id: DrawFunctionId, items: &[PhaseItem]) -> usize {
        self.functions
            .get(id.index())
            .map_or(items.len(), |function| function.draw_call_count(items))
    }

    pub fn material_type_id(&self, id: DrawFunctionId) -> Option<TypeId> {
        self.functions
            .get(id.index())
            .and_then(|function| function.material_type_id())
    }

    pub fn supports_scene_material_prepass(&self, id: DrawFunctionId) -> bool {
        self.functions
            .get(id.index())
            .is_some_and(|function| function.supports_scene_material_prepass())
    }

    #[allow(private_interfaces)]
    pub fn draw_scene_material_prepass_batch(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut SceneMaterialPrepassContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw_scene_material_prepass_batch(ctx, items)
    }

    pub fn assign_model_matrices(
        &mut self,
        items: &mut [PhaseItem],
        transforms: &ResolvedSceneTransforms,
        entity_to_slot: &mut FxHashMap<EntityId, u32>,
        model_matrices: &mut Vec<[f32; 16]>,
    ) {
        for item in items {
            let Some(function) = self.functions.get_mut(item.draw_function_id.index()) else {
                continue;
            };
            function.assign_model_matrix(item, transforms, entity_to_slot, model_matrices);
        }
    }
}

pub struct DrawMesh<M> {
    marker: PhantomData<fn() -> M>,
}

impl<M> DrawMesh<M> {
    #[inline]
    pub fn new() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<M> Default for DrawMesh<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> DrawFunction for DrawMesh<M>
where
    M: Material,
{
    #[inline]
    fn material_type_id(&self) -> Option<TypeId> {
        Some(TypeId::of::<M>())
    }

    #[inline]
    fn supports_scene_material_prepass(&self) -> bool {
        true
    }

    #[allow(private_interfaces)]
    fn draw_scene_material_prepass_batch(
        &mut self,
        ctx: &mut SceneMaterialPrepassContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        if items.is_empty() {
            return Ok(());
        }

        let Some(storage) = ctx.material_registry.try_materials::<M>() else {
            return Ok(());
        };
        let material_layout = ctx
            .material_registry
            .pipeline_cache()
            .layout::<M>()
            .ok_or(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<M>(),
            })?
            .clone();
        let bind_context = crate::render::resources::material::MaterialBindContext::new(
            ctx.device,
            ctx.sampler_linear,
            ctx.sampler_nearest,
            &material_layout,
            ctx.fallback_texture,
        );
        let mut bind_group_keepalive = Vec::new();
        let mut cursor = 0usize;

        while cursor < items.len() {
            let base = *items[cursor].data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let material_handle = base.material_handle::<M>();
            let sub_mesh_index = base.sub_mesh_index();
            let mut batch_end = cursor + 1;
            while batch_end < items.len() {
                let next = *items[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle
                    || next.material_handle::<M>() != material_handle
                    || next.sub_mesh_index() != sub_mesh_index
                {
                    break;
                }
                batch_end += 1;
            }

            let mesh = ctx
                .mesh_registry
                .get(mesh_handle)
                .ok_or(DrawError::MissingMesh {
                    handle: mesh_handle,
                })?;
            let material = storage
                .get(material_handle)
                .ok_or(DrawError::MissingMaterial {
                    type_name: std::any::type_name::<M>(),
                })?;
            let Some(pipeline) = ctx.pipeline_cache.ensure_pipeline(
                ctx.device,
                material,
                ctx.view_bind_group_layout,
                &material_layout,
                mesh.vertex_layout(),
                ctx.albedo_format,
                ctx.material_format,
                ctx.emissive_format,
                ctx.normal_format,
                ctx.depth_format,
            )?
            else {
                cursor = batch_end;
                continue;
            };
            bind_group_keepalive.push(material.create_bind_group(&bind_context));

            let models: Vec<[f32; 16]> = items[cursor..batch_end]
                .iter()
                .map(|item| {
                    let mesh_data = *item.data::<MeshDrawData>();
                    *ctx.cpu_model_matrices
                        .and_then(|matrices| matrices.get(mesh_data.model_slot() as usize))
                        .unwrap_or(&IDENTITY_MODEL)
                })
                .collect();
            let instance_buffer = mesh_phase_instance_buffer(
                ctx.device,
                "scene_material_prepass_instance_buffer",
                &models,
            );

            ctx.pass.set_pipeline(&pipeline);
            ctx.pass.set_bind_group(0, ctx.view_bind_group, &[]);
            ctx.pass.set_bind_group(
                1,
                bind_group_keepalive
                    .last()
                    .expect("scene material prepass should keep a material bind group alive"),
                &[],
            );
            ctx.pass
                .set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            ctx.pass.set_vertex_buffer(1, instance_buffer.slice(..));

            let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                return Err(DrawError::InvalidSubMeshIndex {
                    mesh: mesh.label().to_string(),
                    sub_mesh_index,
                });
            };
            if mesh.has_indices() {
                let Some(index_buffer) = mesh.index_buffer() else {
                    return Err(DrawError::MissingIndexBuffer {
                        mesh: mesh.label().to_string(),
                        sub_mesh_index,
                    });
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
                ctx.pass.set_index_buffer(
                    index_buffer.slice(..),
                    mesh.index_format()
                        .expect("indexed meshes provide an index format"),
                );
                ctx.pass.draw_indexed(
                    index_offset..(index_offset + index_count),
                    sub_mesh.vertex_offset,
                    0..models.len() as u32,
                );
            } else {
                ctx.pass
                    .draw(0..mesh.vertex_count(), 0..models.len() as u32);
            }

            cursor = batch_end;
        }

        Ok(())
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

        let (storage, pipeline_cache) =
            ctx.material_registry.materials_and_pipeline_cache::<M>()?;
        let cpu_model_matrices = ctx.cpu_model_matrices;
        let mut bind_group_keepalive = Vec::new();
        let mut cursor = 0usize;

        while cursor < items.len() {
            let base = *items[cursor].data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let material_handle = base.material_handle::<M>();
            let sub_mesh_index = base.sub_mesh_index();
            let mut batch_end = cursor + 1;
            while batch_end < items.len() {
                let next = *items[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle
                    || next.material_handle::<M>() != material_handle
                    || next.sub_mesh_index() != sub_mesh_index
                {
                    break;
                }
                batch_end += 1;
            }

            let mesh = ctx
                .mesh_registry
                .get(mesh_handle)
                .ok_or(DrawError::MissingMesh {
                    handle: mesh_handle,
                })?;
            let material = storage
                .get(material_handle)
                .ok_or(DrawError::MissingMaterial {
                    type_name: std::any::type_name::<M>(),
                })?;
            let bind_layout = pipeline_cache
                .layout::<M>()
                .ok_or(MaterialError::UnregisteredMaterialType {
                    type_name: std::any::type_name::<M>(),
                })?
                .clone();
            let scene_bindings = material.scene_bindings();

            let mut fixed_layouts = vec![(0, ctx.view_bind_group_layout)];
            if !scene_bindings.is_empty() {
                let Some(gpu_scene) = ctx.gpu_scene else {
                    return Err(DrawError::MissingSceneBinding {
                        type_name: std::any::type_name::<M>(),
                        kind: scene_bindings[0].kind,
                    });
                };
                for binding in &scene_bindings {
                    match binding.kind {
                        SceneBindingKind::GpuTable(type_id) => {
                            let Some(table) = gpu_scene.try_table_by_type_id(type_id) else {
                                return Err(DrawError::MissingSceneBinding {
                                    type_name: std::any::type_name::<M>(),
                                    kind: binding.kind,
                                });
                            };
                            fixed_layouts.push((binding.slot, table.bind_group_layout()));
                        }
                        SceneBindingKind::ShadowView => {
                            let Some(scene_shadows) = ctx.scene_shadows.as_ref() else {
                                return Err(DrawError::MissingSceneBinding {
                                    type_name: std::any::type_name::<M>(),
                                    kind: binding.kind,
                                });
                            };
                            let Some(layout) = scene_shadows.bind_group_layout() else {
                                return Err(DrawError::MissingSceneBinding {
                                    type_name: std::any::type_name::<M>(),
                                    kind: binding.kind,
                                });
                            };
                            fixed_layouts.push((binding.slot, layout));
                        }
                    }
                }
            }

            let pipeline = pipeline_cache
                .get_or_create::<M>(
                    ctx.device,
                    material,
                    mesh.vertex_layout(),
                    &fixed_layouts,
                    ctx.target_format,
                    ctx.depth_format,
                )?
                .clone();
            let bind_context = crate::render::resources::material::MaterialBindContext::new(
                ctx.device,
                ctx.sampler_linear,
                ctx.sampler_nearest,
                &bind_layout,
                ctx.fallback_texture,
            );
            bind_group_keepalive.push(material.create_bind_group(&bind_context));
            let empty_bind_group = pipeline_cache.shared_empty_bind_group(ctx.device).clone();

            let models: Vec<[f32; 16]> = items[cursor..batch_end]
                .iter()
                .map(|item| {
                    let mesh_data = *item.data::<MeshDrawData>();
                    *cpu_model_matrices
                        .and_then(|matrices| matrices.get(mesh_data.model_slot() as usize))
                        .unwrap_or(&IDENTITY_MODEL)
                })
                .collect();
            let instance_buffer =
                mesh_phase_instance_buffer(ctx.device, "mesh_phase_instance_buffer", &models);

            ctx.pass.set_pipeline(&pipeline);
            ctx.pass.set_bind_group(0, ctx.view_bind_group, &[]);
            ctx.pass.set_bind_group(
                1,
                bind_group_keepalive
                    .last()
                    .expect("mesh draw should cache current material bind group"),
                &[],
            );
            if let Some(gpu_scene) = ctx.gpu_scene {
                for binding in &scene_bindings {
                    match binding.kind {
                        SceneBindingKind::GpuTable(type_id) => {
                            let Some(table) = gpu_scene.try_table_by_type_id(type_id) else {
                                return Err(DrawError::MissingSceneBinding {
                                    type_name: std::any::type_name::<M>(),
                                    kind: binding.kind,
                                });
                            };
                            ctx.pass
                                .set_bind_group(binding.slot, table.bind_group(), &[]);
                        }
                        SceneBindingKind::ShadowView => {
                            let Some(scene_shadows) = ctx.scene_shadows.as_ref() else {
                                return Err(DrawError::MissingSceneBinding {
                                    type_name: std::any::type_name::<M>(),
                                    kind: binding.kind,
                                });
                            };
                            let Some(bind_group) = scene_shadows.bind_group() else {
                                return Err(DrawError::MissingSceneBinding {
                                    type_name: std::any::type_name::<M>(),
                                    kind: binding.kind,
                                });
                            };
                            ctx.pass.set_bind_group(binding.slot, bind_group, &[]);
                        }
                    }
                }
            }
            let occupied_slots: BTreeSet<u32> = std::iter::once(0u32)
                .chain(std::iter::once(1u32))
                .chain(scene_bindings.iter().map(|binding| binding.slot))
                .collect();
            let max_slot = occupied_slots.iter().copied().max().unwrap_or(1);
            for slot in 0..=max_slot {
                if !occupied_slots.contains(&slot) {
                    ctx.pass.set_bind_group(slot, &empty_bind_group, &[]);
                }
            }

            ctx.pass
                .set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            ctx.pass.set_vertex_buffer(1, instance_buffer.slice(..));

            let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                return Err(DrawError::InvalidSubMeshIndex {
                    mesh: mesh.label().to_string(),
                    sub_mesh_index,
                });
            };
            if mesh.has_indices() {
                let Some(index_buffer) = mesh.index_buffer() else {
                    return Err(DrawError::MissingIndexBuffer {
                        mesh: mesh.label().to_string(),
                        sub_mesh_index,
                    });
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
                ctx.pass.set_index_buffer(
                    index_buffer.slice(..),
                    mesh.index_format()
                        .expect("indexed meshes provide an index format"),
                );
                ctx.pass.draw_indexed(
                    index_offset..(index_offset + index_count),
                    sub_mesh.vertex_offset,
                    0..models.len() as u32,
                );
            } else {
                ctx.pass
                    .draw(0..mesh.vertex_count(), 0..models.len() as u32);
            }

            cursor = batch_end;
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
        item.data_mut::<MeshDrawData>().set_model_slot(slot);
    }

    fn draw_call_count(&self, items: &[PhaseItem]) -> usize {
        if items.is_empty() {
            return 0;
        }

        let mut draws = 0usize;
        let mut cursor = 0usize;
        while cursor < items.len() {
            let base = *items[cursor].data::<MeshDrawData>();
            let mut batch_end = cursor + 1;
            while batch_end < items.len() {
                let next = *items[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != base.mesh_handle()
                    || next.sub_mesh_index() != base.sub_mesh_index()
                    || next.material_handle::<M>() != base.material_handle::<M>()
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

const IDENTITY_MODEL: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

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
                bind_group_layouts: &[view_layout, &self.texture_bgl],
                push_constant_ranges: &[],
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
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
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
        use crate::render::resources::material::SpriteMaterial;

        let storage = ctx
            .material_registry
            .try_materials::<SpriteMaterial>()
            .ok_or(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<SpriteMaterial>(),
            })?;
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
        let material = storage
            .get(first_material_handle)
            .ok_or(DrawError::MissingMaterial {
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
