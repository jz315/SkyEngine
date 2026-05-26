use rustc_hash::FxHashSet;

use super::serialize::persist_value_to_transform;
use super::{PersistDocumentData, PersistError, PersistId, PersistNode, TRANSFORM_COMPONENT_TYPE};

pub(crate) fn validate_persist_document(data: &PersistDocumentData) -> Result<(), PersistError> {
    validate_roots(&data.roots)
}

pub(crate) fn validate_roots(roots: &[PersistNode]) -> Result<(), PersistError> {
    let mut seen = FxHashSet::default();
    for root in roots {
        validate_node(root, &mut seen)?;
    }
    Ok(())
}

fn validate_node(node: &PersistNode, seen: &mut FxHashSet<PersistId>) -> Result<(), PersistError> {
    if node.id.is_empty() {
        return Err(PersistError::EmptyPersistId);
    }
    if !seen.insert(node.id.clone()) {
        return Err(PersistError::DuplicatePersistId(node.id.clone()));
    }
    for type_name in node.components.duplicate_type_names() {
        if type_name == TRANSFORM_COMPONENT_TYPE {
            return Err(PersistError::DuplicateTransform(node.id.clone()));
        }
        return Err(PersistError::DuplicatePersistComponent {
            entity: node.id.clone(),
            type_name: type_name.clone(),
        });
    }

    for (type_name, value) in node.components.iter() {
        if type_name == TRANSFORM_COMPONENT_TYPE {
            persist_value_to_transform(value)?;
        }
    }

    let mut component_types = FxHashSet::default();
    for (type_name, _) in node.components.iter() {
        if !component_types.insert(type_name) {
            return Err(PersistError::DuplicatePersistComponent {
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
