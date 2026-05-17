use sky_engine::asset::{Handle, TextureAsset};
use sky_engine::render::TileId;
use sky_engine::tile::{PaletteId, TileDefId, TilePaletteStore};

pub const GROUND_PALETTE: PaletteId = PaletteId(1);
pub const STRUCTURE_PALETTE: PaletteId = PaletteId(100);

#[derive(Clone, Copy)]
pub struct SpriteAsset {
    pub handle: Handle<TextureAsset>,
    pub width: f32,
    pub height: f32,
    pub uv: [f32; 4],
}

#[derive(Clone, Copy)]
pub struct LoadedBlueprint {
    pub rotations: [SpriteAsset; 4],
}

#[derive(Clone)]
pub struct GameAssets {
    pub hover: SpriteAsset,
    pub blueprints: Vec<LoadedBlueprint>,
    pub ground_palettes: TilePaletteStore,
    pub structure_palettes: TilePaletteStore,
}

impl GameAssets {
    pub fn ground_tile_ref(&self, tile: TileId) -> sky_engine::tile::TileRef {
        sky_engine::tile::TileRef::new(GROUND_PALETTE, TileDefId(tile.0))
    }

    pub fn structure_tile_ref(
        &self,
        blueprint: usize,
        orientation: usize,
    ) -> sky_engine::tile::TileRef {
        sky_engine::tile::TileRef::new(
            STRUCTURE_PALETTE,
            TileDefId((blueprint * crate::model::ORIENTATIONS.len() + orientation) as u32),
        )
    }
}
