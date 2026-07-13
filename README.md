<p align="center">
  <img src="assets/brand/skyengine-logo-skyline.svg" alt="SkyEngine" width="560">
</p>

# SkyEngine

**SkyEngine is a data-oriented Rust game engine built around a fast, typed,
chunk-based ECS.**

It combines a standalone high-performance ECS core with an evolving engine stack
for rendering, assets, app runtime, UI, tiles, scenes, physics, audio, video, and
tooling experiments.

The goal is simple: make game and interactive-runtime code feel direct,
predictable, and fast.

[![sky_ecs on crates.io](https://img.shields.io/crates/v/sky_ecs.svg)](https://crates.io/crates/sky_ecs)
[![sky_ecs docs](https://docs.rs/sky_ecs/badge.svg)](https://docs.rs/sky_ecs)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Highlights

- Fast chunk-based ECS with columnar component storage
- World-bound typed queries, cached plans, optional params, and type filters
- Chunk iteration APIs for hot loops and batch processing
- Typed system parameters, deterministic parallel stages, deferred commands, and bounded fixed steps
- Runtime-typed dynamic ECS APIs for tools, scripting, and reflection workflows
- Programmable wgpu render stack with render features, phases, and render graph
- Feature-gated modules for assets, UI, tiles, scenes, physics, audio, video, and VN runtime
- Official examples and cross-engine ECS benchmarks

## Sky ECS

Sky ECS is the performance core of the project and is available as a standalone
crate:

```bash
cargo add sky_ecs
```

```rust
use sky_ecs::{ParView, Res, Time, Update, World};

#[derive(Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Velocity {
    x: f32,
    y: f32,
}

fn movement(bodies: ParView<(&mut Position, &Velocity)>, time: Res<Time>) {
    bodies.par_for_each(|(position, velocity)| {
        position.x += velocity.x * time.delta;
        position.y += velocity.y * time.delta;
    });
}

fn main() {
    let mut world = World::new();

    world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 2.0 },
    ));

    world.stage(Update).add(movement);
    world.tick_with_delta(1.0 / 60.0).unwrap();
}
```

Sky ECS gives you:

- generational entities
- bundle-based spawning
- world-cached typed queries plus explicit `PreparedQuery` for advanced hot paths
- named `#[derive(QueryData)]` items plus `With<T>` / `Without<T>` / `Any<(...)>` filters
- optional components through `Option<&T>` and `Option<&mut T>`
- cache-friendly chunk iteration
- cached `par_for_each` / `par_for_each_chunk` execution with automatic serial fallback
- access-inferred `View` / `ParView` / `Res` / `Commands` systems with deterministic parallel waves
- typed stages, explicit exclusive barriers, panic-safe ticks, and bounded fixed-step execution
- dynamic ECS access for editor/tooling scenarios
- expert APIs for low-level engine and benchmark use

## Why SkyEngine

SkyEngine is designed around a data-oriented runtime model.

Instead of hiding everything behind a large framework, the engine keeps its core
systems explicit: entities live in chunks, render work is assembled through
features and phases, app services are feature-gated, and examples are meant to
show the real APIs.

This makes SkyEngine useful both as:

- a game/runtime engine in progress
- a focused ECS crate
- a research-friendly codebase for engine architecture
- a practical benchmark bed for data-oriented Rust systems

## Quick Start

Run ECS examples without GPU features:

```bash
cargo run --example hello_ecs
cargo run --example queries
cargo run --example commands
cargo run --example systems
```

Run app/render examples:

```bash
cargo run --example clear_screen --features app
cargo run --example sprite_demo --features app
cargo run --example tilemap_demo --features app
```

Run asset examples:

```bash
cargo run --example asset_cook_smoke --features asset
cargo run --example asset_load_texture --features asset
cargo run --example asset_custom_factory --features asset
```

Run UI examples:

```bash
cargo run --example ui_legacy_hud_menu --features ui-legacy
cargo run --example ui_serein_eui_gallery --features ui-serein
cargo run --example ui_yakui_demo --features yakui-ui
```

## Engine Modules

SkyEngine currently includes:

| Area | What it provides |
| --- | --- |
| ECS | entities, components, bundles, typed queries, filters, resources, commands, schedules |
| Render | wgpu runtime, render features, phases, render graph, sprites, tilemaps, materials |
| App | window loop, input sync, frame lifecycle, screenshots, GPU context |
| Asset | typed handles, cooked assets, texture loading, custom factories |
| UI | backend-neutral UI host, retained UI, EUI-NEO-style UI, yakui, egui overlay |
| Tile | tile scene model, editable maps, Tiled import/export bridge |
| Scene | scene and prefab documents |
| Physics | optional Rapier-backed 2D physics |
| Media | optional audio and video runtime |
| VN | visual-novel style runtime and presentation helpers |

## Benchmarks

Cross-engine ECS comparisons live in a separate package under
`tools/ecs-comparison`.

```bash
cargo compare-ecs
cargo compare-ecs -- sky
cargo compare-ecs -- fair_random_access/get/sky --exact
```

The comparison suite focuses on workloads that Sky ECS, `hecs`, `bevy_ecs`, and
`flecs_ecs` can all express through safe public APIs.

## Repository Layout

```text
crates/sky_ecs/          Standalone ECS crate
crates/sky_type/         Shared type metadata helpers
crates/sky_profile/      Optional profiling support
src/ecs/                 sky_engine ECS facade
src/render/              Render runtime, features, phases, graph
src/app/                 Window and app lifecycle
src/asset/               Asset server, handles, cooked assets
src/ui/                  UI host and backend integrations
src/tile/                Tile scene and authoring model
examples/                Official examples and local showcase sources
tools/ecs-comparison/    Cross-engine ECS benchmark suite
docs/                    User and developer documentation
benches/                 SkyEngine-local benchmarks
```

## Documentation

- [Examples](examples/README.md)
- [ECS Reference](docs/reference/ecs.md)
- [Render Reference](docs/reference/render.md)
- [Asset Reference](docs/reference/asset.md)
- [UI Reference](docs/reference/ui.md)
- [Benchmark Notes](benches/BENCHMARKS.md)

## Status

SkyEngine is under active development. The ECS crate is published separately as
`sky_ecs`, and the engine modules are growing around it with examples,
benchmarks, and release checks.

## License

SkyEngine is licensed under the [MIT License](LICENSE).
