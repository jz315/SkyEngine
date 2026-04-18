# Examples Guide

SkyEngine's runnable examples are organized by learning path instead of a flat file list.

## Recommended Order

If you're new to the project, read and run examples in this order:

1. `hello_ecs` — smallest ECS starting point
2. `queries` — typed queries and iteration patterns
3. `commands` — deferred structural changes via `Commands`
4. `systems` — grouped scheduling and frame updates
5. `tiny_defense` — a complete ECS-only game loop
6. `clear_screen` → `sprite_demo` → `textured_demo` → `lighting_demo`
7. `render_graph_showcase` → `perf_test` → `renderer_probe`
8. `custom_feature_demo` — a public zero-engine-modification `RenderFeature` extension example
9. `custom_material_demo` — a public user-defined `Material` + `MeshRenderer` example
10. `boids` / `boids_classic` / `cosmic_jellyfish` / `neon_galaxy`

## Render Learning Path

The `examples/render/` directory now forms a complete path from first window to advanced rendering systems:

1. **`clear_screen`** — learn the minimal app + GPU frame loop and surface pass.
2. **`sprite_demo`** — drive the default unified scene pipeline from ECS through `App::with_render_pipeline(...)`.
3. **`textured_demo`** — keep building on the ECS-first textured sprite path.
4. **`lighting_demo`** — add high-level 2D lighting plus bloom, tone mapping, and vignette.
5. **`render_graph_showcase`** — study the low-level `render::expert::RenderGraph` API and resource scheduling model.
6. **`frame_pipeline_showcase`** — inspect the expert-only setup/view/finalize execution backbone directly.
7. **`perf_test`** — low-level `SpriteBatch` throughput / scaling observation.
8. **`renderer_probe`** — headless `RenderComposer` probe for the default universal scene pipeline.
9. **`custom_feature_demo`** — add a custom `RenderFeature` that injects its own post-fx step without changing engine code.
10. **`custom_material_demo`** — add a custom `Material`, upload a mesh, and render it through the public high-level pipeline.
11. **`live2d_demo`** — programmable multi-feature example (`SpriteFeature + Live2DFeature`) after you already know the base render stack.

Render mental model for the example set:

- `clear_screen` teaches the expert/no-pipeline `ctx.gpu()` path.
- `sprite_demo`, `textured_demo`, and `lighting_demo` teach the default high-level path: `App -> RenderPipelineAsset -> RenderComposer -> SpriteFeature`.
- If you need to tweak the high-level flow, change builder registrations and ordered steps such as `phase / compute / pass / postfx / feature`.
- If you need another renderer family, add another `RenderFeature`.
- `custom_feature_demo` is the first public example that shows a user-defined `RenderFeature` inserting its own post-fx step.
- `custom_material_demo` is the public end-to-end example for `impl Material`, `register_material::<M>()`, and runtime mesh/material setup through `FrameContext::with_renderer_mut(...)`.
- `render_graph_showcase` and `frame_pipeline_showcase` are expert-facing backend examples, not the default extension path.

Recommended progression:

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
  ↓
custom_feature_demo
  ↓
custom_material_demo

specialized branch: live2d_demo
```

## Directory Layout

### `examples/ecs/`

Beginner-friendly ECS tutorials. No GPU feature flags required.

```bash
cargo run --example hello_ecs
cargo run --example queries
cargo run --example commands
cargo run --example systems
cargo run --example tiny_defense
```

### `examples/render/`

Focused rendering API showcases built on the `app` feature.

```bash
cargo run --example clear_screen --features app
cargo run --example sprite_demo --features app
cargo run --example textured_demo --features app
cargo run --example lighting_demo --features app
cargo run --example render_graph_showcase --features app
cargo run --example frame_pipeline_showcase --features app
cargo run --example perf_test --features app --release
cargo run --example renderer_probe --features app --release
cargo run --example custom_feature_demo --features app --release
cargo run --example custom_material_demo --features app --release
```

Suggested study order inside `render/`:

- `clear_screen` — frame lifecycle and surface pass
- `sprite_demo` — ECS-first `RenderPipelineAsset` + `RenderComposer`
- `textured_demo` — ECS-driven textured sprites
- `lighting_demo` — high-level lighting + post-processing
- `render_graph_showcase` — expert-only graph compilation model
- `frame_pipeline_showcase` — expert-only setup/view/finalize backbone
- `perf_test` — low-level throughput / scaling observation
- `renderer_probe` — scene-pipeline workload matrix for the default universal path
- `custom_feature_demo` — public `RenderFeature` extension with a custom post-fx
- `custom_material_demo` — public custom `Material` + uploaded mesh path

`live2d_demo` is the first multi-feature branch after the main path. It routes sprites and Live2D through the same transparent scene phase:

```bash
cargo run --example live2d_demo --features "live2d egui" --release -- <path-to-model3.json>
cargo run --example live2d_demo --features "live2d egui" --release -- --no-ui <path-to-model3.json>
```

### `examples/demo/`

Full showcase demos that combine ECS, rendering, lighting, and post-processing.

```bash
cargo run --example boids --features app --release
cargo run --example boids_classic --features app --release
cargo run --example cosmic_jellyfish --features app --release
cargo run --example neon_galaxy --features app --release
```

### `examples/compare/`

Cross-engine comparison examples. These are not part of the recommended learning path.

```bash
cargo run --example boids_hecs --features compare --release
cargo run --example boids_bevy --features compare --release
cargo run --example boids_bevy_gpu --features compare-bevy --release
```

### `examples/legacy/`

Historical CPU-rendered demos kept for reference.

```bash
cargo run --example particles --features demo-legacy
cargo run --example asteroids --features demo-legacy
cargo run --example snake --features demo-legacy
```

## Notes

- The source of truth for runnable examples is `Cargo.toml`.
- `examples/ecs` is the best entry point for API learning.
- `examples/demo` is optimized for showcasing the engine, not for teaching individual APIs step by step.
- `examples/compare` is for cross-engine reference material; `examples/legacy` is for older SkyEngine-only CPU demos.
