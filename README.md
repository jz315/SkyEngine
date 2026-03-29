# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A high-performance Entity Component System written in Rust, built on **chunk-based columnar storage** for maximum cache efficiency.

## ⚡ Performance

Benchmarked against **Bevy ECS** and **Hecs** on identical workloads:

| Benchmark | SkyEngine | Bevy ECS | Hecs |
|---|---|---|---|
| **Iteration** (2-of-4 query, 10k) | **1.7 µs** | 8.7 µs | 5.3 µs |
| **Fragmented** (26 archetypes) | **104 ns** | 989 ns | 215 ns |
| **Batch Insert** (10k entities) | **135 µs** | 305 µs | 282 µs |

At scale (5M entities), SkyEngine maintains a **10-15% lead** over Hecs, with both approaching memory bandwidth limits.

### Stress Test

**1,000,000 particles at 80 FPS** — physics + pixel rendering on a single thread.

```bash
cargo run --example particles --release
```

## 🏗️ Architecture

### Chunk-Based Columnar Storage

Unlike traditional archetype-table designs, SkyEngine partitions entity data into **fixed-size 512KB chunks** with per-column layout:

```
Chunk (512 KB)
┌─────────────────────────────────────────────────┐
│ [Position][Position][Position] ... (column 0)   │
│ [Velocity][Velocity][Velocity] ... (column 1)   │
│ [Health  ][Health  ][Health  ] ... (column 2)   │
└─────────────────────────────────────────────────┘
```

- **CPU prefetcher** can predict and preload sequential column data
- **SIMD-friendly** — components densely packed per type
- **Fragmentation-resistant** — many small archetypes still iterate with good locality

### Zero-Overhead Batch Insert

`spawn_batch` pre-computes column offsets once, then writes each entity with raw pointer arithmetic — no hash lookups, no binary search in the hot loop.

## 🎮 Quick Start

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    // Spawn entities
    world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 0.0 }));

    // Batch spawn
    world.spawn_batch((0..10_000).map(|i| {
        (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
    }));

    // Query — per-entity
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // Query — chunk-level (max throughput)
    query.for_each_chunk(&world, |(positions, velocities)| {
        for (pos, vel) in positions.iter_mut().zip(velocities.iter()) {
            pos.x += vel.x * 0.016;
            pos.y += vel.y * 0.016;
        }
    });
}
```

## 📋 Features

| Feature | Status |
|---|---|
| Typed `PreparedQuery` with epoch caching | ✅ |
| `With<T>` / `Without<T>` filters | ✅ |
| `Option<&T>` / `Option<&mut T>` in queries | ✅ |
| `spawn` / `spawn_batch` / `despawn` | ✅ |
| `insert` / `remove` component (archetype migration) | ✅ |
| `get` / `get_mut` random access | ✅ |
| Deferred `Commands` buffer | ✅ |
| Type-erased resource storage | ✅ |
| Group-based system scheduling | ✅ |
| Fixed-timestep groups | ✅ |
| `System` trait with init/run/teardown lifecycle | ✅ |
| Dynamic query path (scripting/tooling) | ✅ |
| Generational `EntityId` | ✅ |

## 📦 Project Structure

```
src/
├── ecs/
│   ├── world.rs        # World — primary API surface
│   ├── chunk.rs         # 512KB chunk allocation & column storage
│   ├── archetype.rs     # Archetype definition & caching
│   ├── bundle.rs        # Bundle trait & write_fast optimization
│   ├── query/           # Typed + dynamic query implementations
│   ├── system.rs        # System trait, Schedule, group builder
│   ├── commands.rs      # Deferred command buffer
│   ├── entity.rs        # EntityId with generational indexing
│   ├── resource.rs      # Type-erased resource storage
│   └── time.rs          # Frame time management
├── reflect/             # Runtime type registry
└── lib.rs

benches/
├── common.rs            # Shared components & helpers
├── insert.rs            # Insert benchmarks (Sky vs Bevy vs Hecs)
├── iter.rs              # Iteration benchmarks
├── entity.rs            # Entity lifecycle benchmarks
├── sky.rs               # 5M entity head-to-head (Sky)
└── hevy.rs              # 5M entity head-to-head (Hecs)

examples/
└── particles.rs         # 2D particle simulation demo

docs/
└── api.md               # Full API reference (中文)
```

## 🧪 Running

```bash
# Tests
cargo test

# Benchmarks
cargo bench --bench iter       # Iteration
cargo bench --bench insert     # Insertion
cargo bench --bench entity     # Entity lifecycle
cargo bench --bench sky --bench hevy  # Head-to-head at 5M scale

# Particle demo
cargo run --example particles --release --features demo
```

## License

[MIT](LICENSE)
