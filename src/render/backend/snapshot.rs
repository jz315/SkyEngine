use rustc_hash::FxHashMap;

use crate::asset::AssetId;
use crate::ecs::{EntityId, PreparedQuery, World};
use crate::math::Projection;
use crate::render::component::{
    Camera as CameraMarker, DirectionalLight, MeshRenderer, PointLight, RenderLayerMask,
    RenderSettings, SpotLight, Transform,
};

/// Per-frame backend-neutral 3D scene counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneSnapshotStats {
    pub mesh_instances: usize,
    pub directional_lights: usize,
    pub point_lights: usize,
    pub spot_lights: usize,
    pub cameras: usize,
}

/// Backend-neutral mesh instance payload extracted from ECS.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneMeshInstance {
    pub mesh: AssetId,
    pub materials: Vec<AssetId>,
    pub transform: [f32; 16],
    pub layer_mask: u32,
    pub casts_shadows: bool,
    pub shadow_cascade_mask: u8,
}

/// Backend-neutral directional light payload extracted from ECS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneDirectionalLight {
    pub transform: [f32; 16],
    pub direction: [f32; 3],
    pub intensity: f32,
    pub color: [f32; 4],
    pub radius: f32,
    pub layer_mask: u32,
    pub casts_shadows: bool,
    pub shadow_map_size: u32,
    pub shadow_bias: f32,
}

/// Backend-neutral point light payload extracted from ECS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePointLight {
    pub transform: [f32; 16],
    pub position: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
    pub color: [f32; 4],
    pub temperature: f32,
    pub falloff: f32,
    pub layer_mask: u32,
}

/// Backend-neutral spot light payload extracted from ECS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneSpotLight {
    pub transform: [f32; 16],
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
    pub color: [f32; 4],
    pub temperature: f32,
    pub falloff: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
    pub layer_mask: u32,
    pub casts_shadows: bool,
    pub shadow_resolution: u32,
    pub shadow_bias: f32,
}

/// Backend-neutral camera payload extracted from ECS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneCamera {
    pub transform: [f32; 16],
    pub projection: Projection,
}

/// A semantic 3D scene frame consumed by native renderer backends.
#[derive(Clone, Debug, Default)]
pub struct SceneSnapshot {
    mesh_instances: FxHashMap<EntityId, SceneMeshInstance>,
    directional_lights: FxHashMap<EntityId, SceneDirectionalLight>,
    point_lights: FxHashMap<EntityId, ScenePointLight>,
    spot_lights: FxHashMap<EntityId, SceneSpotLight>,
    cameras: FxHashMap<EntityId, SceneCamera>,
    render_settings: RenderSettings,
}

impl SceneSnapshot {
    #[inline]
    pub fn clear(&mut self) {
        self.mesh_instances.clear();
        self.directional_lights.clear();
        self.point_lights.clear();
        self.spot_lights.clear();
        self.cameras.clear();
        self.render_settings = RenderSettings::default();
    }

    #[inline]
    pub fn stats(&self) -> SceneSnapshotStats {
        SceneSnapshotStats {
            mesh_instances: self.mesh_instances.len(),
            directional_lights: self.directional_lights.len(),
            point_lights: self.point_lights.len(),
            spot_lights: self.spot_lights.len(),
            cameras: self.cameras.len(),
        }
    }

    #[inline]
    pub fn mesh_instance(&self, entity: EntityId) -> Option<&SceneMeshInstance> {
        self.mesh_instances.get(&entity)
    }

    #[inline]
    pub fn directional_light(&self, entity: EntityId) -> Option<&SceneDirectionalLight> {
        self.directional_lights.get(&entity)
    }

    #[inline]
    pub fn point_light(&self, entity: EntityId) -> Option<&ScenePointLight> {
        self.point_lights.get(&entity)
    }

    #[inline]
    pub fn spot_light(&self, entity: EntityId) -> Option<&SceneSpotLight> {
        self.spot_lights.get(&entity)
    }

    #[inline]
    pub fn camera(&self, entity: EntityId) -> Option<&SceneCamera> {
        self.cameras.get(&entity)
    }

    #[inline]
    pub fn render_settings(&self) -> &RenderSettings {
        &self.render_settings
    }

    pub fn mesh_instances(&self) -> impl Iterator<Item = (EntityId, &SceneMeshInstance)> + '_ {
        self.mesh_instances
            .iter()
            .map(|(entity, instance)| (*entity, instance))
    }

    pub fn directional_lights(
        &self,
    ) -> impl Iterator<Item = (EntityId, &SceneDirectionalLight)> + '_ {
        self.directional_lights
            .iter()
            .map(|(entity, light)| (*entity, light))
    }

    pub fn point_lights(&self) -> impl Iterator<Item = (EntityId, &ScenePointLight)> + '_ {
        self.point_lights
            .iter()
            .map(|(entity, light)| (*entity, light))
    }

    pub fn spot_lights(&self) -> impl Iterator<Item = (EntityId, &SceneSpotLight)> + '_ {
        self.spot_lights
            .iter()
            .map(|(entity, light)| (*entity, light))
    }

    pub fn cameras(&self) -> impl Iterator<Item = (EntityId, &SceneCamera)> + '_ {
        self.cameras
            .iter()
            .map(|(entity, camera)| (*entity, camera))
    }

    fn insert_mesh_instance(&mut self, entity: EntityId, instance: SceneMeshInstance) {
        self.mesh_instances.insert(entity, instance);
    }

    fn insert_directional_light(&mut self, entity: EntityId, light: SceneDirectionalLight) {
        self.directional_lights.insert(entity, light);
    }

    fn insert_point_light(&mut self, entity: EntityId, light: ScenePointLight) {
        self.point_lights.insert(entity, light);
    }

    fn insert_spot_light(&mut self, entity: EntityId, light: SceneSpotLight) {
        self.spot_lights.insert(entity, light);
    }

    fn insert_camera(&mut self, entity: EntityId, camera: SceneCamera) {
        self.cameras.insert(entity, camera);
    }

    fn set_render_settings(&mut self, settings: RenderSettings) {
        self.render_settings = settings;
    }
}

/// Reusable extractor for building a [`SceneSnapshot`] from ECS components.
pub struct SceneSnapshotExtractor {
    mesh_query: PreparedQuery<(
        &'static Transform,
        &'static MeshRenderer,
        Option<&'static RenderLayerMask>,
    )>,
    directional_light_query: PreparedQuery<(
        &'static Transform,
        &'static DirectionalLight,
        Option<&'static RenderLayerMask>,
    )>,
    point_light_query: PreparedQuery<(
        &'static Transform,
        &'static PointLight,
        Option<&'static RenderLayerMask>,
    )>,
    spot_light_query: PreparedQuery<(
        &'static Transform,
        &'static SpotLight,
        Option<&'static RenderLayerMask>,
    )>,
    camera_query: PreparedQuery<(
        &'static Transform,
        &'static CameraMarker,
        &'static Projection,
    )>,
}

impl SceneSnapshotExtractor {
    pub fn new() -> Self {
        Self {
            mesh_query: PreparedQuery::new(),
            directional_light_query: PreparedQuery::new(),
            point_light_query: PreparedQuery::new(),
            spot_light_query: PreparedQuery::new(),
            camera_query: PreparedQuery::new(),
        }
    }

    pub fn extract(&mut self, world: &World) -> SceneSnapshot {
        let mut snapshot = SceneSnapshot::default();
        self.extract_into(world, &mut snapshot);
        snapshot
    }

    pub fn extract_into(&mut self, world: &World, snapshot: &mut SceneSnapshot) {
        snapshot.clear();
        snapshot.set_render_settings(
            world
                .get_resource::<RenderSettings>()
                .cloned()
                .unwrap_or_default(),
        );

        self.mesh_query
            .for_each_with_entity(world, |entity, (transform, renderer, layer_mask)| {
                if !renderer.visible {
                    return;
                }
                let effective_layer_mask = layer_mask.map_or(renderer.layer_mask, |mask| mask.0);
                let materials = renderer
                    .materials
                    .iter()
                    .map(|material| material.id())
                    .collect();
                snapshot.insert_mesh_instance(
                    entity,
                    SceneMeshInstance {
                        mesh: renderer.mesh.id(),
                        materials,
                        transform: transform.to_matrix4().to_cols_array(),
                        layer_mask: effective_layer_mask,
                        casts_shadows: renderer.casts_shadows,
                        shadow_cascade_mask: renderer.shadow_cascade_mask,
                    },
                );
            });

        self.directional_light_query.for_each_with_entity(
            world,
            |entity, (transform, light, layer_mask)| {
                if !light.visible {
                    return;
                }
                let effective_layer_mask = layer_mask.map_or(light.layer_mask, |mask| mask.0);
                snapshot.insert_directional_light(
                    entity,
                    SceneDirectionalLight {
                        transform: transform.to_matrix4().to_cols_array(),
                        direction: light.direction,
                        intensity: light.intensity,
                        color: light.color.to_array(),
                        radius: light.radius,
                        layer_mask: effective_layer_mask,
                        casts_shadows: light.casts_shadows,
                        shadow_map_size: light.shadow_map_size,
                        shadow_bias: light.shadow_bias,
                    },
                );
            },
        );

        self.point_light_query.for_each_with_entity(
            world,
            |entity, (transform, light, layer_mask)| {
                if !light.visible {
                    return;
                }
                let effective_layer_mask = layer_mask.map_or(light.layer_mask, |mask| mask.0);
                snapshot.insert_point_light(
                    entity,
                    ScenePointLight {
                        transform: transform.to_matrix4().to_cols_array(),
                        position: [transform.x(), transform.y(), transform.z()],
                        radius: light.radius,
                        intensity: light.intensity,
                        color: light.color.to_array(),
                        temperature: light.temperature,
                        falloff: light.falloff,
                        layer_mask: effective_layer_mask,
                    },
                );
            },
        );

        self.spot_light_query.for_each_with_entity(
            world,
            |entity, (transform, light, layer_mask)| {
                if !light.visible {
                    return;
                }
                let effective_layer_mask = layer_mask.map_or(light.layer_mask, |mask| mask.0);
                snapshot.insert_spot_light(
                    entity,
                    SceneSpotLight {
                        transform: transform.to_matrix4().to_cols_array(),
                        position: [transform.x(), transform.y(), transform.z()],
                        direction: light.direction,
                        radius: light.radius,
                        intensity: light.intensity,
                        color: light.color.to_array(),
                        temperature: light.temperature,
                        falloff: light.falloff,
                        inner_angle: light.inner_angle,
                        outer_angle: light.outer_angle,
                        layer_mask: effective_layer_mask,
                        casts_shadows: light.casts_shadows,
                        shadow_resolution: light.shadow_resolution,
                        shadow_bias: light.shadow_bias,
                    },
                );
            },
        );

        self.camera_query
            .for_each_with_entity(world, |entity, (transform, camera, projection)| {
                if !camera.enabled {
                    return;
                }
                snapshot.insert_camera(
                    entity,
                    SceneCamera {
                        transform: transform.to_matrix4().to_cols_array(),
                        projection: *projection,
                    },
                );
            });
    }
}

impl Default for SceneSnapshotExtractor {
    fn default() -> Self {
        Self::new()
    }
}
