# AGENTS.md - `src/render`

## Overview
- This module is SkyEngine's rendering framework behind `features = ["app"]`.
- The default native renderer is the `wgpu` path: `RenderPipelineAsset` + `RenderPipelineBuilder` declare a pipeline, and `RenderRuntime` owns the live per-frame runtime.
- The app runner does not talk to `RenderRuntime` directly. It owns a backend-neutral `SceneRenderer`; `WgpuSceneRenderer` wraps `GpuContext` + optional `RenderRuntime`.
- Optional Kajiya and Renderling renderers are selected by `RenderPipelineAsset::backend_kind()` and consume a backend-neutral `SceneSnapshot` instead of the `RenderRuntime` frame pipeline.
- The registration-driven `wgpu` runtime uses:
  - `RenderFeature` for renderer families such as sprites, tilemaps, and Live2D.
  - `Extractor`s to pull ECS state into phase data.
  - `DrawFunction`s to execute sorted `PhaseItem`s.
  - `PipelineStep::{Phase, Compute, Graph, Pass, PostFx}` for execution order.
  - `GpuScene` + `GpuTableManager` for shared wgpu-side tables such as model matrices and lights.
- The composition boundary for heterogeneous wgpu renderer families is `PreparedFrame` / `PreparedView` + `FramePipeline`.
- `2D` is a usage pattern, not the architectural root. Sprite, tilemap, mesh/material, lighting, shadows, GI, post-fx, and Live2D all plug into the render stack through explicit registration points.
- Shader language is WGSL. Shaders live under `shaders/`.
- Live2D integration additionally requires `features = ["live2d"]`.

## Module Architecture

```text
render/
├── mod.rs              - Curated public facade and re-exports
├── expert.rs           - Expert-facing low-level facade
├── animation/          - Sprite animation clips/systems
├── asset/              - Backend-neutral CPU-side render asset data
├── backend/            - App-facing scene renderer backend selection and bridges
├── builtins/           - Built-in high-level pipeline passes/steps
├── component/          - ECS-facing render components and settings
├── view/               - Camera/view/projection/frustum/transform types
├── gpu/                - Shared wgpu resources, targets, textures, tables
├── gi/                 - Provider-driven global illumination runtime
├── lighting/           - Light data, light table, light pass, shadows
├── sprite/             - Sprite public API and sprite batch renderer
├── tilemap/            - Chunked tilemap renderer and Tiled import bridge
├── mesh/               - Mesh pass implementation
├── composite/          - Scene/light composition pass
├── runtime/            - High-level wgpu orchestration (`RenderRuntime`)
├── extract/            - Registered ECS extraction path
├── phase/              - `PhaseItem`, sorting, draw dispatch
├── pipeline/           - Builder/asset/feature/step definitions
├── execution/          - Generic prepared-frame execution backbone
├── graph/              - Declarative render graph system
├── resources/          - Texture/material/mesh/atlas/blackboard resources
├── postfx/             - Reusable lower-level post-processing effects
├── live2d/             - Cubism runtime/renderer and feature bridge
└── shaders/            - WGSL sources
```

## Sub-Module AGENTS.md References
- Backend selection and backend-neutral scene snapshots: [`backend/AGENTS.md`](backend/AGENTS.md)
- Render graph internals: [`graph/AGENTS.md`](graph/AGENTS.md)
- Low-level Live2D runtime and renderer: [`live2d/AGENTS.md`](live2d/AGENTS.md)
- Tilemap and Tiled import/rendering: [`tilemap/AGENTS.md`](tilemap/AGENTS.md)
- Global illumination provider contract: [`gi/AGENTS.md`](gi/AGENTS.md)

## Canonical High-Level Surface
- Start normal app code from `sky_engine::render::*`.
- Preferred high-level names are:
  - `SceneRenderer`, `WgpuSceneRenderer`, `SceneRendererError`, `SceneRendererInitError`
  - `RenderRuntime`
  - `RenderPipelineAsset`
  - `RenderPipelineBuilder`
  - `RenderBackendKind`
  - `RenderFeature`
  - `SpriteFeature`
  - `TilemapFeature`
  - `OpaquePhase`, `TransparentPhase`, `DirectionalShadowPhase`
  - `Bloom`, `ToneMap`, `Vignette`, `TemporalAntiAliasing`, `Sharpen`, `DebugView`
  - `Camera`, `CameraMarker`, `Color`, `ViewportRect`, `SceneView`
  - `Texture`, `TextureReadiness`, `SharedRenderAssetCache`
  - `GpuScene`, `GpuTable`, `GpuTableManager`
  - `Material`, `MaterialHandle`, `MaterialRegistry`, `SpriteMaterial`, `UnlitMaterial`, `StandardMaterial`
  - `MeshAsset`, `MeshRenderer`, `WgpuMeshRenderer`
  - `TilemapStorage`, `TilemapRenderer`, `TiledImport`, `TiledMapInstance`
- Expert entry points live under `sky_engine::render::expert::*`.

## Backend Boundary
- `backend/` owns app-facing renderer selection through `create_scene_renderer(...)`.
- `SceneRenderer` is the app-runner contract: frame begin/end, render world, resize, stats, backend name, and optional wgpu escape hatches.
- `WgpuSceneRenderer` owns `GpuContext`, optional `RenderRuntime`, and `WgpuRenderAssetCache`.
- Kajiya and Renderling consume `SceneSnapshot` built by `SceneSnapshotExtractor`; they should not grow ad-hoc ECS query paths.
- `SceneSnapshot` is for backend-neutral 3D renderer backends. It is not the generic frame schema for the wgpu `RenderRuntime`.

## File Map

### `mod.rs`
- Curated public re-exports.
- Exposes render assets, components, backend types, pipeline declarations, runtime access, built-ins, tilemap, material, GI, and expert namespace.

### `animation/`
- Owns sprite animation clips, frames, animator components/resources, and `animate_sprites`.

### `component/`
- Owns ECS-facing render components and renderer settings.
- Important files include camera markers/viewports, sprite/mesh/tilemap renderers, light components, hierarchy parent marker, Live2D components, and render settings.
- Keep ECS-facing authoring data here; do not push it down into runtime/execution modules.

### `asset/` and `backend/`
- `asset/` owns backend-neutral CPU-side render asset data: `MeshAsset`, mesh vertex layout descriptors, material asset descriptors, texture sampler descriptors, and `RenderAssets`.
- `resources/texture_cache.rs` owns the shared runtime GPU texture cache for `TextureAsset` handles.
- `backend/` owns renderer selection and backend-specific bridges. The wgpu bridge adapts `AssetServer` render assets into `RenderRuntime`; Kajiya and Renderling keep their own sync code local to backend modules.
- Keep asset authoring, runtime cache lifetime, and backend upload bridges separate. They change for different reasons.

### `view/`
- Owns camera/view/projection semantics and scene-view construction.
- Shared view/camera/transform concepts belong here, not in `execution/` or `runtime/`.

### `gpu/`
- Owns the shared wgpu resource layer.
- `GpuScene` is the shared scene upload state used by the high-level wgpu runtime.
- `RenderTarget`, `Texture`, fullscreen helpers, model matrix tables, bind-group helpers, and readback helpers live here.
- Extend shared GPU data by registering tables, not by hard-coding one-off fields into unrelated runtime objects.

### `lighting/`
- Owns light-domain code and directional shadow support.
- Keep light/shadow concepts together here. Do not split them back across unrelated folders.

### `gi/`
- Owns provider-driven global illumination runtime and provider registry.
- Provider-specific data, shaders, GPU bindings, update/composite descriptors, and sampling payloads stay in provider modules.
- See [`gi/AGENTS.md`](gi/AGENTS.md).

### `sprite/`, `tilemap/`, `mesh/`, and `composite/`
- `sprite/` owns sprite-specific rendering and `SpriteBatch`.
- `tilemap/` owns tilemap storage/import/extraction/drawing and `TilemapFeature`.
- `mesh/` owns `MeshPass`, mesh draw preparation/recording, and mesh-pass errors/tests.
- `composite/` owns `CompositePass`.
- If a helper is only used by one renderer family, keep it local to that family rather than promoting it too early.

### `runtime/`
- Owns the high-level wgpu runtime layer.
- Important files:
  - `runtime.rs` - app-facing `RenderRuntime`, pipeline descriptor materialization, runtime resource accessors.
  - `state.rs` - runtime plan/resource/state buckets.
  - `frame_coordinator.rs` - short frame recipe that sequences explicit frame stages.
  - `frame/` - input collection, resource preparation, extraction, scene upload, lighting, GI, shadow preparation, frame assembly, execution, and stats/finalization helpers.
  - `executor.rs` - executes assembled frames through `FramePipeline`.
  - `pipeline_runtime.rs` - translates declaration-time `PipelineStep`s into execution step nodes.
  - `view_collection.rs` - world camera/view collection.
  - `history.rs` / `temporal.rs` - history texture and temporal-view state.
  - `presentation.rs` - `ViewportBlitNode`.
  - `stats.rs` - `RenderTimingStats` and timing helpers.
- `RenderRuntime` should stay an orchestrator. Put new heavy logic in adjacent helpers rather than turning it into a god object.

### `pipeline/`
- Owns declaration-time configuration, not per-frame execution state.
- Important files:
  - `pipeline_asset.rs` - pipeline presets and descriptor materialization.
  - `builder.rs` - `RenderPipelineBuilder` and registration methods.
  - `backend_kind.rs` - `RenderBackendKind` and backend-specific settings.
  - `features.rs` - `RenderFeature`, runtime feature hooks, built-in `SpriteFeature`, and optional `Live2DFeature`.
  - `step.rs` / `passes.rs` / `phases.rs` / `resource_spec.rs` - pipeline step, pass, phase, and resource declarations.
  - `material_registration.rs` - material registration records used by the runtime plan.
- Builder execution order is explicit and linear:
  - `add_phase(...)`
  - `add_compute(...)`
  - `add_graph_pass(...)`
  - `add_pass(...)`
  - `add_postfx(...)`
  - `add_feature(...)`
  - `add_extractor(...)`
  - `add_draw_function(...)`
  - `add_gpu_table(...)`
  - `register_material::<M>()`

### `extract/`
- Owns ECS extraction into per-view phase data.
- Built-in extractors populate sprite and mesh draws into `OpaquePhase` / `TransparentPhase`.
- New wgpu renderer families should normally enter through `RenderFeature` hooks and/or registered extractors rather than ad-hoc branches in `RenderRuntime`.

### `phase/`
- Owns `PhaseItem`, sorting, and draw dispatch.
- Built-in flow is centered on opaque and transparent phase execution plus `DrawFunctionRegistry`.
- Keep phase payloads generic and sortable; do not sneak renderer-specific global state into shared execution contexts.

### `execution/`
- Generic prepared-frame execution backbone.
- Key concepts:
  - `PreparedFrame`
  - `PreparedView`
  - `FramePipeline`
  - setup/view/finalize nodes
  - typed frame/view payload stores
  - `PhaseState` / `CompletedViewState` / `FinalizePhaseState`
  - typed scene inputs via `SceneGBufferSlots`
- Scene-input allocation/reuse rules should live in execution helpers so passes reuse one policy for attaching scene depth/normal/velocity.
- This is the main composition boundary for mixing sprite, mesh, tilemap, shadows, Live2D, GI, and future wgpu renderer families.

### `graph/`
- Owns the low-level declarative render graph backend.
- Read [`graph/AGENTS.md`](graph/AGENTS.md) before changing internals there.

### `builtins/`, `postfx/`, and `resources/`
- `builtins/` owns high-level built-in pipeline pass wrappers and phases.
- `postfx/` owns reusable effect implementations behind built-in post-fx markers.
- `resources/` owns shared material, mesh, texture-cache, atlas, and blackboard systems.
- Material internals keep `MaterialRegistry` as the facade. Mesh internals keep `MeshRegistry` as the runtime facade.
- Material pipelines resolve vertex inputs by semantic against the actual mesh layout, so meshes may contain extra attributes if the material-required ones are present with compatible formats.
- `StandardMaterial` without a normal map requires `Position + Normal + UV0`.
- `StandardMaterial` with a normal map requires `Position + Normal + Tangent + UV0`.

### `live2d/`
- Owns both the low-level Cubism runtime/renderer and the high-level Live2D feature integration.
- See [`live2d/AGENTS.md`](live2d/AGENTS.md).

## Wgpu Runtime Flow
1. `WgpuSceneRenderer::render_world()` calls `RenderRuntime::render_world(gpu, world)`.
2. `RenderRuntime` collects frame inputs, render settings, assets, transforms, and `SceneView`s.
3. Registered features run `extract(...)` and may extend the view list through `collect_views(...)`.
4. Registered extractors populate per-view `OpaquePhase` and `TransparentPhase`.
5. Registered features run `prepare(...)` and may append additional phase items.
6. Shared GPU tables are updated and uploaded through `GpuScene`.
7. GI and shadow runtime resources are prepared for material shaders and shadow phases.
8. `RenderRuntime` builds a `PreparedFrame` and one `PreparedView` per visible view, then lets features inject typed frame/view payloads.
9. `pipeline_runtime.rs` converts `PipelineStep`s into `FramePipeline` nodes.
10. `FramePipeline` executes phases, compute steps, graph passes, custom passes, post-fx, and final viewport presentation.

## Composition Boundary
- `RenderPipelineAsset` is the declarative configuration.
- `RenderRuntime` owns live wgpu runtime state for that asset.
- `PreparedFrame` / `PreparedView` carry typed data between preparation and execution.
- `FramePipeline` is the wgpu execution engine.
- `SceneSnapshot` is a separate backend-neutral 3D payload for Kajiya/Renderling-style backends.
- New wgpu renderer integrations should normally follow this shape:
  - feature registration
  - extractor and/or per-view preparation
  - phase items or explicit pipeline steps
  - typed payload access through prepared frame/view stores

## Implementation Guidelines
- Do not reintroduce the old domain-centric render API model or app-side domain access patterns.
- Keep new high-level wgpu work registration-driven: builder registrations, extractors, draw functions, phases, GPU tables, and explicit pipeline steps.
- Keep ECS authoring in `component/`, view/camera semantics in `view/`, shared wgpu resources in `gpu/`, and light/shadow logic in `lighting/`.
- `GpuScene` is shared wgpu scene upload state, not a universal home for every renderer-specific cache.
- Only promote concepts into shared frame state when they are truly cross-renderer.
- When a pass needs canonical scene inputs, use the typed scene-input accessors on execution state. `SceneGBufferSlots` is the source of truth; the generic slot map is for ad-hoc resources and does not mirror `scene_*` attachments.
- Keep Live2D-specific semantics local to `live2d/`.
- Keep tilemap-specific batching, Tiled import rules, and layer ordering local to `tilemap/`.
- If you change public render or app-facing APIs, validate examples with `cargo check --examples --features app`.

## Test Commands
- Run render tests: `cargo test --features app`
- Run render graph tests: `cargo test --features app graph`
- Run runtime tests: `cargo test --features app render::runtime::tests`
- Run extract tests: `cargo test --features app render::extract`
- Run backend tests: `cargo test --features app render::backend`
- Run tilemap tests: `cargo test --features app render::tilemap`
- Run GI-focused tests: `cargo test --features app gi`
- Run example compile check after high-level renderer changes: `cargo check --examples --features app`

## Relation to Other Modules
- GPU: `src/gpu/context.rs` provides `GpuContext`.
- ECS: high-level integration happens through extractors operating on `World`.
- App: `src/app/runner.rs` owns the active `SceneRenderer`; `FrameContext` exposes render/runtime/gpu accessors where available.
- Asset: `src/asset` owns CPU asset lifetimes; `SharedRenderAssetCache` and backend bridges handle render-side readiness/upload.
- UI: current game UI overlays live in `src/ui` / `src/app/egui_integration.rs` and render after scene output; they are not currently a `RenderFeature` or render phase.
