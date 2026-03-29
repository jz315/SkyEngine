# SkyEngine

用 Rust 写的 ECS，基于 chunk 列存储。核心代码不到 2000 行，跑得比 Bevy 和 Hecs 都快。

## 性能

同机器、同 workload 对比：

| | SkyEngine | Hecs | Bevy |
|---|---|---|---|
| 迭代 (10k entity, 查 2/4 组件) | **1.7 µs** | 5.3 µs | 8.7 µs |
| 碎片化迭代 (26 archetype) | **104 ns** | 215 ns | 989 ns |
| 批量插入 (10k entity) | **135 µs** | 282 µs | 305 µs |

5M entity 规模下领先 hecs 10-15%，再往上大家都被内存带宽卡住了。

粒子压测：**100 万 entity，80 FPS**，单线程，有物理有渲染。

## 为什么快

组件数据按列连续存在 512KB 的 chunk 里，CPU prefetcher 直接命中。query 缓存 archetype 匹配结果，batch insert 预算 column offset，热路径零查表。

```
一个 chunk 长这样：
[Pos Pos Pos ...][Vel Vel Vel ...][Hp Hp Hp ...]
     列 0             列 1            列 2
```

迭代的时候 CPU 顺着内存读，不跳来跳去。

## 用法

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Pos { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Vel { x: f32, y: f32 }

let mut world = World::new();

// 单个
world.spawn((Pos { x: 0.0, y: 0.0 }, Vel { x: 1.0, y: 0.0 }));

// 批量
world.spawn_batch((0..10_000).map(|i| {
    (Pos { x: i as f32, y: 0.0 }, Vel { x: 1.0, y: 1.0 })
}));

// 查询
let mut q = world.query::<(&mut Pos, &Vel)>();
q.for_each(&world, |(pos, vel)| {
    pos.x += vel.x * 0.016;
});

// chunk 级遍历，适合手动 SIMD
q.for_each_chunk(&world, |(positions, velocities)| {
    for (p, v) in positions.iter_mut().zip(velocities.iter()) {
        p.x += v.x * 0.016;
    }
});
```

## 功能

- 类型化 `PreparedQuery`，epoch 缓存失效
- `With<T>` / `Without<T>` 过滤，`Option<&T>` 可选访问
- `spawn` / `spawn_batch` / `despawn` / `insert` / `remove`
- `get` / `get_mut` 随机访问
- 延迟命令 `Commands`
- 全局 Resource 存储
- 分组调度器 + 固定时间步长
- `System` trait（init / run / teardown）
- 动态查询（给脚本/工具用）
- 代际 EntityId

## 运行

```bash
cargo test                                             # 测试
cargo bench                                            # 全部 benchmark
cargo run --example particles --release --features demo  # 粒子 demo
```

## 目录

```
src/ecs/           ECS 核心（world, chunk, archetype, query, bundle, system）
src/reflect/       运行时类型注册
benches/           Criterion benchmark（insert, iter, entity, 对比）
examples/          粒子模拟 demo
docs/api.md        API 文档
```

## 许可

MIT
