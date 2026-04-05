

# Benchmark

## Table of Contents
1. [Quick Start](#quick-start)
2. [Performance Overview](#performance-overview)
3. [Detailed Test Data](#detailed-test-data)

---

## Quick Start

```bash
# Fair comparison of four engines (Sky/hecs/Bevy/Flecs)
cargo bench --bench fair

# Single engine test
cargo bench --bench fair -- sky
cargo bench --bench fair -- hecs
cargo bench --bench fair -- bevy
cargo bench --bench fair -- flecs

# Run a specific benchmark precisely
cargo bench --bench fair -- fair_random_access/get/sky --exact

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