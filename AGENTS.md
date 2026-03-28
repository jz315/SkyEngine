# AGENTS.md

## Overview
- This repo is a Rust ECS prototype focused on query-path performance.
- The current hot path is the typed prepared query API in `src/ecs/query.rs`.
- Storage is columnar per chunk, not entity-interleaved.
- Benchmarks are Criterion-based and live under `benches/`.

## Repo Map
- `src/ecs/archetype.rs`: archetype definition and builders.
- `src/ecs/chunk.rs`: chunk allocation, column offsets, and per-chunk storage.
- `src/ecs/query.rs`: typed prepared queries, dynamic query compatibility layer, and query tests.
- `src/ecs/world.rs`: world storage and archetype epoch tracking.
- `src/reflect/reflect.rs`: runtime type registry used by both dynamic and typed query setup.
- `benches/sky.rs`: project benchmark targets for typed query hot paths.
- `benches/hevy.rs`: `hecs` comparison benchmark.
- `src/main.rs`: scratch/local playground, not the canonical API surface.

## Current Query Model
- Preferred API: `world.query::<Q>() -> PreparedQuery<Q>`.
- `PreparedQuery<Q>` caches matching archetypes and refreshes only when `World::archetype_epoch()` changes.
- Typed queries are the performance path and should be preferred for runtime systems.
- Dynamic `Query { types: Vec<Type> }` + `QueryIter` is still supported, but it is a compatibility/tooling path.
- Duplicate component types in a single query are rejected on purpose.

## Storage and Performance Notes
- Chunks use heap-allocated, aligned column storage.
- Archetypes are sorted by component type id; never assume builder insertion order is preserved.
- Query code is performance-sensitive. Avoid adding abstraction layers to the typed hot path unless they compile away cleanly.
- `Cargo.toml` keeps `release.debug = true` so profilers can resolve hot code.
- If you change query internals, validate both correctness and codegen. This repo has already regressed once from “cleaner” abstractions that hurt vectorization.

## Bench and Test Commands
- Run tests: `cargo test`
- Run project bench: `cargo bench --bench sky`
- Compare against `hecs`: `cargo bench --bench sky --bench hevy`
- The fair head-to-head workload is `sky_2_of_4` vs `hecs_2_of_4`.
- `sky_4_of_4` is the project-side regression check for typed query generalization.

## Implementation Guidelines
- Prefer typed archetype construction with `create_archetype().add_rust_component::<T>()`.
- Prefer typed queries over raw pointer callbacks for new runtime code.
- Keep dynamic query support working, but do not optimize it at the expense of typed query codegen.
- If query caching changes, preserve the epoch-based invalidation model in `World`.
- Do not rely on `src/main.rs` for correctness or benchmark behavior; it is not the source of truth.

## Commit Hygiene
- Do not commit profiler artifacts such as `sky-profile*.json.gz` or `*.syms.json`.
- Do not mix benchmark/profiler output cleanup with ECS logic changes unless explicitly asked.
- Be careful with local edits in `src/main.rs`; treat them as user-owned unless told otherwise.
