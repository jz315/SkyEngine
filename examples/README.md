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
8. `boids` / `boids_classic` / `cosmic_jellyfish` / `neon_galaxy`

## Render Learning Path

The `examples/render/` directory now forms a complete path from first window to advanced rendering systems:

1. **`clear_screen`** — learn the minimal app + GPU frame loop and surface pass.
2. **`sprite_demo`** — drive `Renderer2D` directly from ECS with `Transform2D + Sprite2D + Camera2D`.
3. **`textured_demo`** — keep building on the ECS-first textured sprite path.
4. **`lighting_demo`** — add high-level 2D lighting plus bloom, tone mapping, and vignette.
5. **`render_graph_showcase`** — study the low-level `render::expert::RenderGraph` API and resource scheduling model.
6. **`perf_test`** — low-level `SpriteBatch` throughput / scaling observation.
7. **`renderer_probe`** — headless high-level `Renderer2D` timing probe across dirty workloads.
8. **`live2d_demo`** — specialized integration example after you already know the base render stack.

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
perf_test
  ↓
renderer_probe

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
cargo run --example perf_test --features app --release
cargo run --example renderer_probe --features app --release
```

Suggested study order inside `render/`:

- `clear_screen` — frame lifecycle and surface pass
- `sprite_demo` — ECS-first `Renderer2D`
- `textured_demo` — ECS-driven textured sprites
- `lighting_demo` — high-level lighting + post-processing
- `render_graph_showcase` — expert-only graph compilation model
- `perf_test` — low-level throughput / scaling observation
- `renderer_probe` — high-level `Renderer2D` timings and workload matrix

`live2d_demo` is a specialized branch after the main path, and requires `live2d` instead of plain `app`:

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
