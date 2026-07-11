#[cfg(feature = "live2d")]
use rustc_hash::FxHashMap;

#[cfg(feature = "live2d")]
use crate::ecs::{EntityId, PreparedQuery, World};
#[cfg(feature = "live2d")]
use crate::gpu::GpuContext;
#[cfg(feature = "live2d")]
use crate::render::component::{
    Live2DAnimator, Live2DCommand, Live2DCommands, Live2DModelInstance, RenderLayerMask,
    SortingLayer, Transform,
};
#[cfg(feature = "live2d")]
use crate::render::execution::{PreparedFrame, PreparedView, TextureFormat};
#[cfg(feature = "live2d")]
use crate::render::live2d::backend::Live2DBackend;
#[cfg(feature = "live2d")]
use crate::render::live2d::{Live2DLoadError, Live2DModel, Live2DModelResource, Live2DUserModel};
#[cfg(feature = "live2d")]
use crate::render::phase::{
    transparent_sort_key, DrawFunctionId, Live2DDrawData, PhaseItem, TransparentPhase,
};

#[cfg(feature = "live2d")]
use crate::render::view::{column_major_mul, scene_transform_matrix};
#[cfg(feature = "live2d")]
use crate::render::view::{RenderQueueSort, ResolvedSceneTransforms, SceneView};

#[cfg(feature = "live2d")]
type Live2DInstanceQuery = (
    &'static Live2DModelInstance,
    Option<&'static Transform>,
    Option<&'static RenderLayerMask>,
    Option<&'static SortingLayer>,
    Option<&'static Live2DAnimator>,
);

#[cfg(feature = "live2d")]
pub struct Live2DFeature {
    backend: Live2DBackend,
    draw_function_id: Option<DrawFunctionId>,
    instance_query: PreparedQuery<Live2DInstanceQuery>,
    entity_to_index: FxHashMap<EntityId, usize>,
    failed_entities: FxHashMap<EntityId, String>,
    pending_instances: Vec<PendingLive2DInstance>,
    sort_policy: RenderQueueSort,
    target_format: TextureFormat,
    phase_views: Vec<PreparedLive2DPhaseView>,
    pending_commands: Vec<Live2DCommand>,
    frame_delta: f32,
}

#[cfg(feature = "live2d")]
impl Live2DFeature {
    pub fn new() -> Self {
        Self {
            backend: Live2DBackend::new(),
            draw_function_id: None,
            instance_query: PreparedQuery::new(),
            entity_to_index: FxHashMap::default(),
            failed_entities: FxHashMap::default(),
            pending_instances: Vec::new(),
            sort_policy: RenderQueueSort::TransparentScene,
            target_format: TextureFormat::Bgra8Unorm,
            phase_views: Vec::new(),
            pending_commands: Vec::new(),
            frame_delta: 0.0,
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

    pub(crate) fn phase_view(&self, view_index: usize) -> Option<&PreparedLive2DPhaseView> {
        self.phase_views.get(view_index)
    }

    pub(crate) fn set_draw_function_id(&mut self, draw_function_id: DrawFunctionId) {
        self.draw_function_id = Some(draw_function_id);
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
            |entity, (instance, transform, layer_mask, sorting_layer, animator)| {
                instances.push(PendingLive2DInstance {
                    entity,
                    instance: instance.clone(),
                    transform: transforms
                        .get(entity)
                        .or_else(|| transform.copied())
                        .unwrap_or_default(),
                    layer_mask: layer_mask.map(|mask| mask.0).unwrap_or(u32::MAX),
                    sorting_layer: sorting_layer.copied().unwrap_or_default(),
                    animator: animator.copied(),
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

    fn resolve_instance_transform(
        &self,
        model_index: usize,
        instance: &Live2DModelInstance,
        mut transform: Transform,
    ) -> Transform {
        if let Some(height) = instance.height {
            if let Some(model) = self.backend.model(model_index) {
                let model_height = model.render_size_units()[1].max(f32::EPSILON);
                let scale = height / model_height;
                transform.scale[0] *= scale;
                transform.scale[1] *= scale;
            }
        }
        transform
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
    animator: Option<Live2DAnimator>,
}

#[cfg(feature = "live2d")]
pub(crate) struct PreparedLive2DPhaseView {
    pub(crate) transparent_phase: TransparentPhase,
    frames: Vec<crate::render::live2d::PreparedLive2DFrame>,
}

#[cfg(feature = "live2d")]
impl PreparedLive2DPhaseView {
    fn new() -> Self {
        Self {
            transparent_phase: TransparentPhase::new(),
            frames: Vec::new(),
        }
    }

    fn push(&mut self, item: PhaseItem, frame: crate::render::live2d::PreparedLive2DFrame) {
        self.transparent_phase.add_item(item);
        self.frames.push(frame);
    }

    pub(crate) fn frame(
        &self,
        index: usize,
    ) -> Option<&crate::render::live2d::PreparedLive2DFrame> {
        self.frames.get(index)
    }
}

#[cfg(feature = "live2d")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Live2DSceneInstance {
    pub(crate) entity: EntityId,
    pub(crate) model_index: usize,
    pub(crate) transform: Transform,
    pub(crate) layer_mask: u32,
    pub(crate) sorting_layer: SortingLayer,
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
            .then_with(|| transparent_scene_depth_cmp(lhs.transform, rhs.transform, scene_view))
            .then_with(|| {
                live2d_scene_entity_key(lhs.entity).cmp(&live2d_scene_entity_key(rhs.entity))
            }),
        RenderQueueSort::OpaqueDepthFrontToBack => {
            opaque_scene_depth_cmp(lhs.transform, rhs.transform, scene_view).then_with(|| {
                live2d_scene_entity_key(lhs.entity).cmp(&live2d_scene_entity_key(rhs.entity))
            })
        }
        RenderQueueSort::OverlayStable => {
            lhs.sorting_layer.cmp(&rhs.sorting_layer).then_with(|| {
                live2d_scene_entity_key(lhs.entity).cmp(&live2d_scene_entity_key(rhs.entity))
            })
        }
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
    if scene_view.is_planar_2d {
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
    if scene_view.is_planar_2d {
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
impl Default for Live2DFeature {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "live2d")]
impl Live2DFeature {
    pub(crate) fn set_target_format(&mut self, target_format: TextureFormat) {
        self.target_format = target_format;
    }

    pub(crate) fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        surface_size: [u32; 2],
    ) {
        self.backend.extract(world, surface_size);
        self.frame_delta = world.time.frame_delta;
        if let Some(commands) = world.get_resource::<Live2DCommands>() {
            self.pending_commands.extend(commands.drain());
        }
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

    pub(crate) fn collect_views(&self, views: &mut Vec<SceneView>) {
        self.backend.collect_views(views);
    }

    pub(crate) fn prepare(&mut self, gpu: &mut GpuContext, views: &[SceneView]) {
        let draw_function_id = self
            .draw_function_id
            .expect("Live2DFeature must register its draw function before prepare");
        let pending_instances = self.pending_instances.clone();
        let mut scene_instances = Vec::new();

        for pending in &pending_instances {
            let entity = pending.entity;
            let instance = &pending.instance;
            let transform = pending.transform;
            let layer_mask = pending.layer_mask;
            let sorting_layer = pending.sorting_layer;
            if self.entity_to_index.contains_key(&entity)
                || self.failed_entities.contains_key(&entity)
            {
                if let Some(index) = self.entity_to_index.get(&entity).copied() {
                    self.backend.set_visible(index, instance.visible);
                    if instance.visible {
                        let transform = self.resolve_instance_transform(index, instance, transform);
                        scene_instances.push(Live2DSceneInstance {
                            entity,
                            model_index: index,
                            transform,
                            layer_mask,
                            sorting_layer,
                        });
                    }
                }
                continue;
            }
            match self.ensure_entity_loaded(entity, instance, gpu) {
                Ok(index) => {
                    self.backend.set_visible(index, instance.visible);
                    if instance.visible {
                        let transform = self.resolve_instance_transform(index, instance, transform);
                        scene_instances.push(Live2DSceneInstance {
                            entity,
                            model_index: index,
                            transform,
                            layer_mask,
                            sorting_layer,
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

        self.apply_pending_commands();
        self.update_animators(&pending_instances);

        self.phase_views.clear();
        self.phase_views.reserve(views.len());
        for (view_index, view) in views.iter().enumerate() {
            let mut view_instances = scene_instances
                .iter()
                .copied()
                .filter(|instance| live2d_instance_visible_in_view(instance, view))
                .collect::<Vec<_>>();
            sort_live2d_scene_instances(&mut view_instances, self.sort_policy, Some(view));
            let mut phase_view = PreparedLive2DPhaseView::new();

            for instance in &view_instances {
                let Some(projection) =
                    self.scene_projection_for_view(instance.model_index, instance.transform, view)
                else {
                    continue;
                };
                let Some(prepared) = self.backend.prepare_entry_for_view(
                    gpu,
                    self.target_format,
                    view.target_size,
                    instance.model_index,
                    &projection,
                ) else {
                    continue;
                };
                let frame_index = phase_view.frames.len() as u32;
                let batch_key = ((draw_function_id.index() as u64) & 0xff) << 56;
                phase_view.push(
                    PhaseItem::new(
                        transparent_sort_key(
                            instance.sorting_layer,
                            batch_key,
                            instance.transform,
                            view,
                        ),
                        draw_function_id,
                        instance.entity,
                        batch_key,
                        Live2DDrawData::new(frame_index),
                    ),
                    prepared,
                );
            }
            phase_view.transparent_phase.sort();
            let _ = view_index;
            self.phase_views.push(phase_view);
        }
    }

    fn apply_pending_commands(&mut self) {
        let commands = std::mem::take(&mut self.pending_commands);
        for command in commands {
            self.apply_command(command);
        }
    }

    fn apply_command(&mut self, command: Live2DCommand) {
        match command {
            Live2DCommand::PlayMotion {
                entity,
                group_name,
                index_in_group,
            } => {
                if let Some(model) = self.user_model_mut_for_entity(entity) {
                    let _ = model.set_motion(&group_name, index_in_group);
                }
            }
            Live2DCommand::PlayMotionByIndex { entity, index } => {
                if let Some(model) = self.user_model_mut_for_entity(entity) {
                    let _ = model.set_motion_by_index(index);
                }
            }
            Live2DCommand::SetExpression { entity, name } => {
                if let Some(model) = self.user_model_mut_for_entity(entity) {
                    let _ = model.set_expression(&name);
                }
            }
            Live2DCommand::SetLookTarget { entity, target } => {
                if let Some(model) = self.user_model_mut_for_entity(entity) {
                    let _ = model.set_look_target(target);
                }
            }
            Live2DCommand::ClearLookTarget { entity } => {
                if let Some(model) = self.user_model_mut_for_entity(entity) {
                    let _ = model.clear_look_target();
                }
            }
            Live2DCommand::TapScreen {
                entity,
                screen_position,
                view_size,
            } => {
                if let Some(model) = self.user_model_mut_for_entity(entity) {
                    let _ = model.handle_tap_screen(screen_position, view_size);
                }
            }
            Live2DCommand::TapModel { entity, point } => {
                if let Some(model) = self.user_model_mut_for_entity(entity) {
                    let _ = model.handle_tap_model_space(point);
                }
            }
        }
    }

    fn update_animators(&mut self, pending_instances: &[PendingLive2DInstance]) {
        if self.frame_delta <= 0.0 {
            return;
        }

        for pending in pending_instances {
            let Some(animator) = pending.animator else {
                continue;
            };
            if !animator.enabled || animator.speed <= 0.0 {
                continue;
            }
            if !pending.instance.visible && !animator.update_when_hidden {
                continue;
            }
            let Some(index) = self.entity_to_index.get(&pending.entity).copied() else {
                continue;
            };
            if let Some(model) = self.backend.user_model_mut(index) {
                model.update(self.frame_delta * animator.speed);
            }
        }
    }

    pub(crate) fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        self.backend.insert_frame_payloads(frame);
    }

    pub(crate) fn insert_view_payloads<'a>(
        &'a self,
        view_index: usize,
        view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    ) {
        self.backend
            .insert_view_payloads(view_index, view, prepared_view);
        if let Some(phase_view) = self.phase_views.get(view_index) {
            let _ = prepared_view.insert_payload(phase_view);
        }
    }
}
