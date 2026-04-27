use rustc_hash::FxHashSet;

use super::serialize::scene_value_to_transform;
use super::{
    PrefabDocument, SceneDocument, SceneEntityId, SceneError, SceneNode, TRANSFORM_COMPONENT_TYPE,
};

pub(crate) fn validate_scene_document(scene: &SceneDocument) -> Result<(), SceneError> {
    validate_roots(&scene.roots)
}

pub(crate) fn validate_prefab_document(prefab: &PrefabDocument) -> Result<(), SceneError> {
    if prefab.root.id.is_empty() {
        return Err(SceneError::MissingPrefabRoot);
    }
    validate_roots(std::slice::from_ref(&prefab.root))
}

pub(crate) fn validate_roots(roots: &[SceneNode]) -> Result<(), SceneError> {
    let mut seen = FxHashSet::default();
    for root in roots {
        validate_node(root, &mut seen)?;
    }
    Ok(())
}

fn validate_node(node: &SceneNode, seen: &mut FxHashSet<SceneEntityId>) -> Result<(), SceneError> {
    if node.id.is_empty() {
        return Err(SceneError::EmptySceneEntityId);
    }
    if !seen.insert(node.id.clone()) {
        return Err(SceneError::DuplicateSceneEntityId(node.id.clone()));
    }
    for type_name in node.components.duplicate_type_names() {
        if type_name == TRANSFORM_COMPONENT_TYPE {
            return Err(SceneError::DuplicateTransform(node.id.clone()));
        }
        return Err(SceneError::DuplicateSceneComponent {
            entity: node.id.clone(),
            type_name: type_name.clone(),
        });
    }

    for (type_name, value) in node.components.iter() {
        if type_name == TRANSFORM_COMPONENT_TYPE {
            scene_value_to_transform(value)?;
        }
    }

    let mut component_types = FxHashSet::default();
    for (type_name, _) in node.components.iter() {
        if !component_types.insert(type_name) {
            return Err(SceneError::DuplicateSceneComponent {
                entity: node.id.clone(),
                type_name: type_name.to_string(),
            });
        }
    }
    for child in &node.children {
        validate_node(child, seen)?;
    }
    Ok(())
}
