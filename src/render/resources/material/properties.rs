//! Typed uniform buffer with named float/vector properties and STD140 layout.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use super::MaterialError;

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
    pub(crate) const fn size(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Vec2 => 8,
            Self::Vec3 => 12,
            Self::Vec4 => 16,
        }
    }

    #[inline]
    pub(crate) const fn align(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Vec2 => 8,
            Self::Vec3 | Self::Vec4 => 16,
        }
    }
}

// ── Property layout ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub(crate) struct PropertySlot {
    pub offset: usize,
    pub ty: PropertyType,
}

pub(crate) fn build_property_layout(
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
