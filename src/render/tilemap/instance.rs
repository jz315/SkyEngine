use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::asset::{AssetServer, Handle, TextureAsset};
use crate::ecs::{EntityId, World};
use crate::render::component::{
    SortingLayer, SpriteAnimationClip, SpriteAnimationFrame, SpriteAnimator, SpriteRenderer,
    TileAnimation, TilemapRenderer, TilesetGrid, Transform,
};
use crate::render::Color;

use super::{
    TileFlags, TileId, TiledImport, TiledImportError, TiledObjectShape, TilemapHandle,
    TilemapStorage,
};

const PARALLAX_TILE_OBJECT_OVERLAP_X: f32 = 1.0;

/// Placement policy used when spawning a Tiled map into a world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TiledSpawnOrigin {
    /// Place the visual bounds of the imported map around world origin.
    Centered,
    /// Place the imported map at an explicit world-space origin.
    Position([f32; 2]),
}

/// Options for turning a [`TiledImport`] into render entities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TiledSpawnOptions {
    pub origin: TiledSpawnOrigin,
    pub spawn_tile_objects: bool,
    pub parallax: bool,
}

impl TiledSpawnOptions {
    #[inline]
    pub const fn centered() -> Self {
        Self {
            origin: TiledSpawnOrigin::Centered,
            spawn_tile_objects: true,
            parallax: true,
        }
    }

    #[inline]
    pub const fn at(position: [f32; 2]) -> Self {
        Self {
            origin: TiledSpawnOrigin::Position(position),
            spawn_tile_objects: true,
            parallax: true,
        }
    }

    #[inline]
    pub const fn with_tile_objects(mut self, spawn_tile_objects: bool) -> Self {
        self.spawn_tile_objects = spawn_tile_objects;
        self
    }

    #[inline]
    pub const fn with_parallax(mut self, parallax: bool) -> Self {
        self.parallax = parallax;
        self
    }
}

impl Default for TiledSpawnOptions {
    #[inline]
    fn default() -> Self {
        Self::centered()
    }
}

/// Entities and runtime assets created for one spawned Tiled map.
#[derive(Debug)]
pub struct TiledMapInstance {
    pub map: TilemapHandle,
    pub texture: Handle<TextureAsset>,
    pub textures: Vec<Handle<TextureAsset>>,
    pub entities: Vec<EntityId>,
    origin: [f32; 2],
    parallax_origin: [f32; 2],
    parallax_items: Vec<TiledMapParallaxItem>,
    runtime_animation_clips: Vec<Handle<SpriteAnimationClip>>,
    unload_texture_on_despawn: bool,
}

#[derive(Clone, Copy, Debug)]
struct TiledMapParallaxItem {
    entity: EntityId,
    base_position: [f32; 2],
    factor: [f32; 2],
}

impl TiledMapInstance {
    pub fn spawn(
        world: &mut World,
        path: impl AsRef<Path>,
        options: TiledSpawnOptions,
    ) -> Result<Self, TiledMapInstanceError> {
        let import = TiledImport::from_file(path)?;
        Self::spawn_import(world, &import, options)
    }

    pub fn spawn_import(
        world: &mut World,
        import: &TiledImport,
        options: TiledSpawnOptions,
    ) -> Result<Self, TiledMapInstanceError> {
        let asset_server = world
            .get_resource::<AssetServer>()
            .cloned()
            .ok_or(TiledMapInstanceError::MissingAssetServer)?;
        let textures = import
            .load_tileset_textures()?
            .into_iter()
            .map(|texture| asset_server.insert_runtime(texture))
            .collect::<Vec<_>>();
        Self::spawn_import_with_textures_internal(world, import, textures, options, true)
    }

    /// Spawns a Tiled map using a texture handle that has already been loaded.
    ///
    /// This is useful for browsers/editors that keep tileset textures resident
    /// while repeatedly spawning and despawning map instances.
    pub fn spawn_import_with_texture(
        world: &mut World,
        import: &TiledImport,
        texture: Handle<TextureAsset>,
        options: TiledSpawnOptions,
    ) -> Result<Self, TiledMapInstanceError> {
        Self::spawn_import_with_textures_internal(world, import, vec![texture], options, false)
    }

    pub fn spawn_import_with_textures(
        world: &mut World,
        import: &TiledImport,
        textures: Vec<Handle<TextureAsset>>,
        options: TiledSpawnOptions,
    ) -> Result<Self, TiledMapInstanceError> {
        Self::spawn_import_with_textures_internal(world, import, textures, options, false)
    }

    fn spawn_import_with_textures_internal(
        world: &mut World,
        import: &TiledImport,
        textures: Vec<Handle<TextureAsset>>,
        options: TiledSpawnOptions,
        unload_textures_on_despawn: bool,
    ) -> Result<Self, TiledMapInstanceError> {
        if textures.is_empty() || textures.len() < import.tilesets.len() {
            return Err(TiledMapInstanceError::MissingTilesetTexture);
        }
        let asset_server = world.get_resource::<AssetServer>().cloned();
        if options.spawn_tile_objects
            && asset_server.is_none()
            && import_has_animated_tile_objects(import)
        {
            return Err(TiledMapInstanceError::MissingAssetServer);
        }

        if world.get_resource::<TilemapStorage>().is_none() {
            world.insert_resource(TilemapStorage::new());
        }
        let map = {
            let storage = world
                .get_resource_mut::<TilemapStorage>()
                .expect("TilemapStorage was just inserted");
            storage.insert(import.map.clone())
        };

        let origin = match options.origin {
            TiledSpawnOrigin::Centered => centered_origin(import, map, &textures),
            TiledSpawnOrigin::Position(position) => position,
        };
        let parallax_origin = [
            origin[0] + import.parallax_origin[0],
            origin[1] + import.parallax_origin[1],
        ];
        let mut entities = Vec::with_capacity(import.layers.len());
        let mut parallax_items = Vec::new();
        let tileset_grids = textures
            .iter()
            .enumerate()
            .filter_map(|(index, texture)| import.tileset_grid_for(index, *texture))
            .collect::<Vec<_>>();
        let mut animation_clips = HashMap::new();
        let mut runtime_animation_clips = Vec::new();

        for layer_index in 0..import.layers.len() {
            let layer = &import.layers[layer_index];
            let Some(renderer) = import.renderer_for_layer(map, &textures, layer_index as u32)
            else {
                continue;
            };
            let base_position = [origin[0] + layer.offset[0], origin[1] + layer.offset[1]];
            let transform = Transform::from_xyz(base_position[0], base_position[1], 0.0);
            let entity = spawn_tile_layer(
                world,
                transform,
                renderer,
                SortingLayer(layer.sorting_layer),
            );
            entities.push(entity);
            if options.parallax {
                parallax_items.push(TiledMapParallaxItem {
                    entity,
                    base_position,
                    factor: layer.parallax,
                });
            }
        }

        if options.spawn_tile_objects {
            for object_layer in &import.object_layers {
                if !object_layer.visible {
                    continue;
                }
                let parallax_object = options.parallax && is_parallax_factor(object_layer.parallax);
                for object in &object_layer.objects {
                    let TiledObjectShape::Tile {
                        tileset_index,
                        tile_id,
                        flags,
                    } = object.shape
                    else {
                        continue;
                    };
                    let Some(tileset) = tileset_grids.get(tileset_index) else {
                        continue;
                    };
                    let texture = textures[tileset_index];
                    let base_tile_id = tile_id;
                    let current_tile_id = tileset.animated_tile_id(base_tile_id, 0.0);
                    let Some(uv) = sprite_uv_for_tile(tileset, current_tile_id, flags) else {
                        continue;
                    };
                    let center = object.center();
                    let base_position = [
                        origin[0] + object_layer.offset[0] + center[0],
                        origin[1] + object_layer.offset[1] + center[1],
                    ];
                    let transform = Transform::from_xyz(base_position[0], base_position[1], 0.0);
                    let draw_size = tile_object_draw_size(object.size, parallax_object);
                    let sprite = SpriteRenderer::new(draw_size[0], draw_size[1])
                        .texture(texture)
                        .uv(uv[0], uv[1], uv[2], uv[3])
                        .color(Color::new(1.0, 1.0, 1.0, object_layer.opacity));
                    let animator = sprite_animator_for_tile_object(
                        asset_server.as_ref(),
                        tileset,
                        tileset_index,
                        base_tile_id,
                        flags,
                        &mut animation_clips,
                        &mut runtime_animation_clips,
                    )?;
                    let entity = if let Some(animator) = animator {
                        spawn_animated_tile_object(
                            world,
                            transform,
                            sprite,
                            animator,
                            SortingLayer(object_layer.sorting_layer),
                        )
                    } else {
                        spawn_tile_object(
                            world,
                            transform,
                            sprite,
                            SortingLayer(object_layer.sorting_layer),
                        )
                    };
                    entities.push(entity);
                    if options.parallax {
                        parallax_items.push(TiledMapParallaxItem {
                            entity,
                            base_position,
                            factor: object_layer.parallax,
                        });
                    }
                }
            }
        }

        Ok(Self {
            map,
            texture: textures[0],
            textures,
            entities,
            origin,
            parallax_origin,
            parallax_items,
            runtime_animation_clips,
            unload_texture_on_despawn: unload_textures_on_despawn,
        })
    }

    pub fn origin(&self) -> [f32; 2] {
        self.origin
    }

    pub fn sync_parallax(&self, world: &mut World, camera_position: [f32; 2]) {
        for item in &self.parallax_items {
            let Some(transform) = world.get_mut::<Transform>(item.entity) else {
                continue;
            };
            transform.position[0] = item.base_position[0]
                + (camera_position[0] - self.parallax_origin[0]) * (1.0 - item.factor[0]);
            transform.position[1] = item.base_position[1]
                + (camera_position[1] - self.parallax_origin[1]) * (1.0 - item.factor[1]);
        }
    }

    pub fn reset_parallax(&self, world: &mut World) {
        for item in &self.parallax_items {
            let Some(transform) = world.get_mut::<Transform>(item.entity) else {
                continue;
            };
            transform.position[0] = item.base_position[0];
            transform.position[1] = item.base_position[1];
        }
    }

    pub fn despawn(self, world: &mut World) {
        let Self {
            map,
            texture: _,
            textures,
            entities,
            origin: _,
            parallax_origin: _,
            parallax_items: _,
            runtime_animation_clips,
            unload_texture_on_despawn,
        } = self;

        for entity in entities {
            let _ = world.despawn(entity);
        }
        if let Some(storage) = world.get_resource_mut::<TilemapStorage>() {
            let _ = storage.remove(map);
        }
        if unload_texture_on_despawn {
            if let Some(asset_server) = world.get_resource::<AssetServer>().cloned() {
                for texture in &textures {
                    asset_server.unload(texture);
                }
            }
        }
        if let Some(asset_server) = world.get_resource::<AssetServer>().cloned() {
            for clip in &runtime_animation_clips {
                asset_server.unload(clip);
            }
        }
    }
}

#[derive(Debug)]
pub enum TiledMapInstanceError {
    Import(TiledImportError),
    MissingAssetServer,
    MissingTilesetTexture,
}

impl fmt::Display for TiledMapInstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Import(error) => error.fmt(f),
            Self::MissingAssetServer => write!(f, "AssetServer resource is missing"),
            Self::MissingTilesetTexture => write!(f, "not enough tileset textures were provided"),
        }
    }
}

impl std::error::Error for TiledMapInstanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Import(error) => Some(error),
            Self::MissingAssetServer | Self::MissingTilesetTexture => None,
        }
    }
}

impl From<TiledImportError> for TiledMapInstanceError {
    fn from(value: TiledImportError) -> Self {
        Self::Import(value)
    }
}

fn spawn_tile_layer(
    world: &mut World,
    transform: Transform,
    renderer: TilemapRenderer,
    sorting_layer: SortingLayer,
) -> EntityId {
    world.spawn((transform, renderer, sorting_layer))
}

fn spawn_tile_object(
    world: &mut World,
    transform: Transform,
    sprite: SpriteRenderer,
    sorting_layer: SortingLayer,
) -> EntityId {
    world.spawn((transform, sprite, sorting_layer))
}

fn spawn_animated_tile_object(
    world: &mut World,
    transform: Transform,
    sprite: SpriteRenderer,
    animator: SpriteAnimator,
    sorting_layer: SortingLayer,
) -> EntityId {
    world.spawn((transform, sprite, animator, sorting_layer))
}

fn is_parallax_factor(factor: [f32; 2]) -> bool {
    (factor[0] - 1.0).abs() > f32::EPSILON || (factor[1] - 1.0).abs() > f32::EPSILON
}

fn tile_animation<'a>(tileset: &'a TilesetGrid, tile_id: TileId) -> Option<&'a TileAnimation> {
    tileset
        .animations
        .iter()
        .find(|animation| animation.tile_id == tile_id)
}

fn import_has_animated_tile_objects(import: &TiledImport) -> bool {
    import.object_layers.iter().any(|layer| {
        layer.visible
            && layer.objects.iter().any(|object| {
                let TiledObjectShape::Tile {
                    tileset_index,
                    tile_id,
                    ..
                } = object.shape
                else {
                    return false;
                };
                import.tilesets.get(tileset_index).is_some_and(|tileset| {
                    tileset
                        .animations
                        .iter()
                        .any(|animation| animation.tile_id == tile_id)
                })
            })
    })
}

fn tile_object_draw_size(size: [f32; 2], parallax_object: bool) -> [f32; 2] {
    if parallax_object {
        [size[0] + PARALLAX_TILE_OBJECT_OVERLAP_X, size[1]]
    } else {
        size
    }
}

fn sprite_animator_for_tile_object(
    asset_server: Option<&AssetServer>,
    tileset: &TilesetGrid,
    tileset_index: usize,
    tile_id: TileId,
    flags: TileFlags,
    animation_clips: &mut HashMap<(usize, TileId, TileFlags), Handle<SpriteAnimationClip>>,
    runtime_animation_clips: &mut Vec<Handle<SpriteAnimationClip>>,
) -> Result<Option<SpriteAnimator>, TiledMapInstanceError> {
    if tile_animation(tileset, tile_id).is_none() {
        return Ok(None);
    }
    let key = (tileset_index, tile_id, flags);
    if let Some(clip) = animation_clips.get(&key).copied() {
        return Ok(Some(SpriteAnimator::new(clip)));
    }

    let clip = sprite_animation_clip_for_tile(tileset, tile_id, flags);
    if clip.frames.is_empty() {
        return Ok(None);
    }
    let asset_server = asset_server.ok_or(TiledMapInstanceError::MissingAssetServer)?;
    let handle = asset_server.insert_runtime(clip);
    animation_clips.insert(key, handle);
    runtime_animation_clips.push(handle);
    Ok(Some(SpriteAnimator::new(handle)))
}

fn sprite_animation_clip_for_tile(
    tileset: &TilesetGrid,
    tile_id: TileId,
    flags: TileFlags,
) -> SpriteAnimationClip {
    let frames = tile_animation(tileset, tile_id)
        .into_iter()
        .flat_map(|animation| {
            animation.frames.iter().filter_map(move |frame| {
                sprite_uv_for_tile(tileset, frame.tile_id, flags)
                    .map(|uv| SpriteAnimationFrame::new(uv, frame.duration_ms))
            })
        })
        .collect::<Vec<_>>();
    SpriteAnimationClip::new(frames)
}

fn centered_origin(
    import: &TiledImport,
    map: TilemapHandle,
    textures: &[Handle<TextureAsset>],
) -> [f32; 2] {
    let Some(renderer) = import.renderer_for_layer(map, textures, 0) else {
        let width = import.map.width() as f32 * import.tile_size[0] as f32;
        let height = import.map.height() as f32 * import.tile_size[1] as f32;
        return [-width * 0.5, -height * 0.5];
    };

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for y in 0..import.map.height() {
        for x in 0..import.map.width() {
            let Some(tile) = import.map.tile(0, x, y) else {
                continue;
            };
            if tile.is_empty() {
                continue;
            }
            let mut origin = renderer.cell_to_local_origin([x as i32, y as i32]);
            origin[0] += renderer.tile_offset[0];
            origin[1] += renderer.tile_offset[1];
            let tile_id = renderer.tileset.animated_tile_id(tile.id, 0.0);
            let [tile_w, tile_h] = renderer
                .tileset
                .tile_draw_size(tile_id)
                .map(|size| [size[0] as f32, size[1] as f32])
                .unwrap_or(renderer.tile_draw_size);
            min_x = min_x.min(origin[0]);
            min_y = min_y.min(origin[1]);
            max_x = max_x.max(origin[0] + tile_w);
            max_y = max_y.max(origin[1] + tile_h);
        }
    }
    for object_layer in &import.object_layers {
        if !object_layer.visible {
            continue;
        }
        for object in &object_layer.objects {
            let center = object.center();
            let center_x = object_layer.offset[0] + center[0];
            let center_y = object_layer.offset[1] + center[1];
            let half_w = object.size[0] * 0.5;
            let half_h = object.size[1] * 0.5;
            min_x = min_x.min(center_x - half_w);
            min_y = min_y.min(center_y - half_h);
            max_x = max_x.max(center_x + half_w);
            max_y = max_y.max(center_y + half_h);
        }
    }

    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return [0.0, 0.0];
    }
    [-(min_x + max_x) * 0.5, -(min_y + max_y) * 0.5]
}

fn sprite_uv_for_tile(
    tileset: &TilesetGrid,
    tile_id: TileId,
    flags: TileFlags,
) -> Option<[f32; 4]> {
    let mut uv = tileset.uv_rect(tile_id)?;
    if flags.contains(TileFlags::FLIP_X) {
        uv.swap(0, 2);
    }
    if flags.contains(TileFlags::FLIP_Y) {
        uv.swap(1, 3);
    }
    if flags.contains(TileFlags::FLIP_DIAGONAL) {
        return None;
    }
    Some(uv)
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetConfig, AssetId, AssetServer, AssetState};
    use crate::render::animate_sprites;
    use crate::render::component::TileAnimationFrame;

    use super::*;

    #[test]
    fn spawns_and_despawns_official_forest_map_instance() {
        let mut world = World::new();
        world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));

        let map = TiledMapInstance::spawn(
            &mut world,
            "examples/assets/tiled/tiled/examples/forest/forest.tmx",
            TiledSpawnOptions::centered(),
        )
        .expect("official forest map instance should spawn");

        assert!(!map.entities.is_empty());
        assert!(map.entities.iter().any(|entity| {
            world
                .get::<SpriteRenderer>(*entity)
                .is_some_and(|sprite| (sprite.width - 161.0).abs() < f32::EPSILON)
        }));
        assert!(world.get_resource::<TilemapStorage>().is_some());
        map.despawn(&mut world);
    }

    #[test]
    fn animates_official_forest_tile_objects() {
        let mut world = World::new();
        world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));

        let map = TiledMapInstance::spawn(
            &mut world,
            "examples/assets/tiled/tiled/examples/forest/forest.tmx",
            TiledSpawnOptions::centered(),
        )
        .expect("official forest map instance should spawn");

        assert_eq!(map.runtime_animation_clips.len(), 1);
        let squirrel = map
            .entities
            .iter()
            .copied()
            .find(|entity| world.get::<SpriteAnimator>(*entity).is_some())
            .expect("forest squirrel should have a sprite animator");
        let first_uv = world
            .get::<SpriteRenderer>(squirrel)
            .expect("animated object should have a sprite")
            .uv;

        world.time.delta = 0.15;
        animate_sprites(&mut world);

        let second_uv = world
            .get::<SpriteRenderer>(squirrel)
            .expect("animated object should still have a sprite")
            .uv;
        assert_ne!(first_uv, second_uv);

        let asset_server = world.get_resource::<AssetServer>().cloned().unwrap();
        let clip = map.runtime_animation_clips[0];
        map.despawn(&mut world);
        asset_server.update().unwrap();
        assert_eq!(asset_server.state(&clip), AssetState::Unloaded);
    }

    #[test]
    fn tile_object_animation_clips_reuse_handles_per_instance() {
        let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
        let texture = Handle::<TextureAsset>::new(AssetId::new());
        let tileset = TilesetGrid::new(texture, [16, 16], 4, 1).animations([TileAnimation::new(
            TileId(0),
            [
                TileAnimationFrame::new(TileId(1), 100),
                TileAnimationFrame::new(TileId(2), 100),
            ],
        )]);
        let mut animation_clips = HashMap::new();
        let mut runtime_animation_clips = Vec::new();

        let first = sprite_animator_for_tile_object(
            Some(&asset_server),
            &tileset,
            0,
            TileId(0),
            TileFlags::empty(),
            &mut animation_clips,
            &mut runtime_animation_clips,
        )
        .unwrap()
        .unwrap();
        let second = sprite_animator_for_tile_object(
            Some(&asset_server),
            &tileset,
            0,
            TileId(0),
            TileFlags::empty(),
            &mut animation_clips,
            &mut runtime_animation_clips,
        )
        .unwrap()
        .unwrap();
        let flipped = sprite_animator_for_tile_object(
            Some(&asset_server),
            &tileset,
            0,
            TileId(0),
            TileFlags::FLIP_X,
            &mut animation_clips,
            &mut runtime_animation_clips,
        )
        .unwrap()
        .unwrap();

        assert_eq!(first.clip, second.clip);
        assert_ne!(first.clip, flipped.clip);
        assert_eq!(runtime_animation_clips.len(), 2);
    }
}
