use crate::ecs::EntityId;
use crate::render::view::SceneView;
use crate::render::{SortingLayer, Transform};

#[inline]
pub fn entity_sort_key(entity: EntityId) -> u64 {
    ((entity.index() as u64) << 32) | entity.generation() as u64
}

pub fn transparent_sort_key(
    sorting_layer: SortingLayer,
    batch_key: u64,
    transform: Transform,
    scene_view: &SceneView,
) -> u64 {
    let layer = biased_i16(sorting_layer.0) as u64;
    let batch = batch_bucket(batch_key) as u64;
    let depth = transparent_depth_key(transform, scene_view);

    (layer << 48) | (batch << 36) | depth
}

pub(crate) fn transparent_ordered_2d_sort_key(
    sorting_layer: SortingLayer,
    batch_key: u64,
    local_order: u64,
) -> u64 {
    let layer = biased_i16(sorting_layer.0) as u64;
    let batch = batch_bucket(batch_key) as u64;
    let order = local_order & ((1u64 << 36) - 1);

    (layer << 48) | (batch << 36) | order
}

pub fn opaque_sort_key(
    batch_key: u64,
    entity: EntityId,
    transform: Transform,
    scene_view: &SceneView,
) -> u64 {
    let batch = batch_bucket(batch_key) as u64;
    let depth = opaque_depth_key(transform, scene_view) as u64;
    let entity = entity_sort_key(entity) & ((1u64 << 36) - 1);

    (batch << 52) | (depth << 36) | entity
}

fn biased_i16(value: i32) -> u16 {
    let clamped = value.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    (clamped as u16) ^ 0x8000
}

fn batch_bucket(batch_key: u64) -> u16 {
    let draw_function = ((batch_key >> 56) & 0x0f) as u16;
    let material = ((batch_key >> 48) & 0xff) as u16;
    (draw_function << 8) | material
}

fn transparent_depth_key(transform: Transform, scene_view: &SceneView) -> u64 {
    let depth = if scene_view.is_planar_2d {
        transform.z()
    } else {
        -scene_view.view_depth([transform.x(), transform.y(), transform.z()])
    };
    ((ordered_f32(depth) as u64) << 4) & ((1u64 << 36) - 1)
}

fn opaque_depth_key(transform: Transform, scene_view: &SceneView) -> u16 {
    let depth = if scene_view.is_planar_2d {
        transform.z()
    } else {
        scene_view.view_depth([transform.x(), transform.y(), transform.z()])
    };
    (ordered_f32(depth) >> 16) as u16
}

fn ordered_f32(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits ^ 0x8000_0000
    }
}
