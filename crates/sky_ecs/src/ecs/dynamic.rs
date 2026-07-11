//! Safe runtime-typed ECS entry points for tools, scripts, and editors.
//!
//! This module is intentionally separate from [`crate::ecs::expert`]: dynamic
//! APIs validate component identity and access mode at runtime, while expert
//! APIs hand storage invariants to the caller.

use super::commands::InsertValue;
use super::query::{resolve_column_ptr, PreparedCache, QueryComponent, QueryDescriptor};
use super::{component_type, ComponentType, EntityId, World};
use core::fmt;
use core::marker::PhantomData;
use core::slice;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicAccess {
    Read,
    Write,
}

#[derive(Debug)]
pub enum DynamicQueryError {
    DuplicateComponent {
        component: ComponentType,
    },
    InvalidSlot {
        slot: usize,
        slot_count: usize,
    },
    ComponentMismatch {
        slot: usize,
        expected: ComponentType,
        actual: ComponentType,
    },
    RequiresMutableWorld,
    RequiresWriteAccess {
        slot: usize,
    },
    MissingRequiredComponent {
        slot: usize,
    },
    SlotAlias {
        left: usize,
        right: usize,
    },
}

impl fmt::Display for DynamicQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateComponent { component } => {
                write!(
                    f,
                    "duplicate component `{}` in dynamic query",
                    component.name
                )
            }
            Self::InvalidSlot { slot, slot_count } => {
                write!(f, "dynamic query slot {slot} is outside 0..{slot_count}")
            }
            Self::ComponentMismatch {
                slot,
                expected,
                actual,
            } => write!(
                f,
                "dynamic query slot {slot} expected `{}`, found `{}`",
                expected.name, actual.name
            ),
            Self::RequiresMutableWorld => {
                write!(f, "dynamic query with write slots requires &mut World")
            }
            Self::RequiresWriteAccess { slot } => {
                write!(f, "dynamic query slot {slot} was not declared writable")
            }
            Self::MissingRequiredComponent { slot } => {
                write!(
                    f,
                    "dynamic query required component at slot {slot} is absent"
                )
            }
            Self::SlotAlias { left, right } => {
                write!(f, "dynamic query slots {left} and {right} alias")
            }
        }
    }
}

impl std::error::Error for DynamicQueryError {}

#[derive(Debug)]
pub enum DynamicSpawnError {
    DuplicateComponent { component: ComponentType },
}

impl fmt::Display for DynamicSpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateComponent { component } => {
                write!(
                    f,
                    "duplicate component `{}` in dynamic bundle",
                    component.name
                )
            }
        }
    }
}

impl std::error::Error for DynamicSpawnError {}

pub struct ErasedComponentValue {
    pub(crate) component: ComponentType,
    pub(crate) value: InsertValue,
}

impl ErasedComponentValue {
    pub fn from_typed<T: 'static>(value: T) -> Self {
        Self {
            component: component_type::<T>(),
            value: InsertValue::from_value(value),
        }
    }

    #[inline]
    pub fn component(&self) -> ComponentType {
        self.component
    }
}

#[derive(Default)]
pub struct DynamicBundle {
    pub(crate) values: Vec<ErasedComponentValue>,
}

impl DynamicBundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with<T: 'static>(mut self, value: T) -> Self {
        self.values.push(ErasedComponentValue::from_typed(value));
        self
    }

    pub fn from_values(values: Vec<ErasedComponentValue>) -> Result<Self, DynamicSpawnError> {
        validate_unique_components(values.iter().map(|value| value.component), |component| {
            DynamicSpawnError::DuplicateComponent { component }
        })?;
        Ok(Self { values })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

pub trait WorldDynamicExt {
    fn spawn_dynamic(&mut self, bundle: DynamicBundle) -> Result<EntityId, DynamicSpawnError>;
}

impl WorldDynamicExt for World {
    fn spawn_dynamic(&mut self, mut bundle: DynamicBundle) -> Result<EntityId, DynamicSpawnError> {
        self.spawn_dynamic_values(&mut bundle.values)
    }
}

#[derive(Clone, Copy)]
struct DynamicQuerySlot {
    component: ComponentType,
    access: DynamicAccess,
    optional: bool,
}

#[derive(Default)]
pub struct DynamicQueryBuilder {
    slots: Vec<DynamicQuerySlot>,
}

impl DynamicQueryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read<T: 'static>(self) -> Self {
        self.read_component(component_type::<T>())
    }

    pub fn write<T: 'static>(self) -> Self {
        self.write_component(component_type::<T>())
    }

    pub fn optional_read<T: 'static>(self) -> Self {
        self.optional_read_component(component_type::<T>())
    }

    pub fn optional_write<T: 'static>(self) -> Self {
        self.optional_write_component(component_type::<T>())
    }

    pub fn read_component(mut self, component: ComponentType) -> Self {
        self.slots.push(DynamicQuerySlot {
            component,
            access: DynamicAccess::Read,
            optional: false,
        });
        self
    }

    pub fn write_component(mut self, component: ComponentType) -> Self {
        self.slots.push(DynamicQuerySlot {
            component,
            access: DynamicAccess::Write,
            optional: false,
        });
        self
    }

    pub fn optional_read_component(mut self, component: ComponentType) -> Self {
        self.slots.push(DynamicQuerySlot {
            component,
            access: DynamicAccess::Read,
            optional: true,
        });
        self
    }

    pub fn optional_write_component(mut self, component: ComponentType) -> Self {
        self.slots.push(DynamicQuerySlot {
            component,
            access: DynamicAccess::Write,
            optional: true,
        });
        self
    }

    pub fn build(self) -> Result<DynamicQuery, DynamicQueryError> {
        validate_unique_components(self.slots.iter().map(|slot| slot.component), |component| {
            DynamicQueryError::DuplicateComponent { component }
        })?;

        let mut components = SmallVec::with_capacity(self.slots.len());
        let mut has_writes = false;
        for slot in &self.slots {
            has_writes |= slot.access == DynamicAccess::Write;
            let component = if slot.optional {
                QueryComponent::optional(slot.component, slot.access == DynamicAccess::Write)
            } else {
                QueryComponent::new(slot.component, slot.access == DynamicAccess::Write)
            };
            components.push(component);
        }

        Ok(DynamicQuery {
            descriptor: QueryDescriptor::new(components),
            prepared: PreparedCache::default(),
            slots: self.slots,
            has_writes,
        })
    }
}

pub struct DynamicQuery {
    descriptor: QueryDescriptor,
    prepared: PreparedCache,
    slots: Vec<DynamicQuerySlot>,
    has_writes: bool,
}

impl DynamicQuery {
    pub fn builder() -> DynamicQueryBuilder {
        DynamicQueryBuilder::new()
    }

    #[inline]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub fn has_writes(&self) -> bool {
        self.has_writes
    }

    pub fn for_each_chunk<F>(&mut self, world: &World, mut f: F) -> Result<(), DynamicQueryError>
    where
        F: for<'w> FnMut(DynamicQueryChunk<'w>) -> Result<(), DynamicQueryError>,
    {
        if self.has_writes {
            return Err(DynamicQueryError::RequiresMutableWorld);
        }

        self.prepared.prepare::<()>(world, &self.descriptor);
        let mut result = Ok(());
        self.prepared.visit_chunks(world, |cached, chunk| {
            if result.is_err() {
                return;
            }
            result = f(DynamicQueryChunk {
                chunk,
                slots: &self.slots,
                component_indices: &cached.component_indices,
            });
        });
        result
    }

    pub fn for_each_chunk_mut<F>(
        &mut self,
        world: &mut World,
        mut f: F,
    ) -> Result<(), DynamicQueryError>
    where
        F: for<'w> FnMut(DynamicQueryChunkMut<'w>) -> Result<(), DynamicQueryError>,
    {
        self.prepared.prepare::<()>(world, &self.descriptor);
        let mut result = Ok(());
        self.prepared.visit_chunks(world, |cached, chunk| {
            if result.is_err() {
                return;
            }
            result = f(DynamicQueryChunkMut {
                chunk,
                slots: &self.slots,
                component_indices: &cached.component_indices,
                marker: PhantomData,
            });
        });
        result
    }
}

pub struct DynamicQueryChunk<'w> {
    chunk: &'w super::Chunk,
    slots: &'w [DynamicQuerySlot],
    component_indices: &'w [u8],
}

impl<'w> DynamicQueryChunk<'w> {
    #[inline]
    pub fn len(&self) -> usize {
        self.chunk.entity_count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn entities(&self) -> &'w [EntityId] {
        self.chunk.entities()
    }

    pub fn component(&self, slot: usize) -> Result<ComponentType, DynamicQueryError> {
        Ok(self.slot(slot)?.component)
    }

    pub fn read<T: 'static>(&self, slot: usize) -> Result<&'w [T], DynamicQueryError> {
        self.optional_read(slot)?
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot })
    }

    pub fn optional_read<T: 'static>(
        &self,
        slot: usize,
    ) -> Result<Option<&'w [T]>, DynamicQueryError> {
        self.validate_type::<T>(slot)?;
        let Some(ptr) = self.ptr(slot) else {
            return Ok(None);
        };
        Ok(Some(unsafe {
            slice::from_raw_parts(ptr.cast::<T>(), self.chunk.entity_count)
        }))
    }

    pub fn read_pair<A: 'static, B: 'static>(
        &self,
        a: usize,
        b: usize,
    ) -> Result<(&'w [A], &'w [B]), DynamicQueryError> {
        self.ensure_distinct(a, b)?;
        Ok((self.read(a)?, self.read(b)?))
    }

    fn slot(&self, slot: usize) -> Result<DynamicQuerySlot, DynamicQueryError> {
        self.slots
            .get(slot)
            .copied()
            .ok_or(DynamicQueryError::InvalidSlot {
                slot,
                slot_count: self.slots.len(),
            })
    }

    fn validate_type<T: 'static>(&self, slot: usize) -> Result<(), DynamicQueryError> {
        let actual = self.slot(slot)?.component;
        let expected = component_type::<T>();
        if actual.id() != expected.id() {
            return Err(DynamicQueryError::ComponentMismatch {
                slot,
                expected,
                actual,
            });
        }
        Ok(())
    }

    fn ptr(&self, slot: usize) -> Option<*const u8> {
        let ptr = resolve_column_ptr(self.chunk, self.component_indices[slot]);
        (!ptr.is_null()).then_some(ptr.cast_const())
    }

    fn ensure_distinct(&self, left: usize, right: usize) -> Result<(), DynamicQueryError> {
        if left == right {
            return Err(DynamicQueryError::SlotAlias { left, right });
        }
        Ok(())
    }
}

pub struct DynamicQueryChunkMut<'w> {
    chunk: &'w super::Chunk,
    slots: &'w [DynamicQuerySlot],
    component_indices: &'w [u8],
    marker: PhantomData<&'w mut World>,
}

impl<'w> DynamicQueryChunkMut<'w> {
    #[inline]
    pub fn len(&self) -> usize {
        self.chunk.entity_count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn entities(&self) -> &'w [EntityId] {
        self.chunk.entities()
    }

    pub fn component(&self, slot: usize) -> Result<ComponentType, DynamicQueryError> {
        Ok(self.slot(slot)?.component)
    }

    pub fn read<T: 'static>(&self, slot: usize) -> Result<&[T], DynamicQueryError> {
        self.optional_read(slot)?
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot })
    }

    pub fn optional_read<T: 'static>(
        &self,
        slot: usize,
    ) -> Result<Option<&[T]>, DynamicQueryError> {
        self.validate_type::<T>(slot)?;
        let Some(ptr) = self.ptr(slot) else {
            return Ok(None);
        };
        Ok(Some(unsafe {
            slice::from_raw_parts(ptr.cast::<T>(), self.chunk.entity_count)
        }))
    }

    pub fn write<T: 'static>(&mut self, slot: usize) -> Result<&mut [T], DynamicQueryError> {
        self.optional_write(slot)?
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot })
    }

    pub fn optional_write<T: 'static>(
        &mut self,
        slot: usize,
    ) -> Result<Option<&mut [T]>, DynamicQueryError> {
        self.validate_type::<T>(slot)?;
        self.ensure_write(slot)?;
        let Some(ptr) = self.ptr(slot) else {
            return Ok(None);
        };
        Ok(Some(unsafe {
            slice::from_raw_parts_mut(ptr.cast::<T>(), self.chunk.entity_count)
        }))
    }

    pub fn write_read<A: 'static, B: 'static>(
        &mut self,
        write_slot: usize,
        read_slot: usize,
    ) -> Result<(&mut [A], &[B]), DynamicQueryError> {
        self.ensure_distinct(write_slot, read_slot)?;
        self.validate_type::<A>(write_slot)?;
        self.validate_type::<B>(read_slot)?;
        self.ensure_write(write_slot)?;
        let write_ptr = self
            .ptr(write_slot)
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot: write_slot })?;
        let read_ptr = self
            .ptr(read_slot)
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot: read_slot })?;
        Ok(unsafe {
            (
                slice::from_raw_parts_mut(write_ptr.cast::<A>(), self.chunk.entity_count),
                slice::from_raw_parts(read_ptr.cast::<B>(), self.chunk.entity_count),
            )
        })
    }

    pub fn write_write<A: 'static, B: 'static>(
        &mut self,
        left_slot: usize,
        right_slot: usize,
    ) -> Result<(&mut [A], &mut [B]), DynamicQueryError> {
        self.ensure_distinct(left_slot, right_slot)?;
        self.validate_type::<A>(left_slot)?;
        self.validate_type::<B>(right_slot)?;
        self.ensure_write(left_slot)?;
        self.ensure_write(right_slot)?;
        let left_ptr = self
            .ptr(left_slot)
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot: left_slot })?;
        let right_ptr = self
            .ptr(right_slot)
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot: right_slot })?;
        Ok(unsafe {
            (
                slice::from_raw_parts_mut(left_ptr.cast::<A>(), self.chunk.entity_count),
                slice::from_raw_parts_mut(right_ptr.cast::<B>(), self.chunk.entity_count),
            )
        })
    }

    pub fn write_optional_read<A: 'static, B: 'static>(
        &mut self,
        write_slot: usize,
        read_slot: usize,
    ) -> Result<(&mut [A], Option<&[B]>), DynamicQueryError> {
        self.ensure_distinct(write_slot, read_slot)?;
        self.validate_type::<A>(write_slot)?;
        self.validate_type::<B>(read_slot)?;
        self.ensure_write(write_slot)?;
        let write_ptr = self
            .ptr(write_slot)
            .ok_or(DynamicQueryError::MissingRequiredComponent { slot: write_slot })?;
        let read_ptr = self.ptr(read_slot);
        Ok(unsafe {
            (
                slice::from_raw_parts_mut(write_ptr.cast::<A>(), self.chunk.entity_count),
                read_ptr.map(|ptr| slice::from_raw_parts(ptr.cast::<B>(), self.chunk.entity_count)),
            )
        })
    }

    fn slot(&self, slot: usize) -> Result<DynamicQuerySlot, DynamicQueryError> {
        self.slots
            .get(slot)
            .copied()
            .ok_or(DynamicQueryError::InvalidSlot {
                slot,
                slot_count: self.slots.len(),
            })
    }

    fn validate_type<T: 'static>(&self, slot: usize) -> Result<(), DynamicQueryError> {
        let actual = self.slot(slot)?.component;
        let expected = component_type::<T>();
        if actual.id() != expected.id() {
            return Err(DynamicQueryError::ComponentMismatch {
                slot,
                expected,
                actual,
            });
        }
        Ok(())
    }

    fn ensure_write(&self, slot: usize) -> Result<(), DynamicQueryError> {
        if self.slot(slot)?.access != DynamicAccess::Write {
            return Err(DynamicQueryError::RequiresWriteAccess { slot });
        }
        Ok(())
    }

    fn ptr(&self, slot: usize) -> Option<*mut u8> {
        let ptr = resolve_column_ptr(self.chunk, self.component_indices[slot]);
        (!ptr.is_null()).then_some(ptr)
    }

    fn ensure_distinct(&self, left: usize, right: usize) -> Result<(), DynamicQueryError> {
        if left == right {
            return Err(DynamicQueryError::SlotAlias { left, right });
        }
        Ok(())
    }
}

fn validate_unique_components<E>(
    components: impl IntoIterator<Item = ComponentType>,
    make_error: impl Fn(ComponentType) -> E,
) -> Result<(), E> {
    let components = components.into_iter().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        for other in &components[(index + 1)..] {
            if component.id() == other.id() {
                return Err(make_error(*component));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DynamicBundle, DynamicQuery, DynamicQueryError, WorldDynamicExt};
    use crate::ecs::World;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Health(f32);

    #[derive(Debug)]
    struct Droppable {
        drops: Arc<AtomicUsize>,
    }

    impl Drop for Droppable {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn spawn_dynamic_writes_typed_values() {
        let mut world = World::new();
        let entity = world
            .spawn_dynamic(
                DynamicBundle::new()
                    .with(Position { x: 1.0, y: 2.0 })
                    .with(Velocity { x: 3.0, y: 4.0 }),
            )
            .unwrap();

        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 2.0 })
        );
        assert_eq!(
            world.get::<Velocity>(entity),
            Some(&Velocity { x: 3.0, y: 4.0 })
        );
    }

    #[test]
    fn dynamic_query_updates_from_read_slice() {
        let mut world = World::new();
        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 2.0 }));
        world.spawn((Position { x: 10.0, y: 0.0 },));

        let mut query = DynamicQuery::builder()
            .write::<Position>()
            .optional_read::<Velocity>()
            .build()
            .unwrap();

        query
            .for_each_chunk_mut(&mut world, |mut chunk| {
                let (positions, velocities) =
                    chunk.write_optional_read::<Position, Velocity>(0, 1)?;
                if let Some(velocities) = velocities {
                    for (position, velocity) in positions.iter_mut().zip(velocities) {
                        position.x += velocity.x;
                        position.y += velocity.y;
                    }
                }
                Ok(())
            })
            .unwrap();

        let mut positions = Vec::new();
        let check = world.query::<&Position>();
        check.for_each(|position| positions.push(*position));
        positions.sort_by(|left, right| left.x.total_cmp(&right.x));
        assert_eq!(
            positions,
            vec![Position { x: 1.0, y: 2.0 }, Position { x: 10.0, y: 0.0 },]
        );
    }

    #[test]
    fn readonly_iteration_rejects_write_slots() {
        let world = World::new();
        let mut query = DynamicQuery::builder().write::<Position>().build().unwrap();

        let err = query.for_each_chunk(&world, |_| Ok(())).unwrap_err();
        assert!(matches!(err, DynamicQueryError::RequiresMutableWorld));
    }

    #[test]
    fn duplicate_query_components_return_error() {
        let result = DynamicQuery::builder()
            .read::<Health>()
            .optional_read::<Health>()
            .build();

        assert!(matches!(
            result,
            Err(DynamicQueryError::DuplicateComponent { .. })
        ));
    }

    #[test]
    fn dynamic_spawn_transfers_drop_ownership_to_world() {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();

        let entity = world
            .spawn_dynamic(DynamicBundle::new().with(Droppable {
                drops: drops.clone(),
            }))
            .unwrap();

        assert_eq!(drops.load(Ordering::Relaxed), 0);
        assert!(world.despawn(entity));
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn failed_dynamic_spawn_drops_unconsumed_values() {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();

        let result = world.spawn_dynamic(
            DynamicBundle::new()
                .with(Droppable {
                    drops: drops.clone(),
                })
                .with(Droppable {
                    drops: drops.clone(),
                }),
        );

        assert!(result.is_err());
        assert_eq!(world.entity_count(), 0);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }
}
