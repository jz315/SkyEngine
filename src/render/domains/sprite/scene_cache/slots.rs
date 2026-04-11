use crate::ecs::EntityId;

use super::{SceneCache2D, SceneLightItem, SceneSpriteItem};

#[derive(Clone, Default)]
pub(crate) struct SceneSpriteSlot {
    pub(crate) entity: Option<EntityId>,
    pub(crate) item: SceneSpriteItem,
    pub(crate) seen_epoch: u64,
    pub(crate) active_index: usize,
}

impl SceneSpriteSlot {
    #[inline]
    pub(crate) fn occupied(&self) -> bool {
        self.entity.is_some()
    }

    #[inline]
    fn occupied_entity(&self) -> EntityId {
        self.entity
            .expect("occupied sprite slots must keep their entity until release")
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SceneLightSlot {
    pub(crate) entity: Option<EntityId>,
    pub(crate) item: SceneLightItem,
    pub(crate) seen_epoch: u64,
    pub(crate) active_index: usize,
}

impl SceneLightSlot {
    #[inline]
    pub(crate) fn occupied(&self) -> bool {
        self.entity.is_some()
    }

    #[inline]
    fn occupied_entity(&self) -> EntityId {
        self.entity
            .expect("occupied light slots must keep their entity until release")
    }
}

impl SceneCache2D {
    pub(crate) fn release_sprite_slot(&mut self, slot: usize) {
        if slot >= self.sprite_slots.len() || !self.sprite_slots[slot].occupied() {
            return;
        }

        let entity = self.sprite_slots[slot].occupied_entity();
        self.sprite_slots[slot].entity = None;
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
    }

    pub(crate) fn release_light_slot(&mut self, slot: usize) {
        if slot >= self.light_slots.len() || !self.light_slots[slot].occupied() {
            return;
        }

        let entity = self.light_slots[slot].occupied_entity();
        self.light_slots[slot].entity = None;
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

    pub(super) fn allocate_sprite_slot(&mut self) -> usize {
        if let Some(slot) = self.free_sprite_slots.pop() {
            return slot;
        }

        let slot = self.sprite_slots.len();
        self.sprite_slots.push(SceneSpriteSlot::default());
        self.sprite_dirty_stamps.push(0);
        slot
    }

    pub(super) fn allocate_light_slot(&mut self) -> usize {
        if let Some(slot) = self.free_light_slots.pop() {
            return slot;
        }

        let slot = self.light_slots.len();
        self.light_slots.push(SceneLightSlot::default());
        self.light_dirty_stamps.push(0);
        slot
    }
}
