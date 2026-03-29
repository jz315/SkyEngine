# SkyEngine

A high-performance Entity Component System written in Rust, built on chunk-based columnar storage for maximum cache efficiency.

## ⚡ Performance

Benchmarked against Bevy ECS and Hecs on identical workloads (10k entities, 4 components):

| Benchmark | Sky | Bevy | Hecs | 
|---|---|---|---|
| **Iteration** (2-of-4 query) | **1.7 µs** | 8.7 µs | 5.3 µs |
| **Fragmented** (26 archetypes) | **104 ns** | 989 ns | 215 ns |
| **Batch Insert** (10k entities) | **135 µs** | 305 µs | 282 µs |

At large scale (5M entities), Sky maintains a consistent **10-15% lead** over Hecs on head-to-head iteration, with both implementations approaching memory bandwidth limits.

> All benchmarks run on the same machine with `cargo bench`. See `benches/` for full source.

## 🏗️ Architecture

### Chunk-Based Columnar Storage

Unlike traditional archetype-table designs, SkyEngine partitions entity data into **fixed-size 512KB chunks** with per-column layout within each chunk:

```
Chunk (512 KB)
┌─────────────────────────────────────────────────┐
│ [Position][Position][Position] ... (column 0)   │
│ [Velocity][Velocity][Velocity] ... (column 1)   │
│ [Health  ][Health  ][Health  ] ... (column 2)   │
└─────────────────────────────────────────────────┘
```

This gives us:
- **CPU cache prefetcher** can predict and preload sequential column data
- **SIMD-friendly** data layout — components densely packed per type
- **Fragmentation-resistant** — many small archetypes still iterate with good locality

### Zero-Overhead Batch Write

`spawn_batch` pre-computes column offsets once, then writes each entity with raw pointer arithmetic — no hash lookups, no binary search, no type registration in the hot loop.

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

    // Query iteration
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // Chunk-level access for max throughput
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
| Typed `PreparedQuery` with caching | ✅ |
| `With<T>` / `Without<T>` filters | ✅ |
| `spawn` / `spawn_batch` / `despawn` | ✅ |
| `insert` / `remove` component | ✅ |
| `get` / `get_mut` random access | ✅ |
| Deferred `Commands` | ✅ |
| Resource storage | ✅ |
| Group-based scheduling | ✅ |
| Fixed-timestep groups | ✅ |
| `System` trait with lifecycle hooks | ✅ |
| Dynamic query (scripting/tooling path) | ✅ |

## 🔧 Scheduling

```rust
let mut world = World::new();

// Groups execute in creation order
world.group("input").add(InputSystem);
world.group("physics").fixed(0.02).add(PhysicsSystem);
world.group("gameplay").add(|world: &mut World| {
    // Closure systems work too
    let dt = world.time.delta;
});
world.group("render").add(RenderSystem);

// Run one frame
world.tick();

// Cleanup
world.shutdown();
```

## 📦 Project Structure

```
src/ecs/
├── world.rs       # World — primary API surface
├── chunk.rs       # 512KB chunk allocation and column storage
├── archetype.rs   # Archetype definition and caching
├── bundle.rs      # Bundle trait and write_fast optimization
├── query/         # Typed + dynamic query implementations
├── system.rs      # System trait, Schedule, GroupBuilder
├── time.rs        # Time management
├── commands.rs    # Deferred command buffer
├── entity.rs      # EntityId with generational indexing
└── resource.rs    # Type-erased resource storage

benches/
├── common.rs      # Shared components and helpers
├── insert.rs      # Insert benchmarks (Sky vs Bevy vs Hecs)
├── iter.rs        # Iteration benchmarks
└── entity.rs      # Entity lifecycle benchmarks
```

## 🧪 Running

```bash
cargo test                    # Run all tests
cargo bench --bench iter      # Iteration benchmarks
cargo bench --bench insert    # Insert benchmarks
cargo bench --bench entity    # Entity lifecycle benchmarks
cargo bench                   # Run everything
```

## License

MIT
