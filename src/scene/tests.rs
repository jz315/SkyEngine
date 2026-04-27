use crate::ecs::World;
use crate::math::Transform;

use super::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct RuntimeStats {
    hp: u32,
    speed: f32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct AutoNamed {
    value: u32,
}

#[test]
fn spawn_scene_creates_hierarchy_and_id_mapping() {
    let mut world = World::new();
    let scene = SceneDocument::new().with_root(
        SceneNode::new("root")
            .named("Room")
            .with_transform(Transform::from_xy(10.0, 20.0))
            .with_child(
                SceneNode::new("child")
                    .named("Lamp")
                    .with_transform(Transform::from_xy(3.0, 4.0)),
            ),
    );

    let instance = spawn_scene(&mut world, &scene).unwrap();
    let root = instance.entity_by_str("root").unwrap();
    let child = instance.entity_by_str("child").unwrap();

    assert_eq!(instance.roots, vec![root]);
    assert_eq!(instance.entities, vec![root, child]);
    assert_eq!(world.get::<Name>(root).unwrap().as_str(), "Room");
    assert_eq!(world.get::<Name>(child).unwrap().as_str(), "Lamp");
    assert_eq!(world.get::<Transform>(root).unwrap().position.x(), 10.0);
    assert_eq!(world.get::<Transform>(child).unwrap().position.y(), 4.0);
    assert!(world.has::<SceneRoot>(root));
    assert!(!world.has::<SceneRoot>(child));
    assert_eq!(world.get::<Parent>(child).unwrap().entity(), root);
    assert_eq!(world.get::<Children>(root).unwrap().as_slice(), &[child]);
}

#[test]
fn despawn_scene_instance_removes_all_spawned_entities() {
    let mut world = World::new();
    let scene = SceneDocument::new().with_root(
        SceneNode::new("root")
            .with_child(SceneNode::new("a"))
            .with_child(SceneNode::new("b")),
    );
    let instance = spawn_scene(&mut world, &scene).unwrap();
    let entities = instance.entities.clone();

    despawn_scene_instance(&mut world, instance);

    assert!(entities.into_iter().all(|entity| !world.contains(entity)));
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn duplicate_scene_ids_are_rejected_before_spawning() {
    let mut world = World::new();
    let scene = SceneDocument::new()
        .with_root(SceneNode::new("same"))
        .with_root(SceneNode::new("same"));

    let error = spawn_scene(&mut world, &scene).unwrap_err();

    assert_eq!(
        error,
        SceneError::DuplicateSceneEntityId(SceneEntityId::from("same"))
    );
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn duplicate_transform_component_is_rejected() {
    let json = r#"{
        "roots": [{
            "id": "root",
            "components": {
                "sky.Transform": { "position": [1.0, 2.0, 0.0] },
                "sky.Transform": { "position": [3.0, 4.0, 0.0] }
            }
        }]
    }"#;

    let error = SceneDocument::from_json_str(json).unwrap_err();

    assert_eq!(
        error,
        SceneError::DuplicateTransform(SceneEntityId::from("root"))
    );
}

#[test]
fn prefab_spawn_can_override_root_transform_and_spawn_multiple_instances() {
    let mut world = World::new();
    let prefab = PrefabDocument::new(
        SceneNode::new("enemy")
            .named("Enemy")
            .with_transform(Transform::from_xy(1.0, 2.0))
            .with_child(SceneNode::new("weapon")),
    );

    let first = spawn_prefab(&mut world, &prefab, PrefabSpawnOptions::new()).unwrap();
    let second = spawn_prefab(
        &mut world,
        &prefab,
        PrefabSpawnOptions::new().with_root_transform(Transform::from_xy(100.0, 50.0)),
    )
    .unwrap();

    assert_ne!(first.root, second.root);
    assert_eq!(
        world.get::<Transform>(first.root).unwrap().position.x(),
        1.0
    );
    assert_eq!(
        world.get::<Transform>(second.root).unwrap().position.x(),
        100.0
    );
    assert_eq!(
        world.get::<Transform>(second.root).unwrap().position.y(),
        50.0
    );
    assert_ne!(
        first.entity_by_str("weapon").unwrap(),
        second.entity_by_str("weapon").unwrap()
    );

    despawn_prefab_instance(&mut world, first);
    assert!(world.contains(second.root));
    despawn_prefab_instance(&mut world, second);
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn scene_json_roundtrips_and_spawns() {
    let mut world = World::new();
    let scene = SceneDocument::new().named("json_room").with_root(
        SceneNode::new("root")
            .named("Root")
            .with_transform(Transform::from_xyz(1.0, 2.0, 3.0).with_scale3(2.0, 3.0, 1.0))
            .with_child(SceneNode::new("child").with_transform(Transform::from_xy(4.0, 5.0))),
    );

    let json = scene.to_json_string_pretty().unwrap();
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"sky.Transform\": {"));

    let decoded = SceneDocument::from_json_str(&json).unwrap();
    assert_eq!(decoded.name.as_deref(), Some("json_room"));
    let instance = spawn_scene(&mut world, &decoded).unwrap();
    let root = instance.entity_by_str("root").unwrap();
    let transform = world.get::<Transform>(root).unwrap();

    assert_eq!(transform.position.to_array(), [1.0, 2.0, 3.0]);
    assert_eq!(transform.scale.to_array(), [2.0, 3.0, 1.0]);
}

#[test]
fn scene_runtime_roundtrips_serde_component_without_reflection() {
    let mut world = World::new();
    let mut scenes = SceneRuntime::new();
    scenes
        .component_as::<RuntimeStats>("game.RuntimeStats")
        .unwrap();

    let player = scenes
        .spawn_root(
            &mut world,
            "player",
            (
                Name::new("Player"),
                Transform::from_xy(2.0, 3.0),
                RuntimeStats {
                    hp: 100,
                    speed: 3.5,
                },
            ),
        )
        .unwrap();

    world.get_mut::<RuntimeStats>(player).unwrap().hp = 12;
    let scene = scenes.capture_scene(&world, [player]).unwrap();
    let json = scene.to_json_string_pretty().unwrap();

    assert!(json.contains("\"components\": {"));
    assert!(json.contains("\"game.RuntimeStats\": {"));
    assert!(json.contains("\"hp\": 12"));
    assert!(!json.contains("\"type\":"));

    let mut loaded_world = World::new();
    let instance = scenes.spawn_scene(&mut loaded_world, &scene).unwrap();
    let loaded_player = instance.entity_by_str("player").unwrap();

    assert_eq!(
        loaded_world.get::<RuntimeStats>(loaded_player).unwrap(),
        &RuntimeStats { hp: 12, speed: 3.5 }
    );
    assert_eq!(
        loaded_world.get::<Name>(loaded_player).unwrap().as_str(),
        "Player"
    );
    assert_eq!(
        loaded_world
            .get::<Transform>(loaded_player)
            .unwrap()
            .position
            .to_array(),
        [2.0, 3.0, 0.0]
    );
}

#[test]
fn scene_runtime_component_uses_rust_type_name_by_default() {
    let mut world = World::new();
    let mut scenes = SceneRuntime::new();
    scenes.component::<AutoNamed>().unwrap();

    let entity = scenes
        .spawn_root(&mut world, "auto", (AutoNamed { value: 7 },))
        .unwrap();
    let scene = scenes.capture_scene(&world, [entity]).unwrap();
    let json = scene.to_json_string_pretty().unwrap();

    assert!(json.contains(std::any::type_name::<AutoNamed>()));
}

#[test]
fn scene_runtime_spawn_root_and_child_write_hierarchy_metadata() {
    let mut world = World::new();
    let scenes = SceneRuntime::new();

    let root = scenes
        .spawn_root(&mut world, "root", (Name::new("Root"),))
        .unwrap();
    let child = scenes
        .spawn_child(&mut world, root, "child", (Transform::from_xy(4.0, 5.0),))
        .unwrap();

    assert!(world.has::<SceneRoot>(root));
    assert!(!world.has::<SceneRoot>(child));
    assert_eq!(world.get::<SceneEntity>(root).unwrap().id.as_str(), "root");
    assert_eq!(
        world.get::<SceneEntity>(child).unwrap().id.as_str(),
        "child"
    );
    assert_eq!(world.get::<Parent>(child).unwrap().entity(), root);
    assert_eq!(world.get::<Children>(root).unwrap().as_slice(), &[child]);
}

#[test]
fn scene_runtime_rejects_unknown_component_before_spawning() {
    let mut world = World::new();
    let scenes = SceneRuntime::new();
    let scene = SceneDocument::from_json_str(
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

    let error = scenes.spawn_scene(&mut world, &scene).unwrap_err();

    assert_eq!(
        error,
        SceneError::UnregisteredComponentType("game.Missing".to_string())
    );
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn low_level_spawn_rejects_custom_components() {
    let mut world = World::new();
    let scene = SceneDocument::new().with_root(
        SceneNode::new("player").with_component(
            "game.RuntimeStats",
            SceneValue::object()
                .with_field("hp", 100u32)
                .with_field("speed", 3.5f32),
        ),
    );

    let error = spawn_scene(&mut world, &scene).unwrap_err();

    assert_eq!(
        error,
        SceneError::UnregisteredComponentType("game.RuntimeStats".to_string())
    );
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn scene_runtime_rejects_bad_serde_component_before_spawning() {
    let mut world = World::new();
    let mut scenes = SceneRuntime::new();
    scenes
        .component_as::<RuntimeStats>("game.RuntimeStats")
        .unwrap();
    let scene = SceneDocument::from_json_str(
        r#"{
            "roots": [{
                "id": "player",
                "components": {
                    "game.RuntimeStats": { "hp": "wrong", "speed": 3.5 }
                }
            }]
        }"#,
    )
    .unwrap();

    let error = scenes.spawn_scene(&mut world, &scene).unwrap_err();

    assert!(matches!(error, SceneError::ComponentSerde { .. }));
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn component_array_json_is_rejected() {
    let error = SceneDocument::from_json_str(
        r#"{
            "roots": [{
                "id": "player",
                "components": [
                    {
                        "type": "sky.Transform",
                        "value": {
                            "position": [1.0, 2.0, 3.0],
                            "rotation_z": 0.0,
                            "scale": [1.0, 1.0, 1.0]
                        }
                    }
                ]
            }]
        }"#,
    )
    .unwrap_err();

    assert!(matches!(error, SceneError::Json(_)));
}

#[test]
fn capture_requires_stable_scene_entity_ids() {
    let mut world = World::new();
    let entity = world.spawn((Transform::default(),));

    let error = capture_scene(&world, [entity]).unwrap_err();

    assert_eq!(error, SceneError::MissingSceneEntity(entity));
}

#[test]
fn prefab_json_roundtrips() {
    let prefab = PrefabDocument::new(
        SceneNode::new("crate")
            .named("Crate")
            .with_transform(Transform::from_xy(8.0, 9.0)),
    );

    let json = prefab.to_json_string().unwrap();
    let decoded = PrefabDocument::from_json_str(&json).unwrap();

    assert_eq!(decoded.root.id, SceneEntityId::from("crate"));
    assert_eq!(decoded.root.name.as_deref(), Some("Crate"));
}

#[test]
fn scene_json_load_validates_duplicate_ids() {
    let json = r#"{
        "roots": [
            { "id": "same" },
            { "id": "same" }
        ]
    }"#;

    let error = SceneDocument::from_json_str(json).unwrap_err();

    assert_eq!(
        error,
        SceneError::DuplicateSceneEntityId(SceneEntityId::from("same"))
    );
}

#[test]
fn scene_json_load_validates_duplicate_component_keys() {
    let json = r#"{
        "roots": [{
            "id": "player",
            "components": {
                "game.Stats": { "hp": 1, "speed": 1.0 },
                "game.Stats": { "hp": 2, "speed": 2.0 }
            }
        }]
    }"#;

    let error = SceneDocument::from_json_str(json).unwrap_err();

    assert_eq!(
        error,
        SceneError::DuplicateSceneComponent {
            entity: SceneEntityId::from("player"),
            type_name: "game.Stats".to_string(),
        }
    );
}
