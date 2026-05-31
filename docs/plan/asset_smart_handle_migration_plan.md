# Asset Smart Handle Migration Plan

## Status

Implementation reference for the strong-handle asset surface.

This plan is the recommended next step for the asset system. It supersedes the
older weak-`Handle` / strong-`AssetRef` direction where they conflict. The target
API optimizes for normal game code:

```rust
let player = assets.load::<TextureAsset>("sprites/player.png")?;

world.spawn((
    Transform::default(),
    SpriteRenderer::new(64.0, 64.0).texture(player.clone()),
));
```

The user should not manually manage asset lifetime. A cloned handle keeps the
asset alive. Dropping the final strong handle allows the asset system to retire
the asset when it is safe.

## Design Goal

SkyEngine's asset system should be:

- modern: RAII lifetime, async loading, events, safe hot reload;
- simple: one public facade, one normal handle type, minimal status surface;
- hard to misuse: no routine manual `unload`, no exposed internal state maze;
- decoupled: asset owns CPU/runtime assets, render/audio/video own native
  backend caches;
- plugin-friendly: each engine/game plugin registers its own asset types and
  factories;
- observable: status, stats, diagnostics, and explanation APIs are first-class.

## Final Target Model

### Public Names

Use these names consistently:

- `Assets`: the public asset facade resource.
- `Handle<T>`: the normal strong smart asset handle.
- `WeakHandle<T>`: typed weak identity handle.
- `AssetId`: stable serializable identity.
- `AssetPath<T>`: typed path/key reference for source-authored documents.
- `AssetStatus`: simple user-facing readiness status.
- `AssetDiagnostics` / `AssetStats`: observability.

Do not expose `AssetServer` in docs unless the code is renamed. The current code
exports `Assets`, so user-facing documentation should say `Assets`.

### Strong `Handle<T>`

`Handle<T>` becomes the default runtime smart pointer.

Required behavior:

- `Handle<T>` is `Clone`, not `Copy`.
- Cloning a handle increments a strong load lease.
- Dropping a handle releases that lease.
- While at least one strong handle exists, the asset remains requested.
- Dependencies are kept alive by internal dependency leases.
- `Handle<T>` can be stored directly in ECS components.
- `Handle<T>` should not implement `Deref<Target = T>`, because the asset may
  still be loading, failed, or reloading.

Required methods:

```rust
impl<T: Asset> Handle<T> {
    pub fn id(&self) -> AssetId;
    pub fn downgrade(&self) -> WeakHandle<T>;
    pub fn status(&self) -> AssetStatus;
    pub fn is_ready(&self) -> bool;
    pub fn try_get(&self) -> Option<Arc<T>>;
    pub fn get(&self) -> Result<Arc<T>, AssetError>;
    pub fn error(&self) -> Option<AssetError>;
}
```

Implementation note: the methods may forward through a private weak pointer to
the owning `AssetsInner`. If the owning asset facade is gone, the handle should
report a terminal missing/disconnected status rather than panic.

### Weak Identity

`WeakHandle<T>` is for identity without lifetime.

Use it for:

- serialized scenes;
- prefab documents;
- editor selections;
- save files;
- asset references that should not force residency.

Required behavior:

- `WeakHandle<T>` is `Copy` if possible.
- `WeakHandle<T>` contains `AssetId` and type marker only.
- `WeakHandle<T>` does not keep the asset loaded.
- `WeakHandle<T>` can be upgraded through an `Assets` facade:

```rust
let strong: Handle<TextureAsset> = assets.load_handle(weak)?;
```

### Asset Paths

`AssetPath<T>` represents a typed source path or virtual package path.

Use it for authoring-facing data:

```rust
#[derive(Serialize, Deserialize)]
struct SpritePrefab {
    texture: AssetPath<TextureAsset>,
}
```

At runtime:

```rust
let handle = assets.load(sprite_prefab.texture)?;
```

This keeps serialization stable while letting runtime code use strong handles.

## Public API Target

The normal surface should fit on one screen:

```rust
impl Assets {
    pub fn new(config: AssetConfig) -> Result<Self, AssetError>;
    pub fn empty(config: AssetConfig) -> Self;

    pub fn load<T: Asset>(&self, path: impl IntoAssetKey<T>) -> Result<Handle<T>, AssetError>;
    pub fn load_id<T: Asset>(&self, id: AssetId) -> Result<Handle<T>, AssetError>;
    pub fn load_handle<T: Asset>(&self, handle: WeakHandle<T>) -> Result<Handle<T>, AssetError>;

    pub fn get<T: Asset>(&self, handle: &Handle<T>) -> Result<Arc<T>, AssetError>;
    pub fn try_get<T: Asset>(&self, handle: &Handle<T>) -> Option<Arc<T>>;
    pub fn status<T: Asset>(&self, handle: &Handle<T>) -> AssetStatus;
    pub fn explain<T: Asset>(&self, handle: &Handle<T>) -> AssetExplanation;

    pub fn insert_runtime<T: Asset>(&self, asset: T) -> Handle<T>;
    pub fn replace_runtime<T: Asset>(&self, handle: &Handle<T>, asset: T) -> Result<(), AssetError>;

    pub fn update(&self) -> Result<(), AssetError>;
    pub fn stats(&self) -> AssetStats;
    pub fn events_since(&self, cursor: &mut AssetEventCursor) -> Vec<AssetEvent>;

    pub fn evict_unused(&self);
}
```

`unload` should be removed from normal docs. If retained temporarily, move it to
an expert section and make it operate only on weak identity:

```rust
pub fn force_evict<T: Asset>(&self, handle: WeakHandle<T>);
```

## User-Facing Status

Expose a small status enum:

```rust
pub enum AssetStatus {
    NotRequested,
    Loading,
    Ready,
    Failed,
}
```

Internal states may remain more detailed:

- queued;
- loading;
- dependency waiting;
- installing;
- installed;
- reload pending;
- retiring;
- failed.

The detailed states should appear in `AssetExplanation`, not in the common API.

## Internal Architecture

Split `src/asset` into clearer private modules:

```text
src/asset/
  mod.rs
  config.rs
  error.rs
  id.rs
  handle.rs
  status.rs
  events.rs
  stats.rs
  registry.rs
  manifest.rs
  store.rs
  lifetime.rs
  loader.rs
  worker.rs
  reload.rs
  runtime.rs
  texture.rs
  font.rs
  cook/
```

The public facade remains one resource:

```rust
pub struct Assets {
    inner: Arc<AssetsInner>,
}
```

Internal ownership:

```text
Assets
  -> AssetRegistry        asset type factories
  -> AssetManifestDb      cooked/source/package lookup
  -> AssetStore           records, installed Arc payloads
  -> AssetLifetime        strong leases, dependency leases, retire queue
  -> AssetLoadQueue       priority, cancellation, worker pool jobs
  -> AssetEvents          bounded event log
  -> AssetDiagnostics     structured warnings/errors
```

## Lifetime Model

### Old Problem

The old lifetime model was manual:

```text
Assets::load -> direct_request_count += 1
Assets::unload -> direct_request_count -= 1
```

This is easy to leak and easy to forget.

### Implemented Direction

Each `Handle<T>` owns one direct lease.

```text
Assets::load(path)
  -> resolve AssetId
  -> create direct lease
  -> return Handle<T> carrying lease token

Handle<T>::clone
  -> clone lease token

last Handle<T>::drop
  -> enqueue release
  -> asset can retire after dependencies/backend caches release
```

Dependencies use internal leases:

```text
Material Handle alive
  -> material record owns dependency leases for texture ids
  -> textures stay requested

Material Handle dropped
  -> material lease releases
  -> dependency leases release
  -> textures become eligible for retirement if nothing else uses them
```

### Important Rule

Backend caches must not keep CPU assets alive forever unless they explicitly own
a cache lease. Normal render/audio/video native caches should react to events
and evict when CPU assets retire or reload.

## Loading And Worker Pool

Replace thread-per-load with a bounded worker pool.

Required behavior:

- configurable worker count;
- priority queue;
- cancellation of jobs whose generation is stale;
- generation check on completion;
- no panic if the facade is dropped while jobs are running;
- background loading defaults on for app usage;
- install budget still applies on the main/update side.

Priorities:

```rust
pub enum AssetLoadPriority {
    Background,
    Preload,
    Visible,
    Blocking,
}
```

Use cases:

- visible sprite texture: `Visible`;
- scene preload group: `Preload`;
- editor thumbnail: `Background`;
- `load_blocking`: `Blocking`.

## Events

Keep cursor-style events, but expand and clean names:

```rust
pub enum AssetEventKind {
    Requested,
    LoadStarted,
    CpuLoaded,
    Ready,
    ReloadStarted,
    ReloadCommitted,
    ReloadFailed,
    Retired,
    Failed,
}
```

Events should include:

- `AssetId`;
- asset type;
- generation;
- public status;
- optional diagnostic id;
- optional error summary.

Render/audio/video caches should consume events through a shared utility instead
of each module inventing its own polling shape.

## Hot Reload Model

Hot reload must be safe.

Rules:

- reload success commits atomically;
- reload failure keeps the previous installed asset alive;
- dependents reload after dependencies;
- backend caches invalidate only after commit;
- diagnostics report reload failure without breaking the running scene;
- file watching is optional but the reload pipeline must support it.

Flow:

```text
file watcher / manual reload_changed
  -> detect changed source/cooked manifest
  -> mark root changed
  -> compute dependent closure
  -> load new generation in background
  -> validate dependencies
  -> install new generation
  -> commit record
  -> emit ReloadCommitted
  -> backend caches invalidate old native resources
```

Failure path:

```text
load/install fails
  -> keep previous installed payload
  -> emit ReloadFailed
  -> keep status Ready if old generation exists, Failed only if no old asset exists
```

## Backend Cache Boundary

Asset system owns CPU/runtime assets only.

Render owns:

- GPU textures;
- GPU meshes;
- GPU material handles;
- render asset upload stats.

Audio owns:

- decoded playback buffers or streaming state;
- mixer/backend resources.

Video owns:

- decoder/player state;
- streamed frame textures;
- native frame queues.

Asset system must not depend on `wgpu`, audio backend internals, or video
decoder internals.

## Plugin Model

App setup:

```rust
let mut world = World::new();
world.install(AssetPlugin::new("assets"))?;
world.install(RenderPlugin::forward_2d())?;
world.install(AudioPlugin::default())?;
world.install(VideoPlugin)?;
```

Plugin responsibilities:

- `AssetPlugin` installs `AssetConfig`.
- app runner creates one `Assets` facade if absent.
- render/audio/video plugins register their asset factories during service setup.
- game plugins may register their own factories through `Assets::register_factory`.

No central `DefaultPlugins` is required.

## Cooking Model

Keep cooked manifest, but make cooking extensible.

Current issue: `cook.rs` has central file-extension switches.

Target:

```rust
pub trait AssetCooker {
    fn asset_type(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn import(&self, ctx: ImportContext<'_>) -> Result<AssetMeta, AssetError>;
    fn cook(&self, ctx: CookContext<'_>) -> Result<CookedAsset, AssetError>;
}
```

Built-in cookers:

- texture;
- font;
- sound clip;
- music track;
- video clip;
- mesh;
- standard material.

Later cookers:

- shader module;
- scene/prefab;
- tile map document;
- Live2D model package.

## Diagnostics And Stats

Add:

```rust
pub struct AssetStats {
    pub known_assets: usize,
    pub strong_handles: usize,
    pub loading_assets: usize,
    pub ready_assets: usize,
    pub failed_assets: usize,
    pub retired_assets: usize,
    pub queued_jobs: usize,
    pub inflight_jobs: usize,
    pub resident_cpu_bytes: usize,
    pub last_update_ms: f64,
}
```

Add explanation:

```rust
pub struct AssetExplanation {
    pub id: AssetId,
    pub asset_type: String,
    pub status: AssetStatus,
    pub strong_count: usize,
    pub dependency_count: usize,
    pub dependents: Vec<AssetId>,
    pub source_path: Option<String>,
    pub cooked_path: Option<String>,
    pub last_error: Option<AssetError>,
}
```

Diagnostics should use stable ids:

- `asset.manifest.missing`;
- `asset.manifest.invalid`;
- `asset.load.failed`;
- `asset.install.failed`;
- `asset.reload.failed`;
- `asset.dependency.missing`;
- `asset.dependency.cycle`;
- `asset.type.factory_missing`.

## Migration Phases

### Phase 0: Align Documentation And Naming

Tasks:

- Update `docs/reference/asset.md` from `AssetServer` to `Assets`.
- Update event examples to match actual events or new planned events.
- Add a short "current vs target" note while migration is in progress.
- Keep `docs/plan/sakura_resource_system_adaptation_plan.md` as the current
  asset-system status record where older asset planning drafts used to conflict
  with strong `Handle<T>`.

Validation:

```bash
rg "AssetServer|AssetRef|load_ticket|StrongHandle" docs src
```

### Phase 1: Introduce Strong Handle Internals

Tasks:

- Move handle code to `src/asset/handle.rs`.
- Make `Handle<T>` non-`Copy`.
- Add `WeakHandle<T>`.
- Add internal `AssetLease`.
- Teach `AssetsInner` to create/release direct leases.
- Keep old `load/unload` behavior behind temporary internal adapters only if
  needed during migration.

Tests:

- cloning a handle keeps one asset requested;
- dropping the last handle retires an unused asset;
- weak handle does not keep an asset alive;
- stale weak handle can be reloaded through `Assets`;
- runtime inserted asset remains alive while handle exists.

### Phase 2: Migrate Public Load API

Tasks:

- Change `Assets::load<T>(id)` to return strong `Handle<T>`.
- Add path/key overload through a trait or explicit `load_path`.
- Change `load_by_path` call sites to the new API.
- Change `replace_runtime` to accept `&Handle<T>`.
- Hide or deprecate normal `unload`.

High-risk call sites:

- render sprite/tilemap components;
- video frame replacement;
- audio emitter components;
- VN presentation sync;
- examples that currently copy handles freely.

Tests:

- `cargo test`;
- `cargo test --features app`;
- `cargo check --examples --features app`;
- `cargo check --examples --features "app audio"`;
- `cargo check --examples --features video`.

### Phase 3: Dependency Lease Rewrite

Tasks:

- Replace `dependency_ref_count` with dependency lease records.
- Parent asset install creates leases for all dependencies.
- Parent retirement releases dependency leases.
- Reload replaces dependency leases atomically.
- Dependency cycles remain detected before install commit.

Tests:

- parent handle keeps dependency alive;
- dropping parent retires dependency when unused;
- shared dependency stays alive while one parent remains;
- dependency reload reloads dependents;
- dependency failed does not leak leases.

### Phase 4: Simplify User Status

Tasks:

- Add `AssetStatus`.
- Keep internal state enum private or move it to expert docs.
- Add `Assets::status`, `Handle::status`, `Handle::is_ready`.
- Update render readiness code to use simple status where possible.

Tests:

- each internal state maps to a correct public status;
- reload failure with old generation maps to `Ready` plus error event;
- first-load failure maps to `Failed`.

### Phase 5: Worker Pool And Priority

Tasks:

- Add `AssetLoadQueue`.
- Add bounded worker threads.
- Add priority and generation cancellation.
- Remove direct thread-per-load from `spawn_load_record`.
- Keep synchronous path only for explicit blocking loads/tests.

Tests:

- queue respects visible over preload priority;
- stale completion is ignored;
- dropping all handles before completion cancels or discards the result;
- worker count is bounded;
- no deadlock when factory load fails.

### Phase 6: Safe Hot Reload

Tasks:

- Add two-generation reload record: current installed and pending generation.
- Commit only after load/install success.
- Keep old payload on reload failure.
- Emit `ReloadCommitted` / `ReloadFailed`.
- Update render/audio/video caches to consume reload events consistently.

Tests:

- texture reload updates GPU texture after commit;
- failed texture reload keeps old GPU texture;
- material reload invalidates dependent GPU material;
- audio reload does not stop unrelated playback;
- dependency reload reloads dependent material/video/scene asset.

### Phase 7: Asset Stats And Debug UI Support

Tasks:

- Add `AssetStats`.
- Add `AssetExplanation`.
- Add stable diagnostic ids.
- Add docs and optionally a small debug example.

Tests:

- stats counts loading/ready/failed/retired correctly;
- explanation reports dependencies and last error;
- event log cursor handles overflow predictably.

### Phase 8: Extensible Cooking

Tasks:

- Introduce `AssetCooker`.
- Move texture/font/audio/video cooking into cookers.
- Register built-in cookers.
- Add mesh and standard material cookers.
- Keep manifest format versioned.

Tests:

- existing texture/audio/video cook tests still pass;
- custom test cooker can import/cook without editing central enum;
- verify catches missing cooked artifacts and hash mismatch;
- cooked manifest round-trips asset type and dependencies.

## Compatibility Policy

This migration may be breaking.

Allowed breaking changes:

- `Handle<T>` no longer implements `Copy`;
- normal `unload` is removed or hidden;
- `load_by_path` may be renamed or folded into `load`;
- docs stop using `AssetServer`;
- serialized documents should use `AssetId`, `WeakHandle<T>`, or `AssetPath<T>`.

Not allowed:

- asset system depending on render/audio/video backend internals;
- making global asset state required;
- forcing every app to install render/audio/video;
- losing cooked texture/audio/video functionality during migration;
- breaking runtime inserted generated assets.

## Acceptance Criteria

The migration is done when:

- normal examples do not call `unload`;
- ECS components can store `Handle<T>` directly;
- dropping handles releases unused assets automatically;
- dependencies are automatically retained and released;
- reload failure keeps old working assets alive;
- render texture cache invalidates on reload/unload events;
- CPU asset stats and diagnostics are available;
- docs consistently say `Assets`, `Handle<T>`, `WeakHandle<T>`, and `AssetPath<T>`;
- all core and app/example checks pass.

Required checks:

```bash
cargo fmt
cargo test
cargo test --features app
cargo check --examples --features app
cargo check --examples --features "app audio"
cargo check --examples --features video
cargo check --examples --features ui-legacy
cargo check --examples --features ui-neo
cargo check --examples --features yakui-ui
```

## First Implementation Slice

Start small and prove the lifetime model before touching worker pools or cooking.

Slice:

1. Add `WeakHandle<T>`.
2. Make `Handle<T>` non-`Copy`.
3. Add internal lease create/drop.
4. Keep current loading implementation.
5. Convert `direct_request_count` to lease count.
6. Migrate compile errors from non-`Copy` handles.
7. Add focused lifetime tests.

Why this first:

- it attacks the biggest usability flaw;
- it reveals all hidden assumptions about handles being copy ids;
- it does not require solving hot reload, cooking, or worker pools at the same
  time;
- once this passes, the rest of the plan becomes incremental instead of risky.
