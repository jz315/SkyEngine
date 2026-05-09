use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::gpu::GpuContext;
use crate::render::resources::mesh::{VertexLayout, VertexSemantic};

use super::{
    MaterialError, MaterialInterface, MaterialModel, MaterialModelId, MaterialRenderState,
    ShaderVariantKey,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialPipelineKey {
    pub model: MaterialModelId,
    pub shader: u64,
    pub variant: u64,
    pub pass: u64,
    pub vertex_layout: u64,
    pub scene_layout: u64,
    pub material_layout: u64,
    pub target_format: wgpu::TextureFormat,
    pub depth_format: Option<wgpu::TextureFormat>,
    pub sample_count: u32,
    pub render_state: MaterialRenderState,
}

impl MaterialPipelineKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: MaterialModelId,
        interface: &MaterialInterface,
        variant: &ShaderVariantKey,
        pass: u64,
        mesh_layout: &VertexLayout,
        scene_layout: u64,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Self {
        Self {
            model,
            shader: fingerprint(&interface.shader),
            variant: variant.stable_hash(),
            pass,
            vertex_layout: fingerprint(mesh_layout),
            scene_layout,
            material_layout: interface.binding_layout_fingerprint(),
            target_format,
            depth_format,
            sample_count,
            render_state: interface.render_state,
        }
    }
}

/// Interface-and-variant-driven render pipeline cache.
#[derive(Default)]
pub struct PipelineCache {
    pipelines: FxHashMap<MaterialPipelineKey, wgpu::RenderPipeline>,
    layouts: FxHashMap<std::any::TypeId, wgpu::BindGroupLayout>,
    empty_layout: Option<wgpu::BindGroupLayout>,
    empty_bind_group: Option<wgpu::BindGroup>,
    current_frame: u64,
}

impl PipelineCache {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn new_frame(&mut self) {
        self.current_frame = self.current_frame.wrapping_add(1);
    }

    #[inline]
    pub fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }

    #[inline]
    pub fn layout_count(&self) -> usize {
        self.layouts.len()
    }

    pub fn register_layout<M: MaterialModel>(&mut self, device: &wgpu::Device) {
        self.layouts
            .entry(std::any::TypeId::of::<M>())
            .or_insert_with(|| {
                M::interface().bindings.create_bind_group_layout(
                    device,
                    format!("{}_material_bgl", M::interface().name),
                )
            });
    }

    pub fn layout<M: MaterialModel>(&self) -> Option<&wgpu::BindGroupLayout> {
        self.layouts.get(&std::any::TypeId::of::<M>())
    }

    fn shared_empty_layout(&mut self, device: &wgpu::Device) -> &wgpu::BindGroupLayout {
        self.empty_layout.get_or_insert_with(|| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material_pipeline_empty_bgl"),
                entries: &[],
            })
        })
    }

    pub fn shared_empty_bind_group(&mut self, device: &wgpu::Device) -> &wgpu::BindGroup {
        if self.empty_bind_group.is_none() {
            let layout = self.shared_empty_layout(device).clone();
            self.empty_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("material_pipeline_empty_bg"),
                layout: &layout,
                entries: &[],
            }));
        }
        self.empty_bind_group.as_ref().expect("empty bind group")
    }

    #[inline]
    pub fn garbage_collect(&mut self) {}

    #[inline]
    pub fn contains(&self, key: &MaterialPipelineKey) -> bool {
        self.pipelines.contains_key(key)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_or_create<M: MaterialModel>(
        &mut self,
        device: &wgpu::Device,
        material: &M::Data,
        mesh_layout: &VertexLayout,
        fixed_layouts: &[(u32, &wgpu::BindGroupLayout)],
        shader_injection: Option<&str>,
        shader_key: u64,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Result<&wgpu::RenderPipeline, MaterialError> {
        self.register_layout::<M>(device);
        let interface = M::interface();
        let variant = M::variant(
            material,
            &super::MaterialVariantContext {
                interface: &interface,
            },
        );
        let key = MaterialPipelineKey::new(
            MaterialModelId::new(0, 0),
            &interface,
            &variant,
            shader_key,
            mesh_layout,
            fingerprint(
                &fixed_layouts
                    .iter()
                    .map(|(slot, layout)| (*slot, std::ptr::from_ref(*layout) as usize))
                    .collect::<Vec<_>>(),
            ),
            target_format,
            depth_format,
            1,
        );
        if !self.pipelines.contains_key(&key) {
            let empty_layout = self.shared_empty_layout(device).clone();
            let material_layout = self
                .layout::<M>()
                .expect("material layout registered")
                .clone();
            let pipeline = compile_model_pipeline::<M>(
                device,
                &empty_layout,
                &material_layout,
                material,
                mesh_layout,
                fixed_layouts,
                shader_injection,
                target_format,
                depth_format,
            )?;
            self.pipelines.insert(key, pipeline);
        }
        Ok(self.pipelines.get(&key).expect("pipeline cached"))
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_model_pipeline<M: MaterialModel>(
    device: &wgpu::Device,
    empty_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    material: &M::Data,
    mesh_layout: &VertexLayout,
    fixed_layouts: &[(u32, &wgpu::BindGroupLayout)],
    shader_injection: Option<&str>,
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
) -> Result<wgpu::RenderPipeline, MaterialError> {
    let mut occupied_slots = FxHashSet::default();
    for (slot, _) in fixed_layouts {
        if !occupied_slots.insert(*slot) {
            return Err(MaterialError::OccupiedBindGroupSlot { slot: *slot });
        }
    }
    const MATERIAL_BIND_GROUP_SLOT: u32 = 1;
    if occupied_slots.contains(&MATERIAL_BIND_GROUP_SLOT) {
        return Err(MaterialError::OccupiedBindGroupSlot {
            slot: MATERIAL_BIND_GROUP_SLOT,
        });
    }
    let max_slot = fixed_layouts
        .iter()
        .map(|(slot, _)| *slot)
        .chain(Some(MATERIAL_BIND_GROUP_SLOT))
        .max()
        .expect("material slot exists");
    let mut bind_group_layouts = vec![empty_layout.clone(); max_slot as usize + 1];
    for (slot, layout) in fixed_layouts {
        bind_group_layouts[*slot as usize] = (*layout).clone();
    }
    bind_group_layouts[MATERIAL_BIND_GROUP_SLOT as usize] = material_layout.clone();
    let bind_group_layout_refs: Vec<&wgpu::BindGroupLayout> = bind_group_layouts.iter().collect();

    let source = M::shader_source(material);
    let wgsl = source.wgsl_source();
    let injection = shader_injection.unwrap_or(crate::render::gi::NULL_GI_SHADER);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(std::any::type_name::<M>()),
        source: wgpu::ShaderSource::Wgsl(wgsl.replace("/*GI_SHADER*/", injection).into()),
    });
    let required_layout = M::vertex_layout(material);
    let attributes = resolve_vertex_attributes(&required_layout, mesh_layout)?;
    let vertex_buffer_layout = [
        wgpu::VertexBufferLayout {
            array_stride: mesh_layout.stride() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        },
        mesh_instance_vertex_buffer_layout(),
    ];
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("material_pipeline_layout"),
        bind_group_layouts: &bind_group_layout_refs,
        push_constant_ranges: &[],
    });
    let render_state = M::render_state(material);
    Ok(
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("material_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(M::vertex_entry(material)),
                buffers: &vertex_buffer_layout,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(M::fragment_entry(material)),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: render_state.blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: render_state.cull_mode,
                polygon_mode: render_state.polygon_mode,
                ..Default::default()
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: render_state.depth_write,
                depth_compare: render_state.depth_compare,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        }),
    )
}

fn mesh_instance_vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
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
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<[f32; 16]>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

#[derive(Clone)]
pub struct MaterialPipelineDesc {
    pub label: Cow<'static, str>,
    pub shader_source: Cow<'static, str>,
    pub vs_entry: &'static str,
    pub fs_entry: &'static str,
    pub blend: Option<wgpu::BlendState>,
    pub material_properties_slot: Option<u32>,
    pub material_resources_slot: Option<u32>,
    pub vertex_buffers: Vec<wgpu::VertexBufferLayout<'static>>,
    pub primitive: wgpu::PrimitiveState,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub multisample: wgpu::MultisampleState,
    pub color_write_mask: wgpu::ColorWrites,
}

impl MaterialPipelineDesc {
    pub fn new(
        label: impl Into<Cow<'static, str>>,
        shader_source: impl Into<Cow<'static, str>>,
        vs_entry: &'static str,
        fs_entry: &'static str,
    ) -> Self {
        Self {
            label: label.into(),
            shader_source: shader_source.into(),
            vs_entry,
            fs_entry,
            blend: None,
            material_properties_slot: None,
            material_resources_slot: None,
            vertex_buffers: Vec::new(),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            color_write_mask: wgpu::ColorWrites::ALL,
        }
    }
}

fn resolve_bind_group_slots(
    property_slot: Option<u32>,
    has_properties_layout: bool,
    resource_slot: Option<u32>,
    has_resource_layout: bool,
    fixed_slots: &[u32],
) -> Result<(Option<u32>, Option<u32>, Option<u32>), MaterialError> {
    if property_slot.is_some() && !has_properties_layout {
        return Err(MaterialError::MissingPropertiesLayout);
    }
    if resource_slot.is_some() && !has_resource_layout {
        return Err(MaterialError::MissingResourceLayout);
    }
    if let (Some(prop_slot), Some(res_slot)) = (property_slot, resource_slot) {
        if has_properties_layout && has_resource_layout && prop_slot == res_slot {
            return Err(MaterialError::ConflictingBindGroupSlot { slot: prop_slot });
        }
    }

    let mut occupied: FxHashSet<u32> = fixed_slots.iter().copied().collect();

    if has_properties_layout {
        if let Some(slot) = property_slot {
            if occupied.contains(&slot) {
                return Err(MaterialError::OccupiedBindGroupSlot { slot });
            }
            occupied.insert(slot);
        }
    }
    if has_resource_layout {
        if let Some(slot) = resource_slot {
            if occupied.contains(&slot) {
                return Err(MaterialError::OccupiedBindGroupSlot { slot });
            }
            occupied.insert(slot);
        }
    }

    let mut next_free_slot = || -> u32 {
        let mut slot = 0u32;
        while occupied.contains(&slot) {
            slot += 1;
        }
        occupied.insert(slot);
        slot
    };

    let resolved_property_slot = if has_properties_layout {
        Some(property_slot.unwrap_or_else(&mut next_free_slot))
    } else {
        None
    };

    let resolved_resource_slot = if has_resource_layout {
        Some(resource_slot.unwrap_or_else(&mut next_free_slot))
    } else {
        None
    };

    if let (Some(prop_slot), Some(res_slot)) = (resolved_property_slot, resolved_resource_slot) {
        if prop_slot == res_slot {
            return Err(MaterialError::ConflictingBindGroupSlot { slot: prop_slot });
        }
    }

    let max_slot = resolved_property_slot
        .into_iter()
        .chain(resolved_resource_slot)
        .chain(fixed_slots.iter().copied())
        .max();

    Ok((resolved_property_slot, resolved_resource_slot, max_slot))
}

pub struct MaterialPipelineCache {
    shader: Arc<wgpu::ShaderModule>,
    desc: MaterialPipelineDesc,
    pipelines: FxHashMap<wgpu::TextureFormat, Arc<wgpu::RenderPipeline>>,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    resolved_property_slot: Option<u32>,
    resolved_resource_slot: Option<u32>,
}

impl MaterialPipelineCache {
    pub fn new(
        ctx: &GpuContext,
        desc: MaterialPipelineDesc,
        properties_layout: Option<&wgpu::BindGroupLayout>,
        bindings_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Self {
        Self::new_with_fixed_layouts(ctx, desc, &[], properties_layout, bindings_layout)
    }

    pub fn new_with_fixed_layouts(
        ctx: &GpuContext,
        desc: MaterialPipelineDesc,
        fixed_layouts: &[(u32, &wgpu::BindGroupLayout)],
        properties_layout: Option<&wgpu::BindGroupLayout>,
        bindings_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Self {
        Self::try_new_with_fixed_layouts(
            ctx,
            desc,
            fixed_layouts,
            properties_layout,
            bindings_layout,
        )
        .expect("MaterialPipelineCache::new_with_fixed_layouts failed")
    }

    pub fn try_new(
        ctx: &GpuContext,
        desc: MaterialPipelineDesc,
        properties_layout: Option<&wgpu::BindGroupLayout>,
        bindings_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Result<Self, MaterialError> {
        Self::try_new_with_fixed_layouts(ctx, desc, &[], properties_layout, bindings_layout)
    }

    pub fn try_new_with_fixed_layouts(
        ctx: &GpuContext,
        desc: MaterialPipelineDesc,
        fixed_layouts: &[(u32, &wgpu::BindGroupLayout)],
        properties_layout: Option<&wgpu::BindGroupLayout>,
        bindings_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Result<Self, MaterialError> {
        let mut seen_fixed_slots = FxHashSet::default();
        for (slot, _) in fixed_layouts {
            if !seen_fixed_slots.insert(*slot) {
                return Err(MaterialError::OccupiedBindGroupSlot { slot: *slot });
            }
        }

        let shader = ctx
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("{}_shader", desc.label)),
                source: wgpu::ShaderSource::Wgsl(desc.shader_source.clone()),
            });

        let fixed_slots: Vec<u32> = fixed_layouts.iter().map(|(slot, _)| *slot).collect();
        let (resolved_property_slot, resolved_resource_slot, max_slot) = resolve_bind_group_slots(
            desc.material_properties_slot,
            properties_layout.is_some(),
            desc.material_resources_slot,
            bindings_layout.is_some(),
            &fixed_slots,
        )?;

        let bind_group_layouts = if let Some(max_slot) = max_slot {
            let empty_layout =
                ctx.device()
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("material_empty_bgl"),
                        entries: &[],
                    });

            let mut layouts = vec![empty_layout; max_slot as usize + 1];
            for (slot, layout) in fixed_layouts {
                layouts[*slot as usize] = (*layout).clone();
            }
            if let (Some(slot), Some(layout)) = (resolved_property_slot, properties_layout) {
                layouts[slot as usize] = layout.clone();
            }
            if let (Some(slot), Some(layout)) = (resolved_resource_slot, bindings_layout) {
                layouts[slot as usize] = layout.clone();
            }
            layouts
        } else {
            Vec::new()
        };

        Ok(Self {
            shader: Arc::new(shader),
            desc,
            pipelines: FxHashMap::default(),
            bind_group_layouts,
            resolved_property_slot,
            resolved_resource_slot,
        })
    }

    fn create_pipeline(
        &self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let bind_group_layout_refs: Vec<&wgpu::BindGroupLayout> =
            self.bind_group_layouts.iter().collect();

        let pipeline_layout =
            ctx.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(&format!("{}_layout", self.desc.label)),
                    bind_group_layouts: &bind_group_layout_refs,
                    push_constant_ranges: &[],
                });

        ctx.device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("{}_pipeline_{target_format:?}", self.desc.label)),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: self.shader.as_ref(),
                    entry_point: Some(self.desc.vs_entry),
                    buffers: &self.desc.vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: self.shader.as_ref(),
                    entry_point: Some(self.desc.fs_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: self.desc.blend,
                        write_mask: self.desc.color_write_mask,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: self.desc.primitive,
                depth_stencil: self.desc.depth_stencil.clone(),
                multisample: self.desc.multisample,
                multiview: None,
                cache: None,
            })
    }

    pub fn try_pipeline_arc(
        &mut self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> Result<Arc<wgpu::RenderPipeline>, MaterialError> {
        if !self.pipelines.contains_key(&target_format) {
            let pipeline = Arc::new(self.create_pipeline(ctx, target_format));
            self.pipelines.insert(target_format, pipeline);
        }
        Ok(Arc::clone(
            self.pipelines
                .get(&target_format)
                .expect("pipeline inserted for requested target format"),
        ))
    }

    #[inline]
    pub fn desc(&self) -> &MaterialPipelineDesc {
        &self.desc
    }

    #[inline]
    pub fn property_slot(&self) -> Option<u32> {
        self.resolved_property_slot
    }

    #[inline]
    pub fn resource_slot(&self) -> Option<u32> {
        self.resolved_resource_slot
    }

    #[inline]
    pub fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }
}

fn fingerprint<T: Hash>(value: &T) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn resolve_vertex_attributes(
    material_layout: &VertexLayout,
    mesh_layout: &VertexLayout,
) -> Result<Vec<wgpu::VertexAttribute>, MaterialError> {
    material_layout
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

pub(crate) fn vertex_attribute(
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
