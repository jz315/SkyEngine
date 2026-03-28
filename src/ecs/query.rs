use super::{Archetype, Data, World};
use crate::reflect::*;

const UNROLL: usize = 8;

pub struct Query {
    pub types: Vec<Type>,
}

impl Query {
    pub fn new(types: Vec<Type>) -> Self {
        Self { types }
    }
}

pub struct CachedData<'a> {
    pub data: &'a Data,
    pub component_indices: Vec<usize>,
}

impl<'a> CachedData<'a> {
    fn new(data: &'a Data, component_indices: Vec<usize>) -> Self {
        Self {
            data,
            component_indices,
        }
    }
}

pub struct QueryIter<'a> {
    world: &'a World,
    pub query: &'a Query,
    pub cached: Vec<CachedData<'a>>,
}

impl<'a> QueryIter<'a> {
    pub fn new(world: &'a World, query: &'a Query) -> Self {
        let mut iter = Self {
            world,
            query,
            cached: Vec::new(),
        };
        iter.cache_data();
        iter
    }

    fn cache_component_indices(&self, archetype: &Archetype) -> Vec<usize> {
        self.query
            .types
            .iter()
            .filter_map(|ty| archetype.query_component_index(ty))
            .collect()
    }

    fn cache_data(&mut self) {
        for (archetype, data) in &self.world.data {
            if archetype.matches_query(self.query) {
                let cache = CachedData::new(data, self.cache_component_indices(archetype));
                self.cached.push(cache);
            }
        }
    }
}

impl<'a> QueryIter<'a> {
    #[inline(always)]
    pub fn for_each2<F>(&mut self, mut f: F)
    where
        F: FnMut(*mut u8, *mut u8),
    {
        let mut current_data_index = 0;

        while current_data_index < self.cached.len() {
            let cache = &self.cached[current_data_index];

            debug_assert!(cache.component_indices.len() >= 2);

            let component1 = cache.component_indices[0];
            let component2 = cache.component_indices[1];
            let stride1 = cache.data.archetype.components[component1].size;
            let stride2 = cache.data.archetype.components[component2].size;

            for chunk in &cache.data.chunks {
                let entity_count = chunk.entity_count;
                let mut ptr1 = chunk.column_ptr(component1);
                let mut ptr2 = chunk.column_ptr(component2);

                let step = UNROLL;
                let until = entity_count - (entity_count % step);
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
                        i += step;
                    }

                    while i < entity_count {
                        process_entity!(0);

                        ptr1 = ptr1.add(stride1);
                        ptr2 = ptr2.add(stride2);
                        i += 1;
                    }
                }
            }

            current_data_index += 1;
        }
    }

    #[inline(always)]
    pub fn for_each<F>(&mut self, mut f: F)
    where
        F: FnMut(*mut u8, *mut u8, *mut u8, *mut u8),
    {
        let mut current_data_index = 0;

        while current_data_index < self.cached.len() {
            let cache = &self.cached[current_data_index];

            debug_assert!(cache.component_indices.len() >= 4);

            let component1 = cache.component_indices[0];
            let component2 = cache.component_indices[1];
            let component3 = cache.component_indices[2];
            let component4 = cache.component_indices[3];
            let stride1 = cache.data.archetype.components[component1].size;
            let stride2 = cache.data.archetype.components[component2].size;
            let stride3 = cache.data.archetype.components[component3].size;
            let stride4 = cache.data.archetype.components[component4].size;

            for chunk in &cache.data.chunks {
                let entity_count = chunk.entity_count;
                let mut ptr1 = chunk.column_ptr(component1);
                let mut ptr2 = chunk.column_ptr(component2);
                let mut ptr3 = chunk.column_ptr(component3);
                let mut ptr4 = chunk.column_ptr(component4);

                let step = UNROLL;
                let until = entity_count - (entity_count % step);
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
                        i += step;
                    }

                    while i < entity_count {
                        process_entity!(0);

                        ptr1 = ptr1.add(stride1);
                        ptr2 = ptr2.add(stride2);
                        ptr3 = ptr3.add(stride3);
                        ptr4 = ptr4.add(stride4);
                        i += 1;
                    }
                }
            }

            current_data_index += 1;
        }
    }
}
