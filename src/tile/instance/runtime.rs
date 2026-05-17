use std::fmt;
use std::path::Path;

use crate::asset::{AssetServer, Handle, TextureAsset, TextureColorSpace};
use crate::ecs::{EntityId, World};
use crate::render::{SortingLayer, TilemapStorage, Transform};

use super::super::document::TileMapDocument;
use super::super::palette::{
    RectU, TileAtlasImageSource, TilePalette, TilePaletteStore, TileTextureSource,
};
use super::super::scene::TileMap;
use super::super::sync::{TileMapRenderLayer, TileMapRenderSync, TileMapRenderSyncError};

/// Placement policy used when spawning a tile scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileMapSpawnOrigin {
    Position([f32; 2]),
}

/// Options for turning a [`TileMap`] into render entities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileMapSpawnOptions {
    pub origin: TileMapSpawnOrigin,
    pub unload_textures_on_despawn: bool,
}

impl TileMapSpawnOptions {
    pub const fn at(position: [f32; 2]) -> Self {
        Self {
            origin: TileMapSpawnOrigin::Position(position),
            unload_textures_on_despawn: true,
        }
    }
}

impl Default for TileMapSpawnOptions {
    fn default() -> Self {
        Self::at([0.0, 0.0])
    }
}

/// Runtime ECS entities/resources created for one tile scene.
#[derive(Debug)]
pub struct TileMapInstance {
    pub entities: Vec<EntityId>,
    pub textures: Vec<Handle<TextureAsset>>,
    pub scene: crate::render::TilemapHandle,
    pub layers: Vec<TileMapRenderLayer>,
    pub(super) palettes: TilePaletteStore,
    pub(super) origin: [f32; 2],
    unload_textures_on_despawn: bool,
}

impl TileMapInstance {
    pub fn spawn(
        world: &mut World,
        scene: &TileMap,
        palettes: &TilePaletteStore,
        options: TileMapSpawnOptions,
    ) -> Result<Self, TileMapInstanceError> {
        Self::spawn_with_palettes(world, scene, palettes, Vec::new(), options)
    }

    pub fn spawn_document(
        world: &mut World,
        document: &TileMapDocument,
        options: TileMapSpawnOptions,
    ) -> Result<Self, TileMapInstanceError> {
        Self::spawn(world, &document.scene, &document.palettes, options)
    }

    fn spawn_with_palettes(
        world: &mut World,
        scene: &TileMap,
        palettes: &TilePaletteStore,
        mut textures: Vec<Handle<TextureAsset>>,
        options: TileMapSpawnOptions,
    ) -> Result<Self, TileMapInstanceError> {
        let asset_server = world
            .get_resource::<AssetServer>()
            .cloned()
            .ok_or(TileMapInstanceError::MissingAssetServer)?;

        let mut resolved_palettes = TilePaletteStore::new();
        for palette in palettes.iter().cloned() {
            let mut palette = palette;
            if let Some(texture) = load_palette_texture(&palette)? {
                let size = texture.size();
                let handle = asset_server.insert_runtime(texture);
                palette.texture = TileTextureSource::Texture { handle, size };
                textures.push(handle);
            }
            resolved_palettes.insert(palette);
        }

        if world.get_resource::<TilemapStorage>().is_none() {
            world.insert_resource(TilemapStorage::new());
        }
        let (map, layers) = {
            let storage = world
                .get_resource_mut::<TilemapStorage>()
                .expect("TilemapStorage was just inserted");
            TileMapRenderSync::insert(scene, &resolved_palettes, storage)?
        };

        let TileMapSpawnOrigin::Position(origin) = options.origin;
        let entities = spawn_render_entities(world, scene, &layers, origin);

        Ok(Self {
            entities,
            textures,
            scene: map,
            layers,
            palettes: resolved_palettes,
            origin,
            unload_textures_on_despawn: options.unload_textures_on_despawn,
        })
    }

    pub fn despawn(self, world: &mut World) {
        for entity in self.entities {
            let _ = world.despawn(entity);
        }
        if let Some(storage) = world.get_resource_mut::<TilemapStorage>() {
            let _ = storage.remove(self.scene);
        }
        if self.unload_textures_on_despawn {
            if let Some(asset_server) = world.get_resource::<AssetServer>().cloned() {
                for texture in &self.textures {
                    asset_server.unload(texture);
                }
            }
        }
    }
}

pub(super) fn spawn_render_entities(
    world: &mut World,
    scene: &TileMap,
    layers: &[TileMapRenderLayer],
    origin: [f32; 2],
) -> Vec<EntityId> {
    let mut entities = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        let source_layer = scene.layer(layer.source_layer);
        let offset = source_layer.map_or([0.0, 0.0], |layer| layer.offset);
        let transform = Transform::from_xyz(origin[0] + offset[0], origin[1] + offset[1], 0.0);
        let sorting_layer = SortingLayer(index as i32 * 256);
        entities.push(world.spawn((transform, layer.renderer.clone(), sorting_layer)));
    }
    entities
}

#[derive(Debug)]
pub enum TileMapInstanceError {
    MissingAssetServer,
    MissingTilemapStorage,
    RenderSync(TileMapRenderSyncError),
    InvalidAtlasSource {
        path: std::path::PathBuf,
        reason: &'static str,
    },
    Image {
        path: std::path::PathBuf,
        source: image::ImageError,
    },
}

impl fmt::Display for TileMapInstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAssetServer => write!(f, "AssetServer resource is missing"),
            Self::MissingTilemapStorage => write!(f, "TilemapStorage resource is missing"),
            Self::RenderSync(error) => error.fmt(f),
            Self::InvalidAtlasSource { path, reason } => {
                write!(
                    f,
                    "invalid tile scene atlas source {}: {reason}",
                    path.display()
                )
            }
            Self::Image { path, source } => {
                write!(
                    f,
                    "failed to load tile scene image {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for TileMapInstanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RenderSync(error) => Some(error),
            Self::Image { source, .. } => Some(source),
            Self::MissingAssetServer
            | Self::MissingTilemapStorage
            | Self::InvalidAtlasSource { .. } => None,
        }
    }
}

impl From<TileMapRenderSyncError> for TileMapInstanceError {
    fn from(value: TileMapRenderSyncError) -> Self {
        Self::RenderSync(value)
    }
}

fn load_texture(path: &Path) -> Result<TextureAsset, TileMapInstanceError> {
    let image = image::open(path).map_err(|source| TileMapInstanceError::Image {
        path: path.to_path_buf(),
        source,
    })?;
    let image = image.to_rgba8();
    let (width, height) = image.dimensions();
    Ok(TextureAsset::new(
        width,
        height,
        TextureColorSpace::Srgb,
        image.into_raw(),
    ))
}

fn load_palette_texture(
    palette: &TilePalette,
) -> Result<Option<TextureAsset>, TileMapInstanceError> {
    match &palette.texture {
        TileTextureSource::Image(path) => load_texture(path).map(Some),
        TileTextureSource::ImageCollectionAtlas { size, tiles } => {
            load_image_collection_atlas(palette, *size, tiles).map(Some)
        }
        TileTextureSource::None | TileTextureSource::Texture { .. } => Ok(None),
    }
}

fn load_image_collection_atlas(
    palette: &TilePalette,
    size: [u32; 2],
    tiles: &[TileAtlasImageSource],
) -> Result<TextureAsset, TileMapInstanceError> {
    let atlas_size = [size[0].max(1), size[1].max(1)];
    let mut atlas_pixels = vec![0u8; (atlas_size[0] * atlas_size[1] * 4) as usize];
    for source in tiles {
        let tile =
            palette
                .tile(source.tile)
                .ok_or_else(|| TileMapInstanceError::InvalidAtlasSource {
                    path: source.image.clone(),
                    reason: "source references a missing tile definition",
                })?;
        let image = image::open(&source.image)
            .map_err(|error| TileMapInstanceError::Image {
                path: source.image.clone(),
                source: error,
            })?
            .to_rgba8();
        blit_image_rect(
            &image,
            source.source_rect,
            &mut atlas_pixels,
            atlas_size,
            tile.source_rect,
            &source.image,
        )?;
    }

    Ok(TextureAsset::new(
        atlas_size[0],
        atlas_size[1],
        TextureColorSpace::Srgb,
        atlas_pixels,
    ))
}

fn blit_image_rect(
    image: &image::RgbaImage,
    source_rect: RectU,
    atlas_pixels: &mut [u8],
    atlas_size: [u32; 2],
    atlas_rect: RectU,
    image_path: &Path,
) -> Result<(), TileMapInstanceError> {
    if source_rect.x.saturating_add(source_rect.width) > image.width()
        || source_rect.y.saturating_add(source_rect.height) > image.height()
    {
        return Err(TileMapInstanceError::InvalidAtlasSource {
            path: image_path.to_path_buf(),
            reason: "source rectangle is outside the source image",
        });
    }
    if atlas_rect.x.saturating_add(atlas_rect.width) > atlas_size[0]
        || atlas_rect.y.saturating_add(atlas_rect.height) > atlas_size[1]
        || atlas_rect.width != source_rect.width
        || atlas_rect.height != source_rect.height
    {
        return Err(TileMapInstanceError::InvalidAtlasSource {
            path: image_path.to_path_buf(),
            reason: "atlas rectangle is invalid",
        });
    }

    let source_row_stride = image.width() as usize * 4;
    let atlas_row_stride = atlas_size[0] as usize * 4;
    let pixels = image.as_raw();
    let copy_len = source_rect.width as usize * 4;
    for row in 0..source_rect.height {
        let source_start =
            ((source_rect.y + row) as usize * source_row_stride) + source_rect.x as usize * 4;
        let source_end = source_start + copy_len;
        let atlas_start =
            ((atlas_rect.y + row) as usize * atlas_row_stride) + atlas_rect.x as usize * 4;
        let atlas_end = atlas_start + copy_len;
        atlas_pixels[atlas_start..atlas_end].copy_from_slice(&pixels[source_start..source_end]);
    }
    Ok(())
}
