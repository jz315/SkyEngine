use std::cmp::Ordering;

use crate::render::scene::{RenderQueueSort, SceneView};

use super::super::scene_cache::{SceneCache2D, SceneSpriteItem};

pub(super) fn sort_visible_sprite_slots_for_view(
    slots: &mut [u32],
    sort_policy: RenderQueueSort,
    scene_view: &SceneView,
    scene: &SceneCache2D,
) {
    slots.sort_by(|lhs, rhs| {
        compare_sprite_items_for_view(
            scene.require_sprite_item(*lhs as usize),
            scene.require_sprite_item(*rhs as usize),
            sort_policy,
            scene_view,
        )
    });
}

fn compare_sprite_items_for_view(
    lhs: &SceneSpriteItem,
    rhs: &SceneSpriteItem,
    sort_policy: RenderQueueSort,
    scene_view: &SceneView,
) -> Ordering {
    match sort_policy {
        RenderQueueSort::TransparentScene => lhs
            .sorting_layer
            .cmp(&rhs.sorting_layer)
            .then_with(|| lhs.order_in_layer.cmp(&rhs.order_in_layer))
            .then_with(|| transparent_depth_cmp(lhs.transform, rhs.transform, scene_view))
            .then_with(|| lhs.texture_sort_key.cmp(&rhs.texture_sort_key))
            .then_with(|| lhs.sort_key.cmp(&rhs.sort_key)),
        RenderQueueSort::OpaqueDepthFrontToBack => {
            opaque_depth_cmp(lhs.transform, rhs.transform, scene_view)
                .then_with(|| lhs.texture_sort_key.cmp(&rhs.texture_sort_key))
                .then_with(|| lhs.sort_key.cmp(&rhs.sort_key))
        }
        RenderQueueSort::OverlayStable => lhs
            .sorting_layer
            .cmp(&rhs.sorting_layer)
            .then_with(|| lhs.order_in_layer.cmp(&rhs.order_in_layer))
            .then_with(|| lhs.texture_sort_key.cmp(&rhs.texture_sort_key))
            .then_with(|| lhs.sort_key.cmp(&rhs.sort_key)),
    }
}

fn transparent_depth_cmp(
    lhs: crate::render::Transform,
    rhs: crate::render::Transform,
    scene_view: &SceneView,
) -> Ordering {
    if scene_view.cull_camera_2d.is_some() {
        lhs.z().total_cmp(&rhs.z())
    } else {
        scene_depth_for_view(rhs, scene_view).total_cmp(&scene_depth_for_view(lhs, scene_view))
    }
}

fn opaque_depth_cmp(
    lhs: crate::render::Transform,
    rhs: crate::render::Transform,
    scene_view: &SceneView,
) -> Ordering {
    if scene_view.cull_camera_2d.is_some() {
        lhs.z().total_cmp(&rhs.z())
    } else {
        scene_depth_for_view(lhs, scene_view).total_cmp(&scene_depth_for_view(rhs, scene_view))
    }
}

fn scene_depth_for_view(transform: crate::render::Transform, scene_view: &SceneView) -> f32 {
    scene_view.view_depth([transform.x(), transform.y(), transform.z()])
}
