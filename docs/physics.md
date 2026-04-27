# SkyEngine 2D Physics v1

SkyEngine 的 `physics` 功能是可选的 2D 物理层，当前后端使用 `rapier2d`，但公共 API 保持在
`sky_engine::physics` 命名空间内。应用代码不需要保存 Rapier handle，也不应该直接依赖 Rapier 类型。

v1 面向 top-down / Tiled 2D 游戏：固定步长模拟、静态地图碰撞、kinematic 玩家移动、dynamic 物体、trigger/contact 事件、基础场景查询和调试线框。

## 启用功能

```bash
cargo test --features physics
cargo run --example physics_arcade_demo --features "app physics"
cargo run --example tiled_physics_demo --features "app physics"
```

`physics` 不依赖 `app`。窗口、渲染、Tiled physics demo 和 debug draw 需要同时启用 `app physics`。

## 最小用法

```rust
use sky_engine::ecs::World;
use sky_engine::math::{Transform, Vec2};
use sky_engine::physics::{
    install_physics, BodyType2D, Collider2D, PhysicsConfig2D, RigidBody2D, Velocity2D,
};

let mut world = World::new();

install_physics(
    &mut world,
    PhysicsConfig2D {
        gravity: Vec2::ZERO,
        fixed_dt: 1.0 / 60.0,
        pixels_per_meter: 32.0,
    },
);

world.spawn((
    Transform::from_xy(0.0, 0.0),
    RigidBody2D::new(BodyType2D::Kinematic),
    Collider2D::rectangle(16.0, 16.0),
    Velocity2D::new(120.0, 0.0),
));

world.spawn((
    Transform::from_xy(64.0, 0.0),
    RigidBody2D::static_body(),
    Collider2D::rectangle(32.0, 32.0),
));

world.tick_with_delta(1.0 / 60.0);
```

`install_physics` 会插入：

- `PhysicsWorld2D`
- `PhysicsEvents`
- 一个固定步长 `"physics"` system group

重复调用 `install_physics` 会更新配置和固定步长，不会重复注册 physics system。

## 配置

`PhysicsConfig2D::default()` 是 top-down 默认值：

```rust
PhysicsConfig2D {
    gravity: Vec2::ZERO,
    fixed_dt: 1.0 / 60.0,
    pixels_per_meter: 32.0,
}
```

`gravity` 使用 SkyEngine 世界单位每秒平方，不是 Rapier meter。比如平台游戏常见写法：

```rust
PhysicsConfig2D {
    gravity: Vec2::new(0.0, -900.0),
    pixels_per_meter: 64.0,
    ..Default::default()
}
```

`pixels_per_meter` 是世界单位到物理米制单位的转换比例。经验值：

- tile / sprite 以 16 到 32 world units 为一格时，用 `32.0`
- 物体尺寸偏大、速度偏高时，用 `64.0`
- 避免让 Rapier 中的物体小到接近 0 或大到几百米

## Body 和 Collider

物理实体通常由三个组件组成：

```rust
(
    Transform::from_xy(x, y),
    RigidBody2D::dynamic(),
    Collider2D::circle(radius),
)
```

`RigidBody2D` 支持：

| 类型 | 用途 |
|------|------|
| `Static` | 墙、地面、Tiled map collider |
| `Kinematic` | 玩家、移动平台、由代码直接驱动的角色 |
| `Dynamic` | 被物理模拟推动、碰撞和弹开的物体 |

构造辅助：

```rust
RigidBody2D::static_body()
RigidBody2D::kinematic()
RigidBody2D::dynamic()
RigidBody2D::dynamic().lock_rotation().ccd(true)
```

`Collider2D` 支持：

```rust
Collider2D::rectangle(width, height)
Collider2D::circle(radius)
Collider2D::capsule_y(half_height, radius)
```

常用配置：

```rust
Collider2D::rectangle(32.0, 16.0)
    .sensor(false)
    .friction(0.8)
    .restitution(0.2)
    .offset(Vec2::new(0.0, -4.0))
```

`sensor(true)` 表示 trigger。Trigger 会产生 enter/exit 事件，但不会阻挡 kinematic / dynamic 物体。

## Kinematic 移动

v1 的 kinematic body 是 position-based。推荐用 `Velocity2D` 驱动：

```rust
if let Some(velocity) = world.get_mut::<Velocity2D>(player) {
    velocity.linear = input_direction.normalized() * 180.0;
    velocity.angular = 0.0;
}
```

physics step 会把 linear velocity 转成当前固定步长内的期望位移，并通过 Rapier 的 character controller 对 solid collider 做滑动阻挡。Sensor collider 会被忽略。

也可以不加 `Velocity2D`，直接在 physics step 前写 `Transform`，kinematic body 会尝试移动到该位置。

## 调度和输入时序

`AppConfig::auto_tick` 默认开启。此时 App runner 的顺序是：

```text
同步输入资源
world.tick_with_frame_delta(...)
AppState::update(...)
render
```

也就是说，如果你在 `AppState::update` 里改 `Velocity2D`，默认会在下一帧 physics step 生效。这个延迟通常只有一帧，但做手感敏感的玩家控制时建议使用下面两种方式之一。

### 方式 A：把输入写成 physics 前的 ECS system

系统 group 按创建顺序运行。先创建 `"pre_physics"`，再调用 `install_physics`：

```rust
world.group("pre_physics").add(|world: &mut World| {
    // 从 Input resource / 自己的控制资源读取输入，写 Velocity2D
});

install_physics(&mut world, PhysicsConfig2D::default());
```

这样每帧顺序是：

```text
pre_physics
physics
```

### 方式 B：关闭 auto_tick，手动 tick

如果输入逻辑必须留在 `AppState::update`，可以关闭自动 tick：

```rust
App::new(
    AppConfig::new("Manual Physics", 960, 720).with_auto_tick(false),
    World::new(),
)
```

然后在 `update` 里先改组件，再 tick，再 render：

```rust
fn update(&mut self, ctx: &mut FrameContext<'_>) {
    self.apply_input(ctx);
    ctx.world.tick_with_delta(ctx.dt);
    ctx.render();
}
```

## Events

`PhysicsEvents` 是 drainable resource。每次 physics step 会把接触和 trigger 事件追加进去，使用方应在自己处理完后 drain。

```rust
if let Some(events) = world.get_resource_mut::<PhysicsEvents>() {
    for event in events.drain() {
        match event {
            PhysicsEvent2D::ContactStarted { a, b } => {}
            PhysicsEvent2D::ContactStopped { a, b } => {}
            PhysicsEvent2D::TriggerEntered { trigger, other } => {}
            PhysicsEvent2D::TriggerExited { trigger, other } => {}
        }
    }
}
```

建议每帧 drain 一次。若不 drain，队列会保留旧事件。

## Scene Queries

从 `PhysicsWorld2D` resource 读取查询接口：

```rust
let physics = world.get_resource::<PhysicsWorld2D>().unwrap();

let hit = physics.raycast(
    Vec2::new(0.0, 0.0),
    Vec2::new(1.0, 0.0),
    256.0,
    true,
);
```

带过滤：

```rust
let filter = PhysicsQueryFilter2D::default()
    .exclude_entity(player)
    .solids_only();

let hits = physics.raycast_all_with_filter(
    origin,
    direction,
    max_distance,
    true,
    filter,
);
```

Overlap：

```rust
let sensors = physics.overlap_shape_with_filter(
    player_position,
    ColliderShape2D::circle(48.0),
    PhysicsQueryFilter2D::default().sensors_only(),
);
```

查询返回 `EntityId`，不会暴露 Rapier handle。

## Collision Groups

`CollisionGroups2D` 包含两个 mask：

- `memberships`：这个 collider 属于哪些组
- `filters`：这个 collider 愿意和哪些组交互

```rust
const PLAYER: u32 = 1 << 0;
const WALL: u32 = 1 << 1;

let player_groups = CollisionGroups2D::new(PLAYER, WALL);
let wall_groups = CollisionGroups2D::new(WALL, PLAYER);

Collider2D::rectangle(16.0, 16.0).collision_groups(player_groups);
```

查询也可以使用 collision groups：

```rust
let filter = PhysicsQueryFilter2D::default()
    .collision_groups(CollisionGroups2D::new(PLAYER, WALL));
```

## Tiled Physics

Tiled physics integration 位于 `sky_engine::render`，需要 `app + physics`：

```rust
use sky_engine::render::{
    TiledImport, TiledMapInstance, TiledPhysicsInstance, TiledPhysicsOptions, TiledSpawnOptions,
};

let import = TiledImport::from_file("map.tmx")?;
let map = TiledMapInstance::spawn_import(
    world,
    &import,
    TiledSpawnOptions::centered(),
)?;
let physics = TiledPhysicsInstance::spawn(
    world,
    &import,
    map.origin(),
    TiledPhysicsOptions::default(),
)?;
```

Tiled 属性规则：

- tile / layer / object 上的 `solid=true` 会生成 solid static collider
- tile / layer / object 上的 `trigger=true` 会生成 sensor static collider
- `trigger=true` 也会被视为需要 collider
- orthogonal tile layer 会按 layer 和 sensor 状态合并成矩形 collider
- object collider v1 只支持 rectangle
- 非 orthogonal tile collider 和非 rectangle object collider 会返回 `TiledPhysicsError`

`TiledMapInstance::origin()` 要传给 `TiledPhysicsInstance::spawn`，这样渲染实例使用 centered origin 时 collider 仍能对齐。

## Debug Draw

Debug draw 需要 `app + physics`，它用普通 `SpriteRenderer` 画 collider 线框：

```rust
use sky_engine::physics::{install_physics_debug_draw, PhysicsDebugDrawOptions2D};

install_physics_debug_draw(
    world,
    PhysicsDebugDrawOptions2D {
        enabled: true,
        line_thickness: 2.0,
        ..Default::default()
    },
);
```

运行时开关：

```rust
if let Some(debug) = world.get_resource_mut::<PhysicsDebugDraw2D>() {
    debug.set_enabled(false);
}
```

如果刚切换开关并希望马上更新线框，可调用：

```rust
sync_physics_debug_draw(world);
```

默认颜色含义：

- static solid：蓝色
- kinematic solid：黄色
- dynamic solid：绿色
- trigger / sensor：紫色
- disabled：灰色

`physics_arcade_demo` 已经接入 debug draw，按 `F` 开关。

## 示例

```bash
cargo run --example physics_arcade_demo --features "app physics" --release
cargo run --example tiled_physics_demo --features "app physics" --release
```

`physics_arcade_demo` 展示 dynamic 物体、可切换 gravity、trigger、debug draw。

`tiled_physics_demo` 展示 Tiled map 渲染实例和独立 physics 实例对齐，以及 kinematic 玩家被 solid tile 阻挡、踩 trigger 产生事件。

## v1 限制

- 暂不暴露 raw Rapier handle/type
- 每个实体 v1 只建一个 body 和一个 collider
- Kinematic body 走 position-based 移动
- Tiled tile collider 只支持 orthogonal map
- Tiled object collider 只支持 rectangle
- Debug draw 是 sprite 线段池，不是 GPU-native gizmo renderer

