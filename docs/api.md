# SkyEngine ECS API 文档

本文档对应当前仓库中的实际 API，重点覆盖：

- `World`：实体、组件、资源、调度入口
- `PreparedQuery`：推荐的高性能 typed query API
- `Commands`：延迟结构修改 API
- `raw`：动态查询和底层 archetype / entity 入口

SkyEngine 当前的设计重点是：

- 查询热路径优先：`world.query::<Q>() -> PreparedQuery<Q>`
- 存储为 chunked columnar SoA
- 结构修改支持直接执行，也支持通过 `Commands` 延迟到阶段末批量应用

---

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

fn main() {
    let mut world = World::new();

    let entity = world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 0.5 },
    ));

    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x;
        pos.y += vel.y;
    });

    assert!(world.contains(entity));
}
```

---

## 顶层导出

`sky_engine::ecs` 当前公开导出：

```rust
use sky_engine::ecs::{
    Bundle, Commands, EntityId, System, Time, With, Without, World,
};
```

底层 raw API：

```rust
use sky_engine::ecs::raw::{
    create_archetype, Archetype, ArchetypeBuilder, Chunk, PreparedQuery, Query, QueryIter,
    WorldRawExt,
};
```

---

## World

`World` 是实体、组件、资源和系统调度的中心。

```rust
World::new() -> World
```

### 实体生命周期

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

说明：

- `spawn()` 立即创建实体并返回 `EntityId`
- `spawn_batch()` 不返回 ID，适合纯批量创建
- `insert/remove` 会触发 archetype 迁移
- `clear()` 清空所有实体，但保留资源

### 资源

```rust
world.insert_resource(resource) -> Option<R>
world.get_resource::<R>() -> Option<&R>
world.get_resource_mut::<R>() -> Option<&mut R>
world.contains_resource::<R>() -> bool
world.remove_resource::<R>() -> Option<R>
```

资源是全局单例，不参与 archetype。

---

## EntityId

`EntityId` 是带代际的实体句柄，可以长期保存。

```rust
entity.index() -> u32
entity.generation() -> u32
```

实现了：

- `Clone`
- `Copy`
- `Debug`
- `PartialEq`
- `Eq`
- `Hash`

如果实体被 `despawn` 后重用同一槽位，旧 `EntityId` 会因 generation 不匹配而失效。

---

## Bundle

实体创建使用 tuple bundle。每个组件都必须满足：

- `Copy`
- `'static`

示例：

```rust
world.spawn((Position { x: 0.0, y: 0.0 },));
world.spawn((
    Position { x: 0.0, y: 0.0 },
    Velocity { x: 1.0, y: 1.0 },
));
```

注意单元素 tuple 末尾的逗号：

```rust
(Position { x: 0.0, y: 0.0 },)
```

当前 tuple bundle 支持到 8 个组件。

---

## 查询：PreparedQuery

### 推荐用法

推荐使用 typed prepared query：

```rust
let mut query = world.query::<(&mut Position, &Velocity)>();
let mut filtered = world.query_filtered::<&Position, With<Velocity>>();
```

`PreparedQuery<Q, Flt>` 会缓存匹配的 archetype，并在 `World` 的 archetype epoch 变化时自动刷新。

### Query 参数

支持的参数形式：

| 写法 | 含义 |
|------|------|
| `&T` | 只读组件 |
| `&mut T` | 可写组件 |
| `Option<&T>` | 可选只读组件 |
| `Option<&mut T>` | 可选可写组件 |

tuple query 目前支持 1 到 8 个组件位。

### 遍历接口

```rust
query.for_each(&world, |item| { ... });
query.for_each_chunk(&world, |chunk| { ... });
query.for_each_with_entity(&world, |entity, item| { ... });
query.for_each_chunk_with_entities(&world, |entities, chunk| { ... });

query.count(&world) -> usize
query.is_empty(&world) -> bool
query.cached_archetype_count() -> usize
```

示例：

```rust
let mut query = world.query::<(&mut Position, &Velocity)>();

query.for_each(&world, |(pos, vel)| {
    pos.x += vel.x;
    pos.y += vel.y;
});

query.for_each_chunk(&world, |(positions, velocities)| {
    for i in 0..positions.len() {
        positions[i].x += velocities[i].x;
        positions[i].y += velocities[i].y;
    }
});
```

### 过滤器

可用过滤器：

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
let mut moving_non_dead =
    world.query_filtered::<(&mut Position, &Velocity), (With<Velocity>, Without<Dead>)>();
```

### 查询期间的结构修改

`PreparedQuery` 的遍历 API 都接收 `&World`：

```rust
query.for_each(&world, ...);
```

而 `spawn / insert / remove / despawn` 需要 `&mut World`。

这意味着：

- 在正常 safe Rust 下，active query 期间不能直接做结构修改
- 如果你需要在查询中安排结构变化，应该使用 `Commands`

这也是当前推荐的系统写法。

---

## Commands

`Commands` 用于延迟结构修改和资源修改。

```rust
let mut commands = Commands::new();
```

### 可用操作

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

### 推荐场景

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

### 当前语义

`Commands` 当前保留队列顺序和 barrier 语义：

- entity 结构命令会按批次应用
- `spawn` 会自动合并相邻同 bundle 类型的批量创建
- 同一实体在同一批次中的重复 `insert/remove` 会折叠成最终状态
- `despawn` 会吞掉该实体后续同批次组件修改

### 何时用 Commands，何时直接改 World

| 场景 | 建议 |
|------|------|
| active query / system update 中需要结构修改 | 用 `Commands` |
| 迭代外立刻需要 `EntityId` | 用 `world.spawn()` |
| 纯批量创建，不需要 ID | 优先 `spawn_batch()` 或 `commands.spawn()` |
| 迭代外、单个直接修改 | 直接 `world.insert/remove/despawn` 即可 |

---

## 系统与调度

SkyEngine 当前没有公开独立的 `Schedule` 类型；调度器内置在 `World` 中。

### System trait

```rust
pub trait System: 'static {
    fn init(&mut self, _world: &mut World) {}
    fn run(&mut self, world: &mut World);
    fn teardown(&mut self, _world: &mut World) {}
}
```

闭包也可以直接作为系统：

```rust
world.group("sim").add(|world: &mut World| {
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x;
    });
});
```

### 分组 API

```rust
world.group("sim").add(MySystem);
world.group("render").add(|world: &mut World| { ... });
world.group("physics").fixed(0.02).add(PhysicsSystem);
```

说明：

- `group(name)` 会查找或创建同名分组
- group 按创建顺序运行
- 默认每帧运行一次
- `.fixed(dt)` 会把该组改成固定步长组

### 驱动调度

```rust
world.tick();                  // 使用真实时间差
world.tick_with_delta(0.016);  // 指定 delta
world.shutdown();              // 逆序 teardown
```

### Time

`World` 暴露 `time: Time`：

```rust
world.time.delta
world.time.elapsed
world.time.frame_count
world.time.time_scale
```

含义：

- `delta`：当前组使用的 delta
- `elapsed`：累计经过时间（受 `time_scale` 影响）
- `frame_count`：tick 次数
- `time_scale`：时间缩放

---

## raw API

`ecs::raw` 适合：

- 动态脚本/工具链
- 兼容层
- 需要手工构建 archetype 的场景

它不是推荐的运行时热路径 API。

### Archetype 构建

```rust
use sky_engine::ecs::raw::create_archetype;

let archetype = create_archetype()
    .add_rust_component::<Position>()
    .add_rust_component::<Velocity>()
    .build();
```

也可以直接按反射 `Type` 添加：

```rust
use sky_engine::ecs::raw::create_archetype;
use sky_engine::reflect::register_rust_type;

let archetype = create_archetype()
    .add_component(register_rust_type::<Position>())
    .add_component(register_rust_type::<Velocity>())
    .build();
```

### 低层实体创建

```rust
use sky_engine::ecs::raw::WorldRawExt;

let entity = world.add_entity(archetype);
```

注意：

- `raw::WorldRawExt::add_entity()` 只创建槽位
- 它不会自动初始化组件数据
- 适合测试、工具或自定义底层写入，不适合一般游戏逻辑

### 动态查询

```rust
use sky_engine::ecs::raw::{Query, QueryIter};
use sky_engine::reflect::register_rust_type;

let query = Query::new(vec![
    register_rust_type::<Position>(),
    register_rust_type::<Velocity>(),
]);

let mut iter = QueryIter::new(&world, &query);
iter.for_each2(|position, velocity| {
    // 原始指针
});
```

动态查询接口：

```rust
Query::new(types) -> Query
query.types() -> &[Type]

QueryIter::new(&world, &query) -> QueryIter
iter.for_each2(|a, b| ...)
iter.for_each_chunk2::<A, B, _>(|a_slice, b_slice| ...)
iter.for_each(|a, b, c, d| ...)
```

说明：

- `for_each2`：双组件原始指针遍历
- `for_each_chunk2::<A, B>`：双组件 chunk slice 遍历
- `for_each`：四组件原始指针遍历
- 动态查询会拒绝重复组件类型

---

## 设计建议

### 推荐

- 运行时系统优先使用 `PreparedQuery`
- 结构变化优先走 `Commands`
- 高频纯创建优先 `spawn_batch`
- 把 `PreparedQuery` 缓存在系统结构体里，重复复用

### 不推荐

- 在 active query 期间尝试直接结构修改
- 把 `raw` 动态查询当作主运行时 API
- 依赖 `src/main.rs` 作为 ECS API 的权威示例

---

## 一个更完整的系统示例

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

#[derive(Clone, Copy)]
struct Lifetime(f32);

struct MovementSystem {
    query: sky_engine::ecs::raw::PreparedQuery<(&mut Position, &Velocity)>,
}

impl Default for MovementSystem {
    fn default() -> Self {
        Self {
            query: Default::default(),
        }
    }
}

impl System for MovementSystem {
    fn run(&mut self, world: &mut World) {
        self.query.for_each(&world, |(pos, vel)| {
            pos.x += vel.x * world.time.delta;
            pos.y += vel.y * world.time.delta;
        });
    }
}

struct LifetimeSystem;

impl System for LifetimeSystem {
    fn run(&mut self, world: &mut World) {
        let mut commands = Commands::new();
        let mut query = world.query::<&Lifetime>();

        query.for_each_with_entity(&world, |entity, lifetime| {
            if lifetime.0 <= 0.0 {
                commands.despawn(entity);
            }
        });

        commands.apply(world);
    }
}

fn main() {
    let mut world = World::new();
    world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 0.0 },
        Lifetime(3.0),
    ));

    world.group("sim").add(MovementSystem::default());
    world.group("sim").add(LifetimeSystem);

    world.tick_with_delta(0.016);
}
```
