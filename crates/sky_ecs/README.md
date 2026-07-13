# Sky ECS

**Sky ECS is a high-performance, typed, chunk-based Entity Component System for Rust.**

It is built for the part of your game or simulation that cannot afford to be
casual: hot iteration loops, large worlds, predictable frame time, and systems
that scale across CPU cores without giving up a direct API.

> ## Performance first
>
> Sky ECS is **the fastest Rust ECS in our fair, safe public-API benchmark
> suite**. In the repository's current Criterion snapshot, Sky leads
> the tested `hecs`, `bevy_ecs`, and `flecs_ecs` implementations in simple
> iteration, bulk insertion, and mixed-frame simulation.
>
> Performance is workload-, hardware-, compiler-, and revision-dependent.
> Treat that as a strong, reproducible benchmark claim—not an unverifiable
> universal promise. Run the included comparison suite on your target machine.

## Why Sky ECS

- **Chunk-columnar storage** keeps component data dense and iteration friendly.
- **Typed, world-bound queries** cache matching archetype plans and refresh only
  when structure changes.
- **Parallel by default when it pays off**: cached stripe jobs use Rayon and
  automatically stay serial below the useful threshold.
- **Ergonomic systems without hidden cost**: typed `View`, `ParView`, `Res`,
  `ResMut`, and `Commands` infer access and form deterministic compatible waves.
- **Structural changes made for real engines**: generational entities, cached
  transitions, deferred/coalesced commands, correct non-`Copy` drop behavior.
- **Escape hatches with clear boundaries**: `dynamic` supports tools and
  scripting; `expert` exposes low-level internals deliberately, without
  compromising the typed hot path.

## Start here

```toml
[dependencies]
sky_ecs = "0.1"
```

```rust
use sky_ecs::World;

#[derive(Clone, Copy)]
struct Position { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Velocity { x: f32, y: f32 }

fn main() {
    let mut world = World::new();

    world.spawn_batch((0..10_000).map(|i| (
        Position { x: i as f32, y: 0.0 },
        Velocity { x: 80.0, y: 30.0 },
    )));

    world
        .query_mut::<(&mut Position, &Velocity)>()
        .for_each(|(position, velocity)| {
            position.x += velocity.x / 60.0;
            position.y += velocity.y / 60.0;
        });
}
```

For large, CPU-heavy worlds, keep the same query shape and switch to parallel
iteration:

```rust
world
    .query_mut::<(&mut Position, &Velocity)>()
    .par_for_each(|(position, velocity)| {
        position.x += velocity.x / 60.0;
        position.y += velocity.y / 60.0;
    });
```

## The performance model

Sky ECS is an archetype ECS with columnar chunk storage. Entities with the same
component set share chunks, so a typed query can walk contiguous component
columns rather than chasing a pointer graph.

For the fast path, prefer:

- `world.spawn((A, B, ...))` and `spawn_batch(...)` for construction;
- `world.query::<Q>()` / `world.query_mut::<Q>()` for normal iteration;
- `for_each_chunk` or `par_for_each_chunk` when slices help your inner loop;
- `PreparedQuery` only when an explicit reusable plan is truly needed;
- stable archetypes during the hottest phase of a frame.

Frequent add/remove component operations necessarily migrate entities between
archetypes. Sky caches those transitions, but workloads that churn structure
constantly should still batch or defer changes through `Commands`.

## Queries that stay readable

Named query items make wide queries self-documenting, with no runtime
reflection or dynamic dispatch in the typed path.

```rust
use sky_ecs::{QueryData, World};

#[derive(QueryData)]
struct Movement<'w> {
    position: &'w mut Position,
    velocity: &'w Velocity,
}

fn advance(world: &mut World) {
    world.query_mut::<Movement>().for_each(|body| {
        body.position.x += body.velocity.x;
        body.position.y += body.velocity.y;
    });
}
```

Queries support optional components and compile-time filters:

```rust
use sky_ecs::{Any, With, Without};

let query = world
    .query::<(&Position, Option<&Velocity>)>()
    .filter::<(With<Player>, Without<Disabled>)>();

let visible_or_selected = world
    .query::<&Position>()
    .filter::<Any<(With<Visible>, With<Selected>)>>();
```

## Systems and scheduling

Install typed systems directly on stages. Access is inferred from parameters;
compatible systems can run in deterministic parallel waves, while exclusive
systems remain explicit barriers.

```rust
use sky_ecs::{ParView, Res, Time, Update, World};

fn movement(mut bodies: ParView<(&mut Position, &Velocity)>, time: Res<Time>) {
    bodies.par_for_each(|(position, velocity)| {
        position.x += velocity.x * time.delta;
        position.y += velocity.y * time.delta;
    });
}

let mut world = World::new();
world.stage(Update).add(movement);
world.tick_with_delta(1.0 / 60.0).unwrap();
```

Deferred structural work belongs in `Commands`; command application coalesces
multiple changes to an entity and preserves first-seen entity order.

## Benchmarks: prove it on your hardware

The repository ships two deliberately separate benchmark tracks:

1. `cargo compare-ecs` is the canonical fair comparison against `hecs`,
   `bevy_ecs`, and `flecs_ecs`. Every workload uses safe public APIs that all
   four can express, and query/prepared state is created outside the timed loop.
2. `cargo bench` tracks Sky ECS's internal hot paths and protects against
   regressions. Its results must not be presented as a cross-engine comparison.

From a checkout of [SkyEngine](https://github.com/jz315/SkyEngine):

```bash
cargo compare-ecs
cargo compare-ecs -- fair_iteration/simple/sky --exact
```

The recorded historical snapshot, methodology, and citation rules live in
[`benches/BENCHMARKS.md`](../../benches/BENCHMARKS.md). Re-run before publishing
new numbers: benchmark results are evidence, not a permanent entitlement.

## API map

| Need | Use |
| --- | --- |
| Normal read/write iteration | `World::query` / `World::query_mut` |
| Reusable or cross-world plan | `PreparedQuery` |
| Entity-level parallel work | `par_for_each` |
| Chunk-slice parallel work | `par_for_each_chunk` |
| Filter archetypes | `With<T>`, `Without<T>`, `Any<(...)>` |
| Deferred structural changes | `Commands` or `CommandBuffer` |
| Runtime-known component types | `sky_ecs::dynamic` |
| Low-level archetype/storage work | `sky_ecs::expert` |

## Status and ecosystem

`sky_ecs` is the standalone ECS core of
[SkyEngine](https://github.com/jz315/SkyEngine). It is designed for direct use,
while the engine re-exports it as `sky_engine::ecs`.

The crate is actively evolving. If you depend on it directly, pin a compatible
version and follow release notes for API changes.

## License

MIT. See the repository [LICENSE](../../LICENSE).
