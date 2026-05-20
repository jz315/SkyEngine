# SkyEngine

SkyEngine is a Rust game engine project built around a fast chunk-based ECS and
a programmable wgpu rendering stack. It is meant to be a practical engine
workbench: small enough to understand, but broad enough to explore real game
runtime problems such as rendering, assets, UI, audio, video, physics, tile
maps, and visual-novel presentation.

The project is in active development. The ECS, examples, and core rendering
paths are usable, while higher-level engine APIs are still evolving.

[Chinese README](README_zh.md) | [Docs](docs/api.md) |
[Examples](examples/README.md) | [Benchmarks](benches/BENCHMARKS.md)

## What SkyEngine Includes

- A chunk-columnar archetype ECS with typed queries, optional query parameters,
  filters, resources, deferred commands, and grouped system scheduling.
- A high-level wgpu rendering runtime driven by `RenderPipelineAsset`,
  `RenderPipelineBuilder`, `RenderRuntime`, and app-facing `SceneRenderer`
  backends.
- Built-in renderer features for sprites, tilemaps, meshes, materials, lights,
  shadows, post-processing, and experimental backend paths.
- App runtime modules for windowing, frame lifecycle, input, screenshots, asset
  updates, audio, video, logging, and UI overlays.
- Multiple UI routes: retained ECS UI, EUI-NEO-style declarative UI, yakui, and
  egui overlays.
- A high-level tile map model with editing APIs, Tiled IO, and a render bridge.
- Optional modules for scene/prefab documents, Rapier-backed 2D physics, Live2D,
  audio, video, and VN/Galgame-style scripting.

## Quick Start

```bash
git clone https://github.com/jz315/SkyEngine.git
cd SkyEngine
cargo test
```

Run the smallest ECS example:

```bash
cargo run --example hello_ecs
```

Run a windowed render example:

```bash
cargo run --example sprite_demo --features app
```

Run the EUI-NEO gallery:

```bash
cargo run --example eui_neo_gallery --features ui-neo
```

## ECS Example

```rust
use sky_engine::ecs::World;

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

fn main() {
    let mut world = World::new();

    let entity = world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 2.0 },
    ));

    world.spawn_batch((0..10_000).map(|i| {
        (
            Position {
                x: i as f32,
                y: 0.0,
            },
            Velocity { x: 1.0, y: 1.0 },
        )
    }));

    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(position, velocity)| {
        position.x += velocity.x * 0.016;
        position.y += velocity.y * 0.016;
    });

    let position = world.get::<Position>(entity).unwrap();
    println!("({}, {})", position.x, position.y);
}
```

## Example Paths

The full example index is in [examples/README.md](examples/README.md). Good
starting points are:

| Area | Command |
| --- | --- |
| ECS basics | `cargo run --example hello_ecs` |
| Typed queries | `cargo run --example queries` |
| System scheduling | `cargo run --example systems` |
| First window | `cargo run --example clear_screen --features app` |
| Sprites | `cargo run --example sprite_demo --features app` |
| Lighting | `cargo run --example lighting_demo --features app` |
| UI | `cargo run --example hud_menu --features ui` |
| EUI-NEO | `cargo run --example eui_neo_gallery --features ui-neo` |
| Physics | `cargo run --example physics_arcade_demo --features "app physics"` |
| Scene documents | `cargo run --example scene_basic --features scene` |
| VN runtime | `cargo run --example vn_minimal --features vn` |

## Feature Overview

| Feature | Enables |
| --- | --- |
| `app` | winit app runner, GPU context, input, render surface, assets |
| `asset` | asset manifest/server, handles, texture assets, cooked assets |
| `ui` | retained ECS UI compatibility feature |
| `ui-neo` | EUI-NEO-style declarative UI backend |
| `yakui-ui` | experimental yakui UI backend |
| `egui` | egui immediate-mode overlay |
| `scene` | scene and prefab documents |
| `physics` | Rapier-backed 2D physics |
| `audio` | audio runtime and commands |
| `video` | video playback resources |
| `video-ffmpeg` | FFmpeg-backed video decoding |
| `vn` | visual novel runtime |
| `live2d` | Live2D Cubism integration |
| `kajiya-renderer` | experimental Kajiya backend |
| `renderling-renderer` | experimental Renderling backend |

## Repository Map

```text
src/            main engine modules
crates/         reusable internal crates and vendored dependencies
examples/       runnable examples grouped by learning path
docs/           user and developer documentation
benches/        Criterion benchmark suites
```

Important internal crates:

- `crates/sky_ecs`: standalone ECS core used by `sky_engine::ecs`.
- `crates/sky_type`: shared runtime type identity and layout metadata.
- `crates/eui-neo`: headless EUI-NEO-style UI runtime.
- `crates/eui-neo-wgpu`: wgpu renderer support for EUI-NEO.
- `crates/eui-neo-winit`: winit input/platform adapter for EUI-NEO.

## Documentation

| Document | Topic |
| --- | --- |
| [docs/api.md](docs/api.md) | documentation index |
| [docs/ecs.md](docs/ecs.md) | ECS world, entities, queries, commands, schedule |
| [docs/app.md](docs/app.md) | app runner and frame context |
| [docs/render.md](docs/render.md) | rendering guide |
| [docs/render_api.md](docs/render_api.md) | render API reference |
| [docs/asset.md](docs/asset.md) | asset system |
| [docs/ui.md](docs/ui.md) | UI systems |
| [docs/scene.md](docs/scene.md) | scene and prefab documents |
| [docs/physics.md](docs/physics.md) | 2D physics |
| [docs/audio.md](docs/audio.md) | audio runtime |
| [docs/video.md](docs/video.md) | video runtime |
| [docs/vn.md](docs/vn.md) | VN/Galgame runtime |

## Benchmarks

The canonical cross-engine comparison suite is:

```bash
cargo bench --bench fair
```

You can run a single engine slice with:

```bash
cargo bench --bench fair -- sky
cargo bench --bench fair -- hecs
cargo bench --bench fair -- bevy
```

Recorded local results and methodology notes live in
[benches/BENCHMARKS.md](benches/BENCHMARKS.md). Treat the numbers as
machine-specific history, not universal marketing claims.

## Project Status

SkyEngine is not a polished engine release yet. It is currently best suited for:

- learning how an ECS and renderer can be built in Rust,
- experimenting with data-oriented engine architecture,
- developing examples and vertical slices inside this repository,
- testing render, UI, asset, scene, tile, audio, video, and VN runtime ideas.

APIs may change as systems are split into cleaner crates and module boundaries.

## License

SkyEngine is licensed under the [MIT License](LICENSE).
