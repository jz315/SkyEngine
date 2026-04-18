# AGENTS.md

## Overview
- This repo is a Rust game engine with a chunk-based ECS core and a `wgpu`-based 2D rendering framework.
- **ECS**: the performance-critical paths are typed prepared queries, chunk iteration, and structural entity/component transitions. Entities, bundles, typed queries, optional query params, filters, deferred commands, resources, and a lightweight system schedule are all in active use.
- **Rendering**: the default high-level path is `RenderComposer` + registration-driven `RenderPipelineAsset` / `RenderPipelineBuilder`, with built-in `SpriteFeature`, `OpaquePhase`, `TransparentPhase`, and optional `Live2DFeature`. Internally, execution is shared through `PreparedFrame` / `PreparedView`, `FramePipeline`, and `RenderGraph`. The GPU backend is `wgpu`.
- **App lifecycle**: `App` manages the winit event loop, GPU context, and frame lifecycle behind the `app` feature flag.
- Benchmarks are Criterion-based under `benches/`, with a single canonical `fair` target and engine-specific implementations split under `benches/fair/`.

## Canonical API Surface
- **ECS** entry points: `sky_engine::ecs` — `World`, `EntityId`, `Bundle`, `PreparedQuery`, `Commands`, `With`, `Without`, `System`, `Time`.
- **Render** entry points: `sky_engine::render` — `RenderComposer`, `RenderPipelineAsset`, `RenderPipelineBuilder`, `RenderFeature`, `RenderPhase`, `SpriteFeature`, `Camera`, `Color`, `Texture`.
- **Expert render** entry points: `sky_engine::render::expert` — `FramePipeline`, `RenderGraph`, `DrawFunction`, `OpaquePhase`, `TransparentPhase`, passes, post-fx, targets, and lower-level GPU composition primitives.
- **GPU** entry points: `sky_engine::gpu` — `GpuContext` (wraps wgpu device/queue/surface).
- **Input** entry points: `sky_engine::input` (behind `features = ["app"]`) — `Input`, `InputActions`, `InputBinding`.
- **App** entry points: `sky_engine::app` (behind `features = ["app"]`) — `App`, `AppConfig`, `FrameContext`.
- Preferred entity construction is bundle-based: `world.spawn((A, B, ...))` and `world.spawn_batch(...)`.
- Preferred query construction is typed: `world.query::<Q>()` or `world.query_filtered::<Q, Flt>()`.
- Low-level compatibility/benchmark helpers live under `sky_engine::ecs::raw`.

## Repo Map
- `src/lib.rs`: crate root, global allocator setup, public module exports.
- `src/ecs/mod.rs`: canonical ECS re-exports.
- `src/ecs/world.rs`: world storage, entity lifecycle, archetype epoch tracking, resources, structural transitions, and schedule execution.
- `src/ecs/archetype.rs`: archetype intern table, sorted component sets, builder API, and lookup caches.
- `src/ecs/chunk.rs`: chunk allocation, aligned column layout, chunk/block pooling, dense storage, and spare-chunk reuse.
- `src/ecs/bundle.rs`: tuple-based bundle metadata and fast spawn writes.
- `src/ecs/query/mod.rs`: shared query descriptors, prepared-cache logic, and re-exports.
- `src/ecs/query/prepared.rs`: typed prepared query API and typed query tests.
- `src/ecs/query/param.rs`: typed query param/spec machinery, including tuple support and optional params.
- `src/ecs/query/filter.rs`: `With<T>` / `Without<T>` filter logic and tuple-composed filters.
- `src/ecs/query/dynamic.rs`: dynamic query compatibility layer (`Query`, `QueryIter`).
- `src/ecs/commands.rs`: deferred command buffer, spawn batching, per-entity command coalescing, and inline/heap insert payload storage.
- `src/ecs/system.rs`: `System` trait, system groups, fixed-tick policy, and schedule builder.
- `src/ecs/resource.rs`: typed singleton resource storage.
- `src/ecs/entity.rs`: generational entity IDs and entity-location bookkeeping types.
- `src/ecs/raw.rs`: low-level/raw exports used by benches and archetype-oriented tests.
- `src/reflect/registry.rs`: runtime type registry, layout metadata, and type-erased drop support.
- `src/main.rs`: scratch/local playground, not the canonical API surface.
- `benches/common.rs`: shared components, constants, and helpers for all benchmarks.
- `benches/fair/main.rs`: canonical apples-to-apples comparison entry point against `hecs` and `bevy_ecs`.
- `benches/fair/sky.rs`, `benches/fair/hecs.rs`, `benches/fair/bevy.rs`: engine-specific fair benchmark implementations.
- `benches/fair/shared.rs`: shared fair-suite helpers.
- `examples/ecs/queries.rs`: typed query example.
- `examples/ecs/commands.rs`: deferred command buffer example.
- `examples/ecs/systems.rs`: schedule and grouped-system example.
- `examples/ecs/hello_ecs.rs`: minimal getting-started example.
- `examples/ecs/tiny_defense.rs`: ECS-only mini game example.
- `examples/render/`: focused render API showcases (`clear_screen`, `sprite_demo`, `textured_demo`, `lighting_demo`, `render_graph_showcase`, `perf_test`, `renderer_probe`).
- `examples/live2d/`: Live2D-specific probe/demo entry points (`live2d_demo`, `live2d_probe`).
- `examples/demo/`: full GPU showcase demos (`boids`, `boids_classic`, `cosmic_jellyfish`, `neon_galaxy`).
- `examples/legacy/particles.rs`, `examples/legacy/asteroids.rs`, `examples/legacy/snake.rs`: legacy CPU-rendered demos.
- `examples/compare/boids_hecs.rs`, `examples/compare/boids_bevy.rs`, `examples/compare/boids_bevy_gpu.rs`: comparison examples.
- `README.md`, `README_EN.md`: user-facing overview and quick-start docs.
- `benches/BENCHMARKS.md`, `benches/BENCHMARKS_CN.md`: benchmark policy, history, and recorded local results.
- `docs/api.md`: API notes/reference material.
- `src/gpu/context.rs`: `GpuContext` — wgpu device/queue/surface wrapper, headless mode for tests, frame encoder lifecycle.
- `src/gpu/mod.rs`: GPU module re-exports.
- `src/render/`: 2D rendering framework (see `src/render/AGENTS.md` for full module docs).
- `src/render/mod.rs`: render module re-exports.
- `src/render/component/`: ECS-facing render components and settings — camera markers/viewports, sprite/mesh/light components, render settings.
- `src/render/view/`: camera/view/projection/frustum/viewport/transform resolution types used to build `SceneView`s.
- `src/render/gpu/`: shared GPU resource layer — `Texture`, `RenderTarget`, fullscreen helpers, `GpuScene`, `GpuTableManager`.
- `src/render/lighting/`: light data, GPU light tables, `LightPass`, and directional shadow support.
- `src/render/sprite/`: sprite rendering and `SpriteBatch`.
- `src/render/mesh/`: mesh rendering and `MeshPass`.
- `src/render/composite/`: `CompositePass` for scene/light composition.
- `src/render/runtime/`: high-level runtime orchestration around `RenderComposer`.
- `src/render/runtime/presentation.rs`: internal viewport presentation/blit node used by the runtime.
- `src/render/runtime/stats.rs`: render timing helpers and `RenderTimingStats`.
- `src/render/execution/`: generic prepared-frame execution backbone around `FramePipeline`.
- `src/render/graph/`: declarative render graph system (see `src/render/graph/AGENTS.md` for detailed docs).
- `src/render/pipeline/`: registration-driven pipeline builder/runtime traits, asset descriptors, and phase/pass/postfx/compute extension points.
- `src/render/postfx/`: post-processing effects — `Bloom`, `ToneMap`, `Vignette`.
- `src/render/resources/`: shared resource systems — `TextureAtlas`, `Blackboard`, `Material*`.
- `src/render/shaders/`: all WGSL shader sources.
- `src/render/live2d/`: Live2D Cubism model renderer (see `src/render/live2d/AGENTS.md`, feature-gated).
- `src/app/runner.rs`: `App` — winit event loop, frame lifecycle, GPU context management.
- `src/app/config.rs`: `AppConfig` — window title, size, vsync.
- `src/input/`: `Input`, action maps, bindings, and raw keyboard/mouse state helpers.

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
- Dynamic `Query { types: Vec<Type> }` + `QueryIter` still exists for compatibility/tooling, but it is not the primary optimization target.
- `ecs::raw::PreparedQuery` is a low-level/bench-facing export, not the preferred app-facing entry point.

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
- Run all tests (ECS only): `cargo test`
- Run all tests (ECS + render): `cargo test --features app`
- Run render graph tests: `cargo test --features app graph`
- Run a specific render test: `cargo test --features app render::graph::tests::linear_chain_orders_correctly`
- Run reorder tests only: `cargo test --features app reorder::tests`
- Run alias tests only: `cargo test --features app alias::tests`
- Run render/example compile check after render/app API changes: `cargo check --examples --features app`
- Run canonical fair comparison: `cargo bench --bench fair`
- Run all benches: `cargo bench`
- Run one engine slice: `cargo bench --bench fair -- sky`
- Run one exact benchmark: `cargo bench --bench fair -- fair_random_access/get/sky --exact`
- Run legacy CPU demos with `--features demo-legacy` (`particles`, `snake`, `asteroids`).
- Run render and showcase demos with `--features app` (`boids`, `boids_classic`, `cosmic_jellyfish`, `neon_galaxy`, render examples).
- Comparison examples require `--features compare` or `--features compare-bevy` depending on the target example.
- Chunk-size sweeps are done by editing `CHUNK_SIZE` in `src/ecs/chunk.rs` and rerunning the relevant benches.
- Render graph tests requiring GPU use `create_test_device()` or `GpuContext::new_headless()` and need a GPU-capable environment.

## Benchmark Policy
- `benches/fair/main.rs` is the only canonical cross-engine comparison suite.
- `fair` only includes workloads that Sky, hecs, and Bevy can all express through safe public APIs.
- Query/prepared state must be created outside the timed loop in `fair` for every engine.
- Engine-specific fair implementations live under `benches/fair/` and are selected via Criterion filters rather than separate bench targets.
- Historical benchmark numbers live in `benches/BENCHMARKS.md` / `benches/BENCHMARKS_CN.md`; treat them as machine-specific and time-specific.

## Implementation Guidelines
- Prefer bundle-based `spawn` / `spawn_batch` for normal runtime code.
- Prefer typed queries over dynamic/raw-pointer iteration for application and system code.
- Prefer `for_each_chunk` when a loop is genuinely hot and chunk-slice code helps vectorization or batching.
- Use `create_archetype().add_rust_component::<T>()` only when low-level archetype construction is actually needed.
- Keep dynamic query support and `ecs::raw` exports working, but do not optimize them at the expense of typed query codegen.
- If query caching changes, preserve the epoch-based invalidation model in `World`.
- If structural transition logic changes, preserve generational entity validity.
- If structural transition logic changes, preserve moved-entity location updates.
- If structural transition logic changes, preserve non-`Copy` drop behavior.
- If structural transition logic changes, preserve resource lifetime correctness.
- If schedule code changes, preserve group creation order and fixed-step accumulator semantics.
- Do not rely on `src/main.rs` for correctness, benchmarks, or API direction; it is not the source of truth.
- Examples are useful usage references, but benchmark behavior and correctness expectations come from `src/` tests plus the bench suites.
- Render and app-facing example builds are part of the compatibility surface. If you change high-level render APIs, `RenderComposer`, `RenderPipelineAsset`, `RenderPipelineBuilder`, `RenderPhase`, `App`, or demo/shared render helpers, run `cargo check --examples --features app` instead of relying only on unit tests.
- Unify heterogeneous renderers at the composition layer (`RenderComposer` / `RenderPipelineAsset` / `PreparedFrame` / `PreparedView`), not by forcing every renderer feature into one shared scene schema.
- `GpuScene` is the shared scene upload layer for the current high-level renderer, not the universal frame schema for every future renderer payload.
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
- `resource_has_external_sink` and `resource_has_external_source` intentionally share the same implementation — imported resources are both sources and sinks.
- Copy passes must flush the current frame encoder before submitting their own command buffers.
- All new `CopyOp` variants must register proper reads/writes in `CopyPassSetup` for dependency analysis.
- `queue.write_texture()` (used by `UploadToTexture`) does NOT require 256-byte `bytes_per_row` alignment; `encoder.copy_buffer_to_texture()` does.
- Transient pool keys must be cheap to hash (`FxHashMap`).
- Do not add heavyweight per-frame allocations to the compilation pipeline.

## GPU Context Guidelines
- `GpuContext` wraps the wgpu `Device`, `Queue`, and optional `Surface`.
- `GpuContext::new_headless()` creates a surfaceless context for unit testing render graph allocation without a window.
- The `surface` field is `Option<wgpu::Surface>` — always check `has_surface()` before calling surface-dependent methods.
- Frame lifecycle: `begin_frame()` → encoder operations → `end_frame()` submits and presents.

## Commit Hygiene
- Do not commit profiler artifacts such as `sky-profile*.json.gz` or `*.syms.json`.
- Do not mix benchmark/profiler output cleanup with ECS logic changes unless explicitly asked.
- Be careful with local edits in `src/main.rs`, `examples/`, and other user-owned worktree files; treat them as user-owned unless the task explicitly targets them.
- Do not commit local scratch artifacts or temp files unless the task explicitly requires them.
