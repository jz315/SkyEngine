use std::borrow::Cow;

use super::{MaterialInstanceVersion, ShaderVariantKey};

/// One owned GPU resource kept alive by a prepared material.
pub enum PreparedMaterialBinding {
    Buffer(wgpu::Buffer),
    TextureView(wgpu::TextureView),
    Sampler(wgpu::Sampler),
    BindGroup(wgpu::BindGroup),
}

/// Compatibility wrapper used by the older expert `MeshPass` path.
pub struct MaterialResourceBindings {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
}

impl MaterialResourceBindings {
    pub fn empty(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material_resource_empty_bgl"),
            entries: &[],
        });
        Self {
            bind_group_layout,
            bind_group: None,
        }
    }

    #[inline]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    #[inline]
    pub fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.bind_group.as_ref()
    }
}

/// Compatibility material instance used by the older expert `MeshPass` path.
pub struct MaterialInstance {
    prepared: PreparedMaterial,
}

impl MaterialInstance {
    #[inline]
    pub fn from_prepared(prepared: PreparedMaterial) -> Self {
        Self { prepared }
    }

    #[inline]
    pub fn upload(&mut self, _ctx: &crate::gpu::GpuContext) {}

    #[inline]
    pub fn property_bind_group(&self) -> &wgpu::BindGroup {
        self.prepared.bind_group()
    }

    #[inline]
    pub fn try_resource_bind_group(&self) -> Result<&wgpu::BindGroup, super::MaterialError> {
        Ok(self.prepared.bind_group())
    }
}

/// Temporary bind context retained only while older draw code is being moved
/// onto `PreparedMaterial`.
pub struct MaterialBindContext<'a> {
    device: &'a wgpu::Device,
    sampler_linear: &'a wgpu::Sampler,
    sampler_nearest: &'a wgpu::Sampler,
    layout: &'a wgpu::BindGroupLayout,
    fallback_texture: Option<&'a crate::render::gpu::Texture>,
}

impl<'a> MaterialBindContext<'a> {
    #[inline]
    pub fn new(
        device: &'a wgpu::Device,
        sampler_linear: &'a wgpu::Sampler,
        sampler_nearest: &'a wgpu::Sampler,
        layout: &'a wgpu::BindGroupLayout,
        fallback_texture: Option<&'a crate::render::gpu::Texture>,
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
    pub fn sampler_linear(&self) -> &wgpu::Sampler {
        self.sampler_linear
    }

    #[inline]
    pub fn sampler_nearest(&self) -> &wgpu::Sampler {
        self.sampler_nearest
    }

    #[inline]
    pub fn texture_or_fallback<'b>(
        &'b self,
        texture: Option<&'b crate::render::gpu::Texture>,
    ) -> &'b crate::render::gpu::Texture {
        texture
            .or(self.fallback_texture)
            .expect("material bind context requires either a material texture or a fallback")
    }
}

/// GPU-facing state consumed by draw code.
pub struct PreparedMaterial {
    bind_group: wgpu::BindGroup,
    resources: Vec<PreparedMaterialBinding>,
    variant: ShaderVariantKey,
    prepared_version: MaterialInstanceVersion,
    debug_label: Option<Cow<'static, str>>,
}

pub type PreparedMaterialVersion = MaterialInstanceVersion;

impl PreparedMaterial {
    pub fn new(
        bind_group: wgpu::BindGroup,
        resources: Vec<PreparedMaterialBinding>,
        variant: ShaderVariantKey,
        prepared_version: MaterialInstanceVersion,
        debug_label: Option<Cow<'static, str>>,
    ) -> Self {
        Self {
            bind_group,
            resources,
            variant,
            prepared_version,
            debug_label,
        }
    }

    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    #[inline]
    pub fn variant(&self) -> &ShaderVariantKey {
        &self.variant
    }

    #[inline]
    pub fn prepared_version(&self) -> MaterialInstanceVersion {
        self.prepared_version
    }

    #[inline]
    pub fn debug_label(&self) -> Option<&str> {
        self.debug_label.as_deref()
    }

    #[inline]
    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }
}
