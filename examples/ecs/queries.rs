//! # Queries & Filters
//!
//! Demonstrates the typed query API: single and multi-component queries,
//! optional components, named query data, filters, and chunk iteration.
//!
//! ```
//! cargo run --example queries
//! ```

use sky_engine::ecs::{QueryData, With, Without, World};

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

#[derive(Clone, Copy, Debug)]
struct Health(f32);

#[derive(Clone, Copy, Debug)]
struct Enemy;

#[derive(Clone, Copy, Debug)]
struct Player;

#[derive(QueryData)]
struct Movement<'w> {
    position: &'w mut Position,
    velocity: &'w Velocity,
}

fn main() {
    let mut world = World::new();

    // Spawn a player
    world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 0.0 },
        Health(100.0),
        Player,
    ));

    // Spawn enemies — some with velocity, some stationary
    world.spawn((
        Position { x: 10.0, y: 5.0 },
        Velocity { x: -1.0, y: 0.0 },
        Health(30.0),
        Enemy,
    ));
    world.spawn((Position { x: 20.0, y: 0.0 }, Health(50.0), Enemy)); // no Velocity
    world.spawn((
        Position { x: 30.0, y: 3.0 },
        Velocity { x: -0.5, y: 0.0 },
        Health(20.0),
        Enemy,
    ));

    // --- 1. Simple query: all entities with Position ---
    println!("=== All positioned entities ===");
    let q = world.query::<&Position>();
    println!("  Count: {}\n", q.count());

    // --- 2. Filtered query: enemies only ---
    println!("=== Enemies ===");
    let q = world
        .query::<(&Position, &Health)>()
        .filter::<With<Enemy>>();
    q.for_each(|(pos, hp)| {
        println!("  pos=({:.0}, {:.0})  hp={:.0}", pos.x, pos.y, hp.0);
    });

    // --- 3. Exclude filter: everything that is NOT an enemy ---
    println!("\n=== Non-enemies (player) ===");
    let q = world
        .query::<(&Position, &Health)>()
        .filter::<Without<Enemy>>();
    q.for_each(|(pos, hp)| {
        println!("  pos=({:.0}, {:.0})  hp={:.0}", pos.x, pos.y, hp.0);
    });

    // --- 4. Optional: Position + optional Velocity ---
    println!("\n=== Optional velocity ===");
    let q = world.query::<(&Position, Option<&Velocity>)>();
    q.for_each(|(pos, vel)| match vel {
        Some(v) => println!(
            "  ({:.0}, {:.0}) moving at ({:.1}, {:.1})",
            pos.x, pos.y, v.x, v.y
        ),
        None => println!("  ({:.0}, {:.0}) stationary", pos.x, pos.y),
    });

    // --- 5. Named query data for readable business logic ---
    println!("\n=== Named movement query ===");
    world.query_mut::<Movement>().for_each(|movement| {
        movement.position.x += movement.velocity.x;
        movement.position.y += movement.velocity.y;
    });

    // --- 6. Chunk iteration for batch processing ---
    println!("\n=== Chunk iteration (move all with velocity) ===");
    let dt = 1.0;
    let mut q = world.query_mut::<(&mut Position, &Velocity)>();
    q.for_each_chunk(|(positions, velocities)| {
        println!("  Processing chunk of {} entities", positions.len());
        for i in 0..positions.len() {
            positions[i].x += velocities[i].x * dt;
            positions[i].y += velocities[i].y * dt;
        }
    });

    // --- 7. Combined filter ---
    println!("\n=== Moving enemies only (With<Enemy>, With<Velocity>) ===");
    let q = world
        .query::<&Position>()
        .filter::<(With<Enemy>, With<Velocity>)>();
    q.for_each(|pos| {
        println!("  ({:.0}, {:.0})", pos.x, pos.y);
    });
}
