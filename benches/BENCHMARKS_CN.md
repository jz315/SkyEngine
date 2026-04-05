# Benchmark

## 目录
1. [快速开始](#快速开始)
2. [性能概览](#性能概览)
3. [详细测试数据](#详细测试数据)

---

## 快速开始

```bash
# 四引擎公平对比（Sky/hecs/Bevy/Flecs）
cargo bench --bench fair

# 单引擎测试
cargo bench --bench fair -- sky
cargo bench --bench fair -- hecs
cargo bench --bench fair -- bevy
cargo bench --bench fair -- flecs

# 精确运行单个 benchmark
cargo bench --bench fair -- fair_random_access/get/sky --exact

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

---

