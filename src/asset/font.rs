use std::sync::Arc;

use super::registry::AssetRuntimeFactory;
use super::types::{
    Asset, AssetError, AssetId, AssetInstallContext, AssetLoadContext, LoadedAsset,
};

const FONT_COOKED_MAGIC: &[u8; 8] = b"SKYFNT01";
const FONT_COOKED_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontAsset {
    bytes: Arc<[u8]>,
}

impl Asset for FontAsset {
    const TYPE: &'static str = "font";
}

impl FontAsset {
    #[must_use]
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

pub(crate) struct FontAssetFactory;

impl AssetRuntimeFactory for FontAssetFactory {
    type Asset = FontAsset;
    type Loaded = FontAsset;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let font = decode_font_cooked(ctx.asset_id, ctx.bytes)?;
        Ok(LoadedAsset::new(font).with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<Self::Asset, AssetError> {
        Ok(loaded.clone())
    }
}

pub(crate) fn encode_font_cooked(asset: &FontAsset) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + 4 + 8 + asset.bytes.len());
    bytes.extend_from_slice(FONT_COOKED_MAGIC);
    bytes.extend_from_slice(&FONT_COOKED_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(asset.bytes.len() as u64).to_le_bytes());
    bytes.extend_from_slice(asset.bytes());
    bytes
}

pub(crate) fn decode_font_cooked(asset_id: AssetId, bytes: &[u8]) -> Result<FontAsset, AssetError> {
    if bytes.len() < 20 || &bytes[..8] != FONT_COOKED_MAGIC {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: "missing SKYFNT01 header".to_string(),
        });
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().expect("slice length checked"));
    if version != FONT_COOKED_VERSION {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: format!(
                "unsupported cooked font version {version} (expected {FONT_COOKED_VERSION})"
            ),
        });
    }
    let data_len =
        u64::from_le_bytes(bytes[12..20].try_into().expect("slice length checked")) as usize;
    let payload = &bytes[20..];
    if payload.len() != data_len {
        return Err(AssetError::InvalidCookedAsset {
            id: Some(asset_id),
            message: format!(
                "font payload length mismatch: expected {data_len}, got {}",
                payload.len()
            ),
        });
    }
    Ok(FontAsset::new(Arc::<[u8]>::from(payload.to_vec())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooked_round_trip_preserves_font_bytes() {
        let original = FontAsset::new(Arc::<[u8]>::from(vec![0, 1, 2, 3, 4, 5]));
        let cooked = encode_font_cooked(&original);
        let decoded = decode_font_cooked(AssetId::new(), &cooked).expect("decode should work");
        assert_eq!(decoded, original);
    }
}
