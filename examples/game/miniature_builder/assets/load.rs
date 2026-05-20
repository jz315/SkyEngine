use std::path::{Path, PathBuf};

use image::{ImageReader, RgbaImage};
use sky_engine::asset::{Assets, TextureAsset, TextureColorSpace};
use sky_engine::ecs::World;
use sky_engine::tile::{
    RectU, TileDef, TileDefId, TilePalette, TilePaletteStore, TileTextureSource,
};

use super::types::{GameAssets, LoadedBlueprint, SpriteAsset, GROUND_PALETTE, STRUCTURE_PALETTE};
use crate::geometry::{ASSET_SCALE, TILE_DRAW_PIXEL_H, TILE_PIXEL_W};
use crate::model::{BLUEPRINTS, ORIENTATIONS};

impl GameAssets {
    pub fn load(world: &World) -> Self {
        let server = world
            .get_resource::<Assets>()
            .expect("App should install Assets before setup")
            .clone();
        let root = kenney_asset_root().join("Isometric");
        let ground_palettes = load_ground_palette(
            &server,
            [
                root.join("planks_W.png"),
                root.join("planks_E.png"),
                root.join("planks_N.png"),
                root.join("planks_S.png"),
            ],
        );
        let hover = load_sprite_asset(&server, root.join("planks_W.png"));
        let blueprints = BLUEPRINTS
            .iter()
            .copied()
            .map(|def| {
                let rotations = ORIENTATIONS.map(|suffix| {
                    load_sprite_asset(&server, root.join(format!("{}_{}.png", def.stem, suffix)))
                });
                let _ = def;
                LoadedBlueprint { rotations }
            })
            .collect();
        let structure_palettes = load_structure_palette(&server, &root);

        Self {
            hover,
            blueprints,
            ground_palettes,
            structure_palettes,
        }
    }
}

pub fn load_sprite_asset(server: &Assets, path: impl AsRef<Path>) -> SpriteAsset {
    let path = path.as_ref();
    let image = open_rgba(path);
    let (width, height) = image.dimensions();
    let handle = server.insert_runtime(TextureAsset::new(
        width,
        height,
        TextureColorSpace::Srgb,
        image.into_raw(),
    ));
    SpriteAsset {
        handle,
        width: width as f32 * ASSET_SCALE,
        height: height as f32 * ASSET_SCALE,
        uv: [0.0, 0.0, 1.0, 1.0],
    }
}

fn load_ground_palette(server: &Assets, paths: [PathBuf; 4]) -> TilePaletteStore {
    let mut images = Vec::with_capacity(paths.len());
    let mut atlas_width = 0;
    let mut atlas_height = 0;

    for path in paths {
        let image = open_rgba(&path);
        debug_assert_eq!(image.width(), TILE_PIXEL_W);
        debug_assert_eq!(image.height(), TILE_DRAW_PIXEL_H);
        atlas_width += TILE_PIXEL_W;
        atlas_height = atlas_height.max(TILE_DRAW_PIXEL_H);
        images.push(image);
    }

    let mut atlas = RgbaImage::new(atlas_width.max(1), atlas_height.max(1));
    let mut x = 0;
    let mut palette = TilePalette::new(GROUND_PALETTE, "Miniature ground");
    for image in images {
        image::imageops::overlay(&mut atlas, &image, x.into(), 0);
        let rect = RectU::new(x, 0, image.width(), image.height());
        let mut tile = TileDef::new(TileDefId(palette.tiles.len() as u32), rect);
        tile.draw_size = [image.width(), image.height()];
        tile.draw_offset = [0, 0];
        palette.tiles.push(tile);
        x += image.width();
    }

    let handle = server.insert_runtime(TextureAsset::new(
        atlas.width(),
        atlas.height(),
        TextureColorSpace::Srgb,
        atlas.into_raw(),
    ));

    palette.texture = TileTextureSource::Texture {
        handle,
        size: [atlas_width.max(1), atlas_height.max(1)],
    };

    let mut store = TilePaletteStore::new();
    store.insert(palette);
    store
}

fn load_structure_palette(server: &Assets, root: &Path) -> TilePaletteStore {
    const MAX_ATLAS_WIDTH: u32 = 4096;

    let entries = BLUEPRINTS
        .iter()
        .flat_map(|def| {
            ORIENTATIONS
                .iter()
                .map(move |suffix| root.join(format!("{}_{}.png", def.stem, suffix)))
        })
        .map(|path| open_rgba(&path))
        .collect::<Vec<_>>();

    let mut placements = Vec::with_capacity(entries.len());
    let mut cursor_x = 0u32;
    let mut cursor_y = 0u32;
    let mut row_height = 0u32;
    let mut atlas_width = 1u32;
    for image in &entries {
        if cursor_x > 0 && cursor_x.saturating_add(image.width()) > MAX_ATLAS_WIDTH {
            cursor_y = cursor_y.saturating_add(row_height);
            cursor_x = 0;
            row_height = 0;
        }
        placements.push([cursor_x, cursor_y]);
        atlas_width = atlas_width.max(cursor_x.saturating_add(image.width()));
        cursor_x = cursor_x.saturating_add(image.width());
        row_height = row_height.max(image.height());
    }
    let atlas_height = cursor_y.saturating_add(row_height).max(1);

    let mut atlas = RgbaImage::new(atlas_width.max(1), atlas_height.max(1));
    let mut palette = TilePalette::new(STRUCTURE_PALETTE, "Miniature structures");
    for (index, image) in entries.iter().enumerate() {
        let [x, y] = placements[index];
        image::imageops::overlay(&mut atlas, image, x.into(), y.into());
        let rect = RectU::new(x, y, image.width(), image.height());
        let mut tile = TileDef::new(TileDefId(index as u32), rect);
        tile.draw_size = [image.width(), image.height()];
        tile.draw_offset = [0, 0];
        palette.tiles.push(tile);
    }

    let handle = server.insert_runtime(TextureAsset::new(
        atlas.width(),
        atlas.height(),
        TextureColorSpace::Srgb,
        atlas.into_raw(),
    ));
    palette.texture = TileTextureSource::Texture {
        handle,
        size: [atlas_width.max(1), atlas_height.max(1)],
    };

    let mut store = TilePaletteStore::new();
    store.insert(palette);
    store
}

fn open_rgba(path: &Path) -> RgbaImage {
    ImageReader::open(path)
        .unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()))
        .decode()
        .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()))
        .to_rgba8()
}

fn kenney_asset_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("kenney_isometric_miniature_builder")
}
