use crate::ecs::{EntityId, PreparedQuery, World};
use crate::render::component::{Camera, CameraViewport, MainCamera, Transform};
use crate::render::view::{
    build_scene_view, Projection, ResolvedSceneTransforms, SceneTransformResolver, SceneView,
};
use rustc_hash::FxHashSet;

type ViewQuery = (
    &'static Transform,
    &'static Camera,
    Option<&'static Projection>,
    Option<&'static CameraViewport>,
    Option<&'static MainCamera>,
);

pub(crate) struct WorldViewCollector {
    transform_resolver: SceneTransformResolver,
    logged_missing_projection: FxHashSet<EntityId>,
    view_query: PreparedQuery<ViewQuery>,
}

impl WorldViewCollector {
    pub(crate) fn resolve_transforms(&mut self, world: &World) -> ResolvedSceneTransforms {
        self.transform_resolver.resolve_owned(world)
    }

    pub(crate) fn collect_world_views(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    ) -> Vec<SceneView> {
        let mut views = Vec::new();
        let mut first_camera = None;
        let mut first_main_camera = None;
        let mut missing_projection_entity = None;

        self.view_query.for_each_with_entity(
            world,
            |entity, (transform, camera, projection, viewport, main_camera)| {
                if !camera.enabled {
                    return;
                }
                if projection.is_none() && missing_projection_entity.is_none() {
                    missing_projection_entity = Some(entity);
                }
                let transform = transforms.get(entity).unwrap_or(*transform);
                let projection = projection.copied();

                if let Some(viewport) = viewport.copied() {
                    views.push(
                        build_scene_view(transform, projection, Some(viewport), surface_size)
                            .with_history_key(camera_history_key(entity)),
                    );
                    return;
                }

                if first_camera.is_none() {
                    first_camera = Some((camera_history_key(entity), transform, projection));
                }
                if main_camera.is_some() && first_main_camera.is_none() {
                    first_main_camera = Some((camera_history_key(entity), transform, projection));
                }
            },
        );

        if let Some(entity) = missing_projection_entity {
            self.log_missing_projection(entity);
        }

        if views.is_empty() {
            if let Some((history_key, transform, projection)) = first_main_camera.or(first_camera) {
                views.push(
                    build_scene_view(transform, projection, None, surface_size)
                        .with_history_key(history_key),
                );
            }
        }

        views
    }

    fn log_missing_projection(&mut self, entity: EntityId) {
        if self.logged_missing_projection.insert(entity) {
            log::warn!(
                target: "sky_engine::render::camera",
                "render.camera.missing_projection: camera {:?} has no Projection; using an implicit orthographic view whose visible height matches the current viewport",
                entity,
            );
        }
    }
}

#[inline]
fn camera_history_key(entity: EntityId) -> u64 {
    let key =
        0x5100_0000_0000_0000u64 ^ ((entity.index() as u64) << 16) ^ entity.generation() as u64;
    if key == 0 {
        1
    } else {
        key
    }
}

impl Default for WorldViewCollector {
    fn default() -> Self {
        Self {
            transform_resolver: SceneTransformResolver::default(),
            logged_missing_projection: FxHashSet::default(),
            view_query: PreparedQuery::new(),
        }
    }
}
