use super::{PreparedCache, QueryDescriptor, World};
use crate::ecs::ComponentType;
use core::{mem, slice};

const UNROLL: usize = 8;

pub struct Query {
    pub(crate) types: Vec<ComponentType>,
}

impl Query {
    pub fn new(types: Vec<ComponentType>) -> Self {
        Self { types }
    }

    pub fn types(&self) -> &[ComponentType] {
        &self.types
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
        prepared.prepare::<()>(world, &descriptor);

        Self {
            world,
            query,
            prepared,
        }
    }

    #[inline(always)]
    fn debug_assert_query_type<T>(ty: &ComponentType) {
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
                let components1 = slice::from_raw_parts_mut(
                    chunk.column_ptr(component1) as *mut A,
                    chunk.entity_count,
                );
                let components2 = slice::from_raw_parts(
                    chunk.column_ptr(component2) as *const B,
                    chunk.entity_count,
                );
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

#[cfg(test)]
mod tests {
    use super::super::super::create_archetype;
    use super::super::super::World;
    use super::{Query, QueryIter};

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

    fn spawn(world: &mut World, archetype: super::super::super::Archetype, count: usize) {
        for _ in 0..count {
            world.add_entity(archetype);
        }
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
            crate::ecs::component_type::<Position>(),
            crate::ecs::component_type::<Velocity>(),
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
    #[should_panic(expected = "duplicate component type")]
    fn dynamic_query_rejects_duplicate_types() {
        let archetype = create_archetype().add_rust_component::<Position>().build();
        let mut world = World::new();
        world.add_entity(archetype);

        let ty = crate::ecs::component_type::<Position>();
        let query = Query::new(vec![ty, ty]);
        let _ = QueryIter::new(&world, &query);
    }
}
