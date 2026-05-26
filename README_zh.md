<p align="center">
  <h1 align="center">🚀 SkyEngine</h1>
  <p align="center">
    <strong>高性能 · 块列式 ECS · wgpu 2D 渲染 · Rust 原生</strong>
  </p>
  <p align="center">
    <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-2021_Edition-orange?logo=rust&logoColor=white" alt="Rust"></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT"></a>
    <a href="https://github.com/nicories/wgpu"><img src="https://img.shields.io/badge/GPU-wgpu_24-green?logo=webgpu" alt="wgpu"></a>
    <a href="benches/BENCHMARKS_CN.md"><img src="https://img.shields.io/badge/Bench-Criterion-purple" alt="Criterion"></a>
  </p>
</p>

<p align="center">
  <a href="README_EN.md">English</a> · <a href="#-快速上手">快速上手</a> · <a href="#-性能基准">性能基准</a> · <a href="docs/reference/index.md">API 文档</a> · <a href="docs/reference/scene.md">Persistence 文档</a> · <a href="docs/reference/physics.md">Physics 文档</a> · <a href="#-示例展示">示例展示</a>
</p>

---

## 📖 简介

**SkyEngine** 是一款 Rust 原生 2D 游戏引擎。核心特性包括：

- **ECS 架构**：高性能的实体组件系统
- **现代渲染**：基于 `RenderPipelineAsset + RenderComposer` 的高层可编程场景管线，以及基于 `wgpu` 的专家级 RenderGraph
- **Persistence / Prefab**：`scene` feature 提供 `#[persist(component)]`、World 存档、Prefab 子树保存和文档中间层
- **可选 2D 物理**：`physics` feature 提供 top-down/Tiled 2D 物理、事件、查询和 debug draw
- **数学模块**：提供 engine-owned 的 `sky_engine::math` 公共数学层，当前内部基于 `glam`
- **简单易用**：用户友好的API，详细的文档

🚧 项目目前处于快速开发阶段，欢迎贡献！

---

### 为什么选择 SkyEngine？
- 在性能测试中，我们ECS处于领先地位。
  - 迭代性能 **2.7x–4.2x 优于 hecs**
  - 完整帧模拟 **领先 hecs 14%、领先 Bevy 19%**
- 精心设计的接口，API 简洁直观
- 原生支持AI协作


---


## 🚀 快速上手

### 前置条件

- [Rust](https://www.rust-lang.org/tools/install) 稳定版（推荐 1.80+）

### 安装

```bash
git clone https://github.com/jz315/SkyEngine.git
cd SkyEngine
```

### 最小 ECS 示例

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    // 创建单个实体
    let entity = world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 2.0 },
    ));

    // 批量创建 10,000 个实体
    world.spawn_batch((0..10_000).map(|i| {
        (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
    }));

    // 类型化查询 — 自动缓存匹配 Archetype
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&mut world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // Chunk 级迭代 — 返回连续切片，适合 SIMD
    query.for_each_chunk(&mut world, |(positions, velocities)| {
        for (p, v) in positions.iter_mut().zip(velocities.iter()) {
            p.x += v.x * 0.016;
        }
    });

    let pos = world.get::<Position>(entity).unwrap();
    println!("({}, {})", pos.x, pos.y);
}
```

---

## 📊 性能基准

所有数据来自 `cargo bench --bench fair` 公平横向对比，使用 Criterion 框架在同一台 Windows 机器上采集。详细历史记录见 [BENCHMARKS.md](benches\BENCHMARKS_CN.md)。

### 迭代性能

| 工作负载 | Sky 🚀 | hecs | Bevy | Sky 优势 |
|---------|--------|------|------|----------|
| 简单迭代 10K | **1.98 µs** | 5.56 µs | 8.15 µs | ⚡ 2.8x |
| 碎片化迭代 10K | **1.06 µs** | 3.20 µs | 6.14 µs | ⚡ 3.0x |
| 矩阵计算 100K | **1.90 ms** | 2.37 ms | 2.03 ms | ⚡ 1.2x |

### 结构操作

| 工作负载 | Sky 🚀 | hecs | Bevy | Sky 优势 |
|---------|--------|------|------|----------|
| 批量插入 10K | **135 µs** | 290 µs | 274 µs | ⚡ 2.1x |
| 创建/销毁 1K | **26.3 µs** | 25.2 µs | 58.8 µs | ≈ hecs |
| 组件增删 1K | **58.8 µs** | 59.1 µs | 88.2 µs | ≈ hecs |

### 综合帧模拟

| 工作负载 | Sky 🚀 | hecs | Bevy |
|---------|--------|------|------|
| **完整游戏帧** | **181 µs** | 211 µs | 223 µs |

> 💡 Sky 在完整帧模拟中 **领先 hecs 14%、领先 Bevy 19%**

```bash
# 运行公平对比基准
cargo bench --bench fair

# 运行指定引擎
cargo bench --bench fair -- sky
cargo bench --bench fair -- hecs
cargo bench --bench fair -- bevy
cargo bench --bench fair -- flecs
```

---

## 🎮 示例展示

完整示例索引见 [`examples/README.md`](examples/README.md)。如果你是第一次接触这个仓库，建议按“ECS 入门 → Render API → 完整 Demo”的顺序阅读。

Physics 文档见 [`docs/reference/physics.md`](docs/reference/physics.md)。相关示例：

```bash
cargo run --example physics_arcade_demo --features "app physics" --release
cargo run --example tiled_physics_demo --features "app physics" --release
```

Persistence / Prefab 文档见 [`docs/reference/scene.md`](docs/reference/scene.md)：

```bash
cargo run --example scene_basic --features scene
```

高层渲染推荐工作流：

- `clear_screen` 这类最小示例直接走 `ctx.gpu()`，不安装高层管线
- `world.install(RenderPlugin::forward_2d())` 安装默认统一场景管线
- 在 `update()` 里调用 `ctx.render()`
- 想改高层执行顺序，就自定义 builder 注册和有序步骤：`phase / compute / pass / postfx / feature`
- 需要组合更多渲染类型时，在同一条 pipeline 里组合多个 feature、extractor、draw function 和 pipeline step
- 需要新增一种渲染类型时，优先实现新的 `RenderFeature`、`Extractor`、`DrawFunction` 或自定义 step
- `StandardMaterial` 的 normal map 现在走切线空间；`Mesh::from_gltf(...)` 会自动准备 tangent 数据
- `RenderPipelineAsset::forward_3d()` 现在会为透视视图和可投影的 `DirectionalLight` 自动执行 directional shadow map
- 只有在需要直接控制 graph / pass / target 时，才下潜到 `render::expert::*`
- 专家级 backend 示例看 `render_graph_showcase` / `frame_pipeline_showcase`，性能统计看 `renderer_probe`

---

## 🗺️ Roadmap

- [x] 块列式 Archetype ECS
- [x] 类型化查询 + 编译期过滤
- [x] 延迟命令与批量 Spawn
- [x] 系统分组调度
- [x] wgpu GPU 上下文
- [x] 声明式 RenderGraph
- [x] SpriteBatch 2D 渲染
- [x] 动态光照 + 后处理管线
- [x] Live2D Cubism 集成
- [ ] 并行化系统调度
- [ ] 资产管线热重载
- [ ] 场景序列化 / 反序列化
- [ ] 粒子系统 GPU 加速
- [ ] 编辑器 GUI
- [ ] 音频系统

---

## 📄 文档

| 文档 | 说明 |
|------|------|
| [文档索引](docs/README.md) | 文档总入口 |
| [API Reference](docs/reference/index.md) | 正式 API 文档入口 |
| [ECS](docs/reference/ecs.md) | World、Query、Commands、Schedule |
| [Reflect](docs/reference/reflect.md) | Type layout 反射与 derive Inspector 反射 |
| [Render](docs/reference/render.md) | 高层渲染模块导览 |
| [Persistence](docs/reference/scene.md) | World / Prefab / Save |
| [Physics](docs/reference/physics.md) | 2D physics 和 Tiled physics |
| [benches/BENCHMARKS_CN.md](benches/BENCHMARKS_CN.md) | 基准测试方法论与历史记录 |
| [src/render/AGENTS.md](src/render/AGENTS.md) | 渲染模块架构指南 |
| [src/render/graph/AGENTS.md](src/render/graph/AGENTS.md) | RenderGraph 详细设计文档 |

---

## 🙏 致谢

- [SakuraEngine](https://github.com/SakuraEngine/SakuraEngine)  — 图形学部分参考了 SakuraEngine 的实现，特此感谢。
- [hecs](https://github.com/Ralith/hecs)
- [Bevy](https://github.com/bevyengine/bevy)
- [wgpu](https://github.com/gfx-rs/wgpu)

---

## 📜 许可证

本项目采用 [MIT 许可证](LICENSE) 开源。

---

<p align="center">
  使用 Rust 🦀 和 ❤️ 构建
</p>
