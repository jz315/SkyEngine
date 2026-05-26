use std::any::TypeId;
use std::marker::PhantomData;
use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ecs::{EntityId, World};
use crate::math::Transform;

use super::serialize::persist_value_to_transform;
use super::validation::validate_persist_document;
use super::{
    Children, Name, Parent, PersistDocumentData, PersistEntity, PersistError, PersistId,
    PersistNode, PersistRoot, PersistValue, TRANSFORM_COMPONENT_TYPE,
};

/// A type that can be persisted by SkyEngine's save/prefab layer.
///
/// Most users should not implement this by hand. Use `#[persist(component)]`
/// instead; the macro supplies serde support, metadata, and auto-registration.
pub trait Persist: Serialize + DeserializeOwned + 'static {
    const SHORT_NAME: &'static str;
    const NAME: Option<&'static str> = None;
}

/// Auto-registration entry produced by `#[persist(component)]`.
pub struct PersistRegistration {
    pub short_name: &'static str,
    pub explicit_name: Option<&'static str>,
    register: fn(&mut Persistence) -> Result<(), PersistError>,
}

impl PersistRegistration {
    pub const fn component<T: Persist>() -> Self {
        Self {
            short_name: T::SHORT_NAME,
            explicit_name: T::NAME,
            register: register_persist_component::<T>,
        }
    }

    fn register(&self, persistence: &mut Persistence) -> Result<(), PersistError> {
        (self.register)(persistence)
    }
}

inventory::collect!(PersistRegistration);

fn register_persist_component<T: Persist>(
    persistence: &mut Persistence,
) -> Result<(), PersistError> {
    persistence.register_component::<T>().map(drop)
}

/// Runtime save/prefab API.
///
/// `Persistence` owns the save policy for serde-backed ECS components. It does
/// not replace ECS: users still create normal entities with `World::spawn`.
pub struct Persistence {
    namespace: String,
    adapters: FxHashMap<String, Box<dyn PersistComponentAdapter>>,
    type_names_by_id: FxHashMap<TypeId, String>,
}

impl Persistence {
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            adapters: FxHashMap::default(),
            type_names_by_id: FxHashMap::default(),
        }
    }

    pub fn auto(namespace: impl Into<String>) -> Result<Self, PersistError> {
        let mut persistence = Self::new(namespace);
        for registration in inventory::iter::<PersistRegistration> {
            registration.register(&mut persistence)?;
        }
        Ok(persistence)
    }

    fn register_component<T: Persist>(&mut self) -> Result<&mut Self, PersistError> {
        let type_name = self.persist_type_name::<T>();
        self.register_component_as::<T>(type_name)
    }

    fn register_component_as<T>(
        &mut self,
        name: impl Into<String>,
    ) -> Result<&mut Self, PersistError>
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        let type_name = name.into();
        if type_name == TRANSFORM_COMPONENT_TYPE {
            return Err(PersistError::DuplicateComponentType(type_name));
        }
        if self.adapters.contains_key(&type_name) {
            return Err(PersistError::DuplicateComponentType(type_name));
        }

        let rust_type_id = TypeId::of::<T>();
        if let Some(existing) = self.type_names_by_id.get(&rust_type_id) {
            return Err(PersistError::DuplicateComponentType(existing.clone()));
        }

        self.type_names_by_id
            .insert(rust_type_id, type_name.clone());
        self.adapters.insert(
            type_name.clone(),
            Box::new(PersistComponentCodec::<T> {
                type_name,
                marker: PhantomData,
            }),
        );
        Ok(self)
    }

    pub fn capture_world(&self, world: &mut World) -> Result<PersistDocument, PersistError> {
        let entities: Vec<_> = world.entities().collect();
        let persistable: FxHashSet<_> = entities
            .into_iter()
            .filter(|entity| self.entity_has_persisted_component(world, *entity))
            .collect();

        for entity in persistable.iter().copied() {
            self.ensure_persist_id(world, entity)?;
        }

        let mut roots = Vec::new();
        for entity in persistable.iter().copied() {
            let parent_is_persisted = world
                .get::<Parent>(entity)
                .is_some_and(|parent| persistable.contains(&parent.entity()));
            if !parent_is_persisted {
                roots.push(entity);
            }
        }
        roots.sort_by_key(|entity| (entity.index(), entity.generation()));

        let mut seen = FxHashSet::default();
        let mut data = PersistDocumentData::new();
        for root in roots {
            data.roots.push(self.capture_node(world, root, &mut seen)?);
        }

        Ok(PersistDocument::new(data))
    }

    pub fn capture_prefab(
        &self,
        world: &mut World,
        root: EntityId,
    ) -> Result<PersistDocument, PersistError> {
        let mut seen = FxHashSet::default();
        let mut data = PersistDocumentData::new();
        data.roots.push(self.capture_node(world, root, &mut seen)?);
        Ok(PersistDocument::new(data))
    }

    pub fn spawn_world(
        &self,
        world: &mut World,
        document: &PersistDocument,
    ) -> Result<PersistWorldInstance, PersistError> {
        let data = document.as_data();
        validate_persist_document(data)?;
        self.validate_components(&data.roots)?;

        let mut instance = PersistWorldInstance {
            roots: Vec::with_capacity(data.roots.len()),
            entities: Vec::new(),
            id_to_entity: FxHashMap::default(),
        };

        for root in &data.roots {
            match self.spawn_node(world, root, None, true, &mut instance) {
                Ok(entity) => instance.roots.push(entity),
                Err(error) => {
                    cleanup_spawned(world, &instance.entities);
                    return Err(error);
                }
            }
        }

        Ok(instance)
    }

    pub fn save_world(
        &self,
        world: &mut World,
        path: impl AsRef<Path>,
    ) -> Result<(), PersistError> {
        self.capture_world(world)?.write_json_file(path)
    }

    pub fn load_world(
        &self,
        world: &mut World,
        path: impl AsRef<Path>,
    ) -> Result<PersistWorldInstance, PersistError> {
        let document = PersistDocument::from_json_file(path)?;
        world.clear();
        self.spawn_world(world, &document)
    }

    pub fn save_prefab(
        &self,
        world: &mut World,
        root: EntityId,
        path: impl AsRef<Path>,
    ) -> Result<(), PersistError> {
        self.capture_prefab(world, root)?.write_json_file(path)
    }

    pub fn load_prefab(
        &self,
        world: &mut World,
        path: impl AsRef<Path>,
    ) -> Result<PersistPrefabInstance, PersistError> {
        let document = PersistDocument::from_json_file(path)?;
        let instance = self.spawn_world(world, &document)?;
        let [root] = instance.roots.as_slice() else {
            return Err(PersistError::MissingPrefabRoot);
        };
        Ok(PersistPrefabInstance {
            root: *root,
            entities: instance.entities,
            id_to_entity: instance.id_to_entity,
        })
    }

    fn persist_type_name<T: Persist>(&self) -> String {
        if let Some(name) = T::NAME {
            return name.to_string();
        }
        if self.namespace.is_empty() {
            T::SHORT_NAME.to_string()
        } else {
            format!("{}.{}", self.namespace, T::SHORT_NAME)
        }
    }

    fn entity_has_persisted_component(&self, world: &World, entity: EntityId) -> bool {
        self.adapters
            .values()
            .any(|adapter| adapter.has(world, entity))
    }

    fn ensure_persist_id(
        &self,
        world: &mut World,
        entity: EntityId,
    ) -> Result<PersistId, PersistError> {
        if !world.contains(entity) {
            return Err(PersistError::MissingRuntimeEntity(entity));
        }
        if let Some(persist_entity) = world.get::<PersistEntity>(entity) {
            return Ok(persist_entity.id.clone());
        }

        let id = PersistId::from(uuid::Uuid::new_v4().to_string());
        world.insert(entity, PersistEntity::new(id.clone()));
        Ok(id)
    }

    fn capture_node(
        &self,
        world: &mut World,
        entity: EntityId,
        seen: &mut FxHashSet<EntityId>,
    ) -> Result<PersistNode, PersistError> {
        if !world.contains(entity) {
            return Err(PersistError::MissingRuntimeEntity(entity));
        }
        if !seen.insert(entity) {
            return Err(PersistError::DuplicateRuntimeEntity(entity));
        }

        let id = self.ensure_persist_id(world, entity)?;
        let mut node = PersistNode::new(id);

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

        let children = world
            .get::<Children>(entity)
            .map(|children| children.as_slice().to_vec())
            .unwrap_or_default();
        for child in children {
            node.children.push(self.capture_node(world, child, seen)?);
        }

        Ok(node)
    }

    fn validate_components(&self, roots: &[PersistNode]) -> Result<(), PersistError> {
        for root in roots {
            self.validate_node_components(root)?;
        }
        Ok(())
    }

    fn validate_node_components(&self, node: &PersistNode) -> Result<(), PersistError> {
        for (type_name, value) in node.components.iter() {
            if type_name == TRANSFORM_COMPONENT_TYPE {
                persist_value_to_transform(value)?;
                continue;
            }

            let Some(adapter) = self.adapters.get(type_name) else {
                return Err(PersistError::UnregisteredComponentType(
                    type_name.to_string(),
                ));
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
        node: &PersistNode,
        parent: Option<EntityId>,
        root: bool,
        instance: &mut PersistWorldInstance,
    ) -> Result<EntityId, PersistError> {
        let transform = self.node_transform(node)?.unwrap_or_default();
        let entity = world.spawn((PersistEntity::new(node.id.clone()), transform));

        if let Some(name) = &node.name {
            world.insert(entity, Name::new(name.clone()));
        }
        if let Some(parent) = parent {
            world.insert(entity, Parent::new(parent));
        }
        if root {
            world.insert(entity, PersistRoot);
        }

        instance.entities.push(entity);
        instance.id_to_entity.insert(node.id.clone(), entity);

        self.insert_node_components(world, entity, node)?;

        let mut children = Vec::with_capacity(node.children.len());
        for child in &node.children {
            let child_entity = self.spawn_node(world, child, Some(entity), false, instance)?;
            children.push(child_entity);
        }
        if !children.is_empty() {
            world.insert(entity, Children::new(children));
        }

        Ok(entity)
    }

    fn node_transform(&self, node: &PersistNode) -> Result<Option<Transform>, PersistError> {
        node.components
            .get(TRANSFORM_COMPONENT_TYPE)
            .map(persist_value_to_transform)
            .transpose()
    }

    fn insert_node_components(
        &self,
        world: &mut World,
        entity: EntityId,
        node: &PersistNode,
    ) -> Result<(), PersistError> {
        for (type_name, value) in node.components.iter() {
            if type_name == TRANSFORM_COMPONENT_TYPE {
                continue;
            }

            let Some(adapter) = self.adapters.get(type_name) else {
                return Err(PersistError::UnregisteredComponentType(
                    type_name.to_string(),
                ));
            };
            adapter.insert(world, entity, value)?;
        }
        Ok(())
    }
}

/// In-memory persisted document used by save files and prefab files.
#[derive(Clone, Debug, PartialEq)]
pub struct PersistDocument {
    data: PersistDocumentData,
}

impl PersistDocument {
    pub(crate) fn new(data: PersistDocumentData) -> Self {
        Self { data }
    }

    pub fn from_json_str(input: &str) -> Result<Self, PersistError> {
        let data = PersistDocumentData::from_json_str(input)?;
        Ok(Self { data })
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, PersistError> {
        Self::from_json_str(&std::fs::read_to_string(path)?)
    }

    pub fn to_json_string(&self) -> Result<String, PersistError> {
        self.data.to_json_string()
    }

    pub fn to_json_string_pretty(&self) -> Result<String, PersistError> {
        self.data.to_json_string_pretty()
    }

    pub fn write_json_file(&self, path: impl AsRef<Path>) -> Result<(), PersistError> {
        self.data.write_json_file(path)
    }

    pub(crate) fn as_data(&self) -> &PersistDocumentData {
        &self.data
    }
}

impl Serialize for PersistDocument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.data.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PersistDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = PersistDocumentData::deserialize(deserializer)?;
        validate_persist_document(&data).map_err(serde::de::Error::custom)?;
        Ok(Self { data })
    }
}

#[derive(Debug)]
pub struct PersistWorldInstance {
    pub roots: Vec<EntityId>,
    pub entities: Vec<EntityId>,
    pub id_to_entity: FxHashMap<PersistId, EntityId>,
}

impl PersistWorldInstance {
    pub fn entity(&self, id: &PersistId) -> Option<EntityId> {
        self.id_to_entity.get(id).copied()
    }
}

#[derive(Debug)]
pub struct PersistPrefabInstance {
    root: EntityId,
    pub entities: Vec<EntityId>,
    pub id_to_entity: FxHashMap<PersistId, EntityId>,
}

impl PersistPrefabInstance {
    pub fn root(&self) -> EntityId {
        self.root
    }

    pub fn entity(&self, id: &PersistId) -> Option<EntityId> {
        self.id_to_entity.get(id).copied()
    }
}

fn cleanup_spawned(world: &mut World, entities: &[EntityId]) {
    for entity in entities.iter().rev().copied() {
        let _ = world.despawn(entity);
    }
}

trait PersistComponentAdapter {
    fn has(&self, world: &World, entity: EntityId) -> bool;
    fn validate(&self, value: &PersistValue) -> Result<(), PersistError>;
    fn insert(
        &self,
        world: &mut World,
        entity: EntityId,
        value: &PersistValue,
    ) -> Result<(), PersistError>;
    fn capture(
        &self,
        world: &World,
        entity: EntityId,
    ) -> Result<Option<(String, PersistValue)>, PersistError>;
}

struct PersistComponentCodec<T> {
    type_name: String,
    marker: PhantomData<fn() -> T>,
}

impl<T> PersistComponentAdapter for PersistComponentCodec<T>
where
    T: Serialize + DeserializeOwned + 'static,
{
    fn has(&self, world: &World, entity: EntityId) -> bool {
        world.has::<T>(entity)
    }

    fn validate(&self, value: &PersistValue) -> Result<(), PersistError> {
        self.decode(value).map(drop)
    }

    fn insert(
        &self,
        world: &mut World,
        entity: EntityId,
        value: &PersistValue,
    ) -> Result<(), PersistError> {
        let component = self.decode(value)?;
        if world.insert(entity, component) {
            Ok(())
        } else {
            Err(PersistError::MissingRuntimeEntity(entity))
        }
    }

    fn capture(
        &self,
        world: &World,
        entity: EntityId,
    ) -> Result<Option<(String, PersistValue)>, PersistError> {
        let Some(component) = world.get::<T>(entity) else {
            return Ok(None);
        };
        let value =
            serde_json::to_value(component).map_err(|error| PersistError::ComponentSerde {
                type_name: self.type_name.clone(),
                error: error.to_string(),
            })?;
        Ok(Some((self.type_name.clone(), PersistValue::from(value))))
    }
}

impl<T> PersistComponentCodec<T>
where
    T: DeserializeOwned,
{
    fn decode(&self, value: &PersistValue) -> Result<T, PersistError> {
        serde_json::from_value::<T>(value.as_json().clone()).map_err(|error| {
            PersistError::ComponentSerde {
                type_name: self.type_name.clone(),
                error: error.to_string(),
            }
        })
    }
}
