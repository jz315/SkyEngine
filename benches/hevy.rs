use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::*;
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
    // Nearly any type can be used as a component with zero boilerplate

    for i in 1..5000000 {
        world.spawn((
            VelocityComponent { x: 0.0, y: 0.0 },
            PositionComponent { x: 0.0, y: 0.0 },
            test3Component { x: 0.0, y: 0.0 },
            test4Component { x: 0.0, y: 0.0 },
        ));
    }

    c.bench_function("hevy", |b| {
        let mut query = PreparedQuery::<(&mut PositionComponent, &VelocityComponent)>::default();
        let _ = query.query(&world).iter();
        b.iter(|| {
            for (_, (p, v)) in query.query(&world).iter() {
                p.x += v.x * 0.1;
                p.y += v.y * 0.1;
            }
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
