# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Chunk-based columnar ECS in Rust. Fast iteration, fast insertion, small codebase.

## Benchmarks

Against Bevy ECS and Hecs, same workload, same machine:

| | SkyEngine | Hecs | Bevy |
|---|---|---|---|
| iter 2-of-4 (10k) | **1.7 µs** | 5.3 µs | 8.7 µs |
| fragmented (26 archetypes) | **104 ns** | 215 ns | 989 ns |
| batch insert (10k) | **135 µs** | 282 µs | 305 µs |

At 5M entities both Sky and Hecs hit memory bandwidth, Sky still leads ~10-15%.

Particle stress test: **1M entities, 80 FPS**, single thread.

## Why it's fast

512KB chunks, columns packed per component type. CPU prefetcher loves sequential access. No hash maps in the hot path — query caches archetype matches, batch insert pre-computes column offsets.

```
Chunk layout:
[Pos Pos Pos Pos ...][Vel Vel Vel Vel ...][Hp Hp Hp Hp ...]
       column 0             column 1            column 2
```

## Usage

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Pos { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Vel { x: f32, y: f32 }

let mut world = World::new();

world.spawn((Pos { x: 0.0, y: 0.0 }, Vel { x: 1.0, y: 0.0 }));

world.spawn_batch((0..10_000).map(|i| {
    (Pos { x: i as f32, y: 0.0 }, Vel { x: 1.0, y: 1.0 })
}));

let mut q = world.query::<(&mut Pos, &Vel)>();
q.for_each(&world, |(pos, vel)| {
    pos.x += vel.x * 0.016;
});

// chunk-level for SIMD-friendly loops
q.for_each_chunk(&world, |(positions, velocities)| {
    for (p, v) in positions.iter_mut().zip(velocities.iter()) {
        p.x += v.x * 0.016;
    }
});
```

## What's in the box

- Typed `PreparedQuery` with epoch-based cache invalidation
- `With<T>` / `Without<T>` filters, `Option<&T>` optional access
- `spawn` / `spawn_batch` / `despawn` / `insert` / `remove`
- `get` / `get_mut` random access
- Deferred `Commands` buffer
- Resource storage
- Group-based system scheduling with fixed timestep
- `System` trait (init / run / teardown)
- Dynamic query path for tooling/scripting
- Generational entity IDs

## Running

```bash
cargo test
cargo bench
cargo run --example particles --release --features demo
```

## Structure

```
src/ecs/           Core ECS (world, chunk, archetype, query, bundle, system)
src/reflect/       Runtime type registry
benches/           Criterion benchmarks (insert, iter, entity, head-to-head)
examples/          Particle simulation demo
docs/api.md        API reference (中文)
```

## License

MIT
