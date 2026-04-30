//! Lightweight material building blocks for reusable pipelines, property
//! buffers, and resource bindings.
//!
//! [`MaterialProperties`] owns a typed uniform buffer.
//! [`MaterialResourceBindings`] owns a bind-group layout and can build a
//! runtime bind group from texture/sampler/buffer resources.
//! [`MaterialPipelineCache`] owns a shader plus per-format render pipelines.
//! [`MaterialInstance`] combines properties and resource bindings into a
//! reusable material instance.

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;
use crate::render::gpu::Texture;
use crate::render::resources::mesh::{Mesh, VertexAttribute, VertexLayout, VertexSemantic};
use crate::render::view::Color;

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
    MissingVertexAttribute {
        semantic: VertexSemantic,
    },
    VertexAttributeFormatMismatch {
        semantic: VertexSemantic,
        expected: wgpu::VertexFormat,
        actual: wgpu::VertexFormat,
    },
    UnregisteredMaterialType {
        type_name: &'static str,
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
            Self::MissingVertexAttribute { semantic } => {
                write!(
                    f,
                    "Mesh vertex layout is missing required attribute {semantic:?}"
                )
            }
            Self::VertexAttributeFormatMismatch {
                semantic,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Mesh vertex attribute {semantic:?} uses format {actual:?}, expected {expected:?}"
                )
            }
            Self::UnregisteredMaterialType { type_name } => {
                write!(f, "Material type `{type_name}` has not been registered")
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

// ── Programmable material system ───────────────────────────────────────────

/// Shader source used by [`Material`] implementations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSource {
    Wgsl(Cow<'static, str>),
}

impl ShaderSource {
    #[inline]
    pub(crate) fn wgsl_source(&self) -> &str {
        match self {
            Self::Wgsl(source) => source.as_ref(),
        }
    }
}

/// Render-state settings contributed by a [`Material`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialRenderState {
    pub blend: Option<wgpu::BlendState>,
    pub depth_write: bool,
    pub depth_compare: wgpu::CompareFunction,
    pub cull_mode: Option<wgpu::Face>,
    pub polygon_mode: wgpu::PolygonMode,
}

impl MaterialRenderState {
    #[inline]
    pub const fn opaque() -> Self {
        Self {
            blend: None,
            depth_write: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
        }
    }

    #[inline]
    pub const fn transparent() -> Self {
        Self {
            blend: Some(Self::alpha_blend()),
            depth_write: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
        }
    }

    #[inline]
    pub const fn additive() -> Self {
        Self {
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            depth_write: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
        }
    }

    #[inline]
    const fn alpha_blend() -> wgpu::BlendState {
        wgpu::BlendState {
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
        }
    }
}

impl Default for MaterialRenderState {
    fn default() -> Self {
        Self::opaque()
    }
}

/// Runtime context passed to [`Material::create_bind_group`].
pub struct MaterialBindContext<'a> {
    device: &'a wgpu::Device,
    sampler_linear: &'a wgpu::Sampler,
    sampler_nearest: &'a wgpu::Sampler,
    layout: &'a wgpu::BindGroupLayout,
    fallback_texture: Option<&'a Texture>,
}

impl<'a> MaterialBindContext<'a> {
    #[inline]
    pub fn new(
        device: &'a wgpu::Device,
        sampler_linear: &'a wgpu::Sampler,
        sampler_nearest: &'a wgpu::Sampler,
        layout: &'a wgpu::BindGroupLayout,
        fallback_texture: Option<&'a Texture>,
    ) -> Self {
        Self {
            device,
            sampler_linear,
            sampler_nearest,
            layout,
            fallback_texture,
        }
    }

    #[inline]
    pub fn device(&self) -> &wgpu::Device {
        self.device
    }

    #[inline]
    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        self.layout
    }

    #[inline]
    pub fn fallback_texture(&self) -> Option<&Texture> {
        self.fallback_texture
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
    pub fn texture_or_fallback<'b>(&'b self, texture: Option<&'b Texture>) -> &'b Texture {
        texture
            .or(self.fallback_texture)
            .expect("material bind context requires either a material texture or a fallback")
    }
}

/// User-defined programmable material.
pub trait Material: Send + Sync + 'static {
    fn shader_source(&self) -> ShaderSource;
    fn vertex_layout(&self) -> VertexLayout;

    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout
    where
        Self: Sized;

    fn create_bind_group(&self, ctx: &MaterialBindContext<'_>) -> wgpu::BindGroup;
    fn render_state(&self) -> MaterialRenderState;

    #[inline]
    fn vertex_entry(&self) -> &'static str {
        "vs_main"
    }

    #[inline]
    fn fragment_entry(&self) -> &'static str {
        "fs_main"
    }

    #[inline]
    fn scene_prepass_shader_source(&self) -> Option<ShaderSource> {
        None
    }

    #[inline]
    fn scene_prepass_vertex_layout(&self) -> VertexLayout {
        self.vertex_layout()
    }

    #[inline]
    fn scene_prepass_vertex_entry(&self) -> &'static str {
        "vs_main"
    }

    #[inline]
    fn scene_prepass_fragment_entry(&self) -> &'static str {
        "fs_main"
    }

    fn scene_prepass_pipeline_key(&self) -> Option<u64> {
        let shader = self.scene_prepass_shader_source()?;
        let mut hasher = rustc_hash::FxHasher::default();
        shader.hash(&mut hasher);
        self.scene_prepass_vertex_layout().hash(&mut hasher);
        self.scene_prepass_vertex_entry().hash(&mut hasher);
        self.scene_prepass_fragment_entry().hash(&mut hasher);
        Some(hasher.finish())
    }

    fn pipeline_key(&self) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        self.shader_source().hash(&mut hasher);
        self.vertex_layout().hash(&mut hasher);
        self.render_state().hash(&mut hasher);
        self.vertex_entry().hash(&mut hasher);
        self.fragment_entry().hash(&mut hasher);
        hasher.finish()
    }

    #[inline]
    fn is_transparent(&self) -> bool {
        self.render_state().blend.is_some()
    }

    #[inline]
    fn scene_bindings(&self) -> Vec<SceneBindingDesc> {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneBindingKind {
    GpuTable(TypeId),
    ShadowView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneBindingDesc {
    pub slot: u32,
    pub kind: SceneBindingKind,
}

impl SceneBindingDesc {
    pub fn gpu_table<T>(slot: u32) -> Self
    where
        T: crate::render::GpuTable + 'static,
    {
        Self {
            slot,
            kind: SceneBindingKind::GpuTable(TypeId::of::<T>()),
        }
    }

    #[inline]
    pub const fn shadow_view(slot: u32) -> Self {
        Self {
            slot,
            kind: SceneBindingKind::ShadowView,
        }
    }
}

/// Type-erased material handle for ECS-facing renderer components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle {
    type_id: TypeId,
    index: u32,
    generation: u32,
}

impl MaterialHandle {
    #[inline]
    pub fn new<M: Material>(index: u32, generation: u32) -> Self {
        Self {
            type_id: TypeId::of::<M>(),
            index,
            generation,
        }
    }

    #[inline]
    pub fn index(self) -> u32 {
        self.index
    }

    #[inline]
    pub fn generation(self) -> u32 {
        self.generation
    }

    #[inline]
    pub fn type_id(self) -> TypeId {
        self.type_id
    }

    #[inline]
    pub fn is<M: Material>(self) -> bool {
        self.type_id == TypeId::of::<M>()
    }
}

/// Per-material-type typed storage.
pub struct MaterialStorage<M: Material> {
    materials: Vec<Option<M>>,
    generations: Vec<u32>,
    free_list: Vec<u32>,
    len: usize,
}

impl<M: Material> MaterialStorage<M> {
    #[inline]
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            len: 0,
        }
    }

    pub fn insert(&mut self, material: M) -> MaterialHandle {
        let index = if let Some(index) = self.free_list.pop() {
            self.materials[index as usize] = Some(material);
            index
        } else {
            let index = self.materials.len() as u32;
            self.materials.push(Some(material));
            self.generations.push(0);
            index
        };
        self.len += 1;
        MaterialHandle::new::<M>(index, self.generations[index as usize])
    }

    pub fn get(&self, handle: MaterialHandle) -> Option<&M> {
        if !self.matches(handle) {
            return None;
        }
        self.materials.get(handle.index as usize)?.as_ref()
    }

    pub fn get_mut(&mut self, handle: MaterialHandle) -> Option<&mut M> {
        if !self.matches(handle) {
            return None;
        }
        self.materials.get_mut(handle.index as usize)?.as_mut()
    }

    pub fn remove(&mut self, handle: MaterialHandle) -> Option<M> {
        if !self.matches(handle) {
            return None;
        }

        let slot = handle.index as usize;
        let material = self.materials[slot].take()?;
        self.generations[slot] = self.generations[slot].wrapping_add(1);
        self.free_list.push(handle.index);
        self.len -= 1;
        Some(material)
    }

    pub fn clear(&mut self) {
        self.free_list.clear();
        for (index, slot) in self.materials.iter_mut().enumerate() {
            if slot.take().is_some() {
                self.generations[index] = self.generations[index].wrapping_add(1);
                self.free_list.push(index as u32);
            }
        }
        self.len = 0;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn matches(&self, handle: MaterialHandle) -> bool {
        if handle.type_id != TypeId::of::<M>() {
            return false;
        }
        self.generations
            .get(handle.index as usize)
            .is_some_and(|generation| *generation == handle.generation)
    }
}

impl<M> MaterialStorage<M>
where
    M: Material + Clone,
{
    pub fn clone_from_storage(&mut self, other: &Self) {
        self.materials = other.materials.clone();
        self.generations = other.generations.clone();
        self.free_list = other.free_list.clone();
        self.len = other.len;
    }
}

impl<M: Material> Default for MaterialStorage<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    material_type: TypeId,
    pipeline_key: u64,
    mesh_layout: u64,
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    fixed_layouts: u64,
}

struct CachedPipeline {
    pipeline: wgpu::RenderPipeline,
    last_used_frame: u64,
}

/// Automatic pipeline cache for registered [`Material`] types.
pub struct PipelineCache {
    cache: FxHashMap<PipelineKey, CachedPipeline>,
    layouts: FxHashMap<TypeId, wgpu::BindGroupLayout>,
    empty_layout: Option<wgpu::BindGroupLayout>,
    empty_bind_group: Option<wgpu::BindGroup>,
    current_frame: u64,
}

const PIPELINE_TTL_FRAMES: u64 = 300;
const MATERIAL_BIND_GROUP_SLOT: u32 = 1;
const MESH_INSTANCE_LOCATION_BASE: u32 = 8;

fn mesh_instance_vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: MESH_INSTANCE_LOCATION_BASE,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 16,
            shader_location: MESH_INSTANCE_LOCATION_BASE + 1,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 32,
            shader_location: MESH_INSTANCE_LOCATION_BASE + 2,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 48,
            shader_location: MESH_INSTANCE_LOCATION_BASE + 3,
            format: wgpu::VertexFormat::Float32x4,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<[f32; 16]>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

impl PipelineCache {
    #[inline]
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::default(),
            layouts: FxHashMap::default(),
            empty_layout: None,
            empty_bind_group: None,
            current_frame: 0,
        }
    }

    pub fn register_layout<M: Material>(&mut self, device: &wgpu::Device) {
        self.layouts
            .entry(TypeId::of::<M>())
            .or_insert_with(|| M::bind_group_layout(device));
    }

    #[inline]
    pub fn layout<M: Material>(&self) -> Option<&wgpu::BindGroupLayout> {
        self.layouts.get(&TypeId::of::<M>())
    }

    #[inline]
    pub fn layout_count(&self) -> usize {
        self.layouts.len()
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
        self.empty_bind_group
            .as_ref()
            .expect("shared empty bind group should exist")
    }

    pub fn get_or_create<M: Material>(
        &mut self,
        device: &wgpu::Device,
        material: &M,
        mesh_layout: &VertexLayout,
        fixed_layouts: &[(u32, &wgpu::BindGroupLayout)],
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Result<&wgpu::RenderPipeline, MaterialError> {
        self.register_layout::<M>(device);

        let key = PipelineKey {
            material_type: TypeId::of::<M>(),
            pipeline_key: material.pipeline_key(),
            mesh_layout: fingerprint_vertex_layout(mesh_layout),
            target_format,
            depth_format,
            fixed_layouts: fingerprint_fixed_layouts(fixed_layouts),
        };

        if !self.cache.contains_key(&key) {
            let empty_layout = self.shared_empty_layout(device).clone();
            let material_layout = self
                .layout::<M>()
                .expect("layout registered before pipeline creation");
            let pipeline = self.compile_pipeline(
                device,
                &empty_layout,
                material,
                mesh_layout,
                material_layout,
                fixed_layouts,
                target_format,
                depth_format,
            )?;
            self.cache.insert(
                key,
                CachedPipeline {
                    pipeline,
                    last_used_frame: self.current_frame,
                },
            );
        }

        let cached = self
            .cache
            .get_mut(&key)
            .expect("pipeline inserted for requested key");
        cached.last_used_frame = self.current_frame;
        Ok(&cached.pipeline)
    }

    #[inline]
    pub fn new_frame(&mut self) {
        self.current_frame = self.current_frame.wrapping_add(1);
    }

    pub fn garbage_collect(&mut self) {
        self.cache.retain(|_, cached| {
            self.current_frame.saturating_sub(cached.last_used_frame) < PIPELINE_TTL_FRAMES
        });
    }

    #[inline]
    pub fn pipeline_count(&self) -> usize {
        self.cache.len()
    }

    fn compile_pipeline<M: Material>(
        &self,
        device: &wgpu::Device,
        empty_layout: &wgpu::BindGroupLayout,
        material: &M,
        mesh_layout: &VertexLayout,
        material_layout: &wgpu::BindGroupLayout,
        fixed_layouts: &[(u32, &wgpu::BindGroupLayout)],
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Result<wgpu::RenderPipeline, MaterialError> {
        let mut occupied_slots = FxHashSet::default();
        for (slot, _) in fixed_layouts {
            if !occupied_slots.insert(*slot) {
                return Err(MaterialError::OccupiedBindGroupSlot { slot: *slot });
            }
        }
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
            .expect("material slot provides at least one bind-group slot");
        let mut bind_group_layouts = vec![empty_layout.clone(); max_slot as usize + 1];
        for (slot, layout) in fixed_layouts {
            bind_group_layouts[*slot as usize] = (*layout).clone();
        }
        bind_group_layouts[MATERIAL_BIND_GROUP_SLOT as usize] = material_layout.clone();
        let bind_group_layout_refs: Vec<&wgpu::BindGroupLayout> =
            bind_group_layouts.iter().collect();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(std::any::type_name::<M>()),
            source: wgpu::ShaderSource::Wgsl(material.shader_source().wgsl_source().into()),
        });

        let attributes = resolve_vertex_attributes(&material.vertex_layout(), mesh_layout)?;
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

        let render_state = material.render_state();

        Ok(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("material_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(material.vertex_entry()),
                    buffers: &vertex_buffer_layout,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(material.fragment_entry()),
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
}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new()
    }
}

fn fingerprint_fixed_layouts(fixed_layouts: &[(u32, &wgpu::BindGroupLayout)]) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    for (slot, layout) in fixed_layouts {
        slot.hash(&mut hasher);
        (std::ptr::from_ref(*layout) as usize).hash(&mut hasher);
    }
    hasher.finish()
}

fn fingerprint_vertex_layout(layout: &VertexLayout) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    layout.hash(&mut hasher);
    hasher.finish()
}

fn resolve_vertex_attributes(
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

/// Type-erased registry for registered [`Material`] types.
pub struct MaterialRegistry {
    storages: FxHashMap<TypeId, Box<dyn Any>>,
    pipeline_cache: PipelineCache,
}

impl MaterialRegistry {
    #[inline]
    pub fn new() -> Self {
        Self {
            storages: FxHashMap::default(),
            pipeline_cache: PipelineCache::new(),
        }
    }

    pub fn register_material<M: Material>(&mut self, device: &wgpu::Device) {
        self.storages
            .entry(TypeId::of::<M>())
            .or_insert_with(|| Box::new(MaterialStorage::<M>::new()));
        self.pipeline_cache.register_layout::<M>(device);
    }

    #[inline]
    pub fn is_registered<M: Material>(&self) -> bool {
        self.storages.contains_key(&TypeId::of::<M>())
    }

    pub fn try_materials<M: Material>(&self) -> Option<&MaterialStorage<M>> {
        self.storages
            .get(&TypeId::of::<M>())
            .and_then(|storage| storage.downcast_ref::<MaterialStorage<M>>())
    }

    pub fn materials<M: Material>(&self) -> &MaterialStorage<M> {
        self.try_materials::<M>()
            .expect("requested material storage has not been registered")
    }

    pub fn try_materials_mut<M: Material>(&mut self) -> Option<&mut MaterialStorage<M>> {
        self.storages
            .get_mut(&TypeId::of::<M>())
            .and_then(|storage| storage.downcast_mut::<MaterialStorage<M>>())
    }

    pub fn ensure_storage<M: Material>(&mut self) -> &mut MaterialStorage<M> {
        self.storages
            .entry(TypeId::of::<M>())
            .or_insert_with(|| Box::new(MaterialStorage::<M>::new()));
        self.try_materials_mut::<M>()
            .expect("requested material storage should exist after ensure_storage")
    }

    pub fn materials_mut<M: Material>(&mut self) -> &mut MaterialStorage<M> {
        self.try_materials_mut::<M>()
            .expect("requested material storage has not been registered")
    }

    pub fn bind_context<'a, M: Material>(
        &'a self,
        device: &'a wgpu::Device,
        sampler_linear: &'a wgpu::Sampler,
        sampler_nearest: &'a wgpu::Sampler,
        fallback_texture: Option<&'a Texture>,
    ) -> Result<MaterialBindContext<'a>, MaterialError> {
        let layout =
            self.pipeline_cache
                .layout::<M>()
                .ok_or(MaterialError::UnregisteredMaterialType {
                    type_name: std::any::type_name::<M>(),
                })?;
        Ok(MaterialBindContext::new(
            device,
            sampler_linear,
            sampler_nearest,
            layout,
            fallback_texture,
        ))
    }

    pub fn materials_and_pipeline_cache<M: Material>(
        &mut self,
    ) -> Result<(&MaterialStorage<M>, &mut PipelineCache), MaterialError> {
        let storage = self
            .storages
            .get(&TypeId::of::<M>())
            .and_then(|storage| storage.downcast_ref::<MaterialStorage<M>>())
            .ok_or(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<M>(),
            })?;
        Ok((storage, &mut self.pipeline_cache))
    }

    #[inline]
    pub fn pipeline_cache(&self) -> &PipelineCache {
        &self.pipeline_cache
    }

    #[inline]
    pub fn pipeline_cache_mut(&mut self) -> &mut PipelineCache {
        &mut self.pipeline_cache
    }

    pub fn sync_material_storage<M>(&mut self, source: &MaterialRegistry, device: &wgpu::Device)
    where
        M: Material + Clone,
    {
        self.register_material::<M>(device);
        let dst = self.materials_mut::<M>();
        if let Some(src) = source.try_materials::<M>() {
            dst.clone_from_storage(src);
        } else {
            dst.clear();
        }
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in sprite material used by the planned mesh/material path.
#[derive(Clone)]
pub struct SpriteMaterial {
    pub color: Color,
    pub texture: Option<Texture>,
    pub uv_rect: [f32; 4],
}

impl SpriteMaterial {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn texture(mut self, texture: Texture) -> Self {
        self.texture = Some(texture);
        self
    }

    #[inline]
    pub fn clear_texture(mut self) -> Self {
        self.texture = None;
        self
    }

    #[inline]
    pub fn uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.uv_rect = [u_min, v_min, u_max, v_max];
        self
    }

    fn vertex_layout_desc() -> VertexLayout {
        VertexLayout::new(
            20,
            [
                VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
                VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
            ],
        )
    }
}

impl Default for SpriteMaterial {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            texture: None,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

impl std::fmt::Debug for SpriteMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpriteMaterial")
            .field("color", &self.color.to_array())
            .field("textured", &self.texture.is_some())
            .field("uv_rect", &self.uv_rect)
            .finish()
    }
}

impl Material for SpriteMaterial {
    fn shader_source(&self) -> ShaderSource {
        ShaderSource::Wgsl(Cow::Borrowed(include_str!(
            "../shaders/sprite/sprite_material.wgsl"
        )))
    }

    fn vertex_layout(&self) -> VertexLayout {
        Self::vertex_layout_desc()
    }

    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_material_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    fn create_bind_group(&self, ctx: &MaterialBindContext<'_>) -> wgpu::BindGroup {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct SpriteMaterialUniform {
            color: [f32; 4],
            uv_rect: [f32; 4],
        }

        let texture = ctx.texture_or_fallback(self.texture.as_ref());
        let uniform = SpriteMaterialUniform {
            color: self.color.to_array(),
            uv_rect: self.uv_rect,
        };
        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sprite_material_uniform"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_material_bg"),
            layout: ctx.layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_nearest()),
                },
            ],
        })
    }

    fn render_state(&self) -> MaterialRenderState {
        MaterialRenderState::transparent()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
    Additive,
}

impl AlphaMode {
    #[inline]
    pub const fn is_alpha_test(self) -> bool {
        matches!(self, Self::Mask)
    }

    #[inline]
    pub const fn is_transparent(self) -> bool {
        matches!(self, Self::Blend | Self::Additive)
    }
}

fn textured_uniform_layout(device: &wgpu::Device, label: &'static str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn standard_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("standard_material_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

/// Unlit mesh material using vertex positions and UVs.
#[derive(Clone)]
pub struct UnlitMaterial {
    pub color: Color,
    pub texture: Option<Texture>,
}

impl UnlitMaterial {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn texture(mut self, texture: Texture) -> Self {
        self.texture = Some(texture);
        self
    }
}

impl Default for UnlitMaterial {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            texture: None,
        }
    }
}

impl std::fmt::Debug for UnlitMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlitMaterial")
            .field("color", &self.color.to_array())
            .field("textured", &self.texture.is_some())
            .finish()
    }
}

impl Material for UnlitMaterial {
    fn shader_source(&self) -> ShaderSource {
        ShaderSource::Wgsl(Cow::Borrowed(include_str!(
            "../shaders/materials/unlit_material.wgsl"
        )))
    }

    fn vertex_layout(&self) -> VertexLayout {
        Mesh::vertex_layout_position_uv()
    }

    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        textured_uniform_layout(device, "unlit_material_bgl")
    }

    fn create_bind_group(&self, ctx: &MaterialBindContext<'_>) -> wgpu::BindGroup {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct UnlitUniform {
            color: [f32; 4],
        }

        let texture = ctx.texture_or_fallback(self.texture.as_ref());
        let uniform = UnlitUniform {
            color: self.color.to_array(),
        };
        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("unlit_material_uniform"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("unlit_material_bg"),
            layout: ctx.layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }

    fn render_state(&self) -> MaterialRenderState {
        if self.color.a < 0.999 {
            MaterialRenderState::transparent()
        } else {
            MaterialRenderState::opaque()
        }
    }

    fn scene_prepass_shader_source(&self) -> Option<ShaderSource> {
        Some(ShaderSource::Wgsl(Cow::Borrowed(include_str!(
            "../shaders/prepass/scene_material_unlit_prepass.wgsl"
        ))))
    }

    fn scene_prepass_vertex_layout(&self) -> VertexLayout {
        Mesh::vertex_layout_position_uv()
    }
}

/// Simplified forward-lit mesh material.
#[derive(Clone)]
pub struct StandardMaterial {
    pub albedo: Color,
    pub albedo_texture: Option<Texture>,
    pub metallic: f32,
    pub roughness: f32,
    pub normal_texture: Option<Texture>,
    pub emissive: Color,
    pub emissive_texture: Option<Texture>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub receive_shadows: bool,
}

impl StandardMaterial {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn receive_shadows(mut self, receive_shadows: bool) -> Self {
        self.receive_shadows = receive_shadows;
        self
    }

    #[inline]
    pub fn alpha_mode(mut self, alpha_mode: AlphaMode) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[inline]
    pub fn alpha_cutoff(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub fn alpha_mask(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_mode = AlphaMode::Mask;
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub(crate) fn casts_alpha_test_shadow(&self) -> bool {
        self.alpha_mode.is_alpha_test()
    }
}

impl Default for StandardMaterial {
    fn default() -> Self {
        Self {
            albedo: Color::WHITE,
            albedo_texture: None,
            metallic: 0.0,
            roughness: 0.8,
            normal_texture: None,
            emissive: Color::BLACK,
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            receive_shadows: true,
        }
    }
}

impl std::fmt::Debug for StandardMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StandardMaterial")
            .field("albedo", &self.albedo.to_array())
            .field("textured", &self.albedo_texture.is_some())
            .field("metallic", &self.metallic)
            .field("roughness", &self.roughness)
            .field("emissive", &self.emissive.to_array())
            .field("alpha_mode", &self.alpha_mode)
            .field("alpha_cutoff", &self.alpha_cutoff)
            .field("receive_shadows", &self.receive_shadows)
            .finish()
    }
}

impl Material for StandardMaterial {
    fn shader_source(&self) -> ShaderSource {
        ShaderSource::Wgsl(Cow::Borrowed(if self.normal_texture.is_some() {
            include_str!("../shaders/materials/standard_material_normal_mapped.wgsl")
        } else {
            include_str!("../shaders/materials/standard_material.wgsl")
        }))
    }

    fn vertex_layout(&self) -> VertexLayout {
        if self.normal_texture.is_some() {
            Mesh::vertex_layout_position_normal_tangent_uv()
        } else {
            Mesh::vertex_layout_position_normal_uv()
        }
    }

    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        standard_material_layout(device)
    }

    fn create_bind_group(&self, ctx: &MaterialBindContext<'_>) -> wgpu::BindGroup {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct StandardUniform {
            albedo: [f32; 4],
            emissive: [f32; 4],
            params: [f32; 4],
            shadow: [f32; 4],
        }

        let albedo_texture = ctx.texture_or_fallback(self.albedo_texture.as_ref());
        let emissive_texture = ctx.texture_or_fallback(self.emissive_texture.as_ref());
        let normal_texture = ctx.texture_or_fallback(self.normal_texture.as_ref());
        let uniform = StandardUniform {
            albedo: self.albedo.to_array(),
            emissive: self.emissive.to_array(),
            params: [
                self.metallic,
                self.roughness,
                self.normal_texture.is_some() as u32 as f32,
                self.emissive_texture.is_some() as u32 as f32,
            ],
            shadow: [
                self.receive_shadows as u32 as f32,
                self.alpha_cutoff,
                self.alpha_mode.is_alpha_test() as u32 as f32,
                0.0,
            ],
        };
        let uniform_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("standard_material_uniform"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("standard_material_bg"),
            layout: ctx.layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(albedo_texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(emissive_texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(normal_texture.view()),
                },
            ],
        })
    }

    fn render_state(&self) -> MaterialRenderState {
        match self.alpha_mode {
            AlphaMode::Opaque | AlphaMode::Mask => MaterialRenderState::opaque(),
            AlphaMode::Blend => MaterialRenderState::transparent(),
            AlphaMode::Additive => MaterialRenderState::additive(),
        }
    }

    fn scene_prepass_shader_source(&self) -> Option<ShaderSource> {
        Some(ShaderSource::Wgsl(Cow::Borrowed(
            if self.normal_texture.is_some() {
                include_str!(
                    "../shaders/prepass/scene_material_standard_normal_mapped_prepass.wgsl"
                )
            } else {
                include_str!("../shaders/prepass/scene_material_standard_prepass.wgsl")
            },
        )))
    }

    fn scene_prepass_vertex_layout(&self) -> VertexLayout {
        if self.normal_texture.is_some() {
            Mesh::vertex_layout_position_normal_tangent_uv()
        } else {
            Mesh::vertex_layout_position_normal_uv()
        }
    }

    fn scene_bindings(&self) -> Vec<SceneBindingDesc> {
        vec![SceneBindingDesc::shadow_view(3)]
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
    use crate::render::expert::MeshPass;
    use crate::render::gpu::Texture;
    use crate::render::phase::create_model_bind_group_layout;
    use crate::render::{RenderComposer, RenderPipelineAsset};

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for material tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("material_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn standard_material_receive_shadows_defaults_to_wicked_style_enabled() {
        assert!(StandardMaterial::default().receive_shadows);
        assert!(
            !StandardMaterial::default()
                .receive_shadows(false)
                .receive_shadows
        );
    }

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

    #[test]
    fn material_storage_reuses_slots_with_generation_bumps() {
        let mut storage = MaterialStorage::<SpriteMaterial>::new();
        let first = storage.insert(SpriteMaterial::default());

        assert_eq!(storage.len(), 1);
        assert!(storage.get(first).is_some());

        let removed = storage.remove(first);
        assert!(removed.is_some());
        assert!(storage.get(first).is_none());

        let second = storage.insert(SpriteMaterial::default().uv(0.25, 0.25, 0.75, 0.75));
        assert_eq!(second.index(), first.index());
        assert_ne!(second.generation(), first.generation());
    }

    #[test]
    fn sprite_material_can_create_bind_group_and_pipeline() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);

        let mut registry = MaterialRegistry::new();
        registry.register_material::<SpriteMaterial>(ctx.device());

        let fallback = Texture::white_pixel(&ctx);
        let material = SpriteMaterial::default();
        let bind_context = registry
            .bind_context::<SpriteMaterial>(
                ctx.device(),
                ctx.sampler_linear(),
                ctx.sampler_nearest(),
                Some(&fallback),
            )
            .expect("sprite material should have a registered bind-group layout");
        let _bind_group = material.create_bind_group(&bind_context);

        let mesh_pass = MeshPass::new(&ctx);
        let pipeline_ptr = {
            registry
                .pipeline_cache_mut()
                .get_or_create::<SpriteMaterial>(
                    ctx.device(),
                    &material,
                    &Mesh::vertex_layout_position_uv(),
                    &[(0, mesh_pass.view_layout())],
                    wgpu::TextureFormat::Bgra8Unorm,
                    None,
                )
                .expect("sprite material pipeline should compile")
                as *const wgpu::RenderPipeline
        };

        assert_eq!(registry.pipeline_cache().layout_count(), 1);
        assert_eq!(registry.pipeline_cache().pipeline_count(), 1);
        let pipeline_again_ptr = {
            registry
                .pipeline_cache_mut()
                .get_or_create::<SpriteMaterial>(
                    ctx.device(),
                    &material,
                    &Mesh::vertex_layout_position_uv(),
                    &[(0, mesh_pass.view_layout())],
                    wgpu::TextureFormat::Bgra8Unorm,
                    None,
                )
                .expect("cached sprite material pipeline should be reused")
                as *const wgpu::RenderPipeline
        };
        assert_eq!(pipeline_ptr, pipeline_again_ptr);
    }

    #[test]
    fn render_composer_register_material_exposes_typed_storage() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);
        let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());

        assert!(renderer.try_materials::<SpriteMaterial>().is_none());

        renderer.register_material::<SpriteMaterial>(&ctx);
        let handle = renderer
            .materials_mut::<SpriteMaterial>()
            .insert(SpriteMaterial::default().color(Color::CYAN));

        assert!(renderer.try_materials::<SpriteMaterial>().is_some());
        assert!(renderer.materials::<SpriteMaterial>().get(handle).is_some());
        assert_eq!(renderer.material_pipeline_cache().layout_count(), 1);
    }

    #[test]
    fn unlit_and_standard_materials_build_pipelines() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);
        let mut registry = MaterialRegistry::new();
        registry.register_material::<UnlitMaterial>(ctx.device());
        registry.register_material::<StandardMaterial>(ctx.device());

        let fallback = Texture::white_pixel(&ctx);
        let mesh_pass = MeshPass::new(&ctx);
        let model_layout = create_model_bind_group_layout(ctx.device());
        let shadow_layout =
            crate::render::lighting::shadow::create_shadow_scene_bind_group_layout(ctx.device());

        let unlit = UnlitMaterial::default().color(Color::new(0.9, 0.8, 0.2, 1.0));
        let unlit_ctx = registry
            .bind_context::<UnlitMaterial>(
                ctx.device(),
                ctx.sampler_linear(),
                ctx.sampler_nearest(),
                Some(&fallback),
            )
            .expect("unlit material layout should be registered");
        let _ = unlit.create_bind_group(&unlit_ctx);
        let _ = registry
            .pipeline_cache_mut()
            .get_or_create::<UnlitMaterial>(
                ctx.device(),
                &unlit,
                &Mesh::vertex_layout_position_uv(),
                &[(0, mesh_pass.view_layout()), (2, &model_layout)],
                wgpu::TextureFormat::Bgra8Unorm,
                None,
            )
            .expect("unlit pipeline should compile");

        let standard = StandardMaterial {
            albedo: Color::rgb(0.8, 0.6, 0.4),
            metallic: 0.2,
            roughness: 0.65,
            emissive: Color::rgb(0.05, 0.02, 0.01),
            alpha_mode: AlphaMode::Opaque,
            ..Default::default()
        };
        let standard_ctx = registry
            .bind_context::<StandardMaterial>(
                ctx.device(),
                ctx.sampler_linear(),
                ctx.sampler_nearest(),
                Some(&fallback),
            )
            .expect("standard material layout should be registered");
        let _ = standard.create_bind_group(&standard_ctx);
        let _ = registry
            .pipeline_cache_mut()
            .get_or_create::<StandardMaterial>(
                ctx.device(),
                &standard,
                &Mesh::vertex_layout_position_normal_tangent_uv(),
                &[
                    (0, mesh_pass.view_layout()),
                    (2, &model_layout),
                    (3, &shadow_layout),
                ],
                wgpu::TextureFormat::Bgra8Unorm,
                Some(crate::render::DEFAULT_DEPTH_FORMAT),
            )
            .expect("standard pipeline with depth should compile");

        assert_eq!(registry.pipeline_cache().layout_count(), 2);
        assert_eq!(registry.pipeline_cache().pipeline_count(), 2);
    }

    #[test]
    fn pipeline_cache_garbage_collects_stale_entries_after_ttl() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);

        let mut registry = MaterialRegistry::new();
        registry.register_material::<SpriteMaterial>(ctx.device());

        let fallback = Texture::white_pixel(&ctx);
        let material = SpriteMaterial::default();
        let bind_context = registry
            .bind_context::<SpriteMaterial>(
                ctx.device(),
                ctx.sampler_linear(),
                ctx.sampler_nearest(),
                Some(&fallback),
            )
            .expect("sprite material should have a registered bind-group layout");
        let _bind_group = material.create_bind_group(&bind_context);

        let mesh_pass = MeshPass::new(&ctx);
        let _ = registry
            .pipeline_cache_mut()
            .get_or_create::<SpriteMaterial>(
                ctx.device(),
                &material,
                &Mesh::vertex_layout_position_uv(),
                &[(0, mesh_pass.view_layout())],
                wgpu::TextureFormat::Bgra8Unorm,
                None,
            )
            .expect("sprite material pipeline should compile");

        assert_eq!(registry.pipeline_cache().pipeline_count(), 1);

        for _ in 0..PIPELINE_TTL_FRAMES {
            registry.pipeline_cache_mut().new_frame();
        }
        registry.pipeline_cache_mut().garbage_collect();

        assert_eq!(registry.pipeline_cache().pipeline_count(), 0);
    }

    #[test]
    fn standard_material_accepts_superset_mesh_layouts_and_keys_cache_by_mesh_layout() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);
        let mut registry = MaterialRegistry::new();
        registry.register_material::<StandardMaterial>(ctx.device());

        let mesh_pass = MeshPass::new(&ctx);
        let model_layout = create_model_bind_group_layout(ctx.device());
        let shadow_layout =
            crate::render::lighting::shadow::create_shadow_scene_bind_group_layout(ctx.device());
        let standard = StandardMaterial::default();

        let _ = registry
            .pipeline_cache_mut()
            .get_or_create::<StandardMaterial>(
                ctx.device(),
                &standard,
                &Mesh::vertex_layout_position_normal_uv(),
                &[
                    (0, mesh_pass.view_layout()),
                    (2, &model_layout),
                    (3, &shadow_layout),
                ],
                wgpu::TextureFormat::Bgra8Unorm,
                Some(crate::render::DEFAULT_DEPTH_FORMAT),
            )
            .expect("baseline standard-material pipeline should compile");

        let _ = registry
            .pipeline_cache_mut()
            .get_or_create::<StandardMaterial>(
                ctx.device(),
                &standard,
                &Mesh::vertex_layout_position_normal_tangent_uv(),
                &[
                    (0, mesh_pass.view_layout()),
                    (2, &model_layout),
                    (3, &shadow_layout),
                ],
                wgpu::TextureFormat::Bgra8Unorm,
                Some(crate::render::DEFAULT_DEPTH_FORMAT),
            )
            .expect("superset mesh layout should also compile");

        assert_eq!(registry.pipeline_cache().pipeline_count(), 2);
    }

    #[test]
    fn normal_mapped_standard_material_requires_tangent_vertex_data() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);
        let mut registry = MaterialRegistry::new();
        registry.register_material::<StandardMaterial>(ctx.device());

        let fallback = Texture::white_pixel(&ctx);
        let mesh_pass = MeshPass::new(&ctx);
        let model_layout = create_model_bind_group_layout(ctx.device());
        let shadow_layout =
            crate::render::lighting::shadow::create_shadow_scene_bind_group_layout(ctx.device());
        let standard = StandardMaterial {
            normal_texture: Some(fallback.clone()),
            ..Default::default()
        };

        let error = registry
            .pipeline_cache_mut()
            .get_or_create::<StandardMaterial>(
                ctx.device(),
                &standard,
                &Mesh::vertex_layout_position_normal_uv(),
                &[
                    (0, mesh_pass.view_layout()),
                    (2, &model_layout),
                    (3, &shadow_layout),
                ],
                wgpu::TextureFormat::Bgra8Unorm,
                Some(crate::render::DEFAULT_DEPTH_FORMAT),
            )
            .expect_err("normal-mapped standard material should require a tangent attribute");

        assert_eq!(
            error,
            MaterialError::MissingVertexAttribute {
                semantic: VertexSemantic::Tangent,
            }
        );
    }

    #[test]
    fn normal_mapped_standard_material_pipeline_compiles_with_tangent_vertex_data() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);
        let mut registry = MaterialRegistry::new();
        registry.register_material::<StandardMaterial>(ctx.device());

        let fallback = Texture::white_pixel(&ctx);
        let mesh_pass = MeshPass::new(&ctx);
        let model_layout = create_model_bind_group_layout(ctx.device());
        let shadow_layout =
            crate::render::lighting::shadow::create_shadow_scene_bind_group_layout(ctx.device());
        let standard = StandardMaterial {
            normal_texture: Some(fallback),
            ..Default::default()
        };

        let _ = registry
            .pipeline_cache_mut()
            .get_or_create::<StandardMaterial>(
                ctx.device(),
                &standard,
                &Mesh::vertex_layout_position_normal_tangent_uv(),
                &[
                    (0, mesh_pass.view_layout()),
                    (2, &model_layout),
                    (3, &shadow_layout),
                ],
                wgpu::TextureFormat::Bgra8Unorm,
                Some(crate::render::DEFAULT_DEPTH_FORMAT),
            )
            .expect("normal-mapped standard-material pipeline should compile");
    }

    #[test]
    fn standard_material_scene_prepass_matches_normal_mapping_requirements() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16]);
        let fallback = Texture::white_pixel(&ctx);

        let plain = StandardMaterial::default();
        assert_eq!(
            plain.scene_prepass_vertex_layout(),
            Mesh::vertex_layout_position_normal_uv()
        );
        let plain_shader = plain
            .scene_prepass_shader_source()
            .expect("standard material should provide a scene prepass shader");
        assert_eq!(
            plain_shader.wgsl_source(),
            include_str!("../shaders/prepass/scene_material_standard_prepass.wgsl")
        );
        assert!(
            plain_shader.wgsl_source().contains(
                "clamp(material.params.y, 0.0, 1.0),\n        clamp(material.params.x, 0.0, 1.0),"
            ),
            "standard material prepass should pack material.r = roughness and material.g = metallic"
        );

        let normal_mapped = StandardMaterial {
            normal_texture: Some(fallback),
            ..Default::default()
        };
        assert_eq!(
            normal_mapped.scene_prepass_vertex_layout(),
            Mesh::vertex_layout_position_normal_tangent_uv()
        );
        let normal_mapped_shader = normal_mapped
            .scene_prepass_shader_source()
            .expect("normal-mapped standard material should provide a scene prepass shader");
        assert_eq!(
            normal_mapped_shader.wgsl_source(),
            include_str!("../shaders/prepass/scene_material_standard_normal_mapped_prepass.wgsl")
        );
        assert!(
            normal_mapped_shader.wgsl_source().contains(
                "clamp(material.params.y, 0.0, 1.0),\n        clamp(material.params.x, 0.0, 1.0),"
            ),
            "normal-mapped material prepass should pack material.r = roughness and material.g = metallic"
        );
    }
}
