use std::path::Path;

use rustc_hash::FxHashSet;

use crate::ecs::{EntityId, World};
use crate::math::Transform;

use super::{
    Children, Name, PrefabDocument, PrefabInstance, SceneDocument, SceneEntity, SceneError,
    SceneInstance, SceneNode,
};

/// Captures runtime scene entities back into a serializable scene document.
pub fn capture_scene(
    world: &World,
    roots: impl IntoIterator<Item = EntityId>,
) -> Result<SceneDocument, SceneError> {
    let mut seen = FxHashSet::default();
    let mut scene = SceneDocument::new();

    for root in roots {
        scene.roots.push(capture_node(world, root, &mut seen)?);
    }

    Ok(scene)
}

/// Captures one runtime root as a prefab document.
pub fn capture_prefab(world: &World, root: EntityId) -> Result<PrefabDocument, SceneError> {
    let mut seen = FxHashSet::default();
    Ok(PrefabDocument::new(capture_node(world, root, &mut seen)?))
}

fn capture_node(
    world: &World,
    entity: EntityId,
    seen: &mut FxHashSet<EntityId>,
) -> Result<SceneNode, SceneError> {
    if !world.contains(entity) {
        return Err(SceneError::MissingRuntimeEntity(entity));
    }
    if !seen.insert(entity) {
        return Err(SceneError::DuplicateRuntimeEntity(entity));
    }

    let scene_entity = world
        .get::<SceneEntity>(entity)
        .ok_or(SceneError::MissingSceneEntity(entity))?;
    let mut node = SceneNode::new(scene_entity.id.clone());

    if let Some(name) = world.get::<Name>(entity) {
        node.name = Some(name.as_str().to_string());
    }
    if let Some(transform) = world.get::<Transform>(entity) {
        node.components.insert_transform(*transform);
    }
    if let Some(children) = world.get::<Children>(entity) {
        for child in children.as_slice() {
            node.children.push(capture_node(world, *child, seen)?);
        }
    }

    Ok(node)
}

impl SceneInstance {
    pub fn capture(&self, world: &World) -> Result<SceneDocument, SceneError> {
        capture_scene(world, self.roots.iter().copied())
    }

    pub fn save_json_file(&self, world: &World, path: impl AsRef<Path>) -> Result<(), SceneError> {
        self.capture(world)?.write_json_file(path)
    }
}

impl PrefabInstance {
    pub fn capture(&self, world: &World) -> Result<PrefabDocument, SceneError> {
        capture_prefab(world, self.root)
    }

    pub fn save_json_file(&self, world: &World, path: impl AsRef<Path>) -> Result<(), SceneError> {
        self.capture(world)?.write_json_file(path)
    }
}
