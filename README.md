# SkyEngine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

SkyEngine is a data-oriented game engine prototype built in Rust, centered around a custom ECS with chunk-based columnar storage. It's a library, not a framework — organize your game however you like.

The ECS is designed around fixed-size 512KB chunks where component data is laid out column-by-column, giving the CPU prefetcher predictable sequential access patterns. Queries cache their archetype matches and only refresh when the world changes.

### Example

```rust
use sky_engine::ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

let mut world = World::new();

let e = world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 2.0 }));

let mut query = world.query::<(&mut Position, &Velocity)>();
query.for_each(&world, |(pos, vel)| {
    pos.x += vel.x;
    pos.y += vel.y;
});

assert_eq!(world.get::<Position>(e).unwrap().x, 1.0);
```

Batch operations avoid per-entity overhead by pre-computing column offsets:

```rust
world.spawn_batch((0..10_000).map(|i| {
    (Position { x: i as f32, y: 0.0 }, Velocity { x: 1.0, y: 1.0 })
}));
```

For maximum throughput, queries also expose chunk-level slice access:

```rust
query.for_each_chunk(&world, |(positions, velocities)| {
    for (p, v) in positions.iter_mut().zip(velocities.iter()) {
        p.x += v.x;
    }
});
```

### Design Goals

* **Fast iteration**: Columnar chunk layout for cache-friendly traversal
* **Fast insertion**: Pre-computed column offsets, zero hash lookups in the hot path
* **Small surface**: Core ECS is under 2,000 lines of Rust
* **No magic**: No proc macros, no global state, no implicit parallelism

### Performance

Criterion benchmarks against [hecs](https://github.com/Ralith/hecs) and [Bevy ECS](https://github.com/bevyengine/bevy) are included under `benches/`. On the author's machine (same workload, same entity count, single-threaded):

* Iteration throughput is roughly 3-5x that of Bevy and 2-3x that of hecs at moderate entity counts. At millions of entities both Sky and hecs approach memory bandwidth limits with Sky maintaining a ~10-15% lead.
* Fragmented iteration across many archetypes is where chunk storage helps most — roughly 10x faster than Bevy, 2x faster than hecs.
* Batch insertion is about 2x faster than both after the `write_fast` optimization.

Run them yourself:

```sh
cargo bench
```

A particle simulation example spawns up to 1M entities at 80 FPS on a single thread:

```sh
cargo run --example particles --release --features demo
```

### Docs

* **[API Reference](docs/api.md)** — full API documentation
* **[Benchmark History](BENCHMARKS.md)** — detailed benchmark records and chunk-size sweep data
* **[Examples](examples/)** — runnable demos

### Other Libraries

SkyEngine's ECS draws inspiration from the Rust ECS ecosystem. If it doesn't fit your needs, consider:

- [bevy](https://github.com/bevyengine/bevy) — batteries-included engine with a mature plugin ecosystem
- [hecs](https://github.com/Ralith/hecs) — minimal, high-quality archetype ECS library
- [flecs](https://github.com/SanderMertens/flecs) — feature-rich C/C++ ECS with Rust bindings

### License

MIT ([LICENSE](LICENSE))
