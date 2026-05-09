use std::borrow::Cow;

use rustc_hash::FxHashSet;

use super::MaterialError;

/// One material-local binding declaration. Scene resources are declared through
/// [`super::SceneResourceRequirements`] instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MaterialBinding {
    Uniform {
        binding: u32,
        size: wgpu::BufferSize,
        visibility: wgpu::ShaderStages,
    },
    Texture2D {
        binding: u32,
        sample_type: wgpu::TextureSampleType,
        visibility: wgpu::ShaderStages,
    },
    Sampler {
        binding: u32,
        kind: wgpu::SamplerBindingType,
        visibility: wgpu::ShaderStages,
    },
    StorageBuffer {
        binding: u32,
        read_only: bool,
        visibility: wgpu::ShaderStages,
    },
}

impl MaterialBinding {
    #[inline]
    pub fn uniform(binding: u32, size: wgpu::BufferSize) -> Self {
        Self::Uniform {
            binding,
            size,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        }
    }

    #[inline]
    pub fn texture_2d(binding: u32) -> Self {
        Self::Texture2D {
            binding,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            visibility: wgpu::ShaderStages::FRAGMENT,
        }
    }

    #[inline]
    pub fn sampler(binding: u32) -> Self {
        Self::Sampler {
            binding,
            kind: wgpu::SamplerBindingType::Filtering,
            visibility: wgpu::ShaderStages::FRAGMENT,
        }
    }

    #[inline]
    pub fn binding(&self) -> u32 {
        match self {
            Self::Uniform { binding, .. }
            | Self::Texture2D { binding, .. }
            | Self::Sampler { binding, .. }
            | Self::StorageBuffer { binding, .. } => *binding,
        }
    }

    #[inline]
    pub fn visibility(&self) -> wgpu::ShaderStages {
        match self {
            Self::Uniform { visibility, .. }
            | Self::Texture2D { visibility, .. }
            | Self::Sampler { visibility, .. }
            | Self::StorageBuffer { visibility, .. } => *visibility,
        }
    }

    pub(crate) fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        match *self {
            Self::Uniform {
                binding,
                size,
                visibility,
            } => wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(size),
                },
                count: None,
            },
            Self::Texture2D {
                binding,
                sample_type,
                visibility,
            } => wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            Self::Sampler {
                binding,
                kind,
                visibility,
            } => wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Sampler(kind),
                count: None,
            },
            Self::StorageBuffer {
                binding,
                read_only,
                visibility,
            } => wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        }
    }
}

/// Static material-local bind group declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct MaterialBindingLayout {
    bindings: Vec<MaterialBinding>,
}

impl MaterialBindingLayout {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, binding: MaterialBinding) -> Self {
        self.bindings.push(binding);
        self
    }

    #[inline]
    pub fn bindings(&self) -> &[MaterialBinding] {
        &self.bindings
    }

    pub(crate) fn validate(&self, model: &'static str) -> Result<(), MaterialError> {
        let mut seen = FxHashSet::default();
        for binding in &self.bindings {
            if !seen.insert(binding.binding()) {
                return Err(MaterialError::DuplicateBinding {
                    model,
                    binding: binding.binding(),
                });
            }
            if binding.visibility().is_empty() {
                return Err(MaterialError::InvalidBindingVisibility {
                    model,
                    binding: binding.binding(),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn create_bind_group_layout(
        &self,
        device: &wgpu::Device,
        label: impl Into<Cow<'static, str>>,
    ) -> wgpu::BindGroupLayout {
        let label = label.into();
        let entries: Vec<_> = self
            .bindings
            .iter()
            .map(MaterialBinding::layout_entry)
            .collect();
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&label),
            entries: &entries,
        })
    }
}
