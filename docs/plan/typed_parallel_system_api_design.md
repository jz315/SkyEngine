# Sky ECS Typed Parallel System API Design

Status: implemented (2026-07-11)

Implementation notes: all four phases below are present in `sky_ecs`. Function and parameter tuples support 16 arguments; sequential `View` and parallel `ParView` iteration take `&self` and use invocation-local re-entry guards; missing resources use the public `ScheduleError`; overlapping parameters are programmer errors rejected during registration. `World::schedule_diagnostics()` exposes the compiled access graph and wave layout.

Scope: replace the current ordinary `System::run(&mut World)` path with typed system parameters and deterministic stage-local parallel scheduling. Full-world access remains available as an explicit exclusive escape hatch.

## 1. Design goals

The public model should require only these concepts:

- `Stage`: an explicit execution barrier and ordering unit.
- `View<Q, F>` / `ParView<Q, F>`: sequential or explicitly parallel typed ECS component views.
- `Res<T>` / `ResMut<T>`: shared or exclusive resource access.
- `Commands`: a system-local deferred structural writer.
- `Local<T>`: persistent state owned by one system.

The implementation may use access sets, world cells, type-erased runners, conflict graphs, waves, and Rayon jobs. Those concepts must not leak into normal gameplay signatures.

Goals:

- infer component/resource access from function parameters;
- run conflict-free systems concurrently;
- keep conflicting systems in deterministic registration order;
- preserve deterministic deferred-command application;
- retain the existing chunk/stripe query executor;
- provide validated and bounded fixed-timestep execution;
- restore the schedule even when a system panics;
- keep a clearly marked full-`World` escape hatch.

Non-goals for the first implementation:

- arbitrary per-system `before` / `after` DAG syntax;
- automatic rollback of component writes after panic;
- distributing one schedule across multiple processes;
- making non-`Send` components or resources run on worker threads;
- reproducing the full Bevy `SystemParam` surface.

## 2. Canonical user API

### 2.1 Ordinary typed systems

```rust
use sky_engine::ecs::{Commands, Local, ParView, Res, ResMut, Time, View, With};

fn movement(
    entities: ParView<(&mut Position, &Velocity), With<Active>>,
    time: Res<Time>,
) {
    entities.par_for_each(|(position, velocity)| {
        position.x += velocity.x * time.delta;
        position.y += velocity.y * time.delta;
    });
}

fn update_score(
    players: View<&Score, With<Player>>,
    mut total: ResMut<TotalScore>,
) {
    total.0 = 0;
    players.for_each(|score| total.0 += score.0);
}
```

`View` is the sequential system-facing borrowed query facade. `ParView` opts the parameter into serial stripe preparation and parallel iteration. Access mode remains part of `Q`:

```rust
View<&Position>                         // component read
View<&mut Position>                     // component write
View<(&mut Position, &Velocity)>        // mixed component access
View<(&Position, Option<&Velocity>)>     // optional component read
View<&Position, With<Visible>>           // type-level filter
View<Movement>                           // derived QueryData
ParView<Movement>                        // same access, explicit parallel preparation
```

There is no system-param-level `ViewMut` or `query_mut` entry point. The scheduler constructs the view after validating the complete parameter access set; `&mut T` already expresses write capability.

### 2.2 Structural changes

```rust
fn despawn_dead(
    entities: View<&Health>,
    mut commands: Commands,
) {
    entities.for_each_with_entity(|entity, health| {
        if health.value <= 0.0 {
            commands.despawn(entity);
            commands.spawn((Loot::default(), Transform::default()));
        }
    });
}
```

`Commands` never grants direct `&mut World`. Each system writes to its own command buffer. Buffers are applied at a deterministic stage boundary.

For manual command construction outside a schedule, the owned type is named `CommandBuffer`:

```rust
let mut commands = CommandBuffer::new();
commands.spawn((Position::default(),));
commands.apply(&mut world);
```

This deliberately separates the borrowed system writer (`Commands<'_>`) from the owned queue (`CommandBuffer`).

### 2.3 Persistent local state

```rust
#[derive(Default)]
struct SpawnClock {
    remaining: f32,
}

fn spawn_periodically(
    time: Res<Time>,
    mut clock: Local<SpawnClock>,
    mut commands: Commands,
) {
    clock.remaining -= time.delta;
    if clock.remaining <= 0.0 {
        commands.spawn((Enemy::default(),));
        clock.remaining = 1.0;
    }
}
```

`Local<T>` belongs to exactly one registered system, does not participate in access conflicts, and is dropped when that system is removed or the schedule shuts down.

### 2.4 Registration

```rust
use sky_engine::ecs::stage::{FixedUpdate, Last, PostUpdate, Update};

world
    .stage(Update)
    .add(movement)
    .add(update_score)
    .add(despawn_dead);

world
    .stage(PostUpdate)
    .add(sync_presentation);

world
    .stage(Last)
    .add(publish_diagnostics);
```

`add` accepts typed functions and closures whose parameters implement the hidden system-parameter contract. The default diagnostic name is `type_name::<F>()`; names are retained in every build:

```rust
world.stage(Update).add_named("movement", movement);
```

Built-in stages are zero-sized typed labels:

```text
First
FixedUpdate
PreUpdate
Update
PostUpdate
Last
```

Their order is fixed. Plugins that require another barrier may insert a typed stage label explicitly:

```rust
#[derive(StageLabel)]
struct RenderExtract;

world
    .insert_stage_after(PostUpdate, RenderExtract)?
    .add(extract_scene);
```

Stage labels are identified by type, so spelling mistakes cannot silently create a second stage. Custom stages must be installed explicitly; `stage` fails fast and `try_stage` returns `UnknownStage` instead of appending an unknown label after `Last`. Repeated insertion after one anchor preserves call order, while nested inserted stages remain inside their parent's contiguous subtree.

### 2.5 Fixed timestep

```rust
world
    .stage(FixedUpdate)
    .fixed(
        FixedStep::hz(60)
            .max_substeps(8)
            .overflow(FixedOverflow::Drop),
    )?
    .add(integrate_physics)
    .add(resolve_contacts);
```

Contract:

- `FixedUpdate` is fixed at 60 Hz by default;
- the first explicit fixed configuration may replace that default;
- repeating an equivalent explicit configuration is idempotent, while a conflicting later configuration returns `ScheduleBuildError`;
- fixed duration must be finite and greater than zero;
- the accumulator uses `f64` internally;
- `max_substeps` must be non-zero and defaults to 8;
- a stage never executes more than `max_substeps` in one frame;
- `FixedOverflow::Drop` discards excess accumulated time and reports it;
- `FixedOverflow::Carry` retains backlog for a later frame;
- `Time::delta` is the fixed duration during a fixed substep;
- `Time::fixed_alpha` represents built-in `FixedUpdate`; custom fixed-stage backlog remains available in schedule diagnostics.

Dynamic configuration can use checked constructors:

```rust
let step = FixedStep::try_hz(config.physics_hz)?;
```

`FixedStep::hz(0)` is a fail-fast programmer error; it must never enter the execution loop.

### 2.6 Exclusive escape hatch

```rust
world.stage(Update).add_exclusive(|world: &mut World| {
    rebuild_editor_world(world);
});
```

An exclusive system:

- is always a serial barrier;
- flushes preceding stage-local command buffers before it runs;
- may query, mutate resources, or perform structural changes directly;
- invalidates affected query/storage plans through normal `World` APIs;
- prevents later systems from moving into an earlier wave across the barrier.

The current `FnMut(&mut World)` behavior becomes this explicit path rather than the default system model.

Advanced stateful integrations may implement:

```rust
pub trait ExclusiveSystem: 'static {
    fn init(&mut self, _world: &mut World) {}
    fn run(&mut self, world: &mut World);
    fn teardown(&mut self, _world: &mut World) {}
}
```

Ordinary typed functions should use `Local<T>`, resources, normal stages, and Rust `Drop` instead of lifecycle callbacks.

## 3. Public type contracts

### 3.1 View and ParView

Conceptual signature:

```rust
pub struct View<'w, Q, F = ()> {
    // private scheduler-issued storage capability
}

pub struct ParView<'w, Q, F = ()> {
    // private scheduler-issued storage + prepared stripe capability
}
```

The capabilities are deliberately split:

```rust
view.for_each(...);
view.for_each_with_entity(...);
view.for_each_chunk(...);
view.for_each_chunk_with_entities(...);
view.count();
view.is_empty();

par_view.par_for_each(...);
par_view.par_for_each_with_entity(...);
par_view.par_for_each_chunk(...);
par_view.par_for_each_chunk_with_entities(...);
```

Neither type exposes structural APIs or can outlive one system invocation. `View` never builds stripe jobs. `ParView` prepares them during the serial system-prepare pass and reuses the existing cached 4096-entity stripe executor; its runner still falls back to sequential work for small inputs.

### 3.2 Resources

```rust
pub struct Res<'w, T>(&'w T);
pub struct ResMut<'w, T>(&'w mut T);
```

Both implement `Deref`; `ResMut` also implements `DerefMut`.

Required worker-thread bounds for ordinary systems:

```text
Res<T>       requires T: Sync
ResMut<T>    requires T: Send
View<&T>     requires T: Sync
View<&mut T> requires T: Send
ParView<&T>     requires T: Sync
ParView<&mut T> requires T: Send
```

Missing required resources are detected by a whole-frame serial preflight before time advances or any system runs. The returned schedule error contains the system and resource type names. Required resources therefore exist at frame entry rather than being created opportunistically by an earlier system. The successful preflight is cached until `resource_epoch` or registered access changes. `World` changes that epoch on resource insertion/removal; cached resource pointers are reused only while World identity and the epoch still match.

Optional and non-worker-thread resource parameters are deferred until a demonstrated use case. They must not complicate the first safety model.

`Time` is injected read-only through `Res<Time>`. Ordinary systems cannot overwrite `delta` for later systems; time-scale changes use a dedicated configuration/resource mutation path and take effect at a documented frame boundary.

### 3.3 Commands

```rust
pub struct Commands<'w> {
    buffer: &'w mut CommandBuffer,
}
```

Commands are append-only during a system invocation. Existing per-entity coalescing and first-seen entity order remain properties of each buffer.

Arbitrary command payloads make general rollback impossible. If apply unwinds, the World is poisoned: inspection and shutdown remain available, but later command application and schedule ticks fail fast instead of continuing from a partial commit.

### 3.4 Local

```rust
pub struct Local<'s, T>(&'s mut T);
```

Initial MVP requires `T: Default + Send + 'static`. A later `FromWorld` constructor can be added without changing call-site syntax.

## 4. Access inference

Each system parameter contributes an internal access declaration:

| Parameter | Access |
| --- | --- |
| `View<&T>` | read component `T` |
| `View<&mut T>` | write component `T` |
| `ParView<Q>` | same access as `View<Q>`, plus serial parallel-job preparation |
| `Option<&T>` inside a view | read component `T` |
| `Option<&mut T>` inside a view | write component `T` |
| `Res<T>` | read resource `T` |
| `ResMut<T>` | write resource `T` |
| `Commands` | deferred structural write; no direct World conflict |
| `Local<T>` | system-private; no conflict |
| exclusive system | conflicts with everything |

Filters contribute no data access.

The MVP does not use filters to prove disjointness. For example, `With<Player>` and `Without<Player>` still conflict when both systems write the same component type. This conservative rule keeps safety independent of archetype-set changes.

Two ordinary systems conflict when any component or resource has:

```text
write / read
read / write
write / write
```

Read/read is compatible.

The complete parameter tuple is validated at registration. These signatures are rejected before the first tick:

```rust
fn invalid(a: View<&mut Position>, b: View<&Position>) { ... }
fn invalid(a: ResMut<Config>, b: Res<Config>) { ... }
```

This is separate from the existing duplicate-component validation inside one query.

## 5. Deterministic wave compilation

Stages are hard ordering and command-visibility barriers. Each stage compiles its ordinary systems into waves.

For systems in registration order `S0..Sn`:

```text
wave(Sj) = max(wave(Si) + 1) for every earlier Si that conflicts with Sj
wave(Sj) = 0 when no earlier conflict exists
```

Exclusive systems split a stage into independent segments and occupy their own barrier wave.

Example:

```text
A: write Position
B: read AudioSource
C: read Position
D: write Animation
E: write Position
```

Compiled execution:

```text
Wave 0: A || B || D
Wave 1: C
Wave 2: E
```

Properties:

- systems in one wave never conflict;
- registration order is preserved for every conflicting pair;
- unrelated systems may move earlier, which is the source of parallelism;
- compilation is deterministic;
- the compiled plan is cached until stage membership or access metadata changes.

Before worker execution, the stage performs a serial prepare pass:

1. Refresh every `View` / `ParView` system parameter against `archetype_epoch`.
2. For `ParView` only, refresh cached stripe jobs against `storage_epoch`; `par_*` only clones the prepared snapshot during execution.
3. Resolve resource pointers against `resource_epoch` and report missing required resources.
4. Resolve stable component pointers for the upcoming wave.
5. Freeze component layout and resource-map mutation until the wave joins.

Worker threads never access the current World-owned `RefCell` query cache. Query-plan state belongs to the registered function system; `View` receives the sequential plan and `ParView` receives the already-prepared stripe capability for that invocation.

Arbitrary system-level `before` / `after` edges are intentionally omitted from the MVP. Use stages for semantic ordering. If real projects demonstrate that stages are insufficient, explicit edges can be added to the same compiler later.

## 6. Command visibility and determinism

Ordinary systems cannot mutate World structure directly.

Default rule:

1. Every system receives a private command buffer.
2. Waves execute without applying structural commands.
3. At the end of the stage, buffers are applied in system registration order.
4. The next stage observes those structural changes.

The merge order does not depend on worker completion order.

An exclusive system introduces an internal flush boundary:

```text
ordinary waves
  -> apply preceding command buffers in registration order
  -> exclusive system
  -> continue later ordinary segment
  -> stage-end command apply
```

Commands must not be applied after every automatically generated wave. Wave partitioning is an implementation detail; changing it must not change command visibility.

If one operation needs to observe another operation's commands, they belong in different stages or on opposite sides of an explicit exclusive barrier.

## 7. Execution and nested data parallelism

Stage-level system parallelism and query-level stripe parallelism share one Rayon pool.

```text
Schedule stage
  -> run conflict-free systems in parallel
       -> systems with ParView may call ParView::par_for_each
            -> reuse cached query stripe jobs
  -> join complete wave
```

Rayon's nested work stealing avoids creating a second thread pool. The scheduler must not spawn one pool per system.

Small query workloads retain the existing automatic sequential fallback. A system may therefore run in parallel with other systems while processing its own small view sequentially.

System waves also have a dispatch threshold. The default minimum is three compatible systems, so one- and two-system waves stay on the scheduler thread; a stage can opt a known-heavy pair into Rayon with `parallel_wave_min_systems(2)`. The actual path is counted in `TickReport`.

## 8. Internal implementation boundary

Normal users do not implement this trait. It is public only through a hidden macro-support module when necessary:

```rust
pub unsafe trait SystemParam {
    type State: Send + 'static;
    type Item<'world, 'state>;

    fn register(meta: &mut SystemMeta) -> Result<(), SystemConfigError>;
    fn init(world: &mut World) -> Result<Self::State, ParamError>;

    unsafe fn get<'world, 'state>(
        world: UnsafeWorldCell<'world>,
        state: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> Self::Item<'world, 'state>;
}
```

`SystemParamContext` is a private copyable capability containing the invocation's unique command-buffer pointer and other per-run state. Parameter-tuple registration rejects duplicate `Commands`, `ResMut<T>`, or overlapping view/resource accesses before `get` runs. This avoids repeatedly borrowing one `&mut CommandBuffer` while constructing tuple parameters.

Safety contract:

- `register` must describe every direct component/resource access exactly;
- `get` may construct only the references declared by `register`;
- references must remain inside one system invocation;
- command access must target only the provided private buffer;
- no ordinary parameter may expose structural mutation;
- the scheduler may call `get` concurrently only after proving access sets disjoint.

Function arities can initially be generated up to 16 parameters, matching query/filter tuple capacity.

The erased runner is conceptually:

```rust
trait ErasedSystem: Send {
    fn name(&self) -> &str;
    fn access(&self) -> &AccessSet;
    fn initialize(&mut self, world: &mut World) -> Result<(), ScheduleError>;
    unsafe fn run(&mut self, world: UnsafeWorldCell<'_>);
}
```

`ErasedSystem` covers ordinary worker-capable systems only. Exclusive systems are stored as separate main-thread barrier nodes, so their implementation may own non-`Send` state while still accessing non-`Send` World data safely.

`UnsafeWorldCell` remains private to `sky_ecs`. It is never an expert public API.

## 9. Initialization, shutdown, and panic behavior

Initialization occurs before the first schedule execution pass, in stage and registration order. It is not delayed until a fixed stage accumulates its first substep.

Shutdown order is the reverse of successful initialization order.

The schedule must be restored to `World` by an RAII guard on every unwind path:

```text
take schedule
  -> guard owns restoration obligation
  -> execute / panic
  -> guard restores schedule in Drop
```

Panic policy:

- all Rayon work in the current wave is joined;
- pending command buffers since the last documented flush boundary are discarded;
- the schedule is restored and remains inspectable/shutdown-capable;
- already completed component/resource writes are not rolled back;
- a panic from command application poisons the World, so partial command state cannot be ticked or flushed again;
- the panic continues unless the application installs a catch policy.

`shutdown` needs the same restoration guarantee when an exclusive teardown hook panics.

Recursive tick and schedule mutation during execution remain rejected.

## 10. Tick API and diagnostics

Driver:

```rust
let report = world.tick()?;
let report = world.tick_with_delta(0.016)?;
let report = world.tick_with_frame_delta(frame_delta, raw_delta)?;
```

Report:

```rust
pub struct TickReport {
    pub frame: u64,
    pub systems_run: u32,
    pub waves_run: u32,
    pub parallel_waves_run: u32,
    pub sequential_waves_run: u32,
    pub fixed_substeps: u32,
    pub dropped_fixed_time: f64,
}
```

Names and timing metadata exist in every build; the `profile` feature controls profiler emission, not whether the schedule knows system names.

Useful diagnostics:

- compiled waves per stage;
- access set per system;
- reason two systems conflict;
- fixed-step backlog and dropped time;
- last completed stage/wave/system;
- last/total enqueued, successfully flushed, and panic-discarded command counts per system;
- sequential fallback vs parallel execution counters.

## 11. Error model

Configuration errors are reported before execution when possible:

```rust
pub enum ScheduleBuildError {
    DuplicateStage,
    UnknownStage,
    UnknownStageAnchor,
    InvalidFixedStep,
    ConflictingFixedStep,
    InvalidParallelWaveMinimum,
}

pub enum ScheduleError {
    MissingResource { system: String, resource: &'static str },
}
```

The MVP has no arbitrary ordering DAG, so it cannot form a stage cycle. `ScheduleError` is frame-atomic: it is returned only by preflight, before time or systems change. Removing a declared required resource during the same tick is instead a programmer invariant panic; the schedule guard still restores scheduler ownership and clears unflushed commands. Other programmer-only invariant violations such as overlapping parameters panic during registration with type-rich messages. Runtime configuration uses checked constructors and returned errors.

## 12. Migration from the current API

Compatibility is not required.

Current:

```rust
world.group("sim").add(|world: &mut World| {
    world
        .query_mut::<(&mut Position, &Velocity)>()
        .for_each(|(position, velocity)| {
            position.x += velocity.x;
        });
});
```

New:

```rust
fn movement(entities: View<(&mut Position, &Velocity)>) {
    entities.for_each(|(position, velocity)| {
        position.x += velocity.x;
    });
}

world.stage(Update).add(movement);
```

Current full-world closures migrate explicitly:

```rust
world.stage(Update).add_exclusive(old_system);
```

Current owned `Commands` becomes `CommandBuffer`; the system parameter takes the canonical `Commands` name.

## 13. Implementation sequence

### Phase 1: correctness hardening

- validate fixed durations;
- add bounded `max_substeps` and overflow reporting;
- move fixed accumulator to `f64`;
- add panic-safe schedule restoration;
- retain system names without the profile feature;
- test panic recovery, zero/negative/NaN configuration, and large delta.

### Phase 2: typed sequential systems

- add `View`, `ParView`, `Res`, `ResMut`, `Commands`, and `Local`;
- add World `resource_epoch` invalidation for cached system resource pointers;
- add hidden unsafe `SystemParam` contract;
- implement function-to-system conversion up to 16 parameters;
- validate duplicate/conflicting parameters inside one system;
- execute typed systems sequentially first;
- rename owned `Commands` to `CommandBuffer`.

### Phase 3: deterministic wave parallelism

- derive and store `AccessSet`;
- compile conflict edges and stable waves;
- execute wave systems through the shared Rayon pool;
- retain exclusive barriers;
- add deterministic stage-end command application;
- verify results under randomized worker completion delays.

### Phase 4: fixed stages and diagnostics

- migrate fixed execution to typed stages;
- expose `TickReport` and fixed overflow metrics;
- expose compiled-wave diagnostics;
- benchmark schedule overhead, conflict-heavy stages, conflict-free stages, nested query parallelism, and command merging.

## 14. Required validation

Correctness tests:

- read/read systems share a wave;
- read/write and write/write systems are ordered;
- resources participate in the same conflict rules;
- registration order is preserved for conflicting pairs;
- exclusive systems are hard barriers;
- command application order is registration order, not completion order;
- commands become visible only at documented boundaries;
- a command-apply panic poisons World and prevents later apply/tick;
- panic restores schedule ownership;
- fixed zero/negative/NaN steps cannot execute;
- fixed stages respect max substeps and overflow policy;
- missing resources report system/type context;
- non-`Send` data cannot enter a worker-thread system;
- `Local<T>` instances are isolated per system;
- teardown/drop order is deterministic.

Performance gates:

- sequential typed system dispatch stays within noise of the current boxed dispatch for non-trivial systems;
- cached schedule execution performs no per-frame graph allocation;
- access/wave compilation occurs only after schedule mutation;
- command buffers are reused between frames;
- conflict-free CPU-heavy systems scale across available workers;
- nested system/query parallelism uses one thread pool;
- an empty or tiny schedule does not pay meaningful Rayon dispatch overhead.

## 15. Final API boundary

Canonical gameplay surface:

```text
World::stage / try_stage / insert_stage_after
StageBuilder::add / add_named / add_exclusive / fixed
View<Q, F>
ParView<Q, F>
Res<T> / ResMut<T>
Commands
Local<T>
FixedStep / FixedOverflow
Time
```

Advanced but public:

```text
CommandBuffer
ExclusiveSystem
StageLabel
TickReport
schedule diagnostics
```

Private implementation:

```text
SystemParam
FunctionSystem
ErasedSystem
AccessSet
UnsafeWorldCell
compiled conflict edges
wave executor
per-system command storage
```

The central design rule is: access types are explicit in function signatures, scheduling is inferred internally, and arbitrary `&mut World` access is visible as an exclusive barrier rather than disguised as an ordinary system.
