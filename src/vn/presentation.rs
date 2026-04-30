use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::asset::{AssetServer, Handle, TextureAsset, TextureColorSpace};
use crate::ecs::{EntityId, World};
use crate::render::{Color, SortingLayer, SpriteRenderer, Transform};
use crate::vn::components::{VnActorSprite, VnBackground};
use crate::vn::scene::{VnActor, VnImageLayer, VnSceneState};
use crate::vn::VnRuntime;

#[derive(Clone, Debug, PartialEq)]
pub struct VnSpritePresentationConfig {
    pub resolution: [u32; 2],
    pub background_size: [f32; 2],
    pub cg_size: [f32; 2],
    pub actor_size: [f32; 2],
    pub actor_positions: BTreeMap<String, [f32; 2]>,
}

impl Default for VnSpritePresentationConfig {
    fn default() -> Self {
        let mut actor_positions = BTreeMap::new();
        actor_positions.insert("far_left".to_owned(), [-480.0, -42.0]);
        actor_positions.insert("left".to_owned(), [-320.0, -42.0]);
        actor_positions.insert("center".to_owned(), [0.0, -42.0]);
        actor_positions.insert("right".to_owned(), [320.0, -42.0]);
        actor_positions.insert("far_right".to_owned(), [480.0, -42.0]);
        Self {
            resolution: [1280, 720],
            background_size: [1280.0, 720.0],
            cg_size: [1280.0, 720.0],
            actor_size: [360.0, 680.0],
            actor_positions,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VnSpriteTextureMap {
    textures: BTreeMap<String, Handle<TextureAsset>>,
    sizes: BTreeMap<String, [u32; 2]>,
    visible_rects: BTreeMap<String, [u32; 4]>,
}

impl VnSpriteTextureMap {
    pub fn insert(
        &mut self,
        asset: impl Into<String>,
        texture: Handle<TextureAsset>,
    ) -> Option<Handle<TextureAsset>> {
        let asset = asset.into();
        self.sizes.remove(&asset);
        self.visible_rects.remove(&asset);
        self.textures.insert(asset, texture)
    }

    pub fn get(&self, asset: &str) -> Option<Handle<TextureAsset>> {
        self.textures.get(asset).copied()
    }

    pub fn insert_with_size(
        &mut self,
        asset: impl Into<String>,
        texture: Handle<TextureAsset>,
        size: [u32; 2],
    ) -> Option<Handle<TextureAsset>> {
        self.insert_with_visible_rect(asset, texture, size, [0, 0, size[0], size[1]])
    }

    pub fn insert_with_visible_rect(
        &mut self,
        asset: impl Into<String>,
        texture: Handle<TextureAsset>,
        size: [u32; 2],
        visible_rect: [u32; 4],
    ) -> Option<Handle<TextureAsset>> {
        let asset = asset.into();
        self.sizes.insert(asset.clone(), size);
        self.visible_rects
            .insert(asset.clone(), clamp_visible_rect(size, visible_rect));
        self.textures.insert(asset, texture)
    }

    pub fn size(&self, asset: &str) -> Option<[u32; 2]> {
        self.sizes.get(asset).copied()
    }

    pub fn size_f32(&self, asset: &str) -> Option<[f32; 2]> {
        self.size(asset)
            .map(|size| [size[0] as f32, size[1] as f32])
    }

    pub fn visible_rect(&self, asset: &str) -> Option<[u32; 4]> {
        self.visible_rects.get(asset).copied()
    }

    pub fn visible_size(&self, asset: &str) -> Option<[u32; 2]> {
        self.visible_rect(asset)
            .map(|rect| [rect[2], rect[3]])
            .or_else(|| self.size(asset))
    }

    pub fn visible_uv_rect(&self, asset: &str) -> Option<[f32; 4]> {
        let size = self.size(asset)?;
        let rect = self
            .visible_rect(asset)
            .unwrap_or([0, 0, size[0], size[1]]);
        if size[0] == 0 || size[1] == 0 {
            return Some([0.0, 0.0, 1.0, 1.0]);
        }
        let width = size[0] as f32;
        let height = size[1] as f32;
        Some([
            rect[0] as f32 / width,
            rect[1] as f32 / height,
            (rect[0] + rect[2]) as f32 / width,
            (rect[1] + rect[3]) as f32 / height,
        ])
    }

    pub fn load_image_file(
        &mut self,
        assets: &AssetServer,
        asset: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<VnLoadedTexture, VnTextureLoadError> {
        let asset = asset.into();
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| VnTextureLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let image = image::load_from_memory(&bytes)
            .map_err(|source| VnTextureLoadError::Decode {
                path: path.to_path_buf(),
                source,
            })?;
        let has_alpha = image.color().has_alpha();
        let image = image.to_rgba8();
        let (width, height) = image.dimensions();
        let visible_rect = if has_alpha {
            alpha_visible_rect(&image)
        } else {
            [0, 0, width, height]
        };
        let handle = assets.insert_runtime(TextureAsset::new(
            width,
            height,
            TextureColorSpace::Srgb,
            image.into_raw(),
        ));
        self.insert_with_visible_rect(asset.clone(), handle, [width, height], visible_rect);
        Ok(VnLoadedTexture {
            asset,
            handle,
            size: [width, height],
        })
    }

    pub fn load_image_files<A, P>(
        &mut self,
        assets: &AssetServer,
        files: impl IntoIterator<Item = (A, P)>,
    ) -> Result<Vec<VnLoadedTexture>, VnTextureLoadError>
    where
        A: Into<String>,
        P: AsRef<Path>,
    {
        files
            .into_iter()
            .map(|(asset, path)| self.load_image_file(assets, asset, path))
            .collect()
    }

    pub fn load_image_files_from<A, P>(
        &mut self,
        assets: &AssetServer,
        root: impl AsRef<Path>,
        files: impl IntoIterator<Item = (A, P)>,
    ) -> Result<Vec<VnLoadedTexture>, VnTextureLoadError>
    where
        A: Into<String>,
        P: AsRef<Path>,
    {
        let root = root.as_ref();
        files
            .into_iter()
            .map(|(asset, path)| self.load_image_file(assets, asset, root.join(path)))
            .collect()
    }

    pub fn from_image_files<A, P>(
        assets: &AssetServer,
        files: impl IntoIterator<Item = (A, P)>,
    ) -> Result<Self, VnTextureLoadError>
    where
        A: Into<String>,
        P: AsRef<Path>,
    {
        let mut textures = Self::default();
        textures.load_image_files(assets, files)?;
        Ok(textures)
    }

    pub fn from_image_files_from<A, P>(
        assets: &AssetServer,
        root: impl AsRef<Path>,
        files: impl IntoIterator<Item = (A, P)>,
    ) -> Result<Self, VnTextureLoadError>
    where
        A: Into<String>,
        P: AsRef<Path>,
    {
        let mut textures = Self::default();
        textures.load_image_files_from(assets, root, files)?;
        Ok(textures)
    }
}

fn alpha_visible_rect(image: &image::RgbaImage) -> [u32; 4] {
    let (width, height) = image.dimensions();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;

    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel.0[3] == 0 {
            continue;
        }
        found = true;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VnLoadedTexture {
    pub asset: String,
    pub handle: Handle<TextureAsset>,
    pub size: [u32; 2],
}

#[derive(Debug)]
pub enum VnTextureLoadError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Decode {
        path: PathBuf,
        source: image::ImageError,
    },
}

impl fmt::Display for VnTextureLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    f,
                    "failed to read VN texture '{}': {source}",
                    path.display()
                )
            }
            Self::Decode { path, source } => {
                write!(
                    f,
                    "failed to decode VN texture '{}': {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for VnTextureLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Decode { source, .. } => Some(source),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VnSpriteSceneEntities {
    pub background: Option<EntityId>,
    pub cg: Option<EntityId>,
    pub actors: BTreeMap<String, EntityId>,
}

pub fn sync_runtime_scene_to_world(world: &mut World) {
    let Some(scene) = world
        .get_resource::<VnRuntime>()
        .map(|runtime| runtime.scene().clone())
    else {
        return;
    };
    let config = world
        .get_resource::<VnSpritePresentationConfig>()
        .cloned()
        .unwrap_or_default();
    let textures = world
        .get_resource::<VnSpriteTextureMap>()
        .cloned()
        .unwrap_or_default();
    let mut entities = world
        .remove_resource::<VnSpriteSceneEntities>()
        .unwrap_or_default();

    sync_scene_to_world(world, &mut entities, &scene, &config, &textures);
    world.insert_resource(entities);
}

pub fn sync_scene_to_world(
    world: &mut World,
    entities: &mut VnSpriteSceneEntities,
    scene: &VnSceneState,
    config: &VnSpritePresentationConfig,
    textures: &VnSpriteTextureMap,
) {
    sync_image_layer(
        world,
        &mut entities.background,
        scene.background.as_ref(),
        config.background_size,
        textures,
        "background",
    );
    sync_image_layer(
        world,
        &mut entities.cg,
        scene.cg.as_ref(),
        config.cg_size,
        textures,
        "cg",
    );
    sync_actors(world, &mut entities.actors, scene, config, textures);
}

fn sync_image_layer(
    world: &mut World,
    slot: &mut Option<EntityId>,
    layer: Option<&VnImageLayer>,
    size: [f32; 2],
    textures: &VnSpriteTextureMap,
    marker_name: &str,
) {
    let Some(layer) = layer else {
        despawn_slot(world, slot);
        return;
    };

    let sprite = sprite_for_asset(&layer.asset, size, 1.0, textures);
    let transform = Transform::from_xyz(0.0, 0.0, layer.layer as f32);
    let marker = VnBackground {
        asset: layer.asset.clone(),
        layer: layer.layer,
    };
    let entity = ensure_entity(world, slot, transform, sprite, SortingLayer(layer.layer));
    world.insert(entity, marker);
    world.insert(
        entity,
        crate::vn::components::VnSceneLayer {
            name: marker_name.to_owned(),
            order: layer.layer,
        },
    );
}

fn sync_actors(
    world: &mut World,
    entities: &mut BTreeMap<String, EntityId>,
    scene: &VnSceneState,
    config: &VnSpritePresentationConfig,
    textures: &VnSpriteTextureMap,
) {
    let stale: Vec<_> = entities
        .keys()
        .filter(|id| !scene.actors.contains_key(*id))
        .cloned()
        .collect();
    for id in stale {
        if let Some(entity) = entities.remove(&id) {
            world.despawn(entity);
        }
    }

    for actor in scene.actors.values().filter(|actor| actor.visible) {
        sync_actor(world, entities, actor, config, textures);
    }
}

fn sync_actor(
    world: &mut World,
    entities: &mut BTreeMap<String, EntityId>,
    actor: &VnActor,
    config: &VnSpritePresentationConfig,
    textures: &VnSpriteTextureMap,
) {
    let asset = actor
        .asset
        .as_deref()
        .or(actor.expression.as_deref())
        .unwrap_or(&actor.id);
    let position = actor_position(actor, config);
    let sprite = sprite_for_asset(asset, config.actor_size, actor.opacity, textures);
    let transform = Transform::from_xyz(position[0], position[1], actor.layer as f32);
    let marker = VnActorSprite {
        actor_id: actor.id.clone(),
        asset: actor.asset.clone(),
        expression: actor.expression.clone(),
        layer: actor.layer,
        z: actor.z,
        opacity: actor.opacity,
    };

    let mut slot = entities.get(&actor.id).copied();
    let entity = ensure_entity(
        world,
        &mut slot,
        transform,
        sprite,
        SortingLayer(actor.layer),
    );
    entities.insert(actor.id.clone(), entity);
    world.insert(entity, marker);
    world.insert(
        entity,
        crate::vn::components::VnSceneLayer {
            name: actor.id.clone(),
            order: actor.layer,
        },
    );
}

fn ensure_entity(
    world: &mut World,
    slot: &mut Option<EntityId>,
    transform: Transform,
    sprite: SpriteRenderer,
    sorting_layer: SortingLayer,
) -> EntityId {
    if let Some(entity) = *slot {
        if world.contains(entity) {
            world.insert(entity, transform);
            world.insert(entity, sprite);
            world.insert(entity, sorting_layer);
            return entity;
        }
    }

    let entity = world.spawn((transform, sprite, sorting_layer));
    *slot = Some(entity);
    entity
}

fn despawn_slot(world: &mut World, slot: &mut Option<EntityId>) {
    if let Some(entity) = slot.take() {
        world.despawn(entity);
    }
}

fn sprite_for_asset(
    asset: &str,
    size: [f32; 2],
    opacity: f32,
    textures: &VnSpriteTextureMap,
) -> SpriteRenderer {
    let alpha = opacity.clamp(0.0, 1.0);
    if let Some(texture) = textures.get(asset) {
        return SpriteRenderer::new(size[0], size[1])
            .texture(texture)
            .color(Color::new(1.0, 1.0, 1.0, alpha));
    }

    SpriteRenderer::new(size[0], size[1]).color(placeholder_color(asset, alpha))
}

fn placeholder_color(asset: &str, alpha: f32) -> Color {
    let hash = asset.bytes().fold(0x811c9dc5u32, |hash, byte| {
        hash.wrapping_mul(16777619) ^ byte as u32
    });
    let hue = (hash % 360) as f32;
    Color::hsl(hue, 0.48, 0.58).with_alpha(alpha)
}

fn actor_position(actor: &VnActor, config: &VnSpritePresentationConfig) -> [f32; 2] {
    let Some(position) = actor.position.as_deref() else {
        return config
            .actor_positions
            .get("center")
            .copied()
            .unwrap_or([0.0, 0.0]);
    };
    if let Some(named) = config.actor_positions.get(position) {
        return *named;
    }
    parse_xy(position).unwrap_or_else(|| {
        config
            .actor_positions
            .get("center")
            .copied()
            .unwrap_or([0.0, 0.0])
    })
}

fn parse_xy(raw: &str) -> Option<[f32; 2]> {
    let (x, y) = raw.split_once(',')?;
    Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
}

trait ColorAlphaExt {
    fn with_alpha(self, alpha: f32) -> Self;
}

impl ColorAlphaExt for Color {
    fn with_alpha(mut self, alpha: f32) -> Self {
        self.a = alpha;
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetConfig, AssetServer};
    use crate::vn::{VnRuntime, VnRuntimeEvent, YarnScript};

    use super::*;

    #[test]
    fn sprite_sync_spawns_and_reuses_scene_entities() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<scene "bg/classroom.png" layer=0>>
<<show alice "alice/smile.png" at="right" layer=20>>
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();
        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Command(_)
        ));
        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Command(_)
        ));

        let mut world = World::new();
        world.insert_resource(runtime);
        sync_runtime_scene_to_world(&mut world);

        let entities = world
            .get_resource::<VnSpriteSceneEntities>()
            .unwrap()
            .clone();
        let background = entities.background.unwrap();
        let alice = entities.actors["alice"];
        assert!(world.get::<VnBackground>(background).is_some());
        assert!(world.get::<VnActorSprite>(alice).is_some());

        sync_runtime_scene_to_world(&mut world);
        let reused = world.get_resource::<VnSpriteSceneEntities>().unwrap();
        assert_eq!(reused.background, Some(background));
        assert_eq!(reused.actors["alice"], alice);
    }

    #[test]
    fn texture_map_loads_image_files_with_sizes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("white.png");
        image::save_buffer(&path, &[255, 255, 255, 255], 1, 1, image::ColorType::Rgba8).unwrap();

        let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
        let textures = VnSpriteTextureMap::from_image_files_from(
            &asset_server,
            temp.path(),
            [("vn/white", "white.png")],
        )
        .unwrap();

        let handle = textures.get("vn/white").unwrap();
        assert_eq!(textures.size("vn/white"), Some([1, 1]));
        assert!(asset_server.is_installed(&handle));
    }

    #[test]
    fn texture_map_records_alpha_visible_rect() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("pose.png");
        let pixels = [
            0, 0, 0, 0, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255,
            255, 255,
        ];
        image::save_buffer(&path, &pixels, 3, 2, image::ColorType::Rgba8).unwrap();

        let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
        let textures = VnSpriteTextureMap::from_image_files_from(
            &asset_server,
            temp.path(),
            [("vn/pose", "pose.png")],
        )
        .unwrap();

        assert_eq!(textures.size("vn/pose"), Some([3, 2]));
        assert_eq!(textures.visible_rect("vn/pose"), Some([1, 0, 2, 2]));
        assert_eq!(textures.visible_size("vn/pose"), Some([2, 2]));
        assert_slice_near(
            textures.visible_uv_rect("vn/pose").unwrap(),
            [1.0 / 3.0, 0.0, 1.0, 1.0],
        );
    }

    fn assert_slice_near(actual: [f32; 4], expected: [f32; 4]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.001,
                "expected {expected}, got {actual}"
            );
        }
    }
}
