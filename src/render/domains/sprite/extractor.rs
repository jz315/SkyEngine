//! Per-frame scene extraction from ECS World.

use crate::ecs::{PreparedQuery, World};
use crate::render::ecs::{
    OrderInLayer, PointLight2D, RenderLayerMask, RenderSettings, SortingLayer, SpriteRenderer,
    Transform,
};
use crate::render::scene::ResolvedSceneTransforms;

use super::scene_cache::{SceneCache2D, SceneLightItem, SceneSpriteItem};

pub(crate) struct SceneExtractor {
    sprite_query: PreparedQuery<(
        &'static Transform,
        &'static SpriteRenderer,
        Option<&'static SortingLayer>,
        Option<&'static OrderInLayer>,
        Option<&'static RenderLayerMask>,
    )>,
    light_query: PreparedQuery<(
        &'static Transform,
        &'static PointLight2D,
        Option<&'static RenderLayerMask>,
    )>,
    extract_epoch: u64,
    stale_sprites: Vec<usize>,
    stale_lights: Vec<usize>,
}

impl SceneExtractor {
    pub fn new() -> Self {
        Self {
            sprite_query: PreparedQuery::new(),
            light_query: PreparedQuery::new(),
            extract_epoch: 0,
            stale_sprites: Vec::with_capacity(32),
            stale_lights: Vec::with_capacity(16),
        }
    }

    pub fn sync_incremental(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        scene: &mut SceneCache2D,
        _surface_size: [u32; 2],
        default_settings: RenderSettings,
    ) {
        self.update_frame_metadata(world, scene, default_settings);

        self.extract_epoch = self.extract_epoch.wrapping_add(1);
        if self.extract_epoch == 0 {
            self.extract_epoch = 1;
        }

        self.sync_sprites_incremental(world, transforms, scene);
        self.sync_lights_incremental(world, transforms, scene);
    }

    fn update_frame_metadata(
        &mut self,
        world: &World,
        scene: &mut SceneCache2D,
        default_settings: RenderSettings,
    ) {
        scene.settings = world
            .get_resource::<RenderSettings>()
            .copied()
            .unwrap_or(default_settings);
    }

    fn sync_sprites_incremental(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        scene: &mut SceneCache2D,
    ) {
        self.sprite_query.for_each_with_entity(
            world,
            |entity, (transform, sprite, sorting_layer, order, mask)| {
                let mut sprite = sprite.clone();
                if let Some(mask) = mask {
                    sprite.layer_mask = mask.0;
                }
                let texture_sort_key = texture_sort_key(&sprite.texture);
                scene.upsert_sprite(
                    entity,
                    SceneSpriteItem {
                        transform: transforms.get(entity).unwrap_or(*transform),
                        sprite,
                        sorting_layer: sorting_layer.copied().unwrap_or_default(),
                        order_in_layer: order.copied().unwrap_or_default(),
                        sort_key: entity_sort_key(entity),
                        texture_sort_key,
                    },
                    self.extract_epoch,
                );
            },
        );

        self.stale_sprites.clear();
        self.stale_sprites
            .extend(scene.active_sprite_slots().iter().copied().filter(|&slot| {
                scene
                    .sprite_seen_epoch(slot)
                    .is_some_and(|seen_epoch| seen_epoch != self.extract_epoch)
            }));
        for slot in self.stale_sprites.drain(..) {
            scene.release_sprite_slot(slot);
        }
    }

    fn sync_lights_incremental(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        scene: &mut SceneCache2D,
    ) {
        self.light_query
            .for_each_with_entity(world, |entity, (transform, light, mask)| {
                let mut light = *light;
                if let Some(mask) = mask {
                    light.layer_mask = mask.0;
                }
                scene.upsert_light(
                    entity,
                    SceneLightItem {
                        transform: transforms.get(entity).unwrap_or(*transform),
                        light,
                    },
                    self.extract_epoch,
                );
            });

        self.stale_lights.clear();
        self.stale_lights
            .extend(scene.active_light_slots().iter().copied().filter(|&slot| {
                scene
                    .light_seen_epoch(slot)
                    .is_some_and(|seen_epoch| seen_epoch != self.extract_epoch)
            }));
        for slot in self.stale_lights.drain(..) {
            scene.release_light_slot(slot);
        }
    }
}

fn entity_sort_key(entity: crate::ecs::EntityId) -> u64 {
    ((entity.index() as u64) << 32) | entity.generation() as u64
}

fn texture_sort_key(texture: &Option<crate::render::Texture>) -> u64 {
    texture
        .as_ref()
        .map(|texture| texture.texture() as *const wgpu::Texture as usize as u64)
        .unwrap_or(0)
}
