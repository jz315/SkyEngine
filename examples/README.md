# Examples Guide

SkyEngine keeps `examples/` in two layers:

- **Official examples** are the stable release surface registered in the root `Cargo.toml`.
- **Local showcase sources** stay in this directory for reference, experiments, and richer demos, but are not published as Cargo examples.

## Start Here

Follow this path when learning the engine from scratch:

```bash
cargo run --example hello_ecs
cargo run --example queries
cargo run --example commands
cargo run --example systems
cargo run --example tiny_defense
```

Then add subsystems in this order:

```bash
cargo run --example asset_cook_smoke --features asset
cargo run --example asset_load_texture --features asset
cargo run --example scene_basic --features scene
cargo run --example clear_screen --features app
cargo run --example sprite_demo --features app
cargo run --example textured_demo --features app
cargo run --example lighting_demo --features app
cargo run --example ui_legacy_hud_menu --features ui-legacy
```

The main mental model is:

- ECS examples teach `World`, typed queries, deferred commands, systems, and game-loop structure.
- Asset examples teach cooking, typed handles, dependencies, hot reload, and custom factories.
- Render examples move from the app frame lifecycle to the high-level scene pipeline and expert graph APIs.
- UI examples show the supported backend entry points without requiring showcase assets.

## Official Examples

These names are intentionally allowlisted in `Cargo.toml`. New official examples should be small, documented, CI-checkable, and runnable without large checked-in assets.

### ECS

```bash
cargo run --example hello_ecs
cargo run --example queries
cargo run --example commands
cargo run --example systems
cargo run --example tiny_defense
```

### Asset And Scene

```bash
cargo run --example asset_cook_smoke --features asset
cargo run --example asset_load_texture --features asset
cargo run --example asset_hot_reload_texture --features asset
cargo run --example asset_load_with_dependency --features asset
cargo run --example asset_custom_factory --features asset
cargo run --example scene_basic --features scene
```

### App And Render

```bash
cargo run --example clear_screen --features app
cargo run --example sprite_demo --features app
cargo run --example textured_demo --features app
cargo run --example lighting_demo --features app
cargo run --example render_graph_showcase --features app
cargo run --example frame_pipeline_showcase --features app
cargo run --example custom_feature_demo --features app
cargo run --example custom_material_demo --features app
cargo run --example tilemap_demo --features app
```

### Physics

```bash
cargo run --example physics_arcade_demo --features "app physics"
cargo run --example physics_headless_probe --features physics
```

### UI

```bash
cargo run --example ui_legacy_hud_menu --features ui-legacy
cargo run --example ui_serein_eui_gallery --features ui-serein
cargo run --example ui_serein_layout_primitives --features ui-serein
cargo run --example ui_serein_scroll_y --features ui-serein
cargo run --example ui_yakui_demo --features yakui-ui
```

### VN And Media

```bash
cargo run --example vn_runtime_minimal --features vn
cargo run --example vn_sprite_presentation --features "vn app"
cargo run --example audio_demo --features "app audio"
cargo run --example video_demo --features video
cargo run --example mp4_video_demo --features video-ffmpeg
```

## Local Showcase Sources

The repository still keeps larger or more experimental showcase sources in place under paths such as `examples/demo/`, `examples/game/`, `examples/live2d/`, `examples/ui/`, `examples/render/`, `examples/physics/`, and `examples/vn/`. They are useful as local reference material, but they are not part of the published Cargo example surface.

This includes full game slices, Live2D probes, Kajiya/Renderling backend experiments, Tiled browser/import/physics demos, renderer probes, performance labs, UI stress/mock apps, asset-skin showcases, and EduCanvas physics experiments.

Large or scratch assets live under `examples/assets/`. That directory is ignored by git and excluded from crates.io packages; only `examples/assets/README.md` is tracked. To run a local showcase that depends on those files, provide the assets locally and temporarily register the source as a Cargo example or run it from your own local harness.
