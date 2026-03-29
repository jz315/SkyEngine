# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## SkyEngine 是什么？

SkyEngine 是一个用 Rust 写的游戏引擎原型，核心是一套自研的 Entity Component System（ECS）。它的存储基于固定大小的 512KB chunk，组件数据在 chunk 内按列排布——同一类型的组件紧挨着存在一起，而不是按 entity 交错存放。这种布局直接对应现代 CPU 的内存访问方式：顺序、可预测、prefetcher 友好。

SkyEngine 是一个库，不是框架。没有隐式全局状态，没有 proc macro，没有强制的应用结构。你创建一个 `World`，spawn entity，query 组件，在上面搭自己的游戏。

项目还在早期。ECS 运行时已经可用并且经过了 benchmark 验证，但还没有渲染器、资产管线和编辑器。

## 为什么选 SkyEngine？

- **Chunk 列存储。** 组件数据按类型打包在 512KB chunk 内，CPU prefetcher 能直接顺序预取。这是迭代快的根本原因，尤其在 archetype 碎片化的场景下优势最大。

- **零开销批量插入。** `spawn_batch` 在循环外一次性算好 column offset，循环内每个 entity 的写入就是一次 `ptr::write`——没有 hash 查表，没有二分查找，没有类型注册。

- **Epoch 缓存查询。** `PreparedQuery` 将 archetype 匹配结果缓存下来，只在 world 的 archetype 集合变化时重新扫描。游戏主循环里重复跑的 query 几乎没有 setup 开销。

- **代码量小。** 整个 ECS 核心不到 2000 行 Rust。没有 proc macro，没有代码生成，运行时反射只有一个最小的类型注册表。

## 快速开始

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    // 创建单个 entity
    let entity = world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 2.0 },
    ));

    // 批量创建 10000 个 entity
    world.spawn_batch((0..10_000).map(|i| {
        (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
    }));

    // 逐 entity 查询
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // chunk 级查询，拿到的是 slice，适合手动向量化
    query.for_each_chunk(&world, |(positions, velocities)| {
        for (p, v) in positions.iter_mut().zip(velocities.iter()) {
            p.x += v.x * 0.016;
        }
    });

    // 随机访问
    let pos = world.get::<Position>(entity).unwrap();
    println!("({}, {})", pos.x, pos.y);
}
```

## 性能

Benchmark 基于 Criterion，代码在 `benches/` 目录下。以下数据在同一台机器上采集，workload 相同，单线程，对比对象是 [hecs](https://github.com/Ralith/hecs) 和 [Bevy ECS](https://github.com/bevyengine/bevy)。

### 迭代

中等规模（10k entity，4 组件，查其中 2 个）下，SkyEngine 的吞吐量大约是 hecs 的 3 倍、Bevy 的 5 倍。碎片化迭代（大量 archetype）是 chunk 存储最受益的场景——大约比 hecs 快 2 倍、比 Bevy 快 10 倍。

5M entity 规模下，SkyEngine 和 hecs 都接近内存带宽极限，SkyEngine 仍领先 10-15%。

### 插入

`spawn_batch` 插入 10000 个 entity 耗时约 135µs，hecs 约 282µs，Bevy 约 305µs。加速来自循环外预算 column offset，循环内每个 entity 的写入是直接的 `ptr::write`，不做任何查找。

### 压力测试

粒子模拟 example 在单线程下处理 100 万 entity，跑到 80 FPS，包含物理和像素缓冲渲染：

```sh
cargo run --example particles --release --features demo
```

### 跑 benchmark

```sh
cargo bench                           # 全部
cargo bench --bench iter              # 迭代
cargo bench --bench insert            # 插入
cargo bench --bench sky --bench hevy  # 5M 正面对比
```

详细历史数据和 chunk size 调优过程见 [BENCHMARKS.md](BENCHMARKS.md)。

## 文档

- [API 文档](docs/api.md) — 完整接口说明
- [Benchmark 记录](BENCHMARKS.md) — 历史数据

## 相关项目

- [bevy](https://github.com/bevyengine/bevy) — 功能完整的游戏引擎，插件生态成熟
- [hecs](https://github.com/Ralith/hecs) — 精简高质量的 archetype ECS
- [flecs](https://github.com/SanderMertens/flecs) — C99 写的功能丰富 ECS，有 Rust binding

## 许可

MIT ([LICENSE](LICENSE))
