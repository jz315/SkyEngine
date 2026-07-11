use std::borrow::Cow;

use super::{MaterialInstanceVersion, ShaderVariantKey};

/// One owned GPU resource kept alive by a prepared material.
pub enum PreparedMaterialBinding {
    Buffer(wgpu::Buffer),
    TextureView(wgpu::TextureView),
    Sampler(wgpu::Sampler),
    BindGroup(wgpu::BindGroup),
}

/// GPU-facing state consumed by draw code.
pub struct PreparedMaterial {
    bind_group: wgpu::BindGroup,
    resources: Vec<PreparedMaterialBinding>,
    variant: ShaderVariantKey,
    prepared_version: MaterialInstanceVersion,
    debug_label: Option<Cow<'static, str>>,
}

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
