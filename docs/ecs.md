# SkyEngine ECS

`sky_engine::ecs` 是 SkyEngine 的核心数据层。它负责实体生命周期、组件存储、资源、查询、延迟命令和系统调度。当前设计是 archetype + chunked columnar storage：同一组件组合的实体放在同一 archetype，组件按列存储在 chunk 中，查询时尽量批量访问连续内存。

## 导出

```rust
use sky_engine::ecs::{
    component_type, Bundle, Commands, ComponentType, EntityId, PreparedQuery, System, Time, With,
    Without, World,
};
```

底层工具 API 位于：

```rust
use sky_engine::ecs::{dynamic, expert};
```

## 快速上手

```rust
use sky_engine::ecs::*;

#[derive(Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Velocity {
    x: f32,
    y: f32,
}

let mut world = World::new();

let entity = world.spawn((
    Position { x: 0.0, y: 0.0 },
    Velocity { x: 1.0, y: 0.5 },
));

let mut query = world.query::<(&mut Position, &Velocity)>();
query.for_each(&mut world, |(position, velocity)| {
    position.x += velocity.x;
    position.y += velocity.y;
});

assert!(world.contains(entity));
```

## World

`World` 是实体、组件、资源和系统 schedule 的所有者。

```rust
World::new() -> World
```

实体生命周期：

```rust
world.spawn(bundle) -> EntityId
world.spawn_batch(bundles)

world.contains(entity) -> bool
world.entity_count() -> usize
world.archetype_count() -> usize

world.has::<T>(entity) -> bool
world.get::<T>(entity) -> Option<&T>
world.get_mut::<T>(entity) -> Option<&mut T>

world.insert(entity, component) -> bool
world.remove::<T>(entity) -> bool
world.despawn(entity) -> bool

world.clear()
```

语义：

- `spawn` 立即创建实体并返回 `EntityId`。
- `spawn_batch` 批量创建同一 bundle 类型的实体，不返回每个 ID。
- `insert/remove` 会迁移实体到新的 archetype。
- `despawn/remove/clear/World drop` 会正确执行非 `Copy` 组件析构。
- `clear` 清空实体和组件，但保留 resources。

## EntityId

`EntityId` 是 generational handle。

```rust
entity.index() -> u32
entity.generation() -> u32
```

实体被 `despawn` 后，旧 ID 会因为 generation 不匹配而失效。即使底层 slot 被复用，旧 ID 也不会误指向新实体。

## Bundle

实体创建使用 tuple bundle：

```rust
world.spawn((Position { x: 0.0, y: 0.0 },));
world.spawn((
    Position { x: 0.0, y: 0.0 },
    Velocity { x: 1.0, y: 1.0 },
));
```

约束：

- 组件类型必须 `'static`。
- 单元素 tuple 需要尾逗号。
- 当前 tuple bundle 支持到 8 个组件。
- 同一个 bundle 里不允许重复组件类型。

Bundle metadata 会缓存，所以常规 `world.spawn((A, B, C))` 不需要用户手动注册 archetype。

## Resources

资源是 world 级单例，不参与 archetype。

```rust
world.insert_resource(resource) -> Option<R>
world.get_resource::<R>() -> Option<&R>
world.get_resource_mut::<R>() -> Option<&mut R>
world.contains_resource::<R>() -> bool
world.remove_resource::<R>() -> Option<R>
```

适合放输入状态、全局配置、asset server、physics world、game state 等。

## PreparedQuery

推荐使用 typed prepared query：

```rust
let mut query = world.query::<(&mut Position, &Velocity)>();
let mut filtered = world.query_filtered::<&Position, With<Velocity>>();
```

`PreparedQuery<Q, Flt>` 会缓存匹配 archetype，并在 `World::archetype_epoch()` 变化时刷新。它是常规运行时系统的主路径。

支持参数：

| 写法 | 含义 |
| --- | --- |
| `&T` | 只读组件 |
| `&mut T` | 可写组件 |
| `Option<&T>` | 可选只读组件 |
| `Option<&mut T>` | 可选可写组件 |

tuple query 当前支持 1 到 8 个组件位。重复组件类型会 panic，这是为了避免 aliasing 语义不清。

遍历接口：

```rust
query.for_each(&mut world, |item| { ... });
query.for_each_chunk(&mut world, |chunk| { ... });
query.par_for_each_chunk(&mut world, |chunk| { ... });
query.for_each_with_entity(&mut world, |entity, item| { ... });
query.for_each_chunk_with_entities(&mut world, |entities, chunk| { ... });
query.par_for_each_chunk_with_entities(&mut world, |entities, chunk| { ... });

query.count(&world) -> usize
query.is_empty(&world) -> bool
query.cached_archetype_count() -> usize
```

只读 query 也可以传 `&world`；包含 `&mut T` 或 `Option<&mut T>` 的 query 必须传 `&mut world`。

按实体遍历：

```rust
let mut query = world.query::<(&mut Position, &Velocity)>();

query.for_each(&mut world, |(position, velocity)| {
    position.x += velocity.x;
    position.y += velocity.y;
});
```

按 chunk 遍历：

```rust
query.for_each_chunk(&mut world, |(positions, velocities)| {
    for index in 0..positions.len() {
        positions[index].x += velocities[index].x;
        positions[index].y += velocities[index].y;
    }
});
```

并行 chunk 遍历：

```rust
let dt = 0.016;

query.par_for_each_chunk(&mut world, |(positions, velocities)| {
    for index in 0..positions.len() {
        positions[index].x += velocities[index].x * dt;
        positions[index].y += velocities[index].y * dt;
    }
});
```

并行约束：

- chunk 执行顺序未定义。
- 闭包不能直接做结构修改。
- 闭包不应捕获可变外部状态；需要共享结果时使用原子、锁，或先收集再顺序应用。
- 如果需要 resource 输入，建议在并行前复制出轻量 owned 数据。

## Filters

可用 filter：

```rust
With<T>
Without<T>
()
```

支持 tuple AND 组合：

```rust
(With<A>, Without<B>)
(With<A>, With<B>, Without<C>)
(With<A>, With<B>, Without<C>, Without<D>)
```

示例：

```rust
let mut enemies = world.query_filtered::<&Position, With<Enemy>>();
let mut moving_alive =
    world.query_filtered::<(&mut Position, &Velocity), (With<Velocity>, Without<Dead>)>();
```

## Commands

`Commands` 用于延迟结构修改和资源修改，尤其适合 active query 中安排 spawn/despawn/insert/remove。

```rust
let mut commands = Commands::new();
```

接口：

```rust
commands.spawn(bundle)
commands.despawn(entity)
commands.insert(entity, component)
commands.remove::<T>(entity)

commands.insert_resource(resource)
commands.remove_resource::<R>()

commands.apply(&mut world)
commands.is_empty() -> bool
commands.len() -> usize
```

示例：

```rust
let mut commands = Commands::new();
let mut query = world.query::<&Health>();

query.for_each_with_entity(&world, |entity, health| {
    if health.hp <= 0.0 {
        commands.despawn(entity);
        commands.spawn((Loot { value: 10 },));
    }
});

commands.apply(&mut world);
```

语义：

- 相邻同 bundle 类型的 `spawn` 会合并成批量创建。
- 同一实体同一批次里的重复 `insert/remove` 会折叠成最终状态。
- `despawn` 会吞掉该实体后续同批次组件修改。
- command queue 保留 first-seen entity 顺序。

## 系统与调度

SkyEngine 的 schedule 内置在 `World` 中。

```rust
pub trait System: 'static {
    fn init(&mut self, _world: &mut World) {}
    fn run(&mut self, world: &mut World);
    fn teardown(&mut self, _world: &mut World) {}
}
```

闭包也能作为系统：

```rust
world.group("sim").add(|world: &mut World| {
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&mut *world, |(position, velocity)| {
        position.x += velocity.x;
    });
});
```

分组：

```rust
world.group("sim").add(MySystem);
world.group("render").add(|world: &mut World| { ... });
world.group("physics").fixed(0.02).add(PhysicsSystem);
```

规则：

- group 按创建顺序运行。
- 默认每帧运行一次。
- `.fixed(dt)` 会创建固定步长 group，可能一帧内运行多次。
- `world.shutdown()` 会逆序调用已初始化系统的 `teardown`。

驱动：

```rust
world.tick();
world.tick_with_delta(0.016);
world.tick_with_frame_delta(frame_delta, raw_delta);
world.shutdown();
```

`World::time`：

```rust
world.time.delta
world.time.frame_delta
world.time.raw_delta
world.time.elapsed
world.time.raw_elapsed
world.time.frame_count
world.time.time_scale
```

## ComponentType 和 Reflect 基础层

ECS 的组件 layout 使用 `reflect::Type` 的语义别名：

```rust
use sky_engine::ecs::{component_type, ComponentType};

let ty: ComponentType = component_type::<Position>();
```

这层只包含类型名、size、align、drop 函数和 Rust `TypeId`。它不是 Inspector 字段反射，不会要求用户组件 `#[derive(Reflect)]`。

## dynamic API

`ecs::dynamic` 适合动态脚本、工具链和编辑器接入。它不是第二套 ECS，只是在同一套 storage kernel 上提供运行期类型检查。

动态创建实体：

```rust
use sky_engine::ecs::{
    dynamic::{DynamicBundle, WorldDynamicExt},
    World,
};

let mut world = World::new();
let entity = world.spawn_dynamic(
    DynamicBundle::new()
        .with(Position { x: 0.0, y: 0.0 })
        .with(Velocity { x: 1.0, y: 0.0 }),
)?;
```

动态查询：

```rust
use sky_engine::ecs::dynamic::DynamicQuery;

let mut query = DynamicQuery::builder()
    .write::<Position>()
    .optional_read::<Velocity>()
    .build()?;

query.for_each_chunk_mut(&mut world, |mut chunk| {
    let (positions, velocities) = chunk.write_optional_read::<Position, Velocity>(0, 1)?;
    if let Some(velocities) = velocities {
        for (position, velocity) in positions.iter_mut().zip(velocities) {
            position.x += velocity.x;
            position.y += velocity.y;
        }
    }
    Ok(())
})?;
```

只读动态 query 可以传 `&World`。包含 write slot 的动态 query 必须传 `&mut World`。

## expert API

`ecs::expert` 是 unsafe 底层入口，适合 benchmark、引擎内部工具和明确需要维护初始化/aliasing 契约的代码。一般游戏逻辑不应该使用它。

手工构建 archetype：

```rust
use sky_engine::ecs::expert::create_archetype;

let archetype = create_archetype()
    .add_rust_component::<Position>()
    .add_rust_component::<Velocity>()
    .build();
```

低层实体创建：

```rust
use sky_engine::ecs::expert::WorldExpertExt;

let entity = unsafe { world.spawn_uninit(archetype) };
```

`spawn_uninit` 只创建槽位，不自动初始化组件数据。调用者必须在实体被查询、迁移、删除或 drop 前初始化每个组件列。

也可以按 `ComponentType` 添加：

```rust
use sky_engine::ecs::{component_type, expert::create_archetype};

let archetype = create_archetype()
    .add_component(component_type::<Position>())
    .add_component(component_type::<Velocity>())
    .build();
```

## 推荐和边界

推荐：

- 普通运行时逻辑用 `world.query::<Q>()`。
- 需要结构变化时用 `Commands`。
- 批量创建优先 `spawn_batch` 或连续 `commands.spawn`。
- 热系统把 `PreparedQuery` 缓存在 system struct 中。
- 工具/脚本桥用 `ecs::dynamic`，benchmark/底层实验才用 `ecs::expert`。

不推荐：

- active query 中直接结构修改。
- 把 dynamic/expert query 当主运行时 API。
- 让工具层 Inspector 反射进入 ECS 热路径。

## 测试

```bash
cargo test
cargo test ecs
```
