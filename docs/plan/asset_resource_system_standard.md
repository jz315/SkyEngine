# Asset Resource System Standard

## Status

Draft standard for the direct rewrite of SkyEngine's asset and resource
architecture.

This document is normative for future asset-system design. Implementation plans
may reference it, but this document defines the target rules and boundaries.

## Normative Language

The words `MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, and `MAY` are used with
their ordinary standards meaning:

- `MUST`: required for the target architecture.
- `MUST NOT`: forbidden in the target architecture.
- `SHOULD`: strongly preferred unless there is a documented reason.
- `MAY`: allowed, but not required.

## Goals

The asset resource system MUST be:

- easy to use for normal game code;
- explicit about asset identity and lifetime;
- hard to misuse accidentally;
- decoupled from rendering, audio playback, and video playback;
- observable through status, events, diagnostics, and explanation APIs;
- extensible for new asset types without editing a central type switch;
- suitable for cooked builds, development hot reload, tools, and tests.

## Non-Goals

The asset resource system MUST NOT become:

- a renderer;
- an audio playback engine;
- a video decoder/player;
- a global singleton dependency;
- a collection of many public servers inserted into `World`;
- a service locator for unrelated engine subsystems;
- a compatibility wrapper around the old manual `load/unload` model.

## Top-Level Rule

SkyEngine MUST expose one normal public asset facade:

```rust
Assets
```

`Assets` is the user-facing entry point for asset identity, loading,
dependencies, lifetime, events, status, and diagnostics.

The implementation MAY be split into many internal modules, but normal user code
MUST NOT need to assemble or fetch those internal services individually.

## No Global Asset Singleton

The engine MUST NOT require global asset state.

There MUST NOT be a canonical `Assets::global()` path used by engine internals.
Examples, tests, tools, apps, and editors MUST be able to create independent
`Assets` instances.

Rationale:

- tests must not contaminate each other through global asset state;
- editors may open multiple projects or asset roots;
- preview worlds may need different databases from the game world;
- hot reload policy must belong to a concrete asset instance;
- package mounts and IO backends must be explicit.

## World Resource Rule

`World` MAY contain one public asset facade:

```rust
world.insert_resource(Assets::new(...));
```

`World` MUST NOT contain the internal asset implementation pieces as separate
public resources:

```rust
// Forbidden as normal architecture:
world.insert_resource(AssetDatabase);
world.insert_resource(AssetStore);
world.insert_resource(AssetLoadQueue);
world.insert_resource(AssetEvents);
world.insert_resource(AssetIo);
world.insert_resource(CookRegistry);
```

Internal asset components MUST remain owned by `Assets` or private module
types.

Expert/debug APIs MAY expose snapshots or reports, but SHOULD NOT expose mutable
internal services directly.

## Public Facade Shape

The target shape is:

```rust
pub struct Assets {
    inner: Arc<AssetsInner>,
}

struct AssetsInner {
    database: AssetDatabase,
    store: AssetStore,
    loader: AssetLoadQueue,
    registry: AssetRegistry,
    io: Box<dyn AssetIo>,
    events: AssetEvents,
    diagnostics: AssetDiagnostics,
}
```

The exact fields MAY change. The facade rule MUST remain.

## Identity And Lifetime

The asset system MUST distinguish identity from lifetime.

### `AssetId`

`AssetId` is a stable asset identifier.

`AssetId` MUST be:

- stable across runs once assigned;
- serializable;
- usable in manifests, scene documents, prefab documents, and cooked assets.

### `Handle<T>`

`Handle<T>` is a typed weak identity handle.

`Handle<T>` MUST:

- contain asset identity;
- be cheap to copy;
- be serializable when the asset type supports scene/prefab usage;
- be valid for ECS components and documents;
- NOT keep an asset loaded by itself;
- NOT imply the asset is ready.

`Handle<T>` SHOULD be used in serialized data:

```rust
struct Sprite {
    texture: Handle<TextureAsset>,
}
```

### `AssetRef<T>`

`AssetRef<T>` is the runtime strong asset smart pointer.

`AssetRef<T>` MUST:

- keep the asset load intent alive;
- be cheap to clone;
- release its load intent automatically when the final strong reference drops;
- expose its weak `Handle<T>`;
- expose readiness and error state;
- provide safe access to the installed runtime asset when ready.

`AssetRef<T>` MUST NOT require user code to call `unload`.

`AssetRef<T>` SHOULD NOT implement `Deref<Target = T>`, because assets may be
loading, failed, reloading, or evicted.

Preferred access pattern:

```rust
let texture: AssetRef<TextureAsset> = assets.load("sprites/player.png")?;

if let Some(cpu_texture) = texture.try_get() {
    // use installed CPU-side asset
}
```

Required common methods:

```rust
impl<T: Asset> AssetRef<T> {
    pub fn handle(&self) -> Handle<T>;
    pub fn id(&self) -> AssetId;
    pub fn state(&self) -> AssetState;
    pub fn is_ready(&self) -> bool;
    pub fn try_get(&self) -> Option<Arc<T>>;
    pub fn get(&self) -> Result<Arc<T>, AssetError>;
    pub fn error(&self) -> Option<AssetError>;
}
```

Names MAY change, but the semantics MUST remain.

### `AssetGroup`

`AssetGroup` is a batch of strong asset references.

`AssetGroup` MUST:

- keep all group roots alive;
- keep discovered dependencies alive through the asset system;
- provide aggregate readiness;
- provide aggregate progress;
- provide aggregate error reporting;
- release its load intent automatically when dropped.

`AssetGroup` SHOULD be used for:

- levels;
- scenes;
- character packs;
- UI skins;
- VN chapters;
- tilemap packages;
- preload screens.

Preferred pattern:

```rust
let chapter = assets.load_group("chapter_01")?;
world.insert_resource(chapter);
```

Scene components SHOULD usually store `Handle<T>`, while a scene-level or
chapter-level `AssetGroup` keeps the required assets alive.

## Public Loading API

Normal loading MUST return strong asset references:

```rust
let texture = assets.load::<TextureAsset>("sprites/player.png")?;
```

The system MAY support typed keys:

```rust
assets.load::<TextureAsset>(AssetKey::Path("sprites/player.png"))?;
assets.load::<TextureAsset>(AssetKey::Id(id))?;
assets.load::<TextureAsset>(AssetKey::Label("player.texture"))?;
```

String convenience APIs MAY exist, but the internal key model SHOULD avoid
confusing path, id, label, and package locator semantics.

The normal user-facing API MUST NOT require:

- resolving ids manually;
- calling update loops manually just to complete one load;
- manually balancing `load` and `unload`;
- querying multiple internal services.

## State Model

The internal state model SHOULD be expressive enough for diagnostics and tools.

Recommended states:

```text
Unloaded
Queued
Loading
Loaded
WaitingDependencies
Installing
Ready
Reloading
Failed
Evicting
```

The user-facing API SHOULD reduce this complexity to simple questions:

```rust
asset.is_ready()
asset.try_get()
asset.error()
asset.progress()
assets.explain(handle)
```

## Events

Asset events MUST be useful for caches, loading screens, diagnostics, and tools.

Recommended event kinds:

```text
Queued
LoadStarted
LoadFinished
WaitingDependencies
DependenciesReady
InstallStarted
Ready
ReloadQueued
Reloaded
Failed
Released
Evicted
```

Render/audio/video caches MUST be able to respond to asset readiness, reload,
failure, release, and eviction without polling private internals.

## Explanation API

`Assets` MUST provide a way to explain why an asset is not ready.

Recommended API:

```rust
let report = assets.explain(handle);
```

The report SHOULD include:

- asset id;
- asset type;
- source path if known;
- cooked path or package location if known;
- current state;
- active strong reference count or lease count;
- dependency states;
- queued load priority;
- inflight load stage;
- last error;
- reload status.

## Asset Database

`AssetDatabase` is an internal component of `Assets`.

It SHOULD own:

- manifest entries;
- id lookup;
- source path lookup;
- labels and tags;
- dependency graph;
- reverse dependency graph;
- cooked hashes;
- package locations;
- source metadata summaries.

`AssetDatabase` MUST NOT be required as a separate `World` resource.

## IO

Asset IO MUST be abstracted behind an internal trait.

Recommended trait:

```rust
trait AssetIo: Send + Sync {
    fn read(&self, location: &AssetLocation) -> AssetIoResult<Vec<u8>>;
    fn exists(&self, location: &AssetLocation) -> bool;
}
```

The actual trait MAY be async or staged through a load queue.

The architecture SHOULD support:

- native filesystem IO;
- package IO;
- layered mount IO;
- future web/wasm IO;
- future remote IO.

Gameplay and renderer code MUST NOT call raw filesystem paths for canonical
asset loading.

## Cook Architecture

Cooking MUST be registry-driven.

The cook pipeline MUST NOT depend on a central private enum that lists every
asset kind.

Recommended components:

```text
AssetImporter: source -> metadata/intermediate/dependencies
AssetCooker: source/intermediate -> cooked bytes
AssetRuntimeFactory: cooked bytes -> runtime CPU asset
AssetTypePlugin: optional bundle that registers importer, cooker, runtime factory
```

Importer, cooker, and runtime factory SHOULD be separate concepts. A convenience
plugin MAY register all three for a built-in type.

Built-in asset types SHOULD include:

- `TextureAsset`;
- `SoundClip`;
- `MusicTrack`;
- `VideoClip`;
- `MeshAsset`;
- `StandardMaterialAsset`.

## Runtime Store

`AssetStore` is an internal component of `Assets`.

It MUST store installed runtime CPU assets in a type-safe way.

It MUST NOT store GPU textures, audio playback handles, video decoder state, or
renderer-private objects.

## Loading Queue

Loading SHOULD be queued and bounded.

The loader SHOULD support:

- priority;
- cancellation;
- queue stats;
- separated IO/decode/install stages;
- dependency waits;
- install budget;
- clear failure propagation.

Recommended priorities:

```text
Visible
Imminent
Preload
Background
```

The loader MUST NOT spawn unbounded threads per asset load in the target
architecture.

The loader MUST NOT hold a global asset lock while performing file IO or
expensive decode work.

## Dependencies

Asset dependencies MUST be loaded and held by the asset system.

If `AssetRef<A>` depends on `B`, then `B` MUST remain alive as long as `A` needs
it, unless loading fails or the dependency is explicitly optional.

Dependency cycles MUST be detected and reported through structured errors.

Reverse dependency information SHOULD be available for reload and diagnostics.

## Diagnostics

Asset failures MUST be observable through structured diagnostics, not only
stderr output.

Required diagnostic categories SHOULD include:

```text
asset.manifest.missing
asset.manifest.invalid
asset.load.failed
asset.decode.failed
asset.install.failed
asset.dependency.missing
asset.dependency.failed
asset.dependency.cycle
asset.reload.failed
asset.package.invalid
```

Diagnostics SHOULD include asset id, asset type, path/location, and error
details when known.

## Rendering Boundary

`Assets` MUST NOT create GPU resources.

Rendering owns GPU residency.

Recommended render flow:

```text
Handle<TextureAsset>
  -> Assets resolves CPU TextureAsset readiness
  -> RenderAssetCache uploads or reuses GPU Texture
  -> renderer draws or uses fallback while loading/failing
```

Mesh/material flow:

```text
Handle<MeshAsset>
Handle<StandardMaterialAsset>
  -> Assets resolves CPU assets
  -> renderer backend uploads mesh/material GPU resources
  -> renderer invalidates cache on asset reload/release/evict events
```

Render caches MAY be `World` resources if they are public renderer services, but
they MUST NOT own the asset database.

## Audio Boundary

`Assets` owns audio asset identity, loading, lifetime, and dependencies.

`AudioServer` owns playback behavior.

`Assets` MUST NOT play sounds or music.

`AudioServer` SHOULD accept `AssetRef<SoundClip>` and `AssetRef<MusicTrack>` or
resolve `Handle<T>` through an explicit `Assets` reference.

When playback begins, `AudioServer` SHOULD keep a strong asset reference for as
long as the playback instance needs the asset.

Recommended user flow:

```rust
let click = assets.load::<SoundClip>("audio/click.wav")?;
audio.play(click.clone(), AudioPlayback::default())?;

let bgm = assets.load::<MusicTrack>("audio/title.ogg")?;
audio.play_music(bgm.clone(), AudioPlayback::looped())?;
```

Short sound effects MAY decode fully into memory.

Long music tracks SHOULD support streaming or a runtime representation that does
not require unnecessary full duplication.

## Video Boundary

`Assets` owns video asset identity, metadata, dependencies, and lifetime.

`VideoServer` owns video playback, decode clocks, frame queues, and runtime
decoder state.

`Assets` MUST NOT decode live video frames as part of normal asset loading.

Two video asset families SHOULD be supported:

### Pre-Cooked Frame Sequence

`VideoClip` MAY be an asset containing frame texture handles and frame timing.

This is appropriate for:

- visual novels;
- short cutscenes;
- frame-accurate sprite animation;
- precomputed effects.

`VideoClip` dependencies MUST include its frame textures.

### Streaming Video Source

Container formats such as MP4/WebM SHOULD be represented as source/metadata
assets plus runtime players.

Recommended split:

```text
VideoSourceAsset: asset identity, source/package location, metadata
VideoPlayer: runtime decoder/player
GpuVideoFrameBuffer: current frame GPU target
```

`VideoServer` SHOULD keep a strong `AssetRef<VideoSourceAsset>` while playback
needs access to the source.

## Domain Runtime Rule

The asset system owns resource identity, loading, dependencies, and lifetime.

Domain runtimes own behavior:

```text
AudioServer owns playback.
VideoServer owns decoding and clocks.
Render owns GPU residency and drawing.
```

`Assets` MUST NOT call:

- `play`;
- `pause`;
- `decode_next_frame`;
- `create_wgpu_texture`;
- `draw`;
- `submit`.

Domain runtimes MAY subscribe to asset events or query `Assets` through explicit
references.

## App Integration

Asset support is installed through `AssetPlugin`; its constructor is the asset
configuration surface:

```rust
let mut world = World::new();

world.install(AssetPlugin::new("assets"))?;
world.install(WindowPlugin::new("Game", 1280, 720))?;
world.install(RenderPlugin::forward_2d())?;

App::new(world).run(Game);
```

`App` SHOULD create and insert one public `Assets` facade when `AssetPlugin`
has been installed, unless the user has already inserted one.

Advanced users MAY construct `Assets` manually and insert it into `World`, but
normal app code SHOULD prefer `AssetPlugin`.

`App` MUST NOT use global asset state.

## Tests And Tools

Tests SHOULD create isolated `Assets` instances.

Tools SHOULD receive an explicit asset root/config.

`sky-cook` SHOULD use the same cook registry model as runtime asset type
registration, but it MUST remain usable without running an app.

## Compatibility Policy For Direct Rewrite

The rewrite does not need to preserve the old manual lifecycle API.

The old model:

```rust
let handle = server.load::<TextureAsset>(id)?;
server.unload(&handle);
```

SHOULD be replaced by:

```rust
let texture = assets.load::<TextureAsset>("sprites/player.png")?;
drop(texture); // releases automatically when the final strong ref is gone
```

Compatibility shims MAY exist temporarily during migration, but they MUST NOT
define the final public API.

## Acceptance Criteria

The standard is satisfied when:

- normal user code interacts with one `Assets` facade;
- no asset global singleton is required;
- `World` contains at most the public `Assets` facade for asset internals;
- `Handle<T>` is weak identity;
- `AssetRef<T>` is strong automatic lifetime;
- users do not manually balance load/unload in normal code;
- asset groups support scene-level residency;
- cook is registry-driven;
- runtime factories are extensible;
- render/audio/video own their behavior and backend resources;
- asset failures are diagnosable through structured reports;
- tests and tools can create isolated asset systems.
