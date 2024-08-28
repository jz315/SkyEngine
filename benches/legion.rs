use criterion::{criterion_group, criterion_main, Criterion};
use specs::{
    Builder, Component, DispatcherBuilder, Join, ReadStorage, System, VecStorage, World, WorldExt,
    WriteStorage,
};

#[derive(Debug, Clone)]
struct Position {
    x: f32,
    y: f32,
}

impl Component for Position {
    type Storage = VecStorage<Self>;
}

#[derive(Debug, Clone)]
struct Velocity {
    x: f32,
    y: f32,
}

impl Component for Velocity {
    type Storage = VecStorage<Self>;
}

struct UpdatePositions;

impl<'a> System<'a> for UpdatePositions {
    type SystemData = (WriteStorage<'a, Position>, ReadStorage<'a, Velocity>);

    fn run(&mut self, (mut pos, vel): Self::SystemData) {
        for (pos, vel) in (&mut pos, &vel).join() {
            pos.x += vel.x;
            pos.y += vel.y;
        }
    }
}

fn create_world(entity_count: usize) -> World {
    let mut world = World::new();

    world.register::<Position>();
    world.register::<Velocity>();

    for _ in 0..entity_count {
        world
            .create_entity()
            .with(Position { x: 0.0, y: 0.0 })
            .with(Velocity { x: 1.0, y: 1.0 })
            .build();
    }

    world
}
#[derive(Clone)]
struct Conbine {
    v: Velocity,
    p: Position,
}

impl Conbine {
    fn new() -> Self {
        Self {
            v: Velocity { x: 0.0, y: 0.0 },
            p: Position { x: 0.0, y: 0.0 },
        }
    }
}
fn criterion_benchmark(c: &mut Criterion) {
    let entity_count = 50000000; // Adjust the number of entities as needed
    let mut array: Vec<Conbine> = Vec::with_capacity(entity_count);
    array.resize(entity_count, Conbine::new());

    c.bench_function("update_positions", |b| {
        b.iter(|| {
            for ele in array.iter_mut() {
                ele.p.x += ele.v.x * 0.1;
                ele.p.y += ele.v.y * 0.1;
            }
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
