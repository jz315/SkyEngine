<p align="center">
  <h1 align="center">🚀 SkyEngine</h1>
  <p align="center">
    <strong>High Performance · Chunk-Columnar ECS · wgpu 2D Rendering · Pure Rust</strong>
  </p>
  <p align="center">
    <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-2021_Edition-orange?logo=rust&logoColor=white" alt="Rust"></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT"></a>
    <a href="https://github.com/nicories/wgpu"><img src="https://img.shields.io/badge/GPU-wgpu_24-green?logo=webgpu" alt="wgpu"></a>
    <a href="benches/BENCHMARKS.md"><img src="https://img.shields.io/badge/Bench-Criterion-purple" alt="Criterion"></a>
  </p>
</p>

<p align="center">
  <a href="README.md">中文</a> · <a href="#-quick-start">Quick Start</a> · <a href="#-benchmarks">Benchmarks</a> · <a href="docs/api.md">API Docs</a> · <a href="docs/scene.md">Scene Docs</a> · <a href="docs/physics.md">Physics Docs</a> · <a href="#-examples">Examples</a>
</p>

---

## 📖 Introduction

**SkyEngine** is a high-performance game engine built from scratch in Rust, standing on two pillars:

1. **Chunk-Columnar Archetype ECS** — Components of the same type are stored contiguously within fixed-size memory chunks, naturally aligning with hardware prefetching for extreme iteration performance.
2. **Programmable Scene Rendering** — A high-level `RenderPipelineAsset + RenderComposer` model coordinates ECS 2D, Live2D, and future renderer features on top of a declarative render graph, with built-in SpriteBatch, dynamic lighting, Bloom/ToneMap/Vignette post-processing, and Live2D Cubism SDK integration.

SkyEngine takes the **library approach**: no proc macros, no global state, no imposed application structure. Use just the ECS, or combine it with the full rendering pipeline — everything is opt-in.

> **Why SkyEngine?**
> - In fair benchmarks, iteration performance is **2.7x–4.2x faster than hecs**, with overall frame simulation **14% ahead of hecs, 19% ahead of Bevy**.
> - Pure Rust, zero-unsafe typed query API while retaining a low-level raw API for tooling and scripting.
> - Batteries-included rendering: RenderGraph with automatic resource aliasing, transient allocation, and dead-pass culling — no manual GPU resource lifetime management.
> - An engine-owned `sky_engine::math` layer now fronts shared math types while the current backend remains `glam`.

---

## ✨ Key Features

### 🏗️ ECS Core

| Feature | Description |
|---------|-------------|
| **Chunk-Columnar Storage** | Each archetype organized into fixed-size chunks with per-type contiguous columns, aligning iteration with hardware prefetch |
| **Typed Queries** | `world.query::<(&mut Pos, &Vel)>()` returns a `PreparedQuery` with automatic archetype caching |
| **Optional Components** | Queries support `Option<&T>` / `Option<&mut T>` for optional component access |
| **Compile-Time Filters** | `With<T>` / `Without<T>` and tuple combinations with zero runtime overhead |
| **Batch Insert** | `spawn_batch()` skips per-entity lookups — 2x faster than hecs at 10K entities |
| **Deferred Commands** | `Commands` schedules structural changes during active queries, coalesced and batch-applied |
| **System Scheduling** | Groups + fixed-timestep policy, `world.tick()` drives a complete frame |
| **Generational Entities** | `EntityId` with generation tracking — stale handles auto-invalidate on slot reuse |
| **Chunk Iteration** | `for_each_chunk()` yields contiguous slices for manual SIMD vectorization |
| **Scene / Prefab** | Optional `scene` feature for entity-tree documents, stable IDs, hierarchy, and prefab spawning |

### 🎨 Rendering Framework (feature = `"app"`)

| Feature | Description |
|---------|-------------|
| **Declarative RenderGraph** | Compile-time topological sort + resource aliasing + transient allocation + dead-pass culling |
| **SpriteBatch** | High-performance 2D sprite batch rendering with texture atlases and material instances |
| **Dynamic Lighting** | `LightPass` + `Light2D` point lights with color temperature, radius, and intensity |
| **Post-Processing** | `Bloom` · `ToneMap` · `Vignette` — composable PostFx chain |
| **Material System** | `MaterialInstance` + `MaterialPipelineCache` for data-driven pipelines |
| **Texture Atlas** | `TextureAtlas` + `AtlasPacker` auto-packing to reduce draw calls |
| **2D Camera** | `Camera2D` orthographic projection with zoom / pan / viewport fitting |
| **Live2D Integration** | Cubism SDK v5 native bindings, per-drawable GPU rendering (feature = `"live2d"`) |

---

## 🛠️ Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust 2021 Edition |
| Global Allocator | mimalloc |
| Hashing | rustc-hash (FxHashMap) |
| GPU Backend | wgpu 24 |
| Windowing | winit 0.30 |
| Shaders | WGSL |
| Benchmarking | Criterion 0.8 |
| Comparison Engines | hecs · bevy_ecs · flecs_ecs |

---

## 🚀 Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) stable (1.80+ recommended)
- GPU rendering examples require Vulkan / DX12 / Metal capable drivers

### Installation

```bash
git clone https://github.com/your-username/SkyEngine.git
cd SkyEngine
```

### Minimal ECS Example

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

    // Batch-spawn 10,000 entities
    world.spawn_batch((0..10_000).map(|i| {
        (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
    }));

    // Typed query — auto-cached archetype matching
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // Chunk-level iteration — contiguous slices for SIMD
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

## 📊 Benchmarks

All data from `cargo bench --bench fair` — apples-to-apples comparison using Criterion on the same Windows machine. Full history in [benches/BENCHMARKS.md](benches/BENCHMARKS.md).

### Iteration Performance

| Workload | Sky 🚀 | hecs | Bevy | Sky Advantage |
|----------|--------|------|------|---------------|
| Simple Iteration 10K | **1.98 µs** | 5.56 µs | 8.15 µs | ⚡ 2.8x |
| Fragmented Iteration 10K | **1.06 µs** | 3.20 µs | 6.14 µs | ⚡ 3.0x |
| Heavy Compute 100K | **1.90 ms** | 2.37 ms | 2.03 ms | ⚡ 1.2x |

### Structural Operations

| Workload | Sky 🚀 | hecs | Bevy | Sky Advantage |
|----------|--------|------|------|---------------|
| Batch Insert 10K | **135 µs** | 290 µs | 274 µs | ⚡ 2.1x |
| Spawn/Despawn 1K | **26.3 µs** | 25.2 µs | 58.8 µs | ≈ hecs |
| Add/Remove Component 1K | **58.8 µs** | 59.1 µs | 88.2 µs | ≈ hecs |

### Full Frame Simulation

| Workload | Sky 🚀 | hecs | Bevy |
|----------|--------|------|------|
| **Complete Game Frame** | **181 µs** | 211 µs | 223 µs |

> 💡 Sky leads by **14% over hecs, 19% over Bevy** in full frame simulation

```bash
cargo bench --bench fair          # fair cross-engine comparison
cargo bench --bench fair -- sky   # Sky only
cargo bench --bench fair -- hecs  # hecs only
cargo bench --bench fair -- bevy  # Bevy only
```

---

## 🎮 Examples

See [`examples/README.md`](examples/README.md) for the full example index and recommended learning order. If you're new to the repo, start with ECS tutorials, then move to render showcases, then the full demos.

Recommended high-level render workflow:

- `clear_screen` and similar minimal samples stay on the no-pipeline `ctx.gpu()` path
- install the default unified scene pipeline with `App::with_render_pipeline(RenderPipelineAsset::forward_2d())`
- call `ctx.render()` inside `update()`
- customize the high-level flow by registering your own `phase / compute / pass / postfx / feature` steps
- combine multiple renderer families by composing multiple features, extractors, and draw functions into one pipeline
- add a brand new renderer type by implementing a `RenderFeature`, `Extractor`, `DrawFunction`, or custom pipeline step
- `StandardMaterial` normal maps now use tangent-space shading, and `Mesh::from_gltf(...)` prepares tangent data for that path
- `RenderPipelineAsset::forward_3d()` now runs a directional shadow-map phase automatically for perspective views with shadow-casting `DirectionalLight`s
- drop to `render::expert::*` only when you need direct graph / pass / target control

### 1. ECS Tutorials (no GPU required)

```bash
cargo run --example hello_ecs       # minimal starting point
cargo run --example queries         # typed queries
cargo run --example commands        # deferred structural changes
cargo run --example systems         # system scheduling
cargo run --example tiny_defense    # ECS-only complete mini game
```

### 2. Render Learning Path (`--features app`)

Recommended order:

1. `clear_screen` — understand the window, GPU context, and per-frame clear pass
2. `sprite_demo` — add `Camera` and the default high-level scene pipeline path
3. `textured_demo` — move from flat-color sprites to textures and mixed drawing on the same pipeline-driven path
4. `lighting_demo` — introduce normals, lighting composition, bloom, and tonemapping through `App::with_render_pipeline(...)`
5. `render_graph_showcase` — study how the declarative `RenderGraph` organizes resources and passes
6. `frame_pipeline_showcase` — inspect the expert-only `FramePipeline` setup/view/finalize backbone directly
7. `perf_test` — inspect throughput and scaling after the main path is clear
8. `renderer_probe` — inspect scene-pipeline workload and timing stats on the default universal path

`live2d_demo` is the first multi-feature branch and is best read after the main path.

```bash
cargo run --example clear_screen          --features app
cargo run --example sprite_demo           --features app
cargo run --example textured_demo         --features app
cargo run --example lighting_demo         --features app
cargo run --example render_graph_showcase --features app
cargo run --example frame_pipeline_showcase --features app
cargo run --example perf_test             --features app --release
cargo run --example renderer_probe        --features app --release
```

### Render Path Map

```text
clear_screen
  ↓
sprite_demo
  ↓
textured_demo
  ↓
lighting_demo
  ↓
render_graph_showcase
  ↓
frame_pipeline_showcase
  ↓
perf_test
  ↓
renderer_probe

specialized branch: live2d_demo
```

### 3. Full Showcase Demos (`--features app`)

```bash
cargo run --example boids            --features app --release
cargo run --example boids_classic    --features app --release
cargo run --example cosmic_jellyfish --features app --release
cargo run --example neon_galaxy      --features app --release
```

### 4. Live2D (`--features live2d`)

```bash
cargo run --example live2d_demo --features live2d --release -- <path-to-model3.json>
```

### 5. Compare / Legacy Examples (not part of the main learning path)

```bash
# Cross-engine comparison
cargo run --example boids_bevy_gpu --features compare-bevy --release
cargo run --example boids_hecs     --features compare --release
cargo run --example boids_bevy     --features compare --release

# Historical CPU-rendered demos
cargo run --example particles --features demo-legacy
cargo run --example asteroids --features demo-legacy
cargo run --example snake     --features demo-legacy
```

---

## 📁 Project Structure

```
SkyEngine/
├── src/
│   ├── lib.rs                  # Crate root, global allocator, module exports
│   ├── ecs/                    # 🏗️ ECS core
│   │   ├── world.rs            #   World: entities, archetypes, resources, scheduling
│   │   ├── archetype.rs        #   Archetype definition and builder
│   │   ├── chunk.rs            #   Chunk allocation and columnar storage
│   │   ├── query/              #   Typed queries, filters, dynamic queries
│   │   ├── bundle.rs           #   Tuple Bundle trait
│   │   ├── commands.rs         #   Deferred command buffer
│   │   ├── system.rs           #   System trait and group scheduling
│   │   ├── entity.rs           #   Generational EntityId
│   │   ├── resource.rs         #   Typed singleton resources
│   │   └── raw.rs              #   Low-level tooling API
│   ├── gpu/                    # 🖥️ GPU context (feature: app)
│   ├── render/                 # 🎨 2D rendering framework (feature: app)
│   │   ├── core/               #   Camera2D, Color, Texture, RenderTarget
│   │   ├── graph/              #   Declarative RenderGraph
│   │   ├── passes/             #   SpriteBatch, LightPass, CompositePass
│   │   ├── postfx/             #   Bloom, ToneMap, Vignette
│   │   ├── resources/          #   TextureAtlas, Blackboard, Material
│   │   ├── shaders/            #   WGSL shaders (8)
│   │   ├── light.rs            #   Light2D point lights
│   │   └── live2d/             #   Live2D Cubism renderer (feature: live2d)
│   ├── app/                    # 🚀 Application framework (feature: app)
│   └── reflect/                # 🔍 Runtime type registry
├── examples/                   # 📚 Runnable examples
│   ├── README.md               #   Example index and learning path
│   ├── ecs/                    #   ECS tutorials
│   ├── render/                 #   Render API showcases + Live2D
│   ├── demo/                   #   Full showcase demos
│   ├── compare/                #   Cross-engine comparisons
│   └── legacy/                 #   CPU-only SkyEngine demos
├── benches/                    # 📊 Criterion benchmarks
├── docs/                       # 📖 Documentation
├── benches/BENCHMARKS.md       # Benchmark methodology and history
├── Cargo.toml
└── LICENSE                     # MIT
```

---

## ⚙️ Feature Flags

| Feature | Description | Dependencies |
|---------|-------------|-------------|
| `app` | Full application framework (window + GPU + input) | wgpu, winit, pollster, bytemuck |
| `asset` | Asset loading (textures, etc.) | image |
| `demo` | GPU-accelerated demos | app + asset + rand |
| `live2d` | Live2D Cubism SDK integration | app + asset + cubism-sys + serde_json |
| `demo-legacy` | CPU-rendered demos | minifb + rand |
| `compare` | Cross-engine comparison examples | demo-legacy + hecs + bevy_ecs |
| `compare-bevy` | Full Bevy GPU comparison | bevy |

```bash
# ECS only — zero external dependencies
cargo test

# ECS + rendering
cargo test --features app

# Full demo
cargo run --example boids --features app --release
```

---

## 🗺️ Roadmap

- [x] Chunk-columnar Archetype ECS
- [x] Typed queries + compile-time filters
- [x] Deferred commands and batch spawn
- [x] System group scheduling
- [x] wgpu GPU context
- [x] Declarative RenderGraph
- [x] SpriteBatch 2D rendering
- [x] Dynamic lighting + post-processing pipeline
- [x] Live2D Cubism integration
- [ ] Parallel system scheduling
- [ ] Hot-reloading asset pipeline
- [ ] Scene serialization / deserialization
- [ ] GPU-accelerated particle system
- [ ] Editor GUI
- [ ] Audio system

---

## 🤝 Contributing

Contributions are welcome! Please follow this workflow:

1. **Fork** the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Commit your changes: `git commit -m 'feat: add amazing feature'`
4. Push the branch: `git push origin feature/amazing-feature`
5. Open a **Pull Request**

### Development Guidelines

- Run `cargo test` to ensure all tests pass
- Run `cargo bench --bench fair` to confirm no performance regressions
- Follow existing code style — avoid unnecessary abstractions in hot paths
- Read `src/render/AGENTS.md` before modifying the rendering framework

---

## 📄 Documentation

| Document | Description |
|----------|-------------|
| [Documentation Index](docs/api.md) | Entry point for module docs |
| [ECS](docs/ecs.md) | World, queries, commands, schedule |
| [Reflect](docs/reflect.md) | Type layout reflection and derive-based inspector reflection |
| [Render](docs/render.md) | High-level rendering module guide |
| [Scene](docs/scene.md) | Scene / prefab / save |
| [Physics](docs/physics.md) | 2D physics and Tiled physics |
| [benches/BENCHMARKS.md](benches/BENCHMARKS.md) | Benchmark methodology and history |
| [src/render/AGENTS.md](src/render/AGENTS.md) | Rendering module architecture guide |
| [src/render/graph/AGENTS.md](src/render/graph/AGENTS.md) | RenderGraph detailed design docs |

---

## 🔗 See Also

| Project | Description |
|---------|-------------|
| [hecs](https://github.com/Ralith/hecs) | Minimal archetype ECS |
| [Bevy](https://github.com/bevyengine/bevy) | Full game engine with plugin ecosystem |
| [flecs](https://github.com/SanderMertens/flecs) | Feature-rich ECS in C99 |
| [wgpu](https://github.com/gfx-rs/wgpu) | Cross-platform GPU abstraction layer |

---

## 📜 License

This project is licensed under the [MIT License](LICENSE).

---

<p align="center">
  Built with Rust 🦀 and ❤️
</p>
