use rustc_hash::{FxHashMap, FxHashSet};

use crate::ecs::{EntityId, PreparedQuery, World};
use crate::render::{Parent, Transform};

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
    resolving: FxHashSet<EntityId>,
    entities: Vec<EntityId>,
    resolve_chain: Vec<EntityId>,
}

#[derive(Clone, Copy)]
struct LocalSceneTransform {
    local: Transform,
    parent: Option<EntityId>,
}

impl SceneTransformResolver {
    pub(crate) fn resolve_into(&mut self, world: &World, resolved: &mut ResolvedSceneTransforms) {
        self.locals.clear();
        self.resolving.clear();
        self.entities.clear();
        self.resolve_chain.clear();
        resolved.clear();

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
                &mut self.resolving,
                &mut resolved.world,
                &mut self.resolve_chain,
            );
        }
    }
}

fn resolve_scene_transform_entity(
    entity: EntityId,
    locals: &FxHashMap<EntityId, LocalSceneTransform>,
    resolving: &mut FxHashSet<EntityId>,
    resolved: &mut FxHashMap<EntityId, Transform>,
    chain: &mut Vec<EntityId>,
) -> Option<Transform> {
    chain.clear();
    let mut cursor = entity;
    let mut parent_world = None;

    loop {
        if let Some(transform) = resolved.get(&cursor).copied() {
            parent_world = Some(transform);
            break;
        }
        let Some(local) = locals.get(&cursor).copied() else {
            break;
        };
        if !resolving.insert(cursor) {
            // Break malformed parent cycles at the edge that re-enters the
            // active chain. The unwind below applies every local transform at
            // most once and avoids recursive stack growth for deep scenes.
            break;
        }

        chain.push(cursor);
        let Some(parent) = local.parent else {
            break;
        };
        cursor = parent;
    }

    while let Some(current) = chain.pop() {
        let local = locals
            .get(&current)
            .expect("resolve chain only contains collected transforms")
            .local;
        let world_transform = parent_world
            .map(|parent| parent.mul_transform(local))
            .unwrap_or(local);
        resolved.insert(current, world_transform);
        resolving.remove(&current);
        parent_world = Some(world_transform);
    }

    resolved.get(&entity).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_into_reuses_output_storage() {
        let mut world = World::new();
        for index in 0..128 {
            world.spawn((Transform::from_xyz(index as f32, 0.0, 0.0),));
        }

        let mut resolver = SceneTransformResolver::default();
        let mut resolved = ResolvedSceneTransforms::default();
        resolver.resolve_into(&world, &mut resolved);
        let first_capacity = resolved.world.capacity();
        assert_eq!(resolved.len(), 128);

        resolver.resolve_into(&world, &mut resolved);

        assert_eq!(resolved.len(), 128);
        assert_eq!(resolved.world.capacity(), first_capacity);
    }

    #[test]
    fn parent_cycles_do_not_apply_local_transforms_twice() {
        let mut world = World::new();
        let first = world.spawn((Transform::from_xyz(1.0, 0.0, 0.0),));
        let second = world.spawn((Transform::from_xyz(2.0, 0.0, 0.0), Parent::new(first)));
        world.insert(first, Parent::new(second));

        let mut resolver = SceneTransformResolver::default();
        let mut resolved = ResolvedSceneTransforms::default();
        resolver.resolve_into(&world, &mut resolved);

        let first_x = resolved.get(first).expect("first transform").x();
        let second_x = resolved.get(second).expect("second transform").x();
        assert_eq!(first_x.max(second_x), 3.0);
        assert!(first_x.min(second_x) == 1.0 || first_x.min(second_x) == 2.0);
    }

    #[test]
    fn deep_parent_chains_resolve_iteratively() {
        const DEPTH: usize = 4096;
        let mut world = World::new();
        let entities = (0..DEPTH)
            .map(|_| world.spawn((Transform::default(),)))
            .collect::<Vec<_>>();
        let mut locals = FxHashMap::default();
        for (index, &entity) in entities.iter().enumerate() {
            locals.insert(
                entity,
                LocalSceneTransform {
                    local: Transform::from_xyz(1.0, 0.0, 0.0),
                    parent: index.checked_sub(1).map(|parent| entities[parent]),
                },
            );
        }
        let mut resolving = FxHashSet::default();
        let mut resolved = FxHashMap::default();
        let mut chain = Vec::new();

        let leaf = resolve_scene_transform_entity(
            *entities.last().expect("deep chain has a leaf"),
            &locals,
            &mut resolving,
            &mut resolved,
            &mut chain,
        )
        .expect("deep chain should resolve");

        assert_eq!(leaf.x(), DEPTH as f32);
        assert_eq!(resolved.len(), DEPTH);
        assert!(chain.is_empty());
    }
}
