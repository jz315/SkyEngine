//! Per-frame scene extraction from ECS World.

use crate::ecs::{PreparedQuery, World};
use crate::render::core::camera::Camera2D;
use crate::render::ecs::{
    PointLight2D, PrimaryCamera2D, RenderSettings2D, RenderView2D, Sprite2D, Transform2D,
    ViewportRect,
};
use crate::render::pipeline::{SceneCache2D, SceneLightItem, SceneSpriteItem, SceneView2D};

pub(crate) struct SceneExtractor {
    camera_query: PreparedQuery<(
        &'static Camera2D,
        Option<&'static RenderView2D>,
        Option<&'static PrimaryCamera2D>,
    )>,
    sprite_query: PreparedQuery<(&'static Transform2D, &'static Sprite2D)>,
    light_query: PreparedQuery<(&'static Transform2D, &'static PointLight2D)>,
    extract_epoch: u64,
    stale_sprites: Vec<usize>,
    stale_lights: Vec<usize>,
}

impl SceneExtractor {
    pub fn new() -> Self {
        Self {
            camera_query: PreparedQuery::new(),
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
        scene: &mut SceneCache2D,
        surface_size: [u32; 2],
        default_settings: RenderSettings2D,
    ) {
        self.update_frame_metadata(world, scene, surface_size, default_settings);

        self.extract_epoch = self.extract_epoch.wrapping_add(1);
        if self.extract_epoch == 0 {
            self.extract_epoch = 1;
        }

        self.sync_sprites_incremental(world, scene);
        self.sync_lights_incremental(world, scene);
    }

    fn update_frame_metadata(
        &mut self,
        world: &World,
        scene: &mut SceneCache2D,
        surface_size: [u32; 2],
        default_settings: RenderSettings2D,
    ) {
        scene.views.clear();

        scene.settings = world
            .get_resource::<RenderSettings2D>()
            .copied()
            .unwrap_or(default_settings);

        let fallback_camera =
            Camera2D::new(surface_size[0].max(1) as f32, surface_size[1].max(1) as f32);
        let mut first_primary_camera = None;
        let mut first_camera = None;
        let mut explicit_view_count = 0usize;

        self.camera_query
            .for_each(world, |(camera, render_view, primary)| {
                let camera = *camera;
                if primary.is_some() && first_primary_camera.is_none() {
                    first_primary_camera = Some(camera);
                }
                if first_camera.is_none() {
                    first_camera = Some(camera);
                }
                if let Some(render_view) = render_view {
                    explicit_view_count += 1;
                    scene.views.push(SceneView2D::new(camera, *render_view));
                }
            });

        if explicit_view_count == 0 {
            let mut camera = first_primary_camera
                .or(first_camera)
                .unwrap_or(fallback_camera);
            camera.set_viewport(surface_size[0].max(1) as f32, surface_size[1].max(1) as f32);
            scene.views.push(SceneView2D::new(
                camera,
                RenderView2D::new(ViewportRect::from_surface_size(surface_size)),
            ));
        }
    }

    fn sync_sprites_incremental(&mut self, world: &World, scene: &mut SceneCache2D) {
        self.sprite_query
            .for_each_with_entity(world, |entity, (transform, sprite)| {
                scene.upsert_sprite(
                    entity,
                    SceneSpriteItem {
                        transform: *transform,
                        sprite: sprite.clone(),
                        sort_key: entity_sort_key(entity),
                        texture_sort_key: texture_sort_key(&sprite.texture),
                    },
                    self.extract_epoch,
                );
            });

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

    fn sync_lights_incremental(&mut self, world: &World, scene: &mut SceneCache2D) {
        self.light_query
            .for_each_with_entity(world, |entity, (transform, light)| {
                scene.upsert_light(
                    entity,
                    SceneLightItem {
                        transform: *transform,
                        light: *light,
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
