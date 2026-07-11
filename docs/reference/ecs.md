# SkyEngine ECS

`sky_engine::ecs` 是 SkyEngine 的核心数据层。它负责实体生命周期、组件存储、资源、查询、延迟命令和系统调度。当前设计是 archetype + chunked columnar storage：同一组件组合的实体放在同一 archetype，组件按列存储在 chunk 中，查询时尽量批量访问连续内存。

## 导出

```rust
use sky_engine::ecs::{
    component_type, Any, Bundle, CommandBuffer, Commands, ComponentType, EntityId,
    FixedOverflow, FixedStep, FixedUpdate, Local, PostUpdate, PreparedQuery, Query,
    ParView, QueryData, QueryMut, Res, ResMut, StageLabel, Time, Update, View, With,
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

let mut query = world.query_mut::<(&mut Position, &Velocity)>();
query.for_each(|(position, velocity)| {
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

`Time` 是永久存在的 World frame state：`get_resource::<Time>()` / `get_resource_mut::<Time>()` 与 `world.time` 指向同一值，`contains_resource::<Time>()` 恒为 true；不能 insert/remove。普通 worker system 只能通过 `Res<Time>` 读取。

## Typed Query

普通运行时逻辑使用绑定到 `World` 的 `Query` / `QueryMut`：

```rust
let positions = world.query::<&Position>();
let moving = world
    .query::<(&Position, &Velocity)>()
    .filter::<With<Velocity>>();
let mut writable = world.query_mut::<(&mut Position, &Velocity)>();
```

`query` 只接受只读参数，`query_mut` 通过独占借用 `World` 开放可写参数。查询对象已绑定 world，因此遍历时不再重复传 `world`。匹配计划由 `World` 按 `(Q, Filter)` 类型缓存，在 archetype epoch 变化时增量刷新。`.filter::<F>()` 是惰性的，不会先生成一个无过滤计划。

Archetype 的 component 列表按 TypeId 排序。至多两个组件的 query 直接逐项二分；更宽的 query 按比较成本选择后缀二分或双指针归并。最终列索引仍写回 query 的声明顺序，并保存在固定 16-slot map 中，所以 9～16 组件查询不会为每个匹配 archetype 单独堆分配。纯 AND filter tuple 会在 epoch refresh 时排序、去重、消解与 required query component 重复或矛盾的条件；单个 `With` / `Without`、`Any` 和无 filter 路径不经过这层编译。

支持参数：

| 写法 | 含义 |
| --- | --- |
| `&T` | 只读组件 |
| `&mut T` | 可写组件 |
| `Option<&T>` | 可选只读组件 |
| `Option<&mut T>` | 可选可写组件 |

tuple query 当前支持 1 到 16 个组件位。重复组件类型会被拒绝，这是为了避免 aliasing 语义不清；`QueryData` derive 能在编译期识别直接重复的字段类型，其余类型别名等情况由运行时 descriptor 校验兜底。

业务查询可以用命名字段代替位置 tuple：

```rust
#[derive(QueryData)]
struct Movement<'w> {
    position: &'w mut Position,
    velocity: &'w Velocity,
    mass: Option<&'w Mass>,
}

world.query_mut::<Movement>().for_each(|item| {
    item.position.x += item.velocity.x;
});
```

`QueryData` 字段支持 `&T`、`&mut T`、`Option<&T>` 和 `Option<&mut T>`；命名查询与 tuple 查询共享同一 descriptor、chunk 和并行执行路径。

遍历接口：

```rust
query.for_each(|item| { ... });
query.par_for_each(|item| { ... });
query.for_each_chunk(|chunk| { ... });
query.par_for_each_chunk(|chunk| { ... });
query.for_each_with_entity(|entity, item| { ... });
query.par_for_each_with_entity(|entity, item| { ... });
query.for_each_chunk_with_entities(|entities, chunk| { ... });
query.par_for_each_chunk_with_entities(|entities, chunk| { ... });

query.count() -> usize
query.is_empty() -> bool
query.cached_archetype_count() -> usize
```

只读查询的顺序和并行遍历都只需要 `&self`。绑定查询的并行 job 计划由 `World` 按查询类型缓存，因此重复创建轻量 `Query` 不会重复划分任务；可写查询的遍历需要 `&mut self`。缓存命中只比较 World 身份和 `storage_epoch`，不扫描 Chunk：spawn、despawn、组件迁移和 clear 会使其失效，普通组件值更新不会。

按实体遍历：

```rust
let mut query = world.query_mut::<(&mut Position, &Velocity)>();

query.for_each(|(position, velocity)| {
    position.x += velocity.x;
    position.y += velocity.y;
});
```

按 chunk 遍历：

```rust
query.for_each_chunk(|(positions, velocities)| {
    for index in 0..positions.len() {
        positions[index].x += velocities[index].x;
        positions[index].y += velocities[index].y;
    }
});
```

实体级并行是常规系统的首选写法：

```rust
let dt = 0.016;

query.par_for_each(|(position, velocity)| {
    position.x += velocity.x * dt;
    position.y += velocity.y * dt;
});
```

需要直接处理连续切片时使用并行 chunk 遍历：

```rust
let dt = 0.016;

query.par_for_each_chunk(|(positions, velocities)| {
    for index in 0..positions.len() {
        positions[index].x += velocities[index].x * dt;
        positions[index].y += velocities[index].y * dt;
    }
});
```

两种并行入口都按连续的 4096 实体 stripe 划分 Rayon job；`par_for_each` 在线程内部顺序展开实体，不会为每个实体创建任务。任务量不足时自动回退到普通顺序遍历。

需要把查询计划长期保存在 system struct、跨多个 world 显式复用，或反复复用 parallel job 划分时，使用高级 API `PreparedQuery<Q, Flt>`：

```rust
let mut query = PreparedQuery::<(&mut Position, &Velocity)>::new();
query.for_each(&mut world, |(position, velocity)| {
    position.x += velocity.x;
});
```

`PreparedQuery` 自己持有 epoch cache，遍历时显式接收 `&World` / `&mut World`。这也是引擎内部抽取器和极热 system 的持久化路径。

并行约束：

- stripe、chunk 和实体执行顺序未定义。
- 闭包不能直接做结构修改。
- 闭包不应捕获可变外部状态；需要共享结果时使用原子、锁，或先收集再顺序应用。
- 如果需要 resource 输入，建议在并行前复制出轻量 owned 数据。
- `par_for_each` 要求查询 item 为 `Send`，`par_for_each_chunk` 要求 chunk view 为 `Send`；`&T` / `&mut T` 会自然推导出相应的 `T: Sync` / `T: Send` 约束。

## Filters

可用 filter：

```rust
With<T>
Without<T>
Any<(A, B, ...)>
()
```

支持 tuple AND 组合：

```rust
(With<A>, Without<B>)
(With<A>, With<B>, Without<C>)
(With<A>, With<B>, Without<C>, Without<D>)
```

`Any<(...)>` 提供 OR 组合，并且可以嵌入外层 AND tuple：

```rust
let targets = world
    .query::<&Position>()
    .filter::<(Any<(With<Enemy>, With<Boss>)>, Without<Dead>)>();
```

示例：

```rust
let enemies = world.query::<&Position>().filter::<With<Enemy>>();
let mut moving_alive = world
    .query_mut::<(&mut Position, &Velocity)>()
    .filter::<(With<Velocity>, Without<Dead>)>();
```

## Commands

`Commands<'_>` 是 scheduler 借给普通 typed system 的私有延迟写入器；它不能脱离一次 system 调用，也不暴露 `apply`。每个 system 有自己的 buffer，scheduler 在确定的边界按注册顺序合并，因此 worker 完成顺序不会改变结果。

```rust
fn despawn_dead(health: View<&Health>, mut commands: Commands<'_>) {
    health.for_each_with_entity(|entity, health| {
        if health.hp <= 0.0 {
            commands.despawn(entity);
            commands.spawn((Loot { value: 10 },));
        }
    });
}
```

在 schedule 外手动构造命令时使用拥有所有权的 `CommandBuffer`：

```rust
let mut buffer = CommandBuffer::new();
buffer.spawn((Position { x: 0.0, y: 0.0 },));
buffer.apply(&mut world);
```

两者共享的写入接口：

```rust
commands.spawn(bundle)
commands.despawn(entity)
commands.insert(entity, component)
commands.remove::<T>(entity)

commands.insert_resource(resource)
commands.remove_resource::<R>()
commands.is_empty() -> bool
commands.len() -> usize
```

只有 `CommandBuffer` 额外提供 `apply(&mut World)` 与 `clear()`。

语义：

- 相邻同 bundle 类型的 `spawn` 会合并成批量创建。
- 同一实体同一批次里的重复 `insert/remove` 会折叠成最终状态。
- `despawn` 会吞掉该实体后续同批次组件修改。
- command queue 保留 first-seen entity 顺序。
- 普通 wave 之间不会 flush；stage 结束或 exclusive barrier 前才 flush。
- system buffer 在 panic 时丢弃，已经完成的组件/resource 值写入不会回滚。
- command apply 一旦 panic，`World::is_poisoned()` 会变为 `true`；World 仍可检查和 shutdown，但拒绝继续 apply 或 tick，避免半提交状态继续运行。

## 系统与调度

普通 system 是带 typed 参数的函数或闭包。参数签名就是完整访问集合：

```rust
fn movement(
    entities: ParView<(&mut Position, &Velocity), With<Active>>,
    time: Res<Time>,
) {
    entities.par_for_each(|(position, velocity)| {
        position.x += velocity.x * time.delta;
        position.y += velocity.y * time.delta;
    });
}

fn update_score(scores: View<&Score>, mut total: ResMut<TotalScore>) {
    total.0 = 0;
    scores.for_each(|score| total.0 += score.0);
}

world.stage(Update).add(movement).add(update_score);
```

系统参数：

| 参数 | 推导访问 |
| --- | --- |
| `View<&T, F>` | 读组件 `T` |
| `View<&mut T, F>` | 写组件 `T` |
| `ParView<Q, F>` | 与 `View<Q, F>` 相同，并在串行 prepare 中构建并行 stripe jobs |
| `Res<T>` | 读 resource `T` |
| `ResMut<T>` | 写 resource `T` |
| `Commands<'_>` | system 私有延迟 buffer |
| `Local<T>` | system 私有持久状态，不产生冲突 |

`View` 只提供 `for_each` / `for_each_chunk` 及 entity-aware 顺序遍历，不支付并行 cache 成本；`ParView` 提供对应的 `par_*` 遍历，并继续对小数据量自动回退顺序执行。句柄本身不需要写成 `mut`；`&mut T` 已经完整表达组件写权限。一次 view 不能递归迭代。函数系统最多支持 16 个参数。

资源缺失会在整帧 preflight 阶段返回 `ScheduleError::MissingResource`，包含 system 名与 resource 类型；此时不会执行任何 system，也不会推进 `Time`。成功结果会缓存到 resource epoch 或 system access 变化为止。resource 必须在 tick 开始前存在，不能依赖本帧前面的 system 临时创建。若运行中的 system 移除了本帧后续 system 必需的 resource，则属于调度契约破坏并明确 panic。`Res<Time>` 是只读时间输入；`ResMut<Time>` 不属于 worker system 的合法参数。

内置 stage 顺序固定：

```text
First -> FixedUpdate -> PreUpdate -> Update -> PostUpdate -> Last
```

自定义 barrier 使用类型标签，不使用字符串：

```rust
#[derive(StageLabel)]
struct RenderExtract;

world
    .insert_stage_after(PostUpdate, RenderExtract)?
    .add(extract_scene);
```

自定义 stage 必须通过 `insert_stage_after` 显式安装；`world.stage(UninstalledCustomStage)` 不会再静默追加到 `Last` 后面。对同一 anchor 重复插入会保持调用顺序；嵌套插入的 stage 会留在其父 stage 的连续子树内，不会出现后插 sibling 反转。需要探测可选 stage 时使用 `world.try_stage(label)`。

同一 stage 内，scheduler 根据组件/resource 的 read/write 集合编译确定性 wave。read/read 可同 wave；write/read、read/write、write/write 必须按注册顺序进入后续 wave。不同 system 的 `Commands` 不冲突。自动 wave 只影响并行执行，不改变 command 可见性。默认少于 3 个 system 的 wave 直接在当前线程执行，避免 tiny wave 的 Rayon 调度成本；已知工作很重的二元 wave 可通过 `world.stage(Update).parallel_wave_min_systems(2)?` 开启并行。

固定步配置经过验证且有每帧上限：

```rust
world
    .stage(FixedUpdate)
    .fixed(
        FixedStep::hz(60)
            .max_substeps(8)
            .overflow(FixedOverflow::Drop),
    )
    .expect("fixed-step configuration must not conflict")
    .add(integrate_physics);
```

`FixedUpdate` 默认就是 60 Hz；第一次显式 `.fixed(...)` 可覆盖默认值。之后重复同一配置是幂等操作，不同配置会返回 `ScheduleBuildError::ConflictingFixedStep`，不再采用隐蔽的 last-writer-wins。`FixedOverflow::Drop` 丢弃超过上限的完整积压 step 并计入 report；`Carry` 留到后续 frame。固定 substep 内 `Time::delta` 等于 step。`Time::fixed_alpha` 只表示内置 `FixedUpdate` 的插值比例，其他 fixed stage 的 backlog 可从 schedule diagnostics 读取。

必须直接拿整个 `&mut World` 的旧逻辑需要显式 exclusive barrier：

```rust
world.stage(Update).add_exclusive(|world: &mut World| {
    rebuild_editor_world(world);
});
```

`ExclusiveSystem` 可实现 `init/run/teardown`，允许持有非 `Send` 状态；它会 flush 前一 segment 的命令并阻止普通系统跨 barrier 重排。

规则：

- typed system 的 `View` archetype plan、`ParView` stripe jobs 和 resource pointer 都在主线程串行 prepare，worker 不访问 World query cache。
- 达到 stage 并行阈值的无冲突 system 共享 Rayon pool；较小 wave 直接顺序 dispatch。
- stage access graph 只在注册内容变化后重新编译，正常 tick 不做 graph allocation。
- schedule 由 RAII guard 在 panic 时恢复；递归 tick 和执行期间修改 schedule 会被拒绝。
- `world.shutdown()` 逆注册顺序 teardown exclusive system，并释放普通 system 的 `Local` 状态。

驱动：

```rust
let report = world.tick()?;
let report = world.tick_with_delta(0.016)?;
let report = world.tick_with_frame_delta(frame_delta, raw_delta)?;
world.shutdown();
```

`TickReport` 包含 frame、执行 system/wave 数、实际 parallel/sequential wave 数、fixed substep 数与丢弃时间。`world.schedule_diagnostics()` 返回已编译 stage/segment/wave、系统名、访问集合、exclusive barrier、并行阈值、fixed backlog，以及每个 system 最近一次和累计的 command enqueue/apply/discard 统计；`SystemAccessDiagnostics::conflict_reason` 可解释两个系统为何冲突。

`World::time`：

```rust
world.time.delta
world.time.frame_delta
world.time.raw_delta
world.time.elapsed
world.time.raw_elapsed
world.time.frame_count
world.time.fixed_alpha
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

- gameplay update 优先写成 `View` / `ParView` / `Res` / `Commands` typed system，注册到语义明确的 stage。
- schedule 外的只读逻辑用 `world.query::<Q>()`，可写逻辑用 `world.query_mut::<Q>()`。
- 过滤使用 `.filter::<With<T>>()`、`.filter::<Without<T>>()` 或 filter tuple。
- system 中的结构变化用 borrowed `Commands`；schedule 外使用 `CommandBuffer`。
- 批量创建优先 `spawn_batch` 或连续 `commands.spawn`。
- 普通 system 让 `View` 持有顺序 plan、`ParView` 持有并行 job cache；只有 exclusive/引擎内部持久状态才手动保存 `PreparedQuery`。
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
