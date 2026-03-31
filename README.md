# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[中文文档](README_CN.md)

A chunk-based archetype ECS in Rust. Components of the same type are stored contiguously in memory, producing sequential access patterns that play well with hardware prefetching.

Library, not a framework. No proc macros, no global state, no imposed application structure.

> **Status:** The ECS runtime is functional and benchmarked. Higher-level engine components (renderer, asset pipeline, editor) are not yet implemented.

## Quick Start

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    let entity = world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 2.0 },
    ));

    // Batch insert
    world.spawn_batch((0..10_000).map(|i| {
        (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
    }));

    // Iterate
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // Chunk-level iteration — yields slices for manual vectorization
    query.for_each_chunk(&world, |(positions, velocities)| {
        for (p, v) in positions.iter_mut().zip(velocities.iter()) {
            p.x += v.x * 0.016;
        }
    });

    let pos = world.get::<Position>(entity).unwrap();
    println!("({}, {})", pos.x, pos.y);
}
```

## Features

- **Chunk-columnar storage:** Each archetype is organized into fixed-size chunks with per-type contiguous columns, aligning iteration with hardware prefetch.
- **Batch insert:** `spawn_batch` avoids repeated per-entity lookups, significantly faster than individual inserts at scale.
- **Query caching:** `PreparedQuery` caches matching archetypes; repeated queries skip all setup when archetypes haven't changed.
- **Optional components and filters:** Queries support `Option<&T>` for optional access, and `With<T>` / `Without<T>` for compile-time archetype filtering.

## Performance

Benchmarks are split into two tracks. Methodology, recorded numbers, and historical runs live in [BENCHMARKS.md](BENCHMARKS.md).

- `cargo bench --bench fair` is the canonical apples-to-apples comparison against `hecs` and `bevy_ecs`. It only includes workloads all engines can express, and it builds query/prepared state outside the timed loop for every engine.
- `cargo bench --bench sky` is the project-side regression suite. It keeps Sky-specific hot paths such as chunk iteration, filtered typed queries, and command-buffer paths out of the fair comparison numbers.

Particle simulation example (80,000 concurrent entities):

```sh
cargo run --example particles --release --features demo
```

Run benchmarks:

```sh
cargo bench --bench fair   # canonical cross-engine comparison
cargo bench --bench sky    # Sky regression suite
cargo bench --bench hecs   # hecs reference suite
cargo bench --bench bevy   # bevy reference suite
cargo bench                # all benches
```

## Project Structure

```
src/
├── ecs/
│   ├── archetype.rs    # Archetype definition and builder
│   ├── chunk.rs        # Chunk allocation, columnar storage
│   ├── query/          # Typed queries, filters
│   ├── world.rs        # World storage and entity management
│   ├── bundle.rs       # Component bundle trait
│   ├── system.rs       # System scheduling
│   └── ...
├── reflect/            # Runtime type registry
└── lib.rs
benches/                # Benchmarks
examples/               # Runnable demos
```

## Documentation

- [API Reference](docs/api.md)
- [Benchmark History](BENCHMARKS.md)

## See Also

- [hecs](https://github.com/Ralith/hecs) — minimal archetype ECS
- [Bevy](https://github.com/bevyengine/bevy) — full game engine with plugin ecosystem
- [flecs](https://github.com/SanderMertens/flecs) — feature-rich ECS in C99

## License

MIT ([LICENSE](LICENSE))
