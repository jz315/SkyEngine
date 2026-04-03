# AGENTS.md

## Overview
- This repo is a Rust ECS library built around chunk-based, archetype-oriented, columnar storage.
- The performance-critical paths are typed prepared queries, chunk iteration, and structural entity/component transitions.
- The runtime is functional today: entities, bundles, typed queries, optional query params, filters, deferred commands, resources, and a lightweight system schedule are all in active use.
- Benchmarks are Criterion-based under `benches/`, with a single canonical `fair` target and engine-specific implementations split under `benches/fair/`.

## Canonical API Surface
- Main entry points live under `sky_engine::ecs`.
- Primary runtime types: `World`, `EntityId`, `Bundle`, `PreparedQuery`, `Commands`, `With`, `Without`, `System`, and `Time`.
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
- `examples/queries.rs`: typed query examples.
- `examples/commands.rs`: deferred command buffer example.
- `examples/systems.rs`: schedule and grouped-system example.
- `examples/hello_ecs.rs`: minimal getting-started example.
- `examples/particles.rs`, `examples/asteroids.rs`, `examples/boids.rs`, `examples/snake.rs`: demo-feature examples.
- `examples/boids_hecs.rs`, `examples/boids_bevy.rs`: comparison-feature examples.
- `README.md`, `README_CN.md`: user-facing overview and quick-start docs.
- `BENCHMARKS.md`: benchmark policy, history, and recorded local results.
- `docs/api.md`: API notes/reference material.

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
- Run tests: `cargo test`
- Run canonical fair comparison: `cargo bench --bench fair`
- Run all benches: `cargo bench`
- Run one engine slice: `cargo bench --bench fair -- sky`
- Run one exact benchmark: `cargo bench --bench fair -- fair_random_access/get/sky --exact`
- Run demo examples: `cargo run --example particles --release --features demo`
- Other demo examples use the same `--features demo` pattern (`snake`, `boids`, `asteroids`).
- Comparison examples require `--features compare`.
- Chunk-size sweeps are done by editing `CHUNK_SIZE` in `src/ecs/chunk.rs` and rerunning the relevant benches.

## Benchmark Policy
- `benches/fair/main.rs` is the only canonical cross-engine comparison suite.
- `fair` only includes workloads that Sky, hecs, and Bevy can all express through safe public APIs.
- Query/prepared state must be created outside the timed loop in `fair` for every engine.
- Engine-specific fair implementations live under `benches/fair/` and are selected via Criterion filters rather than separate bench targets.
- Historical benchmark numbers live in `BENCHMARKS.md`; treat them as machine-specific and time-specific.

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

## Commit Hygiene
- Do not commit profiler artifacts such as `sky-profile*.json.gz` or `*.syms.json`.
- Do not mix benchmark/profiler output cleanup with ECS logic changes unless explicitly asked.
- Be careful with local edits in `src/main.rs`, `examples/`, and other user-owned worktree files; treat them as user-owned unless the task explicitly targets them.
- Do not commit local scratch artifacts or temp files unless the task explicitly requires them.
