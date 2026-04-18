//! Core material traits and type definitions: [`Material`], [`ShaderSource`],
//! [`MaterialRenderState`], [`MaterialBindContext`], and scene binding descriptors.

use std::any::TypeId;
use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use crate::render::gpu::Texture;
use crate::render::resources::mesh::VertexLayout;

// ── Shader source ──────────────────────────────────────────────────────────

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

// ── Render state ───────────────────────────────────────────────────────────

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

// ── Bind context ───────────────────────────────────────────────────────────

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

// ── Material trait ─────────────────────────────────────────────────────────

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

// ── Scene binding descriptors ──────────────────────────────────────────────

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

// ── Alpha mode ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Blend,
    Additive,
}
