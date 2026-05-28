# AGENTS.md

## Overview
- This repo is a Rust game engine with a chunk-based ECS core, an app runner, asset/audio/video/UI/runtime modules, and a programmable render stack.
- **ECS**: the performance-critical paths are typed prepared queries, chunk iteration, and structural entity/component transitions. Entities, bundles, typed queries, optional query params, filters, deferred commands, resources, and a lightweight system schedule are all in active use.
- **Rendering**: the current high-level `wgpu` path is `RenderPipelineAsset` / `RenderPipelineBuilder` -> `RenderRuntime`, installed through the app-facing `SceneRenderer` backend wrapper. Built-ins include `SpriteFeature`, `TilemapFeature`, `OpaquePhase`, `TransparentPhase`, material prepasses, shadows, GI steps, and post-fx. Internally, execution is shared through `PreparedFrame` / `PreparedView`, `FramePipeline`, and `RenderGraph`.
- **Renderer backends**: `WgpuSceneRenderer` wraps `RenderRuntime`. Optional Kajiya and Renderling backends consume a backend-neutral `SceneSnapshot` extracted from ECS.
- **App lifecycle**: `App` owns the winit event loop, active `SceneRenderer`, input resource sync, optional asset/audio/video service updates, diagnostics, screenshots, and frame present lifecycle behind the `app` feature flag.
- **UI**: current UI is feature-gated. `ui-core` provides `UiHost` / `UiBackend`; `ui-legacy` adapts the retained ECS UI; `ui-neo` installs the experimental EUI-NEO-style backend; `yakui-ui` installs the experimental yakui backend; `egui` is a separate immediate-mode overlay integration under `src/app/egui_integration.rs`.
- Benchmarks are Criterion-based under `benches/`, with a single canonical `fair` target and engine-specific implementations split under `benches/fair/`.

## Canonical API Surface
- **ECS** entry points: `sky_engine::ecs` - `World`, `EntityId`, `Bundle`, `PreparedQuery`, `Commands`, `With`, `Without`, `System`, `Time`.
- **Dynamic ECS** entry points: `sky_engine::ecs::dynamic` - runtime-typed bundles and queries for tools, scripting, and reflection-driven workflows.
- **Expert ECS** entry points: `sky_engine::ecs::expert` - low-level archetype, component metadata, and unsafe uninitialized spawn helpers for engine internals and benchmarks.
- **Render** entry points: `sky_engine::render` - `RenderRuntime`, `RenderPipelineAsset`, `RenderPipelineBuilder`, `RenderFeature`, `RenderPhase`, `SpriteFeature`, `TilemapFeature`, `SceneRenderer`, `Camera`, `Color`, `Texture`.
- **Expert render** entry points: `sky_engine::render::expert` - `FramePipeline`, `RenderGraph`, `DrawFunction`, `OpaquePhase`, `TransparentPhase`, passes, post-fx, targets, and lower-level GPU composition primitives.
- **GPU** entry points: `sky_engine::gpu` - `GpuContext` (wraps wgpu device/queue/surface).
- **Input** entry points: `sky_engine::input` (behind `features = ["app"]`) - `Input`, `InputActions`, `InputBinding`, `InteractionContext`.
- **App** entry points: `sky_engine::app` (behind `features = ["app"]`) - `App`, `AppConfig`, `FrameContext`, `SetupContext`, `AppState`.
- **Asset** entry points: `sky_engine::asset` (behind `features = ["asset"]`) - `AssetServer`, handles, texture assets, cooked asset support.
- **UI** entry points: `sky_engine::ui` (behind UI features) - `UiHost`, `UiBackend`, `UiCaptureState`, `UiPlugin` (`ui-legacy`), `neo::{NeoUiPlugin, NeoUiBackend, NeoRuntime}` (`ui-neo`), `YakuiUiPlugin` (`yakui-ui`).
- **Tile scene** entry points: `sky_engine::tile` - `Tiles`, `Map`, `MapBuilder`, `MapEditor`, `TilePalette`, `TileLayer`, `CollisionLayer`, `MetadataLayer`, `ObjectLayer`, `TileCell`, `TileRef`, `MapData`, and related grid/palette/object model types.
- **Scene/VN/audio/video** entry points are feature-gated under `sky_engine::scene`, `sky_engine::vn`, `sky_engine::audio`, and `sky_engine::video`.
- Preferred entity construction is bundle-based: `world.spawn((A, B, ...))` and `world.spawn_batch(...)`.
- Preferred query construction is typed: `world.query::<Q>()` or `world.query_filtered::<Q, Flt>()`.
- There is no `sky_engine::ecs::raw` compatibility layer. Use typed ECS APIs first, `ecs::dynamic` for safe runtime-typed access, and `ecs::expert` for explicit low-level internals.

## Repo Map
- `src/lib.rs`: crate root, global allocator setup, public module exports and feature gates.
- `src/ecs/`: archetype/chunk ECS, typed queries, dynamic queries, expert internals, bundles, resources, commands, and schedule execution.
- `src/reflect/`: runtime type registry, layout metadata, and type-erased drop support.
- `src/math/`: engine-facing math re-exports/types, currently backed by `glam`.
- `src/action_queue.rs`: lightweight action queue utility.
- `src/diagnostics/`: diagnostic events, console output, and app-runner reporting support.
- `src/asset/`: asset registry/server, cooked asset metadata, texture assets, and asset handles.
- `src/gpu/`: `GpuContext`, headless/device helpers, frame encoder lifecycle, screenshot/readback support.
- `src/input/`: raw keyboard/mouse input, action maps, input sources, and interaction context.
- `src/app/`: `App`, `AppConfig`, `FrameContext`, `SetupContext`, winit runner, and optional egui integration.
- `src/render/`: rendering framework (see `src/render/AGENTS.md` for module-level rules).
- `src/ui/`: backend-neutral UI host plus legacy ECS, EUI-NEO-style, and yakui backends (see `src/ui/AGENTS.md`).
- `src/tile/`: format-neutral tile scene model, editable documents, palettes, layers, objects, persistence, Tiled import/export facades, and sync into render tilemaps.
- `src/audio/`: audio server, backend, commands, ECS sync, and audio asset/types.
- `src/video/`: video server, streamed playback state, commands, FFmpeg backend, and frame queues.
- `src/scene/`: serializable scene/prefab documents, IDs, validation, capture, and spawning.
- `src/physics/`: optional Rapier-backed 2D physics runtime.
- `src/vn/`: visual novel / Galgame script runtime, systems, UI/audio/video bindings, and presentation helpers.
- `src/main.rs`: scratch/local playground, not the canonical API surface.
- `benches/common.rs`: shared components, constants, and helpers for all benchmarks.
- `benches/fair/main.rs`: canonical apples-to-apples comparison entry point against `hecs` and `bevy_ecs`.
- `benches/fair/sky.rs`, `benches/fair/hecs.rs`, `benches/fair/bevy.rs`: engine-specific fair benchmark implementations.
- `examples/`: feature-focused examples split into `ecs`, `render`, `ui`, `vn`, `scene`, `physics`, `live2d`, `demo`, `game`, and `compare`.
- `docs/`: current user/developer docs. Planning or future architecture notes belong under `docs/plan/`, not in `AGENTS.md`.
- `README.md`, `README_zh.md`: user-facing overview and quick-start docs.
- `benches/BENCHMARKS.md`, `benches/BENCHMARKS_CN.md`: benchmark policy, history, and recorded local results.

## Current Query Model
- Preferred runtime path: `world.query::<Q>() -> PreparedQuery<Q>` and `world.query_filtered::<Q, Flt>() -> PreparedQuery<Q, Flt>`.
- `PreparedQuery` caches matching archetypes and refreshes only when `World::archetype_epoch()` changes.
- Typed queries support entity iteration via `for_each`.
- Typed queries support chunk iteration via `for_each_chunk`.
- Typed queries support entity-aware variants via `for_each_with_entity` and `for_each_chunk_with_entities`.
- Typed queries provide helpers like `count` and `is_empty`.
- Optional query params are supported via `Option<&T>` and `Option<&mut T>`.
- Compile-time archetype filters are supported via `With<T>`, `Without<T>`, and tuples of filters.
- Query tuple support in `QuerySpec` currently goes up to 8 parameters.
- Duplicate component types in a single query are rejected intentionally for both typed and dynamic queries.
- Runtime-typed ECS access is through `ecs::dynamic::DynamicQuery` and `ecs::dynamic::DynamicBundle`.
- `ecs::dynamic` performs runtime access validation and should stay separate from the typed query hot path.
- `ecs::expert` is the only public low-level ECS surface for archetype metadata and unsafe uninitialized entity construction.

## World and Structural Model
- `World` owns entities, archetype-backed data, resources, and the optional system schedule.
- `EntityId` is generational; stale IDs must be treated as invalid.
- Structural component inserts/removes migrate entities across archetypes using cached transition plans and copy spans.
- Entity removal is swap-compacting and must always preserve moved-entity location correctness.
- `Commands` is the deferred structural mutation path.
- Deferred entity commands are coalesced per entity, and flush order follows first-seen entity order.
- Resources are typed singletons stored in the world.
- Scheduling uses `world.group("name")`, `tick()`, `tick_with_delta()`, and `shutdown()`.
- Groups run in creation order; fixed-timestep groups accumulate time and may run multiple substeps per frame.

## Current App and Module Installation Model
- There is not currently one universal engine-level `Plugin` trait in `src/app`.
- Existing installable modules use local installer shapes:
  - `UiPlugin::install(world)` for the legacy retained UI resources/backend.
  - `YakuiUiPlugin::install(world)` for the experimental yakui UI backend.
  - `NeoUiPlugin::install(world)` for the experimental EUI-NEO-style UI backend.
  - `VnPlugin::install(world)` for visual-novel runtime resources and systems.
- Keep `AGENTS.md` files factual. Do not add future plugin-system plans here; put proposals under `docs/plan/`.
- `App::with_render_pipeline(...)` installs a `RenderPipelineAsset`; `WgpuSceneRenderer` materializes it as a `RenderRuntime` after GPU creation.
- `FrameContext` is the app-facing per-frame access point for rendering, backend-neutral render assets, texture readiness, screenshots, UI facade methods, egui overlays, and wgpu escape hatches.

## Current Tile Scene Model
- `src/tile/` is the high-level tile scene and authoring layer. It owns the `Tiles -> Map` facade, `MapData`, palettes, layers, tile references, objects, edit history, persistence, and format import/export.
- `src/render/tilemap/` is the low-level render implementation. It owns `TilemapStorage`, `TilemapRenderer`, extraction, chunk/GPU caches, draw logic, and the quick render-only Tiled path.
- Prefer `tile::Tiles::open_tiled`, `tile::Tiles::create`, and the returned `tile::Map` facade for game/editor maps that need editing, persistence, palettes, objects, collision, metadata, or format round-tripping.
- Use `render::TilemapStorage` / `render::TilemapRenderer` directly only for render demos, low-level renderer tests, or code that intentionally bypasses the tile scene model.
- Tiled authoring import/export is exposed through `Tiles::open_tiled`, `Map::save`, `Map::save_as`, and `Map::save_as_tiled`; the internal format facade lives under `src/tile/io/tiled`.
- `render::TiledImport` / `render::TiledMapInstance` remain the render-only fast path for loading Tiled data directly into render entities. Do not use them as the general game/editor tile scene model.

## Current UI Model
- `ui-core` owns the backend-neutral UI host contract: `UiHost`, `UiBackend`, `UiBackendId`, `UiCaptureState`, event handling, begin-frame updates, and overlay rendering.
- `ui-legacy` is the current retained ECS UI path. It owns UI components, layout, input, state, text, and direct overlay rendering.
- `ui-neo` is the experimental EUI-NEO-style declarative UI path. The host-agnostic DSL/runtime/widgets live in `crates/eui-neo`, the wgpu overlay renderer lives in `crates/eui-neo-wgpu`, and `src/ui/neo/` adapts them into SkyEngine's `UiHost`.
- `ui-neo` exposes layout-safe helper APIs for common failure-prone surfaces: `Ui::scroll_y` / `widgets::scroll_y` for vertical scroll regions, `Ui::popover` / `widgets::popover` for root-layer anchored floating content, and `.rounded_clip(...)` / `.clip_to_radius()` for rounded clipped containers.
- `yakui-ui` installs `YakuiBackend` into `UiHost`. It handles winit events, updates yakui state, reports capture, and renders through `yakui_wgpu`.
- `egui` is independent of `UiHost`; it lives in `src/app/egui_integration.rs` and renders at the end of the app frame.
- Current UI overlays are rendered after the scene by `FrameContext::render_ui_overlays()`, `FrameContext::render_ui()`, or egui end-frame integration. There is no canonical `UiPhase` in the render pipeline today.
- UI input capture is expressed through `UiCaptureState` and event `consumed` responses. Preserve this when changing app input routing.

## EUI-NEO Port Rules
- Local source reference: `C:\Coding\EUI-NEO`. Check it before changing `ui-neo` APIs, widget behavior, layout, event ordering, animation, renderer behavior, or gallery parity.
- Keep `src/ui/neo/` behavior mechanically traceable to EUI-NEO sources: `core/dsl*.h`, `core/layout.h`, `core/event.h`, `core/animation.h`, `core/image.*`, and `components/*.h`.
- Preserve EUI-NEO defaults, clamp rules, callback ordering, transition masks, z-index/layering, modal input blocking, focus, clipboard, IME rect, and dirty/redraw behavior unless SkyEngine platform seams require a documented adaptation.
- Keep reusable widget behavior in `crates/eui-neo/src/widgets/` or shared neo runtime modules; `src/ui/neo/` should stay a SkyEngine adapter. `examples/ui/neo/eui_gallery.rs` is a parity pressure test, not a place to hide missing component behavior.
- Prefer `scroll_y` for vertically scrollable UI panels instead of hand-composing a clipped viewport, translated content, scrollbar, and manual content height. Use `.inset(...)` to keep the viewport and scrollbar inside rounded outer shells, `.offset_bind(...)` for state, and auto content-height measurement unless an explicit height is required.
- Prefer `popover` for dropdowns, menus, pickers, and other floating UI that should not affect parent layout. Popovers are root-layer content anchored from the previous resolved frame, so call `.anchor(...)` with a stable element id and provide `.fallback_anchor(...)` when first-frame placement matters.
- Use `.rounded_clip(radius)` or `.clip_to_radius()` when the visual shell is rounded and its children must be clipped to the same shape. `UiClip` carries the clip rect and radius through draw-list generation, hit testing, and `eui-neo-wgpu` primitive rendering.
- In retained `ui-neo`, keep layout inputs stable for visual-only animation. Do not drive progress fills, chart bars, pulses, meters, or decorative bars by changing `.size(...)`, `.min_width`, `.height`, margins, or grow every frame. Use a stable layout box plus transform/scale, color, opacity, clip, or draw-time state so retained scopes can stay structurally compatible and partial layout can run.
- Validate visible UI with SkyEngine's built-in screenshot path (`FrameContext::request_screenshot`) rather than browser screenshots.
- When debugging retained `ui-neo` layout, do not trust a single-frame layout dump. Many failures are temporal: `live_scope`, animation, scroll widgets, and dirty-scope partial rebuilds can make a correct first frame drift on later frames. For suspected retained UI bugs, dump the same element across several frames and compare layout frame, active clip rect, draw-list command, renderer primitive/scissor data, and final screenshot pixels before assigning blame to layout, draw-list generation, or WGPU.
- Keep the EUI-NEO port plan consolidated in `docs/plan/eui_neo_rust_ui_port_plan.md`; remove completed execution plans from that file instead of creating more plan files.

## Storage and Performance Notes
- Storage is columnar per chunk, never entity-interleaved.
- `CHUNK_SIZE` is currently `512 * 1024` bytes in `src/ecs/chunk.rs`.
- Chunk backing blocks are pooled per thread with a retained-budget cap (`4 MiB` today).
- `Data` caches one empty `spare_chunk` per archetype to avoid unnecessary pool round-trips during spawn/despawn churn.
- Archetype component lists are sorted by component type ID; never assume builder insertion order is preserved.
- Archetype component-index lookup uses a thread-local last-hit cache on top of binary search.
- The type registry and several structural metadata paths use `rustc_hash::FxHashMap`; do not casually regress hot structural paths back to `std::collections::HashMap`.
- Query/filter/archetype code is performance-sensitive. Avoid adding abstraction layers to the typed hot path unless they compile away cleanly.
- If you change query internals, validate both correctness and codegen. This repo has already had regressions from cleaner-looking abstractions that hurt vectorization or loop quality.
- If you change chunk/world transition logic, validate drop semantics for non-`Copy` components in addition to plain movement correctness.
- `Cargo.toml` keeps `[profile.release] debug = true` so profilers can resolve hot code.

## Benchmark and Test Commands
- Run all tests (ECS/core only): `cargo test`
- Run all tests with app/render enabled: `cargo test --features app`
- Run render graph tests: `cargo test --features app graph`
- Run runtime tests: `cargo test --features app render::runtime::tests`
- Run legacy UI tests: `cargo test --features ui-legacy`
- Run tile scene tests: `cargo test --features app tile::`
- Run EUI-NEO-style UI tests/builds: `cargo test --manifest-path crates/eui-neo/Cargo.toml`, `cargo test --manifest-path crates/eui-neo-wgpu/Cargo.toml`, `cargo test --features ui-neo ui::neo`, and `cargo check --examples --features ui-neo`
- Run yakui UI tests/builds: `cargo test --features yakui-ui`
- Run VN tests: `cargo test --features vn`
- Run render/example compile check after render/app API changes: `cargo check --examples --features app`
- Run legacy UI example compile check after retained UI changes: `cargo check --examples --features ui-legacy`
- Run canonical fair comparison: `cargo bench --bench fair`
- Run all benches: `cargo bench`
- Run one engine slice: `cargo bench --bench fair -- sky`
- Run one exact benchmark: `cargo bench --bench fair -- fair_random_access/get/sky --exact`
- Render graph tests requiring GPU use `create_test_device()` or `GpuContext::new_headless()` and need a GPU-capable environment.
- EUI-NEO visual verification examples support built-in screenshot probes via `SKY_NEO_SCREENSHOT_PATH`, `SKY_NEO_SCREENSHOT_FRAME`, and `SKY_NEO_EXIT_AFTER_SCREENSHOT`; gallery overlay states can be forced with variables such as `SKY_NEO_GALLERY_DIALOG_OPEN`, `SKY_NEO_GALLERY_CONTEXT_OPEN`, `SKY_NEO_GALLERY_DATE_OPEN`, `SKY_NEO_GALLERY_TIME_OPEN`, `SKY_NEO_GALLERY_COLOR_OPEN`, and `SKY_NEO_GALLERY_PAGE`.

## Benchmark Policy
- `benches/fair/main.rs` is the only canonical cross-engine comparison suite.
- `fair` only includes workloads that Sky, hecs, and Bevy can all express through safe public APIs.
- Query/prepared state must be created outside the timed loop in `fair` for every engine.
- Engine-specific fair implementations live under `benches/fair/` and are selected via Criterion filters rather than separate bench targets.
- Historical benchmark numbers live in `benches/BENCHMARKS.md` / `benches/BENCHMARKS_CN.md`; treat them as machine-specific and time-specific.

## Implementation Guidelines
- Prefer bundle-based `spawn` / `spawn_batch` for normal runtime code.
- Prefer typed queries over dynamic runtime-typed iteration for application and system code.
- Prefer `for_each_chunk` when a loop is genuinely hot and chunk-slice code helps vectorization or batching.
- Use `ecs::expert::create_archetype().add_rust_component::<T>()` only when low-level archetype construction is actually needed.
- Keep `ecs::dynamic` working for tools and reflection-driven code, but do not optimize it at the expense of typed query codegen.
- Do not reintroduce `ecs::raw` compatibility exports or pointer-based public query iteration.
- If query caching changes, preserve the epoch-based invalidation model in `World`.
- If structural transition logic changes, preserve generational entity validity.
- If structural transition logic changes, preserve moved-entity location updates.
- If structural transition logic changes, preserve non-`Copy` drop behavior.
- If structural transition logic changes, preserve resource lifetime correctness.
- If schedule code changes, preserve group creation order and fixed-step accumulator semantics.
- Do not rely on `src/main.rs` for correctness, benchmarks, or API direction; it is not the source of truth.
- Examples are useful usage references, but benchmark behavior and correctness expectations come from `src/` tests plus the bench suites.
- Keep tile scene API names directional: use importer/exporter names for external formats, edit/refresh names for document mutations and render sync, and reserve `World::spawn` for ECS entity creation in examples where practical.
- Render and app-facing example builds are part of the compatibility surface. If you change high-level render APIs, `RenderRuntime`, `RenderPipelineAsset`, `RenderPipelineBuilder`, `RenderPhase`, `SceneRenderer`, `App`, or demo/shared render helpers, run `cargo check --examples --features app` instead of relying only on unit tests.
- Unify heterogeneous renderers at the composition layer (`RenderRuntime` / `RenderPipelineAsset` / `PreparedFrame` / `PreparedView`) or through backend-neutral `SceneSnapshot` for non-runtime backends. Do not force every renderer feature into one shared scene schema.
- `GpuScene` is the shared scene upload layer for the current high-level wgpu runtime, not the universal frame schema for every backend or future renderer payload.
- When adding a new renderer family (for example Live2D, text, particles, mesh-like 2D), prefer: family-specific prepare/cache/upload path + feature registration + typed frame/view payload entry.
- Keep render-phase and draw execution contexts generic. Prefer typed payload access over adding one-off renderer-specific fields to shared execution state.
- Only introduce shared scene-level abstractions for concepts that are truly cross-feature, such as view/camera/viewport/order/layer semantics. Do not prematurely unify geometry/material/runtime models.

## Render Graph Guidelines
- The render graph has its own detailed `AGENTS.md` at `src/render/graph/AGENTS.md`; read it before modifying graph internals.
- All resource handle validation must use the `handle_token` mechanism; never index into `textures`/`buffers` without checking the token first.
- `compile()` is the single source of truth for execution order. It is idempotent; repeated calls return the cached result.
- `allocate_physical_resources()` must be called after `compile()` and before accessing physical resources.
- `buffer_usage_for()` must only be called after compilation (enforced by `debug_assert`).
- `execute_copy_pass()` must remain `&self` (not `&mut self`) to avoid borrow conflicts with `PhysicalResources` during execution.
- `alias_group_count()` only counts multi-member groups (groups where actual physical sharing occurs).
- `resource_has_external_sink` and `resource_has_external_source` intentionally share the same implementation - imported resources are both sources and sinks.
- Copy passes must flush the current frame encoder before submitting their own command buffers.
- All new `CopyOp` variants must register proper reads/writes in `CopyPassSetup` for dependency analysis.
- `queue.write_texture()` (used by `UploadToTexture`) does NOT require 256-byte `bytes_per_row` alignment; `encoder.copy_buffer_to_texture()` does.
- Transient pool keys must be cheap to hash (`FxHashMap`).
- Do not add heavyweight per-frame allocations to the compilation pipeline.

## GPU Context Guidelines
- `GpuContext` wraps the wgpu `Device`, `Queue`, and optional `Surface`.
- `GpuContext::new_headless()` creates a surfaceless context for unit testing render graph allocation without a window.
- The `surface` field is `Option<wgpu::Surface>` - always check `has_surface()` before calling surface-dependent methods.
- Frame lifecycle: `begin_frame()` -> encoder operations -> `end_frame()` submits and presents.

## Commit Hygiene
- Do not commit profiler artifacts such as `sky-profile*.json.gz` or `*.syms.json`.
- Do not mix benchmark/profiler output cleanup with ECS logic changes unless explicitly asked.
- Be careful with local edits in `src/main.rs`, `examples/`, and other user-owned worktree files; treat them as user-owned unless the task explicitly targets them.
- Do not commit local scratch artifacts or temp files unless the task explicitly requires them.
