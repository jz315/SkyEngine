use crate::render::{TileId, TiledImport, TiledTileset};
use crate::tile::{
    AssetSource, PaletteId, RectU, TileAtlasImageSource, TileDef, TileDefId, TilePalette,
    TileTextureSource,
};

use super::properties::property_bag_from_tiled;

pub(super) fn import_palettes(import: &TiledImport) -> Vec<TilePalette> {
    import
        .tilesets
        .iter()
        .map(tile_palette_from_tileset)
        .collect()
}

fn tile_palette_from_tileset(tileset: &TiledTileset) -> TilePalette {
    let mut palette = TilePalette::new(PaletteId(tileset.first_gid), "Tiled Tileset");
    palette.source = tileset
        .image
        .parent()
        .map(|path| AssetSource::new(path.to_path_buf()));
    palette.texture = texture_source_from_tileset(tileset);
    palette.properties = property_bag_from_tiled(&tileset.properties);

    for id in 0..tileset.tile_count {
        let rect = tileset_rect(tileset, TileId(id));
        let mut tile = TileDef::new(TileDefId(id), rect);
        tile.draw_size = tileset.tile_draw_size(TileId(id));
        tile.draw_offset = tileset.tile_offset;
        if let Some(properties) = tileset.tile_properties.get(id as usize) {
            tile.properties = property_bag_from_tiled(properties);
        }
        palette.tiles.push(tile);
    }
    palette
}

fn texture_source_from_tileset(tileset: &TiledTileset) -> TileTextureSource {
    if tileset.tile_images.is_empty() {
        return TileTextureSource::Image(tileset.image.clone());
    }

    let tiles = tileset
        .tile_images
        .iter()
        .enumerate()
        .filter_map(|(tile_id, source)| {
            let source = source.as_ref()?;
            Some(TileAtlasImageSource {
                tile: TileDefId(tile_id as u32),
                image: source.image.clone(),
                source_rect: RectU::new(
                    source.source_rect.x,
                    source.source_rect.y,
                    source.source_rect.width,
                    source.source_rect.height,
                ),
            })
        })
        .collect();
    TileTextureSource::ImageCollectionAtlas {
        size: tileset.image_size,
        tiles,
    }
}

fn tileset_rect(tileset: &TiledTileset, tile_id: TileId) -> RectU {
    if let Some(Some(rect)) = tileset.tile_rects.get(tile_id.0 as usize) {
        return RectU::new(rect.x, rect.y, rect.width, rect.height);
    }
    let column = tile_id.0 % tileset.columns.max(1);
    let row = tile_id.0 / tileset.columns.max(1);
    let stride_x = tileset.tile_size[0].saturating_add(tileset.spacing);
    let stride_y = tileset.tile_size[1].saturating_add(tileset.spacing);
    RectU::new(
        tileset
            .margin
            .saturating_add(column.saturating_mul(stride_x)),
        tileset.margin.saturating_add(row.saturating_mul(stride_y)),
        tileset.tile_size[0],
        tileset.tile_size[1],
    )
}
