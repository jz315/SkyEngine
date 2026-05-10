# AGENTS.md — `src/render`

## Overview
- This module is SkyEngine's `wgpu`-based rendering framework.
- The public high-level surface is centered on `RenderPipelineAsset` + `RenderPipelineBuilder` + `RenderRuntime`.
- The runtime is registration-driven:
  - `RenderFeature` installs renderer families into the builder/runtime.
  - `Extractor`s pull ECS state into per-view phase data.
  - `DrawFunction`s execute sorted `PhaseItem`s.
  - `PipelineStep::{Phase, Compute, Pass, PostFx}` defines execution order.
  - `GpuScene` + `GpuTableManager` own shared GPU-side tables such as model matrices and lights.
- The composition boundary for heterogeneous renderers is `PreparedFrame` / `PreparedView` + `FramePipeline`.
- `2D` is a usage pattern, not the architectural root. Sprite, mesh, lighting, and Live2D all plug into the same frame pipeline.
- The GPU backend is `wgpu`. All rendering goes through `GpuContext` in `src/gpu/context.rs`.
- Shader language is WGSL. Shaders live under `shaders/`.
- The module is compiled behind `features = ["app"]`. Live2D integration additionally requires `features = ["live2d"]`.

## Module Architecture

```text
render/
├── mod.rs              — Curated public facade and re-exports
├── expert.rs           — Expert-facing low-level facade
├── asset/              — Backend-neutral CPU-side render asset data
├── backend/            — Scene renderer backend selection and backend bridges
├── component/          — ECS-facing render components and settings
├── view/               — Camera/view/projection/frustum/transform types
├── gpu/                — Shared GPU resources, targets, textures, tables
├── lighting/           — Light data, light table, light pass, shadows
├── sprite/             — Sprite public API and sprite batch renderer
├── tilemap/            — Chunked tilemap renderer and Tiled import bridge
├── mesh/               — Mesh pass implementation
├── composite/          — Scene/light composition pass
├── runtime/            — High-level orchestration (`RenderRuntime`)
├── extract/            — Registered ECS extraction path
├── phase/              — `PhaseItem`, sorting, draw dispatch
├── pipeline/           — Builder/asset/feature/step definitions
├── builtins/           — High-level built-in pass implementations
├── execution/          — Generic prepared-frame execution backbone
├── graph/              — Declarative render graph system
├── resources/          — Runtime registries, texture cache, material/mesh/atlas/blackboard resources
├── postfx/             — Reusable effect implementations
├── live2d/             — Low-level Cubism runtime/renderer and feature bridge
└── shaders/            — WGSL sources
```

## Sub-Module AGENTS.md References
- Render graph internals: [`graph/AGENTS.md`](graph/AGENTS.md)
- Low-level Live2D runtime and renderer: [`live2d/AGENTS.md`](live2d/AGENTS.md)
- Tilemap and Tiled import/rendering: [`tilemap/AGENTS.md`](tilemap/AGENTS.md)

## Canonical High-Level Surface
- Start normal app code from `sky_engine::render::*`.
- Preferred high-level names are:
  - `RenderRuntime`
  - `RenderPipelineAsset`
  - `RenderPipelineBuilder`
  - `RenderFeature`
  - `SpriteFeature`
  - `DirectionalShadowPhase`
  - `Bloom`, `ToneMap`, `Vignette`
  - `Camera`, `Color`, `ViewportRect`, `SceneView`
  - `Texture`
  - `GpuScene`, `GpuTable`, `GpuTableManager`
  - `Material`, `MaterialHandle`, `MaterialRegistry`
- Expert entry points live under `sky_engine::render::expert::*`.

## File Map

### `mod.rs`
- Curated public re-exports.
- Exposes the registration-driven render API and expert namespace.

### `component/`
- Owns ECS-facing render components and renderer settings.
- Important files:
  - `camera.rs` — ECS camera marker, `CameraViewport`, `MainCamera`
  - `sprite.rs` — `SpriteRenderer`, sorting components
  - `mesh.rs` — `MeshRenderer`
  - `light.rs` — `PointLight`, `DirectionalLight`
  - `settings.rs` — `RenderSettings`, bloom/tonemap/vignette settings
  - `hierarchy.rs` — `Parent`
- Keep ECS-facing authoring data here; do not push it down into runtime/execution modules.

### `asset/` and `backend/`
- `asset/` owns backend-neutral CPU-side render asset data: `MeshAsset`, mesh vertex layout descriptors, material asset descriptors, and the `RenderAssets` authoring collection.
- `resources/texture_cache.rs` owns the runtime GPU texture cache for `TextureAsset` handles. Do not put semantic asset definitions there.
- `backend/` owns renderer selection and backend-specific bridges. The `wgpu_asset_bridge.rs` file adapts CPU render assets into the `wgpu` runtime cache; Kajiya and Renderling keep their own backend sync code local to their backend folders.
- Keep asset authoring, runtime cache lifetime, and backend upload bridges separate. They change for different reasons.

### `view/`
- Owns camera/view/projection semantics and scene-view construction.
- Important files:
  - `camera.rs` — public `Camera`, `RenderView`, `ViewUniform`
  - `scene_view.rs` — `SceneView`, `SceneViewKind`, fallback/build helpers
  - `viewport.rs` — `ViewportRect`
  - `projection.rs` — projection-to-view-uniform construction
  - `transform.rs` — `SceneTransformResolver`, `ResolvedSceneTransforms`
  - `types.rs` — `RenderStats`, `RenderQueueSort`, `SCENE_HDR_FORMAT`
- Shared view/camera/transform concepts belong here, not in `execution/` or `runtime/`.

### `gpu/`
- Owns the shared GPU resource layer.
- Important files:
  - `scene.rs` — `GpuScene`, the shared scene upload state used by the high-level runtime
  - `mod.rs` — `GpuTable`, `GpuTableManager`, re-exports for texture/target/fullscreen helpers
  - `model_matrix.rs` — `ModelMatrixTable`
  - `target.rs` — `RenderTarget`, `RenderTargetDescriptor`, depth-format helpers
  - `texture.rs` — `Texture` and creation/upload helpers
  - `fullscreen.rs` — `FullscreenPass`, `FullscreenPipeline`
  - `helpers.rs` — bind-group/pipeline caches and quad geometry helpers shared by sprite/light/composite code
- Extend shared GPU data by registering tables, not by hard-coding more one-off fields into unrelated runtime objects.

### `lighting/`
- Owns light-domain code and directional shadow support.
- Important files:
  - `data.rs` — `Light2D`, `color_temperature`
  - `gpu_table.rs` — `GpuLight`, `LightTable`
  - `pass.rs` — `LightPass`
  - `shadow/` — shadow bind groups, shadow view payloads, `DirectionalShadowPhase`
- Keep light/shadow concepts together here. Do not split them back across unrelated folders.

### `sprite/`, `mesh/`, and `composite/`
- `sprite/` owns sprite-specific rendering. `batch/mod.rs` implements `SpriteBatch` and `Sprite`.
- `mesh/` owns `MeshPass`, mesh draw preparation/recording, and mesh-pass errors/tests.
- `composite/` owns `CompositePass`.
- If a helper is only used by one renderer family, keep it local to that family rather than promoting it to a generic module too early.

### `runtime/`
- Owns the high-level runtime layer.
- Important files:
  - `runtime.rs` — app-facing `RenderRuntime`, pipeline descriptor materialization, and public resource accessors
  - `frame_coordinator.rs` — short frame recipe that sequences explicit frame stages
  - `frame/` — explicit frame-stage records plus input collection, resource preparation, extraction, scene upload, lighting, GI, shadow preparation/summary, frame assembly, execution bridge, and stats/finalization helpers
  - `executor.rs` — executes assembled frames through `FramePipeline` and runtime render services
  - `pipeline_runtime.rs` — translates `PipelineStep`s into `FramePipeline` nodes
  - `view_collection.rs` — world camera/view collection
  - `pipeline_runtime.rs` — adapts declaration-time `PipelineStep`s into execution step nodes
  - `presentation.rs` — `ViewportBlitNode`
  - `stats.rs` — `RenderTimingStats` and timing helpers
- `RenderRuntime` should stay an orchestrator. Put new heavy logic in adjacent helpers rather than turning it into a god object.

### `pipeline/`
- Owns declaration-time configuration, not per-frame execution state.
- Important files:
  - `pipeline_asset.rs` — `RenderPipelineAsset` presets and descriptor materialization
  - `builder.rs` — `RenderPipelineBuilder` and registration methods
  - `step.rs` — `PipelineStep` and `PipelineStepDescriptor`
  - `backend_kind.rs` — `RenderBackendKind` and backend-specific settings
  - `material_registration.rs` — material registration records used by the runtime plan
  - `features.rs` — `RenderFeature`, runtime feature hooks, built-in `SpriteFeature`, and optional `Live2DFeature`
  - `phases.rs` / `passes.rs` — `RenderPhase`, `RenderPass`, `ComputePass`, and `PostFxPass`
- Built-in passes live under `builtins/`; `pipeline/` should depend on them as registrations, not own their implementations.
- Builder execution order is explicit and linear:
  - `add_phase(...)`
  - `add_compute(...)`
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
- New renderer families should normally enter the high-level runtime here or through `RenderFeature` hooks rather than via ad-hoc branches in `RenderRuntime`.

### `phase/`
- Owns `PhaseItem`, sorting, and draw dispatch.
- Built-in flow is centered on opaque and transparent phase execution plus `DrawFunctionRegistry`.
- `mesh_draw.rs` owns the generic material-backed mesh draw function, including scene material prepass batching.
- `sprite_draw.rs` owns the built-in sprite phase draw function and its pipeline cache.
- `scene_bindings.rs` owns shared model/view binding helpers used by phase draw paths and standalone tests.
- `scene_prepass.rs` owns scene material prepass context and pipeline cache.
- Keep phase payloads generic and sortable; do not sneak renderer-specific global state into shared execution contexts.

### `execution/`
- Generic prepared-frame execution backbone.
- Key concepts:
  - `PreparedFrame`
  - `PreparedView`
  - `FramePipeline`
  - setup/view/finalize nodes
  - `contexts/` — setup/execute context types split by phase, compute, graph, render pass, post-fx, and shared scene texture helpers
  - `step_nodes/` — runtime-installed step nodes split by execution mode: scene seed/keepalive, phase, compute, graph, post-fx, and finalize pass
  - typed frame/view payload stores
  - `PhaseState` / `CompletedViewState` / `FinalizePhaseState`
  - typed scene inputs via `SceneGBufferSlots` (`scene_color`, `scene_depth`, `scene_normal`, `scene_velocity`, and future material buffers)
- Scene-input allocation/reuse rules should live in execution helpers so passes reuse one policy for attaching scene depth/normal/velocity instead of open-coding texture creation.
- This is the main composition boundary for mixing sprite, mesh, shadows, Live2D, and future renderers.

### `graph/`
- Owns the low-level declarative render graph backend.
- Read [`graph/AGENTS.md`](graph/AGENTS.md) before changing internals there.

### `postfx/` and `resources/`
- `builtins/` owns high-level built-in pipeline pass wrappers and phases.
- `builtins/debug.rs` owns the debug-view pass and render-debug source selection.
- `builtins/gi.rs` owns the generic GI update/composite pipeline steps.
- `builtins/shadows.rs` owns shadow-flavored built-in post effects such as contact shadows.
- `postfx/` owns reusable effect implementations behind the built-in post-fx markers.
- `builtins/prepass.rs` owns the high-level scene normal/material prepass pipeline phases and their gbuffer setup policy.
- `builtins/postfx.rs` owns the high-level image post-fx wrappers (`Bloom`, `ToneMap`, `TemporalAntiAliasing`, `Sharpen`, and `Vignette`) that adapt `RenderSettings` and graph context to the lower-level reusable post-fx implementations.
- `resources/` owns shared material, mesh, texture-cache, atlas, and blackboard systems.
- Material internals keep `MaterialRegistry` as the facade. `records.rs` owns model records and erased callbacks, `instance_store.rs` owns generational instance slots, `dirty_queue.rs` owns prepare queue de-duplication, and `debug.rs` owns material introspection DTOs/summary construction.
- `resources/mesh/` owns persistent GPU mesh resources. `gpu_mesh.rs` uploads tangent-capable glTF meshes for `StandardMaterial` normal mapping, `registry.rs` owns `MeshHandle` / `MeshRegistry`, `vertex_layout.rs` owns semantic vertex layouts, and `shape.rs` owns mesh bounds/sub-mesh metadata.
- `resources/mesh/ray_geometry.rs` extracts CPU ray geometry for meshes with a Float32x3 `Position` attribute; this feeds provider-driven GI tracing while non-position meshes remain renderable but not GI-traceable.
- Material pipelines resolve vertex inputs by semantic against the actual mesh layout, so meshes may contain extra attributes if the material-required ones are present with compatible formats.
- `StandardMaterial` without a normal map requires `Position + Normal + UV0`.
- `StandardMaterial` with a normal map requires `Position + Normal + Tangent + UV0`.
- The built-in 3D path now runs directional shadows and generic `GiUpdateCompute` ahead of opaque shading; `StandardMaterial` samples indirect diffuse through an independent GI sampling bind group selected by the active provider.
- Custom mesh materials can opt into `SceneMaterialPrepass` by implementing the `Material::scene_prepass_*` hooks; once they do, they participate in scene gbuffer generation without extra engine-side registration.

### `live2d/`
- Owns both the low-level Cubism runtime/renderer and the high-level Live2D feature integration.
- Important files:
  - `feature.rs` — `Live2DFeature`, ECS extraction, per-view preparation, transparent-phase item submission
  - `draw.rs` — `DrawLive2D`, the standalone transparent-phase draw function
  - `backend.rs` — feature-owned backend/runtime bridge
  - `render/` — low-level prepared-frame and renderer implementation

## Runtime Flow
1. `RenderRuntime::render_world()` resolves transforms and collects `SceneView`s.
2. Registered features run `extract(...)` and may extend the view list through `collect_views(...)`.
3. Registered `Extractor`s populate per-view `OpaquePhase` and `TransparentPhase`.
4. Registered features run `prepare(...)` and may append additional phase items.
5. Shared GPU tables are updated and uploaded through `GpuScene`.
6. `RenderRuntime` prepares generic GI provider resources and per-view directional-shadow payloads used by `StandardMaterial` and `DirectionalShadowPhase`.
7. `RenderRuntime` builds a `PreparedFrame` and one `PreparedView` per visible view, then lets features inject typed frame/view payloads.
8. `pipeline_runtime.rs` converts `PipelineStep`s into `FramePipeline` nodes.
9. `FramePipeline` executes phases, compute steps, custom passes, post-fx, and final viewport presentation.

## Composition Boundary
- `RenderPipelineAsset` is the declarative configuration.
- `RenderRuntime` owns live runtime state for that asset.
- `PreparedFrame` / `PreparedView` carry typed data between preparation and execution.
- `FramePipeline` is the execution engine.
- New renderer integrations should normally follow this shape:
  - feature registration
  - extractor and/or per-view preparation
  - phase items or explicit pipeline steps
  - typed payload access through prepared frame/view stores

## Implementation Guidelines
- Do not reintroduce the legacy domain-centric API model, old presentation config APIs, or app-side domain access patterns.
- Keep new high-level work registration-driven: builder registrations, extractors, draw functions, phases, GPU tables, and explicit pipeline steps.
- Keep ECS authoring in `component/`, view/camera semantics in `view/`, shared GPU resources in `gpu/`, and light/shadow logic in `lighting/`.
- `GpuScene` is shared scene upload state, not a universal home for every renderer-specific cache.
- Only promote concepts into shared frame state when they are truly cross-renderer.
- When a pass needs canonical scene inputs, use the typed scene-input accessors on execution state. `SceneGBufferSlots` is the source of truth; the generic slot map is for ad-hoc resources and does not mirror `scene_*` attachments.
- Keep Live2D-specific semantics local to `live2d/`.
- If you change public render or app-facing APIs, validate examples with `cargo check --examples --features app`.

## Test Commands
- Run render tests: `cargo test --features app`
- Run render graph tests: `cargo test --features app graph`
- Run runtime tests: `cargo test --features app render::runtime::tests`
- Run extract tests: `cargo test --features app render::extract`
- Run example compile check after high-level renderer changes: `cargo check --examples --features app`

## Relation to Other Modules
- GPU: `src/gpu/context.rs` provides `GpuContext`.
- ECS: high-level integration happens through extractors operating on `World`.
- App: `src/app/runner.rs` drives `RenderRuntime` and exposes `feature_mut` / `with_feature_mut` on `FrameContext`.
