//! Lightweight material building blocks for reusable pipelines, property
//! buffers, and resource bindings.
//!
//! [`MaterialProperties`] owns a typed uniform buffer.
//! [`MaterialResourceBindings`] owns a bind-group layout plus runtime-bound
//! textures, samplers, and buffers.
//! [`MaterialPipelineCache`] owns a shader plus per-format render pipelines.
//! [`MaterialInstance`] combines properties and resource bindings into a
//! reusable material instance.
//!
//! # Usage
//!
//! ```rust,ignore
//! let props = MaterialProperties::new(gpu, &[
//!     ("intensity", PropertyType::Float),
//!     ("color",     PropertyType::Vec4),
//! ]);
//! props.set_float("intensity", 0.8);
//! props.set_vec4("color", [1.0, 0.0, 0.0, 1.0]);
//! props.upload(gpu); // single write_buffer call
//! ```

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::{
    BindGroup, BindGroupDesc, BindGroupEntry, BindGroupLayout, BindGroupLayoutDesc,
    BindGroupLayoutEntry, BindingType, BlendState, Buffer, BufferDesc, BufferUsage,
    ColorTargetState, DepthStencilState, Gpu, Image, ImageView, Pipeline, PrimitiveState,
    RenderPassEncoder, RenderPipelineDesc, Shader, ShaderDesc, ShaderStages, TextureFormat,
    VertexBufferLayout,
};

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
    /// Size in bytes (always f32-aligned).
    #[inline]
    const fn size(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Vec2 => 8,
            Self::Vec3 => 12,
            Self::Vec4 => 16,
        }
    }

    /// Alignment in bytes (GPU uniform buffers require 4-byte alignment).
    #[inline]
    const fn align(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Vec2 => 8,
            Self::Vec3 => 16, // vec3 is 16-byte aligned in std140
            Self::Vec4 => 16,
        }
    }
}

// ── Property layout ─────────────────────────────────────────────────────────

struct PropertySlot {
    offset: usize,
    ty: PropertyType,
}

/// A typed uniform buffer with named float/vector properties.
///
/// Properties are laid out using std140 alignment rules and uploaded
/// to the GPU with a single `write_buffer` call.
pub struct MaterialProperties {
    slots: FxHashMap<Cow<'static, str>, PropertySlot>,
    data: Vec<u8>,
    buffer: Buffer,
    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,
    dirty: bool,
}

impl MaterialProperties {
    /// Create a new property set from a list of `(name, type)` pairs.
    ///
    /// The properties are laid out in declaration order using std140
    /// alignment.
    pub fn new(gpu: &mut impl Gpu, properties: &[(&str, PropertyType)]) -> Self {
        let mut slots = FxHashMap::default();
        let mut offset = 0usize;

        for (name, ty) in properties {
            let align = ty.align();
            // Align offset.
            offset = (offset + align - 1) & !(align - 1);
            slots.insert(
                Cow::Owned(name.to_string()),
                PropertySlot { offset, ty: *ty },
            );
            offset += ty.size();
        }

        // Round up to 16-byte boundary (GPU uniform buffer alignment).
        let total = (offset + 15) & !15;
        let data = vec![0u8; total.max(16)];

        let buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("material_props"),
            size: data.len() as u64,
            usage: BufferUsage::UNIFORM | BufferUsage::COPY_DST,
        });

        let bind_group_layout = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("material_props_bgl"),
            entries: vec![BindGroupLayoutEntry {
                binding: 0,
                ty: BindingType::UniformBuffer,
                visibility: ShaderStages::VERTEX_FRAGMENT,
            }],
        });

        let bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("material_props_bg"),
            layout: bind_group_layout,
            entries: vec![BindGroupEntry::Buffer {
                binding: 0,
                buffer,
                offset: 0,
                size: data.len() as u64,
            }],
        });

        Self {
            slots,
            data,
            buffer,
            bind_group_layout,
            bind_group,
            dirty: true,
        }
    }

    /// Set a float property.
    pub fn set_float(&mut self, name: &str, value: f32) {
        if let Some(slot) = self.slots.get(name) {
            debug_assert_eq!(slot.ty, PropertyType::Float, "type mismatch for '{name}'");
            self.data[slot.offset..slot.offset + 4].copy_from_slice(&value.to_le_bytes());
            self.dirty = true;
        }
    }

    /// Set a vec2 property.
    pub fn set_vec2(&mut self, name: &str, value: [f32; 2]) {
        if let Some(slot) = self.slots.get(name) {
            debug_assert_eq!(slot.ty, PropertyType::Vec2, "type mismatch for '{name}'");
            self.data[slot.offset..slot.offset + 8].copy_from_slice(bytemuck::bytes_of(&value));
            self.dirty = true;
        }
    }

    /// Set a vec3 property.
    pub fn set_vec3(&mut self, name: &str, value: [f32; 3]) {
        if let Some(slot) = self.slots.get(name) {
            debug_assert_eq!(slot.ty, PropertyType::Vec3, "type mismatch for '{name}'");
            self.data[slot.offset..slot.offset + 12].copy_from_slice(bytemuck::bytes_of(&value));
            self.dirty = true;
        }
    }

    /// Set a vec4 property.
    pub fn set_vec4(&mut self, name: &str, value: [f32; 4]) {
        if let Some(slot) = self.slots.get(name) {
            debug_assert_eq!(slot.ty, PropertyType::Vec4, "type mismatch for '{name}'");
            self.data[slot.offset..slot.offset + 16].copy_from_slice(bytemuck::bytes_of(&value));
            self.dirty = true;
        }
    }

    /// Get a float property value.
    pub fn get_float(&self, name: &str) -> Option<f32> {
        self.slots.get(name).map(|slot| {
            debug_assert_eq!(slot.ty, PropertyType::Float);
            f32::from_le_bytes(self.data[slot.offset..slot.offset + 4].try_into().unwrap())
        })
    }

    /// Get a vec4 property value.
    pub fn get_vec4(&self, name: &str) -> Option<[f32; 4]> {
        self.slots.get(name).map(|slot| {
            debug_assert_eq!(slot.ty, PropertyType::Vec4);
            *bytemuck::from_bytes::<[f32; 4]>(&self.data[slot.offset..slot.offset + 16])
        })
    }

    /// Upload dirty properties to the GPU buffer.
    pub fn upload(&mut self, gpu: &impl Gpu) {
        if self.dirty {
            gpu.write_buffer(self.buffer, 0, &self.data);
            self.dirty = false;
        }
    }

    /// The bind group layout for this property set.
    #[inline]
    pub fn bind_group_layout(&self) -> BindGroupLayout {
        self.bind_group_layout
    }

    /// The bind group to set on a render pass.
    #[inline]
    pub fn bind_group(&self) -> BindGroup {
        self.bind_group
    }

    /// The GPU buffer handle.
    #[inline]
    pub fn buffer(&self) -> Buffer {
        self.buffer
    }

    /// Whether any property has been changed since the last upload.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Destroy the underlying GPU resources.
    pub fn destroy(&self, gpu: &mut impl Gpu) {
        gpu.destroy_bind_group(self.bind_group);
        gpu.destroy_bind_group_layout(self.bind_group_layout);
        gpu.destroy_buffer(self.buffer);
    }
}

// ── Material resource bindings ─────────────────────────────────────────────

#[derive(Clone)]
pub struct MaterialBindingLayout {
    pub binding: u32,
    pub name: Cow<'static, str>,
    pub ty: BindingType,
    pub visibility: ShaderStages,
}

impl MaterialBindingLayout {
    pub fn new(
        binding: u32,
        name: impl Into<Cow<'static, str>>,
        ty: BindingType,
        visibility: ShaderStages,
    ) -> Self {
        Self {
            binding,
            name: name.into(),
            ty,
            visibility,
        }
    }
}

#[derive(Clone, Copy)]
enum MaterialBindingValue {
    Buffer {
        buffer: Buffer,
        offset: u64,
        size: u64,
    },
    Texture {
        image: Image,
    },
    TextureView {
        view: ImageView,
    },
    Sampler {
        sampler: crate::gpu::Sampler,
    },
}

pub struct MaterialResourceBindings {
    bind_group_layout: BindGroupLayout,
    bind_group: Option<BindGroup>,
    layouts: Vec<MaterialBindingLayout>,
    binding_names: FxHashMap<Cow<'static, str>, u32>,
    values: FxHashMap<u32, MaterialBindingValue>,
    dirty: bool,
    label: Cow<'static, str>,
}

impl MaterialResourceBindings {
    pub fn new(
        gpu: &mut impl Gpu,
        label: impl Into<Cow<'static, str>>,
        layouts: &[MaterialBindingLayout],
    ) -> Self {
        let label = label.into();
        let bind_group_layout = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Owned(format!("{}_bgl", label)),
            entries: layouts
                .iter()
                .map(|layout| BindGroupLayoutEntry {
                    binding: layout.binding,
                    ty: layout.ty,
                    visibility: layout.visibility,
                })
                .collect(),
        });

        Self {
            bind_group_layout,
            bind_group: None,
            layouts: layouts.to_vec(),
            binding_names: layouts
                .iter()
                .map(|layout| (layout.name.clone(), layout.binding))
                .collect(),
            values: FxHashMap::default(),
            dirty: true,
            label,
        }
    }

    pub fn bind_group_layout(&self) -> BindGroupLayout {
        self.bind_group_layout
    }

    pub fn set_buffer(&mut self, binding: u32, buffer: Buffer, offset: u64, size: u64) {
        self.values.insert(
            binding,
            MaterialBindingValue::Buffer {
                buffer,
                offset,
                size,
            },
        );
        self.dirty = true;
    }

    pub fn set_texture(&mut self, binding: u32, image: Image) {
        self.values
            .insert(binding, MaterialBindingValue::Texture { image });
        self.dirty = true;
    }

    pub fn set_texture_view(&mut self, binding: u32, view: ImageView) {
        self.values
            .insert(binding, MaterialBindingValue::TextureView { view });
        self.dirty = true;
    }

    pub fn set_sampler(&mut self, binding: u32, sampler: crate::gpu::Sampler) {
        self.values
            .insert(binding, MaterialBindingValue::Sampler { sampler });
        self.dirty = true;
    }

    pub fn set_buffer_named(&mut self, name: &str, buffer: Buffer, offset: u64, size: u64) {
        if let Some(&binding) = self.binding_names.get(name) {
            self.set_buffer(binding, buffer, offset, size);
        }
    }

    pub fn set_texture_named(&mut self, name: &str, image: Image) {
        if let Some(&binding) = self.binding_names.get(name) {
            self.set_texture(binding, image);
        }
    }

    pub fn set_texture_view_named(&mut self, name: &str, view: ImageView) {
        if let Some(&binding) = self.binding_names.get(name) {
            self.set_texture_view(binding, view);
        }
    }

    pub fn set_sampler_named(&mut self, name: &str, sampler: crate::gpu::Sampler) {
        if let Some(&binding) = self.binding_names.get(name) {
            self.set_sampler(binding, sampler);
        }
    }

    pub fn bind_group(&mut self, gpu: &mut impl Gpu) -> BindGroup {
        if self.dirty || self.bind_group.is_none() {
            if let Some(bind_group) = self.bind_group.take() {
                gpu.destroy_bind_group(bind_group);
            }

            let entries = self
                .layouts
                .iter()
                .map(|layout| {
                    let value = self
                        .values
                        .get(&layout.binding)
                        .unwrap_or_else(|| panic!("missing material binding '{}'", layout.name));
                    match *value {
                        MaterialBindingValue::Buffer {
                            buffer,
                            offset,
                            size,
                        } => BindGroupEntry::Buffer {
                            binding: layout.binding,
                            buffer,
                            offset,
                            size,
                        },
                        MaterialBindingValue::Texture { image } => BindGroupEntry::Texture {
                            binding: layout.binding,
                            image,
                        },
                        MaterialBindingValue::TextureView { view } => BindGroupEntry::TextureView {
                            binding: layout.binding,
                            view,
                        },
                        MaterialBindingValue::Sampler { sampler } => BindGroupEntry::Sampler {
                            binding: layout.binding,
                            sampler,
                        },
                    }
                })
                .collect();

            self.bind_group = Some(gpu.create_bind_group(&BindGroupDesc {
                label: Cow::Owned(format!("{}_bg", self.label)),
                layout: self.bind_group_layout,
                entries,
            }));
            self.dirty = false;
        }

        self.bind_group.expect("material bind group missing")
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        if let Some(bind_group) = self.bind_group.take() {
            gpu.destroy_bind_group(bind_group);
        }
        gpu.destroy_bind_group_layout(self.bind_group_layout);
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

    /// Bind the material's properties and resources to the render pass
    /// using **manually specified** slots.
    ///
    /// Prefer [`bind_with_cache`] which reads the slot assignments from the
    /// pipeline cache automatically.
    pub fn bind(
        &mut self,
        gpu: &mut impl Gpu,
        pass: &mut dyn RenderPassEncoder,
        property_slot: u32,
        binding_slot: u32,
    ) {
        self.properties.upload(gpu);
        pass.set_bind_group(property_slot, self.properties.bind_group());
        pass.set_bind_group(binding_slot, self.bindings.bind_group(gpu));
    }

    /// Bind the material's properties and resources to the render pass
    /// using slot assignments from the pipeline cache.
    ///
    /// This is the **preferred** binding path — it eliminates manual slot
    /// arithmetic and stays in sync with the pipeline layout.
    pub fn bind_with_cache(
        &mut self,
        gpu: &mut impl Gpu,
        pass: &mut dyn RenderPassEncoder,
        cache: &MaterialPipelineCache,
    ) {
        self.properties.upload(gpu);
        if let Some(slot) = cache.property_slot() {
            pass.set_bind_group(slot, self.properties.bind_group());
        }
        if let Some(slot) = cache.resource_slot() {
            pass.set_bind_group(slot, self.bindings.bind_group(gpu));
        }
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        self.bindings.destroy(gpu);
        self.properties.destroy(gpu);
    }
}

// ── Material pipeline cache ────────────────────────────────────────────────

#[derive(Clone)]
pub struct MaterialPipelineDesc {
    pub label: Cow<'static, str>,
    pub shader_source: Cow<'static, str>,
    pub vs_entry: &'static str,
    pub fs_entry: &'static str,
    pub vertex_layouts: Vec<VertexBufferLayout>,
    pub bind_group_layouts: Vec<BindGroupLayout>,
    pub blend: Option<BlendState>,
    pub depth_stencil: Option<DepthStencilState>,
    pub primitive: PrimitiveState,
    /// If set, the material property uniform buffer is bound at this slot.
    ///
    /// The slot index must not conflict with any layout already present in
    /// `bind_group_layouts`.  If `None`, the properties layout (if any)
    /// is appended after the existing layouts.
    pub material_properties_slot: Option<u32>,
    /// If set, the material resource bindings are bound at this slot.
    ///
    /// Must not conflict with other slots.  If `None`, the resources
    /// layout (if any) is appended after properties.
    pub material_resources_slot: Option<u32>,
}

pub struct MaterialPipelineCache {
    shader: Shader,
    desc: MaterialPipelineDesc,
    properties_layout: Option<BindGroupLayout>,
    bindings_layout: Option<BindGroupLayout>,
    pipelines: FxHashMap<TextureFormat, Pipeline>,
    /// Resolved bind-group slot for material properties.
    resolved_property_slot: Option<u32>,
    /// Resolved bind-group slot for material resource bindings.
    resolved_resource_slot: Option<u32>,
}

impl MaterialPipelineCache {
    pub fn new(
        gpu: &mut impl Gpu,
        desc: MaterialPipelineDesc,
        properties_layout: Option<BindGroupLayout>,
        bindings_layout: Option<BindGroupLayout>,
    ) -> Self {
        let shader = gpu.create_shader(&ShaderDesc {
            label: Cow::Owned(format!("{}_shader", desc.label)),
            source: desc.shader_source.clone(),
        });

        // Pre-resolve slot assignments so callers can query them.
        let base = desc.bind_group_layouts.len() as u32;
        let resolved_property_slot = if properties_layout.is_some() {
            Some(desc.material_properties_slot.unwrap_or(base))
        } else {
            None
        };
        let resolved_resource_slot = if bindings_layout.is_some() {
            let after_props = if properties_layout.is_some() {
                resolved_property_slot.unwrap() + 1
            } else {
                base
            };
            Some(desc.material_resources_slot.unwrap_or(after_props))
        } else {
            None
        };

        Self {
            shader,
            desc,
            properties_layout,
            bindings_layout,
            pipelines: FxHashMap::default(),
            resolved_property_slot,
            resolved_resource_slot,
        }
    }

    /// The bind-group slot used for material property uniforms, if any.
    #[inline]
    pub fn property_slot(&self) -> Option<u32> {
        self.resolved_property_slot
    }

    /// The bind-group slot used for material resource bindings, if any.
    #[inline]
    pub fn resource_slot(&self) -> Option<u32> {
        self.resolved_resource_slot
    }

    pub fn pipeline(&mut self, gpu: &mut impl Gpu, target_format: TextureFormat) -> Pipeline {
        if let Some(pipeline) = self.pipelines.get(&target_format) {
            return *pipeline;
        }

        // Build the full bind-group-layout list with material slots
        // inserted at the resolved positions.
        let mut slots: Vec<(u32, BindGroupLayout)> = self
            .desc
            .bind_group_layouts
            .iter()
            .enumerate()
            .map(|(i, l)| (i as u32, *l))
            .collect();

        if let (Some(slot), Some(layout)) = (self.resolved_property_slot, self.properties_layout) {
            slots.push((slot, layout));
        }
        if let (Some(slot), Some(layout)) = (self.resolved_resource_slot, self.bindings_layout) {
            slots.push((slot, layout));
        }

        slots.sort_by_key(|(slot, _)| *slot);
        let bind_group_layouts: Vec<BindGroupLayout> = slots.into_iter().map(|(_, l)| l).collect();

        let pipeline = gpu.create_render_pipeline(&RenderPipelineDesc {
            label: Cow::Owned(format!("{}_{target_format:?}", self.desc.label)),
            shader: self.shader,
            vs_entry: self.desc.vs_entry,
            fs_entry: self.desc.fs_entry,
            vertex_layouts: self.desc.vertex_layouts.clone(),
            bind_group_layouts,
            color_targets: vec![ColorTargetState {
                format: target_format,
                blend: self.desc.blend,
            }],
            depth_stencil: self.desc.depth_stencil.clone(),
            primitive: self.desc.primitive.clone(),
        });
        self.pipelines.insert(target_format, pipeline);
        pipeline
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        for pipeline in self.pipelines.drain().map(|(_, pipeline)| pipeline) {
            gpu.destroy_pipeline(pipeline);
        }
        gpu.destroy_shader(self.shader);
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_layout_std140_alignment() {
        // float (4 bytes, offset 0)
        // vec3  (12 bytes, aligned to 16, offset 16)
        // vec4  (16 bytes, aligned to 16, offset 32)
        // Total: 48 bytes
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
        assert_eq!(computed[1], ("b", 16)); // vec3 aligns to 16
        assert_eq!(computed[2], ("c", 32)); // next 16-byte boundary after 16+12=28
    }
}
