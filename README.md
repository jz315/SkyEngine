# SkyEngine

SkyEngine is a production-oriented Rust game engine for building data-driven
2D and hybrid game runtime systems. It combines a fast chunk-based ECS core, a
programmable render stack, and opt-in engine modules for assets, UI, tile maps,
physics, audio, video, scenes, and visual-novel style presentation.

The project is organized around practical engine use: stable public entry
points, feature-gated subsystems, runnable examples, benchmark coverage, and
release gates that keep the core runtime maintainable as the engine grows.

## What It Is

- A Rust-native game engine runtime.
- A practical engine stack for ECS, rendering, UI, asset, scene, tile, physics,
  audio, video, and VN workflows.
- A curated official example set, plus local showcase sources that exercise the
  engine as real application code.
- A codebase that stays small enough for contributors to understand, change,
  and extend.

## What It Can Do Today

- Run ECS-only examples without a GPU.
- Render sprite, tilemap, material, lighting, shadow, and post-processing
  workloads through a wgpu-based stack.
- Drive windowed app examples with input, assets, screenshots, and frame
  lifecycle support.
- Build native UI overlays with retained UI, EUI-NEO-style UI, yakui, or egui.
- Load and edit high-level tile map data and bridge it into rendering.
- Experiment with optional physics, audio, video, Live2D, scene, and VN modules.
- Compare ECS workloads against other Rust ECS libraries through Criterion
  benchmarks.

## Project Status

SkyEngine is pre-1.0 and production-directed. The ECS core, app runner,
rendering architecture, asset pipeline, UI host, tile model, and official
examples are treated as release surfaces, with compatibility tracked through
documentation, tests, examples, and benchmark policy.

Breaking API changes may still happen before 1.0, but they should be deliberate,
documented in the changelog, and covered by the release checklist.

## Where To Start

- [Examples Guide](examples/README.md) for the curated runnable learning path.
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
cargo run --example asset_cook_smoke --features asset
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
- Official examples, local showcase sources, and comparison examples

## License

SkyEngine is licensed under the [MIT License](LICENSE).
