use std::ptr::null;

use super::{Archetype, Data, World};
use crate::reflect::*;

pub struct Query {
    pub types: Vec<Type>,
}

impl Query {
    pub fn new(types: Vec<Type>) -> Self {
        Query { types: types }
    }
}

pub struct CachedData<'a> {
    pub data: &'a Data,
    pub layout: Vec<usize>,
}

impl<'a> CachedData<'a> {
    fn new(data: &'a Data, layout: Vec<usize>) -> Self {
        Self { data, layout }
    }
}

pub struct QueryIter<'a> {
    world: &'a World,
    pub query: &'a Query,
    pub cached: Vec<CachedData<'a>>,
}

impl<'a> QueryIter<'a> {
    pub fn new(world: &'a World, query: &'a Query) -> Self {
        let mut iter = QueryIter {
            world,
            query,
            cached: Vec::new(),
        };
        iter.cache_data();
        iter
    }

    fn cache_layout(&mut self, archetype: &Archetype) -> Vec<usize> {
        /*let mut cached_layout = Vec::with_capacity(self.query.types.capacity());

        for ty in self.query.types.iter() {
            match archetype.query_component_offset(ty) {
                Some(offset) => cached_layout.push(offset),
                None => println!("{:?}", cached_layout),
            }
        }

        cached_layout*/
        self.query
            .types
            .iter()
            .filter_map(|ty| archetype.query_component_offset(ty))
            .collect()
    }

    fn cache_data(&mut self) {
        for (archetype, data) in &self.world.data {
            if archetype.matches_query(self.query) {
                let cache = CachedData::new(data, self.cache_layout(archetype));
                self.cached.push(cache);
            }
        }
    }
}

// cache --> chunk -->
// query type 查找的类型 cached data
impl<'a> QueryIter<'a> {
    //type Item = Vec<*const u8>;
    #[inline(always)]
    pub fn for_each<F>(&mut self, f: F)
    where
        F: Fn(*mut u8, *mut u8, *mut u8, *mut u8),
    {
        let mut current_data_index = 0;

        while current_data_index < self.cached.len() {
            let cache = &self.cached[current_data_index];
            let chunks = &cache.data.chunks;
            let total_size = cache.data.archetype.total_size;

            let layout1 = cache.layout[0];
            let layout2 = cache.layout[1];
            let layout3 = cache.layout[2];
            let layout4 = cache.layout[3];

            for chunk in chunks {
                let entity_count = chunk.entity_count;
                let ptr = chunk.data.as_ptr();

                let mut ptr1 = unsafe { ptr.add(layout1) as *mut u8 };
                let mut ptr2 = unsafe { ptr.add(layout2) as *mut u8 };
                let mut ptr3 = unsafe { ptr.add(layout3) as *mut u8 };
                let mut ptr4 = unsafe { ptr.add(layout4) as *mut u8 };

                let step = 16; // Unroll the loop by a factor of 16
                let until = entity_count - (entity_count % step);

                let mut i = 0;

                unsafe {
                    macro_rules! process_entity {
                        ($offset:expr) => {
                            f(
                                ptr1.add($offset * total_size),
                                ptr2.add($offset * total_size),
                                ptr3.add($offset * total_size),
                                ptr4.add($offset * total_size),
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
                        process_entity!(8);
                        process_entity!(9);
                        process_entity!(10);
                        process_entity!(11);
                        process_entity!(12);
                        process_entity!(13);
                        process_entity!(14);
                        process_entity!(15);

                        ptr1 = ptr1.add(16 * total_size);
                        ptr2 = ptr2.add(16 * total_size);
                        ptr3 = ptr3.add(16 * total_size);
                        ptr4 = ptr4.add(16 * total_size);
                        i += step;
                    }

                    while i < entity_count {
                        process_entity!(0);

                        ptr1 = ptr1.add(total_size);
                        ptr2 = ptr2.add(total_size);
                        ptr3 = ptr3.add(total_size);
                        ptr4 = ptr4.add(total_size);
                        i += 1;
                    }
                }
            }

            current_data_index += 1;
        }
    }
}
