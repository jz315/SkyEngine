# Benchmark

## 目录
1. [快速开始](#快速开始)
2. [性能概览](#性能概览)
3. [详细测试数据](#详细测试数据)

---

## 快速开始

```bash
# 四引擎公平对比（Sky/hecs/Bevy/Flecs）
cargo compare-ecs

# 单引擎测试
cargo compare-ecs -- sky
cargo compare-ecs -- hecs
cargo compare-ecs -- bevy
cargo compare-ecs -- flecs

# 精确运行单个 benchmark
cargo compare-ecs -- fair_random_access/get/sky --exact

# 测量 world-bound query、命名 QueryData 与 PreparedQuery
cargo bench --bench bound_query

# 测量 typed system dispatch、冲突 wave 与 system 级并行
cargo bench --bench system_schedule

# Chunk 大小调优：修改 src/ecs/chunk.rs 中的 CHUNK_SIZE 后重跑
```

---

## 性能概览

| 测试场景 | Sky 性能 | 关键优势 |
|---------|-------------|---------|
| **实体插入** | 🏆 领先 | 比 hecs/Bevy 快 **2.1–2.4x**，比 Flecs 快 **23–47x** |
| **顺序迭代** | 🏆 领先 | 简单迭代比 hecs 快 **2.9x**，比 Bevy 快 **4.3x** |
| **随机访问** | 🥈 次优 | Bevy 凭借稀疏集更快；Sky 比 hecs 快 **2x** |
| **结构变更** | 🏆 并列 | 与 hecs 持平，比 Bevy 快 **2.3x** |
| **完整帧** | 🏆 领先 | 综合领先 hecs **14%**、Bevy **19%**、Flecs **18%** |

---

## 详细测试数据
注：以下测试均在同一台电脑上运行
测试设备: Windows 11, i7-12700F

### 1. 实体插入性能

| 工作负载 | Sky | hecs | Bevy | Flecs | Sky 优势 |
|---------|-----|------|------|-------|---------|
| `batch_10k` (批量插入) | **120 µs** | 294 µs | 277 µs | 5.67 ms | 比 hecs/Bevy 快 **2.1–2.4x**<br>比 Flecs 快 **47x** |
| `single_10k` (单实体插入) | **245 µs** | 416 µs | 523 µs | 5.65 ms | 比 hecs/Bevy 快 **1.7–2.1x**<br>比 Flecs 快 **23x** |

### 2. 顺序迭代性能

| 工作负载 | Sky | hecs | Bevy | Flecs | 备注 |
|---------|-----|------|------|-------|-----|
| `simple` (1万实体) | **1.93 µs** | 5.62 µs | 8.23 µs | 2.04 µs | 比 hecs 快 **2.9x**，比 Bevy 快 **4.3x** |
| `fragmented` (1.04万实体) | **580 ns** | 3.26 µs | 6.20 µs | 843 ns | 碎片化场景优势显著 |
| `heavy_compute` (矩阵求逆) | **1.85 ms** | 2.39 ms | 2.05 ms | 1.87 ms | 计算密集型任务与 Flecs 持平 |

### 3. 随机访问性能

| 工作负载 | Sky | hecs | Bevy | Flecs | 架构说明 |
|---------|-----|------|------|-------|---------|
| `get` (1万次乱序查找) | **73 µs** | 145 µs | **30 µs** | 342 µs | Bevy 的稀疏集架构在此场景占优 |

### 4. 结构操作性能

| 工作负载 | Sky | hecs | Bevy | Flecs | Sky 优势 |
|---------|-----|------|------|-------|---------|
| `spawn_despawn_1k` | **26.3 µs** | 25.2 µs | 59.3 µs | 164.6 µs | 与 hecs 持平，比 Bevy 快 **2.3x** |
| `add_remove_component_1k` | **58.8 µs** | 59.2 µs | 88.7 µs | 124.8 µs | 与 hecs 持平 |

### 5. 混合帧模拟（模拟真实游戏循环）

| 工作负载 | Sky | hecs | Bevy | Flecs | 综合优势 |
|---------|-----|------|------|-------|---------|
| `frame` (完整 tick) | **181 µs** | 211 µs | 224 µs | 220 µs | 领先 **14–19%** |

#### 帧阶段分别模拟

| 阶段 | Sky | hecs | Bevy | Flecs | 备注 |
|-----|-----|------|------|-------|---------|
| `movement` (移动系统) | 4.93 µs | 13.5 µs | 19.8 µs | 5.75 µs | 块列式存储优势显著 |
| `health` (生命系统) | 3.62 µs | 15.2 µs | 38.6 µs | 5.15 µs | 迭代密集型任务领先 |
| `heavy` (重计算) | 151 µs | 162 µs | 165 µs | 150 µs | 各引擎趋于一致 |
| `random_access` (随机寻址) | 3.63 µs | 7.33 µs | 1.57 µs | 17.4 µs | Bevy 稀疏集优势 |
| `structural_churn` (结构变更) | 14.4 µs | 14.6 µs | 18.8 µs | 31.4 µs | 与 hecs 相当 |
| `spawn_despawn` (实体生命周期) | 54.3 µs | 51.4 µs | 108–248 µs | 332 µs | 显著优于 Bevy/Flecs |

### 6. World-Bound Query API（2026-07-11）

本机对 10 万个匹配实体的测量；这里验证 API/codegen 开销，不属于跨引擎比较数据。

| 工作负载 | 中位数 | 结论 |
|---------|--------|------|
| `world_cache_hit` | **13.05 ns** | 重建轻量 bound query 并命中 archetype plan 基本是常数开销 |
| `bound_tuple_for_each` | **101.38 µs** | world-bound tuple query |
| `bound_named_for_each` | **100.62 µs** | `QueryData` 命名查询，没有可测的抽象损耗 |
| `prepared_tuple_for_each` | **101.56 µs** | 持久化 `PreparedQuery`，与 bound facade 的差异处于测量噪声内 |

### 7. 并行 Query API（2026-07-11）

本机对 100 万个匹配实体的测量。Chunk 会切成缓存的 4096 实体 stripe；小工作负载自动保持顺序执行。

| 工作负载 | 中位数 | 吞吐 | 结论 |
|---------|--------|------|------|
| `bound_tuple_for_each_sequential` | **2.516 ms** | **397 Melem/s** | 同一实体更新的顺序基线 |
| `bound_tuple_par_for_each` | **约 0.219 ms** | **约 4.57 Gelem/s** | 易用的实体级并行路径，本机约 **11.5x** 加速 |
| `bound_named_par_for_each` | **约 0.220 ms** | **约 4.56 Gelem/s** | `QueryData` 直接构造具名 item，没有 tuple 适配损耗 |
| `bound_tuple_par_for_each_chunk` | **0.17–0.29 ms** | **3.5–5.9 Gelem/s** | 专家级切片路径；短工作负载对 OS/Rayon 调度较敏感 |

### 8. Typed 并行 System 调度（2026-07-11）

本机 release profile 测量。dispatch 场景复用已编译 access graph 与 command buffer。

| 工作负载 | 中位数 | 吞吐 | 结论 |
|---------|--------|------|------|
| `empty_tick` | **43.73 ns** | — | 完整空 schedule、时间更新与 report 路径 |
| `two_tiny_compatible_systems` | **64.56 ns** | — | compatible 二元 tiny wave 默认顺序执行，避开 Rayon dispatch |
| `three_conflicting_systems` | **73.38 ns** | — | 三个确定性写冲突 wave；相对空 tick 每增加一个 system 约 9.9 ns |
| `four_system_parallel_wave` | **55.91 µs** | **2.34 Gelem/s** | 四个不冲突 CPU resource system 在同一 Rayon wave 执行 |
| `typed_view_for_each_100k` | **104.07 µs** | **961 Melem/s** | 含调度开销的 `View<(&mut Position, &Velocity)>` typed system |
| `typed_par_view_for_each_1m` | **179.96 µs** | **5.56 Gelem/s** | 显式 `ParView` stripe prepare 与实体级并行 system 遍历 |

### 9. 自适应 Archetype 匹配（2026-07-11）

本机 release profile 测量。每轮都创建新的 `PreparedQuery`，因此包含 descriptor 构造和完整 archetype 扫描，不是 epoch cache 命中。至多两个组件的查询保留直接二分快路径；更大的查询按成本选择后缀二分或有序归并。纳秒级绝对值对 CPU 调度和温度较敏感；下列对比只记录直接 A/B Criterion 验证过的变化。

| 工作负载 | 中位数 | 结论 |
|---------|--------|------|
| `fresh_query_1_of_8_shapes` | **172.72 ns** | 小查询保留二分快路径 |
| `fresh_query_8_of_8_shapes` | **284.78 ns** | 混合提前拒绝与完整匹配的 shape |
| `fresh_query_7_dense_matches` | **436.55 ns** | 独立二分为 **667.81 ns**；本机提升 **30.3%** |
| `fresh_query_16_dense_matches` | **506.54 ns** | 固定 16-slot 列索引 map；会分配的 `SmallVec<[u8; 8]>` 版本为 **582.01 ns**，慢约 **13%** |
| `fresh_query_7_redundant_with_tuple` | **约 422～457 ns** | 七项 AND filter 编译计划；重复 typed 二分版本为 **944.67 ns**，提升约 **52～55%** |

---
