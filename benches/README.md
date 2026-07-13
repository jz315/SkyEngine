# Benchmark layout

```text
benches/
├─ math/    # math façade and primitive comparisons
└─ ui/      # feature-gated UI workloads
```

Cargo target names remain stable even when their source files move. Run a
single benchmark with `cargo bench --bench <target>`. ECS hot-path benchmarks,
measurement policy, and the cross-engine comparison suite live in the
[SkyECS repository](https://github.com/jz315/SkyECS).
