//! Lightweight material building blocks for reusable pipelines, property
//! buffers, and resource bindings.
//!
//! [`MaterialProperties`] owns a typed uniform buffer.
//! [`MaterialResourceBindings`] owns a bind-group layout and can build a
//! runtime bind group from texture/sampler/buffer resources.
//! [`MaterialPipelineCache`] owns a shader plus per-format render pipelines.
//! [`MaterialInstance`] combines properties and resource bindings into a
//! reusable material instance.

use std::borrow::Cow;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::gpu::GpuContext;

// ── Errors ─────────────────────────────────────────────────────────────────

/// Errors returned by fallible material APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialError {
    DuplicateProperty {
        name: String,
    },
    MissingProperty {
        name: String,
    },
    PropertyTypeMismatch {
        name: String,
        expected: PropertyType,
        actual: PropertyType,
    },
    DuplicateBinding {
        binding: u32,
    },
    ResourceCountMismatch {
        expected: usize,
        actual: usize,
    },
    MissingBindGroup,
    MissingPropertiesLayout,
    MissingResourceLayout,
    ConflictingBindGroupSlot {
        slot: u32,
    },
    OccupiedBindGroupSlot {
        slot: u32,
    },
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateProperty { name } => {
                write!(f, "Duplicate material property \"{name}\"")
            }
            Self::MissingProperty { name } => {
                write!(f, "Unknown material property \"{name}\"")
            }
            Self::PropertyTypeMismatch {
                name,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Material property \"{name}\" has type {actual:?}, expected {expected:?}"
                )
            }
            Self::DuplicateBinding { binding } => {
                write!(f, "Duplicate material binding index {binding}")
            }
            Self::ResourceCountMismatch { expected, actual } => {
                write!(
                    f,
                    "Material binding resource count mismatch: expected {expected}, got {actual}"
                )
            }
            Self::MissingBindGroup => {
                write!(f, "Material resource bind group has not been created")
            }
            Self::MissingPropertiesLayout => {
                write!(
                    f,
                    "Material properties slot was requested without a properties layout"
                )
            }
            Self::MissingResourceLayout => {
                write!(
                    f,
                    "Material resources slot was requested without a resource layout"
                )
            }
            Self::ConflictingBindGroupSlot { slot } => {
                write!(
                    f,
                    "Material properties and resources both use bind group slot {slot}"
                )
            }
            Self::OccupiedBindGroupSlot { slot } => {
                write!(f, "Bind group slot {slot} is already reserved")
            }
        }
    }
}

impl std::error::Error for MaterialError {}

// ── Property types ──────────────────────────────────────────────────────────

/// Supported property value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    Float,
    Vec2,
    Vec3,
    Vec4,
}

impl PropertyType {
    #[inline]
    const fn size(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Vec2 => 8,
            Self::Vec3 => 12,
            Self::Vec4 => 16,
        }
    }

    #[inline]
    const fn align(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Vec2 => 8,
            Self::Vec3 | Self::Vec4 => 16,
        }
    }
}

// ── Property layout ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct PropertySlot {
    offset: usize,
    ty: PropertyType,
}

fn build_property_layout(
    properties: &[(&str, PropertyType)],
) -> Result<(FxHashMap<Cow<'static, str>, PropertySlot>, Vec<u8>), MaterialError> {
    let mut slots = FxHashMap::default();
    let mut offset = 0usize;

    for (name, ty) in properties {
        let key: Cow<'static, str> = Cow::Owned((*name).to_string());
        if slots.contains_key(key.as_ref()) {
            return Err(MaterialError::DuplicateProperty {
                name: (*name).to_string(),
            });
        }

        let align = ty.align();
        offset = (offset + align - 1) & !(align - 1);
        slots.insert(key, PropertySlot { offset, ty: *ty });
        offset += ty.size();
    }

    let total = (offset + 15) & !15;
    Ok((slots, vec![0u8; total.max(16)]))
}

/// A typed uniform buffer with named float/vector properties.
pub struct MaterialProperties {
    slots: FxHashMap<Cow<'static, str>, PropertySlot>,
    data: Vec<u8>,
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    dirty: bool,
}

impl MaterialProperties {
    pub fn new(ctx: &GpuContext, properties: &[(&str, PropertyType)]) -> Self {
        Self::try_new(ctx, properties).expect("MaterialProperties::new failed")
    }

    pub fn try_new(
        ctx: &GpuContext,
        properties: &[(&str, PropertyType)],
    ) -> Result<Self, MaterialError> {
        let (slots, data) = build_property_layout(properties)?;

        let buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("material_props"),
            size: data.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("material_props_bgl"),
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

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material_props_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            slots,
            data,
            buffer,
            bind_group_layout,
            bind_group,
            dirty: true,
        })
    }

    fn property_slot(
        &self,
        name: &str,
        expected: PropertyType,
    ) -> Result<PropertySlot, MaterialError> {
        let slot = self
            .slots
            .get(name)
            .copied()
            .ok_or_else(|| MaterialError::MissingProperty {
                name: name.to_string(),
            })?;

        if slot.ty != expected {
            return Err(MaterialError::PropertyTypeMismatch {
                name: name.to_string(),
                expected,
                actual: slot.ty,
            });
        }

        Ok(slot)
    }

    pub fn try_set_float(&mut self, name: &str, value: f32) -> Result<(), MaterialError> {
        let slot = self.property_slot(name, PropertyType::Float)?;
        self.data[slot.offset..slot.offset + 4].copy_from_slice(&value.to_le_bytes());
        self.dirty = true;
        Ok(())
    }

    pub fn set_float(&mut self, name: &str, value: f32) {
        self.try_set_float(name, value)
            .expect("MaterialProperties::set_float failed");
    }

    pub fn try_set_vec2(&mut self, name: &str, value: [f32; 2]) -> Result<(), MaterialError> {
        let slot = self.property_slot(name, PropertyType::Vec2)?;
        self.data[slot.offset..slot.offset + 8].copy_from_slice(bytemuck::bytes_of(&value));
        self.dirty = true;
        Ok(())
    }

    pub fn set_vec2(&mut self, name: &str, value: [f32; 2]) {
        self.try_set_vec2(name, value)
            .expect("MaterialProperties::set_vec2 failed");
    }

    pub fn try_set_vec3(&mut self, name: &str, value: [f32; 3]) -> Result<(), MaterialError> {
        let slot = self.property_slot(name, PropertyType::Vec3)?;
        self.data[slot.offset..slot.offset + 12].copy_from_slice(bytemuck::bytes_of(&value));
        self.dirty = true;
        Ok(())
    }

    pub fn set_vec3(&mut self, name: &str, value: [f32; 3]) {
        self.try_set_vec3(name, value)
            .expect("MaterialProperties::set_vec3 failed");
    }

    pub fn try_set_vec4(&mut self, name: &str, value: [f32; 4]) -> Result<(), MaterialError> {
        let slot = self.property_slot(name, PropertyType::Vec4)?;
        self.data[slot.offset..slot.offset + 16].copy_from_slice(bytemuck::bytes_of(&value));
        self.dirty = true;
        Ok(())
    }

    pub fn set_vec4(&mut self, name: &str, value: [f32; 4]) {
        self.try_set_vec4(name, value)
            .expect("MaterialProperties::set_vec4 failed");
    }

    pub fn try_get_float(&self, name: &str) -> Result<f32, MaterialError> {
        let slot = self.property_slot(name, PropertyType::Float)?;
        Ok(f32::from_le_bytes(
            self.data[slot.offset..slot.offset + 4]
                .try_into()
                .expect("validated float property length"),
        ))
    }

    pub fn get_float(&self, name: &str) -> Option<f32> {
        self.try_get_float(name).ok()
    }

    pub fn try_get_vec4(&self, name: &str) -> Result<[f32; 4], MaterialError> {
        let slot = self.property_slot(name, PropertyType::Vec4)?;
        Ok(*bytemuck::from_bytes::<[f32; 4]>(
            &self.data[slot.offset..slot.offset + 16],
        ))
    }

    pub fn get_vec4(&self, name: &str) -> Option<[f32; 4]> {
        self.try_get_vec4(name).ok()
    }

    pub fn upload(&mut self, ctx: &GpuContext) {
        if self.dirty {
            ctx.queue().write_buffer(&self.buffer, 0, &self.data);
            self.dirty = false;
        }
    }

    #[inline]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    #[inline]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

// ── Material binding layout ────────────────────────────────────────────────

#[derive(Clone)]
pub struct MaterialBindingLayout {
    pub binding: u32,
    pub name: Cow<'static, str>,
    pub ty: wgpu::BindingType,
    pub visibility: wgpu::ShaderStages,
}

impl MaterialBindingLayout {
    pub fn new(
        binding: u32,
        name: impl Into<Cow<'static, str>>,
        ty: wgpu::BindingType,
        visibility: wgpu::ShaderStages,
    ) -> Self {
        Self {
            binding,
            name: name.into(),
            ty,
            visibility,
        }
    }
}

fn validate_binding_layouts(layouts: &[MaterialBindingLayout]) -> Result<(), MaterialError> {
    let mut seen = FxHashMap::default();
    for layout in layouts {
        if seen.insert(layout.binding, ()).is_some() {
            return Err(MaterialError::DuplicateBinding {
                binding: layout.binding,
            });
        }
    }
    Ok(())
}

// ── Material resource bindings ─────────────────────────────────────────────

/// Stores resource references (textures, samplers, buffers) for a material.
pub struct MaterialResourceBindings {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
    label: Cow<'static, str>,
    layouts: Vec<MaterialBindingLayout>,
}

impl MaterialResourceBindings {
    pub fn new(
        ctx: &GpuContext,
        label: impl Into<Cow<'static, str>>,
        layouts: &[MaterialBindingLayout],
    ) -> Self {
        Self::try_new(ctx, label, layouts).expect("MaterialResourceBindings::new failed")
    }

    pub fn try_new(
        ctx: &GpuContext,
        label: impl Into<Cow<'static, str>>,
        layouts: &[MaterialBindingLayout],
    ) -> Result<Self, MaterialError> {
        validate_binding_layouts(layouts)?;

        let label = label.into();
        let entries: Vec<wgpu::BindGroupLayoutEntry> = layouts
            .iter()
            .map(|layout| wgpu::BindGroupLayoutEntry {
                binding: layout.binding,
                visibility: layout.visibility,
                ty: layout.ty,
                count: None,
            })
            .collect();

        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("{}_bgl", label)),
                    entries: &entries,
                });

        Ok(Self {
            bind_group_layout,
            bind_group: None,
            label,
            layouts: layouts.to_vec(),
        })
    }

    pub fn set_resources(
        &mut self,
        ctx: &GpuContext,
        resources: &[wgpu::BindingResource<'_>],
    ) -> Result<(), MaterialError> {
        if resources.len() != self.layouts.len() {
            return Err(MaterialError::ResourceCountMismatch {
                expected: self.layouts.len(),
                actual: resources.len(),
            });
        }

        let entries: Vec<wgpu::BindGroupEntry<'_>> = self
            .layouts
            .iter()
            .zip(resources.iter())
            .map(|(layout, resource)| wgpu::BindGroupEntry {
                binding: layout.binding,
                resource: resource.clone(),
            })
            .collect();

        self.bind_group = Some(ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{}_bg", self.label)),
            layout: &self.bind_group_layout,
            entries: &entries,
        }));

        Ok(())
    }

    #[inline]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    #[inline]
    pub fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.bind_group.as_ref()
    }

    pub fn try_bind_group(&self) -> Result<&wgpu::BindGroup, MaterialError> {
        self.bind_group
            .as_ref()
            .ok_or(MaterialError::MissingBindGroup)
    }

    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[inline]
    pub fn layouts(&self) -> &[MaterialBindingLayout] {
        &self.layouts
    }
}

// ── Material instance ──────────────────────────────────────────────────────

pub struct MaterialInstance {
    properties: MaterialProperties,
    bindings: MaterialResourceBindings,
}

impl MaterialInstance {
    pub fn new(properties: MaterialProperties, bindings: MaterialResourceBindings) -> Self {
        Self {
            properties,
            bindings,
        }
    }

    pub fn upload(&mut self, ctx: &GpuContext) {
        self.properties.upload(ctx);
    }

    #[inline]
    pub fn property_bind_group(&self) -> &wgpu::BindGroup {
        self.properties.bind_group()
    }

    pub fn try_resource_bind_group(&self) -> Result<&wgpu::BindGroup, MaterialError> {
        self.bindings.try_bind_group()
    }

    pub fn properties(&self) -> &MaterialProperties {
        &self.properties
    }

    pub fn properties_mut(&mut self) -> &mut MaterialProperties {
        &mut self.properties
    }

    pub fn bindings(&self) -> &MaterialResourceBindings {
        &self.bindings
    }

    pub fn bindings_mut(&mut self) -> &mut MaterialResourceBindings {
        &mut self.bindings
    }
}

// ── Material pipeline cache ────────────────────────────────────────────────

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
        Some(match property_slot {
            Some(slot) => slot,
            None => next_free_slot(),
        })
    } else {
        None
    };

    let resolved_resource_slot = if has_resource_layout {
        Some(match resource_slot {
            Some(slot) => slot,
            None => next_free_slot(),
        })
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

        Self::try_new_inner(ctx, desc, fixed_layouts, properties_layout, bindings_layout)
    }

    fn try_new_inner(
        ctx: &GpuContext,
        desc: MaterialPipelineDesc,
        fixed_layouts: &[(u32, &wgpu::BindGroupLayout)],
        properties_layout: Option<&wgpu::BindGroupLayout>,
        bindings_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Result<Self, MaterialError> {
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

    pub fn try_pipeline(
        &mut self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> Result<&wgpu::RenderPipeline, MaterialError> {
        if !self.pipelines.contains_key(&target_format) {
            let pipeline = Arc::new(self.create_pipeline(ctx, target_format));
            self.pipelines.insert(target_format, pipeline);
        }
        Ok(self
            .pipelines
            .get(&target_format)
            .expect("pipeline inserted for requested target format")
            .as_ref())
    }

    pub fn pipeline(
        &mut self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        self.try_pipeline(ctx, target_format)
            .expect("MaterialPipelineCache::pipeline failed")
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

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_layout_std140_alignment() {
        let slots = [
            ("a", PropertyType::Float),
            ("b", PropertyType::Vec3),
            ("c", PropertyType::Vec4),
        ];

        let mut offset = 0usize;
        let mut computed = Vec::new();
        for (name, ty) in &slots {
            let align = ty.align();
            offset = (offset + align - 1) & !(align - 1);
            computed.push((*name, offset));
            offset += ty.size();
        }

        assert_eq!(computed[0], ("a", 0));
        assert_eq!(computed[1], ("b", 16));
        assert_eq!(computed[2], ("c", 32));
    }

    #[test]
    fn duplicate_property_names_are_rejected() {
        let err = build_property_layout(&[
            ("roughness", PropertyType::Float),
            ("roughness", PropertyType::Vec4),
        ])
        .unwrap_err();

        assert!(matches!(
            err,
            MaterialError::DuplicateProperty { ref name } if name == "roughness"
        ));
    }

    #[test]
    fn duplicate_binding_indices_are_rejected() {
        let err = validate_binding_layouts(&[
            MaterialBindingLayout::new(
                0,
                "albedo",
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                wgpu::ShaderStages::FRAGMENT,
            ),
            MaterialBindingLayout::new(
                0,
                "normal",
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                wgpu::ShaderStages::FRAGMENT,
            ),
        ])
        .unwrap_err();

        assert!(matches!(
            err,
            MaterialError::DuplicateBinding { binding: 0 }
        ));
    }

    #[test]
    fn resolve_bind_group_slots_assigns_defaults() {
        let (prop_slot, res_slot, max_slot) =
            resolve_bind_group_slots(None, true, None, true, &[]).unwrap();

        assert_eq!(prop_slot, Some(0));
        assert_eq!(res_slot, Some(1));
        assert_eq!(max_slot, Some(1));
    }

    #[test]
    fn resolve_bind_group_slots_avoids_collisions() {
        let (prop_slot, res_slot, max_slot) =
            resolve_bind_group_slots(None, true, Some(0), true, &[]).unwrap();

        assert_eq!(prop_slot, Some(1));
        assert_eq!(res_slot, Some(0));
        assert_eq!(max_slot, Some(1));
    }

    #[test]
    fn resolve_bind_group_slots_rejects_conflicts() {
        let err = resolve_bind_group_slots(Some(2), true, Some(2), true, &[]).unwrap_err();

        assert!(matches!(
            err,
            MaterialError::ConflictingBindGroupSlot { slot: 2 }
        ));
    }

    #[test]
    fn resolve_bind_group_slots_skips_reserved_slots() {
        let (prop_slot, res_slot, max_slot) =
            resolve_bind_group_slots(None, true, None, true, &[0]).unwrap();

        assert_eq!(prop_slot, Some(1));
        assert_eq!(res_slot, Some(2));
        assert_eq!(max_slot, Some(2));
    }

    #[test]
    fn resolve_bind_group_slots_rejects_reserved_slot_conflicts() {
        let err = resolve_bind_group_slots(Some(0), true, None, false, &[0]).unwrap_err();

        assert!(matches!(
            err,
            MaterialError::OccupiedBindGroupSlot { slot: 0 }
        ));
    }
}
