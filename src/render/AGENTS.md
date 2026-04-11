# AGENTS.md — `src/render`

## Overview
- This module is SkyEngine's `wgpu`-based rendering framework.
- Public API is intentionally split into two layers:
  - `sky_engine::render::*` is the curated high-level facade built around `RenderPipelineAsset` + `RenderComposer` + `RenderDomain`.
  - `sky_engine::render::expert::*` is the explicit low-level entry point for `FramePipeline`, `RenderGraph`, passes, post-fx, targets, and resource systems.
- High-level architecture is now scene-first and domain-based:
  - shared scene model (`SceneView`, `Projection`, `Transform`)
  - programmable pipeline definition (`stage / queue / domain / feature / output chain`)
  - runtime composition (`RenderComposer`)
  - domain-local prepare/upload/execute paths (`SpriteDomain`, `Live2DDomain`, future domains)
- The composition boundary for mixing heterogeneous renderers is the frame/pipeline layer (`PreparedFrame` / `PreparedView` + `FramePipeline` + `RenderGraph`). Do not treat `GpuScene2D` or `PreparedView2D` as the universal frame schema.
- `2D` is not the top-level architectural concept. It is a domain-local usage pattern built on the shared scene model, typically `orthographic + planar content`.
- The GPU backend is `wgpu`. All rendering goes through `GpuContext` (`src/gpu/context.rs`).
- Shader language is WGSL. All shaders live under `shaders/`.
- The module is gated behind `features = ["app"]` for window/surface-dependent code. The optional `live2d` sub-module requires `features = ["live2d"]`.

## Module Architecture

```text
render/
├── mod.rs              — Public render facade and curated re-exports
├── composer/           — High-level runtime orchestration (`RenderComposer`)
├── scene/              — Shared scene/view/projection/sort semantics
├── pipeline/           — Programmable pipeline definitions (`RenderPipelineAsset`, features)
├── domains/
│   ├── sprite/         — Built-in sprite/light render domain
│   └── live2d/         — High-level Live2D domain integration
├── output_chain/       — Shared post-fx / resolve / presentation nodes
├── frame_pipeline.rs   — Generic three-phase frame orchestration core
├── graph/              — Declarative render graph system
├── core/               — Foundational GPU/view/texture/target types
├── passes/             — Reusable render passes (`SpriteBatch`, `MeshPass`, `LightPass`, ...)
├── postfx/             — Reusable post-processing effects
├── resources/          — Shared render resources (`TextureAtlas`, `Material*`, `Mesh`, ...)
├── live2d/             — Low-level Live2D renderer/runtime implementation
└── shaders/            — WGSL shader sources
```

## Sub-Module AGENTS.md References
- **Render Graph**: see [`graph/AGENTS.md`](graph/AGENTS.md) for the full render graph compilation pipeline, handle model, aliasing, reordering, and execution model.
- **Live2D**: see [`live2d/AGENTS.md`](live2d/AGENTS.md) for the Cubism runtime, renderer architecture, and GPU infrastructure requirements.

## File Map

### `mod.rs`
- Module declarations and curated public re-exports.
- Normal application code should start from `sky_engine::render::*`.
- Low-level rendering code should opt into `sky_engine::render::expert::*` instead of internal module paths.
- Preferred high-level names are:
  - `RenderComposer`
  - `RenderDomain`
  - `SpriteDomain`
  - `Live2DDomain`

### `composer/`
- Owns the high-level runtime orchestration layer.
- Important files:
  - `render_composer.rs` — `RenderComposer` state and external runtime API (`from_asset`, `stats`, resize/surface-loss handling, domain lookup)
  - `view_collection.rs` — world camera/view extraction and resolved transform gathering
  - `frame_builder.rs` — per-frame extraction/prepare/frame-payload assembly and stats population
  - `pipeline_runtime.rs` — compiled pipeline installation into `FramePipeline`, feature node wiring, output-chain injection, target-format propagation
  - `nodes.rs` — composer-level utility nodes such as clear seeding and headless keep-alive
- `RenderComposer` should stay an orchestration shell, not a god object. New high-level responsibilities should usually land in a helper module next to it.

### `scene/`
- Owns shared scene semantics used by all render domains.
- Important files:
  - `view.rs` — `SceneView`, `Projection`, transform resolution, shared scene math helpers
  - `types.rs` — render-stage keys, queue descriptors, injection points, stats, output-format policies
- Keep this layer cross-domain. Only concepts that make sense for sprite, Live2D, and future mesh/3D domains belong here.

### `pipeline/`
- Owns programmable pipeline definition, not draw-time sprite internals.
- Important files:
  - `asset.rs` — `RenderPipelineAsset`, `RenderPipelineBuilder`, compiled stage/queue/domain/feature plan, `OutputChainConfig`
  - `feature.rs` — `RenderFeature`, setup/execute contexts, feature node adapter
- `pipeline/` should describe *what* gets scheduled, not own domain-local prepare/upload data.

### `domains/sprite/`
- Owns the built-in sprite/light render domain.
- Important files:
  - `backend.rs` — sprite-domain-local prepare/upload/execute contexts and backend state
  - `extractor.rs` — ECS-to-scene-cache synchronization
  - `scene_cache.rs` — sprite/light scene cache and dirty tracking
  - `prepared.rs` — sorted/cull-checked per-view sprite/light instances and draw spans
  - `gpu_scene.rs` — `GpuScene2D`, the sprite-domain GPU payload inserted into `PreparedFrame`
  - `sprite_pass/`, `light_node/`, `composite_node.rs` — sprite-domain view execution nodes
  - `render_pipeline.rs` — expert-facing `SpriteFramePipeline` adapter layered on `FramePipeline`
  - `access.rs` — sprite-domain-only typed payload helpers
- `GpuScene2D` and `PreparedView2D` are sprite-domain implementation details exposed for expert use. They are not the universal frame contract.
- `SpriteDomainFeature` is a sprite-domain-specific expert hook. Do not expand it into the top-level render architecture.

### `domains/live2d/`
- Owns the high-level Live2D domain integration that plugs Live2D into the shared programmable pipeline.
- Important files:
  - `domain.rs` — `Live2DDomain`, ECS extraction, view filtering/sorting, per-view prepared-frame insertion
  - `backend.rs` — high-level Live2D domain state, model instances, per-view prepared frame accumulation, overlay-node creation
- The low-level Live2D renderer/runtime still lives under `src/render/live2d/`.

### `output_chain/`
- Owns shared post-fx / resolve / final-blit nodes that are not specific to one domain.
- Current nodes:
  - `BloomNode`
  - `ToneMapNode`
  - `VignetteNode`
  - `ColorResolveNode`
  - `ViewportBlitNode`
- This layer is the shared output chain installed by `RenderComposer` after the configured stage boundary from `OutputChainConfig`.

### `frame_pipeline.rs`
- `FramePipeline` is the generic three-phase frame orchestrator:
  - `frame_setup`
  - `per_view`
  - `frame_finalize`
- Uses `PreparedFrame` + `PreparedView` as the cross-domain frame contract.
- `FramePayloadStore` and `ViewPayloadStore` are typed registries for prepared data.
- `PhaseState` + `ResourceSlotMap` carry cross-phase resource slots. The only framework-reserved slot is `"current_color"`.
- New renderer domains should plug into `FramePipeline` first, then optionally wrap themselves in a curated facade.

## Composition Boundary
- `RenderPipelineAsset` is the high-level definition surface. It describes stages, queues, domains, feature injection points, and the shared output chain.
- `RenderComposer` is the runtime object that owns the active set of `RenderDomain`s and translates one compiled pipeline into a `FramePipeline` execution.
- `FramePipeline` is the generic backend composition layer for mixing render domains under one frame, graph, and three-phase execution model.
- `PreparedFrame` carries frame-scoped typed payloads; each `PreparedView` carries view-scoped typed payloads.
- New render domains should normally follow this shape:
  - domain-specific extract/cache/prepare/upload path
  - one or more `FrameViewNode` / `FrameSetupNode` / `FrameFinalizeNode`
  - typed payload access through `PreparedFrame` / `PreparedView`
- Only promote abstractions into shared pipeline state when they represent true cross-domain concepts, such as camera/view/viewport/order/layer behavior. Keep renderer-specific geometry, masking, batching, and runtime semantics local to that renderer.

## Core / Passes / PostFx / Resources

### `core/`
- Foundational GPU/view types:
  - `camera.rs` — `Camera2D`, `ViewUniform`, `RenderView`
  - `color.rs` — linear RGBA color utilities
  - `texture.rs` — GPU texture wrapper and upload/file descriptors
  - `target.rs` — resizable persistent render targets
  - `fullscreen.rs` — shared fullscreen triangle helpers and pipeline cache
  - `viewport.rs` — viewport rectangle helpers

### `passes/`
- Reusable render passes and draw helpers:
  - `SpriteBatch`
  - `MeshPass`
  - `LightPass`
  - `CompositePass`
- These are reusable low-level building blocks and are not themselves high-level render domains.

### `postfx/`
- Reusable post-processing effect implementations:
  - `Bloom`
  - `ToneMap`
  - `Vignette`
- The high-level output chain nodes wrap these effects into `FrameViewNode`s.

### `resources/`
- Shared render resources and authoring/runtime helpers:
  - `TextureAtlas`
  - `Blackboard`
  - `Material*`
  - `Mesh`

## Rendering Pipeline (Typical SpriteDomain Frame)

1. **Transform resolve + view collection**: `WorldViewCollector` resolves scene transforms and extracts visible camera views.
2. **Domain extraction**: each `RenderDomain` extracts its own world state. For `SpriteDomain`, `SceneExtractor::sync_incremental()` refreshes `SceneCache2D`.
3. **Preparation**: each domain prepares view-scoped data. For `SpriteDomain`, `PreparedRenderWorld2D::prepare_scene()` sorts visible sprites/lights per view and builds draw spans.
4. **GPU upload**: each domain uploads its own prepared data. For `SpriteDomain`, `GpuScene2D::upload_scene_frame()` uploads the prepared per-frame instance data.
5. **Scene assembly**: `RenderComposer` builds a `PreparedFrame`, inserts shared frame payloads (`RenderSettings`, `GpuScene2D`, Live2D prepared frames, etc.), and inserts one `PreparedView` per visible view with domain-specific payloads such as `PreparedView2D`.
6. **View phase**: active domains contribute view nodes on top of `FramePipeline`; the built-in sprite domain runs `SpriteSceneNode`, `SpriteLightNode`, and `SpriteCompositeNode`, while the shared output chain handles post-fx and presentation.
7. **Present**: `ViewportBlitNode` writes the prepared view result to the surface when a surface exists, then `GpuContext::end_frame()` submits and presents.

Other domains may replace steps 2–6 with their own prepare/upload/execute phases, but they should still converge at the same composition layer: nodes scheduled by `FramePipeline`, with data routed through typed frame/view payloads.

## Implementation Guidelines
- Keep top-level render architecture scene-first and domain-based. Do not reintroduce `2D` as the main architectural story.
- All new passes that render to `RenderTarget` must support per-format pipeline caching.
- New fullscreen effects should build on `FullscreenPipeline` + `compose_fullscreen_shader()`, not standalone vertex shaders.
- `RenderTarget` always includes `COPY_SRC | COPY_DST` usage. This is intentional for render graph copy ops.
- Do not bundle samplers with `Texture` objects. Sampler selection happens at bind group creation time.
- Uniform buffers must follow WGSL alignment rules (16-byte struct alignment, 4/8/16 per field).
- If you touch `RenderComposer`, keep orchestration helpers factored into nearby modules instead of growing one central file.
- If a helper needs `GpuScene2D`, `PreparedView2D`, sprite draw spans, or sprite-domain payload access, it belongs in `domains/sprite/access.rs`, not `frame_pipeline.rs` or `pipeline/`.
- Do not force Live2D or future render domains into `SceneCache2D` / `PreparedRenderWorld2D` unless the semantics genuinely match sprite/light rendering.
- Prefer graph-backed composition over direct surface-only integration when adding a new render domain.
- New domain-specific data should enter execution through typed payloads on `PreparedFrame` / `PreparedView`, not through a monolithic shared execution context.
- If you change high-level render or app-facing APIs, also validate demos/examples with `cargo check --examples --features app`. The render examples are part of the supported surface.

## Test Commands
- Run all render tests: `cargo test --features app`
- Run render graph tests: `cargo test --features app graph`
- Run specific pass tests: `cargo test --features app render::passes`
- Run post-fx tests: `cargo test --features app render::postfx`
- Run material tests: `cargo test --features app render::resources::material`
- Run atlas tests: `cargo test --features app render::resources::atlas`
- Run blackboard tests: `cargo test --features app render::resources::blackboard`
- Run render/example compile check after public renderer or app changes: `cargo check --examples --features app`
- GPU-dependent tests require a GPU-capable environment and use `GpuContext::new_headless()`.

## Relation to Other Modules
- **GPU**: `src/gpu/context.rs` provides `GpuContext` — the wgpu device/queue/surface wrapper. All render code takes `&GpuContext` or `&mut GpuContext`.
- **ECS**: render passes and resources are not ECS-aware by default. High-level integration happens through domain extraction and app-level frame execution.
- **App**: `src/app/runner.rs` manages the winit event loop and `GpuContext` lifecycle, and drives `RenderComposer` during the frame loop. `FrameContext` exposes `domain_mut` / `with_domain_mut` for domain access.
