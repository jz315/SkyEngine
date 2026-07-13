# ECS Benchmark 指南

SkyEngine 刻意保留两条 benchmark 轨道。它们回答的问题不同，结果不能混在同一张表中。

## 1. 跨引擎公平对比

`cargo compare-ecs` 是 Sky/hecs/Bevy/Flecs 的唯一标准对比。workload 只使用四个引擎都能表达的安全公共 API，query/prepared 状态在计时区间外创建。

```bash
cargo compare-ecs
cargo compare-ecs -- sky
cargo compare-ecs -- fair_random_access/get/sky --exact
```

第三方 ECS 依赖只放在 `tools/ecs-comparison`。Sky 专属内部机制和 archetype 微基准不得混入此套件。

### 历史跨引擎快照

下表来自 Windows 11、i7-12700F 的一次历史记录，只用于背景参考，不代表当前版本承诺。

| Workload | Sky | hecs | Bevy | Flecs |
|---|---:|---:|---:|---:|
| 批量插入 1 万 | 120 µs | 294 µs | 277 µs | 5.67 ms |
| 简单遍历 1 万 | 1.93 µs | 5.62 µs | 8.23 µs | 2.04 µs |
| 随机访问 1 万 | 73 µs | 145 µs | 30 µs | 342 µs |
| spawn/despawn 1 千 | 26.3 µs | 25.2 µs | 59.3 µs | 164.6 µs |
| 混合帧 | 181 µs | 211 µs | 224 µs | 220 µs |

引用跨引擎结论前必须在当前 revision 重跑，不能把这份历史快照与当前本地微基准拼表。

## 2. Sky 本地热路径基准

`benches/` 下的 Criterion targets 用于机制级回归。源码按 `ecs/`、`math/` 与 feature-gated `ui/` 分组；Cargo target 名称保持不变：

| Target | 范围 |
|---|---|
| `bound_query` | World cache hit，以及 tuple/`QueryData`/`PreparedQuery` 遍历开销 |
| `archetype_match` | 首次 prepare、filter、cache hit、增量 refresh |
| `parallel_query` | 顺序/并行 query，包括 bound facade |
| `parallel_job_cache` | 结构变更后的并行 job plan 重建 |
| `system_schedule` | typed dispatch、冲突 wave、system 并行 |

```bash
cargo bench --bench bound_query
cargo bench --bench archetype_match
cargo bench --bench parallel_query
cargo bench --bench parallel_job_cache
cargo bench --bench system_schedule
```

`archetype_match` 使用独立 target/process，避免百万实体并行 workload 的温度和调度状态污染亚微秒匹配测量。

## 3. Archetype prepare 覆盖

独立 target 覆盖：

- 1、2、8、16 个必需组件的 fresh full scan；
- 密集命中、提前拒绝、optional 缺失；
- prepared-query epoch cache hit；
- 单个 matching/non-matching archetype 增量追加；
- `clear` 后重建、相同 epoch 下切换不同 `World`；
- 单 `With`/`Without`、选择性 filter、重复/矛盾 AND、`Any` fallback。

增量场景使用 `iter_batched`：World 变更在 setup 完成，计时区间只包含 query refresh。因此这些数字只描述 prepare/matching，不等价于遍历性能或整帧同比提升。

历史直接 A/B 曾确认自适应有序匹配、固定 component-index map、编译 AND filter 有明显收益。旧的单轮绝对纳秒值已删除，因为它们不是可复现的正式运行记录；新的优化决策必须重跑 named baseline。

## 4. 可复现 A/B 流程

在固定本机环境执行：

```powershell
pwsh tools/bench-ecs.ps1 -Phase Before -Baseline adaptive-match
# 应用实现变更
pwsh tools/bench-ecs.ps1 -Phase After -Baseline adaptive-match
```

驱动会让每个关键 benchmark ID 在独立进程运行，固定 `RAYON_NUM_THREADS=8`，默认 before/after 各三轮，并在进程间冷却。`-IncludeParallel` 加入并行 facade；`-Only archetype_cache/prepared_epoch_hit` 可选择单项。

`target/criterion/` 下的报告记录 CPU、OS、Rust 版本、Git revision/dirty 状态、时间、Criterion baseline 名称、每轮 95% CI，以及三轮中位数的中位数。

接受优化必须同时满足：

- 目标场景中位数至少提升 5%，且至少 2/3 轮的 95% 比较区间排除零；
- 相邻常见路径不得稳定回退超过 3%；低于 500 ns 的路径容忍线为 5%；
- 结论只来自直接 named-baseline A/B，不能比较不同时刻的绝对值。

绝对耗时始终与机器、时间相关。普通 CI 不设置性能阈值。

## 5. 正确性与 allocation invariant

独立 allocator integration test 将 World/query 构造放在计数区间外，并对 8 与 64 个 matching archetype 执行 16-component 首次 prepare：

```bash
cargo test -p sky_ecs --test query_allocations
```

测试拒绝 allocation 随 matching-archetype 数线性增长。内部测试还断言 `ComponentIndexMap` 是固定内联容量、无 `Drop`，且布局不包含指针大小的堆存储字段。

合并 benchmark 或 ECS 热路径改动前运行：

```bash
cargo test -p sky_ecs
cargo test --features app -- --test-threads=1
cargo clippy --all-targets --features app -- -D warnings
cargo check --examples --features app
cargo bench --no-run
```

`cargo compare-ecs` 只作为独立 smoke/regression 运行；结果不要与本地 archetype microbench 混表。
