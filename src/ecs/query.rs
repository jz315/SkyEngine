use super::{Chunk, World};
use crate::reflect::{register_rust_type, Type};
use core::{marker::PhantomData, mem, slice};
use smallvec::SmallVec;

const UNROLL: usize = 8;
const INLINE_QUERY_COMPONENTS: usize = 8;

#[derive(Clone, Copy)]
pub struct QueryComponent {
    ty: Type,
    mutable: bool,
}

impl QueryComponent {
    fn new(ty: Type, mutable: bool) -> Self {
        Self { ty, mutable }
    }
}

pub struct QueryDescriptor {
    components: SmallVec<[QueryComponent; INLINE_QUERY_COMPONENTS]>,
}

impl QueryDescriptor {
    fn new(components: SmallVec<[QueryComponent; INLINE_QUERY_COMPONENTS]>) -> Self {
        for (index, component) in components.iter().enumerate() {
            for other in &components[(index + 1)..] {
                if component.ty.id() == other.ty.id() {
                    let access_mode = if component.mutable || other.mutable {
                        "mutable"
                    } else {
                        "shared"
                    };
                    panic!(
                        "duplicate component type `{}` is not supported in {} queries",
                        component.ty.name, access_mode
                    );
                }
            }
        }

        Self { components }
    }

    fn from_dynamic_types(types: &[Type]) -> Self {
        let mut components = SmallVec::with_capacity(types.len());
        for ty in types {
            components.push(QueryComponent::new(*ty, false));
        }
        Self::new(components)
    }

    fn len(&self) -> usize {
        self.components.len()
    }
}

#[derive(Clone)]
struct CachedArchetype {
    data_index: usize,
    component_indices: SmallVec<[u8; INLINE_QUERY_COMPONENTS]>,
}

#[derive(Default)]
struct PreparedCache {
    cached_epoch: Option<usize>,
    archetypes: Vec<CachedArchetype>,
}

impl PreparedCache {
    #[inline(always)]
    fn prepare(&mut self, world: &World, descriptor: &QueryDescriptor) {
        let current_epoch = world.archetype_epoch();
        if self.cached_epoch == Some(current_epoch) {
            return;
        }

        self.archetypes.clear();

        for (data_index, data) in world.data.iter().enumerate() {
            let archetype = data.archetype;
            let mut component_indices =
                SmallVec::<[u8; INLINE_QUERY_COMPONENTS]>::with_capacity(descriptor.len());

            let mut matches = true;
            for component in &descriptor.components {
                if let Some(index) = archetype.query_component_index(&component.ty) {
                    debug_assert!(index <= u8::MAX as usize);
                    component_indices.push(index as u8);
                } else {
                    matches = false;
                    break;
                }
            }

            if matches {
                self.archetypes.push(CachedArchetype {
                    data_index,
                    component_indices,
                });
            }
        }

        self.cached_epoch = Some(current_epoch);
    }

    #[inline(always)]
    fn visit_chunks<'w, F>(&self, world: &'w World, mut f: F)
    where
        F: FnMut(&CachedArchetype, &'w Chunk),
    {
        for cached in &self.archetypes {
            let data = &world.data[cached.data_index];

            for chunk in &data.chunks {
                debug_assert!(chunk.entity_count != 0);
                f(cached, chunk);
            }
        }
    }

    fn cached_archetype_count(&self) -> usize {
        self.archetypes.len()
    }
}

pub struct Query {
    pub types: Vec<Type>,
}

impl Query {
    pub fn new(types: Vec<Type>) -> Self {
        Self { types }
    }
}

pub struct QueryIter<'a> {
    world: &'a World,
    pub query: &'a Query,
    prepared: PreparedCache,
}

impl<'a> QueryIter<'a> {
    pub fn new(world: &'a World, query: &'a Query) -> Self {
        let descriptor = QueryDescriptor::from_dynamic_types(&query.types);
        let mut prepared = PreparedCache::default();
        prepared.prepare(world, &descriptor);

        Self {
            world,
            query,
            prepared,
        }
    }

    #[inline(always)]
    fn debug_assert_query_type<T>(ty: &Type) {
        debug_assert_eq!(ty.size, mem::size_of::<T>());
        debug_assert_eq!(ty.align, mem::align_of::<T>());
    }

    #[inline(always)]
    pub fn for_each2<F>(&mut self, mut f: F)
    where
        F: FnMut(*mut u8, *mut u8),
    {
        self.prepared.visit_chunks(self.world, |cached, chunk| {
            debug_assert!(cached.component_indices.len() >= 2);

            let component1 = cached.component_indices[0] as usize;
            let component2 = cached.component_indices[1] as usize;
            let stride1 = chunk.archetype.components[component1].size;
            let stride2 = chunk.archetype.components[component2].size;
            let mut ptr1 = chunk.column_ptr(component1);
            let mut ptr2 = chunk.column_ptr(component2);

            let until = chunk.entity_count - (chunk.entity_count % UNROLL);
            let mut i = 0;

            unsafe {
                macro_rules! process_entity {
                    ($offset:expr) => {
                        f(ptr1.add($offset * stride1), ptr2.add($offset * stride2));
                    };
                }

                while i < until {
                    process_entity!(0);
                    process_entity!(1);
                    process_entity!(2);
                    process_entity!(3);
                    process_entity!(4);
                    process_entity!(5);
                    process_entity!(6);
                    process_entity!(7);

                    ptr1 = ptr1.add(UNROLL * stride1);
                    ptr2 = ptr2.add(UNROLL * stride2);
                    i += UNROLL;
                }

                while i < chunk.entity_count {
                    process_entity!(0);
                    ptr1 = ptr1.add(stride1);
                    ptr2 = ptr2.add(stride2);
                    i += 1;
                }
            }
        });
    }

    #[inline(always)]
    pub fn for_each_chunk2<A, B, F>(&mut self, mut f: F)
    where
        F: FnMut(&mut [A], &[B]),
    {
        debug_assert!(self.query.types.len() >= 2);
        Self::debug_assert_query_type::<A>(&self.query.types[0]);
        Self::debug_assert_query_type::<B>(&self.query.types[1]);

        self.prepared.visit_chunks(self.world, |cached, chunk| {
            debug_assert!(cached.component_indices.len() >= 2);

            let component1 = cached.component_indices[0] as usize;
            let component2 = cached.component_indices[1] as usize;

            debug_assert_ne!(component1, component2);

            unsafe {
                let components1 =
                    slice::from_raw_parts_mut(chunk.column_ptr(component1) as *mut A, chunk.entity_count);
                let components2 =
                    slice::from_raw_parts(chunk.column_ptr(component2) as *const B, chunk.entity_count);
                f(components1, components2);
            }
        });
    }

    #[inline(always)]
    pub fn for_each<F>(&mut self, mut f: F)
    where
        F: FnMut(*mut u8, *mut u8, *mut u8, *mut u8),
    {
        self.prepared.visit_chunks(self.world, |cached, chunk| {
            debug_assert!(cached.component_indices.len() >= 4);

            let component1 = cached.component_indices[0] as usize;
            let component2 = cached.component_indices[1] as usize;
            let component3 = cached.component_indices[2] as usize;
            let component4 = cached.component_indices[3] as usize;
            let stride1 = chunk.archetype.components[component1].size;
            let stride2 = chunk.archetype.components[component2].size;
            let stride3 = chunk.archetype.components[component3].size;
            let stride4 = chunk.archetype.components[component4].size;
            let mut ptr1 = chunk.column_ptr(component1);
            let mut ptr2 = chunk.column_ptr(component2);
            let mut ptr3 = chunk.column_ptr(component3);
            let mut ptr4 = chunk.column_ptr(component4);

            let until = chunk.entity_count - (chunk.entity_count % UNROLL);
            let mut i = 0;

            unsafe {
                macro_rules! process_entity {
                    ($offset:expr) => {
                        f(
                            ptr1.add($offset * stride1),
                            ptr2.add($offset * stride2),
                            ptr3.add($offset * stride3),
                            ptr4.add($offset * stride4),
                        );
                    };
                }

                while i < until {
                    process_entity!(0);
                    process_entity!(1);
                    process_entity!(2);
                    process_entity!(3);
                    process_entity!(4);
                    process_entity!(5);
                    process_entity!(6);
                    process_entity!(7);

                    ptr1 = ptr1.add(UNROLL * stride1);
                    ptr2 = ptr2.add(UNROLL * stride2);
                    ptr3 = ptr3.add(UNROLL * stride3);
                    ptr4 = ptr4.add(UNROLL * stride4);
                    i += UNROLL;
                }

                while i < chunk.entity_count {
                    process_entity!(0);
                    ptr1 = ptr1.add(stride1);
                    ptr2 = ptr2.add(stride2);
                    ptr3 = ptr3.add(stride3);
                    ptr4 = ptr4.add(stride4);
                    i += 1;
                }
            }
        });
    }
}

pub trait QueryParam {
    type Slice<'w>;
    type Item<'w>;

    fn component() -> QueryComponent;
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w>;
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w>;
}

impl<T: 'static> QueryParam for &T {
    type Slice<'w> = &'w [T];
    type Item<'w> = &'w T;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::new(register_rust_type::<T>(), false)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts(ptr as *const T, len)
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        &*((ptr as *const T).add(index))
    }
}

impl<T: 'static> QueryParam for &mut T {
    type Slice<'w> = &'w mut [T];
    type Item<'w> = &'w mut T;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::new(register_rust_type::<T>(), true)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts_mut(ptr as *mut T, len)
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        &mut *((ptr as *mut T).add(index))
    }
}

pub trait QuerySpec {
    type Chunk<'w>;
    type Item<'w>;

    fn descriptor() -> QueryDescriptor;
    unsafe fn chunk_from_raw<'w>(
        chunk: &'w Chunk,
        component_indices: &[u8],
    ) -> Self::Chunk<'w>;
    unsafe fn for_each_entity<'w, Func>(
        chunk: &'w Chunk,
        component_indices: &[u8],
        f: &mut Func,
    )
    where
        Func: FnMut(Self::Item<'w>);
}

impl<P: QueryParam> QuerySpec for P {
    type Chunk<'w> = P::Slice<'w>;
    type Item<'w> = P::Item<'w>;

    #[inline(always)]
    fn descriptor() -> QueryDescriptor {
        let mut components = SmallVec::new();
        components.push(P::component());
        QueryDescriptor::new(components)
    }

    #[inline(always)]
    unsafe fn chunk_from_raw<'w>(
        chunk: &'w Chunk,
        component_indices: &[u8],
    ) -> Self::Chunk<'w> {
        P::slice_from_raw(chunk.column_ptr(component_indices[0] as usize), chunk.entity_count)
    }

    #[inline(always)]
    unsafe fn for_each_entity<'w, Func>(
        chunk: &'w Chunk,
        component_indices: &[u8],
        f: &mut Func,
    )
    where
        Func: FnMut(Self::Item<'w>),
    {
        let base = chunk.column_ptr(component_indices[0] as usize);
        for entity_index in 0..chunk.entity_count {
            f(P::item_from_raw(base, entity_index));
        }
    }
}

macro_rules! impl_query_spec_tuple {
    ($(($Param:ident, $base:ident, $index:tt)),+ $(,)?) => {
        impl<$($Param: QueryParam),+> QuerySpec for ($($Param,)+) {
            type Chunk<'w> = ($($Param::Slice<'w>,)+);
            type Item<'w> = ($($Param::Item<'w>,)+);

            #[inline(always)]
            fn descriptor() -> QueryDescriptor {
                let mut components = SmallVec::new();
                $(components.push($Param::component());)+
                QueryDescriptor::new(components)
            }

            #[inline(always)]
            unsafe fn chunk_from_raw<'w>(
                chunk: &'w Chunk,
                component_indices: &[u8],
            ) -> Self::Chunk<'w> {
                (
                    $(
                        $Param::slice_from_raw(
                            chunk.column_ptr(component_indices[$index] as usize),
                            chunk.entity_count,
                        ),
                    )+
                )
            }

            #[inline(always)]
            unsafe fn for_each_entity<'w, Func>(
                chunk: &'w Chunk,
                component_indices: &[u8],
                f: &mut Func,
            )
            where
                Func: FnMut(Self::Item<'w>),
            {
                $(let $base = chunk.column_ptr(component_indices[$index] as usize);)+

                for entity_index in 0..chunk.entity_count {
                    f((
                        $(
                            $Param::item_from_raw($base, entity_index),
                        )+
                    ));
                }
            }
        }
    };
}

impl_query_spec_tuple!((A, a, 0), (B, b, 1));
impl_query_spec_tuple!((A, a, 0), (B, b, 1), (C, c, 2));
impl_query_spec_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3));
impl_query_spec_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3), (E, e, 4));
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7)
);

pub struct PreparedQuery<Q> {
    descriptor: QueryDescriptor,
    prepared: PreparedCache,
    marker: PhantomData<fn() -> Q>,
}

impl<Q: QuerySpec> Default for PreparedQuery<Q> {
    fn default() -> Self {
        Self {
            descriptor: Q::descriptor(),
            prepared: PreparedCache::default(),
            marker: PhantomData,
        }
    }
}

impl<Q: QuerySpec> PreparedQuery<Q> {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cached_archetype_count(&self) -> usize {
        self.prepared.cached_archetype_count()
    }

    #[inline(always)]
    fn prepare(&mut self, world: &World) {
        self.prepared.prepare(world, &self.descriptor);
    }

    #[inline(always)]
    pub fn for_each_chunk<F>(&mut self, world: &World, mut f: F)
    where
        F: for<'w> FnMut(Q::Chunk<'w>),
    {
        self.prepare(world);
        for cached in &self.prepared.archetypes {
            let data = &world.data[cached.data_index];

            for chunk in &data.chunks {
                debug_assert!(chunk.entity_count != 0);
                unsafe {
                    f(Q::chunk_from_raw(chunk, &cached.component_indices));
                }
            }
        }
    }

    #[inline(always)]
    pub fn for_each<F>(&mut self, world: &World, mut f: F)
    where
        F: for<'w> FnMut(Q::Item<'w>),
    {
        self.prepare(world);
        for cached in &self.prepared.archetypes {
            let data = &world.data[cached.data_index];

            for chunk in &data.chunks {
                debug_assert!(chunk.entity_count != 0);
                unsafe {
                    Q::for_each_entity(chunk, &cached.component_indices, &mut f);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PreparedQuery, Query, QueryIter};
    use crate::ecs::{create_archetype, World};

    #[derive(Clone, Copy, Default)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Copy, Default)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    #[derive(Clone, Copy, Default)]
    struct Extra {
        value: f32,
    }

    #[derive(Clone, Copy, Default)]
    struct Mass {
        value: f32,
    }

    fn spawn(world: &mut World, archetype: crate::ecs::Archetype, count: usize) {
        for _ in 0..count {
            world.add_entity(archetype);
        }
    }

    #[test]
    fn typed_single_component_query_reads_and_writes() {
        let archetype = create_archetype().add_rust_component::<Velocity>().build();
        let mut world = World::new();
        spawn(&mut world, archetype, 4);

        let mut init = world.query::<&mut Velocity>();
        let mut expected = 1.0;
        init.for_each(&world, |velocity| {
            velocity.x = expected;
            expected += 1.0;
        });

        let mut sum = 0.0;
        let mut read = world.query::<&Velocity>();
        read.for_each(&world, |velocity| {
            sum += velocity.x;
        });

        assert_eq!(sum, 10.0);
    }

    #[test]
    fn typed_two_component_query_updates_positions() {
        let archetype = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let mut world = World::new();
        spawn(&mut world, archetype, 8);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&world, |velocity| {
            velocity.x = 1.0;
            velocity.y = 2.0;
        });

        let mut query = world.query::<(&mut Position, &Velocity)>();
        query.for_each_chunk(&world, |(positions, velocities)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x * 0.5;
                positions[index].y += velocities[index].y * 0.5;
            }
        });

        let mut check = world.query::<&Position>();
        check.for_each(&world, |position| {
            assert_eq!(position.x, 0.5);
            assert_eq!(position.y, 1.0);
        });
    }

    #[test]
    fn typed_four_component_query_runs_across_matching_archetypes() {
        let archetype = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .add_rust_component::<Extra>()
            .add_rust_component::<Mass>()
            .build();
        let mut world = World::new();
        spawn(&mut world, archetype, 6);

        let mut init = world.query::<(&mut Velocity, &mut Extra, &mut Mass)>();
        init.for_each(&world, |(velocity, extra, mass)| {
            velocity.x = 2.0;
            velocity.y = 4.0;
            extra.value = 1.0;
            mass.value = 3.0;
        });

        let mut query = world.query::<(&mut Position, &Velocity, &mut Extra, &Mass)>();
        query.for_each_chunk(&world, |(positions, velocities, extras, masses)| {
            for index in 0..positions.len() {
                positions[index].x += velocities[index].x * masses[index].value;
                positions[index].y += velocities[index].y * masses[index].value;
                extras[index].value += masses[index].value;
            }
        });

        let mut position_check = world.query::<&Position>();
        position_check.for_each(&world, |position| {
            assert_eq!(position.x, 6.0);
            assert_eq!(position.y, 12.0);
        });

        let mut extra_check = world.query::<&Extra>();
        extra_check.for_each(&world, |extra| {
            assert_eq!(extra.value, 4.0);
        });
    }

    #[test]
    fn dynamic_query_matches_typed_results() {
        let archetype = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let mut world = World::new();
        spawn(&mut world, archetype, 5);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&world, |velocity| {
            velocity.x = 3.0;
            velocity.y = 1.5;
        });

        let typed_types = vec![
            crate::reflect::register_rust_type::<Position>(),
            crate::reflect::register_rust_type::<Velocity>(),
        ];
        let query = Query::new(typed_types);
        let mut dynamic = QueryIter::new(&world, &query);
        dynamic.for_each2(|position, velocity| {
            let position = unsafe { &mut *(position as *mut Position) };
            let velocity = unsafe { &*(velocity as *const Velocity) };
            position.x += velocity.x;
            position.y += velocity.y;
        });

        let mut typed = world.query::<&Position>();
        typed.for_each(&world, |position| {
            assert_eq!(position.x, 3.0);
            assert_eq!(position.y, 1.5);
        });
    }

    #[test]
    fn query_only_matches_archetypes_with_all_components() {
        let matching = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let position_only = create_archetype().add_rust_component::<Position>().build();

        let mut world = World::new();
        spawn(&mut world, matching, 2);
        spawn(&mut world, position_only, 3);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&world, |velocity| {
            velocity.x = 1.0;
        });

        let mut query = world.query::<(&mut Position, &Velocity)>();
        query.for_each(&world, |(position, velocity)| {
            position.x += velocity.x;
        });

        let mut changed = 0;
        let mut unchanged = 0;
        let mut positions = world.query::<&Position>();
        positions.for_each(&world, |position| {
            if position.x == 1.0 {
                changed += 1;
            } else {
                unchanged += 1;
            }
        });

        assert_eq!(changed, 2);
        assert_eq!(unchanged, 3);
    }

    #[test]
    fn prepared_query_refreshes_when_new_matching_archetype_appears() {
        let base = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .build();
        let extended = create_archetype()
            .add_rust_component::<Position>()
            .add_rust_component::<Velocity>()
            .add_rust_component::<Extra>()
            .build();

        let mut world = World::new();
        world.add_entity(base);

        let mut init = world.query::<&mut Velocity>();
        init.for_each(&world, |velocity| {
            velocity.x = 1.0;
        });

        let mut prepared = PreparedQuery::<(&mut Position, &Velocity)>::new();
        prepared.for_each(&world, |(position, velocity)| {
            position.x += velocity.x;
        });
        assert_eq!(prepared.cached_archetype_count(), 1);

        world.add_entity(extended);
        let mut init_new = world.query::<(&mut Velocity, &mut Extra)>();
        init_new.for_each(&world, |(velocity, extra)| {
            velocity.x = 1.0;
            extra.value = 2.0;
        });

        prepared.for_each(&world, |(position, velocity)| {
            position.x += velocity.x;
        });
        assert_eq!(prepared.cached_archetype_count(), 2);

        let mut sum = 0.0;
        let mut positions = world.query::<&Position>();
        positions.for_each(&world, |position| {
            sum += position.x;
        });

        assert_eq!(sum, 3.0);
    }

    #[test]
    #[should_panic(expected = "duplicate component type")]
    fn dynamic_query_rejects_duplicate_types() {
        let archetype = create_archetype().add_rust_component::<Position>().build();
        let mut world = World::new();
        world.add_entity(archetype);

        let ty = crate::reflect::register_rust_type::<Position>();
        let query = Query::new(vec![ty, ty]);
        let _ = QueryIter::new(&world, &query);
    }

    #[test]
    #[should_panic(expected = "duplicate component type")]
    fn typed_query_rejects_duplicate_types() {
        let archetype = create_archetype().add_rust_component::<Position>().build();
        let mut world = World::new();
        world.add_entity(archetype);

        let mut query = PreparedQuery::<(&mut Position, &mut Position)>::new();
        query.for_each(&world, |_| {});
    }
}
