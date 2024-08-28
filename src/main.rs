use std::{borrow::Borrow, collections::btree_map::Range, rc::Rc};

use sky_engine::{
    ecs::{create_archetype, Query, QueryIter, World},
    reflect,
};
pub struct VelocityComponent {
    pub x: f32,
    pub y: f32,
}

pub struct PositionComponent {
    pub x: f32,
    pub y: f32,
}
static mut count:usize=0;
fn main() {
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

    let archetype = create_archetype()
        .add_component(ty_a)
        .add_component(ty_b)
        .build();

    let mut world = World::new();
    for i in 1..50000000 {
        world.add_entity(archetype);
    }

    let query = Query::new(vec![ty_a, ty_b,ty_a, ty_b]);
    let mut test = QueryIter::new(&world, &query);

    test.for_each(|comp1,comp2,comp3,comp4| {
        let v = unsafe { &mut *(comp1 as *mut VelocityComponent) };
        let p = unsafe { &mut *(comp2 as *mut PositionComponent) };

        p.x += v.x * 0.1 + 1.0;
        p.y += v.y * 0.1 + 1.0;
    });
    
    
}
