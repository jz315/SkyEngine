#[cfg(feature = "live2d")]
use std::any::Any;

#[cfg(feature = "live2d")]
use rustc_hash::FxHashMap;

#[cfg(feature = "live2d")]
use crate::ecs::{EntityId, PreparedQuery, World};
#[cfg(feature = "live2d")]
use crate::gpu::GpuContext;
#[cfg(feature = "live2d")]
use crate::render::domains::live2d::backend::Live2DBackend;
#[cfg(feature = "live2d")]
use crate::render::domains::RenderDomain;
#[cfg(feature = "live2d")]
use crate::render::ecs::{
    Live2DModelInstance, OrderInLayer, RenderLayerMask, SortingLayer, Transform,
};
#[cfg(feature = "live2d")]
use crate::render::frame_pipeline::{FrameViewNode, PreparedFrame, PreparedView, TextureFormat};
#[cfg(feature = "live2d")]
use crate::render::live2d::{Live2DLoadError, Live2DModel, Live2DModelResource, Live2DUserModel};

#[cfg(feature = "live2d")]
use crate::render::scene::{column_major_mul, scene_transform_matrix};
#[cfg(feature = "live2d")]
use crate::render::scene::{RenderQueueSort, ResolvedSceneTransforms, SceneView};

#[cfg(feature = "live2d")]
pub struct Live2DDomain {
    backend: Live2DBackend,
    instance_query: PreparedQuery<(
        &'static Live2DModelInstance,
        Option<&'static Transform>,
        Option<&'static RenderLayerMask>,
        Option<&'static SortingLayer>,
        Option<&'static OrderInLayer>,
    )>,
    entity_to_index: FxHashMap<EntityId, usize>,
    failed_entities: FxHashMap<EntityId, String>,
    pending_instances: Vec<PendingLive2DInstance>,
    sort_policy: RenderQueueSort,
    target_format: TextureFormat,
}

#[cfg(feature = "live2d")]
impl Live2DDomain {
    pub fn new() -> Self {
        Self {
            backend: Live2DBackend::new(),
            instance_query: PreparedQuery::new(),
            entity_to_index: FxHashMap::default(),
            failed_entities: FxHashMap::default(),
            pending_instances: Vec::new(),
            sort_policy: RenderQueueSort::TransparentScene,
            target_format: TextureFormat::Bgra8Unorm,
        }
    }

    pub fn load_model(
        &mut self,
        gpu: &GpuContext,
        path: impl AsRef<std::path::Path>,
    ) -> Result<usize, Live2DLoadError> {
        self.backend.load_model(gpu, path)
    }

    pub fn ensure_entity_loaded(
        &mut self,
        entity: EntityId,
        instance: &Live2DModelInstance,
        gpu: &GpuContext,
    ) -> Result<usize, Live2DLoadError> {
        if let Some(index) = self.entity_to_index.get(&entity).copied() {
            self.backend.set_visible(index, instance.visible);
            return Ok(index);
        }
        let index = self.backend.load_model(gpu, &instance.model_path)?;
        self.entity_to_index.insert(entity, index);
        self.failed_entities.remove(&entity);
        self.backend.set_visible(index, instance.visible);
        Ok(index)
    }

    #[inline]
    pub fn model_count(&self) -> usize {
        self.backend.model_count()
    }

    pub fn user_model_mut(&mut self, index: usize) -> Option<&mut Live2DUserModel> {
        self.backend.user_model_mut(index)
    }

    pub fn user_model(&self, index: usize) -> Option<&Live2DUserModel> {
        self.backend.user_model(index)
    }

    pub fn model_resource(&self, index: usize) -> Option<&Live2DModelResource> {
        self.backend.model_resource(index)
    }

    pub fn model(&self, index: usize) -> Option<&Live2DModel> {
        self.backend.model(index)
    }

    pub fn set_visible(&mut self, index: usize, visible: bool) {
        self.backend.set_visible(index, visible);
    }

    pub fn model_index_for_entity(&self, entity: EntityId) -> Option<usize> {
        self.entity_to_index.get(&entity).copied()
    }

    pub fn user_model_mut_for_entity(&mut self, entity: EntityId) -> Option<&mut Live2DUserModel> {
        let index = self.model_index_for_entity(entity)?;
        self.backend.user_model_mut(index)
    }

    pub fn user_model_for_entity(&self, entity: EntityId) -> Option<&Live2DUserModel> {
        let index = self.model_index_for_entity(entity)?;
        self.backend.user_model(index)
    }

    #[inline]
    pub fn set_active_only(&mut self, index: usize) {
        self.backend.set_active_only(index);
    }

    #[inline]
    pub fn clear_active_only(&mut self) {
        self.backend.clear_active_only();
    }

    fn collect_pending_instances(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
    ) -> Vec<PendingLive2DInstance> {
        let mut instances = Vec::new();
        self.instance_query.for_each_with_entity(
            world,
            |entity, (instance, transform, layer_mask, sorting_layer, order_in_layer)| {
                instances.push(PendingLive2DInstance {
                    entity,
                    instance: instance.clone(),
                    transform: transforms
                        .get(entity)
                        .or_else(|| transform.copied())
                        .unwrap_or_default(),
                    layer_mask: layer_mask.map(|mask| mask.0).unwrap_or(u32::MAX),
                    sorting_layer: sorting_layer.copied().unwrap_or_default(),
                    order_in_layer: order_in_layer.copied().unwrap_or_default(),
                });
            },
        );
        instances
    }

    fn scene_projection_for_view(
        &self,
        model_index: usize,
        transform: Transform,
        view: &SceneView,
    ) -> Option<[f32; 16]> {
        let model_matrix = self.backend.model(model_index)?.render_matrix();
        let world_matrix = scene_transform_matrix(transform);
        Some(column_major_mul(
            view.view_uniform.view_proj,
            column_major_mul(world_matrix, model_matrix),
        ))
    }
}

#[cfg(feature = "live2d")]
#[derive(Clone)]
struct PendingLive2DInstance {
    entity: EntityId,
    instance: Live2DModelInstance,
    transform: Transform,
    layer_mask: u32,
    sorting_layer: SortingLayer,
    order_in_layer: OrderInLayer,
}

#[cfg(feature = "live2d")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Live2DSceneInstance {
    pub(crate) entity: EntityId,
    pub(crate) model_index: usize,
    pub(crate) transform: Transform,
    pub(crate) layer_mask: u32,
    pub(crate) sorting_layer: SortingLayer,
    pub(crate) order_in_layer: OrderInLayer,
}

#[cfg(feature = "live2d")]
fn live2d_scene_entity_key(entity: EntityId) -> u64 {
    ((entity.generation() as u64) << 32) | entity.index() as u64
}

#[cfg(feature = "live2d")]
pub(crate) fn sort_live2d_scene_instances(
    instances: &mut [Live2DSceneInstance],
    sort_policy: RenderQueueSort,
    scene_view: Option<&SceneView>,
) {
    instances.sort_by(|lhs, rhs| match sort_policy {
        RenderQueueSort::TransparentScene => lhs
            .sorting_layer
            .cmp(&rhs.sorting_layer)
            .then_with(|| lhs.order_in_layer.cmp(&rhs.order_in_layer))
            .then_with(|| transparent_scene_depth_cmp(lhs.transform, rhs.transform, scene_view))
            .then_with(|| {
                live2d_scene_entity_key(lhs.entity).cmp(&live2d_scene_entity_key(rhs.entity))
            }),
        RenderQueueSort::OpaqueDepthFrontToBack => {
            opaque_scene_depth_cmp(lhs.transform, rhs.transform, scene_view).then_with(|| {
                live2d_scene_entity_key(lhs.entity).cmp(&live2d_scene_entity_key(rhs.entity))
            })
        }
        RenderQueueSort::OverlayStable => lhs
            .sorting_layer
            .cmp(&rhs.sorting_layer)
            .then_with(|| lhs.order_in_layer.cmp(&rhs.order_in_layer))
            .then_with(|| {
                live2d_scene_entity_key(lhs.entity).cmp(&live2d_scene_entity_key(rhs.entity))
            }),
    });
}

#[cfg(feature = "live2d")]
pub(crate) fn live2d_instance_visible_in_view(
    instance: &Live2DSceneInstance,
    view: &SceneView,
) -> bool {
    instance.layer_mask & view.layer_mask != 0
}

#[cfg(feature = "live2d")]
fn transparent_scene_depth_cmp(
    lhs: Transform,
    rhs: Transform,
    scene_view: Option<&SceneView>,
) -> std::cmp::Ordering {
    let Some(scene_view) = scene_view else {
        return lhs.z().total_cmp(&rhs.z());
    };
    if scene_view.cull_camera_2d.is_some() {
        lhs.z().total_cmp(&rhs.z())
    } else {
        scene_depth_for_sort(rhs, scene_view).total_cmp(&scene_depth_for_sort(lhs, scene_view))
    }
}

#[cfg(feature = "live2d")]
fn opaque_scene_depth_cmp(
    lhs: Transform,
    rhs: Transform,
    scene_view: Option<&SceneView>,
) -> std::cmp::Ordering {
    let Some(scene_view) = scene_view else {
        return lhs.z().total_cmp(&rhs.z());
    };
    if scene_view.cull_camera_2d.is_some() {
        lhs.z().total_cmp(&rhs.z())
    } else {
        scene_depth_for_sort(lhs, scene_view).total_cmp(&scene_depth_for_sort(rhs, scene_view))
    }
}

#[cfg(feature = "live2d")]
fn scene_depth_for_sort(transform: Transform, scene_view: &SceneView) -> f32 {
    scene_view.view_depth([transform.x(), transform.y(), transform.z()])
}

#[cfg(feature = "live2d")]
impl Default for Live2DDomain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "live2d")]
impl RenderDomain for Live2DDomain {
    fn name(&self) -> &'static str {
        "live2d"
    }

    fn configure_queue_sort(&mut self, sort_policy: RenderQueueSort) {
        self.sort_policy = sort_policy;
    }

    fn configure_target_format(&mut self, target_format: TextureFormat) {
        self.target_format = target_format;
    }

    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    ) {
        self.backend.extract(world, surface_size);
        self.pending_instances = self.collect_pending_instances(world, transforms);
        let mut seen = FxHashMap::<EntityId, bool>::default();
        for pending in &self.pending_instances {
            seen.insert(pending.entity, pending.instance.visible);
        }
        for (&entity, &index) in &self.entity_to_index {
            self.backend
                .set_visible(index, seen.get(&entity).copied().unwrap_or(false));
        }
    }

    fn collect_views(&self, views: &mut Vec<SceneView>) {
        self.backend.collect_views(views);
    }

    fn prepare(&mut self, gpu: &mut GpuContext, _world: &World, views: &[SceneView]) {
        let pending_instances = self.pending_instances.clone();
        let mut scene_instances = Vec::new();

        for pending in pending_instances {
            let PendingLive2DInstance {
                entity,
                instance,
                transform,
                layer_mask,
                sorting_layer,
                order_in_layer,
            } = pending;
            if self.entity_to_index.contains_key(&entity)
                || self.failed_entities.contains_key(&entity)
            {
                if let Some(index) = self.entity_to_index.get(&entity).copied() {
                    self.backend.set_visible(index, instance.visible);
                    if instance.visible {
                        scene_instances.push(Live2DSceneInstance {
                            entity,
                            model_index: index,
                            transform,
                            layer_mask,
                            sorting_layer,
                            order_in_layer,
                        });
                    }
                }
                continue;
            }
            match self.ensure_entity_loaded(entity, &instance, gpu) {
                Ok(index) => {
                    self.backend.set_visible(index, instance.visible);
                    if instance.visible {
                        scene_instances.push(Live2DSceneInstance {
                            entity,
                            model_index: index,
                            transform,
                            layer_mask,
                            sorting_layer,
                            order_in_layer,
                        });
                    }
                }
                Err(error) => {
                    let _ = self.failed_entities.insert(entity, error.to_string());
                    eprintln!(
                        "[SkyEngine] Live2D model load failed for entity {:?}: {}",
                        entity, error
                    );
                }
            }
        }

        self.backend.clear_prepared_frames();
        for (view_index, view) in views.iter().enumerate() {
            let mut view_instances = scene_instances
                .iter()
                .copied()
                .filter(|instance| live2d_instance_visible_in_view(instance, view))
                .collect::<Vec<_>>();
            sort_live2d_scene_instances(&mut view_instances, self.sort_policy, Some(view));

            for instance in &view_instances {
                let Some(projection) =
                    self.scene_projection_for_view(instance.model_index, instance.transform, view)
                else {
                    continue;
                };
                self.backend.prepare_entry_for_view(
                    gpu,
                    self.target_format,
                    view_index,
                    view.target_size,
                    instance.model_index,
                    &projection,
                );
            }
        }
    }

    fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        self.backend.insert_frame_payloads(frame);
    }

    fn insert_view_payloads<'a>(
        &'a self,
        view_index: usize,
        view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    ) {
        self.backend
            .insert_view_payloads(view_index, view, prepared_view);
    }

    fn create_view_nodes(&mut self, ctx: &GpuContext) -> Vec<Box<dyn FrameViewNode>> {
        self.backend.create_view_nodes(ctx)
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.backend.resize(ctx, width, height);
    }

    fn surface_lost(&mut self) {
        self.backend.surface_lost();
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
