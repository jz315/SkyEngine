use rustc_hash::FxHashMap;

use crate::ecs::{EntityId, World};
use crate::math::Transform;

use super::serialize::scene_value_to_transform;
use super::validation::{validate_prefab_document, validate_scene_document};
use super::{
    Children, Name, Parent, PrefabDocument, PrefabSpawnOptions, SceneDocument, SceneEntity,
    SceneEntityId, SceneError, SceneNode, SceneRoot, TRANSFORM_COMPONENT_TYPE,
};

/// Runtime entities spawned from a [`SceneDocument`].
#[derive(Debug)]
pub struct SceneInstance {
    pub roots: Vec<EntityId>,
    pub entities: Vec<EntityId>,
    pub scene_to_entity: FxHashMap<SceneEntityId, EntityId>,
}

impl SceneInstance {
    #[inline]
    pub fn entity(&self, id: &SceneEntityId) -> Option<EntityId> {
        self.scene_to_entity.get(id).copied()
    }

    #[inline]
    pub fn entity_by_str(&self, id: &str) -> Option<EntityId> {
        self.scene_to_entity.get(&SceneEntityId::from(id)).copied()
    }
}

/// Runtime entities spawned from a [`PrefabDocument`].
#[derive(Debug)]
pub struct PrefabInstance {
    pub root: EntityId,
    pub entities: Vec<EntityId>,
    pub prefab_to_entity: FxHashMap<SceneEntityId, EntityId>,
}

impl PrefabInstance {
    #[inline]
    pub fn entity(&self, id: &SceneEntityId) -> Option<EntityId> {
        self.prefab_to_entity.get(id).copied()
    }

    #[inline]
    pub fn entity_by_str(&self, id: &str) -> Option<EntityId> {
        self.prefab_to_entity.get(&SceneEntityId::from(id)).copied()
    }
}

/// Spawns a scene document into the world.
///
/// This low-level spawner supports engine-owned scene metadata and
/// `sky.Transform`. Use [`SceneRuntime`](super::SceneRuntime) for serde
/// custom components.
pub fn spawn_scene(world: &mut World, scene: &SceneDocument) -> Result<SceneInstance, SceneError> {
    validate_scene_document(scene)?;
    validate_builtin_components(&scene.roots)?;

    let mut instance = SceneInstance {
        roots: Vec::with_capacity(scene.roots.len()),
        entities: Vec::new(),
        scene_to_entity: FxHashMap::default(),
    };

    for root in &scene.roots {
        let entity = spawn_node(world, root, None, true, None, &mut instance)?;
        instance.roots.push(entity);
    }

    Ok(instance)
}

/// Despawns all entities owned by a scene instance.
pub fn despawn_scene_instance(world: &mut World, instance: SceneInstance) {
    for entity in instance.entities.into_iter().rev() {
        let _ = world.despawn(entity);
    }
}

/// Spawns a prefab document into the world.
pub fn spawn_prefab(
    world: &mut World,
    prefab: &PrefabDocument,
    options: PrefabSpawnOptions,
) -> Result<PrefabInstance, SceneError> {
    validate_prefab_document(prefab)?;
    validate_builtin_components(std::slice::from_ref(&prefab.root))?;

    let mut scene_instance = SceneInstance {
        roots: Vec::with_capacity(1),
        entities: Vec::new(),
        scene_to_entity: FxHashMap::default(),
    };
    let root = spawn_node(
        world,
        &prefab.root,
        None,
        true,
        options.root_transform,
        &mut scene_instance,
    )?;
    scene_instance.roots.push(root);

    Ok(PrefabInstance {
        root,
        entities: scene_instance.entities,
        prefab_to_entity: scene_instance.scene_to_entity,
    })
}

/// Despawns all entities owned by a prefab instance.
pub fn despawn_prefab_instance(world: &mut World, instance: PrefabInstance) {
    for entity in instance.entities.into_iter().rev() {
        let _ = world.despawn(entity);
    }
}

fn spawn_node(
    world: &mut World,
    node: &SceneNode,
    parent: Option<EntityId>,
    root: bool,
    transform_override: Option<Transform>,
    instance: &mut SceneInstance,
) -> Result<EntityId, SceneError> {
    let transform = match transform_override {
        Some(transform) => transform,
        None => node_transform(node)?.unwrap_or_default(),
    };
    let entity = world.spawn((SceneEntity::new(node.id.clone()), transform));

    if let Some(name) = &node.name {
        world.insert(entity, Name::new(name.clone()));
    }
    if let Some(parent) = parent {
        world.insert(entity, Parent::new(parent));
    }
    if root {
        world.insert(entity, SceneRoot);
    }

    instance.entities.push(entity);
    instance.scene_to_entity.insert(node.id.clone(), entity);

    let mut children = Vec::with_capacity(node.children.len());
    for child in &node.children {
        let child_entity = spawn_node(world, child, Some(entity), false, None, instance)?;
        children.push(child_entity);
    }
    if !children.is_empty() {
        world.insert(entity, Children::new(children));
    }

    Ok(entity)
}

fn node_transform(node: &SceneNode) -> Result<Option<Transform>, SceneError> {
    node.components
        .get(TRANSFORM_COMPONENT_TYPE)
        .map(scene_value_to_transform)
        .transpose()
}

fn validate_builtin_components(roots: &[SceneNode]) -> Result<(), SceneError> {
    for root in roots {
        validate_builtin_node_components(root)?;
    }
    Ok(())
}

fn validate_builtin_node_components(node: &SceneNode) -> Result<(), SceneError> {
    for (type_name, value) in node.components.iter() {
        if type_name == TRANSFORM_COMPONENT_TYPE {
            scene_value_to_transform(value)?;
        } else {
            return Err(SceneError::UnregisteredComponentType(type_name.to_string()));
        }
    }
    for child in &node.children {
        validate_builtin_node_components(child)?;
    }
    Ok(())
}
