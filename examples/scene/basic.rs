//! Minimal AI-first scene/save example.
//!
//! ```bash
//! cargo run --example scene_basic --features scene
//! ```

use serde::{Deserialize, Serialize};

use sky_engine::ecs::World;
use sky_engine::math::Transform;
use sky_engine::scene::{despawn_scene_instance, Name, SceneDocument, SceneRuntime};

#[derive(Debug, Serialize, Deserialize)]
struct Stats {
    hp: u32,
    speed: f32,
}

fn main() {
    let mut world = World::new();
    let mut scenes = SceneRuntime::new();
    scenes
        .component_as::<Stats>("game.Stats")
        .expect("component type should register");

    let room = scenes
        .spawn_root(
            &mut world,
            "room_root",
            (Name::new("Room"), Transform::from_xy(0.0, 0.0)),
        )
        .expect("root should spawn");
    let lamp = scenes
        .spawn_child(
            &mut world,
            room,
            "lamp",
            (
                Name::new("Lamp"),
                Transform::from_xy(32.0, 16.0),
                Stats { hp: 25, speed: 0.0 },
            ),
        )
        .expect("child should spawn");

    world.get_mut::<Stats>(lamp).unwrap().hp = 10;

    let scene = scenes
        .capture_scene(&world, [room])
        .expect("scene should capture");
    let json = scene
        .to_json_string_pretty()
        .expect("scene should serialize");
    println!("{json}");

    let roundtrip = SceneDocument::from_json_str(&json).expect("scene JSON should parse");
    let mut loaded_world = World::new();
    let loaded = scenes
        .spawn_scene(&mut loaded_world, &roundtrip)
        .expect("scene should load");
    let loaded_lamp = loaded
        .entity_by_str("lamp")
        .expect("lamp node should map to an entity");

    println!(
        "loaded lamp {:?}: name={:?}, stats={:?}",
        loaded_lamp,
        loaded_world.get::<Name>(loaded_lamp).map(Name::as_str),
        loaded_world.get::<Stats>(loaded_lamp)
    );

    despawn_scene_instance(&mut loaded_world, loaded);
    println!("remaining loaded entities: {}", loaded_world.entity_count());
}
