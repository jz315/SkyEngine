# SkyEngine ECS API 文档

## 快速上手

```rust
use sky_engine::ecs::*;

// 定义组件 — 任何 Copy + 'static 的 struct
#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    // 创建实体
    let entity = world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 }));

    // 查询并更新
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x;
        pos.y += vel.y;
    });

    // 系统 + 调度
    let mut schedule = Schedule::new();
    schedule.add_system(SystemGroup::Simulation, |world: &mut World| {
        // 游戏逻辑
    });
    schedule.run(&mut world);
}
```

---

## World（世界）

所有实体、组件和资源的中央存储。

```rust
World::new() -> World
```

### 实体生命周期

```rust
// 创建 — 立刻返回 EntityId
world.spawn((Position { x: 0.0, y: 0.0 },))                                     -> EntityId
world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 }))         -> EntityId

// 检查
world.contains(entity)                -> bool    // 实体是否存活
world.has::<Position>(entity)         -> bool    // 实体是否拥有该组件

// 访问组件
world.get::<Position>(entity)         -> Option<&Position>
world.get_mut::<Position>(entity)     -> Option<&mut Position>

// 修改结构
world.insert(entity, Health { hp: 100 })  -> bool    // 添加组件（触发 archetype 迁移）
world.remove::<Health>(entity)            -> bool    // 移除组件（触发 archetype 迁移）
world.despawn(entity)                     -> bool    // 销毁实体

// 统计
world.entity_count()      -> usize    // 存活实体数
world.archetype_count()   -> usize    // archetype 数量
world.clear()                          // 清除所有实体，保留资源
```

### 资源（全局单例）

```rust
world.insert_resource(GameTime::default())   -> Option<GameTime>   // 返回旧值
world.get_resource::<GameTime>()             -> Option<&GameTime>
world.get_resource_mut::<GameTime>()         -> Option<&mut GameTime>
world.contains_resource::<GameTime>()        -> bool
world.remove_resource::<GameTime>()          -> Option<GameTime>
```

---

## EntityId（实体标识）

代际索引 — 可以安全存储，despawn 后自动失效。

```rust
entity.index()       -> u32    // 槽位索引
entity.generation()  -> u32    // 代数（防止悬垂引用）

// 自动派生：Debug, Clone, Copy, PartialEq, Eq, Hash
```

---

## 查询系统

### 创建查询

```rust
// 基本查询
let mut query = world.query::<(&mut Position, &Velocity)>();

// 带过滤器的查询
let mut query = world.query_filtered::<&Position, With<Velocity>>();
let mut query = world.query_filtered::<&Position, (With<Velocity>, Without<Dead>)>();
```

### 查询参数

| 语法 | 含义 |
|------|------|
| `&T` | 只读引用 |
| `&mut T` | 可写引用 |
| `Option<&T>` | 可选只读（有返回 Some，无返回 None） |
| `Option<&mut T>` | 可选可写 |

支持 1–8 个组件的元组。

### 遍历方式

```rust
// 逐实体遍历
query.for_each(&world, |(pos, vel)| {
    pos.x += vel.x;
});

// 逐实体遍历（带 EntityId）
query.for_each_with_entity(&world, |entity, (pos, vel)| {
    // entity 是当前实体的 ID
});

// 逐 chunk 遍历（slice 访问，更利于 SIMD 向量化）
query.for_each_chunk(&world, |(positions, velocities)| {
    for i in 0..positions.len() {
        positions[i].x += velocities[i].x;
    }
});

// 逐 chunk 遍历（带实体列表）
query.for_each_chunk_with_entities(&world, |entities, (positions, velocities)| {
    // entities: &[EntityId]
});

// 统计
query.count(&world)      -> usize    // 匹配实体总数
query.is_empty(&world)   -> bool     // 是否无匹配实体
```

### 过滤器

```rust
With<T>                          // 要求 archetype 包含 T
Without<T>                       // 要求 archetype 不包含 T
(With<A>, Without<B>)            // AND 组合（支持 2–4 个）
```

### 查询缓存

`PreparedQuery` 会缓存匹配的 archetype 列表，新 archetype 注册时自动刷新。
存储在 System struct 中可获得最佳性能：

```rust
struct MovementSystem {
    query: PreparedQuery<(&mut Position, &Velocity)>,
}

impl System for MovementSystem {
    fn run(&mut self, world: &mut World) {
        self.query.for_each(world, |(pos, vel)| {
            pos.x += vel.x;
        });
    }
}
```

---

## System 与 Schedule（系统与调度）

### System Trait

```rust
pub trait System: 'static {
    fn init(&mut self, _world: &mut World) {}      // 首次运行时调用一次
    fn run(&mut self, world: &mut World);           // 每帧调用
    fn teardown(&mut self, _world: &mut World) {}   // shutdown 时调用
}
```

### 两种写法

**闭包** — 简单逻辑直接传：

```rust
schedule.add_system(SystemGroup::Simulation, |world: &mut World| {
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(world, |(pos, vel)| {
        pos.x += vel.x;
    });
});
```

**Struct** — 需要状态或生命周期钩子：

```rust
struct WaveSpawner { timer: f32 }

impl System for WaveSpawner {
    fn init(&mut self, world: &mut World) {
        world.insert_resource(Score { value: 0 });
    }

    fn run(&mut self, world: &mut World) {
        self.timer -= 0.016;
        if self.timer <= 0.0 {
            self.timer = 5.0;
            world.spawn((Enemy, Position { x: 0.0, y: 0.0 }));
        }
    }

    fn teardown(&mut self, world: &mut World) {
        // 清理逻辑
    }
}
```

### Schedule（调度器）

```rust
let mut schedule = Schedule::new();

// 注册系统到不同分组
schedule
    .add_system(SystemGroup::Initialization, setup_system)
    .add_system(SystemGroup::Simulation, MovementSystem::default())
    .add_system(SystemGroup::Presentation, |world: &mut World| { /* 渲染 */ });

// 执行顺序：Initialization → Simulation → Presentation
schedule.run(&mut world);

// 固定时间步长（仅执行 FixedStepSimulation 分组）
schedule.run_fixed_step(&mut world);

// 关闭（逆序调用 teardown）
schedule.shutdown(&mut world);
```

### 系统分组

| 分组 | 用途 |
|------|------|
| `Initialization` | 初始化、输入处理 |
| `FixedStepSimulation` | 物理、确定性逻辑（通过 `run_fixed_step` 调用） |
| `Simulation` | 主要游戏逻辑 |
| `Presentation` | 渲染、音频、UI |

---

## Commands（延迟命令）

在查询迭代中需要做结构性修改时使用。

```rust
use sky_engine::ecs::Commands;

fn run(&mut self, world: &mut World) {
    let mut commands = Commands::new();

    let mut query = world.query::<&Health>();
    query.for_each_with_entity(world, |entity, health| {
        if health.hp <= 0 {
            commands.despawn(entity);                         // 延迟执行
            commands.spawn((DroppedItem { value: 10 },));     // 延迟执行
        }
    });

    commands.apply(world);  // 统一执行所有缓冲操作
}
```

### 可用操作

```rust
commands.spawn(bundle)                   // 延迟创建，不返回 ID
commands.despawn(entity)                 // 延迟销毁
commands.insert(entity, component)       // 延迟添加组件
commands.remove::<T>(entity)             // 延迟移除组件
commands.insert_resource(resource)       // 延迟插入资源
commands.remove_resource::<T>()          // 延迟移除资源
commands.apply(&mut world)               // 执行所有缓冲操作
commands.is_empty() -> bool
commands.len() -> usize
```

### 何时用 Commands，何时用 World

| 场景 | 用法 |
|------|------|
| 在 `query.for_each` 内部 | `commands`（world 被借用，无法直接修改） |
| 迭代外，需要 EntityId | `world.spawn()`（立刻返回可用 ID） |
| 迭代外，不需要 EntityId | 都可以，`world.spawn()` 更直接 |

---

## Bundle（组件打包）

创建实体时，组件以元组形式打包。每个组件必须满足 `Copy + 'static`。

```rust
// 单组件（注意尾逗号）
world.spawn((Position { x: 0.0, y: 0.0 },));

// 多组件
world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 1.0 }));

// 最多支持 8 个组件
world.spawn((A, B, C, D, E, F, G, H));
```

> **注意**：单元素元组 `(Pos { .. },)` 的尾逗号是必须的，否则 Rust 会将其解析为括号表达式。

---

## `ecs::raw`（底层 API）

用于动态/脚本化访问和工具开发。

```rust
use sky_engine::ecs::raw::*;

// 手动构建 archetype
let archetype = create_archetype()
    .add_rust_component::<Position>()
    .add_rust_component::<Velocity>()
    .build();

// 动态查询（原始指针访问）
let query = Query::new(vec![position_type, velocity_type]);
let mut iter = QueryIter::new(&world, &query);
iter.for_each2(|ptr_a, ptr_b| { /* unsafe 指针操作 */ });

// 底层实体创建（不写入组件数据）
use sky_engine::ecs::raw::WorldRawExt;
let entity = world.add_entity(archetype);
```
