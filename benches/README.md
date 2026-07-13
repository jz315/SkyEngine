# Benchmark layout

```text
benches/
├─ ecs/     # Sky-local ECS query, matching, parallelism, and schedule hot paths
├─ math/    # math façade and primitive comparisons
├─ ui/      # feature-gated UI workloads
├─ BENCHMARKS.md
└─ BENCHMARKS_CN.md
```

Cargo target names remain stable even when their source files move. Run a
single benchmark with `cargo bench --bench <target>`; see `BENCHMARKS.md` for
measurement policy and the cross-engine suite under `tools/ecs-comparison`.
