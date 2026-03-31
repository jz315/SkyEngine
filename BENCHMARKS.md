# Benchmark Records

Local benchmark notes for this repo.

All numbers below were collected on the same local Windows machine with Criterion and should be treated as **machine-specific**, not universal performance claims.

## How to Run

- Canonical fair comparison: `cargo bench --bench fair`
- Sky regression suite: `cargo bench --bench sky`
- hecs reference suite: `cargo bench --bench hecs`
- bevy reference suite: `cargo bench --bench bevy`
- Full run: `cargo bench`
- Chunk-size sweep: edit `CHUNK_SIZE` in `src/ecs/chunk.rs`, then run `cargo bench --bench sky -- --noplot`

---

## Benchmark Policy

- `fair` is the only canonical apples-to-apples comparison suite.
- `fair` only includes workloads that Sky, hecs, and Bevy can all express through safe public APIs.
- Query/prepared state is created outside the timed loop in `fair` for all engines.
- Sky-specific APIs such as chunk iteration, filtered typed queries, and deferred commands remain in `sky` as project-side regression checks.
- Records collected before the 2026-03-31 normalization pass are still useful for history, but they are not the canonical fair-comparison baseline.

---

## Current Summary

### Latest full run
- Date: **2026-03-31**
- Command: `cargo bench --bench fair -- --noplot`
- Status: **canonical fair-comparison snapshot, post-optimization**

### Current takeaways
- `fair_insert/batch_10k`: Sky is about **2.40x** faster than `hecs` and about **2.33x** faster than Bevy.
- `fair_insert/single_10k`: Sky is now ahead of both `hecs` and Bevy.
- `fair_iteration/simple`: Sky is still fastest, about **2.07x** faster than `hecs` and about **3.03x** faster than Bevy.
- `fair_fragmented_iteration/fragmented`: Sky remains fastest, about **4.05x** faster than `hecs` and about **2.04x** faster than Bevy.
- `fair_heavy_compute/heavy`: Sky and Bevy are now effectively tied, both ahead of `hecs`.
- `fair_entity_ops/spawn_despawn_1k` improved enough for Sky to beat Bevy, while `fair_entity_ops/add_remove_component_1k` remains the main structural-operation gap.
- The older `2026-03-30 cargo bench` record remains below as a pre-normalization regression snapshot.

---

## Latest Fair Run (2026-03-31)

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
