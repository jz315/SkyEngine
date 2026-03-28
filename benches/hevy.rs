use criterion::{criterion_group, criterion_main, Criterion};
use hecs::{PreparedQuery, World};

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
    let mut world = World::new();

    for _ in 0..ENTITY_COUNT {
        world.spawn((
            VelocityComponent { x: 0.0, y: 0.0 },
            PositionComponent { x: 0.0, y: 0.0 },
            test3Component { x: 0.0, y: 0.0 },
            test4Component { x: 0.0, y: 0.0 },
        ));
    }

    let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();

    c.bench_function("hecs_2_of_4", |b| {
        b.iter(|| {
            for (_, (p, v)) in query.query(&world).iter() {
                p.x += v.x * DELTA;
                p.y += v.y * DELTA;
            }
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
