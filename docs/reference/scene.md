# SkyEngine Persistence

`scene` feature 提供 ECS 数据的持久化能力：保存整个 World 的可持久化部分，或者保存一个实体子树作为 prefab。主路径不是反射，也不是手写 JSON，而是在类型和字段旁边声明保存意图。

启用：

```bash
cargo test --features scene
cargo run --example scene_basic --features scene
```

## 推荐用法

```rust
use sky_engine::ecs::World;
use sky_engine::math::Transform;
use sky_engine::scene::{persist, Name, Persistence};

#[derive(Default)]
struct RuntimeCache;

#[persist(component)]
struct PlayerState {
    hp: u32,

    #[persist(default)]
    mana: u32,

    #[persist(skip)]
    cache: RuntimeCache,
}

let persistence = Persistence::auto("game")?;

let mut world = World::new();
world.spawn((
    Name::new("Player"),
    Transform::from_xy(0.0, 0.0),
    PlayerState {
        hp: 100,
        mana: 0,
        cache: RuntimeCache,
    },
));

persistence.save_world(&mut world, "slot_01.save")?;
let loaded = persistence.load_world(&mut world, "slot_01.save")?;
# Ok::<(), sky_engine::scene::PersistError>(())
```

## API 含义

`#[persist(component)]`：这个类型作为 ECS component 参与保存/加载。宏会生成 serde 支持、持久化元数据，并自动注册到 `Persistence::auto(namespace)`。

`#[persist(skip)]`：字段不写入文件，加载时使用该字段类型的 `Default::default()`。适合 runtime cache、GPU handle、临时状态。

`#[persist(default)]`：加载旧文件时，如果字段缺失，使用 `Default::default()`。适合后续新增字段。

`Persistence::auto("game")`：收集所有 `#[persist(component)]` 类型。默认组件名是 `{namespace}.{TypeName}`，例如 `game.PlayerState`。

## 保存范围

保存整个 World：

```rust
persistence.save_world(&mut world, "slot_01.save")?;
let loaded = persistence.load_world(&mut world, "slot_01.save")?;
```

`save_world` 保存所有带可持久化组件的实体。实体的持久 ID 由引擎自动生成；运行时 `EntityId` 不会直接写进长期文件。`load_world` 会清空当前实体并加载文件，资源保持为 `World::clear()` 的语义。

保存 prefab：

```rust
persistence.save_prefab(&mut world, enemy, "enemy.prefab")?;
let enemy = persistence.load_prefab(&mut world, "enemy.prefab")?;
let root = enemy.root();
```

`save_prefab` 保存传入实体以及它的 `Children` 子树。这个入口适合保存角色、敌人、建筑、编辑器选中对象。

## 文档中间层

普通用户可以直接用 `save_world` / `load_world`。工具、编辑器和测试可以使用内存文档：

```rust
let doc = persistence.capture_world(&mut world)?;
doc.write_json_file("slot_01.save")?;

let doc = sky_engine::scene::PersistDocument::from_json_file("slot_01.save")?;
persistence.spawn_world(&mut world, &doc)?;
# Ok::<(), sky_engine::scene::PersistError>(())
```

这层把“从 ECS 捕获数据”和“写入文件格式”分开，方便验证、编辑器修改、AI 修复和未来多格式输出。

## 文件形状

保存后的 JSON 仍然是人类可读的实体树：

```json
{
  "version": 1,
  "roots": [
    {
      "id": "0b7d9f2b-6a6f-4af7-8b41-2d2bcb8f0f2b",
      "name": "Player",
      "components": {
        "game.PlayerState": {
          "hp": 100,
          "mana": 0
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

## 设计原则

- 用户声明数据意图：这个组件保存、这个字段跳过、这个新增字段有默认值。
- 引擎处理机制：注册、稳定类型名、实体持久 ID、JSON 组件表、加载校验。
- API 只表达保存范围：`world` 或 `prefab`，不提供模糊的场景保存入口。
