use std::hash::{Hash, Hasher};

use crate::render::resources::mesh::{Mesh, VertexLayout};

use super::{
    MainPassMode, MaterialBinding, MaterialBindingLayout, MaterialError, MaterialPassSet,
    MaterialShaderSet, SceneResourceRequirements, ShaderVariantPolicy,
};

/// Render-state settings contributed by a material model.
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

/// Static renderer-facing contract for a material model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MaterialInterface {
    pub name: &'static str,
    pub shader: MaterialShaderSet,
    pub vertex: VertexLayout,
    pub bindings: MaterialBindingLayout,
    pub scene: SceneResourceRequirements,
    pub render_state: MaterialRenderState,
    pub passes: MaterialPassSet,
    pub variants: ShaderVariantPolicy,
}

impl MaterialInterface {
    #[inline]
    pub fn builder(name: &'static str) -> MaterialInterfaceBuilder {
        MaterialInterfaceBuilder::new(name)
    }

    pub fn validate(&self) -> Result<(), MaterialError> {
        if self.name.trim().is_empty() {
            return Err(MaterialError::InvalidInterface {
                model: self.name,
                reason: "material name must not be empty".to_string(),
            });
        }
        if self.shader.vertex_entry.trim().is_empty()
            || self.shader.fragment_entry.trim().is_empty()
        {
            return Err(MaterialError::InvalidInterface {
                model: self.name,
                reason: "shader entry points must not be empty".to_string(),
            });
        }
        self.bindings.validate(self.name)?;

        let mut variants = rustc_hash::FxHashSet::default();
        for dimension in self.variants.dimensions() {
            if !variants.insert(*dimension) {
                return Err(MaterialError::DuplicateVariantDimension {
                    model: self.name,
                    name: dimension,
                });
            }
        }

        if self.passes.main.is_transparent() && self.render_state.blend.is_none() {
            return Err(MaterialError::UnsupportedPassCombination {
                model: self.name,
                reason: "transparent main pass requires blend state".to_string(),
            });
        }
        if !self.passes.main.is_transparent() && self.render_state.blend.is_some() {
            return Err(MaterialError::UnsupportedPassCombination {
                model: self.name,
                reason: "opaque main pass must not declare blend state".to_string(),
            });
        }
        Ok(())
    }

    pub(crate) fn binding_layout_fingerprint(&self) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        self.bindings.hash(&mut hasher);
        hasher.finish()
    }
}

pub struct MaterialInterfaceBuilder {
    interface: MaterialInterface,
}

impl MaterialInterfaceBuilder {
    pub fn new(name: &'static str) -> Self {
        Self {
            interface: MaterialInterface {
                name,
                shader: MaterialShaderSet::wgsl(""),
                vertex: Mesh::vertex_layout_position_uv(),
                bindings: MaterialBindingLayout::new(),
                scene: SceneResourceRequirements::new(),
                render_state: MaterialRenderState::opaque(),
                passes: MaterialPassSet::opaque(),
                variants: ShaderVariantPolicy::none(),
            },
        }
    }

    pub fn shader(mut self, shader: MaterialShaderSet) -> Self {
        self.interface.shader = shader;
        self
    }

    pub fn vertex(mut self, vertex: VertexLayout) -> Self {
        self.interface.vertex = vertex;
        self
    }

    pub fn scene(mut self, scene: SceneResourceRequirements) -> Self {
        self.interface.scene = scene;
        self
    }

    pub fn binding(mut self, binding: MaterialBinding) -> Self {
        self.interface.bindings = self.interface.bindings.with(binding);
        self
    }

    pub fn bindings(mut self, bindings: MaterialBindingLayout) -> Self {
        self.interface.bindings = bindings;
        self
    }

    pub fn render_state(mut self, render_state: MaterialRenderState) -> Self {
        self.interface.render_state = render_state;
        self
    }

    pub fn main_pass(mut self, main: MainPassMode) -> Self {
        self.interface.passes.main = main;
        self
    }

    pub fn passes(mut self, passes: MaterialPassSet) -> Self {
        self.interface.passes = passes;
        self
    }

    pub fn variants(mut self, variants: ShaderVariantPolicy) -> Self {
        self.interface.variants = variants;
        self
    }

    pub fn build(self) -> MaterialInterface {
        self.interface
    }
}
