# Benchmark Records

Local benchmark notes for this repo. All numbers below were collected on the same local Windows machine with Criterion and should be treated as machine-specific.

## Commands

- Direct project comparison: `cargo bench --bench hevy --bench sky -- --noplot`
- Suite comparison: `cargo bench --bench suite -- --noplot`
- Chunk-size sweep: edit `CHUNK_SIZE` in `src/ecs/chunk.rs`, then run `cargo bench --bench sky -- --noplot`

## Current Fair Results

### Project Benchmarks

Command: `cargo bench --bench hevy --bench sky -- --noplot`

| Benchmark | Result |
| --- | --- |
| `hecs_2_of_4` | `4.4705-4.6190 ms` |
| `sky_2_of_4` | `3.1541-3.2411 ms` |
| `hecs_4_of_4` | `9.5069-9.6966 ms` |
| `sky_4_of_4` | `6.1364-6.3277 ms` |

Interpretation:

- `sky_2_of_4` is about `1.4x` faster than `hecs_2_of_4`.
- `sky_4_of_4` is about `1.5x` faster than `hecs_4_of_4`.

### ecs_bench_suite-Matched Results

Command: `cargo bench --bench suite -- --noplot`

Fairness adjustments used in `benches/suite.rs`:

- `simple_insert`: Sky writes real component values after insertion, not just archetype allocation.
- `heavy_compute`: `hecs` uses a single-threaded loop instead of the original parallel `rayon` path.

| Benchmark | Sky | hecs |
| --- | --- | --- |
| `simple_insert` | `245.83-250.82 us` | `322.68-331.04 us` |
| `simple_iter` | `2.2224-2.2621 us` | `5.4825-5.6137 us` |
| `fragmented_iter` | `95.647-98.562 ns` | `1.1601-1.1845 us` |
| `heavy_compute` | `2.9745-3.0217 ms` | `2.9849-3.0271 ms` |

Interpretation:

- `simple_insert`: Sky is about `25%` faster.
- `simple_iter`: Sky is about `2.5x` faster.
- `fragmented_iter`: Sky is about `10x+` faster.
- `heavy_compute`: effectively tied in the single-threaded version.

## Chunk Size Sweep

Benchmark target: `sky_2_of_4` and `sky_4_of_4`

### First Full Sweep

| `CHUNK_SIZE` | `sky_2_of_4` | `sky_4_of_4` |
| --- | --- | --- |
| `32KB` | `4.3485 ms` | `7.5325 ms` |
| `48KB` | `3.9788 ms` | `7.4081 ms` |
| `64KB` | `3.8691 ms` | `7.2602 ms` |
| `128KB` | `3.6572 ms` | `6.9387 ms` |
| `256KB` | `3.4613 ms` | `7.0520 ms` |
| `512KB` | `4.0563 ms` | `6.7732 ms` |
| `1MB` | `3.3318 ms` | `7.0734 ms` |

Note: `512KB` looked noisy in `2_of_4`, so the large-size candidates were rerun.

### Focused 3-Run Averages

| `CHUNK_SIZE` | Avg `sky_2_of_4` | Avg `sky_4_of_4` |
| --- | --- | --- |
| `128KB` | `3.6678 ms` | `6.8641 ms` |
| `256KB` | `3.4973 ms` | `7.0024 ms` |
| `512KB` | `3.1358 ms` | `6.6173 ms` |
| `1MB` | `3.2882 ms` | `6.8367 ms` |

Reference:

- `hecs_2_of_4` during this round was about `4.3218 ms`.

Conclusion:

- `512KB` was the best balance for the current workload.
- `1MB` stayed acceptable for `2_of_4`, but started regressing for `4_of_4`.

## Historical Milestones

### Typed Fast Path Initial Result

After switching the hot path to typed chunk slices:

| Benchmark | Result |
| --- | --- |
| `sky_2_of_4` | `5.15-5.49 ms` |
| `hecs_2_of_4` | `5.23-5.56 ms` |

Interpretation:

- This was the first run where Sky and hecs were effectively tied.

### Repeated 4-Run Check

To check whether the tie was real, the same benchmark was rerun four times in alternating order.

Per-run means:

- `sky`: `4.1078`, `4.1463`, `4.1109`, `4.1327 ms`
- `hecs`: `4.7983`, `4.8004`, `4.2823`, `5.0034 ms`

Averages:

- `sky`: `4.1244 ms`
- `hecs`: `4.7211 ms`

Interpretation:

- Sky was ahead in all four runs.
- Average lead was about `12.6%`.

### Post-512KB Optimization Check

Later, after keeping `CHUNK_SIZE = 512KB` and related query-path tuning:

| Benchmark | Result |
| --- | --- |
| `hecs_2_of_4` | `4.79-4.94 ms` |
| `sky_2_of_4` | `2.91-3.06 ms` |
| `sky_4_of_4` | `6.41-6.77 ms` |

## Archived Non-Final Results

These are kept for history, but should not be treated as the current fair conclusion.

### Early suite run before fairness fixes

Command: `cargo bench --bench suite -- --noplot`

At that point:

- `simple_insert` used structure-only insert on the Sky side.
- `heavy_compute` kept the original `hecs` parallel implementation.

| Benchmark | Sky | hecs |
| --- | --- | --- |
| `simple_insert` | `128.53-131.10 us` | `311.55-319.00 us` |
| `simple_iter` | `1.9869-2.0312 us` | `5.4437-5.5683 us` |
| `fragmented_iter` | `157.41-159.96 ns` | `1.1558-1.1758 us` |
| `heavy_compute` | `2.9930-3.0420 ms` | `403.30-412.93 us` |

### Query Overhead Probe

These temporary benchmarks were added only for diagnosis and later removed from `benches/sky.rs`.

| Benchmark | Result |
| --- | --- |
| `sky_2_of_4_query_overhead` | `449.21-460.41 ns` |
| `sky_4_of_4_query_overhead` | `818.13-842.13 ns` |
| `sky_2_of_4_stream_only` | `4.0841-4.2118 ms` |
| `sky_4_of_4_stream_only` | `7.0897-7.3507 ms` |

Interpretation:

- Prepared-query dispatch overhead was negligible.
- The real cost remained in the inner streaming loops.
