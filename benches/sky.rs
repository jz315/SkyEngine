use criterion::{criterion_group, criterion_main, Criterion};
use sky_engine::ecs::{create_archetype, World};

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
    let archetype = create_archetype()
        .add_rust_component::<VelocityComponent>()
        .add_rust_component::<PositionComponent>()
        .add_rust_component::<test3Component>()
        .add_rust_component::<test4Component>()
        .build();

    let mut world = World::new();
    for _ in 0..ENTITY_COUNT {
        world.add_entity(archetype);
    }

    let mut query2 = world.query::<(&mut PositionComponent, &VelocityComponent)>();
    let mut query4 =
        world.query::<(&mut PositionComponent, &VelocityComponent, &mut test3Component, &test4Component)>();

    c.bench_function("sky_2_of_4", |b| {
        b.iter(|| {
            query2.for_each_chunk(&world, |(positions, velocities)| {
                for (position, velocity) in positions.iter_mut().zip(velocities.iter()) {
                    position.x += velocity.x * DELTA;
                    position.y += velocity.y * DELTA;
                }
            });
        })
    });

    c.bench_function("sky_4_of_4", |b| {
        b.iter(|| {
            query4.for_each_chunk(&world, |(positions, velocities, tests3, tests4)| {
                for index in 0..positions.len() {
                    positions[index].x += velocities[index].x * DELTA + tests4[index].x * DELTA;
                    positions[index].y += velocities[index].y * DELTA + tests4[index].y * DELTA;
                    tests3[index].x += velocities[index].x;
                    tests3[index].y += tests4[index].y;
                }
            });
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
