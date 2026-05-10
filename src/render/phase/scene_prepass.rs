use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::render::resources::material::{Material, MaterialError, MaterialRegistry};
use crate::render::resources::mesh::{MeshRegistry, VertexLayout};

use super::mesh_instance::mesh_phase_instance_layout;

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

    pub(super) fn ensure_pipeline<M: Material>(
        &mut self,
        device: &wgpu::Device,
        material: &M::Data,
        view_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        mesh_layout: &VertexLayout,
        albedo_format: wgpu::TextureFormat,
        material_format: wgpu::TextureFormat,
        emissive_format: wgpu::TextureFormat,
        normal_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Result<Option<Arc<wgpu::RenderPipeline>>, MaterialError> {
        let Some(material_pipeline) = M::scene_prepass_pipeline_key(material) else {
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

        let Some(shader_source) = M::scene_prepass_shader_source(material) else {
            return Ok(None);
        };
        let attributes = resolve_scene_prepass_vertex_attributes(
            &M::scene_prepass_vertex_layout(material),
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
            bind_group_layouts: &[Some(view_layout), Some(material_layout)],
            immediate_size: 0,
        });
        let render_state = M::render_state(material);
        let pipeline = Arc::new(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scene_material_prepass_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(M::scene_prepass_vertex_entry(material)),
                    buffers: &vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(M::scene_prepass_fragment_entry(material)),
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
        Ok(Some(pipeline))
    }
}

pub(crate) struct SceneMaterialPrepassContext<'ctx, 'pass> {
    pub(super) device: &'ctx wgpu::Device,
    pub(super) pass: &'ctx mut wgpu::RenderPass<'pass>,
    pub(super) view_bind_group: &'ctx wgpu::BindGroup,
    pub(super) view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    pub(super) cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
    pub(super) material_registry: &'ctx mut MaterialRegistry,
    pub(super) mesh_registry: &'ctx MeshRegistry,
    pub(super) pipeline_cache: &'ctx mut SceneMaterialPrepassPipelineCache,
    pub(super) albedo_format: wgpu::TextureFormat,
    pub(super) material_format: wgpu::TextureFormat,
    pub(super) emissive_format: wgpu::TextureFormat,
    pub(super) normal_format: wgpu::TextureFormat,
    pub(super) depth_format: wgpu::TextureFormat,
}

impl<'ctx, 'pass> SceneMaterialPrepassContext<'ctx, 'pass> {
    #[inline]
    pub(crate) fn new(
        device: &'ctx wgpu::Device,
        pass: &'ctx mut wgpu::RenderPass<'pass>,
        view_bind_group: &'ctx wgpu::BindGroup,
        view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
        material_registry: &'ctx mut MaterialRegistry,
        mesh_registry: &'ctx MeshRegistry,
        pipeline_cache: &'ctx mut SceneMaterialPrepassPipelineCache,
        albedo_format: wgpu::TextureFormat,
        material_format: wgpu::TextureFormat,
        emissive_format: wgpu::TextureFormat,
        normal_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            device,
            pass,
            view_bind_group,
            view_bind_group_layout,
            cpu_model_matrices,
            material_registry,
            mesh_registry,
            pipeline_cache,
            albedo_format,
            material_format,
            emissive_format,
            normal_format,
            depth_format,
        }
    }
}
