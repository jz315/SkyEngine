

# Benchmark

## Table of Contents
1. [Quick Start](#quick-start)
2. [Performance Overview](#performance-overview)
3. [Detailed Test Data](#detailed-test-data)

---

## Quick Start

```bash
# Fair comparison of four engines (Sky/hecs/Bevy/Flecs)
cargo compare-ecs

# Single engine test
cargo compare-ecs -- sky
cargo compare-ecs -- hecs
cargo compare-ecs -- bevy
cargo compare-ecs -- flecs

# Run a specific benchmark precisely
cargo compare-ecs -- fair_random_access/get/sky --exact

# Run the repeated traversal stability benchmark
cargo compare-ecs -- fair_iteration_repeated/simple_x32/sky --exact

# Run the larger 100k-entity traversal benchmark
cargo compare-ecs -- fair_iteration_large

# Measure the world-bound query facade, named QueryData, and PreparedQuery
cargo bench --bench bound_query

# Measure typed system dispatch, conflict waves, and system-level parallelism
cargo bench --bench system_schedule

# Chunk size tuning: modify CHUNK_SIZE in src/ecs/chunk.rs and rerun
```

---

## Performance Overview

| Test Scenario | Sky Performance | Key Advantage |
|---------------|-----------------|---------------|
| **Entity Insertion** | 🏆 Leading | **2.1–2.4x** faster than hecs/Bevy, **23–47x** faster than Flecs |
| **Sequential Iteration** | 🏆 Leading | **2.9x** faster than hecs, **4.3x** faster than Bevy for simple iteration |
| **Random Access** | 🥈 Second Best | Bevy is faster with sparse set; Sky is **2x** faster than hecs |
| **Structural Changes** | 🏆 Tied | On par with hecs, **2.3x** faster than Bevy |
| **Full Frame** | 🏆 Leading | **14%** ahead of hecs, **19%** ahead of Bevy, **18%** ahead of Flecs |

---

## Detailed Test Data
Note: All tests were run on the same machine
Test Device: Windows 11, i7-12700F

### 1. Entity Insertion Performance

| Workload | Sky | hecs | Bevy | Flecs | Sky Advantage |
|----------|-----|------|------|-------|---------------|
| `batch_10k` (Batch Insertion) | **120 µs** | 294 µs | 277 µs | 5.67 ms | vs hecs/Bevy: **2.1–2.4x**<br>vs Flecs: **47x** |
| `single_10k` (Single Entity Insertion) | **245 µs** | 416 µs | 523 µs | 5.65 ms | vs hecs/Bevy: **1.7–2.1x**<br>vs Flecs: **23x** |

### 2. Sequential Iteration Performance

| Workload | Sky | hecs | Bevy | Flecs | Notes |
|----------|-----|------|------|-------|-------|
| `simple` (10K entities) | **1.93 µs** | 5.62 µs | 8.23 µs | 2.04 µs | **2.9x** faster than hecs, **4.3x** faster than Bevy |
| `fragmented` (10.4K entities) | **580 ns** | 3.26 µs | 6.20 µs | 843 ns | Significant advantage in fragmented scenarios |
| `heavy_compute` (Matrix Inversion) | **1.85 ms** | 2.39 ms | 2.05 ms | 1.87 ms | On par with Flecs for compute-intensive tasks |

### 3. Random Access Performance

| Workload | Sky | hecs | Bevy | Flecs | Architecture Notes |
|----------|-----|------|------|-------|-------------------|
| `get` (10K Random Lookups) | **73 µs** | 145 µs | **30 µs** | 342 µs | Bevy's sparse set architecture excels here |

### 4. Structural Operation Performance

| Workload | Sky | hecs | Bevy | Flecs | Sky Advantage |
|----------|-----|------|------|-------|---------------|
| `spawn_despawn_1k` | **26.3 µs** | 25.2 µs | 59.3 µs | 164.6 µs | On par with hecs, **2.3x** faster than Bevy |
| `add_remove_component_1k` | **58.8 µs** | 59.2 µs | 88.7 µs | 124.8 µs | On par with hecs |

### 5. Mixed Frame Simulation (Simulating Real Game Loop)

| Workload | Sky | hecs | Bevy | Flecs | Overall Advantage |
|----------|-----|------|------|-------|-------------------|
| `frame` (Full Tick) | **181 µs** | 211 µs | 224 µs | 220 µs | **14–19%** lead |

#### Frame Phase Breakdown

| Phase | Sky | hecs | Bevy | Flecs | Key Insight |
|-------|-----|------|------|-------|-------------|
| `movement` (Movement System) | 4.93 µs | 13.5 µs | 19.8 µs | 5.75 µs | Chunk-columnar storage advantage is significant |
| `health` (Health System) | 3.62 µs | 15.2 µs | 38.6 µs | 5.15 µs | Leading in iteration-intensive tasks |
| `heavy` (Heavy Computation) | 151 µs | 162 µs | 165 µs | 150 µs | All engines converge |
| `random_access` (Random Addressing) | 3.63 µs | 7.33 µs | 1.57 µs | 17.4 µs | Bevy sparse set advantage |
| `structural_churn` (Structural Changes) | 14.4 µs | 14.6 µs | 18.8 µs | 31.4 µs | On par with hecs |
| `spawn_despawn` (Entity Lifecycle) | 54.3 µs | 51.4 µs | 108–248 µs | 332 µs | Significantly better than Bevy/Flecs |

### 6. World-Bound Query API (2026-07-11)

Local measurements over 100K matching entities. These numbers validate API/codegen overhead and are not cross-engine results.

| Workload | Median | Interpretation |
|----------|--------|----------------|
| `world_cache_hit` | **13.05 ns** | Recreating a bound query and resolving its cached archetype plan is effectively constant-time |
| `bound_tuple_for_each` | **101.38 µs** | World-bound tuple query |
| `bound_named_for_each` | **100.62 µs** | Derived `QueryData` item; no measurable abstraction penalty |
| `prepared_tuple_for_each` | **101.56 µs** | Persistent explicit `PreparedQuery`; within measurement noise of the bound facade |

### 7. Parallel Query API (2026-07-11)

Local measurements over 1M matching entities. Chunks are split into cached 4096-entity stripes; small workloads automatically stay sequential.

| Workload | Median | Throughput | Interpretation |
|----------|--------|------------|----------------|
| `bound_tuple_for_each_sequential` | **2.516 ms** | **397 Melem/s** | Same entity update through the sequential facade |
| `bound_tuple_par_for_each` | **~0.219 ms** | **~4.57 Gelem/s** | Ergonomic entity-level parallel path; about **11.5x** faster locally |
| `bound_named_par_for_each` | **~0.220 ms** | **~4.56 Gelem/s** | Direct derived `QueryData` item construction; no tuple-adapter penalty |
| `bound_tuple_par_for_each_chunk` | **0.17–0.29 ms** | **3.5–5.9 Gelem/s** | Expert slice path; sensitive to OS/Rayon scheduling in this short workload |

### 8. Typed Parallel System Schedule (2026-07-11)

Local release-profile measurements. Dispatch cases use cached compiled graphs and reused command buffers.

| Workload | Median | Throughput | Interpretation |
|----------|--------|------------|----------------|
| `empty_tick` | **43.73 ns** | — | Full empty schedule/timing/report path |
| `two_tiny_compatible_systems` | **64.56 ns** | — | Default tiny-wave fallback keeps a compatible pair serial and avoids Rayon dispatch |
| `three_conflicting_systems` | **73.38 ns** | — | Three deterministic write-conflict waves; about 9.9 ns per added system beyond the empty tick |
| `four_system_parallel_wave` | **55.91 µs** | **2.34 Gelem/s** | Four disjoint CPU resource systems executed in one Rayon wave |
| `typed_view_for_each_100k` | **104.07 µs** | **961 Melem/s** | Typed `View<(&mut Position, &Velocity)>` system including schedule dispatch |
| `typed_par_view_for_each_1m` | **179.96 µs** | **5.56 Gelem/s** | Explicit `ParView` stripe preparation and entity-parallel system traversal |

### 9. Adaptive Archetype Matching (2026-07-11)

Local release-profile measurements create a fresh `PreparedQuery` each iteration, so they include descriptor construction and a complete archetype scan rather than an epoch-cache hit. Queries with at most two components retain the direct binary-search path; larger queries use the cheaper of suffix binary search and sorted merge. Absolute nanosecond results are sensitive to CPU scheduling and thermal state; the comparisons below were accepted only after direct A/B Criterion runs.

| Workload | Median | Result |
|----------|--------|--------|
| `fresh_query_1_of_8_shapes` | **172.72 ns** | Small-query binary fast path remains intact |
| `fresh_query_8_of_8_shapes` | **284.78 ns** | Mixed early-rejection and full-match shapes |
| `fresh_query_7_dense_matches` | **436.55 ns** | Down from **667.81 ns** with independent binary searches; **30.3%** faster locally |
| `fresh_query_16_dense_matches` | **506.54 ns** | Fixed 16-slot column map; the allocating `SmallVec<[u8; 8]>` version measured **582.01 ns**, about **13%** slower |
| `fresh_query_7_redundant_with_tuple` | **~422–457 ns** | Compiled seven-term AND filter; down from **944.67 ns** with repeated typed binary lookups, about **52–55%** faster |
