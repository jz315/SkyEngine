use super::commands::InsertValue;
use super::resource::Resources;
use super::*;
use crate::ecs::entity::{EntityLocation, EntityRecord};
use crate::ecs::system::{GroupBuilder, Schedule, TickPolicy};
use crate::ecs::time::Time;
use crate::ecs::{component_type, ComponentType};
use crate::plugin::{Plugin, PluginError, PluginRegistry, PluginResult};
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

/// The central container for all ECS data.
///
/// A `World` owns every entity, component, and resource.  It provides methods
/// for spawning and despawning entities, reading and writing components,
/// scheduling systems, and running queries.
///
/// # Drop Semantics
///
/// When a `World` is dropped, all component destructors are called
/// automatically.  Non-`Copy` components (e.g. `String`, `Vec<T>`) are
/// handled correctly.
///
/// # Examples
///
/// ```
/// use sky_ecs::World;
///
/// #[derive(Clone, Copy)]
/// struct Position { x: f32, y: f32 }
///
/// let mut world = World::new();
/// let entity = world.spawn((Position { x: 0.0, y: 0.0 },));
/// assert_eq!(world.get::<Position>(entity).unwrap().x, 0.0);
/// ```
pub struct World {
    pub time: Time,
    pub(crate) data: Vec<Data>,
    archetype_epoch: usize,
    archetype_to_data_index: FxHashMap<Archetype, usize>,
    transitions: FxHashMap<TransitionKey, Box<TransitionPlan>>,
    entities: Vec<EntityRecord>,
    free_entities: Vec<u32>,
    resources: Resources,
    plugins: PluginRegistry,
    schedule: Option<Schedule>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Creates an empty world with no entities, resources, or systems.
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
            plugins: PluginRegistry::default(),
            schedule: Some(Schedule::default()),
        }
    }

    /// Install an engine module plugin into this world.
    ///
    /// Plugin constructors are the configuration surface; installing a plugin
    /// records only the fact that the plugin type is present.  The app runner
    /// discovers capabilities from the resources/plugins that were installed
    /// rather than enforcing a fixed required set.
    pub fn install<P>(&mut self, plugin: P) -> PluginResult
    where
        P: Plugin + 'static,
    {
        let name = plugin.name();
        if let Some(existing) = self.plugins.get::<P>() {
            return Err(PluginError::new(
                name,
                format!("{existing} is already installed"),
            ));
        }
        plugin.install(self)?;
        if let Err(existing) = self.plugins.insert::<P>(name) {
            return Err(PluginError::new(
                name,
                format!("{existing} was installed during plugin setup"),
            ));
        }
        Ok(())
    }

    /// Returns `true` if plugin type `P` was installed through [`World::install`].
    #[inline]
    pub fn has_plugin<P: 'static>(&self) -> bool {
        self.plugins.contains::<P>()
    }

    /// Require plugin type `P` to have been installed.
    ///
    /// Plugins should use this for local hard dependencies.  The app runner
    /// does not use it to impose a universal set of required capabilities.
    pub fn require_plugin<P: 'static>(&self, plugin: &'static str) -> PluginResult {
        if self.has_plugin::<P>() {
            Ok(())
        } else {
            Err(PluginError::new(
                plugin,
                format!("requires {}", std::any::type_name::<P>()),
            ))
        }
    }

    // -----------------------------------------------------------------------
    // Schedule API
    // -----------------------------------------------------------------------

    /// Returns a `GroupBuilder` for the named system group.
    ///
    /// Groups execute in creation order during [`tick`](Self::tick).
    /// If the group already exists, the builder appends to it.
    pub fn group(&mut self, name: &str) -> GroupBuilder<'_> {
        let schedule = self
            .schedule
            .as_mut()
            .expect("cannot modify schedule during tick");
        let index = schedule.find_or_create_group(name);
        GroupBuilder::new(schedule, index)
    }

    /// Advances the world by one frame using wall-clock time.
    ///
    /// On the first call, delta is 0.  Subsequent calls measure elapsed time
    /// since the previous tick.  The measured delta is stored in
    /// [`Time::raw_delta`], while [`Time::frame_delta`] is affected by
    /// [`Time::time_scale`].
    ///
    /// # Panics
    ///
    /// Panics if called recursively (e.g. from within a running system).
    pub fn tick(&mut self) {
        let mut schedule = self.schedule.take().expect("cannot call tick recursively");

        let now = std::time::Instant::now();
        let raw_delta = match schedule.last_tick {
            Some(last) => (now - last).as_secs_f32(),
            None => 0.0,
        };
        schedule.last_tick = Some(now);

        self.run_schedule(&mut schedule, raw_delta, raw_delta);

        self.schedule = Some(schedule);
    }

    /// Advances the world by the given delta (in seconds).
    ///
    /// Useful for deterministic tests and fixed-step simulations.  The given
    /// delta is treated as both raw and clamped frame time.
    ///
    /// # Panics
    ///
    /// Panics if called recursively.
    pub fn tick_with_delta(&mut self, delta: f32) {
        self.tick_with_frame_delta(delta, delta);
    }

    /// Advances the world by a clamped frame delta and the raw real delta.
    ///
    /// App runners should use this when they clamp large frame deltas before
    /// simulation but still want [`Time::raw_delta`] and
    /// [`Time::raw_elapsed`] to reflect real elapsed time.
    ///
    /// # Panics
    ///
    /// Panics if called recursively.
    pub fn tick_with_frame_delta(&mut self, frame_delta: f32, raw_delta: f32) {
        let mut schedule = self.schedule.take().expect("cannot call tick recursively");

        self.run_schedule(&mut schedule, frame_delta, raw_delta);

        self.schedule = Some(schedule);
    }

    fn run_schedule(&mut self, schedule: &mut Schedule, frame_delta: f32, raw_delta: f32) {
        let raw_delta = raw_delta.max(0.0);
        let frame_delta = frame_delta.max(0.0);
        let scaled_delta = frame_delta * self.time.time_scale;
        self.time.raw_delta = raw_delta;
        self.time.frame_delta = scaled_delta;
        self.time.delta = scaled_delta;
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

        self.time.delta = self.time.frame_delta;
        self.time.elapsed += scaled_delta;
        self.time.raw_elapsed += raw_delta;
    }

    /// Tears down all initialised systems in reverse order.
    ///
    /// Call this before dropping the world if your systems need a clean
    /// shutdown (e.g. flushing buffers, releasing external resources).
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

    /// Adds an entity in `archetype` without initializing component columns.
    ///
    /// # Safety
    ///
    /// The caller must initialize every component column before the entity can
    /// be observed, migrated, removed, or dropped.
    pub(crate) unsafe fn add_entity(&mut self, archetype: Archetype) -> EntityId {
        let entity = self.allocate_entity();
        let data_index = self.ensure_data_index(archetype);
        let location = unsafe { self.data[data_index].add_entity(entity) };
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

    /// Spawns a new entity with the given component bundle.
    ///
    /// Returns the [`EntityId`] of the newly created entity.  The bundle
    /// type is typically a tuple of components:
    ///
    /// ```
    /// # use sky_ecs::World;
    /// # #[derive(Clone, Copy)] struct Pos { x: f32, y: f32 }
    /// # #[derive(Clone, Copy)] struct Vel { x: f32, y: f32 }
    /// # let mut world = World::new();
    /// let entity = world.spawn((Pos { x: 0.0, y: 0.0 }, Vel { x: 1.0, y: 2.0 }));
    /// ```
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityId {
        let (archetype, columns) = B::cached_meta();
        let entity = self.allocate_entity();
        let data_index = self.ensure_data_index(archetype);
        let location = unsafe { self.data[data_index].add_entity(entity) };
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

    /// Spawns multiple entities from an iterator of bundles.
    ///
    /// More efficient than calling [`spawn`](Self::spawn) in a loop because
    /// the archetype lookup and entity record allocation are amortised.
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
            let location = unsafe { self.data[data_index].add_entity(entity) };
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

    pub(crate) fn spawn_dynamic_values(
        &mut self,
        values: &mut [crate::ecs::dynamic::ErasedComponentValue],
    ) -> Result<EntityId, crate::ecs::dynamic::DynamicSpawnError> {
        for (index, value) in values.iter().enumerate() {
            for other in &values[(index + 1)..] {
                if value.component.id() == other.component.id() {
                    return Err(crate::ecs::dynamic::DynamicSpawnError::DuplicateComponent {
                        component: value.component,
                    });
                }
            }
        }

        let mut builder = create_archetype();
        for value in values.iter() {
            builder = builder.add_component(value.component);
        }
        let archetype = builder.build();

        let entity = unsafe { self.add_entity(archetype) };
        let location = self
            .entity_location(entity)
            .expect("fresh dynamic entity must have a location");
        let chunk = &mut self.data[location.data_index].chunks[location.chunk_index];

        for value in values {
            let component_index = archetype
                .query_component_index(&value.component)
                .expect("dynamic component must exist in target archetype");
            let ptr = unsafe {
                chunk
                    .column_ptr(component_index)
                    .add(location.entity_index * value.component.size)
            };
            value.value.write(ptr);
        }

        Ok(entity)
    }

    /// Returns `true` if `entity` is alive in this world.
    pub fn contains(&self, entity: EntityId) -> bool {
        self.entity_location(entity).is_some()
    }

    /// Inserts a singleton resource, returning the previous value if one existed.
    pub fn insert_resource<R: 'static>(&mut self, resource: R) -> Option<R> {
        self.resources.insert(resource)
    }

    /// Returns an immutable reference to resource `R`, or `None`.
    pub fn get_resource<R: 'static>(&self) -> Option<&R> {
        self.resources.get::<R>()
    }

    /// Returns a mutable reference to resource `R`, or `None`.
    pub fn get_resource_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.resources.get_mut::<R>()
    }

    /// Returns `true` if the world contains resource `R`.
    pub fn contains_resource<R: 'static>(&self) -> bool {
        self.resources.contains::<R>()
    }

    /// Removes and returns resource `R`, or `None` if not present.
    pub fn remove_resource<R: 'static>(&mut self) -> Option<R> {
        self.resources.remove::<R>()
    }

    /// Returns `true` if `entity` is alive and has component `T`.
    #[inline(always)]
    pub fn has<T: 'static>(&self, entity: EntityId) -> bool {
        let Some(location) = self.entity_location(entity) else {
            return false;
        };

        self.data[location.data_index]
            .archetype
            .has_component(&component_type::<T>())
    }

    /// Destroys an entity and drops all its components.
    ///
    /// Returns `true` if the entity existed and was removed,
    /// or `false` if the entity ID was stale or invalid.
    pub fn despawn(&mut self, entity: EntityId) -> bool {
        let Some(location) = self.entity_location(entity) else {
            return false;
        };

        // Safety: location is valid and the entity is about to be destroyed.
        // We must drop its components before the swap-remove overwrites the slot.
        unsafe {
            self.data[location.data_index].chunks[location.chunk_index]
                .drop_entity_components(location.entity_index);
        }

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

    /// Returns a shared reference to component `T` on `entity`.
    ///
    /// Returns `None` if the entity is dead or does not have `T`.
    #[inline(always)]
    pub fn get<T: 'static>(&self, entity: EntityId) -> Option<&T> {
        let location = self.entity_location(entity)?;
        let data = &self.data[location.data_index];
        let chunk = &data.chunks[location.chunk_index];
        let component_index = chunk
            .archetype
            .query_component_index(&component_type::<T>())?;

        Some(unsafe {
            let ptr = chunk
                .column_ptr(component_index)
                .add(location.entity_index * std::mem::size_of::<T>());
            &*(ptr as *const T)
        })
    }

    /// Returns an exclusive reference to component `T` on `entity`.
    ///
    /// Returns `None` if the entity is dead or does not have `T`.
    #[inline(always)]
    pub fn get_mut<T: 'static>(&mut self, entity: EntityId) -> Option<&mut T> {
        let location = self.entity_location(entity)?;
        let data = &mut self.data[location.data_index];
        let chunk = &mut data.chunks[location.chunk_index];
        let component_index = chunk
            .archetype
            .query_component_index(&component_type::<T>())?;

        Some(unsafe {
            let ptr = chunk
                .column_ptr(component_index)
                .add(location.entity_index * std::mem::size_of::<T>());
            &mut *(ptr as *mut T)
        })
    }

    fn archetype_with_component(base: Archetype, component: ComponentType) -> Archetype {
        let mut builder = create_archetype();
        for existing in &base.components {
            builder = builder.add_component(*existing);
        }
        builder.add_component(component).build()
    }

    fn archetype_without_component(base: Archetype, component: ComponentType) -> Option<Archetype> {
        if !base.has_component(&component) {
            return None;
        }

        let mut builder = create_archetype();
        for existing in &base.components {
            if existing.id() != component.id() {
                builder = builder.add_component(*existing);
            }
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
            if skip_component_ids.iter().any(|id| *id == component.id()) {
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
        component: ComponentType,
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

    /// Adds or overwrites component `T` on `entity`.
    ///
    /// If the entity already has `T`, the old value is dropped and replaced
    /// in-place (no archetype migration).  If the entity does not have `T`,
    /// it is migrated to a new archetype that includes `T`.
    ///
    /// Returns `false` if the entity does not exist.
    pub fn insert<T: 'static>(&mut self, entity: EntityId, component: T) -> bool {
        let Some(source_location) = self.entity_location(entity) else {
            return false;
        };

        let component_ty = component_type::<T>();
        let source_archetype = self.data[source_location.data_index].archetype;

        // Overwrite path: entity already has this component.
        if source_archetype.has_component(&component_ty) {
            let component_index = source_archetype
                .query_component_index(&component_ty)
                .unwrap();
            let chunk =
                &mut self.data[source_location.data_index].chunks[source_location.chunk_index];
            unsafe {
                let ptr = chunk
                    .column_ptr(component_index)
                    .add(source_location.entity_index * std::mem::size_of::<T>())
                    as *mut T;
                // Safety: ptr points to a valid, initialised T.
                // Drop the old value, then write the new one.
                std::ptr::drop_in_place(ptr);
                std::ptr::write(ptr, component);
            }
            return true;
        }

        let plan = self
            .transition_plan(source_location.data_index, component_ty, true)
            .expect("adding a missing component must produce a transition plan");
        let plan = unsafe { plan.as_ref() };
        let target_data_index = plan
            .target_data_index
            .get()
            .expect("transition plans must cache the target data index");
        let target_location = unsafe { self.data[target_data_index].add_entity(entity) };

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

            // Bitwise-copy existing columns to the new archetype chunk.
            // This is a semantic move: the source slot should NOT be dropped
            // for these columns.
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
                std::ptr::write(ptr as *mut T, component);
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

        // Source entity data was bitwise-moved to target; the swap-remove
        // here only rearranges the source chunk — no drops needed.
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

    /// Removes component `T` from `entity`, dropping it.
    ///
    /// The entity is migrated to a smaller archetype.  Returns `false` if
    /// the entity does not exist or does not have `T`.
    pub fn remove<T: 'static>(&mut self, entity: EntityId) -> bool {
        let Some(source_location) = self.entity_location(entity) else {
            return false;
        };

        let component_ty = component_type::<T>();
        let Some(plan) = self.transition_plan(source_location.data_index, component_ty, false)
        else {
            return false;
        };
        let plan = unsafe { plan.as_ref() };

        let target_data_index = plan
            .target_data_index
            .get()
            .expect("transition plans must cache the target data index");
        let target_location = unsafe { self.data[target_data_index].add_entity(entity) };

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

            // Bitwise-copy kept columns to target (semantic move).
            Self::copy_components_with_spans(
                source_chunk,
                source_location.entity_index,
                target_chunk,
                target_location.entity_index,
                &plan.copy_spans,
            );

            // Drop the removed component column from the source entity.
            // Safety: source_location is valid and this column is being
            // discarded (not copied to the target archetype).
            if let Some(removed_component_index) =
                source_chunk.archetype.query_component_index(&component_ty)
            {
                unsafe {
                    source_chunk.drop_single_component(
                        removed_component_index,
                        source_location.entity_index,
                    );
                }
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

        // Source entity data was bitwise-moved (kept columns) and dropped
        // (removed column); the swap-remove only rearranges the chunk.
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
        component: ComponentType,
        value: &mut InsertValue,
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
            // Safety: drop the old value before overwriting.
            if let Some(drop_fn) = component.drop_fn() {
                unsafe {
                    drop_fn(ptr);
                }
            }
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
        let target_location = unsafe { self.data[target_data_index].add_entity(entity) };

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

    pub(crate) fn remove_dynamic(&mut self, entity: EntityId, component: ComponentType) -> bool {
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
        let target_location = unsafe { self.data[target_data_index].add_entity(entity) };

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

            // Drop the removed component from the source entity.
            if let Some(removed_component_index) =
                source_chunk.archetype.query_component_index(&component)
            {
                unsafe {
                    source_chunk.drop_single_component(
                        removed_component_index,
                        source_location.entity_index,
                    );
                }
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

    pub(crate) fn archetype_epoch(&self) -> usize {
        self.archetype_epoch
    }

    /// Returns the total number of live entities across all archetypes.
    pub fn entity_count(&self) -> usize {
        self.data
            .iter()
            .map(|d| d.chunks.iter().map(|c| c.entity_count).sum::<usize>())
            .sum()
    }

    /// Returns the number of distinct archetypes currently stored.
    pub fn archetype_count(&self) -> usize {
        self.data.len()
    }

    /// Removes all entities and their components, but keeps resources.
    ///
    /// Component destructors are called for every live entity.
    pub fn clear(&mut self) {
        self.data.clear();
        self.archetype_to_data_index.clear();
        self.transitions.clear();
        self.free_entities.clear();
        for (index, record) in self.entities.iter_mut().enumerate() {
            if record.location.is_some() {
                record.generation = record.generation.wrapping_add(1);
                record.location = None;
            }
            self.free_entities.push(index as u32);
        }
        self.archetype_epoch += 1;
    }

    /// Creates a typed, cached query.
    ///
    /// The query parameter `Q` is usually a reference (`&T`), a mutable
    /// reference (`&mut T`), an optional (`Option<&T>`), or a tuple of
    /// those.  The returned [`PreparedQuery`] caches matched archetypes
    /// and refreshes automatically when the world's archetype set changes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sky_ecs::World;
    /// # #[derive(Clone, Copy)] struct Pos { x: f32, y: f32 }
    /// # #[derive(Clone, Copy)] struct Vel { x: f32, y: f32 }
    /// # let mut world = World::new();
    /// # world.spawn((Pos { x: 0.0, y: 0.0 }, Vel { x: 1.0, y: 0.0 }));
    /// let mut query = world.query::<(&mut Pos, &Vel)>();
    /// query.for_each(&mut world, |(pos, vel)| {
    ///     pos.x += vel.x;
    /// });
    /// ```
    pub fn query<Q>(&self) -> PreparedQuery<Q>
    where
        Q: QuerySpec,
    {
        PreparedQuery::new()
    }

    /// Creates a typed, cached query with an archetype filter.
    ///
    /// Filters narrow which archetypes are matched.  Use [`With<T>`] to
    /// require that `T` is present, or [`Without<T>`] to exclude it.
    /// Filter tuples combine with AND semantics.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sky_ecs::{World, With};
    /// # #[derive(Clone, Copy)] struct Pos { x: f32, y: f32 }
    /// # #[derive(Clone, Copy)] struct Enemy;
    /// # let mut world = World::new();
    /// let mut enemies = world.query_filtered::<&Pos, With<Enemy>>();
    /// ```
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

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Marker;

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
    fn spawn_zero_sized_marker_component() {
        let mut world = World::new();
        let entity = world.spawn((Marker,));

        assert!(world.contains(entity));
        assert_eq!(world.entity_count(), 1);
        assert_eq!(world.get::<Marker>(entity), Some(&Marker));
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
        query.for_each(&mut world, |(position, velocity)| {
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
    fn remove_can_leave_entity_with_only_zero_sized_component() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 }, Marker));

        assert!(world.remove::<Position>(entity));
        assert!(world.contains(entity));
        assert!(!world.has::<Position>(entity));
        assert!(world.has::<Marker>(entity));
        assert_eq!(world.get::<Marker>(entity), Some(&Marker));
    }

    #[test]
    fn remove_last_component_leaves_empty_entity_alive() {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 },));

        assert!(world.remove::<Position>(entity));
        assert!(world.contains(entity));
        assert!(!world.has::<Position>(entity));
        assert_eq!(world.entity_count(), 1);
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
        query.for_each(&mut world, |(position, velocity)| {
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
    fn clear_invalidates_entity_ids_before_reuse() {
        let mut world = World::new();
        let stale = world.spawn((Position { x: 1.0, y: 2.0 },));

        world.clear();
        let current = world.spawn((Position { x: 3.0, y: 4.0 },));

        assert_eq!(stale.index(), current.index());
        assert_ne!(stale.generation(), current.generation());
        assert!(!world.contains(stale));
        assert_eq!(world.get::<Position>(stale), None);
        assert_eq!(
            world.get::<Position>(current),
            Some(&Position { x: 3.0, y: 4.0 })
        );
    }

    #[test]
    fn commands_flush_in_first_seen_entity_order() {
        // Entity A is seen first; even though a later command targets A again,
        // all of A's coalesced commands should execute before B's.
        let mut world = World::new();
        let a = world.spawn((Position { x: 1.0, y: 2.0 },));
        let b = world.spawn((Position { x: 3.0, y: 4.0 },));

        let mut cmds = Commands::new();
        cmds.insert(a, Health(10.0)); // A first seen
        cmds.insert(b, Health(20.0)); // B first seen
        cmds.remove::<Health>(a); // A again — coalesces with first A entry
        cmds.apply(&mut world);

        // A: insert Health then remove Health → net result: no Health
        assert_eq!(world.get::<Health>(a), None);
        // B: insert Health → Health present
        assert_eq!(world.get::<Health>(b), Some(&Health(20.0)));
    }

    // =======================================================================
    // Drop semantics tests
    // =======================================================================

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A component whose Drop increments a shared counter.
    #[derive(Clone)]
    struct Droppable {
        counter: Arc<AtomicUsize>,
    }

    impl Droppable {
        fn new(counter: &Arc<AtomicUsize>) -> Self {
            Self {
                counter: counter.clone(),
            }
        }
    }

    impl Drop for Droppable {
        fn drop(&mut self) {
            self.counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn despawn_calls_drop_on_components() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        let entity = world.spawn((Droppable::new(&counter),));
        assert_eq!(counter.load(Ordering::Relaxed), 0);

        world.despawn(entity);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    /// A second droppable type to test multi-component bundles.
    #[derive(Clone)]
    struct DroppableB {
        counter: Arc<AtomicUsize>,
    }

    impl DroppableB {
        fn new(counter: &Arc<AtomicUsize>) -> Self {
            Self {
                counter: counter.clone(),
            }
        }
    }

    impl Drop for DroppableB {
        fn drop(&mut self) {
            self.counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn despawn_calls_drop_on_multiple_components() {
        let counter_a = Arc::new(AtomicUsize::new(0));
        let counter_b = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        let entity = world.spawn((Droppable::new(&counter_a), DroppableB::new(&counter_b)));

        world.despawn(entity);
        assert_eq!(counter_a.load(Ordering::Relaxed), 1);
        assert_eq!(counter_b.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn remove_component_calls_drop_on_removed_column() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        let entity = world.spawn((Position { x: 0.0, y: 0.0 }, Droppable::new(&counter)));

        world.remove::<Droppable>(entity);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        // Entity should still be alive with its Position.
        assert!(world.contains(entity));
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 0.0, y: 0.0 })
        );
    }

    #[test]
    fn insert_overwrite_calls_drop_on_old_value() {
        let counter_old = Arc::new(AtomicUsize::new(0));
        let counter_new = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        let entity = world.spawn((Droppable::new(&counter_old),));

        world.insert(entity, Droppable::new(&counter_new));
        // Old value should have been dropped.
        assert_eq!(counter_old.load(Ordering::Relaxed), 1);
        // New value should not have been dropped yet.
        assert_eq!(counter_new.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn clear_calls_drop_on_all_entity_components() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        for _ in 0..10 {
            world.spawn((Droppable::new(&counter),));
        }
        assert_eq!(counter.load(Ordering::Relaxed), 0);

        world.clear();
        assert_eq!(counter.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn world_drop_calls_drop_on_all_entity_components() {
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let mut world = World::new();
            for _ in 0..5 {
                world.spawn((Droppable::new(&counter),));
            }
            assert_eq!(counter.load(Ordering::Relaxed), 0);
        }
        // World was dropped — all components should be dropped.
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn spawn_with_non_copy_component_and_read_back() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        let entity = world.spawn((Droppable::new(&counter),));

        // We can get an immutable reference to the non-Copy component.
        let droppable = world.get::<Droppable>(entity).unwrap();
        assert!(Arc::ptr_eq(&droppable.counter, &counter));
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn insert_new_component_does_not_drop_existing_components() {
        let counter_existing = Arc::new(AtomicUsize::new(0));
        let _counter_new = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        let entity = world.spawn((Droppable::new(&counter_existing),));

        // Insert a new component (causing archetype migration).
        world.insert(entity, Position { x: 1.0, y: 2.0 });

        // The existing Droppable should NOT have been dropped.
        assert_eq!(counter_existing.load(Ordering::Relaxed), 0);
        // Both components should be accessible.
        assert!(world.get::<Droppable>(entity).is_some());
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 2.0 })
        );
    }

    #[test]
    fn commands_insert_non_copy_then_apply() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut world = World::new();
        let entity = world.spawn((Position { x: 0.0, y: 0.0 },));

        let mut cmds = Commands::new();
        cmds.insert(entity, Droppable::new(&counter));
        cmds.apply(&mut world);

        // Component should be on the entity now, not dropped.
        assert_eq!(counter.load(Ordering::Relaxed), 0);
        assert!(world.get::<Droppable>(entity).is_some());

        // Despawn should call drop.
        world.despawn(entity);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn commands_insert_value_dropped_when_not_consumed() {
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let mut cmds = Commands::new();
            let entity = super::EntityId::new(9999, 0); // non-existent
            cmds.insert(entity, Droppable::new(&counter));
            // Drop cmds without applying — the InsertValue should drop its payload.
        }
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[repr(align(128))]
    struct HighAlignDroppable {
        drop_count: Arc<AtomicUsize>,
        misaligned_count: Arc<AtomicUsize>,
    }

    impl Drop for HighAlignDroppable {
        fn drop(&mut self) {
            let address = self as *const Self as usize;
            if address % std::mem::align_of::<Self>() != 0 {
                self.misaligned_count.fetch_add(1, Ordering::Relaxed);
            }
            self.drop_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn commands_insert_value_drop_preserves_component_alignment() {
        let drop_count = Arc::new(AtomicUsize::new(0));
        let misaligned_count = Arc::new(AtomicUsize::new(0));
        {
            let mut cmds = Commands::new();
            cmds.insert(
                super::EntityId::new(9999, 0),
                HighAlignDroppable {
                    drop_count: drop_count.clone(),
                    misaligned_count: misaligned_count.clone(),
                },
            );
        }

        assert_eq!(drop_count.load(Ordering::Relaxed), 1);
        assert_eq!(misaligned_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn copy_types_unaffected_by_drop_machinery() {
        // Verify Copy types still work exactly as before.
        let mut world = World::new();
        let entity = world.spawn((Position { x: 1.0, y: 2.0 }, Velocity { x: 3.0, y: 4.0 }));

        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 2.0 })
        );
        world.insert(entity, Position { x: 5.0, y: 6.0 });
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 5.0, y: 6.0 })
        );
        world.remove::<Velocity>(entity);
        assert_eq!(world.get::<Velocity>(entity), None);
        world.despawn(entity);
        assert!(!world.contains(entity));
    }
}
