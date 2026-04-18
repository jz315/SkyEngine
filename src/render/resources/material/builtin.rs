//! Built-in material implementations: [`SpriteMaterial`], [`UnlitMaterial`],
//! and [`StandardMaterial`].

use std::borrow::Cow;

use wgpu::util::DeviceExt;

use crate::render::gpu::Texture;
use crate::render::view::Color;
use crate::render::resources::mesh::{Mesh, VertexAttribute, VertexLayout, VertexSemantic};
use super::traits::{
    AlphaMode, Material, MaterialBindContext, MaterialRenderState, SceneBindingDesc, ShaderSource,
};

// ── Shared bind-group layout helpers ───────────────────────────────────────

pub(crate) fn textured_uniform_layout(device: &wgpu::Device, label: &'static str) -> wgpu::BindGroupLayout {
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

// ── SpriteMaterial ─────────────────────────────────────────────────────────

/// Built-in sprite material used by the planned mesh/material path.
#[derive(Clone)]
pub struct SpriteMaterial {
    pub position: [f32; 3],
    pub size: [f32; 2],
    pub rotation: f32,
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
    pub fn position(mut self, x: f32, y: f32, z: f32) -> Self {
        self.position = [x, y, z];
        self
    }

    #[inline]
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = [width, height];
        self
    }

    #[inline]
    pub fn rotation(mut self, radians: f32) -> Self {
        self.rotation = radians;
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
            position: [0.0, 0.0, 0.0],
            size: [1.0, 1.0],
            rotation: 0.0,
            color: Color::WHITE,
            texture: None,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

impl std::fmt::Debug for SpriteMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpriteMaterial")
            .field("position", &self.position)
            .field("size", &self.size)
            .field("rotation", &self.rotation)
            .field("color", &self.color.to_array())
            .field("textured", &self.texture.is_some())
            .field("uv_rect", &self.uv_rect)
            .finish()
    }
}

impl Material for SpriteMaterial {
    fn shader_source(&self) -> ShaderSource {
        ShaderSource::Wgsl(Cow::Borrowed(include_str!(
            "../../shaders/sprite_material.wgsl"
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
            transform: [f32; 4],
            rotation: [f32; 4],
            color: [f32; 4],
            uv_rect: [f32; 4],
        }

        let texture = ctx.texture_or_fallback(self.texture.as_ref());
        let (sin_a, cos_a) = self.rotation.sin_cos();
        let uniform = SpriteMaterialUniform {
            transform: [
                self.position[0],
                self.position[1],
                self.size[0],
                self.size[1],
            ],
            rotation: [sin_a, cos_a, self.position[2], 0.0],
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

// ── UnlitMaterial ──────────────────────────────────────────────────────────

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
            "../../shaders/unlit_material.wgsl"
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
}

// ── StandardMaterial ───────────────────────────────────────────────────────

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
}

impl StandardMaterial {
    #[inline]
    pub fn new() -> Self {
        Self::default()
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
            .finish()
    }
}

impl Material for StandardMaterial {
    fn shader_source(&self) -> ShaderSource {
        ShaderSource::Wgsl(Cow::Borrowed(if self.normal_texture.is_some() {
            include_str!("../../shaders/standard_material_normal_mapped.wgsl")
        } else {
            include_str!("../../shaders/standard_material.wgsl")
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
            AlphaMode::Opaque => MaterialRenderState::opaque(),
            AlphaMode::Blend => MaterialRenderState::transparent(),
            AlphaMode::Additive => MaterialRenderState::additive(),
        }
    }

    fn scene_bindings(&self) -> Vec<SceneBindingDesc> {
        vec![SceneBindingDesc::shadow_view(3)]
    }
}
