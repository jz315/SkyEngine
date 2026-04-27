use std::sync::Mutex;

use rapier2d::prelude::{
    ColliderHandle, ColliderSet, CollisionEvent, CollisionEventFlags, ContactPair, EventHandler,
    Real, RigidBodySet,
};
use rustc_hash::FxHashMap;

use crate::ecs::EntityId;

/// Collision or trigger event produced by the latest physics step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicsEvent2D {
    ContactStarted { a: EntityId, b: EntityId },
    ContactStopped { a: EntityId, b: EntityId },
    TriggerEntered { trigger: EntityId, other: EntityId },
    TriggerExited { trigger: EntityId, other: EntityId },
}

/// Drainable physics event queue.
#[derive(Default, Debug)]
pub struct PhysicsEvents {
    events: Vec<PhysicsEvent2D>,
}

impl PhysicsEvents {
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &PhysicsEvent2D> {
        self.events.iter()
    }

    #[inline]
    pub fn drain(&mut self) -> impl Iterator<Item = PhysicsEvent2D> + '_ {
        self.events.drain(..)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.events.clear();
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub(crate) fn push(&mut self, event: PhysicsEvent2D) {
        self.events.push(event);
    }
}

pub(crate) struct StepEventCollector<'a> {
    pub(crate) events: &'a Mutex<Vec<CollisionEvent>>,
}

impl EventHandler for StepEventCollector<'_> {
    fn handle_collision_event(
        &self,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        event: CollisionEvent,
        _contact_pair: Option<&ContactPair>,
    ) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    fn handle_contact_force_event(
        &self,
        _dt: Real,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        _contact_pair: &ContactPair,
        _total_force_magnitude: Real,
    ) {
    }
}

pub(crate) fn translate_collision_events(
    raw_events: Vec<CollisionEvent>,
    collider_entities: &FxHashMap<ColliderHandle, EntityId>,
    colliders: &ColliderSet,
    events: &mut PhysicsEvents,
) {
    for event in raw_events {
        let (started, a, b, flags) = match event {
            CollisionEvent::Started(a, b, flags) => (true, a, b, flags),
            CollisionEvent::Stopped(a, b, flags) => (false, a, b, flags),
        };
        let (Some(entity_a), Some(entity_b)) = (
            collider_entities.get(&a).copied(),
            collider_entities.get(&b).copied(),
        ) else {
            continue;
        };

        if flags.contains(CollisionEventFlags::SENSOR) {
            let a_sensor = colliders
                .get(a)
                .map(|collider| collider.is_sensor())
                .unwrap_or(false);
            let b_sensor = colliders
                .get(b)
                .map(|collider| collider.is_sensor())
                .unwrap_or(false);
            let (trigger, other) = if a_sensor || !b_sensor {
                (entity_a, entity_b)
            } else {
                (entity_b, entity_a)
            };
            events.push(if started {
                PhysicsEvent2D::TriggerEntered { trigger, other }
            } else {
                PhysicsEvent2D::TriggerExited { trigger, other }
            });
        } else {
            events.push(if started {
                PhysicsEvent2D::ContactStarted {
                    a: entity_a,
                    b: entity_b,
                }
            } else {
                PhysicsEvent2D::ContactStopped {
                    a: entity_a,
                    b: entity_b,
                }
            });
        }
    }
}
