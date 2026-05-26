use crate::ecs::World;
use crate::math::Transform;
use crate::persist;

use super::*;

#[derive(Debug, Default, PartialEq)]
struct RuntimeCache {
    marker: u32,
}

#[persist(component)]
#[derive(Debug, PartialEq)]
struct PersistStats {
    hp: u32,
    #[persist(default)]
    mana: u32,
    #[persist(skip)]
    cache: RuntimeCache,
}

#[persist(component)]
#[derive(Debug, PartialEq)]
struct PersistWeapon {
    damage: u32,
}

#[test]
fn persistence_auto_saves_world_without_manual_entity_ids() {
    let persistence = Persistence::auto("game").unwrap();
    let mut world = World::new();
    let _player = world.spawn((
        Name::new("Player"),
        Transform::from_xy(2.0, 3.0),
        PersistStats {
            hp: 80,
            mana: 12,
            cache: RuntimeCache { marker: 99 },
        },
    ));

    let document = persistence.capture_world(&mut world).unwrap();
    let json = document.to_json_string_pretty().unwrap();

    assert!(json.contains("\"game.PersistStats\""));
    assert!(json.contains("\"hp\": 80"));
    assert!(json.contains("\"mana\": 12"));
    assert!(!json.contains("marker"));

    let mut loaded_world = World::new();
    let loaded = persistence
        .spawn_world(&mut loaded_world, &document)
        .unwrap();
    let loaded_player = loaded.roots[0];

    assert_eq!(
        loaded_world.get::<PersistStats>(loaded_player).unwrap(),
        &PersistStats {
            hp: 80,
            mana: 12,
            cache: RuntimeCache::default(),
        }
    );
    assert_eq!(
        loaded_world.get::<Name>(loaded_player).unwrap().as_str(),
        "Player"
    );
}

#[test]
fn persistence_default_field_loads_old_documents() {
    let persistence = Persistence::auto("game").unwrap();
    let document = PersistDocument::from_json_str(
        r#"{
            "roots": [{
                "id": "player",
                "components": {
                    "game.PersistStats": { "hp": 25 }
                }
            }]
        }"#,
    )
    .unwrap();

    let mut world = World::new();
    let loaded = persistence.spawn_world(&mut world, &document).unwrap();
    let player = loaded.roots[0];

    assert_eq!(
        world.get::<PersistStats>(player).unwrap(),
        &PersistStats {
            hp: 25,
            mana: 0,
            cache: RuntimeCache::default(),
        }
    );
}

#[test]
fn persistence_prefab_saves_entity_subtree() {
    let persistence = Persistence::auto("game").unwrap();
    let mut world = World::new();
    let player = world.spawn((
        Name::new("Player"),
        PersistStats {
            hp: 40,
            mana: 8,
            cache: RuntimeCache::default(),
        },
    ));
    let sword = world.spawn((
        Name::new("Sword"),
        Parent::new(player),
        PersistWeapon { damage: 7 },
    ));
    world.insert(player, Children::new(vec![sword]));

    let document = persistence.capture_prefab(&mut world, player).unwrap();
    let json = document.to_json_string_pretty().unwrap();
    assert!(json.contains("\"Player\""));
    assert!(json.contains("\"Sword\""));
    assert!(json.contains("\"game.PersistWeapon\""));

    let mut loaded_world = World::new();
    let loaded = persistence
        .spawn_world(&mut loaded_world, &document)
        .unwrap();
    let loaded_player = loaded.roots[0];
    let loaded_sword = loaded_world
        .get::<Children>(loaded_player)
        .unwrap()
        .as_slice()[0];

    assert_eq!(
        loaded_world.get::<PersistWeapon>(loaded_sword).unwrap(),
        &PersistWeapon { damage: 7 }
    );
    assert_eq!(
        loaded_world.get::<Parent>(loaded_sword).unwrap().entity(),
        loaded_player
    );
}

#[test]
fn persistence_rejects_unknown_component_before_spawning() {
    let persistence = Persistence::auto("game").unwrap();
    let document = PersistDocument::from_json_str(
        r#"{
            "roots": [{
                "id": "player",
                "components": {
                    "game.Missing": { "hp": 1 }
                }
            }]
        }"#,
    )
    .unwrap();

    let mut world = World::new();
    let error = persistence.spawn_world(&mut world, &document).unwrap_err();

    assert_eq!(
        error,
        PersistError::UnregisteredComponentType("game.Missing".to_string())
    );
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn persistence_document_validates_duplicate_component_keys() {
    let error = PersistDocument::from_json_str(
        r#"{
            "roots": [{
                "id": "player",
                "components": {
                    "game.PersistStats": { "hp": 1 },
                    "game.PersistStats": { "hp": 2 }
                }
            }]
        }"#,
    )
    .unwrap_err();

    assert_eq!(
        error,
        PersistError::DuplicatePersistComponent {
            entity: PersistId::from("player"),
            type_name: "game.PersistStats".to_string(),
        }
    );
}
