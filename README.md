<p align="center">
  <h1 align="center">🚀 SkyEngine</h1>
  <p align="center">
    <strong>高性能 · 块列式 ECS · wgpu 2D 渲染 · Rust 原生</strong>
  </p>
  <p align="center">
    <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-2021_Edition-orange?logo=rust&logoColor=white" alt="Rust"></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT"></a>
    <a href="https://github.com/nicories/wgpu"><img src="https://img.shields.io/badge/GPU-wgpu_24-green?logo=webgpu" alt="wgpu"></a>
    <a href="BENCHMARKS.md"><img src="https://img.shields.io/badge/Bench-Criterion-purple" alt="Criterion"></a>
  </p>
</p>

<p align="center">
  <a href="README_EN.md">English</a> · <a href="#-快速上手">快速上手</a> · <a href="#-性能基准">性能基准</a> · <a href="docs/api.md">API 文档</a> · <a href="#-示例展示">示例展示</a>
</p>

---

## 📖 简介

**SkyEngine** 是一款 Rust 原生 2D 游戏引擎。核心特性包括：

- **ECS 架构**：高性能的实体组件系统
- **现代渲染**：基于 wgpu 的 2D 渲染管线
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
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // Chunk 级迭代 — 返回连续切片，适合 SIMD
    query.for_each_chunk(&world, |(positions, velocities)| {
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



---

## 📁 项目结构

```
SkyEngine/
├── src/
│   ├── lib.rs                  # Crate 入口，全局分配器，模块导出
│   ├── ecs/                    # 🏗️ ECS 核心
│   │   ├── world.rs            #   World：实体、Archetype、资源、调度
│   │   ├── archetype.rs        #   Archetype 定义与构建器
│   │   ├── chunk.rs            #   Chunk 块分配与列式存储
│   │   ├── query/              #   类型化查询、过滤器、动态查询
│   │   ├── bundle.rs           #   Tuple Bundle trait
│   │   ├── commands.rs         #   延迟命令缓冲
│   │   ├── system.rs           #   System trait 与分组调度
│   │   ├── entity.rs           #   代际 EntityId
│   │   ├── resource.rs         #   类型化单例资源
│   │   └── raw.rs              #   底层 / 兼容 API
│   ├── gpu/                    # 🖥️ GPU 上下文 (feature: app)
│   │   └── context.rs          #   GpuContext — wgpu Device/Queue/Surface
│   ├── render/                 # 🎨 2D 渲染框架 (feature: app)
│   │   ├── core/               #   Camera2D, Color, Texture, RenderTarget
│   │   ├── graph/              #   声明式 RenderGraph
│   │   ├── passes/             #   SpriteBatch, LightPass, CompositePass
│   │   ├── postfx/             #   Bloom, ToneMap, Vignette
│   │   ├── resources/          #   TextureAtlas, Blackboard, Material
│   │   ├── shaders/            #   WGSL 着色器 (8 个)
│   │   ├── light.rs            #   Light2D 点光源
│   │   └── live2d/             #   Live2D Cubism 渲染 (feature: live2d)
│   ├── app/                    # 🚀 应用框架 (feature: app)
│   │   ├── runner.rs           #   AppRunner — winit 事件循环
│   │   ├── config.rs           #   AppConfig — 窗口配置
│   │   └── input.rs            #   Input — 键鼠状态
│   └── reflect/                # 🔍 运行时类型注册
├── examples/                   # 📚 可运行示例
│   ├── README.md               #   示例索引与推荐学习路径
│   ├── ecs/                    #   纯 ECS 教程 (5 个)
│   ├── render/                 #   渲染 API 展示 + Live2D
│   ├── demo/                   #   完整 GPU Showcase Demo
│   ├── compare/                #   跨引擎对比示例
│   └── legacy/                 #   历史保留的 SkyEngine CPU Demo
├── benches/                    # 📊 Criterion 基准测试
│   └── fair/                   #   公平横向对比 (Sky vs hecs vs Bevy)
├── docs/                       # 📖 文档
│   └── api.md                  #   ECS API 参考
├── BENCHMARKS.md               # 基准测试历史记录
├── Cargo.toml
└── LICENSE                     # MIT
```

---

## ⚙️ Feature Flags

SkyEngine 使用 Cargo Feature Flags 按需启用功能模块：

| Feature | 描述 | 依赖 |
|---------|------|------|
| `app` | 完整应用框架（窗口 + GPU + 输入） | wgpu, winit, pollster, bytemuck |
| `asset` | 资源加载（纹理等） | image |
| `demo` | GPU 加速 Demo | app + asset + rand |
| `live2d` | Live2D Cubism SDK 集成 | app + asset + cubism-sys + serde_json |
| `demo-legacy` | 旧版 CPU 渲染 Demo | minifb + rand |
| `compare` | 跨引擎对比示例 | demo-legacy + hecs + bevy_ecs |
| `compare-bevy` | 完整 Bevy GPU 对比 | bevy |

```bash
# 仅 ECS — 零外部依赖
cargo test

# ECS + 渲染
cargo test --features app

# 完整 Demo
cargo run --example boids --features app --release
```

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
| [API 参考](docs/api.md) | ECS 完整 API 文档（中文） |
| [BENCHMARKS.md](BENCHMARKS.md) | 基准测试方法论与历史记录 |
| [src/render/AGENTS.md](src/render/AGENTS.md) | 渲染模块架构指南 |
| [src/render/graph/AGENTS.md](src/render/graph/AGENTS.md) | RenderGraph 详细设计文档 |

---

## 🔗 相关项目

| 项目 | 说明 |
|------|------|
| [hecs](https://github.com/Ralith/hecs) | 精简的 Archetype ECS |
| [Bevy](https://github.com/bevyengine/bevy) | 完整游戏引擎，插件生态 |
| [flecs](https://github.com/SanderMertens/flecs) | C99 实现，功能丰富的 ECS |
| [wgpu](https://github.com/gfx-rs/wgpu) | 跨平台 GPU 抽象层 |

---

## 🙏 致谢

- [SakuraEngine](https://github.com/SakuraEngine/SakuraEngine)  — 引擎很多内容都参考了 SakuraEngine 的实现，特此感谢。

---

## 📜 许可证

本项目采用 [MIT 许可证](LICENSE) 开源。

---

<p align="center">
  使用 Rust 🦀 和 ❤️ 构建
</p>
