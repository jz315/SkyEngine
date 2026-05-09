use std::borrow::Cow;

use wgpu::util::DeviceExt;

use crate::render::gpu::Texture;

use super::{
    MaterialError, MaterialInstanceVersion, MaterialInterface, PreparedMaterial,
    PreparedMaterialBinding, ShaderVariantKey,
};

/// Runtime context passed to [`super::MaterialModel::prepare`].
pub struct MaterialPrepareContext<'a> {
    device: &'a wgpu::Device,
    sampler_linear: &'a wgpu::Sampler,
    sampler_nearest: &'a wgpu::Sampler,
    layout: &'a wgpu::BindGroupLayout,
    fallback_texture: Option<&'a Texture>,
    interface: &'a MaterialInterface,
    version: MaterialInstanceVersion,
    variant: ShaderVariantKey,
    debug_label: Option<Cow<'static, str>>,
}

impl<'a> MaterialPrepareContext<'a> {
    pub(crate) fn new(
        device: &'a wgpu::Device,
        sampler_linear: &'a wgpu::Sampler,
        sampler_nearest: &'a wgpu::Sampler,
        layout: &'a wgpu::BindGroupLayout,
        fallback_texture: Option<&'a Texture>,
        interface: &'a MaterialInterface,
        version: MaterialInstanceVersion,
        variant: ShaderVariantKey,
        debug_label: Option<Cow<'static, str>>,
    ) -> Self {
        Self {
            device,
            sampler_linear,
            sampler_nearest,
            layout,
            fallback_texture,
            interface,
            version,
            variant,
            debug_label,
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
    pub fn fallback_texture(&self) -> Option<&Texture> {
        self.fallback_texture
    }

    #[inline]
    pub fn interface(&self) -> &MaterialInterface {
        self.interface
    }

    #[inline]
    pub fn texture_or_fallback<'b>(&'b self, texture: Option<&'b Texture>) -> &'b Texture {
        texture
            .or(self.fallback_texture)
            .expect("material preparation requires either a material texture or a fallback")
    }

    #[inline]
    pub fn bindings(&self) -> PreparedMaterialBuilder<'_> {
        PreparedMaterialBuilder::new(self)
    }
}

/// Builder for prepared material bind groups.
pub struct PreparedMaterialBuilder<'a> {
    ctx: &'a MaterialPrepareContext<'a>,
    entries: Vec<(u32, PreparedEntry)>,
    resources: Vec<PreparedMaterialBinding>,
}

enum PreparedEntry {
    Buffer(usize),
    TextureView(wgpu::TextureView),
    Sampler(wgpu::Sampler),
}

impl<'a> PreparedMaterialBuilder<'a> {
    fn new(ctx: &'a MaterialPrepareContext<'a>) -> Self {
        Self {
            ctx,
            entries: Vec::new(),
            resources: Vec::new(),
        }
    }

    pub fn uniform<T>(mut self, binding: u32, label: &'static str, value: &T) -> Self
    where
        T: bytemuck::Pod,
    {
        let buffer = self
            .ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::bytes_of(value),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        self.resources.push(PreparedMaterialBinding::Buffer(buffer));
        self.entries
            .push((binding, PreparedEntry::Buffer(self.resources.len() - 1)));
        self
    }

    pub fn texture(mut self, binding: u32, texture: &Texture) -> Self {
        self.entries
            .push((binding, PreparedEntry::TextureView(texture.view().clone())));
        self
    }

    pub fn sampler(mut self, binding: u32, sampler: &wgpu::Sampler) -> Self {
        self.entries
            .push((binding, PreparedEntry::Sampler(sampler.clone())));
        self
    }

    pub fn build(self) -> Result<PreparedMaterial, MaterialError> {
        let expected = self.ctx.interface.bindings.bindings().len();
        if self.entries.len() != expected {
            return Err(MaterialError::ResourceCountMismatch {
                expected,
                actual: self.entries.len(),
            });
        }
        let entries: Vec<_> = self
            .entries
            .iter()
            .map(|(binding, entry)| match entry {
                PreparedEntry::Buffer(index) => {
                    let PreparedMaterialBinding::Buffer(buffer) = &self.resources[*index] else {
                        unreachable!();
                    };
                    wgpu::BindGroupEntry {
                        binding: *binding,
                        resource: buffer.as_entire_binding(),
                    }
                }
                PreparedEntry::TextureView(view) => wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                PreparedEntry::Sampler(sampler) => wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            })
            .collect();
        let bind_group = self
            .ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: self.ctx.debug_label.as_deref(),
                layout: self.ctx.layout,
                entries: &entries,
            });
        Ok(PreparedMaterial::new(
            bind_group,
            self.resources,
            self.ctx.variant.clone(),
            self.ctx.version,
            self.ctx.debug_label.clone(),
        ))
    }
}
