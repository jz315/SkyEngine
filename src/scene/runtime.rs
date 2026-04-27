use std::any::{type_name, TypeId};
use std::marker::PhantomData;
use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::ecs::{Bundle, EntityId, World};
use crate::math::Transform;

use super::serialize::scene_value_to_transform;
use super::validation::validate_scene_document;
use super::{
    Children, Name, Parent, SceneDocument, SceneEntity, SceneEntityId, SceneError, SceneInstance,
    SceneNode, SceneRoot, SceneValue, TRANSFORM_COMPONENT_TYPE,
};

/// AI-first scene/save runtime.
///
/// `SceneRuntime` keeps save/load policy outside ECS. Components stay ordinary
/// Rust ECS components; saveable custom components only need serde.
pub struct SceneRuntime {
    adapters: FxHashMap<String, Box<dyn SerdeComponentAdapter>>,
    type_names_by_id: FxHashMap<TypeId, String>,
}

impl Default for SceneRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneRuntime {
    pub fn new() -> Self {
        Self {
            adapters: FxHashMap::default(),
            type_names_by_id: FxHashMap::default(),
        }
    }

    pub fn component<T>(&mut self) -> Result<&mut Self, SceneError>
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        self.component_as::<T>(type_name::<T>())
    }

    pub fn component_as<T>(&mut self, name: impl Into<String>) -> Result<&mut Self, SceneError>
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        let type_name = name.into();
        if type_name == TRANSFORM_COMPONENT_TYPE {
            return Err(SceneError::DuplicateComponentType(type_name));
        }
        if self.adapters.contains_key(&type_name) {
            return Err(SceneError::DuplicateComponentType(type_name));
        }

        let rust_type_id = TypeId::of::<T>();
        if let Some(existing) = self.type_names_by_id.get(&rust_type_id) {
            return Err(SceneError::DuplicateComponentType(existing.clone()));
        }

        self.type_names_by_id
            .insert(rust_type_id, type_name.clone());
        self.adapters.insert(
            type_name.clone(),
            Box::new(SerdeComponentCodec::<T> {
                type_name,
                marker: PhantomData,
            }),
        );
        Ok(self)
    }

    pub fn spawn_root<B: Bundle>(
        &self,
        world: &mut World,
        id: impl Into<SceneEntityId>,
        bundle: B,
    ) -> Result<EntityId, SceneError> {
        let id = id.into();
        if id.is_empty() {
            return Err(SceneError::EmptySceneEntityId);
        }

        let entity = world.spawn(bundle);
        world.insert(entity, SceneEntity::new(id));
        world.insert(entity, SceneRoot);
        Ok(entity)
    }

    pub fn spawn_child<B: Bundle>(
        &self,
        world: &mut World,
        parent: EntityId,
        id: impl Into<SceneEntityId>,
        bundle: B,
    ) -> Result<EntityId, SceneError> {
        let id = id.into();
        if id.is_empty() {
            return Err(SceneError::EmptySceneEntityId);
        }
        if !world.contains(parent) {
            return Err(SceneError::MissingRuntimeEntity(parent));
        }

        let entity = world.spawn(bundle);
        world.insert(entity, SceneEntity::new(id));
        world.insert(entity, Parent::new(parent));

        if let Some(children) = world.get_mut::<Children>(parent) {
            children.entities.push(entity);
        } else {
            world.insert(parent, Children::new(vec![entity]));
        }

        Ok(entity)
    }

    pub fn capture_scene(
        &self,
        world: &World,
        roots: impl IntoIterator<Item = EntityId>,
    ) -> Result<SceneDocument, SceneError> {
        let mut seen = FxHashSet::default();
        let mut scene = SceneDocument::new();

        for root in roots {
            scene.roots.push(self.capture_node(world, root, &mut seen)?);
        }

        Ok(scene)
    }

    pub fn spawn_scene(
        &self,
        world: &mut World,
        scene: &SceneDocument,
    ) -> Result<SceneInstance, SceneError> {
        validate_scene_document(scene)?;
        self.validate_components(&scene.roots)?;

        let mut instance = SceneInstance {
            roots: Vec::with_capacity(scene.roots.len()),
            entities: Vec::new(),
            scene_to_entity: FxHashMap::default(),
        };

        for root in &scene.roots {
            match self.spawn_node(world, root, None, true, None, &mut instance) {
                Ok(entity) => instance.roots.push(entity),
                Err(error) => {
                    cleanup_spawned(world, &instance.entities);
                    return Err(error);
                }
            }
        }

        Ok(instance)
    }

    pub fn save_scene_file(
        &self,
        world: &World,
        roots: impl IntoIterator<Item = EntityId>,
        path: impl AsRef<Path>,
    ) -> Result<(), SceneError> {
        self.capture_scene(world, roots)?.write_json_file(path)
    }

    pub fn load_scene_file(
        &self,
        world: &mut World,
        path: impl AsRef<Path>,
    ) -> Result<SceneInstance, SceneError> {
        let scene = SceneDocument::from_json_file(path)?;
        self.spawn_scene(world, &scene)
    }

    fn capture_node(
        &self,
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

        let mut type_names: Vec<_> = self.adapters.keys().map(String::as_str).collect();
        type_names.sort_unstable();
        for type_name in type_names {
            let adapter = self.adapters.get(type_name).expect("adapter key exists");
            if let Some((component_type, value)) = adapter.capture(world, entity)? {
                node.components.insert(component_type, value);
            }
        }

        if let Some(children) = world.get::<Children>(entity) {
            for child in children.as_slice() {
                node.children.push(self.capture_node(world, *child, seen)?);
            }
        }

        Ok(node)
    }

    fn validate_components(&self, roots: &[SceneNode]) -> Result<(), SceneError> {
        for root in roots {
            self.validate_node_components(root)?;
        }
        Ok(())
    }

    fn validate_node_components(&self, node: &SceneNode) -> Result<(), SceneError> {
        for (type_name, value) in node.components.iter() {
            if type_name == TRANSFORM_COMPONENT_TYPE {
                scene_value_to_transform(value)?;
                continue;
            }

            let Some(adapter) = self.adapters.get(type_name) else {
                return Err(SceneError::UnregisteredComponentType(type_name.to_string()));
            };
            adapter.validate(value)?;
        }

        for child in &node.children {
            self.validate_node_components(child)?;
        }
        Ok(())
    }

    fn spawn_node(
        &self,
        world: &mut World,
        node: &SceneNode,
        parent: Option<EntityId>,
        root: bool,
        transform_override: Option<Transform>,
        instance: &mut SceneInstance,
    ) -> Result<EntityId, SceneError> {
        let transform = match transform_override {
            Some(transform) => transform,
            None => self.node_transform(node)?.unwrap_or_default(),
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

        self.insert_node_components(world, entity, node)?;

        let mut children = Vec::with_capacity(node.children.len());
        for child in &node.children {
            let child_entity =
                self.spawn_node(world, child, Some(entity), false, None, instance)?;
            children.push(child_entity);
        }
        if !children.is_empty() {
            world.insert(entity, Children::new(children));
        }

        Ok(entity)
    }

    fn node_transform(&self, node: &SceneNode) -> Result<Option<Transform>, SceneError> {
        node.components
            .get(TRANSFORM_COMPONENT_TYPE)
            .map(scene_value_to_transform)
            .transpose()
    }

    fn insert_node_components(
        &self,
        world: &mut World,
        entity: EntityId,
        node: &SceneNode,
    ) -> Result<(), SceneError> {
        for (type_name, value) in node.components.iter() {
            if type_name == TRANSFORM_COMPONENT_TYPE {
                continue;
            }

            let Some(adapter) = self.adapters.get(type_name) else {
                return Err(SceneError::UnregisteredComponentType(type_name.to_string()));
            };
            adapter.insert(world, entity, value)?;
        }
        Ok(())
    }
}

fn cleanup_spawned(world: &mut World, entities: &[EntityId]) {
    for entity in entities.iter().rev().copied() {
        let _ = world.despawn(entity);
    }
}

trait SerdeComponentAdapter {
    fn validate(&self, value: &SceneValue) -> Result<(), SceneError>;
    fn insert(
        &self,
        world: &mut World,
        entity: EntityId,
        value: &SceneValue,
    ) -> Result<(), SceneError>;
    fn capture(
        &self,
        world: &World,
        entity: EntityId,
    ) -> Result<Option<(String, SceneValue)>, SceneError>;
}

struct SerdeComponentCodec<T> {
    type_name: String,
    marker: PhantomData<fn() -> T>,
}

impl<T> SerdeComponentAdapter for SerdeComponentCodec<T>
where
    T: Serialize + DeserializeOwned + 'static,
{
    fn validate(&self, value: &SceneValue) -> Result<(), SceneError> {
        self.decode(value).map(drop)
    }

    fn insert(
        &self,
        world: &mut World,
        entity: EntityId,
        value: &SceneValue,
    ) -> Result<(), SceneError> {
        let component = self.decode(value)?;
        if world.insert(entity, component) {
            Ok(())
        } else {
            Err(SceneError::MissingRuntimeEntity(entity))
        }
    }

    fn capture(
        &self,
        world: &World,
        entity: EntityId,
    ) -> Result<Option<(String, SceneValue)>, SceneError> {
        let Some(component) = world.get::<T>(entity) else {
            return Ok(None);
        };
        let value =
            serde_json::to_value(component).map_err(|error| SceneError::ComponentSerde {
                type_name: self.type_name.clone(),
                error: error.to_string(),
            })?;
        Ok(Some((self.type_name.clone(), SceneValue::from(value))))
    }
}

impl<T> SerdeComponentCodec<T>
where
    T: DeserializeOwned,
{
    fn decode(&self, value: &SceneValue) -> Result<T, SceneError> {
        serde_json::from_value::<T>(value.as_json().clone()).map_err(|error| {
            SceneError::ComponentSerde {
                type_name: self.type_name.clone(),
                error: error.to_string(),
            }
        })
    }
}
