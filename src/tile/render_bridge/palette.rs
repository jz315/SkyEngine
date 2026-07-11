use crate::render::features::tilemap::{TilesetGrid, TilesetTileRect};
use crate::tile::{TilePalette, TileTextureSource};

#[allow(dead_code)]
pub(crate) fn palette_to_tileset_grid(palette: &TilePalette) -> Option<TilesetGrid> {
    let TileTextureSource::Texture { handle, size } = &palette.texture else {
        return None;
    };
    let tile_count = palette
        .tiles
        .iter()
        .map(|tile| tile.id.0)
        .max()
        .unwrap_or_default()
        .saturating_add(1)
        .max(1);
    let tile_size = palette.tiles.first().map_or([1, 1], |tile| {
        [tile.draw_size[0].max(1), tile.draw_size[1].max(1)]
    });
    let mut rects = vec![None; tile_count as usize];
    for tile in &palette.tiles {
        rects[tile.id.0 as usize] = Some(TilesetTileRect::new(
            tile.source_rect.x,
            tile.source_rect.y,
            tile.source_rect.width,
            tile.source_rect.height,
        ));
    }
    Some(
        TilesetGrid::new(handle.clone(), tile_size, tile_count, 1)
            .texture_size(*size)
            .tile_rects(rects),
    )
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetConfig, Assets, TextureAsset};
    use crate::render::features::tilemap::TilesetTileRect;
    use crate::tile::{PaletteId, RectU, TileDef, TileDefId, TilePalette, TileTextureSource};

    use super::palette_to_tileset_grid;

    #[test]
    fn builds_render_grid_from_runtime_palette() {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let texture = assets.insert_runtime(TextureAsset::white_pixel());
        let mut palette = TilePalette::new(PaletteId(7), "test");
        palette.texture = TileTextureSource::Texture {
            handle: texture,
            size: [64, 32],
        };
        palette
            .tiles
            .push(TileDef::new(TileDefId(0), RectU::new(0, 0, 16, 16)));
        palette
            .tiles
            .push(TileDef::new(TileDefId(1), RectU::new(16, 0, 16, 16)));

        let grid = palette_to_tileset_grid(&palette).expect("texture-backed palette");

        assert_eq!(grid.tile_count(), 2);
        assert_eq!(grid.texture_size, [64, 32]);
        assert_eq!(
            grid.tile_rects[1],
            Some(TilesetTileRect::new(16, 0, 16, 16))
        );
    }
}
