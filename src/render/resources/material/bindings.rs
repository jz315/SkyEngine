//! Material resource bindings: layout descriptors, bind-group creation, and
//! the combined [`MaterialInstance`] convenience type.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use super::MaterialError;
use super::properties::MaterialProperties;

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

pub(crate) fn validate_binding_layouts(layouts: &[MaterialBindingLayout]) -> Result<(), MaterialError> {
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
