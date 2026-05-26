//! Minimal persistence example.
//!
//! ```bash
//! cargo run --example scene_basic --features scene
//! ```

use sky_engine::ecs::World;
use sky_engine::math::Transform;
use sky_engine::scene::{persist, Name, Persistence};

#[derive(Debug, Default)]
#[allow(dead_code)]
struct RuntimeCache {
    last_hit_frame: u64,
}

#[persist(component)]
#[derive(Debug)]
#[allow(dead_code)]
struct Stats {
    hp: u32,
    speed: f32,
    #[persist(default)]
    mana: u32,
    #[persist(skip)]
    cache: RuntimeCache,
}

fn main() {
    let persistence = Persistence::auto("game").expect("persistent component registry");

    let mut world = World::new();
    world.spawn((
        Name::new("Player"),
        Transform::from_xy(2.0, 3.0),
        Stats {
            hp: 100,
            speed: 3.5,
            mana: 12,
            cache: RuntimeCache { last_hit_frame: 42 },
        },
    ));

    let document = persistence
        .capture_world(&mut world)
        .expect("world should capture");
    println!("{}", document.to_json_string_pretty().unwrap());

    persistence
        .save_world(&mut world, "save.sky")
        .expect("world should save");

    let mut loaded_world = World::new();
    let loaded = persistence
        .load_world(&mut loaded_world, "save.sky")
        .expect("world should load");
    let loaded_player = loaded.roots[0];

    println!(
        "loaded {:?}: name={:?}, stats={:?}",
        loaded_player,
        loaded_world.get::<Name>(loaded_player).map(Name::as_str),
        loaded_world.get::<Stats>(loaded_player)
    );
}
