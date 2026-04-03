//! # Commands
//!
//! Demonstrates deferred structural changes via `Commands`: spawning,
//! inserting components, removing components, and despawning — all
//! batched and applied atomically.
//!
//! ```
//! cargo run --example commands
//! ```

use sky_engine::ecs::{Commands, World};

#[derive(Clone, Copy, Debug)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
struct Health(f32);

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct Poison {
    dps: f32,
}

#[derive(Clone, Copy, Debug)]
struct Shield;

fn main() {
    let mut world = World::new();

    // Spawn some entities directly
    let warrior = world.spawn((Position { x: 0.0, y: 0.0 }, Health(100.0)));
    let mage = world.spawn((Position { x: 5.0, y: 0.0 }, Health(60.0)));
    let rogue = world.spawn((Position { x: 10.0, y: 0.0 }, Health(80.0)));

    println!("=== Initial state ===");
    print_entities(&world);

    // --- Deferred commands ---
    let mut cmds = Commands::new();

    // Give the warrior a shield
    cmds.insert(warrior, Shield);

    // Poison the mage
    cmds.insert(mage, Poison { dps: 15.0 });

    // Spawn a healer via commands
    cmds.spawn((Position { x: -5.0, y: 0.0 }, Health(50.0)));

    // Kill the rogue
    cmds.despawn(rogue);

    // Apply all at once
    cmds.apply(&mut world);

    println!("\n=== After commands ===");
    print_entities(&world);

    // Check specifics
    println!("\nWarrior has Shield? {}", world.has::<Shield>(warrior));
    println!("Mage has Poison?   {}", world.has::<Poison>(mage));
    println!("Rogue alive?       {}", world.contains(rogue));

    // --- Remove a component ---
    let mut cmds = Commands::new();
    cmds.remove::<Poison>(mage);
    cmds.apply(&mut world);

    println!("\n=== After removing poison ===");
    println!("Mage has Poison?   {}", world.has::<Poison>(mage));
    println!("Mage health:       {:?}", world.get::<Health>(mage));
}

fn print_entities(world: &World) {
    let mut q = world.query::<(&Position, &Health)>();
    q.for_each_with_entity(world, |entity, (pos, hp)| {
        println!(
            "  {:?}: pos=({:.0}, {:.0}) hp={:.0}",
            entity, pos.x, pos.y, hp.0
        );
    });
    println!("  Total entities: {}", world.entity_count());
}
