//! # Systems
//!
//! Demonstrates typed system parameters, stages, and fixed-step execution.
//!
//! ```
//! cargo run --example systems
//! ```

use sky_engine::ecs::{FixedStep, FixedUpdate, Res, ResMut, Time, Update, View, World};

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

fn physics(bodies: View<(&mut Position, &mut Velocity, &Gravity)>, time: Res<Time>) {
    bodies.for_each(|(pos, vel, grav)| {
        vel.y += grav.0 * time.delta;
        pos.x += vel.x * time.delta;
        pos.y += vel.y * time.delta;
    });
}

fn count_frame(mut count: ResMut<FrameCount>) {
    count.0 += 1;
}

fn main() {
    let mut world = World::new();
    world.insert_resource(FrameCount::default());

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

    world
        .stage(FixedUpdate)
        .fixed(FixedStep::hz(50))
        .expect("fixed-step configuration must be unique")
        .add(physics);
    world.stage(Update).add(count_frame);

    // Simulate 5 frames at 60fps
    println!("=== Simulating 5 frames ===\n");
    for frame in 0..5 {
        world.tick_with_delta(1.0 / 60.0).unwrap();

        let q = world.query::<(&Position, &Velocity)>();
        println!("Frame {}:", frame);
        q.for_each_with_entity(|entity, (pos, vel)| {
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
