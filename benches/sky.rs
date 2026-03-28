use criterion::{criterion_group, criterion_main, Criterion};
use sky_engine::{
    ecs::{create_archetype, Query, QueryIter, World},
    reflect,
};

const ENTITY_COUNT: usize = 5_000_000;
const DELTA: f32 = 0.1;

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
    for _ in 0..ENTITY_COUNT {
        world.add_entity(archetype);
    }

    let query = Query::new(vec![ty_b, ty_a]);
    let mut test = QueryIter::new(&world, &query);

    c.bench_function("sky_2_of_4", |b| {
        b.iter(|| {
            test.for_each_chunk2::<PositionComponent, VelocityComponent, _>(|positions, velocities| {
                for (position, velocity) in positions.iter_mut().zip(velocities.iter()) {
                    position.x += velocity.x * DELTA;
                    position.y += velocity.y * DELTA;
                }
            });
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
