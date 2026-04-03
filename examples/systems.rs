//! # Systems
//!
//! Demonstrates the system scheduling API: groups, fixed-step physics,
//! and lifecycle hooks (init / run / teardown).
//!
//! ```
//! cargo run --example systems
//! ```

use sky_engine::ecs::{System, World};

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

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
struct Gravity(f32);

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Default, Debug)]
struct FrameCount(u32);

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// A "proper" system with init/run/teardown.
struct PhysicsSystem;

impl System for PhysicsSystem {
    fn init(&mut self, world: &mut World) {
        // Spawn some entities on first tick
        for i in 0..5 {
            world.spawn((
                Position {
                    x: i as f32 * 20.0,
                    y: 100.0,
                },
                Velocity { x: 0.0, y: 0.0 },
                Gravity(-9.81),
            ));
        }
        println!("[PhysicsSystem] Spawned 5 falling entities");
    }

    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;
        let mut q = world.query::<(&mut Position, &mut Velocity, &Gravity)>();
        q.for_each(world, |(pos, vel, grav)| {
            vel.y += grav.0 * dt;
            pos.x += vel.x * dt;
            pos.y += vel.y * dt;
        });
    }

    fn teardown(&mut self, _world: &mut World) {
        println!("[PhysicsSystem] Teardown complete");
    }
}

fn main() {
    let mut world = World::new();
    world.insert_resource(FrameCount::default());

    // Schedule: physics group at fixed 50Hz, logging group every frame
    world.group("physics").fixed(0.02).add(PhysicsSystem);

    // Closure systems work too
    world.group("logging").add(|world: &mut World| {
        let frame = &mut world.get_resource_mut::<FrameCount>().unwrap().0;
        *frame += 1;
    });

    // Simulate 5 frames at 60fps
    println!("=== Simulating 5 frames ===\n");
    for frame in 0..5 {
        world.tick_with_delta(1.0 / 60.0);

        let mut q = world.query::<(&Position, &Velocity)>();
        println!("Frame {}:", frame);
        q.for_each_with_entity(&world, |entity, (pos, vel)| {
            println!(
                "  {:?}: pos=({:6.2}, {:6.2})  vel=({:6.2}, {:6.2})",
                entity, pos.x, pos.y, vel.x, vel.y
            );
        });
        println!();
    }

    world.shutdown();

    let frames = world.get_resource::<FrameCount>().unwrap().0;
    println!("Total frames logged: {}", frames);
}
