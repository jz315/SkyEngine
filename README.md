# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## What is SkyEngine?

SkyEngine is a game engine prototype built in Rust, with an Entity Component System (ECS) at its core. It stores component data in fixed-size 512KB chunks using a columnar layout — each component type occupies a contiguous column within a chunk, rather than interleaving different components per entity. This design maps directly to how modern CPUs access memory: sequential, predictable, and prefetcher-friendly.

SkyEngine is a library, not a framework. There is no implicit global state, no proc macros, and no required application structure. You create a `World`, spawn entities, query components, and build your game on top.

The project is in early development. The ECS runtime is functional and benchmarked, but there is no renderer, no asset pipeline, and no editor. Contributions and feedback are welcome.

## Why SkyEngine?

- **Chunk-based columnar storage.** Component data is packed by type within 512KB chunks, giving the CPU prefetcher sequential access patterns. This is the primary reason iteration is fast, especially across fragmented archetypes.

- **Zero overhead batch insert.** `spawn_batch` pre-computes column offsets once before the loop. The inner write is raw pointer arithmetic — no hash lookups, no binary search, no type registration per entity.

- **Epoch-based query caching.** `PreparedQuery` caches its archetype match list and only re-scans when the world's archetype set changes. Repeated queries in a game loop pay near-zero setup cost.

- **Small codebase.** The entire ECS core is under 2,000 lines of Rust. No proc macros, no code generation, no runtime reflection beyond a minimal type registry.

## Quick Start

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    // Spawn a single entity
    let entity = world.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 2.0 },
    ));

    // Spawn 10,000 entities in batch
    world.spawn_batch((0..10_000).map(|i| {
        (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
    }));

    // Per-entity query
    let mut query = world.query::<(&mut Position, &Velocity)>();
    query.for_each(&world, |(pos, vel)| {
        pos.x += vel.x * 0.016;
        pos.y += vel.y * 0.016;
    });

    // Chunk-level query for SIMD-friendly inner loops
    query.for_each_chunk(&world, |(positions, velocities)| {
        for (p, v) in positions.iter_mut().zip(velocities.iter()) {
            p.x += v.x * 0.016;
        }
    });

    // Random access
    let pos = world.get::<Position>(entity).unwrap();
    println!("({}, {})", pos.x, pos.y);
}
```

## Performance

Benchmarks are Criterion-based and located under `benches/`. All numbers below were collected on the same machine, same workload, single-threaded, comparing against [hecs](https://github.com/Ralith/hecs) and [Bevy ECS](https://github.com/bevyengine/bevy).

### Iteration

At moderate entity counts (10k entities, 4 components, querying 2), SkyEngine's chunk layout provides roughly 3x the throughput of hecs and 5x that of Bevy ECS. The advantage is most pronounced in fragmented iteration across many archetypes — approximately 2x faster than hecs and 10x faster than Bevy.

At large scale (5M entities), both SkyEngine and hecs approach memory bandwidth limits. SkyEngine maintains a 10-15% lead in this regime.

### Insertion

`spawn_batch` with 10,000 entities runs in ~135µs, compared to ~282µs for hecs and ~305µs for Bevy. The speedup comes from pre-computing column offsets before the loop so each entity write is a direct `ptr::write` with no lookups.

### Stress test

A particle simulation example handles 1,000,000 entities at 80 FPS on a single thread, with physics and pixel-buffer rendering:

```sh
cargo run --example particles --release --features demo
```

### Run benchmarks

```sh
cargo bench                           # everything
cargo bench --bench iter              # iteration only
cargo bench --bench insert            # insertion only
cargo bench --bench sky --bench hevy  # 5M head-to-head
```

See [BENCHMARKS.md](BENCHMARKS.md) for historical data and chunk-size sweep results.

## Documentation

- [API Reference](docs/api.md) — full API documentation
- [BENCHMARKS.md](BENCHMARKS.md) — detailed benchmark records

## Related Projects

- [bevy](https://github.com/bevyengine/bevy) — batteries-included game engine with a mature plugin ecosystem
- [hecs](https://github.com/Ralith/hecs) — minimal, high-quality archetype ECS
- [flecs](https://github.com/SanderMertens/flecs) — feature-rich ECS in C99 with Rust bindings

## License

MIT ([LICENSE](LICENSE))
