use rapier2d::prelude::{ColliderHandle, ColliderSet, RigidBodyHandle};
use rustc_hash::FxHashMap;

use crate::ecs::EntityId;

use super::components::{Collider2D, RigidBody2D};

#[derive(Default)]
pub(crate) struct PhysicsHandles {
    pub(crate) body_handles: FxHashMap<EntityId, RigidBodyHandle>,
    pub(crate) body_entities: FxHashMap<RigidBodyHandle, EntityId>,
    pub(crate) body_snapshots: FxHashMap<EntityId, RigidBody2D>,
    pub(crate) collider_handles: FxHashMap<EntityId, ColliderHandle>,
    pub(crate) collider_entities: FxHashMap<ColliderHandle, EntityId>,
    pub(crate) collider_snapshots: FxHashMap<EntityId, Collider2D>,
}

impl PhysicsHandles {
    pub(crate) fn bind_body(
        &mut self,
        entity: EntityId,
        handle: RigidBodyHandle,
        body: RigidBody2D,
    ) {
        self.body_handles.insert(entity, handle);
        self.body_entities.insert(handle, entity);
        self.body_snapshots.insert(entity, body);
    }

    pub(crate) fn unbind_body(&mut self, entity: EntityId) -> Option<RigidBodyHandle> {
        let handle = self.body_handles.remove(&entity)?;
        self.body_entities.remove(&handle);
        self.body_snapshots.remove(&entity);
        Some(handle)
    }

    pub(crate) fn drop_body_mapping(&mut self, entity: EntityId, handle: RigidBodyHandle) {
        self.body_handles.remove(&entity);
        self.body_entities.remove(&handle);
        self.body_snapshots.remove(&entity);
    }

    pub(crate) fn bind_collider(
        &mut self,
        entity: EntityId,
        handle: ColliderHandle,
        collider: Collider2D,
    ) {
        self.collider_handles.insert(entity, handle);
        self.collider_entities.insert(handle, entity);
        self.collider_snapshots.insert(entity, collider);
    }

    pub(crate) fn unbind_collider(&mut self, entity: EntityId) -> Option<ColliderHandle> {
        let handle = self.collider_handles.remove(&entity)?;
        self.collider_entities.remove(&handle);
        self.collider_snapshots.remove(&entity);
        Some(handle)
    }

    pub(crate) fn retain_live_colliders(&mut self, colliders: &ColliderSet) {
        self.collider_handles.retain(|entity, handle| {
            let alive = colliders.get(*handle).is_some();
            if !alive {
                self.collider_entities.remove(handle);
                self.collider_snapshots.remove(entity);
            }
            alive
        });
    }
}
