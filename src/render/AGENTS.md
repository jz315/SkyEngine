# AGENTS.md - `src/render`

## Architecture

```text
render/
├─ mod.rs                       # curated gameplay facade
├─ expert/{graph,execution,gpu,draw,resources}.rs
├─ core/{graph,gpu,execution,pipeline,runtime,extraction,draw,view,scene,resources}/
├─ features/{sprite,mesh,lighting,gi,postfx,tilemap,live2d}/
├─ integration/{backend,assets,presets}/
└─ shaders/
```

Dependency direction is fixed:

```text
facade → integration → features → core
```

- `core/` must not import a concrete feature, backend, tilemap, GI, or Live2D module.
- Cross-family contracts belong in `core` as neutral payload/resource types; features fill them through the crate-private `FrameExtension` adapter.
- `integration/` owns app backends, backend-neutral `SceneSnapshot`, CPU render assets, and preset assembly. It may compose features but must not become a second runtime.
- `features/` own ECS components, extraction/preparation, passes, GPU caches, and tests for their renderer family.

## Public API

- `sky_engine::render` is the high-level facade: runtime, pipeline asset/builder, scene renderer, common scene components, materials, assets, statistics, and common features.
- Feature-specific configuration belongs below `sky_engine::render::features::{gi,tilemap,live2d,postfx,...}`.
- Low-level APIs are grouped, never flat:
  - `render::expert::graph`
  - `render::expert::execution`
  - `render::expert::gpu`
  - `render::expert::draw`
  - `render::expert::resources`
- Do not restore `render::graph`, `render::gpu`, `render::component`, `render::backend`, or flat `render::expert::*` compatibility paths.

## Runtime Rules

- `RenderRuntime` coordinates generic frame stages only. It does not call concrete GI or shadow functions.
- Family-owned frame work is installed through `FrameExtension`: initialization, view collection, scene upload, preparation, payload injection, stats, and invalidation.
- Extension preparation failures skip the frame through the normal outcome path and invalidate temporal/history state and feature caches.
- Preserve `PreparedFrame` / `PreparedView`, `RenderFeature`, phase, pass, and extractor extension points.
- Do not add per-frame heap allocation or GPU resource recreation to graph compilation, extraction, or feature preparation paths.

## Domain Notes

- `core/graph/`: read `core/graph/AGENTS.md`; handle tokens validate every graph resource, `compile()` is execution-order truth, and copy passes flush the active encoder.
- `features/lighting/renderer/shadow/`: `sync/` owns view synchronization/cascade math, `phase/{pipelines,opaque,transparent}` owns shadow execution, and `atlas`/`resources` own focused data contracts.
- `features/gi/providers/ddgi/`: keep provider, resources, BVH, preparation, execution, shader, and tests separated.
- `features/tilemap/tiled/`: `mod.rs` is only the facade; `import.rs` loads files/assets and `parse.rs` decodes map formats. Never create a same-name `tiled.rs` alongside this directory.
- `features/live2d/`: preserve the existing Cubism behavior and keep Live2D-only state local. See its local AGENTS.md.
- `src/tile/` remains the high-level editable tile-scene model; render tilemap is the low-level render path only.

## Validation

- `cargo test --features app render:: --no-fail-fast`
- `cargo test --features app --no-fail-fast`
- `cargo check --examples --features app`
- `cargo check --manifest-path tools/showcase-check/Cargo.toml --all-targets --all-features`
- `cargo clippy --features app --all-targets -- -D warnings`
- `cargo fmt --check`
- `git diff --check`
