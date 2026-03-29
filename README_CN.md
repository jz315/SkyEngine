# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

SkyEngine 是一个用 Rust 写的数据导向游戏引擎原型，核心是一套基于 chunk 列存储的 ECS。它是一个库，不是框架——你的游戏怎么组织由你决定。

ECS 的存储设计围绕固定大小的 512KB chunk，组件数据按列排布在 chunk 内。这让 CPU prefetcher 能预测内存访问模式，查询会缓存匹配的 archetype 列表，只在 world 结构发生变化时刷新。

### 示例

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

let mut world = World::new();

let e = world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 2.0 }));

let mut query = world.query::<(&mut Position, &Velocity)>();
query.for_each(&world, |(pos, vel)| {
    pos.x += vel.x;
    pos.y += vel.y;
});

assert_eq!(world.get::<Position>(e).unwrap().x, 1.0);
```

批量操作会预算 column offset，避免逐 entity 的查表开销：

```rust
world.spawn_batch((0..10_000).map(|i| {
    (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
}));
```

query 也支持 chunk 级的 slice 访问，方便手动写向量化循环：

```rust
query.for_each_chunk(&world, |(positions, velocities)| {
    for (p, v) in positions.iter_mut().zip(velocities.iter()) {
        p.x += v.x;
    }
});
```

### 设计目标

* **迭代快**：列存储，cache 友好的顺序访问
* **插入快**：预算 column offset，热路径无 hash 查表
* **代码少**：ECS 核心不到 2000 行
* **不搞魔法**：没有 proc macro，没有全局状态，没有隐式并行

### 性能

`benches/` 目录下有对 [hecs](https://github.com/Ralith/hecs) 和 [Bevy ECS](https://github.com/bevyengine/bevy) 的 Criterion 对比。同一台机器、同样 workload、单线程的结论：

* 中等规模下迭代吞吐量大约是 Bevy 的 3-5 倍、hecs 的 2-3 倍。百万 entity 时大家都被内存带宽卡住，Sky 还领先 10-15% 左右。
* 碎片化迭代（大量 archetype）是 chunk 存储最受益的场景，大约比 Bevy 快 10 倍、比 hecs 快 2 倍。
* 批量插入经过 `write_fast` 优化后大约是两者的 2 倍。

自己跑一下：

```sh
cargo bench
```

附带一个粒子模拟 example，单线程 100 万 entity 跑到 80 FPS：

```sh
cargo run --example particles --release --features demo
```

### 文档

* **[API 文档](docs/api.md)** — 完整接口说明
* **[Benchmark 记录](BENCHMARKS.md)** — 详细数据和 chunk size 调优过程
* **[Examples](examples/)** — 可运行的示例

### 相关项目

SkyEngine 的 ECS 受到 Rust 生态中已有工作的启发。如果不适合你的场景，可以看看：

- [bevy](https://github.com/bevyengine/bevy) — 功能完整的引擎，插件生态成熟
- [hecs](https://github.com/Ralith/hecs) — 精简高质量的 archetype ECS 库
- [flecs](https://github.com/SanderMertens/flecs) — 功能丰富的 C/C++ ECS，有 Rust binding

### 许可

MIT ([LICENSE](LICENSE))
