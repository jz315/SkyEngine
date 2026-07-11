use crate::asset::{Asset, Handle, TextureAsset};
use crate::render::resources::material::AlphaMode;
use crate::render::Color;
/// Backend-neutral texture sampling metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureSamplerDesc {
    pub min_filter: TextureFilter,
    pub mag_filter: TextureFilter,
    pub mip_filter: TextureFilter,
    pub address_u: TextureAddressMode,
    pub address_v: TextureAddressMode,
}

impl Default for TextureSamplerDesc {
    fn default() -> Self {
        Self {
            min_filter: TextureFilter::Linear,
            mag_filter: TextureFilter::Linear,
            mip_filter: TextureFilter::Linear,
            address_u: TextureAddressMode::Repeat,
            address_v: TextureAddressMode::Repeat,
        }
    }
}

/// Backend-neutral texture filter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFilter {
    Nearest,
    Linear,
}

/// Backend-neutral texture address mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureAddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

/// Backend-neutral physically based material asset.
#[derive(Debug, Clone)]
pub struct StandardMaterialAsset {
    pub albedo: Color,
    pub albedo_texture: Option<Handle<TextureAsset>>,
    pub albedo_sampler: TextureSamplerDesc,
    pub metallic: f32,
    pub roughness: f32,
    pub normal_texture: Option<Handle<TextureAsset>>,
    pub normal_sampler: TextureSamplerDesc,
    pub emissive: Color,
    pub emissive_texture: Option<Handle<TextureAsset>>,
    pub emissive_sampler: TextureSamplerDesc,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub receive_shadows: bool,
}

impl Asset for StandardMaterialAsset {
    const TYPE: &'static str = "standard_material";
}

impl StandardMaterialAsset {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn albedo(mut self, color: Color) -> Self {
        self.albedo = color;
        self
    }

    #[inline]
    pub fn albedo_texture(mut self, texture: Handle<TextureAsset>) -> Self {
        self.albedo_texture = Some(texture);
        self
    }

    #[inline]
    pub fn normal_texture(mut self, texture: Handle<TextureAsset>) -> Self {
        self.normal_texture = Some(texture);
        self
    }

    #[inline]
    pub fn emissive(mut self, color: Color) -> Self {
        self.emissive = color;
        self
    }

    #[inline]
    pub fn emissive_texture(mut self, texture: Handle<TextureAsset>) -> Self {
        self.emissive_texture = Some(texture);
        self
    }

    #[inline]
    pub fn metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic;
        self
    }

    #[inline]
    pub fn roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness;
        self
    }

    #[inline]
    pub fn alpha_mode(mut self, alpha_mode: AlphaMode) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[inline]
    pub fn alpha_cutoff(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub fn alpha_mask(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_mode = AlphaMode::Mask;
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub fn receive_shadows(mut self, receive_shadows: bool) -> Self {
        self.receive_shadows = receive_shadows;
        self
    }
}

impl Default for StandardMaterialAsset {
    fn default() -> Self {
        Self {
            albedo: Color::WHITE,
            albedo_texture: None,
            albedo_sampler: TextureSamplerDesc::default(),
            metallic: 0.0,
            roughness: 0.8,
            normal_texture: None,
            normal_sampler: TextureSamplerDesc::default(),
            emissive: Color::BLACK,
            emissive_texture: None,
            emissive_sampler: TextureSamplerDesc::default(),
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            receive_shadows: true,
        }
    }
}
