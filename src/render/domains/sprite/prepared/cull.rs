use crate::render::core::camera::Camera2D;

use super::super::scene_cache::{SceneLightItem, SceneSpriteItem};

pub(super) fn sprite_visible_in_camera(camera: Option<&Camera2D>, item: &SceneSpriteItem) -> bool {
    let Some(camera) = camera else {
        return true;
    };
    let (left, right, bottom, top) = camera_world_bounds(camera);
    let half_w = item.sprite.width.abs() * item.transform.scale_x().abs() * 0.5;
    let half_h = item.sprite.height.abs() * item.transform.scale_y().abs() * 0.5;
    let sprite_left = item.transform.x() - half_w;
    let sprite_right = item.transform.x() + half_w;
    let sprite_bottom = item.transform.y() - half_h;
    let sprite_top = item.transform.y() + half_h;
    sprite_right >= left && sprite_left <= right && sprite_top >= bottom && sprite_bottom <= top
}

pub(super) fn light_visible_in_camera(camera: Option<&Camera2D>, item: &SceneLightItem) -> bool {
    let Some(camera) = camera else {
        return true;
    };
    let (left, right, bottom, top) = camera_world_bounds(camera);
    let radius = item.light.radius.abs();
    let light_left = item.transform.x() - radius;
    let light_right = item.transform.x() + radius;
    let light_bottom = item.transform.y() - radius;
    let light_top = item.transform.y() + radius;
    light_right >= left && light_left <= right && light_top >= bottom && light_bottom <= top
}

fn camera_world_bounds(camera: &Camera2D) -> (f32, f32, f32, f32) {
    let hw = camera.viewport_width().max(f32::EPSILON) * 0.5 / camera.zoom.max(f32::EPSILON);
    let hh = camera.viewport_height().max(f32::EPSILON) * 0.5 / camera.zoom.max(f32::EPSILON);
    let (sin_r, cos_r) = camera.rotation.sin_cos();
    let extent_x = cos_r.abs() * hw + sin_r.abs() * hh;
    let extent_y = sin_r.abs() * hw + cos_r.abs() * hh;
    (
        camera.position[0] - extent_x,
        camera.position[0] + extent_x,
        camera.position[1] - extent_y,
        camera.position[1] + extent_y,
    )
}
