use crate::diagnostics::{Diagnostics, EngineDiagnosticKind};
use crate::ecs::{EntityId, PreparedQuery, World};
use crate::render::component::{Camera, CameraViewport, MainCamera, Transform};
use crate::render::view::{
    build_scene_view, Projection, ResolvedSceneTransforms, SceneTransformResolver, SceneView,
};

pub(crate) struct WorldViewCollector {
    transform_resolver: SceneTransformResolver,
    view_query: PreparedQuery<(
        &'static Transform,
        &'static Camera,
        Option<&'static Projection>,
        Option<&'static CameraViewport>,
        Option<&'static MainCamera>,
    )>,
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
                    views.push(build_scene_view(
                        transform,
                        projection,
                        Some(viewport),
                        surface_size,
                    ));
                    return;
                }

                if first_camera.is_none() {
                    first_camera = Some((transform, projection));
                }
                if main_camera.is_some() && first_main_camera.is_none() {
                    first_main_camera = Some((transform, projection));
                }
            },
        );

        if let Some(entity) = missing_projection_entity {
            self.report_missing_projection(world, entity);
        }

        if views.is_empty() {
            if let Some((transform, projection)) = first_main_camera.or(first_camera) {
                views.push(build_scene_view(transform, projection, None, surface_size));
            }
        }

        views
    }

    fn report_missing_projection(&mut self, world: &World, entity: EntityId) {
        let kind = EngineDiagnosticKind::CameraMissingProjection { entity };
        if let Some(diagnostics) = world.get_resource::<Diagnostics>() {
            let _ = diagnostics.report_once(kind);
        }
    }
}

impl Default for WorldViewCollector {
    fn default() -> Self {
        Self {
            transform_resolver: SceneTransformResolver::default(),
            view_query: PreparedQuery::new(),
        }
    }
}
