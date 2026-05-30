use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::install::{AssetInstallContext, AssetInstallResult};
use super::registry::AssetRuntimeFactory;
use super::types::{Asset, AssetCookedSchema, AssetError, AssetId, AssetLoadContext, LoadedAsset};

const TEXTURE_COOKED_MAGIC: &[u8; 8] = b"SKYTEX01";
const TEXTURE_COOKED_VERSION: u32 = 2;

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
    visible_rect: [u32; 4],
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
            visible_rect: [0, 0, width, height],
            pixels: pixels.into(),
        }
    }

    #[must_use]
    pub fn with_visible_rect(
        width: u32,
        height: u32,
        color_space: TextureColorSpace,
        pixels: impl Into<Arc<[u8]>>,
        visible_rect: [u32; 4],
    ) -> Self {
        Self {
            width,
            height,
            color_space,
            visible_rect: clamp_visible_rect([width, height], visible_rect),
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
    pub fn size(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    #[must_use]
    pub fn visible_rect(&self) -> [u32; 4] {
        self.visible_rect
    }

    #[must_use]
    pub fn visible_size(&self) -> [u32; 2] {
        [self.visible_rect[2], self.visible_rect[3]]
    }

    #[must_use]
    pub fn visible_uv_rect(&self) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0, 0.0, 1.0, 1.0];
        }
        let width = self.width as f32;
        let height = self.height as f32;
        [
            self.visible_rect[0] as f32 / width,
            self.visible_rect[1] as f32 / height,
            (self.visible_rect[0] + self.visible_rect[2]) as f32 / width,
            (self.visible_rect[1] + self.visible_rect[3]) as f32 / height,
        ]
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub fn white_pixel() -> Self {
        Self::new(
            1,
            1,
            TextureColorSpace::Srgb,
            Arc::<[u8]>::from([255, 255, 255, 255]),
        )
    }

    #[must_use]
    pub fn checkerboard(size: u32, tile_size: u32, color_a: [u8; 4], color_b: [u8; 4]) -> Self {
        let mut data = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let is_a = ((x / tile_size) + (y / tile_size)) % 2 == 0;
                let color = if is_a { color_a } else { color_b };
                let i = ((y * size + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&color);
            }
        }
        Self::new(size, size, TextureColorSpace::Srgb, data)
    }

    #[must_use]
    pub fn circle(size: u32) -> Self {
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = size as f32 * 0.5;
        let radius = center - 1.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                let dist = (dx * dx + dy * dy).sqrt();
                let alpha = ((radius - dist).max(0.0).min(1.0) * 255.0) as u8;
                let i = ((y * size + x) * 4) as usize;
                data[i] = 255;
                data[i + 1] = 255;
                data[i + 2] = 255;
                data[i + 3] = alpha;
            }
        }
        Self::new(size, size, TextureColorSpace::Srgb, data)
    }
}

pub(crate) struct TextureAssetFactory;

impl AssetRuntimeFactory for TextureAssetFactory {
    type Asset = TextureAsset;
    type Loaded = TextureAsset;

    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        Some(AssetCookedSchema::new("texture.rgba8", 1))
    }

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let texture = decode_texture_cooked(ctx.asset_id, ctx.bytes)?;
        Ok(LoadedAsset::new(texture).with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(loaded.clone()))
    }
}

pub(crate) fn encode_texture_cooked(asset: &TextureAsset) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + 4 + 4 + 4 + 1 + 16 + 8 + asset.pixels.len());
    bytes.extend_from_slice(TEXTURE_COOKED_MAGIC);
    bytes.extend_from_slice(&TEXTURE_COOKED_VERSION.to_le_bytes());
    bytes.extend_from_slice(&asset.width.to_le_bytes());
    bytes.extend_from_slice(&asset.height.to_le_bytes());
    bytes.push(match asset.color_space {
        TextureColorSpace::Linear => 0,
        TextureColorSpace::Srgb => 1,
    });
    for value in asset.visible_rect {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
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
    if !(1..=TEXTURE_COOKED_VERSION).contains(&version) {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: format!(
                "unsupported cooked texture version {version} (expected 1..={TEXTURE_COOKED_VERSION})"
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
    let mut offset = 21usize;
    let visible_rect = if version >= 2 {
        if bytes.len() < offset + 16 + 8 {
            return Err(AssetError::InvalidCookedAsset {
                id: Some(asset_id),
                message: "truncated cooked texture v2 metadata".to_string(),
            });
        }
        let rect = [
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("slice checked")),
            u32::from_le_bytes(
                bytes[offset + 4..offset + 8]
                    .try_into()
                    .expect("slice checked"),
            ),
            u32::from_le_bytes(
                bytes[offset + 8..offset + 12]
                    .try_into()
                    .expect("slice checked"),
            ),
            u32::from_le_bytes(
                bytes[offset + 12..offset + 16]
                    .try_into()
                    .expect("slice checked"),
            ),
        ];
        offset += 16;
        rect
    } else {
        [0, 0, width, height]
    };
    let data_len = u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("slice length checked"),
    ) as usize;
    let payload = &bytes[offset + 8..];

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

    Ok(TextureAsset::with_visible_rect(
        width,
        height,
        color_space,
        Arc::<[u8]>::from(payload.to_vec()),
        visible_rect,
    ))
}

pub(crate) fn decode_texture_source_bytes(
    path: &std::path::Path,
    bytes: &[u8],
    color_space: TextureColorSpace,
) -> Result<TextureAsset, AssetError> {
    let image = image::load_from_memory(bytes).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let has_alpha = image.color().has_alpha();
    let image = image.to_rgba8();
    let (width, height) = image.dimensions();
    let visible_rect = if has_alpha {
        alpha_visible_rect(&image)
    } else {
        [0, 0, width, height]
    };
    Ok(TextureAsset::with_visible_rect(
        width,
        height,
        color_space,
        image.into_raw(),
        visible_rect,
    ))
}

fn alpha_visible_rect(image: &image::RgbaImage) -> [u32; 4] {
    let (width, height) = image.dimensions();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;
    let row_stride = width as usize * 4;
    let pixels = image.as_raw();

    for y in 0..height {
        let row_start = y as usize * row_stride;
        let row = &pixels[row_start..row_start + row_stride];
        let first = row.chunks_exact(4).position(|pixel| pixel[3] != 0);
        let Some(first) = first else {
            continue;
        };
        let last = row
            .chunks_exact(4)
            .rposition(|pixel| pixel[3] != 0)
            .expect("first non-transparent pixel was found");
        let first = first as u32;
        let last = last as u32;
        found = true;
        min_x = min_x.min(first);
        min_y = min_y.min(y);
        max_x = max_x.max(last);
        max_y = max_y.max(y);
    }

    if found {
        [min_x, min_y, max_x - min_x + 1, max_y - min_y + 1]
    } else {
        [0, 0, width, height]
    }
}

fn clamp_visible_rect(size: [u32; 2], rect: [u32; 4]) -> [u32; 4] {
    let x = rect[0].min(size[0]);
    let y = rect[1].min(size[1]);
    let width = rect[2].min(size[0].saturating_sub(x));
    let height = rect[3].min(size[1].saturating_sub(y));
    [x, y, width, height]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooked_round_trip_preserves_texture_data() {
        let original = TextureAsset::with_visible_rect(
            2,
            1,
            TextureColorSpace::Srgb,
            Arc::<[u8]>::from(vec![1, 2, 3, 4, 5, 6, 7, 8]),
            [1, 0, 1, 1],
        );

        let cooked = encode_texture_cooked(&original);
        let decoded = decode_texture_cooked(AssetId::new(), &cooked).expect("decode should work");
        assert_eq!(decoded, original);
    }

    #[test]
    fn source_decode_records_alpha_visible_rect() {
        let mut rgba = image::RgbaImage::new(3, 2);
        rgba.put_pixel(1, 0, image::Rgba([255, 0, 0, 255]));
        rgba.put_pixel(2, 1, image::Rgba([0, 255, 0, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();

        let texture = decode_texture_source_bytes(
            std::path::Path::new("pose.png"),
            bytes.get_ref(),
            TextureColorSpace::Srgb,
        )
        .unwrap();
        assert_eq!(texture.size(), [3, 2]);
        assert_eq!(texture.visible_rect(), [1, 0, 2, 2]);
        assert_eq!(texture.visible_size(), [2, 2]);
    }
}
