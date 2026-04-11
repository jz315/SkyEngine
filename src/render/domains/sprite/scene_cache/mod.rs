//! Internal ECS-driven scene cache for the 2D renderer.

mod change_detection;
mod slots;

use rustc_hash::FxHashMap;

use crate::ecs::EntityId;
use crate::render::scene::RenderQueueSort;
use crate::render::{
    OrderInLayer, PointLight2D, RenderSettings, SortingLayer, SpriteRenderer, Transform,
};

use self::change_detection::{scene_light_matches, sprite_change_flags};
use self::slots::{SceneLightSlot, SceneSpriteSlot};

#[derive(Clone, Default)]
pub(crate) struct SceneSpriteItem {
    pub(crate) transform: Transform,
    pub(crate) sprite: SpriteRenderer,
    pub(crate) sorting_layer: SortingLayer,
    pub(crate) order_in_layer: OrderInLayer,
    pub(crate) sort_key: u64,
    pub(crate) texture_sort_key: u64,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SceneLightItem {
    pub(crate) transform: Transform,
    pub(crate) light: PointLight2D,
}

pub(crate) struct SceneCache2D {
    pub(crate) settings: RenderSettings,
    sprite_slots: Vec<SceneSpriteSlot>,
    light_slots: Vec<SceneLightSlot>,
    sprite_entities: FxHashMap<EntityId, usize>,
    light_entities: FxHashMap<EntityId, usize>,
    active_sprite_slots: Vec<usize>,
    active_light_slots: Vec<usize>,
    free_sprite_slots: Vec<usize>,
    free_light_slots: Vec<usize>,
    dirty_sprite_slots: Vec<u32>,
    dirty_light_slots: Vec<u32>,
    sprite_dirty_stamps: Vec<u64>,
    light_dirty_stamps: Vec<u64>,
    dirty_epoch: u64,
    sprite_sort_policy: RenderQueueSort,
}

impl SceneCache2D {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub(crate) fn set_sort_policy(&mut self, sort_policy: RenderQueueSort) {
        self.sprite_sort_policy = sort_policy;
    }

    #[inline]
    pub(crate) fn sprite_sort_policy(&self) -> RenderQueueSort {
        self.sprite_sort_policy
    }

    pub(crate) fn upsert_sprite(
        &mut self,
        entity: EntityId,
        item: SceneSpriteItem,
        seen_epoch: u64,
    ) -> usize {
        if let Some(&slot) = self.sprite_entities.get(&entity) {
            let record_changed = {
                let slot_ref = &mut self.sprite_slots[slot];
                slot_ref.seen_epoch = seen_epoch;
                let change = sprite_change_flags(&slot_ref.item, &item);
                if change.record_changed {
                    slot_ref.item = item;
                }
                change.record_changed
            };
            if record_changed {
                self.mark_sprite_upload_dirty(slot);
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
    pub(crate) fn require_sprite_item(&self, slot: usize) -> &SceneSpriteItem {
        self.sprite_item(slot)
            .unwrap_or_else(|| panic!("sprite slot {slot} must remain occupied during prepare"))
    }

    #[inline]
    pub(crate) fn require_light_item(&self, slot: usize) -> &SceneLightItem {
        self.light_item(slot)
            .unwrap_or_else(|| panic!("light slot {slot} must remain occupied during prepare"))
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

    #[inline]
    pub(crate) fn dirty_sprite_slots(&self) -> &[u32] {
        &self.dirty_sprite_slots
    }

    #[inline]
    pub(crate) fn dirty_light_slots(&self) -> &[u32] {
        &self.dirty_light_slots
    }
}

impl Default for SceneCache2D {
    fn default() -> Self {
        Self {
            settings: RenderSettings::default(),
            sprite_slots: Vec::with_capacity(256),
            light_slots: Vec::with_capacity(64),
            sprite_entities: FxHashMap::default(),
            light_entities: FxHashMap::default(),
            active_sprite_slots: Vec::with_capacity(256),
            active_light_slots: Vec::with_capacity(64),
            free_sprite_slots: Vec::with_capacity(64),
            free_light_slots: Vec::with_capacity(16),
            dirty_sprite_slots: Vec::with_capacity(64),
            dirty_light_slots: Vec::with_capacity(32),
            sprite_dirty_stamps: Vec::with_capacity(256),
            light_dirty_stamps: Vec::with_capacity(64),
            dirty_epoch: 1,
            sprite_sort_policy: RenderQueueSort::TransparentScene,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_policy_tracks_latest_value() {
        let mut scene = SceneCache2D::new();
        assert_eq!(
            scene.sprite_sort_policy(),
            RenderQueueSort::TransparentScene
        );
        scene.set_sort_policy(RenderQueueSort::OpaqueDepthFrontToBack);
        assert_eq!(
            scene.sprite_sort_policy(),
            RenderQueueSort::OpaqueDepthFrontToBack
        );
    }
}
