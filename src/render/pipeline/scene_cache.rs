//! Internal ECS-driven scene cache for the 2D renderer.

use rustc_hash::FxHashMap;

use crate::ecs::EntityId;
use crate::render::{
    Camera2D, PointLight2D, RenderSettings2D, RenderView2D, Sprite2D, Transform2D,
};

#[derive(Clone, Default)]
pub(crate) struct SceneSpriteItem {
    pub(crate) transform: Transform2D,
    pub(crate) sprite: Sprite2D,
    pub(crate) sort_key: u64,
    pub(crate) texture_sort_key: u64,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SceneLightItem {
    pub(crate) transform: Transform2D,
    pub(crate) light: PointLight2D,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneView2D {
    pub(crate) camera: Camera2D,
    pub(crate) render: RenderView2D,
}

impl SceneView2D {
    #[inline]
    pub(crate) const fn new(camera: Camera2D, render: RenderView2D) -> Self {
        Self { camera, render }
    }
}

#[derive(Clone, Default)]
pub(crate) struct SceneSpriteSlot {
    pub(crate) entity: Option<EntityId>,
    pub(crate) item: SceneSpriteItem,
    pub(crate) seen_epoch: u64,
    active_index: usize,
}

impl SceneSpriteSlot {
    #[inline]
    pub(crate) fn occupied(&self) -> bool {
        self.entity.is_some()
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SceneLightSlot {
    pub(crate) entity: Option<EntityId>,
    pub(crate) item: SceneLightItem,
    pub(crate) seen_epoch: u64,
    active_index: usize,
}

impl SceneLightSlot {
    #[inline]
    pub(crate) fn occupied(&self) -> bool {
        self.entity.is_some()
    }
}

pub(crate) struct SceneCache2D {
    pub(crate) settings: RenderSettings2D,
    pub(crate) views: Vec<SceneView2D>,
    sprite_slots: Vec<SceneSpriteSlot>,
    light_slots: Vec<SceneLightSlot>,
    sprite_entities: FxHashMap<EntityId, usize>,
    light_entities: FxHashMap<EntityId, usize>,
    active_sprite_slots: Vec<usize>,
    active_light_slots: Vec<usize>,
    free_sprite_slots: Vec<usize>,
    free_light_slots: Vec<usize>,
    sorted_sprite_slots: Vec<usize>,
    dirty_sprite_slots: Vec<u32>,
    dirty_light_slots: Vec<u32>,
    sprite_dirty_stamps: Vec<u64>,
    light_dirty_stamps: Vec<u64>,
    dirty_epoch: u64,
    sprite_sort_dirty: bool,
}

impl SceneCache2D {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn upsert_sprite(
        &mut self,
        entity: EntityId,
        item: SceneSpriteItem,
        seen_epoch: u64,
    ) -> usize {
        if let Some(&slot) = self.sprite_entities.get(&entity) {
            let (record_changed, order_changed) = {
                let slot_ref = &mut self.sprite_slots[slot];
                slot_ref.seen_epoch = seen_epoch;
                let change = sprite_change_flags(&slot_ref.item, &item);
                if change.record_changed {
                    slot_ref.item = item;
                }
                (change.record_changed, change.order_changed)
            };
            if record_changed {
                self.mark_sprite_upload_dirty(slot);
            }
            if order_changed {
                self.mark_sprite_order_dirty();
            }
            return slot;
        }

        let slot = self.allocate_sprite_slot();
        let active_index = self.active_sprite_slots.len();
        self.sprite_slots[slot] = SceneSpriteSlot {
            entity: Some(entity),
            item,
            seen_epoch,
            active_index,
        };
        self.sprite_entities.insert(entity, slot);
        self.active_sprite_slots.push(slot);
        self.mark_sprite_upload_dirty(slot);
        self.mark_sprite_order_dirty();
        slot
    }

    pub(crate) fn upsert_light(
        &mut self,
        entity: EntityId,
        item: SceneLightItem,
        seen_epoch: u64,
    ) -> usize {
        if let Some(&slot) = self.light_entities.get(&entity) {
            let mut changed = false;
            {
                let slot_ref = &mut self.light_slots[slot];
                slot_ref.seen_epoch = seen_epoch;
                if !scene_light_matches(&slot_ref.item, &item) {
                    slot_ref.item = item;
                    changed = true;
                }
            }
            if changed {
                self.mark_light_dirty(slot);
            }
            return slot;
        }

        let slot = self.allocate_light_slot();
        let active_index = self.active_light_slots.len();
        self.light_slots[slot] = SceneLightSlot {
            entity: Some(entity),
            item,
            seen_epoch,
            active_index,
        };
        self.light_entities.insert(entity, slot);
        self.active_light_slots.push(slot);
        self.mark_light_dirty(slot);
        slot
    }

    pub(crate) fn release_sprite_slot(&mut self, slot: usize) {
        if slot >= self.sprite_slots.len() || !self.sprite_slots[slot].occupied() {
            return;
        }

        let entity = self.sprite_slots[slot]
            .entity
            .take()
            .expect("occupied sprite slots must have an entity");
        self.sprite_entities.remove(&entity);

        let active_index = self.sprite_slots[slot].active_index;
        self.active_sprite_slots.swap_remove(active_index);
        if active_index < self.active_sprite_slots.len() {
            let moved_slot = self.active_sprite_slots[active_index];
            self.sprite_slots[moved_slot].active_index = active_index;
        }

        self.sprite_slots[slot].item = SceneSpriteItem::default();
        self.sprite_slots[slot].seen_epoch = 0;
        self.free_sprite_slots.push(slot);
        self.mark_sprite_upload_dirty(slot);
        self.mark_sprite_order_dirty();
    }

    pub(crate) fn release_light_slot(&mut self, slot: usize) {
        if slot >= self.light_slots.len() || !self.light_slots[slot].occupied() {
            return;
        }

        let entity = self.light_slots[slot]
            .entity
            .take()
            .expect("occupied light slots must have an entity");
        self.light_entities.remove(&entity);

        let active_index = self.light_slots[slot].active_index;
        self.active_light_slots.swap_remove(active_index);
        if active_index < self.active_light_slots.len() {
            let moved_slot = self.active_light_slots[active_index];
            self.light_slots[moved_slot].active_index = active_index;
        }

        self.light_slots[slot].item = SceneLightItem::default();
        self.light_slots[slot].seen_epoch = 0;
        self.free_light_slots.push(slot);
        self.mark_light_dirty(slot);
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn sprite_slot_for_entity(&self, entity: EntityId) -> Option<usize> {
        self.sprite_entities.get(&entity).copied()
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn light_slot_for_entity(&self, entity: EntityId) -> Option<usize> {
        self.light_entities.get(&entity).copied()
    }

    #[inline]
    pub(crate) fn active_sprite_slots(&self) -> &[usize] {
        &self.active_sprite_slots
    }

    #[inline]
    pub(crate) fn active_light_slots(&self) -> &[usize] {
        &self.active_light_slots
    }

    #[inline]
    pub(crate) fn sprite_item(&self, slot: usize) -> Option<&SceneSpriteItem> {
        self.sprite_slots
            .get(slot)
            .filter(|slot_ref| slot_ref.occupied())
            .map(|slot_ref| &slot_ref.item)
    }

    #[inline]
    pub(crate) fn light_item(&self, slot: usize) -> Option<&SceneLightItem> {
        self.light_slots
            .get(slot)
            .filter(|slot_ref| slot_ref.occupied())
            .map(|slot_ref| &slot_ref.item)
    }

    #[inline]
    pub(crate) fn sprite_seen_epoch(&self, slot: usize) -> Option<u64> {
        self.sprite_slots
            .get(slot)
            .filter(|slot_ref| slot_ref.occupied())
            .map(|slot_ref| slot_ref.seen_epoch)
    }

    #[inline]
    pub(crate) fn light_seen_epoch(&self, slot: usize) -> Option<u64> {
        self.light_slots
            .get(slot)
            .filter(|slot_ref| slot_ref.occupied())
            .map(|slot_ref| slot_ref.seen_epoch)
    }

    #[inline]
    pub(crate) fn sprite_slot_capacity(&self) -> usize {
        self.sprite_slots.len()
    }

    #[inline]
    pub(crate) fn light_slot_capacity(&self) -> usize {
        self.light_slots.len()
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn active_sprite_count(&self) -> usize {
        self.active_sprite_slots.len()
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn active_light_count(&self) -> usize {
        self.active_light_slots.len()
    }

    #[inline]
    pub(crate) fn dirty_sprite_slots(&self) -> &[u32] {
        &self.dirty_sprite_slots
    }

    #[inline]
    pub(crate) fn dirty_light_slots(&self) -> &[u32] {
        &self.dirty_light_slots
    }

    pub(crate) fn ensure_sorted_sprites(&mut self) {
        if !self.sprite_sort_dirty {
            return;
        }

        self.sorted_sprite_slots.clear();
        self.sorted_sprite_slots
            .extend(self.active_sprite_slots.iter().copied());
        self.sorted_sprite_slots.sort_by(|lhs, rhs| {
            let lhs_item = &self.sprite_slots[*lhs].item;
            let rhs_item = &self.sprite_slots[*rhs].item;
            lhs_item
                .transform
                .z
                .total_cmp(&rhs_item.transform.z)
                .then_with(|| lhs_item.texture_sort_key.cmp(&rhs_item.texture_sort_key))
                .then_with(|| lhs_item.sort_key.cmp(&rhs_item.sort_key))
        });
        self.sprite_sort_dirty = false;
    }

    #[inline]
    pub(crate) fn sorted_sprite_slots(&self) -> &[usize] {
        &self.sorted_sprite_slots
    }

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

    fn allocate_sprite_slot(&mut self) -> usize {
        if let Some(slot) = self.free_sprite_slots.pop() {
            return slot;
        }

        let slot = self.sprite_slots.len();
        self.sprite_slots.push(SceneSpriteSlot::default());
        self.sprite_dirty_stamps.push(0);
        slot
    }

    fn allocate_light_slot(&mut self) -> usize {
        if let Some(slot) = self.free_light_slots.pop() {
            return slot;
        }

        let slot = self.light_slots.len();
        self.light_slots.push(SceneLightSlot::default());
        self.light_dirty_stamps.push(0);
        slot
    }

    fn mark_sprite_order_dirty(&mut self) {
        self.sprite_sort_dirty = true;
    }

    fn mark_sprite_upload_dirty(&mut self, slot: usize) {
        if slot >= self.sprite_dirty_stamps.len() {
            self.sprite_dirty_stamps.resize(slot + 1, 0);
        }
        if self.sprite_dirty_stamps[slot] == self.dirty_epoch {
            return;
        }
        self.sprite_dirty_stamps[slot] = self.dirty_epoch;
        self.dirty_sprite_slots.push(slot as u32);
    }

    fn mark_light_dirty(&mut self, slot: usize) {
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

impl Default for SceneCache2D {
    fn default() -> Self {
        Self {
            settings: RenderSettings2D::default(),
            views: Vec::with_capacity(4),
            sprite_slots: Vec::with_capacity(256),
            light_slots: Vec::with_capacity(64),
            sprite_entities: FxHashMap::default(),
            light_entities: FxHashMap::default(),
            active_sprite_slots: Vec::with_capacity(256),
            active_light_slots: Vec::with_capacity(64),
            free_sprite_slots: Vec::with_capacity(64),
            free_light_slots: Vec::with_capacity(16),
            sorted_sprite_slots: Vec::with_capacity(256),
            dirty_sprite_slots: Vec::with_capacity(64),
            dirty_light_slots: Vec::with_capacity(32),
            sprite_dirty_stamps: Vec::with_capacity(256),
            light_dirty_stamps: Vec::with_capacity(64),
            dirty_epoch: 1,
            sprite_sort_dirty: true,
        }
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

fn scene_light_matches(lhs: &SceneLightItem, rhs: &SceneLightItem) -> bool {
    transform_matches(&lhs.transform, &rhs.transform) && point_light_matches(&lhs.light, &rhs.light)
}

#[derive(Clone, Copy)]
struct SpriteChangeFlags {
    record_changed: bool,
    order_changed: bool,
}

fn sprite_change_flags(lhs: &SceneSpriteItem, rhs: &SceneSpriteItem) -> SpriteChangeFlags {
    let order_changed = lhs.transform.z != rhs.transform.z
        || lhs.texture_sort_key != rhs.texture_sort_key
        || lhs.sort_key != rhs.sort_key;
    let record_changed = lhs.texture_sort_key != rhs.texture_sort_key
        || !transform_matches(&lhs.transform, &rhs.transform)
        || !sprite_matches(&lhs.sprite, &rhs.sprite);
    SpriteChangeFlags {
        record_changed,
        order_changed,
    }
}

fn transform_matches(lhs: &Transform2D, rhs: &Transform2D) -> bool {
    lhs.x == rhs.x
        && lhs.y == rhs.y
        && lhs.z == rhs.z
        && lhs.scale_x == rhs.scale_x
        && lhs.scale_y == rhs.scale_y
        && lhs.rotation == rhs.rotation
}

fn sprite_matches(lhs: &Sprite2D, rhs: &Sprite2D) -> bool {
    lhs.width == rhs.width
        && lhs.height == rhs.height
        && color_matches(lhs.color, rhs.color)
        && lhs.uv == rhs.uv
        && texture_option_matches(&lhs.texture, &rhs.texture)
        && lhs.visible == rhs.visible
        && lhs.layer_mask == rhs.layer_mask
}

fn point_light_matches(lhs: &PointLight2D, rhs: &PointLight2D) -> bool {
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
