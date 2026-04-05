use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::registry::AssetRuntimeFactory;
use super::types::{
    Asset, AssetError, AssetId, AssetInstallContext, AssetLoadContext, LoadedAsset,
};

const TEXTURE_COOKED_MAGIC: &[u8; 8] = b"SKYTEX01";
const TEXTURE_COOKED_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureColorSpace {
    Linear,
    Srgb,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureAsset {
    width: u32,
    height: u32,
    color_space: TextureColorSpace,
    pixels: Arc<[u8]>,
}

impl Asset for TextureAsset {
    const TYPE: &'static str = "texture";
}

impl TextureAsset {
    #[must_use]
    pub fn new(
        width: u32,
        height: u32,
        color_space: TextureColorSpace,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Self {
        Self {
            width,
            height,
            color_space,
            pixels: pixels.into(),
        }
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn color_space(&self) -> TextureColorSpace {
        self.color_space
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[cfg(feature = "app")]
    pub fn to_texture(&self, ctx: &crate::gpu::GpuContext) -> crate::render::Texture {
        let format = match self.color_space {
            TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
            TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        };

        crate::render::Texture::from_rgba8_with_format(
            ctx,
            self.width,
            self.height,
            self.pixels(),
            format,
            "asset_texture",
        )
    }
}

pub(crate) struct TextureAssetFactory;

impl AssetRuntimeFactory for TextureAssetFactory {
    type Asset = TextureAsset;
    type Loaded = TextureAsset;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let texture = decode_texture_cooked(ctx.asset_id, ctx.bytes)?;
        Ok(LoadedAsset::new(texture).with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<Self::Asset, AssetError> {
        Ok(loaded.clone())
    }
}

pub(crate) fn encode_texture_cooked(asset: &TextureAsset) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + 4 + 4 + 4 + 1 + 8 + asset.pixels.len());
    bytes.extend_from_slice(TEXTURE_COOKED_MAGIC);
    bytes.extend_from_slice(&TEXTURE_COOKED_VERSION.to_le_bytes());
    bytes.extend_from_slice(&asset.width.to_le_bytes());
    bytes.extend_from_slice(&asset.height.to_le_bytes());
    bytes.push(match asset.color_space {
        TextureColorSpace::Linear => 0,
        TextureColorSpace::Srgb => 1,
    });
    bytes.extend_from_slice(&(asset.pixels.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&asset.pixels);
    bytes
}

pub(crate) fn decode_texture_cooked(
    asset_id: AssetId,
    bytes: &[u8],
) -> Result<TextureAsset, AssetError> {
    if bytes.len() < 29 || &bytes[..8] != TEXTURE_COOKED_MAGIC {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: "missing SKYTEX01 header".to_string(),
        });
    }

    let version = u32::from_le_bytes(bytes[8..12].try_into().expect("slice length checked"));
    if version != TEXTURE_COOKED_VERSION {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: format!(
                "unsupported cooked texture version {version} (expected {TEXTURE_COOKED_VERSION})"
            ),
        });
    }

    let width = u32::from_le_bytes(bytes[12..16].try_into().expect("slice length checked"));
    let height = u32::from_le_bytes(bytes[16..20].try_into().expect("slice length checked"));
    let color_space = match bytes[20] {
        0 => TextureColorSpace::Linear,
        1 => TextureColorSpace::Srgb,
        other => {
            return Err(AssetError::InvalidCookedAsset {
                id: Some(asset_id),
                message: format!("unknown texture color space tag {other}"),
            });
        }
    };
    let data_len =
        u64::from_le_bytes(bytes[21..29].try_into().expect("slice length checked")) as usize;
    let payload = &bytes[29..];

    if payload.len() != data_len {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: format!(
                "texture payload length mismatch: expected {data_len}, got {}",
                payload.len()
            ),
        });
    }

    let expected = width as usize * height as usize * 4;
    if payload.len() != expected {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: format!(
                "texture RGBA payload mismatch: expected {expected}, got {}",
                payload.len()
            ),
        });
    }

    Ok(TextureAsset::new(
        width,
        height,
        color_space,
        Arc::<[u8]>::from(payload.to_vec()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooked_round_trip_preserves_texture_data() {
        let original = TextureAsset::new(
            2,
            1,
            TextureColorSpace::Srgb,
            Arc::<[u8]>::from(vec![1, 2, 3, 4, 5, 6, 7, 8]),
        );

        let cooked = encode_texture_cooked(&original);
        let decoded = decode_texture_cooked(AssetId::new(), &cooked).expect("decode should work");
        assert_eq!(decoded, original);
    }
}
