use super::commands::InsertValue;
use super::resource::Resources;
use super::*;
use crate::ecs::entity::{EntityLocation, EntityRecord};
use crate::ecs::system::{GroupBuilder, Schedule, TickPolicy};
use crate::ecs::time::Time;
use crate::reflect::register_rust_type;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::Cell;
use std::ptr::NonNull;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TransitionKey {
    archetype: Archetype,
    component_id: usize,
    add: bool,
}

struct TransitionPlan {
    copy_spans: SmallVec<[(usize, usize, usize); MAX_COMPONENTS]>,
    target_component_offset: Option<usize>,
    target_data_index: Cell<Option<usize>>,
}

pub struct World {
    pub time: Time,
    pub(crate) data: Vec<Data>,
    archetype_epoch: usize,
    archetype_to_data_index: FxHashMap<Archetype, usize>,
    transitions: FxHashMap<TransitionKey, Box<TransitionPlan>>,
    entities: Vec<EntityRecord>,
    free_entities: Vec<u32>,
    resources: Resources,
    schedule: Option<Schedule>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            time: Time::default(),
            data: Vec::new(),
            archetype_epoch: 0,
            archetype_to_data_index: FxHashMap::default(),
            transitions: FxHashMap::default(),
            entities: Vec::new(),
            free_entities: Vec::new(),
            resources: Resources::default(),
            schedule: Some(Schedule::default()),
        }
    }

    // -----------------------------------------------------------------------
    // Schedule API
    // -----------------------------------------------------------------------

    pub fn group(&mut self, name: &str) -> GroupBuilder<'_> {
        let schedule = self
            .schedule
            .as_mut()
            .expect("cannot modify schedule during tick");
        let index = schedule.find_or_create_group(name);
        GroupBuilder::new(schedule, index)
    }

    pub fn tick(&mut self) {
        let mut schedule = self.schedule.take().expect("cannot call tick recursively");

        let now = std::time::Instant::now();
        let real_delta = match schedule.last_tick {
            Some(last) => (now - last).as_secs_f32(),
            None => 0.0,
        };
        schedule.last_tick = Some(now);

        let scaled_delta = real_delta * self.time.time_scale;
        self.run_schedule(&mut schedule, scaled_delta);

        self.schedule = Some(schedule);
    }

    pub fn tick_with_delta(&mut self, delta: f32) {
        let mut schedule = self.schedule.take().expect("cannot call tick recursively");

        let scaled_delta = delta * self.time.time_scale;
        self.run_schedule(&mut schedule, scaled_delta);

        self.schedule = Some(schedule);
    }

    fn run_schedule(&mut self, schedule: &mut Schedule, scaled_delta: f32) {
        self.time.frame_count += 1;

        for group in &mut schedule.groups {
            match group.tick_policy {
                TickPolicy::EveryFrame => {
                    self.time.delta = scaled_delta;
                    for registered in &mut group.systems {
                        if !registered.initialized {
                            registered.system.init(self);
                            registered.initialized = true;
                        }
                        registered.system.run(self);
                    }
                }
                TickPolicy::Fixed(fixed_dt) => {
                    group.accumulator += scaled_delta;
                    self.time.delta = fixed_dt;
                    while group.accumulator >= fixed_dt {
                        for registered in &mut group.systems {
                            if !registered.initialized {
                                registered.system.init(self);
                                registered.initialized = true;
                            }
                            registered.system.run(self);
                        }
                        group.accumulator -= fixed_dt;
                    }
                }
            }
        }

        self.time.elapsed += scaled_delta;
    }

    pub fn shutdown(&mut self) {
        let mut schedule = self.schedule.take().expect("cannot shutdown during tick");

        for group in schedule.groups.iter_mut().rev() {
            for registered in group.systems.iter_mut().rev() {
                if registered.initialized {
                    registered.system.teardown(self);
                    registered.initialized = false;
                }
            }
        }

        self.schedule = Some(schedule);
    }

    fn allocate_entity(&mut self) -> EntityId {
        if let Some(index) = self.free_entities.pop() {
            let record = &self.entities[index as usize];
            EntityId::new(index, record.generation)
        } else {
            let index = self.entities.len() as u32;
            self.entities.push(EntityRecord {
                generation: 0,
                location: None,
            });
            EntityId::new(index, 0)
        }
    }

    #[inline(always)]
    fn ensure_data_index(&mut self, archetype: Archetype) -> usize {
        if let Some(index) = self.archetype_to_data_index.get(&archetype).copied() {
            return index;
        }

        let index = self.data.len();
        self.data.push(Data::new(archetype));
        self.archetype_to_data_index.insert(archetype, index);
        self.archetype_epoch += 1;
        index
    }

    #[inline(always)]
    fn entity_location(&self, entity: EntityId) -> Option<EntityLocation> {
        let record = self.entities.get(entity.index() as usize)?;
        if record.generation != entity.generation() {
            return None;
        }

        record.location
    }

    #[inline(always)]
    pub(crate) fn set_entity_location(&mut self, entity: EntityId, location: EntityLocation) {
        let record = &mut self.entities[entity.index() as usize];
        debug_assert_eq!(record.generation, entity.generation());
        record.location = Some(location);
    }

    pub(crate) fn add_entity(&mut self, archetype: Archetype) -> EntityId {
        let entity = self.allocate_entity();
        let data_index = self.ensure_data_index(archetype);
        let location = self.data[data_index].add_entity(entity);
        self.set_entity_location(
            entity,
            EntityLocation {
                data_index,
                chunk_index: location.chunk_index,
                entity_index: location.entity_index,
            },
        );
        entity
    }

    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityId {
        let (archetype, columns) = B::cached_meta();
        let entity = self.allocate_entity();
        let data_index = self.ensure_data_index(archetype);
        let location = self.data[data_index].add_entity(entity);
        self.set_entity_location(
            entity,
            EntityLocation {
                data_index,
                chunk_index: location.chunk_index,
                entity_index: location.entity_index,
            },
        );

        let chunk = &mut self.data[data_index].chunks[location.chunk_index];
        unsafe {
            bundle.write_fast(chunk, location.entity_index, columns);
        }

        entity
    }

    pub fn spawn_batch<B: Bundle>(&mut self, bundles: impl IntoIterator<Item = B>) {
        let (archetype, columns) = B::cached_meta();
        let data_index = self.ensure_data_index(archetype);
        let iter = bundles.into_iter();
        let (lower, _) = iter.size_hint();

        // Pre-reserve entity record storage to avoid per-entity Vec reallocation.
        if lower > 0 {
            self.entities.reserve(lower);
        }

        for bundle in iter {
            let entity = self.allocate_entity();
            let location = self.data[data_index].add_entity(entity);
            self.set_entity_location(
                entity,
                EntityLocation {
                    data_index,
                    chunk_index: location.chunk_index,
                    entity_index: location.entity_index,
                },
            );

            let chunk = &mut self.data[data_index].chunks[location.chunk_index];
            unsafe {
                bundle.write_fast(chunk, location.entity_index, columns);
            }
        }
    }

    pub fn contains(&self, entity: EntityId) -> bool {
        self.entity_location(entity).is_some()
    }

    pub fn insert_resource<R: 'static>(&mut self, resource: R) -> Option<R> {
        self.resources.insert(resource)
    }

    pub fn get_resource<R: 'static>(&self) -> Option<&R> {
        self.resources.get::<R>()
    }

    pub fn get_resource_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.resources.get_mut::<R>()
    }

    pub fn contains_resource<R: 'static>(&self) -> bool {
        self.resources.contains::<R>()
    }

    pub fn remove_resource<R: 'static>(&mut self) -> Option<R> {
        self.resources.remove::<R>()
    }

    #[inline(always)]
    pub fn has<T: 'static>(&self, entity: EntityId) -> bool {
        let Some(location) = self.entity_location(entity) else {
            return false;
        };

        self.data[location.data_index]
            .archetype
            .has_component(&register_rust_type::<T>())
    }

    pub fn despawn(&mut self, entity: EntityId) -> bool {
        let Some(location) = self.entity_location(entity) else {
            return false;
        };

        let moved = self.data[location.data_index].remove_entity(ChunkEntityLocation {
            chunk_index: location.chunk_index,
            entity_index: location.entity_index,
        });

        let record = &mut self.entities[entity.index() as usize];
        record.location = None;
        record.generation = record.generation.wrapping_add(1);
        self.free_entities.push(entity.index());

        if let Some((moved_entity, moved_location)) = moved {
            self.set_entity_location(
                moved_entity,
                EntityLocation {
                    data_index: location.data_index,
                    chunk_index: moved_location.chunk_index,
                    entity_index: moved_location.entity_index,
                },
            );
        }

        true
    }

    #[inline(always)]
    pub fn get<T: 'static>(&self, entity: EntityId) -> Option<&T> {
        let location = self.entity_location(entity)?;
        let data = &self.data[location.data_index];
        let chunk = &data.chunks[location.chunk_index];
        let component_index = chunk
            .archetype
            .query_component_index(&register_rust_type::<T>())?;

        Some(unsafe {
            let ptr = chunk
                .column_ptr(component_index)
                .add(location.entity_index * std::mem::size_of::<T>());
            &*(ptr as *const T)
        })
    }

    #[inline(always)]
    pub fn get_mut<T: 'static>(&mut self, entity: EntityId) -> Option<&mut T> {
        let location = self.entity_location(entity)?;
        let data = &mut self.data[location.data_index];
        let chunk = &mut data.chunks[location.chunk_index];
        let component_index = chunk
            .archetype
            .query_component_index(&register_rust_type::<T>())?;

        Some(unsafe {
            let ptr = chunk
                .column_ptr(component_index)
                .add(location.entity_index * std::mem::size_of::<T>());
            &mut *(ptr as *mut T)
        })
    }

    fn archetype_with_component(base: Archetype, component: crate::reflect::Type) -> Archetype {
        let mut builder = create_archetype();
        for existing in &base.components {
            builder = builder.add_component(*existing);
        }
        builder.add_component(component).build()
    }

    fn archetype_without_component(
        base: Archetype,
        component: crate::reflect::Type,
    ) -> Option<Archetype> {
        if !base.has_component(&component) {
            return None;
        }

        let mut builder = create_archetype();
        let mut remaining = 0usize;

        for existing in &base.components {
            if existing.id() != component.id() {
                builder = builder.add_component(*existing);
                remaining += 1;
            }
        }

        if remaining == 0 {
            return None;
        }

        Some(builder.build())
    }

    /// Build copy-span descriptors for transitioning entity data from `source`
    /// to `target`.  Components whose type-id appears in `skip_component_ids`
    /// are excluded (used by insert-batches to avoid copying the newly-inserted
    /// column).  Pass `&[]` when no components should be skipped.
    fn build_copy_spans(
        source: &Data,
        target: &Data,
        skip_component_ids: &[usize],
    ) -> SmallVec<[(usize, usize, usize); MAX_COMPONENTS]> {
        let mut spans = SmallVec::new();

        for (source_index, component) in source.archetype.components.iter().enumerate() {
            if skip_component_ids
                .iter()
                .any(|id| *id == component.id())
            {
                continue;
            }

            let Some(target_index) = target.archetype.query_component_index(component) else {
                continue;
            };
            spans.push((
                source.layout.column_offset(source_index),
                target.layout.column_offset(target_index),
                component.size,
            ));
        }

        spans
    }

    fn transition_plan(
        &mut self,
        source_data_index: usize,
        component: crate::reflect::Type,
        add: bool,
    ) -> Option<NonNull<TransitionPlan>> {
        let base = self.data[source_data_index].archetype;
        let key = TransitionKey {
            archetype: base,
            component_id: component.id(),
            add,
        };

        if let Some(plan) = self.transitions.get(&key) {
            return Some(NonNull::from(plan.as_ref()));
        }

        let plan = if add {
            if base.has_component(&component) {
                return None;
            }

            let target_archetype = Self::archetype_with_component(base, component);
            let target_data_index = self.ensure_data_index(target_archetype);
            let target_component_offset = {
                let target_index = target_archetype.query_component_index(&component).unwrap();
                self.data[target_data_index]
                    .layout
                    .column_offset(target_index)
            };
            TransitionPlan {
                copy_spans: Self::build_copy_spans(
                    &self.data[source_data_index],
                    &self.data[target_data_index],
                    &[],
                ),
                target_component_offset: Some(target_component_offset),
                target_data_index: Cell::new(Some(target_data_index)),
            }
        } else {
            let target_archetype = Self::archetype_without_component(base, component)?;
            let target_data_index = self.ensure_data_index(target_archetype);
            TransitionPlan {
                copy_spans: Self::build_copy_spans(
                    &self.data[source_data_index],
                    &self.data[target_data_index],
                    &[],
                ),
                target_component_offset: None,
                target_data_index: Cell::new(Some(target_data_index)),
            }
        };

        let entry = self
            .transitions
            .entry(key)
            .or_insert_with(|| Box::new(plan));
        Some(NonNull::from(entry.as_ref()))
    }

    pub(crate) fn copy_components_with_spans(
        source: &Chunk,
        source_entity_index: usize,
        target: &mut Chunk,
        target_entity_index: usize,
        spans: &[(usize, usize, usize)],
    ) {
        let source_base = source.data_ptr();
        let target_base = target.data_ptr();

        for &(source_offset, target_offset, component_size) in spans {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    source_base.add(source_offset + source_entity_index * component_size),
                    target_base.add(target_offset + target_entity_index * component_size),
                    component_size,
                );
            }
        }
    }

    pub fn insert<T: Copy + 'static>(&mut self, entity: EntityId, component: T) -> bool {
        let Some(source_location) = self.entity_location(entity) else {
            return false;
        };

        let component_ty = register_rust_type::<T>();
        let source_archetype = self.data[source_location.data_index].archetype;

        if source_archetype.has_component(&component_ty) {
            if let Some(existing) = self.get_mut::<T>(entity) {
                *existing = component;
                return true;
            }
            return false;
        }

        let plan = self
            .transition_plan(source_location.data_index, component_ty, true)
            .expect("adding a missing component must produce a transition plan");
        let plan = unsafe { plan.as_ref() };
        let target_data_index = plan
            .target_data_index
            .get()
            .expect("transition plans must cache the target data index");
        let target_location = self.data[target_data_index].add_entity(entity);

        {
            let (source_chunk, target_chunk) = if source_location.data_index < target_data_index {
                let (left, right) = self.data.split_at_mut(target_data_index);
                (
                    &left[source_location.data_index].chunks[source_location.chunk_index],
                    &mut right[0].chunks[target_location.chunk_index],
                )
            } else {
                let (left, right) = self.data.split_at_mut(source_location.data_index);
                (
                    &right[0].chunks[source_location.chunk_index],
                    &mut left[target_data_index].chunks[target_location.chunk_index],
                )
            };

            Self::copy_components_with_spans(
                source_chunk,
                source_location.entity_index,
                target_chunk,
                target_location.entity_index,
                &plan.copy_spans,
            );

            let component_offset = plan
                .target_component_offset
                .expect("add transition plans must include the inserted component offset");
            unsafe {
                let ptr = target_chunk.data_ptr().add(
                    component_offset + target_location.entity_index * std::mem::size_of::<T>(),
                );
                *(ptr as *mut T) = component;
            }
        }

        self.set_entity_location(
            entity,
            EntityLocation {
                data_index: target_data_index,
                chunk_index: target_location.chunk_index,
                entity_index: target_location.entity_index,
            },
        );

        let moved = self.data[source_location.data_index].remove_entity(ChunkEntityLocation {
            chunk_index: source_location.chunk_index,
            entity_index: source_location.entity_index,
        });

        if let Some((moved_entity, moved_location)) = moved {
            self.set_entity_location(
                moved_entity,
                EntityLocation {
                    data_index: source_location.data_index,
                    chunk_index: moved_location.chunk_index,
                    entity_index: moved_location.entity_index,
                },
            );
        }

        true
    }

    pub fn remove<T: 'static>(&mut self, entity: EntityId) -> bool {
        let Some(source_location) = self.entity_location(entity) else {
            return false;
        };

        let component_ty = register_rust_type::<T>();
        let Some(plan) = self.transition_plan(source_location.data_index, component_ty, false)
        else {
            return false;
        };
        let plan = unsafe { plan.as_ref() };

        let target_data_index = plan
            .target_data_index
            .get()
            .expect("transition plans must cache the target data index");
        let target_location = self.data[target_data_index].add_entity(entity);

        {
            let (source_chunk, target_chunk) = if source_location.data_index < target_data_index {
                let (left, right) = self.data.split_at_mut(target_data_index);
                (
                    &left[source_location.data_index].chunks[source_location.chunk_index],
                    &mut right[0].chunks[target_location.chunk_index],
                )
            } else {
                let (left, right) = self.data.split_at_mut(source_location.data_index);
                (
                    &right[0].chunks[source_location.chunk_index],
                    &mut left[target_data_index].chunks[target_location.chunk_index],
                )
            };

            Self::copy_components_with_spans(
                source_chunk,
                source_location.entity_index,
                target_chunk,
                target_location.entity_index,
                &plan.copy_spans,
            );
        }

        self.set_entity_location(
            entity,
            EntityLocation {
                data_index: target_data_index,
                chunk_index: target_location.chunk_index,
                entity_index: target_location.entity_index,
            },
        );

        let moved = self.data[source_location.data_index].remove_entity(ChunkEntityLocation {
            chunk_index: source_location.chunk_index,
            entity_index: source_location.entity_index,
        });

        if let Some((moved_entity, moved_location)) = moved {
            self.set_entity_location(
                moved_entity,
                EntityLocation {
                    data_index: source_location.data_index,
                    chunk_index: moved_location.chunk_index,
                    entity_index: moved_location.entity_index,
                },
            );
        }

        true
    }

    pub(crate) fn insert_dynamic(
        &mut self,
        entity: EntityId,
        component: crate::reflect::Type,
        value: &InsertValue,
    ) -> bool {
        let Some(source_location) = self.entity_location(entity) else {
            return false;
        };

        let source_archetype = self.data[source_location.data_index].archetype;

        if source_archetype.has_component(&component) {
            let component_index = source_archetype.query_component_index(&component).unwrap();
            let data = &mut self.data[source_location.data_index];
            let chunk = &mut data.chunks[source_location.chunk_index];
            let ptr = unsafe {
                chunk
                    .column_ptr(component_index)
                    .add(source_location.entity_index * component.size)
            };
            value.write(ptr);
            return true;
        }

        let plan = self
            .transition_plan(source_location.data_index, component, true)
            .expect("adding a missing component must produce a transition plan");
        let plan = unsafe { plan.as_ref() };
        let target_data_index = plan
            .target_data_index
            .get()
            .expect("transition plans must cache the target data index");
        let target_location = self.data[target_data_index].add_entity(entity);

        {
            let (source_chunk, target_chunk) = if source_location.data_index < target_data_index {
                let (left, right) = self.data.split_at_mut(target_data_index);
                (
                    &left[source_location.data_index].chunks[source_location.chunk_index],
                    &mut right[0].chunks[target_location.chunk_index],
                )
            } else {
                let (left, right) = self.data.split_at_mut(source_location.data_index);
                (
                    &right[0].chunks[source_location.chunk_index],
                    &mut left[target_data_index].chunks[target_location.chunk_index],
                )
            };

            Self::copy_components_with_spans(
                source_chunk,
                source_location.entity_index,
                target_chunk,
                target_location.entity_index,
                &plan.copy_spans,
            );

            let component_offset = plan
                .target_component_offset
                .expect("add transition plans must include the inserted component offset");
            let ptr = unsafe {
                target_chunk
                    .data_ptr()
                    .add(component_offset + target_location.entity_index * component.size)
            };
            value.write(ptr);
        }

        self.set_entity_location(
            entity,
            EntityLocation {
                data_index: target_data_index,
                chunk_index: target_location.chunk_index,
                entity_index: target_location.entity_index,
            },
        );

        let moved = self.data[source_location.data_index].remove_entity(ChunkEntityLocation {
            chunk_index: source_location.chunk_index,
            entity_index: source_location.entity_index,
        });

        if let Some((moved_entity, moved_location)) = moved {
            self.set_entity_location(
                moved_entity,
                EntityLocation {
                    data_index: source_location.data_index,
                    chunk_index: moved_location.chunk_index,
                    entity_index: moved_location.entity_index,
                },
            );
        }

        true
    }

    pub(crate) fn remove_dynamic(
        &mut self,
        entity: EntityId,
        component: crate::reflect::Type,
    ) -> bool {
        let Some(source_location) = self.entity_location(entity) else {
            return false;
        };

        let Some(plan) = self.transition_plan(source_location.data_index, component, false) else {
            return false;
        };
        let plan = unsafe { plan.as_ref() };

        let target_data_index = plan
            .target_data_index
            .get()
            .expect("transition plans must cache the target data index");
        let target_location = self.data[target_data_index].add_entity(entity);

        {
            let (source_chunk, target_chunk) = if source_location.data_index < target_data_index {
                let (left, right) = self.data.split_at_mut(target_data_index);
                (
                    &left[source_location.data_index].chunks[source_location.chunk_index],
                    &mut right[0].chunks[target_location.chunk_index],
                )
            } else {
                let (left, right) = self.data.split_at_mut(source_location.data_index);
                (
                    &right[0].chunks[source_location.chunk_index],
                    &mut left[target_data_index].chunks[target_location.chunk_index],
                )
            };

            Self::copy_components_with_spans(
                source_chunk,
                source_location.entity_index,
                target_chunk,
                target_location.entity_index,
                &plan.copy_spans,
            );
        }

        self.set_entity_location(
            entity,
            EntityLocation {
                data_index: target_data_index,
                chunk_index: target_location.chunk_index,
                entity_index: target_location.entity_index,
            },
        );

        let moved = self.data[source_location.data_index].remove_entity(ChunkEntityLocation {
            chunk_index: source_location.chunk_index,
            entity_index: source_location.entity_index,
        });

        if let Some((moved_entity, moved_location)) = moved {
            self.set_entity_location(
                moved_entity,
                EntityLocation {
                    data_index: source_location.data_index,
                    chunk_index: moved_location.chunk_index,
                    entity_index: moved_location.entity_index,
                },
            );
        }

        true
    }

    pub(crate) fn archetype_epoch(&self) -> usize {
        self.archetype_epoch
    }

    pub fn entity_count(&self) -> usize {
        self.data
            .iter()
            .map(|d| d.chunks.iter().map(|c| c.entity_count).sum::<usize>())
            .sum()
    }

    pub fn archetype_count(&self) -> usize {
        self.data.len()
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.archetype_to_data_index.clear();
        self.transitions.clear();
        self.entities.clear();
        self.free_entities.clear();
        self.archetype_epoch += 1;
    }

    pub fn query<Q>(&self) -> PreparedQuery<Q>
    where
        Q: QuerySpec,
    {
        PreparedQuery::new()
    }

    pub fn query_filtered<Q, Flt>(&self) -> PreparedQuery<Q, Flt>
    where
        Q: QuerySpec,
        Flt: QueryFilter,
    {
        PreparedQuery::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Commands, World};

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
    struct Tick(u64);

    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    struct HugeState([u8; 16 * 1024]);

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Health(f32);

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Damage(f32);

    #[test]
    fn spawn_initializes_components_and_get_reads_them() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 }, Velocity { x: 3.0, y: 4.0 }));

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
    fn spawn_batch_initializes_all_component_columns() {
        let mut world = World::new();
        let expected: Vec<_> = (0..128)
            .map(|i| {
                (
                    Position {
                        x: i as f32,
                        y: i as f32 + 0.5,
                    },
                    Velocity {
                        x: i as f32 * 2.0,
                        y: i as f32 * 2.0 + 1.0,
                    },
                )
            })
            .collect();

        world.spawn_batch(expected.iter().copied());

        let mut actual = Vec::new();
        let mut query = world.query::<(&Position, &Velocity)>();
        query.for_each(&world, |(position, velocity)| {
            actual.push((*position, *velocity));
        });
        actual.sort_by(|(left, _), (right, _)| left.x.total_cmp(&right.x));

        assert_eq!(actual, expected);
    }

    #[test]
    fn get_mut_updates_entity_component() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 },));

        let position = world.get_mut::<Position>(entity).unwrap();
        position.x = 5.0;

        assert_eq!(world.get::<Position>(entity).unwrap().x, 5.0);
    }

    #[test]
    fn despawn_invalidates_entity_and_keeps_other_entities_accessible() {
        let mut world = World::new();
        let first = world.spawn((Position { x: 1.0, y: 2.0 },));
        let second = world.spawn((Position { x: 3.0, y: 4.0 },));

        assert!(world.despawn(first));
        assert!(!world.contains(first));
        assert_eq!(world.get::<Position>(first), None);
        assert_eq!(
            world.get::<Position>(second),
            Some(&Position { x: 3.0, y: 4.0 })
        );
    }

    #[test]
    fn insert_adds_new_component_and_keeps_existing_values() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 },));

        assert!(world.insert(entity, Velocity { x: 3.0, y: 4.0 }));
        assert!(world.has::<Velocity>(entity));
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
    fn remove_moves_entity_to_smaller_archetype() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 }, Velocity { x: 3.0, y: 4.0 }));

        assert!(world.remove::<Velocity>(entity));
        assert!(!world.has::<Velocity>(entity));
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 2.0 })
        );
        assert_eq!(world.get::<Velocity>(entity), None);
    }

    #[test]
    fn resources_can_be_inserted_read_mutated_and_removed() {
        let mut world = World::new();

        assert!(!world.contains_resource::<Tick>());
        assert_eq!(world.insert_resource(Tick(1)), None);
        assert!(world.contains_resource::<Tick>());
        assert_eq!(world.get_resource::<Tick>(), Some(&Tick(1)));

        world.get_resource_mut::<Tick>().unwrap().0 += 1;
        assert_eq!(world.get_resource::<Tick>(), Some(&Tick(2)));
        assert_eq!(world.remove_resource::<Tick>(), Some(Tick(2)));
        assert!(!world.contains_resource::<Tick>());
    }

    #[test]
    fn commands_apply_spawns_components_and_resources() {
        let mut world = World::new();
        let mut commands = Commands::new();

        commands.insert_resource(Tick(10));
        commands.spawn((Position { x: 7.0, y: 8.0 }, Velocity { x: 1.0, y: 2.0 }));
        commands.apply(&mut world);

        assert_eq!(world.get_resource::<Tick>(), Some(&Tick(10)));

        let mut count = 0usize;
        let mut query = world.query::<(&Position, &Velocity)>();
        query.for_each(&world, |(position, velocity)| {
            count += 1;
            assert_eq!(position, &Position { x: 7.0, y: 8.0 });
            assert_eq!(velocity, &Velocity { x: 1.0, y: 2.0 });
        });

        assert_eq!(count, 1);
    }

    #[test]
    fn commands_apply_entity_changes_in_order() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 },));
        let mut commands = Commands::new();

        commands.insert(entity, Velocity { x: 3.0, y: 4.0 });
        commands.remove::<Velocity>(entity);
        commands.despawn(entity);
        commands.apply(&mut world);

        assert!(!world.contains(entity));
    }

    #[test]
    fn commands_coalesce_repeated_component_changes_per_entity() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 },));
        let mut commands = Commands::new();

        commands.insert(entity, Velocity { x: 1.0, y: 1.0 });
        commands.insert(entity, Velocity { x: 5.0, y: 8.0 });
        commands.remove::<Velocity>(entity);
        commands.insert(entity, Velocity { x: 13.0, y: 21.0 });
        commands.apply(&mut world);

        assert_eq!(
            world.get::<Velocity>(entity),
            Some(&Velocity { x: 13.0, y: 21.0 })
        );
    }

    #[test]
    fn commands_ignore_component_changes_after_despawn() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 },));
        let mut commands = Commands::new();

        commands.insert(entity, Velocity { x: 1.0, y: 1.0 });
        commands.despawn(entity);
        commands.insert(entity, Velocity { x: 5.0, y: 8.0 });
        commands.remove::<Position>(entity);
        commands.apply(&mut world);

        assert!(!world.contains(entity));
        assert_eq!(world.get::<Velocity>(entity), None);
        assert_eq!(world.get::<Position>(entity), None);
    }

    #[test]
    fn commands_keep_distinct_generations_with_same_entity_index_separate() {
        let mut world = World::new();
        let stale = world.spawn((Position { x: 1.0, y: 2.0 },));
        assert!(world.despawn(stale));
        let current = world.spawn((Position { x: 3.0, y: 4.0 },));
        assert_eq!(stale.index(), current.index());
        assert_ne!(stale.generation(), current.generation());

        let mut commands = Commands::new();
        commands.insert(stale, Velocity { x: 1.0, y: 1.0 });
        commands.insert(current, Velocity { x: 5.0, y: 8.0 });
        commands.apply(&mut world);

        assert_eq!(world.get::<Velocity>(stale), None);
        assert_eq!(
            world.get::<Velocity>(current),
            Some(&Velocity { x: 5.0, y: 8.0 })
        );
    }

    #[test]
    fn commands_batch_add_component_spans_multiple_chunks() {
        let mut world = World::new();
        let entities: Vec<_> = (0..40)
            .map(|i| {
                world.spawn((
                    HugeState([i as u8; 16 * 1024]),
                    Position {
                        x: i as f32,
                        y: i as f32 + 0.5,
                    },
                ))
            })
            .collect();

        let mut commands = Commands::new();
        for &entity in &entities {
            commands.insert(entity, Velocity { x: 3.0, y: 4.0 });
        }
        commands.apply(&mut world);

        for (i, &entity) in entities.iter().enumerate() {
            assert_eq!(
                world.get::<Position>(entity),
                Some(&Position {
                    x: i as f32,
                    y: i as f32 + 0.5,
                })
            );
            assert_eq!(
                world.get::<Velocity>(entity),
                Some(&Velocity { x: 3.0, y: 4.0 })
            );
        }
    }

    #[test]
    fn commands_batch_remove_component_keeps_moved_survivor_locations_valid() {
        let mut world = World::new();
        let entities: Vec<_> = (0..40)
            .map(|i| {
                world.spawn((
                    HugeState([i as u8; 16 * 1024]),
                    Position {
                        x: i as f32,
                        y: i as f32 + 0.5,
                    },
                    Velocity {
                        x: i as f32 * 10.0,
                        y: i as f32 * 10.0 + 1.0,
                    },
                ))
            })
            .collect();

        let mut commands = Commands::new();
        for &entity in &entities[..24] {
            commands.remove::<Velocity>(entity);
        }
        commands.apply(&mut world);

        for (i, &entity) in entities.iter().enumerate() {
            assert_eq!(
                world.get::<Position>(entity),
                Some(&Position {
                    x: i as f32,
                    y: i as f32 + 0.5,
                })
            );

            if i < 24 {
                assert_eq!(world.get::<Velocity>(entity), None);
            } else {
                assert_eq!(
                    world.get::<Velocity>(entity),
                    Some(&Velocity {
                        x: i as f32 * 10.0,
                        y: i as f32 * 10.0 + 1.0,
                    })
                );
            }
        }
    }

    #[test]
    fn commands_generic_batch_add_component_handles_multiple_targets_from_same_source() {
        let mut world = World::new();
        let entities: Vec<_> = (0..24)
            .map(|i| {
                world.spawn((
                    Position {
                        x: i as f32,
                        y: i as f32 + 0.5,
                    },
                    Velocity {
                        x: i as f32 * 2.0,
                        y: i as f32 * 2.0 + 1.0,
                    },
                ))
            })
            .collect();

        let mut commands = Commands::new();
        for (i, &entity) in entities.iter().enumerate() {
            if i % 2 == 0 {
                commands.insert(entity, Health(100.0 + i as f32));
            } else {
                commands.insert(entity, Damage(i as f32));
            }
        }
        commands.apply(&mut world);

        for (i, &entity) in entities.iter().enumerate() {
            assert_eq!(
                world.get::<Position>(entity),
                Some(&Position {
                    x: i as f32,
                    y: i as f32 + 0.5,
                })
            );
            assert_eq!(
                world.get::<Velocity>(entity),
                Some(&Velocity {
                    x: i as f32 * 2.0,
                    y: i as f32 * 2.0 + 1.0,
                })
            );

            if i % 2 == 0 {
                assert_eq!(world.get::<Health>(entity), Some(&Health(100.0 + i as f32)));
                assert_eq!(world.get::<Damage>(entity), None);
            } else {
                assert_eq!(world.get::<Health>(entity), None);
                assert_eq!(world.get::<Damage>(entity), Some(&Damage(i as f32)));
            }
        }
    }

    #[test]
    fn entity_count_tracks_live_entities() {
        let mut world = World::new();
        assert_eq!(world.entity_count(), 0);

        let a = world.spawn((Position { x: 0.0, y: 0.0 },));
        let b = world.spawn((Position { x: 1.0, y: 1.0 },));
        assert_eq!(world.entity_count(), 2);

        world.despawn(a);
        assert_eq!(world.entity_count(), 1);

        world.despawn(b);
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn archetype_count_grows_with_distinct_shapes() {
        let mut world = World::new();
        assert_eq!(world.archetype_count(), 0);

        world.spawn((Position { x: 0.0, y: 0.0 },));
        assert_eq!(world.archetype_count(), 1);

        world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 }));
        assert_eq!(world.archetype_count(), 2);

        // same shape reuses existing archetype
        world.spawn((Position { x: 2.0, y: 2.0 },));
        assert_eq!(world.archetype_count(), 2);
    }

    #[test]
    fn clear_removes_all_entities_but_keeps_resources() {
        let mut world = World::new();
        world.insert_resource(Tick(42));
        let entity = world.spawn((Position { x: 1.0, y: 2.0 },));

        world.clear();

        assert_eq!(world.entity_count(), 0);
        assert_eq!(world.archetype_count(), 0);
        assert!(!world.contains(entity));
        assert_eq!(world.get_resource::<Tick>(), Some(&Tick(42)));
    }
    #[test]
    fn commands_flush_in_first_seen_entity_order() {
        // Entity A is seen first; even though a later command targets A again,
        // all of A's coalesced commands should execute before B's.
        let mut world = World::new();
        let a = world.spawn((Position { x: 1.0, y: 2.0 },));
        let b = world.spawn((Position { x: 3.0, y: 4.0 },));

        let mut cmds = Commands::new();
        cmds.insert(a, Health(10.0));     // A first seen
        cmds.insert(b, Health(20.0));     // B first seen
        cmds.remove::<Health>(a);         // A again — coalesces with first A entry
        cmds.apply(&mut world);

        // A: insert Health then remove Health → net result: no Health
        assert_eq!(world.get::<Health>(a), None);
        // B: insert Health → Health present
        assert_eq!(world.get::<Health>(b), Some(&Health(20.0)));
    }
}
