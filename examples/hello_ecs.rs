//! # Hello ECS
//!
//! The simplest possible Sky Engine example: spawn entities, query, mutate.
//!
//! ```
//! cargo run --example hello_ecs
//! ```

use sky_engine::ecs::World;

#[derive(Clone, Copy, Debug)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
struct Velocity {
    x: f32,
    y: f32,
}

fn main() {
    let mut world = World::new();

    // Spawn some entities
    for i in 0..5 {
        let f = i as f32;
        world.spawn((
            Position {
                x: f * 10.0,
                y: 0.0,
            },
            Velocity { x: 1.0, y: 2.0 + f },
        ));
    }

    println!("=== Before movement ===");
    let mut q = world.query::<(&Position, &Velocity)>();
    q.for_each_with_entity(&world, |entity, (pos, vel)| {
        println!(
            "  Entity {:?}: pos=({:.1}, {:.1}) vel=({:.1}, {:.1})",
            entity, pos.x, pos.y, vel.x, vel.y
        );
    });

    // Simulate 10 ticks
    let dt = 1.0 / 60.0;
    for _ in 0..10 {
        let mut q = world.query::<(&mut Position, &Velocity)>();
        q.for_each(&world, |(pos, vel)| {
            pos.x += vel.x * dt;
            pos.y += vel.y * dt;
        });
    }

    println!("\n=== After 10 ticks ===");
    let mut q = world.query::<&Position>();
    q.for_each_with_entity(&world, |entity, pos| {
        println!("  Entity {:?}: pos=({:.2}, {:.2})", entity, pos.x, pos.y);
    });

    println!("\nEntity count: {}", world.entity_count());
}
