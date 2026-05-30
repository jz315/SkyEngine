# SkyEngine

SkyEngine is an experimental Rust game engine for building data-driven game
runtime systems. It combines a fast ECS core, a programmable renderer, and a
set of opt-in engine modules for assets, UI, tile maps, physics, audio, video,
scenes, and visual-novel style presentation.

The goal is not to be a finished commercial engine yet. SkyEngine is a compact
engine workspace for exploring how a modern Rust game runtime can be organized:
entity storage, rendering composition, asset lifetime, UI integration, gameplay
examples, and editor-adjacent data models all live in one repo.

## What It Is

- A Rust-native game engine research project.
- A practical playground for ECS, rendering, UI, asset, scene, tile, physics,
  audio, video, and VN runtime ideas.
- A collection of focused examples and vertical slices that exercise the engine
  as real application code.
- A codebase that stays small enough for contributors to understand, change,
  and extend.

## What It Can Do Today

- Run ECS-only examples without a GPU.
- Render sprite, tilemap, material, lighting, shadow, and post-processing
  experiments through a wgpu-based stack.
- Drive windowed app examples with input, assets, screenshots, and frame
  lifecycle support.
- Build native UI overlays with retained UI, EUI-NEO-style UI, yakui, or egui.
- Load and edit high-level tile map data and bridge it into rendering.
- Experiment with optional physics, audio, video, Live2D, scene, and VN modules.
- Compare ECS workloads against other Rust ECS libraries through Criterion
  benchmarks.

## Project Status

SkyEngine is in active development. The ECS, render architecture, examples, and
several runtime modules are usable, but public APIs are still changing as the
engine is split into cleaner layers.

Use it if you want to study or extend an engine codebase. Expect movement if
you want a stable dependency.

## Where To Start

- [Examples Guide](examples/README.md) for the runnable learning path.
- [Documentation Index](docs/README.md) for all docs.
- [API Reference](docs/reference/index.md) for module-level API docs.
- [Render Reference](docs/reference/render.md) for the rendering stack.
- [ECS Reference](docs/reference/ecs.md) for the entity/component core.
- [Benchmarks](benches/BENCHMARKS.md) for local benchmark history and policy.
- [Chinese README](README_zh.md) for the Chinese overview.

## Asset Quick Start

The asset examples create temporary source and cooked data, so they can be run
without preparing a project asset folder:

```bash
cargo run --example asset_load_texture --features asset
cargo run --example asset_hot_reload_texture --features asset
cargo run --example asset_load_with_dependency --features asset
cargo run --example asset_custom_factory --features asset
```

See [Asset Reference](docs/reference/asset.md) for `Assets`, strong
`Handle<T>`, `WeakHandle<T>`, typed `AssetPath<T>`, hot reload, cook registry,
and custom factory details.

## Main Areas

- ECS and scheduling
- Rendering and renderer backends
- Assets and runtime resources
- App lifecycle and input
- UI backends
- Tile maps and Tiled IO
- Scene and prefab documents
- Physics, audio, video, Live2D, and VN runtime support
- Benchmarks and comparison examples

## License

SkyEngine is licensed under the [MIT License](LICENSE).
