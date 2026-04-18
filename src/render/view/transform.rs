use rustc_hash::FxHashMap;

use crate::ecs::{EntityId, PreparedQuery, World};
use crate::render::component::{Parent, Transform};

#[derive(Clone, Default)]
pub struct ResolvedSceneTransforms {
    world: FxHashMap<EntityId, Transform>,
}

impl ResolvedSceneTransforms {
    #[inline]
    pub fn get(&self, entity: EntityId) -> Option<Transform> {
        self.world.get(&entity).copied()
    }

    #[inline]
    pub fn contains(&self, entity: EntityId) -> bool {
        self.world.contains_key(&entity)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.world.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.world.is_empty()
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, Transform)> + '_ {
        self.world
            .iter()
            .map(|(entity, transform)| (*entity, *transform))
    }

    #[inline]
    fn clear(&mut self) {
        self.world.clear();
    }
}

#[derive(Default)]
pub(crate) struct SceneTransformResolver {
    query: PreparedQuery<(&'static Transform, Option<&'static Parent>)>,
    locals: FxHashMap<EntityId, LocalSceneTransform>,
    states: FxHashMap<EntityId, ResolveState>,
    entities: Vec<EntityId>,
    resolved: ResolvedSceneTransforms,
}

#[derive(Clone, Copy)]
struct LocalSceneTransform {
    local: Transform,
    parent: Option<EntityId>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResolveState {
    Resolving,
    Resolved,
}

impl SceneTransformResolver {
    pub(crate) fn resolve<'a>(&'a mut self, world: &World) -> &'a ResolvedSceneTransforms {
        self.locals.clear();
        self.states.clear();
        self.entities.clear();
        self.resolved.clear();

        self.query
            .for_each_with_entity(world, |entity, (transform, parent)| {
                self.locals.insert(
                    entity,
                    LocalSceneTransform {
                        local: *transform,
                        parent: parent.map(|parent| parent.entity()),
                    },
                );
                self.entities.push(entity);
            });

        for index in 0..self.entities.len() {
            let entity = self.entities[index];
            let _ = resolve_scene_transform_entity(
                entity,
                &self.locals,
                &mut self.states,
                &mut self.resolved.world,
            );
        }

        &self.resolved
    }

    pub(crate) fn resolve_owned(&mut self, world: &World) -> ResolvedSceneTransforms {
        let _ = self.resolve(world);
        std::mem::take(&mut self.resolved)
    }
}

fn resolve_scene_transform_entity(
    entity: EntityId,
    locals: &FxHashMap<EntityId, LocalSceneTransform>,
    states: &mut FxHashMap<EntityId, ResolveState>,
    resolved: &mut FxHashMap<EntityId, Transform>,
) -> Option<Transform> {
    if let Some(transform) = resolved.get(&entity).copied() {
        return Some(transform);
    }

    let local = locals.get(&entity).copied()?;
    match states.get(&entity).copied() {
        Some(ResolveState::Resolved) => return resolved.get(&entity).copied(),
        Some(ResolveState::Resolving) => return Some(local.local),
        None => {}
    }

    states.insert(entity, ResolveState::Resolving);
    let world_transform = local
        .parent
        .and_then(|parent| resolve_scene_transform_entity(parent, locals, states, resolved))
        .map(|parent_world| parent_world.mul_transform(local.local))
        .unwrap_or(local.local);
    states.insert(entity, ResolveState::Resolved);
    resolved.insert(entity, world_transform);
    Some(world_transform)
}
