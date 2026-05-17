use std::path::PathBuf;

use crate::asset::{Handle, TextureAsset};
use crate::render::{TilesetGrid, TilesetTileRect};

use super::property::PropertyBag;

/// Runtime palette identifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaletteId(pub u32);

impl From<u32> for PaletteId {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// Runtime tile definition identifier local to one palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileDefId(pub u32);

impl From<u32> for TileDefId {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// Unsigned pixel rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RectU {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl RectU {
    #[inline]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    pub const fn size(self) -> [u32; 2] {
        [self.width, self.height]
    }
}

/// Optional source metadata for imported or generated assets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetSource {
    pub path: PathBuf,
}

impl AssetSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// Texture backing for a tile palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TileTextureSource {
    None,
    Image(PathBuf),
    ImageCollectionAtlas {
        size: [u32; 2],
        tiles: Vec<TileAtlasImageSource>,
    },
    Texture {
        handle: Handle<TextureAsset>,
        size: [u32; 2],
    },
}

impl Default for TileTextureSource {
    fn default() -> Self {
        Self::None
    }
}

/// Per-tile source image used to build a runtime image-collection atlas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileAtlasImageSource {
    pub tile: TileDefId,
    pub image: PathBuf,
    pub source_rect: RectU,
}

/// One frame in a runtime tile animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileAnimationFrame {
    pub tile: TileDefId,
    pub duration_ms: u32,
}

/// Runtime tile animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileAnimation {
    pub frames: Vec<TileAnimationFrame>,
}

/// Placeholder collision metadata for a tile definition.
#[derive(Clone, Debug, PartialEq)]
pub enum TileCollision {
    Solid,
    Rect(RectU),
    Polygon(Vec<[f32; 2]>),
}

/// One tile definition inside a palette.
#[derive(Clone, Debug)]
pub struct TileDef {
    pub id: TileDefId,
    pub name: Option<String>,
    pub source_rect: RectU,
    pub draw_size: [u32; 2],
    pub draw_offset: [i32; 2],
    pub animation: Option<TileAnimation>,
    pub collision: Option<TileCollision>,
    pub properties: PropertyBag,
}

impl TileDef {
    pub fn new(id: TileDefId, source_rect: RectU) -> Self {
        Self {
            id,
            name: None,
            source_rect,
            draw_size: source_rect.size(),
            draw_offset: [0, 0],
            animation: None,
            collision: None,
            properties: PropertyBag::new(),
        }
    }
}

/// Runtime tileset/palette model.
#[derive(Clone, Debug)]
pub struct TilePalette {
    pub id: PaletteId,
    pub name: String,
    pub source: Option<AssetSource>,
    pub texture: TileTextureSource,
    pub tiles: Vec<TileDef>,
    pub properties: PropertyBag,
}

impl TilePalette {
    pub fn new(id: PaletteId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            source: None,
            texture: TileTextureSource::None,
            tiles: Vec::new(),
            properties: PropertyBag::new(),
        }
    }

    pub fn tile(&self, id: TileDefId) -> Option<&TileDef> {
        self.tiles.iter().find(|tile| tile.id == id)
    }

    pub fn tileset_grid(&self) -> Option<TilesetGrid> {
        let TileTextureSource::Texture { handle, size } = self.texture else {
            return None;
        };
        let tile_count = self
            .tiles
            .iter()
            .map(|tile| tile.id.0)
            .max()
            .unwrap_or_default()
            .saturating_add(1)
            .max(1);
        let tile_size = self.tiles.first().map_or([1, 1], |tile| {
            [tile.draw_size[0].max(1), tile.draw_size[1].max(1)]
        });
        let mut rects = vec![None; tile_count as usize];
        for tile in &self.tiles {
            rects[tile.id.0 as usize] = Some(TilesetTileRect::new(
                tile.source_rect.x,
                tile.source_rect.y,
                tile.source_rect.width,
                tile.source_rect.height,
            ));
        }
        Some(
            TilesetGrid::new(handle, tile_size, tile_count, 1)
                .texture_size(size)
                .tile_rects(rects),
        )
    }
}
