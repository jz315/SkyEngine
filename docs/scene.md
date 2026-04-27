# SkyEngine Scene / Save

`scene` 是可选模块，用来给 ECS 实体树提供稳定文档 ID、层级、名字和 JSON 保存/加载。主路径是 AI-first 的 `SceneRuntime`：ECS 组件保持普通 Rust 类型，保存能力放在 ECS 外部注册。

启用：

```bash
cargo test --features scene
cargo run --example scene_basic --features scene
```

## 推荐用法：SceneRuntime

自定义组件只需要 serde，不需要 `Default`，不需要手写字段反射：

```rust
use serde::{Deserialize, Serialize};
use sky_engine::ecs::World;
use sky_engine::math::Transform;
use sky_engine::scene::{Name, SceneRuntime};

#[derive(Serialize, Deserialize)]
struct Stats {
    hp: u32,
    speed: f32,
}

let mut world = World::new();
let mut scenes = SceneRuntime::new();
scenes.component_as::<Stats>("game.Stats")?;

let player = scenes.spawn_root(
    &mut world,
    "player",
    (
        Name::new("Player"),
        Transform::from_xy(0.0, 0.0),
        Stats { hp: 100, speed: 3.5 },
    ),
)?;

scenes.save_scene_file(&world, [player], "save.scene.json")?;
let instance = scenes.load_scene_file(&mut world, "save.scene.json")?;
# Ok::<(), sky_engine::scene::SceneError>(())
```

`component::<T>()` 会用 `std::any::type_name::<T>()` 作为默认组件名；长期存档建议用 `component_as::<T>("game.Stats")`，这样 Rust 模块改名不会破坏 JSON。

## JSON Format

新保存格式把组件放在对象 map 里，稳定、可读、适合 AI 修改：

```json
{
  "version": 1,
  "roots": [
    {
      "id": "player",
      "name": "Player",
      "components": {
        "game.Stats": {
          "hp": 100,
          "speed": 3.5
        },
        "sky.Transform": {
          "position": [0.0, 0.0, 0.0],
          "rotation_z": 0.0,
          "scale": [1.0, 1.0, 1.0]
        }
      }
    }
  ]
}
```

scene 文件只使用这一种 map 结构。

## Runtime API

`SceneRuntime` 提供：

- `component::<T>()` / `component_as::<T>(name)`：注册可保存/加载的 serde 组件。
- `spawn_root(...)` / `spawn_child(...)`：普通 ECS spawn 之外追加 `SceneEntity`、`SceneRoot`、`Parent`、`Children` 元数据。
- `capture_scene(...)`：从 runtime entities 生成 `SceneDocument`。
- `spawn_scene(...)`：从 `SceneDocument` 生成 ECS 实体树。
- `save_scene_file(...)` / `load_scene_file(...)`：文件级保存/加载。

被 capture 的实体必须有 `SceneEntity`，也就是稳定文档 ID。通过 `SceneRuntime::spawn_root/spawn_child`、`spawn_scene`、`spawn_prefab` 创建的实体都会有这个 ID。

## 校验和失败语义

读取或 spawn 前会校验：

- `SceneEntityId` 不能为空。
- 同一个文档里 `SceneEntityId` 不能重复。
- 同一个 node 不能有重复组件类型。
- JSON 中的组件类型必须已经在 `SceneRuntime` 注册，`sky.Transform` 除外。
- serde 解码失败会返回 `SceneError::ComponentSerde`。

`SceneRuntime::spawn_scene` 会先验证整份文档；如果 spawn 过程中遇到错误，会清理本次已经创建的实体，避免留下半个场景。

## Document / Prefab Helpers

`SceneDocument` / `SceneNode` builder、`spawn_scene`、`spawn_prefab`、`despawn_scene_instance`、`despawn_prefab_instance` 仍可用。它们只处理 scene 元数据和 `sky.Transform`；自定义组件保存/加载走 `SceneRuntime`。Prefab v1 复用 `SceneNode`：

```rust
use sky_engine::math::Transform;
use sky_engine::scene::{spawn_prefab, PrefabDocument, PrefabSpawnOptions, SceneNode};

let enemy = PrefabDocument::new(
    SceneNode::new("enemy")
        .named("Enemy")
        .with_transform(Transform::from_xy(0.0, 0.0))
        .with_child(SceneNode::new("weapon")),
);

let first = spawn_prefab(&mut world, &enemy, PrefabSpawnOptions::new())?;
let second = spawn_prefab(
    &mut world,
    &enemy,
    PrefabSpawnOptions::new().with_root_transform(Transform::from_xy(100.0, 50.0)),
)?;
# Ok::<(), sky_engine::scene::SceneError>(())
```

## Feature Gate

`scene` 不依赖 `app`、`render` 或 `physics`。后续可在对应 feature 下扩展：

- `scene + app`：Sprite、Camera、Tilemap 等渲染组件数据。
- `scene + physics`：RigidBody、Collider、Velocity 等物理组件数据。
