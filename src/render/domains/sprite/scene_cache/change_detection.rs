use super::{SceneCache2D, SceneLightItem, SceneSpriteItem};

#[derive(Clone, Copy)]
pub(super) struct SpriteChangeFlags {
    pub(super) record_changed: bool,
}

pub(super) fn sprite_change_flags(
    lhs: &SceneSpriteItem,
    rhs: &SceneSpriteItem,
) -> SpriteChangeFlags {
    let record_changed = lhs.texture_sort_key != rhs.texture_sort_key
        || lhs.sorting_layer != rhs.sorting_layer
        || lhs.order_in_layer != rhs.order_in_layer
        || !transform_matches(&lhs.transform, &rhs.transform)
        || !sprite_matches(&lhs.sprite, &rhs.sprite);
    SpriteChangeFlags { record_changed }
}

pub(super) fn scene_light_matches(lhs: &SceneLightItem, rhs: &SceneLightItem) -> bool {
    transform_matches(&lhs.transform, &rhs.transform) && point_light_matches(&lhs.light, &rhs.light)
}

impl SceneCache2D {
    pub(crate) fn clear_dirty_tracking(&mut self) {
        self.dirty_sprite_slots.clear();
        self.dirty_light_slots.clear();
        self.dirty_epoch = self.dirty_epoch.wrapping_add(1);
        if self.dirty_epoch == 0 {
            self.dirty_epoch = 1;
            self.sprite_dirty_stamps.fill(0);
            self.light_dirty_stamps.fill(0);
        }
    }

    pub(super) fn mark_sprite_upload_dirty(&mut self, slot: usize) {
        if slot >= self.sprite_dirty_stamps.len() {
            self.sprite_dirty_stamps.resize(slot + 1, 0);
        }
        if self.sprite_dirty_stamps[slot] == self.dirty_epoch {
            return;
        }
        self.sprite_dirty_stamps[slot] = self.dirty_epoch;
        self.dirty_sprite_slots.push(slot as u32);
    }

    pub(super) fn mark_light_dirty(&mut self, slot: usize) {
        if slot >= self.light_dirty_stamps.len() {
            self.light_dirty_stamps.resize(slot + 1, 0);
        }
        if self.light_dirty_stamps[slot] == self.dirty_epoch {
            return;
        }
        self.light_dirty_stamps[slot] = self.dirty_epoch;
        self.dirty_light_slots.push(slot as u32);
    }
}

fn texture_option_matches(
    lhs: &Option<crate::render::Texture>,
    rhs: &Option<crate::render::Texture>,
) -> bool {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => lhs.ptr_eq(rhs),
        (None, None) => true,
        _ => false,
    }
}

fn transform_matches(lhs: &crate::render::Transform, rhs: &crate::render::Transform) -> bool {
    lhs == rhs
}

fn sprite_matches(
    lhs: &crate::render::SpriteRenderer,
    rhs: &crate::render::SpriteRenderer,
) -> bool {
    lhs.width == rhs.width
        && lhs.height == rhs.height
        && color_matches(lhs.color, rhs.color)
        && lhs.uv == rhs.uv
        && texture_option_matches(&lhs.texture, &rhs.texture)
        && lhs.visible == rhs.visible
        && lhs.layer_mask == rhs.layer_mask
}

fn point_light_matches(
    lhs: &crate::render::PointLight2D,
    rhs: &crate::render::PointLight2D,
) -> bool {
    lhs.radius == rhs.radius
        && lhs.intensity == rhs.intensity
        && color_matches(lhs.color, rhs.color)
        && lhs.temperature == rhs.temperature
        && lhs.falloff == rhs.falloff
        && lhs.visible == rhs.visible
        && lhs.layer_mask == rhs.layer_mask
}

fn color_matches(lhs: crate::render::Color, rhs: crate::render::Color) -> bool {
    lhs.r == rhs.r && lhs.g == rhs.g && lhs.b == rhs.b && lhs.a == rhs.a
}
