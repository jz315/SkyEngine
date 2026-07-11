# ECS Benchmark Guide

SkyEngine deliberately uses two benchmark tracks. Their numbers answer different questions and must not be combined in one result table.

## 1. Cross-engine comparison

`cargo compare-ecs` is the canonical Sky/hecs/Bevy/Flecs comparison. Workloads use safe public APIs available in all four engines, and prepared/query state is created outside the timed loop.

```bash
cargo compare-ecs
cargo compare-ecs -- sky
cargo compare-ecs -- fair_random_access/get/sky --exact
```

Third-party ECS dependencies remain isolated in `tools/ecs-comparison`. Sky-specific internals and archetype microbenchmarks do not belong in this suite.

### Historical cross-engine snapshot

The following is a historical snapshot from Windows 11 on an i7-12700F. It is machine- and revision-specific and is retained only as context, not as a current performance claim.

| Workload | Sky | hecs | Bevy | Flecs |
|---|---:|---:|---:|---:|
| batch insert 10k | 120 µs | 294 µs | 277 µs | 5.67 ms |
| simple iteration 10k | 1.93 µs | 5.62 µs | 8.23 µs | 2.04 µs |
| random get 10k | 73 µs | 145 µs | 30 µs | 342 µs |
| spawn/despawn 1k | 26.3 µs | 25.2 µs | 59.3 µs | 164.6 µs |
| mixed frame | 181 µs | 211 µs | 224 µs | 220 µs |

Re-run the suite before citing comparisons; never mix this snapshot with current local microbenchmark data.

## 2. Sky-local hot-path benchmarks

Criterion benches under `benches/` are mechanism-level regression tools:

| Target | Scope |
|---|---|
| `bound_query` | World cache hit and tuple/`QueryData`/`PreparedQuery` traversal overhead |
| `archetype_match` | fresh prepare, filters, cache hit, and incremental refresh |
| `parallel_query` | sequential and parallel query execution, including the bound facade |
| `parallel_job_cache` | parallel job-plan rebuild after structural churn |
| `system_schedule` | typed dispatch, conflict waves, and system parallelism |

```bash
cargo bench --bench bound_query
cargo bench --bench archetype_match
cargo bench --bench parallel_query
cargo bench --bench parallel_job_cache
cargo bench --bench system_schedule
```

`archetype_match` is a separate process target so million-entity parallel workloads cannot thermally bias its sub-microsecond measurements.

## 3. Archetype prepare coverage

The archetype target covers:

- fresh full scans with 1, 2, 8, and 16 required components;
- dense matches, early rejection, and missing optional components;
- prepared-query epoch cache hits;
- one matching or non-matching archetype appended after preparation;
- rebuild after `clear`, and switching to a different `World` with the same epoch;
- single `With`/`Without`, selective filters, redundant and contradictory AND filters, and `Any` fallback.

Incremental cases use `iter_batched`: world mutation happens in setup and only query refresh is timed. These results describe prepare/matching cost, not entity traversal or whole-frame speed.

Historical direct A/B experiments found meaningful improvements from adaptive sorted matching, a fixed component-index map, and compiled AND filters. Their old one-shot absolute nanosecond values were removed because they are not reproducible run records. Re-measure with named baselines for every new decision.

## 4. Reproducible A/B procedure

Use the local driver on a stable machine:

```powershell
pwsh tools/bench-ecs.ps1 -Phase Before -Baseline adaptive-match
# apply the implementation change
pwsh tools/bench-ecs.ps1 -Phase After -Baseline adaptive-match
```

The driver runs each key benchmark ID in a separate process, fixes `RAYON_NUM_THREADS=8`, performs three rounds by default, and cools down between processes. `-IncludeParallel` adds parallel facade cases; `-Only archetype_cache/prepared_epoch_hit` selects individual IDs.

Reports under `target/criterion/` record CPU, OS, Rust version, Git revision/dirty state, timestamp, Criterion baseline names, per-round 95% confidence intervals, and the median of the three round medians.

Accept an optimization only when:

- the target median improves by at least 5%, and at least two of three runs have a 95% comparison interval excluding zero;
- adjacent common paths do not regress consistently by more than 3%; paths below 500 ns use a 5% tolerance;
- the conclusion comes from direct named-baseline A/B data, not absolute values collected at different times.

Absolute times are always machine- and time-specific. Ordinary CI does not enforce performance thresholds.

## 5. Correctness and allocation invariants

The standalone allocator test keeps world/query construction outside the counted region and prepares a 16-component query against 8 and 64 matching archetypes:

```bash
cargo test -p sky_ecs --test query_allocations
```

It rejects allocation growth proportional to matching-archetype count. Internal tests also assert that `ComponentIndexMap` has fixed inline capacity, no `Drop`, and no pointer-sized heap-storage field.

Before merging benchmark or ECS hot-path changes, run:

```bash
cargo test -p sky_ecs
cargo test --features app -- --test-threads=1
cargo clippy --all-targets --features app -- -D warnings
cargo check --examples --features app
cargo bench --no-run
```

Run `cargo compare-ecs` separately as a smoke/regression check; do not place its results beside local archetype microbenchmarks.
