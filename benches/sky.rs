#![feature(portable_simd)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sky_engine::{
    ecs::{create_archetype, Chunk, Query, QueryIter, World},
    reflect,
};
use std::{
    arch::x86_64::{_mm_prefetch, _MM_HINT_T0, _MM_HINT_T1, _MM_HINT_T2},
    borrow::Borrow,
    collections::btree_map::Range,
    ptr,
    rc::Rc,
    simd::u64x4,
};
pub struct VelocityComponent {
    pub x: f32,
    pub y: f32,
}

pub struct PositionComponent {
    pub x: f32,
    pub y: f32,
}

pub struct test3Component {
    pub x: f32,
    pub y: f32,
}

pub struct test4Component {
    pub x: f32,
    pub y: f32,
}
fn criterion_benchmark(c: &mut Criterion) {
    // 假设我们注册了一些组件类型
    let ty_a = reflect::register(
        "VelocityComponent",
        std::mem::size_of::<VelocityComponent>(),
        std::mem::align_of::<VelocityComponent>(),
    );
    let ty_b: reflect::Type = reflect::register(
        "PositionComponent",
        std::mem::size_of::<PositionComponent>(),
        std::mem::align_of::<PositionComponent>(),
    );

    let ty_c = reflect::register(
        "test3Component",
        std::mem::size_of::<test3Component>(),
        std::mem::align_of::<test3Component>(),
    );
    let ty_d: reflect::Type = reflect::register(
        "test4Component",
        std::mem::size_of::<test4Component>(),
        std::mem::align_of::<test4Component>(),
    );

    let archetype = create_archetype()
        .add_component(ty_a)
        .add_component(ty_b)
        .add_component(ty_c)
        .add_component(ty_d)
        .build();

    let mut world = World::new();
    for i in 1..5000000 {
        world.add_entity(archetype);
    }
    println!("Test Start");
    let query = Query::new(vec![ty_a, ty_b, ty_c, ty_d]);
    let mut test = QueryIter::new(&world, &query);

    c.bench_function("sky1", |b| {
        b.iter(|| {
            test.for_each(|comp1,comp2,comp3,comp4| {
                let v = unsafe { &mut *(comp1 as *mut VelocityComponent) };
                let p = unsafe { &mut *(comp2 as *mut PositionComponent) };

                p.x += v.x * 0.1 + 1.0;
                p.y += v.y * 0.1 + 1.0;
            });
        })
    });
    /*
    c.bench_function("sky1", |b| {
        b.iter(|| {
            if test.query.types.len() <= 4 {
                let mut current_data_index = 0;

                while current_data_index < test.cached.len() {
                    let cache = &test.cached[current_data_index];
                    let total_size = cache.data.archetype.total_size;
                    let chunks = &cache.data.chunks;

                    assert_eq!(core::mem::size_of::<usize>(), core::mem::size_of::<u64>());

                    let layout = u64x4::from_array([
                        *cache.layout.get(0).unwrap_or(&0) as u64,
                        *cache.layout.get(1).unwrap_or(&0) as u64,
                        *cache.layout.get(2).unwrap_or(&0) as u64,
                        *cache.layout.get(3).unwrap_or(&0) as u64,
                    ]);
                    let offset = u64x4::splat(total_size as u64);

                    for chunk in chunks {
                        let entity_count = chunk.entity_count;

                        let ptr = chunk.data.as_ptr();

                        let mut address = u64x4::splat(ptr as u64);

                        address += layout;

                        let step = 16; // Unroll the loop by a factor of 16
                        let until = entity_count - (entity_count % step);

                        let mut i = 0;
                        macro_rules! process_entity {
                            ($offset:expr) => {{
                                let iter = address.as_mut() as &[u64; 4];
                                let v = unsafe {
                                    &mut *(*iter.get_unchecked(0) as *mut VelocityComponent)
                                };
                                let p = unsafe {
                                    &mut *(*iter.get_unchecked(1) as *mut PositionComponent)
                                };
                                let t3 = unsafe {
                                    &mut *(*iter.get_unchecked(2) as *mut test3Component)
                                };
                                let t4 = unsafe {
                                    &mut *(*iter.get_unchecked(3) as *mut test4Component)
                                };
                                p.x += v.x * 0.1 + 1.0;
                                p.y += v.y * 0.1 + 1.0;
                                t3.x += t4.x * 0.1 + 1.0;
                                t3.y += t4.y * 0.1 + 1.0;
                                address += offset;
                            }};
                        }
                        while i < until {
                            unsafe {
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
                            }
                            i += step;
                        }

                        // Handle remaining entities
                        while i < entity_count {
                            unsafe {
                                process_entity!(0);
                            }
                            i += 1;
                        }
                    }

                    current_data_index += 1;
                }
            }
        });
    });

    c.bench_function("sky2", |b| {
        b.iter(|| {
            let this = &mut test;
            let mut current_data_index = 0;

            while current_data_index < this.cached.len() {
                let cache = &this.cached[current_data_index];
                let chunks = &cache.data.chunks;
                let total_size = cache.data.archetype.total_size;

                let layout1 = cache.layout[0];
                let layout2 = cache.layout[1];
                let layout3 = cache.layout[2];
                let layout4 = cache.layout[3];

                for chunk in chunks {
                    let entity_count = chunk.entity_count;
                    let ptr = chunk.data.as_ptr();

                    let mut ptr1 = unsafe { ptr.byte_add(layout1) };
                    let mut ptr2 = unsafe { ptr.byte_add(layout2) };
                    let mut ptr3 = unsafe { ptr.byte_add(layout3) };
                    let mut ptr4 = unsafe { ptr.byte_add(layout4) };

                    let step = 16; // Unroll the loop by a factor of 16
                    let until = entity_count - (entity_count % step);

                    let mut i = 0;
                    while i < until {
                        unsafe {
                            macro_rules! process_entity {
                                ($offset:expr) => {
                                    let v = &mut *(ptr1.add($offset * total_size)
                                        as *mut VelocityComponent);
                                    let p = &mut *(ptr2.add($offset * total_size)
                                        as *mut PositionComponent);
                                    let t3 = &mut *(ptr3.add($offset * total_size)
                                        as *mut test3Component);
                                    let t4 = &mut *(ptr4.add($offset * total_size)
                                        as *mut test4Component);
                                    p.x += v.x * 0.1 + 1.0;
                                    p.y += v.y * 0.1 + 1.0;
                                    t3.x += t4.x * 0.1 + 1.0;
                                    t3.y += t4.y * 0.1 + 1.0;
                                };
                            }

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
                        }
                        i += step;
                    }

                    // Handle remaining entities
                    while i < entity_count {
                        unsafe {
                            let v = &mut *(ptr1 as *mut VelocityComponent);
                            let p = &mut *(ptr2 as *mut PositionComponent);
                            p.x += v.x;
                            p.y += v.y;

                            ptr1 = ptr1.add(total_size);
                            ptr2 = ptr2.add(total_size);
                        }
                        i += 1;
                    }
                }

                current_data_index += 1;
            }
        });
    }); */
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
