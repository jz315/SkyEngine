# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[English](README.md)

基于块列式存储的 Archetype ECS，以 Rust 实现。同类型组件在内存中连续排列，迭代时产生对缓存预取友好的顺序访存模式。

SkyECS 是一个高性能，易用的ECS游戏框架，并在性能上力求极致。

> **项目阶段：** ECS 运行时已可用，附有完整基准测试。渲染器、资产管线、编辑器等上层组件尚未实现。

## 快速上手

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    let entity = world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 2.0 },
    ));

    // 批量插入
    world.spawn_batch((0..10_000).map(|i| {
        (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
    }));

    // 迭代
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // 块级迭代，返回切片，便于手动向量化
    query.for_each_chunk(&world, |(positions, velocities)| {
        for (p, v) in positions.iter_mut().zip(velocities.iter()) {
            p.x += v.x * 0.016;
        }
    });

    let pos = world.get::<Position>(entity).unwrap();
    println!("({}, {})", pos.x, pos.y);
}
```

## 主要特性

- **块列式存储：** 每个 archetype 按固定大小的块组织，块内按组件类型分列连续存储，迭代路径与硬件预取对齐。
- **批量插入：** `spawn_batch` 避免逐实体的重复查找，大批量场景下显著优于逐个插入。
- **查询缓存：** `PreparedQuery` 缓存匹配结果，archetype 未变动时重复查询无额外开销。
- **可选组件与过滤器：** 查询支持 `Option<&T>` 访问可选组件，以及 `With<T>` / `Without<T>` 编译期过滤。

## 性能

基准测试现在分成两条线。方法论、记录数据和历史结果详见 [BENCHMARKS.md](BENCHMARKS.md)。

- `cargo bench --bench fair` 是唯一的公平横向对比入口，只包含三家引擎都能等价表达的 workload，并且所有引擎都把 query/prepared state 放在计时区间之外。
- `cargo bench --bench sky` 是项目自身的回归套件，保留 Sky 专有的热路径，例如 chunk 级迭代、类型化过滤查询和 commands 路径；这些不会混入公平对比结果。
- `cargo bench --bench flecs` 新增了 `flecs_ecs` 的参考套件，可用于横向观察，但不会改变 `fair` 作为规范公平基线的定义。

粒子模拟示例（80,000 并发实体）：

```sh
cargo run --example particles --release --features demo
```

运行基准测试：

```sh
cargo bench --bench fair   # 公平横向对比
cargo bench --bench sky    # Sky 回归套件
cargo bench --bench hecs   # hecs 参考套件
cargo bench --bench bevy   # bevy 参考套件
cargo bench --bench flecs  # flecs 参考套件
cargo bench                # 全部 bench
```

## 项目结构

```
src/
├── ecs/
│   ├── archetype.rs    # 原型定义与构建器
│   ├── chunk.rs        # 块分配与列式存储
│   ├── query/          # 类型化查询、过滤器
│   ├── world.rs        # World 存储与实体管理
│   ├── bundle.rs       # 组件包 trait
│   ├── system.rs       # 系统调度
│   └── ...
├── reflect/            # 运行时类型注册表
└── lib.rs
benches/                # 基准测试
examples/               # 可运行示例
```

## 文档

- [API 参考](docs/api.md)
- [基准测试记录](BENCHMARKS.md)

## 相关项目

- [hecs](https://github.com/Ralith/hecs) — 精简的 archetype ECS
- [Bevy](https://github.com/bevyengine/bevy) — 完整游戏引擎，插件生态
- [flecs](https://github.com/SanderMertens/flecs) — C99 实现，功能丰富

## 许可证

MIT（[LICENSE](LICENSE)）
