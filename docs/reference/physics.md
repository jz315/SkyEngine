# SkyEngine Physics

`sky_engine::physics` 是可选 2D 物理模块。公共 API 是 SkyEngine 自有类型，当前内部后端使用 Rapier。

启用：

```toml
sky_engine = { path = "...", features = ["physics"] }
```

渲染调试和 Tiled 物理示例需要：

```toml
sky_engine = { path = "...", features = ["app", "physics"] }
```

## 导出

```rust
use sky_engine::physics::{
    BodyType2D, Collider2D, ColliderShape2D, CollisionGroups2D, PhysicsConfig2D,
    PhysicsEvent2D, PhysicsEvents, PhysicsPlugin, PhysicsQueryFilter2D, PhysicsWorld2D,
    RaycastHit2D, RigidBody2D, Velocity2D, step_physics,
};
```

`app + physics` 额外导出：

```rust
use sky_engine::physics::{
    PhysicsDebugDraw2D, PhysicsDebugDrawOptions2D, PhysicsDebugPlugin,
    sync_physics_debug_draw,
};
```

## 安装

```rust
use sky_engine::ecs::World;
use sky_engine::physics::{PhysicsConfig2D, PhysicsPlugin};
use sky_engine::plugin::Plugin;

let mut world = World::new();
PhysicsPlugin::new(PhysicsConfig2D::default())
    .install(&mut world)
    .unwrap();
```

`PhysicsPlugin` 会插入 `PhysicsWorld2D` 和 `PhysicsEvents`，并注册固定步长 `"physics"` 系统组。重复安装会更新配置，不会重复添加 step system。

## Components

- `RigidBody2D`：刚体组件，支持 `static_body()`、`kinematic()`、`dynamic()`。
- `Velocity2D`：线速度和角速度。
- `Collider2D`：碰撞体组件，支持 rectangle、circle、capsule-y、sensor、friction、restitution、groups、offset、rotation。
- `CollisionGroups2D`：membership/filter mask。

## Runtime Resource

`PhysicsWorld2D` 是物理世界资源：

```rust
let physics = world.get_resource::<PhysicsWorld2D>().unwrap();
let bodies = physics.body_count();
let colliders = physics.collider_count();
```

查询 API：

```rust
physics.raycast(origin, direction, max_distance, solid);
physics.raycast_with_filter(origin, direction, max_distance, solid, filter);
physics.raycast_all(origin, direction, max_distance, solid);
physics.overlap_shape(center, ColliderShape2D::circle(radius));
```

## Events

`PhysicsEvents` 是最近 physics step 产生的事件队列：

```rust
let events = world.get_resource_mut::<PhysicsEvents>().unwrap();
for event in events.drain() {
    match event {
        PhysicsEvent2D::ContactStarted { a, b } => {}
        PhysicsEvent2D::ContactStopped { a, b } => {}
        PhysicsEvent2D::TriggerEntered { trigger, other } => {}
        PhysicsEvent2D::TriggerExited { trigger, other } => {}
    }
}
```

## 手动 Step

大多数 App 使用 `PhysicsPlugin` 的固定系统组。测试或关闭自动 tick 的 runner 可以直接调用：

```rust
step_physics(&mut world);
```

## 示例

```bash
cargo run --example physics_arcade_demo --features "app physics" --release
cargo run --example physics_headless_probe --features physics --release
```
