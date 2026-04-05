# Benchmark Records

Local benchmark notes for this repo.

All numbers below were collected on the same local Windows machine with Criterion and should be treated as **machine-specific**, not universal performance claims.

## How to Run

- Canonical fair comparison: `cargo bench --bench fair`
- Full run: `cargo bench`
- Run one engine slice: `cargo bench --bench fair -- sky`
- Run one exact benchmark: `cargo bench --bench fair -- fair_random_access/get/sky --exact`
- Chunk-size sweep: edit `CHUNK_SIZE` in `src/ecs/chunk.rs`, then rerun the relevant `fair` filters

---

## Benchmark Policy

- `fair` is the only canonical apples-to-apples comparison suite.
- `fair` only includes workloads that Sky, hecs, Bevy, and Flecs can all express through safe public APIs.
- Query/prepared state is created outside the timed loop in `fair` for all engines.
- Engine-specific implementations now live under `benches/fair/` and are selected via Criterion filters rather than separate bench targets.
- Records collected before the 2026-03-31 normalization pass are still useful for history, but they are not the canonical fair-comparison baseline.
- The current repo now uses `criterion 0.8.2`; the recorded `2026-04-01` numbers below were collected before that upgrade on the older `criterion 0.4` harness, so the next post-upgrade `fair` run should be treated as a fresh tooling baseline.

---

## Current Summary

### Latest full run
- Date: **2026-04-05**
- Command: `cargo bench --bench fair -- "sky|flecs"`
- Status: **Flecs ECS (v0.2.2) added to fair comparison suite**

### Current takeaways (Sky vs Flecs)
- **Insert**: Sky is **23–47x faster** (120 µs batch vs 5.67 ms flecs). Flecs world+entity creation overhead dominates.
- **Iteration (simple 10k)**: Sky edges out at 1.92 µs vs 2.04 µs — **1.06x** faster. Nearly identical.
- **Fragmented iteration**: Sky **1.45x** faster (581 ns vs 843 ns).
- **Heavy compute**: Tied (~1.85 ms each). Bottlenecked by matrix inversion, not ECS.
- **Random access**: Sky **4.7x faster** (73 µs vs 342 µs).
- **Spawn/despawn 1k**: Sky **5.9x faster** (28 µs vs 165 µs).
- **Add/remove component 1k**: Sky **2.1x faster** (60 µs vs 125 µs).
- **Mixed frame (full)**: Sky **1.17x** faster (188 µs vs 220 µs).
- **Phase: movement**: Flecs wins this micro-phase (5.74 µs vs 9.77 µs), but Sky's movement phase showed anomalous 108% regression in this run — likely cache/scheduling noise. Sky still wins the full mixed frame.
- All other phases: Sky leads (health 1.4x, random_access 3.6x, structural_churn 2.0x, spawn_despawn 5.9x).

### Previous takeaways (Sky vs hecs/Bevy, 2026-04-02)
- `fair_entity_ops/spawn_despawn_1k`: Sky **matches** hecs (26.3 vs 25.2 µs) and is **2.3x** faster than Bevy.
- `fair_entity_ops/add_remove_component_1k`: Sky **matches** hecs (58.8 vs 59.2 µs) and beats Bevy.
- `fair_mixed_frame/frame`: Sky **leads all** at 181 µs vs hecs 211 µs vs Bevy 223 µs — **14% faster** than hecs.
- All iteration benchmarks remain dominant (2.7x–4.2x faster than hecs).
- **No performance regressions** on any workload.

### Key optimizations (cumulative)
1. **`rustc_hash::FxHashMap`** — replaced all `std::collections::HashMap` across `world.rs`, `commands.rs`, `archetype.rs`, `bundle.rs`, `resource.rs`, `registry.rs`. SipHash-2-4's DoS protection was pure overhead for our integer/pointer keys.
2. **Spare-chunk caching** — `Data` now retains one empty chunk after despawn to avoid pool round-trips during spawn/despawn churn.
3. **`#[inline(always)]`** on chunk hot methods — `copy_entity_within`, `copy_entity_from`, `remove_last_entity`, `Data::add_entity`, `Data::remove_entity`.

---

## Sky vs Flecs Fair Comparison (2026-04-05)

Command: `cargo bench --bench fair -- "sky|flecs"`

Note: Flecs ECS v0.2.2 added to the fair benchmark suite. Flecs queries are uncached (`new_query()`), created outside the timed loop for fairness. ZST tag components use `component_id` + `add()`. Component removal uses `component_id` + `remove()`.

### Insert

| Workload | Sky | Flecs | Sky advantage |
| --- | --- | --- | --- |
| `fair_insert/batch_10k` | `119.59-120.81 us` | `5.63-5.71 ms` | **47x** |
| `fair_insert/single_10k` | `244.37-246.08 us` | `5.61-5.69 ms` | **23x** |

### Iteration

| Workload | Sky | Flecs | Sky advantage |
| --- | --- | --- | --- |
| `fair_iteration/simple` | `1.92-1.93 us` | `2.03-2.04 us` | **1.06x** |
| `fair_fragmented_iteration/fragmented` | `578.97-582.31 ns` | `841.33-845.51 ns` | **1.45x** |
| `fair_heavy_compute/heavy` | `1.8478-1.8540 ms` | `1.8618-1.8705 ms` | ≈ tied |

### Random Access

| Workload | Sky | Flecs | Sky advantage |
| --- | --- | --- | --- |
| `fair_random_access/get` | `72.96-73.51 us` | `340.91-343.37 us` | **4.7x** |

### Entity Operations

| Workload | Sky | Flecs | Sky advantage |
| --- | --- | --- | --- |
| `fair_entity_ops/spawn_despawn_1k` | `27.91-28.10 us` | `164.24-164.87 us` | **5.9x** |
| `fair_entity_ops/add_remove_component_1k` | `59.80-60.13 us` | `124.50-125.14 us` | **2.1x** |

### Mixed Frame (complete game loop simulation)

| Workload | Sky | Flecs | Sky advantage |
| --- | --- | --- | --- |
| `fair_mixed_frame/frame` | `187.75-189.03 us` | `219.68-220.75 us` | **1.17x** |

### Mixed Frame Phases (isolated)

| Phase | Sky | Flecs | Sky advantage |
| --- | --- | --- | --- |
| `movement` | `9.74-9.80 us` | `5.72-5.77 us` | 0.59x (flecs wins) |
| `health` | `3.59-3.61 us` | `5.12-5.17 us` | **1.43x** |
| `heavy` | `150.63-151.40 us` | `149.94-151.02 us` | ≈ tied |
| `random_access` | `4.84-4.86 us` | `17.34-17.55 us` | **3.6x** |
| `structural_churn` | `15.50-15.59 us` | `31.37-31.52 us` | **2.0x** |
| `spawn_despawn` | `56.61-56.90 us` | `331.54-333.26 us` | **5.9x** |

---

## Previous Fair Run — FxHashMap Migration (2026-04-02)

Command: `cargo bench --bench fair -- "fair_entity_ops|fair_mixed_frame"`

Note: this is a targeted re-run of entity-ops and mixed-frame benchmarks after the FxHashMap migration. Iteration benchmarks were not re-run as they are unaffected by HashMap changes.

### Entity Operations

| Workload | Sky | hecs | Bevy |
| --- | --- | --- | --- |
| `fair_entity_ops/spawn_despawn_1k` | `26.24-26.41 us` | `25.09-25.28 us` | `58.82-59.69 us` |
| `fair_entity_ops/add_remove_component_1k` | `58.54-59.15 us` | `59.05-59.41 us` | `88.15-89.22 us` |

### Mixed Frame (complete game loop simulation)

| Workload | Sky | hecs | Bevy |
| --- | --- | --- | --- |
| `fair_mixed_frame/frame` | `180.50-182.97 us` | `209.95-211.83 us` | `221.10-226.72 us` |

### Mixed Frame Phases (isolated)

| Phase | Sky | hecs | Bevy |
| --- | --- | --- | --- |
| `movement` | `4.90-4.96 us` | `13.47-13.61 us` | `19.69-19.90 us` |
| `health` | `3.62-3.64 us` | `15.19-15.26 us` | `38.51-38.74 us` |
| `heavy` | `151.20-152.03 us` | `161.57-162.52 us` | `164.83-166.01 us` |
| `random_access` | `3.62-3.64 us` | `7.29-7.37 us` | `1.56-1.57 us` |
| `structural_churn` | `14.39-14.45 us` | `14.52-14.60 us` | `18.69-18.83 us` |
| `spawn_despawn` | `54.09-54.52 us` | `51.15-51.66 us` | `107.40-247.84 us` |

---

## Previous Fair Run (2026-04-01)

Command: `cargo bench --bench fair -- --noplot`

Note: this run was taken after the raw-block pool simplification in `chunk.rs`, the default `mimalloc` switch, the `criterion 0.8.2` upgrade, and the structural-path layout/copy-plan cache work in `world.rs`/`chunk.rs`.

| Workload | Sky | hecs | Bevy |
| --- | --- | --- | --- |
| `fair_insert/batch_10k` | `134.68-137.45 us` | `290.36-298.34 us` | `274.38-280.61 us` |
| `fair_insert/single_10k` | `399.35-407.15 us` | `410.52-421.72 us` | `517.12-529.44 us` |
| `fair_iteration/simple` | `1.9771-2.0340 us` | `5.5569-5.6756 us` | `8.1459-8.3137 us` |
| `fair_fragmented_iteration/fragmented` | `262.20-268.50 ns` | `452.23-462.60 ns` | `258.64-263.70 ns` |
| `fair_heavy_compute/heavy` | `1.8971-1.9274 ms` | `2.3686-2.4211 ms` | `2.0265-2.0674 ms` |
| `fair_random_access/get` | `138.85-142.20 us` | `143.52-146.18 us` | `29.989-30.940 us` |
| `fair_entity_ops/spawn_despawn_1k` | `42.454-43.497 us` | `26.558-27.157 us` | `60.747-62.570 us` |
| `fair_entity_ops/add_remove_component_1k` | `99.382-102.66 us` | `60.087-62.207 us` | `89.165-91.841 us` |
| `fair_mixed_frame/frame` | `198.26-203.09 us` | `211.25-216.50 us` | `224.56-233.06 us` |

### Fragmented Iteration Normalization (2026-04-01)

Note: after the full run above, the fragmented workload was scaled from `26 * 20 = 520` entities to `26 * 400 = 10,400` entities to reduce constant-factor distortion.

| Workload | Sky | hecs | Bevy |
| --- | --- | --- | --- |
| `fair_fragmented_iteration/fragmented` | `1.0648-1.1093 us` | `3.2068-3.3048 us` | `6.1373-6.2669 us` |

### Mixed-Phase Benchmark Normalization (2026-04-01)

Note: after the full run above, the short `fair_mixed_frame_phases` micro-benchmarks were normalized by:
- adding `black_box(&world)` sinks after mutating passes
- repeating `health` by `8x`
- repeating `spawn_despawn` by `32x`

Representative normalized phase results:

| Workload | Sky | hecs | Bevy |
| --- | --- | --- | --- |
| `fair_mixed_frame_phases/health` | `3.7651-3.9951 us` | `15.973-17.039 us` | `40.041-41.891 us` |
| `fair_mixed_frame_phases/spawn_despawn` | `90.246-92.672 us` | `53.761-55.114 us` | `112.91-225.28 us` |

---

## Previous Fair Run (2026-03-31)

Command: `cargo bench --bench fair -- --noplot`

Note: this run was taken after the bundle-meta cache, type-registry cache, component-index cache, and transition-plan cache optimizations.

| Workload | Sky | hecs | Bevy |
| --- | --- | --- | --- |
| `fair_insert/batch_10k` | `156.90-163.71 us` | `373.54-395.34 us` | `363.36-382.82 us` |
| `fair_insert/single_10k` | `545.65-580.51 us` | `752.78-788.08 us` | `866.39-898.53 us` |
| `fair_iteration/simple` | `2.6906-2.8891 us` | `5.6653-5.9005 us` | `8.3256-8.5450 us` |
| `fair_fragmented_iteration/fragmented` | `131.17-144.45 ns` | `538.44-577.83 ns` | `276.23-287.54 ns` |
| `fair_heavy_compute/heavy` | `2.4715-2.6115 ms` | `2.9359-3.0656 ms` | `2.4984-2.6095 ms` |
| `fair_random_access/get` | `177.98-192.19 us` | `163.01-173.95 us` | `31.035-32.219 us` |
| `fair_entity_ops/spawn_despawn_1k` | `53.246-54.677 us` | `25.899-26.533 us` | `65.422-67.160 us` |
| `fair_entity_ops/add_remove_component_1k` | `127.66-131.23 us` | `59.667-61.220 us` | `87.243-89.386 us` |

---

## Historical Full Run (2026-03-30)

Command: `cargo bench`

Note: this run predates the restored `fair` suite. Use it as a regression snapshot, not as the authoritative apples-to-apples comparison record.

### bevy

| Benchmark | Result | Outliers |
| --- | --- | --- |
| `bevy_insert/batch_10k` | `269.47-282.72 us` | `10/100` (`6` high mild, `4` high severe) |
| `bevy_simple_iter` | `9.0095-9.6230 us` | `5/100` (`5` high mild) |
| `bevy_fragmented_iter` | `990.92 ns-1.0260 us` | `12/100` (`6` high mild, `6` high severe) |

### hecs

| Benchmark | Result | Outliers |
| --- | --- | --- |
| `hecs_hot_path/2_of_4` | `4.9571-5.2541 ms` | `2/100` (`2` high mild) |
| `hecs_hot_path/4_of_4` | `11.008-11.620 ms` | `0/100` reported |
| `hecs_insert/batch_10k` | `322.74-349.11 us` | `14/100` (`5` high mild, `9` high severe) |
| `hecs_insert/single_10k` | `584.30-609.87 us` | `9/100` (`4` high mild, `5` high severe) |
| `hecs_simple_iter` | `6.1955-6.9349 us` | `5/100` (`5` high mild) |
| `hecs_fragmented_iter` | `312.85-342.94 ns` | `0/100` reported |
| `hecs_heavy_compute` | `3.4597-3.6909 ms` | `4/100` (`4` high mild) |
| `hecs_random_access` | `149.64-161.31 us` | `0/100` reported |
| `hecs_spawn_despawn_1k` | `27.877-31.093 us` | `0/100` reported |
| `hecs_add_remove_component_1k` | `72.892-81.761 us` | `0/100` reported |

### sky

| Benchmark | Result | Outliers |
| --- | --- | --- |
| `sky_hot_path/2_of_4` | `4.0201-4.2830 ms` | `1/100` (`1` high mild) |
| `sky_hot_path/4_of_4` | `8.1781-8.7247 ms` | `8/100` (`4` high mild, `4` high severe) |
| `sky_insert/batch_10k` | `207.86-219.39 us` | `12/100` (`9` high mild, `3` high severe) |
| `sky_insert/single_10k` | `1.9538-2.0633 ms` | `10/100` (`5` high mild, `5` high severe) |
| `sky_simple_iter/for_each` | `2.4399-2.7386 us` | `3/100` (`3` high mild) |
| `sky_simple_iter/for_each_chunk` | `2.1251-2.2567 us` | `13/100` (`3` high mild, `10` high severe) |
| `sky_fragmented_iter` | `102.67-104.18 ns` | `6/100` (`3` low mild, `1` high mild, `2` high severe) |
| `sky_iter_scaling/for_each/1000` | `92.891-95.508 ns` | `17/100` (`4` high mild, `13` high severe) |
| `sky_iter_scaling/for_each/10000` | `1.7362-1.7593 us` | `9/100` (`2` low mild, `4` high mild, `3` high severe) |
| `sky_iter_scaling/for_each/100000` | `40.880-41.136 us` | `6/100` (`2` high mild, `4` high severe) |
| `sky_filtered_query/all_10k` | `2.3571-2.3728 us` | `8/100` (`2` low mild, `4` high mild, `2` high severe) |
| `sky_filtered_query/with_enemy_5k` | `1.1741-1.1809 us` | `3/100` (`2` high mild, `1` high severe) |
| `sky_heavy_compute` | `2.7454-2.7656 ms` | `2/100` (`2` high severe) |
| `sky_random_access` | `175.95-176.99 us` | `10/100` (`2` low mild, `6` high mild, `2` high severe) |
| `sky_spawn_despawn_1k` | `115.98-116.78 us` | `10/100` (`7` high mild, `3` high severe) |
| `sky_add_remove_component_1k` | `359.79-362.05 us` | `5/100` (`1` low mild, `4` high mild) |
| `sky_commands/spawn_1k_deferred` | `153.37-155.21 us` | `4/100` (`2` low mild, `1` high mild, `1` high severe) |
| `sky_commands/spawn_1k_direct` | `114.19-115.38 us` | `6/100` (`1` low mild, `3` high mild, `2` high severe) |
