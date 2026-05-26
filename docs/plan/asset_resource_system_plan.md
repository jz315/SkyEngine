# Asset Resource System Plan

## Summary

This plan turns SkyEngine's current asset module from a usable runtime service
into a production-grade resource pipeline for games, demos, tools, and future
editor workflows.

The current system is already useful:

- `AssetServer` owns typed handles, manifest lookup, runtime assets, loading
  state, dependency resolution, unload, reload, and event delivery.
- `TextureAsset` has an end-to-end cooked path.
- Audio and video register runtime factories for cooked assets.
- Rendering has a separate GPU residency cache for texture assets.
- `App` installs and updates the default asset server when the `app` feature is
  enabled.

The missing work is mostly about scale, ergonomics, and extensibility:

- cooking is hard-coded instead of plugin-driven;
- handle lifetime is manual and easy to misuse;
- mesh/material assets are not first-class cooked assets yet;
- hot reload is manual;
- there is no official package, group, or scene loading model;
- async loading is thread-per-load and lacks priority, cancellation, and
  progress reporting;
- diagnostics and tooling are thinner than the rest of the engine.

The target is not to make asset loading part of the renderer. The target is to
keep the existing boundary:

```text
source/import/cook/manifest -> AssetServer CPU/runtime asset -> backend cache/GPU upload
```

## Current State

Primary files:

```text
src/asset/mod.rs
src/asset/types.rs
src/asset/server.rs
src/asset/registry.rs
src/asset/texture.rs
src/asset/cook.rs
src/render/resources/texture_cache.rs
src/render/backend/wgpu_asset_bridge.rs
src/render/asset/
src/audio/assets.rs
src/video/assets.rs
src/app/services.rs
src/app/frame.rs
docs/reference/asset.md
```

Current public asset concepts:

- `AssetId`: stable UUID.
- `Handle<T>`: typed handle containing an `AssetId`.
- `Asset`: marker trait with a static asset type string.
- `AssetState`: unloaded, loading, loaded, waiting dependencies, installing,
  installed, unloading, failed states.
- `AssetRuntimeFactory`: runtime load/install factory.
- `AssetMeta`: source-side metadata.
- `AssetRegistryManifest`: cooked runtime manifest.
- `AssetServer`: runtime service.
- `TextureAsset`: CPU-side RGBA texture.
- `LoadedAsset<T>`: loaded payload plus discovered dependencies.

Runtime capabilities:

- typed load by asset id;
- typed load by source path through manifest lookup;
- raw texture loading by path for quick demos;
- runtime insertion and replacement;
- manual unload;
- dependency loading and dependency cycle detection;
- cooked reload detection;
- cursor-style event polling;
- optional background loading;
- install budget per update.

Renderer integration:

- `SharedRenderAssetCache` owns GPU texture residency.
- Sprite extraction can request a texture handle and fall back while CPU/GPU
  data is missing.
- Texture asset events invalidate stale GPU cache entries.
- Wgpu, Kajiya, and Renderling have separate mesh/material upload paths from
  backend-neutral render assets.

## Problems To Fix

### 1. Cooking Is Not Extensible

`src/asset/cook.rs` currently has a private `AssetKind` enum and direct file
extension switches. Adding a new asset type requires editing the central cooker.

This blocks:

- mesh cooking;
- material cooking;
- glTF scene cooking;
- shader/module cooking;
- font assets;
- UI theme/image assets;
- mod-defined or game-defined asset types.

### 2. Runtime Factory And Cooker Factory Are Split

Runtime has `AssetRuntimeFactory`, but offline cooking has no matching public
factory interface. A user can make a runtime asset type, but cannot add it to
the official cook pipeline without modifying engine internals.

### 3. Handles Do Not Own Lifetime

`Handle<T>` is `Copy` and stores only an id. It does not increment or decrement
asset references.

Current lifetime is driven by:

```text
AssetServer::load -> direct_request_count += 1
AssetServer::unload -> direct_request_count -= 1
```

This creates practical risks:

- cloning handles does not keep assets alive;
- ECS components can reference unloaded assets;
- callers must pair loads and unloads manually;
- accidental repeated load calls require matching repeated unload calls;
- there is no scoped strong/weak handle distinction.

### 4. No Asset Group Or Scene Loading Contract

Games need to load more than single assets:

- a level;
- a character bundle;
- a UI skin;
- a VN chapter;
- a tilemap plus palettes, textures, audio, and scripts.

The current dependency system can load transitive dependencies once a root is
requested, but there is no first-class `AssetGroup`, `LoadTicket`, preload
intent, progress aggregate, or scene transition model.

### 5. Async Loading Is Too Primitive

Background loading uses `std::thread::spawn` per load. The server state is
behind `Arc<Mutex<_>>`.

Missing:

- worker pool;
- job priority;
- cancellation;
- per-job progress;
- queue depth reporting;
- bounded IO concurrency;
- CPU decode/install cost accounting;
- cooperative shutdown;
- future/await integration for tools.

### 6. App Asset Configuration Is Plugin-Owned

`AssetPlugin` installs `AssetConfig::default().with_background_loading(true)`
when the app wants an engine-owned asset facade. Custom asset roots, targets,
budgets, and background loading policy belong on `AssetPlugin::new(...)`,
`AssetPlugin::from_config(...)`, or its builder methods.

Missing:

- failed manifest policy;
- diagnostics policy for missing manifest.

### 7. Hot Reload Is Manual

There is `reload_manifest()` and `reload_changed()`, but no watcher loop and no
source-to-cooked live pipeline.

Missing:

- file watcher integration;
- watch debounce;
- automatic recook for changed sources;
- manifest reload after cook;
- dependent asset invalidation;
- render/audio/video cache refresh hooks;
- clear reload diagnostics.

### 8. Events Are Too Coarse

The actual event kinds are:

```rust
ReloadQueued
Installed
Unloaded
Failed
```

This is enough for cache invalidation, but not enough for UI and tooling.

Missing:

- queued;
- load started;
- load completed;
- waiting dependencies;
- install started;
- progress;
- dependency failed;
- cancelled;
- evicted.

`docs/reference/asset.md` should also be corrected because its event example still refers
to event kinds that no longer exist.

### 9. Diagnostics Are Incomplete

Asset failures are often returned as `AssetError` or printed with `eprintln!`.
Render texture misses report structured diagnostics, but the asset system does
not consistently emit structured `Diagnostics` events.

Missing:

- asset subsystem diagnostics;
- missing manifest diagnostics;
- queue stats;
- per-frame loading stats;
- dependency tree explanation;
- "why is this asset not ready" query;
- report integration for agent and CI tooling.

### 10. Mesh And Material Assets Are Not Fully Cooked

`MeshAsset` and `StandardMaterialAsset` implement `Asset`, but the normal path
is still runtime insertion. They do not have full importer/cooker/factory
coverage comparable to textures.

Missing:

- mesh cooked format;
- standard material cooked format;
- material texture dependency declaration;
- glTF import to mesh/material assets;
- vertex layout validation during cook;
- renderer backend upload invalidation on reload.

### 11. GPU Residency Lacks Budgets

`SharedRenderAssetCache` supports texture upload and invalidation, but it has no
memory policy.

Missing:

- GPU byte budget;
- LRU or priority eviction;
- pin/preload policy;
- mip generation;
- compressed texture formats;
- sampler cache;
- atlas/array texture pipeline;
- GPU residency stats in diagnostics.

### 12. Packaging And Distribution Are Missing

The current cooked layout is a filesystem directory under:

```text
.sky/cooked/<target>/manifest.json
```

Missing:

- asset bundles;
- package manifests;
- compression;
- patch/delta metadata;
- package mount order;
- DLC/mod overlays;
- remote download;
- publish-time verification;
- runtime package integrity checks.

### 13. Web And Virtual Filesystem Support Are Not Real Yet

`AssetConfig::default_target()` distinguishes `web`, but runtime IO still
assumes native filesystem and native threads.

Missing:

- virtual filesystem trait;
- async fetch backend;
- wasm-compatible loading path;
- browser cache strategy;
- no-thread fallback;
- target-specific package layout.

### 14. Verification Is Not Runtime-Strong

Cook/verify checks are useful for development, but runtime loading does not
strongly enforce the cooked hash from metadata or package integrity.

Missing:

- manifest-side cooked hash;
- optional strict runtime hash validation;
- version migration policy;
- source/cooked compatibility diagnostics;
- deterministic error messages for stale cooked data.

### 15. ECS Resource Access Is Simple But Not Scheduling-Aware

`World` resources are `TypeId -> Box<dyn Any>`. This is fine for current serial
systems, but it gives no future parallel scheduler enough information to reason
about resource conflicts.

Missing:

- resource access declarations for systems;
- change ticks or dirty markers for resources;
- resource query params;
- structured resource diagnostics.

This is not an immediate asset blocker, but asset services depend heavily on
typed resources in `World`, so the future scheduler shape matters.

## Design Goals

- Keep `AssetServer` as the canonical CPU/runtime asset access point.
- Keep GPU objects out of `asset`.
- Keep asset ids stable and serializable.
- Make custom asset types end-to-end extensible.
- Make lifetime explicit enough that games can avoid leaks and stale handles.
- Make loading observable through progress, stats, diagnostics, and events.
- Make app defaults ergonomic while still allowing custom configuration.
- Make hot reload dependable for local development.
- Make cooked output suitable for shipped builds.
- Keep renderer backends free to cache GPU objects differently.
- Keep the first rewrite slices small and testable.

## Non-Goals

- Do not merge render resource caches into `AssetServer`.
- Do not force Kajiya, Renderling, and Wgpu to share one GPU asset layout.
- Do not make `AssetServer` depend on `wgpu`.
- Do not require an editor to land the runtime asset improvements.
- Do not introduce a universal engine plugin system just for assets.
- Do not break existing texture/audio/video examples without migrating them in
  the same change.

## Target Architecture

Target module shape:

```text
src/asset/
├── mod.rs
├── id.rs
├── handle.rs
├── error.rs
├── config.rs
├── event.rs
├── diagnostics.rs
├── manifest.rs
├── server.rs
├── registry.rs
├── lifetime.rs
├── group.rs
├── load_queue.rs
├── reload.rs
├── package.rs
├── io.rs
├── cook/
│   ├── mod.rs
│   ├── registry.rs
│   ├── context.rs
│   ├── texture.rs
│   ├── audio.rs
│   ├── video.rs
│   ├── mesh.rs
│   ├── material.rs
│   └── verify.rs
└── texture.rs
```

This does not need to land as a single file move. The end state is a guide for
where concepts should live.

Target data flow:

```text
source file
  -> importer detects type
  -> meta is created or normalized
  -> cooker writes target-specific cooked bytes
  -> manifest records id, type, dependencies, hashes, package info
  -> AssetServer loads by id/path/group
  -> loader reads bytes through AssetIo
  -> runtime factory builds loaded payload
  -> dependencies become load tickets
  -> install phase creates runtime asset
  -> events/diagnostics/stats are emitted
  -> render/audio/video backends cache native resources
```

## Public API Targets

### Asset Configuration

```rust
let config = AssetConfig::new("assets", AssetConfig::default_target())
    .with_background_loading(true)
    .with_install_budget_per_update(16)
    .with_strict_hash_validation(false);

let mut world = World::new();
world.install(WindowPlugin::new("Game", 1280, 720))?;
world.install(AssetPlugin::from_config(config))?;
let app = App::new(world);
```

### Strong And Weak Handles

Keep `Handle<T>` as the lightweight serializable id handle.

Add an optional strong handle or load ticket concept:

```rust
let ticket = assets.load_strong::<TextureAsset>(id)?;
let weak: Handle<TextureAsset> = ticket.handle();
```

The exact names can change, but the semantics should be clear:

- `Handle<T>` is identity and serialization.
- `AssetTicket<T>` or `StrongHandle<T>` is residency intent.
- Dropping the ticket releases the direct request.
- ECS components may still store lightweight handles.
- Scene/asset groups own tickets.

### Asset Groups

```rust
let group = assets.load_group(AssetGroupDesc::new("level_01").root(scene_id))?;
group.progress();
group.is_ready();
group.release();
```

Groups should support:

- root assets;
- explicit dependencies;
- labels/tags;
- readiness aggregation;
- progress;
- cancellation;
- release.

### Cook Registry

```rust
let mut registry = CookRegistry::default();
registry.register(TextureCooker);
registry.register(AudioCooker);
registry.register(VideoCooker);
registry.register(MeshCooker);
registry.register(StandardMaterialCooker);

cook::cook_all_with(&config, &registry)?;
```

The default `cook_all` can still use the built-in registry.

### Runtime Factory Registry

Keep the current runtime factory shape, but make factory registration easier to
bundle with cook registration where appropriate:

```rust
asset_server.register_factory(TextureAssetFactory);
asset_server.register_asset_plugin(TextureAssetPlugin);
```

An asset plugin should be local to the `asset` module and should not imply a
general engine plugin system.

### Readiness And Explanation

```rust
let readiness = assets.readiness(handle);
let explanation = assets.explain(handle.id());
let stats = assets.stats();
```

The explanation should include:

- current state;
- direct and dependency reference counts;
- last error;
- dependency states;
- queue position;
- pending reload state;
- package/source/cooked path if available.

## Implementation Phases

### Phase 0: Contract Cleanup

Purpose: make the current behavior accurately documented before changing it.

Tasks:

- Fix `docs/reference/asset.md` event examples to match actual `AssetEventKind`.
- Document that `Handle<T>` is a lightweight id and does not own residency.
- Document that `load()` must be balanced by `unload()` today.
- Document raw texture loading as a convenience path, not the canonical cooked
  path.
- Add a short "CPU asset vs GPU cache" section.
- Add a "known limitations" section that points to this plan.

Validation:

```bash
cargo test --features asset asset:: --lib
```

Acceptance:

- Docs match actual public API.
- No code behavior changes.

### Phase 1: Asset Plugin Configuration And Diagnostics

Purpose: make default app asset setup explicit and observable.

Tasks:

- Keep app asset setup explicit through `AssetPlugin`.
- Add or refine `AssetPlugin` builder methods for root, target, install budget, background loading, and strictness.
- Update `app::services::install_assets` to consume the `AssetConfig` installed by `AssetPlugin`.
- Emit structured diagnostics when default manifest loading fails.
- Add asset subsystem diagnostic ids:
  - `asset.manifest.missing`;
  - `asset.manifest.invalid`;
  - `asset.load.failed`;
  - `asset.dependency.missing`;
  - `asset.dependency.cycle`;
  - `asset.reload.failed`.
- Add `AssetStats` with queue counts, resident count, loading count, failed
  count, installed count, and event count.
- Expose stats through `AssetServer::stats()`.

Validation:

```bash
cargo test --features asset asset:: --lib
cargo test --features app app::
cargo check --examples --features app
```

Acceptance:

- App users can configure asset root without manually inserting `AssetServer`.
- Missing manifest produces one clear diagnostic instead of only `eprintln!`.
- Existing examples continue to run with default config.

### Phase 2: Lifetime Tickets And Load Intents

Purpose: remove the main footgun around copied handles and manual unload.

Tasks:

- Keep existing `Handle<T>` behavior for compatibility.
- Add `AssetTicket<T>` or `StrongHandle<T>` that owns one direct load request.
- Internally track request ids instead of only raw direct request count if this
  makes duplicate releases safer.
- Add `AssetServer::load_ticket<T>(id)`.
- Add `AssetServer::load_ticket_by_path<T>(path)`.
- Add explicit `release` for callers that cannot rely on `Drop`.
- Keep `load/unload` as lower-level compatibility APIs.
- Add tests for:
  - cloned lightweight handles do not affect residency;
  - dropping ticket unloads when no dependencies or other tickets remain;
  - multiple tickets keep asset alive until all are dropped;
  - dependency-held assets survive parent reload correctly.

Validation:

```bash
cargo test --features asset asset::server
```

Acceptance:

- New gameplay code can use tickets/groups and avoid manual unload pairing.
- Existing load/unload tests still pass.

### Phase 3: Asset Groups

Purpose: support scene and level loading as a first-class workflow.

Tasks:

- Add `AssetGroupDesc`.
- Add `AssetGroupTicket`.
- Add group roots by id and path.
- Track group-owned load tickets.
- Add progress aggregation:
  - total roots;
  - total discovered dependencies;
  - installed count;
  - failed count;
  - waiting count.
- Add group cancellation/release.
- Add a readiness API with failure reasons.
- Add app-facing examples:
  - preload a set of textures;
  - wait for a video clip and its frame textures;
  - level-style group smoke example.

Validation:

```bash
cargo test --features asset asset::group
cargo check --examples --features app
```

Acceptance:

- A loading screen can drive progress from one group handle.
- Releasing the group frees assets not referenced elsewhere.

### Phase 4: Cook Registry

Purpose: make offline asset conversion extensible.

Tasks:

- Split `src/asset/cook.rs` into a small module tree.
- Introduce `CookRegistry`.
- Introduce `AssetCooker` trait with:
  - supported extensions;
  - default meta;
  - meta normalization;
  - cook;
  - verify;
  - dependency discovery.
- Port built-in texture/audio/video cooking onto the registry.
- Keep `cook_all`, `cook_target`, `import_path`, and `verify` using the default
  registry.
- Add `cook_all_with`, `cook_target_with`, `import_path_with`, and
  `verify_with` for custom tooling.
- Make cooked manifest entries include cooked hash.
- Preserve current `.meta` file compatibility where possible.

Validation:

```bash
cargo test --features asset asset::cook
cargo check --bin sky-cook --features asset
cargo run --example asset_cook_smoke --features asset
```

Acceptance:

- Existing texture/audio/video cook tests pass through the registry.
- A test-only custom asset cooker can be registered without editing cook core.

### Phase 5: Mesh And Material Cooked Assets

Purpose: bring render assets onto the normal asset pipeline.

Tasks:

- Define cooked `MeshAsset` binary or JSON-plus-binary format.
- Define cooked `StandardMaterialAsset` format.
- Add dependency declaration for texture handles inside material assets.
- Add simple mesh source support first:
  - `.skymesh` JSON/binary descriptor; or
  - minimal glTF mesh import if the renderer path is ready.
- Add material source support:
  - `.skymat` descriptor;
  - albedo color;
  - optional albedo/normal/emissive textures;
  - alpha mode;
  - shadow participation flags.
- Register runtime factories for cooked mesh and material assets.
- Update wgpu/kajiya/renderling cache invalidation to respond to installed,
  reload, and unload events for mesh/material ids.
- Add examples using loaded mesh/material handles rather than only
  `insert_runtime`.

Validation:

```bash
cargo test --features app render::asset
cargo test --features app render::runtime::tests
cargo check --examples --features app
```

Acceptance:

- A mesh/material pair can be imported, cooked, loaded, uploaded, rendered, and
  reloaded.
- Material texture dependencies load transitively.

### Phase 6: Loading Queue Rewrite

Purpose: replace thread-per-load with a bounded, observable queue.

Tasks:

- Add `AssetLoadQueue`.
- Add worker pool configuration:
  - worker count;
  - max IO jobs;
  - max decode jobs;
  - priority policy.
- Add load priorities:
  - background;
  - preload;
  - imminent;
  - visible.
- Add cancellation tokens for unreferenced jobs.
- Split IO, decode, dependency wait, and install stages.
- Ensure install still runs on the main/update thread unless a factory declares
  it is thread-safe to install elsewhere.
- Make texture loads use the same queue as other assets.
- Avoid holding the global server mutex while doing file IO or decode work.
- Add queue stats and diagnostics.

Validation:

```bash
cargo test --features asset asset::server
cargo test --features asset asset::load_queue
```

Acceptance:

- Heavy load bursts do not spawn unbounded threads.
- Visible/high-priority assets can overtake background loads.
- Unreferenced queued assets can be cancelled before decode/install.

### Phase 7: Hot Reload Service

Purpose: make local iteration automatic.

Tasks:

- Add optional file watcher dependency behind an asset development feature.
- Watch source files, meta files, cooked files, and manifest.
- Debounce rapid changes.
- Add `AssetReloadPolicy`:
  - disabled;
  - cooked-only;
  - recook-and-reload.
- Integrate `sky-cook` functions for recook in development mode.
- Emit reload events with clear diagnostics.
- Requeue dependents when a dependency changes.
- Make render/audio/video caches react to reload events.
- Add example flags for live texture reload.

Validation:

```bash
cargo test --features asset asset::reload
cargo check --examples --features app
```

Acceptance:

- Editing a texture source can update a running sprite demo after recook.
- Editing a material descriptor can update a running mesh demo.
- Reload failure keeps the last installed good asset when possible.

### Phase 8: GPU Residency Policy

Purpose: make render asset memory predictable.

Tasks:

- Add GPU texture cache budget.
- Track byte sizes for resident textures.
- Add LRU or priority-aware eviction.
- Allow pinning textures from asset groups.
- Add sampler cache keyed by sampler descriptor.
- Add mipmap generation for eligible textures.
- Add compressed texture formats to cook pipeline when target supports them.
- Add texture array/atlas support only after the basic budget path is stable.
- Add render asset stats to diagnostics and agent reports.

Validation:

```bash
cargo test --features app render::runtime::tests::texture_assets
cargo test --features app render::resources
cargo check --examples --features app
```

Acceptance:

- GPU cache does not grow without a policy.
- Visible textures are not evicted before background textures.
- Reload/replacement does not leak stale GPU objects.

### Phase 9: Packages And Virtual IO

Purpose: prepare for shipped builds, mods, and web.

Tasks:

- Add `AssetIo` trait:
  - read file;
  - read range;
  - exists;
  - metadata/hash where available;
  - watch support where available.
- Add native filesystem implementation.
- Add package file implementation.
- Add layered mount implementation.
- Add package manifest:
  - package id;
  - target;
  - asset entries;
  - compressed size;
  - uncompressed size;
  - hashes;
  - dependencies;
  - build version.
- Add mount order for game, DLC, and mods.
- Add strict package verification mode.
- Add wasm/web IO backend plan once native package path is stable.

Validation:

```bash
cargo test --features asset asset::package
cargo test --features asset asset::io
```

Acceptance:

- Runtime loading can read from filesystem or package without changing
  gameplay code.
- Asset ids remain stable across package boundaries.

### Phase 10: Tooling And Reports

Purpose: make assets easy to debug for humans, CI, and agents.

Tasks:

- Extend `sky-cook verify` with structured JSON output.
- Add `sky-cook graph` to print dependency graph.
- Add `sky-cook explain <path|id>`.
- Add `AssetServer::explain`.
- Add asset status dump for app diagnostics.
- Integrate with future `sky-agent` report fields:
  - manifest valid;
  - missing dependencies;
  - failed assets;
  - loading queue;
  - render missing assets.
- Add docs for common failure modes.

Validation:

```bash
cargo check --bin sky-cook --features asset
cargo test --features asset asset::
```

Acceptance:

- A missing or failed asset can be diagnosed from one command or report.
- CI can fail on invalid manifests before runtime.

## Migration Strategy

Do not rewrite everything at once.

Recommended landing order:

1. Contract cleanup and docs.
2. App asset config and diagnostics.
3. Lifetime tickets.
4. Asset groups.
5. Cook registry.
6. Mesh/material cooked assets.
7. Load queue rewrite.
8. Hot reload.
9. GPU cache budgets.
10. Packages and virtual IO.
11. Tooling reports.

Compatibility rules:

- Keep `Handle<T>` lightweight and serializable.
- Keep `AssetServer::load`, `load_by_path`, `unload`, `insert_runtime`, and
  `replace_runtime` until all examples and modules have a better path.
- Add new APIs beside old ones first.
- Migrate examples to the new ergonomic APIs after tests cover them.
- Remove old low-level APIs only after a separate deprecation decision.

## Test Matrix

Core asset tests:

```bash
cargo test --features asset asset:: --lib
```

Cook tool:

```bash
cargo check --bin sky-cook --features asset
cargo run --example asset_cook_smoke --features asset
```

App and render integration:

```bash
cargo test --features app render::runtime::tests::texture_assets
cargo test --features app render::asset
cargo check --examples --features app
```

Audio:

```bash
cargo test --features audio audio::
cargo check --example audio_demo --features "app audio"
```

Video:

```bash
cargo test --features video video::
cargo check --example video_demo --features video
```

Future package/hot reload tests:

```bash
cargo test --features asset asset::package
cargo test --features asset asset::reload
```

## Risk Register

### Lifetime API Complexity

Risk: adding strong handles or tickets can confuse users if `Handle<T>` still
exists.

Mitigation:

- Use docs and names to distinguish identity from residency.
- Keep examples on the safer API.
- Reserve lightweight handles for serialized data and ECS components.

### Cook Registry Over-Abstraction

Risk: a generic cook trait can become too complex before mesh/material needs are
clear.

Mitigation:

- Port only texture/audio/video first.
- Add custom test cooker.
- Add mesh/material only after the registry has proven useful.

### Hot Reload Invalid States

Risk: failed reload can leave runtime caches with missing or mismatched data.

Mitigation:

- Keep last good installed asset where possible.
- Emit failed reload event without unloading old asset unless the asset was
  explicitly removed.
- Add render cache tests for replacement and unload.

### Worker Queue Deadlocks

Risk: dependency waits and cancellation can deadlock if jobs hold locks while
waiting.

Mitigation:

- Never hold server mutex across IO/decode.
- Keep dependency evaluation in update.
- Add tests for dependency cycles, cancellation, and reload while loading.

### Backend Cache Drift

Risk: Wgpu, Kajiya, and Renderling can handle reloads differently.

Mitigation:

- Keep backend-neutral asset events.
- Add backend-specific cache tests.
- Require event handling for texture, mesh, and material asset ids.

## First Vertical Slice

The smallest useful slice is:

1. Fix `docs/reference/asset.md` event/lifetime wording.
2. Add `AssetStats`.
3. Add or refine explicit `AssetPlugin` configuration helpers.
4. Add `AssetTicket<T>`.
5. Add tests proving ticket lifetime.

This slice improves correctness and ergonomics without changing cooked formats
or renderer internals.

Validation for first slice:

```bash
cargo test --features asset asset:: --lib
cargo test --features app app::
cargo check --examples --features app
```

## Definition Of Done

The asset resource system can be considered production-ready when:

- custom asset types can register both runtime factory and cooker without
  editing core cook switches;
- app asset configuration is explicit;
- gameplay examples use ticket/group-style residency instead of manual
  load/unload pairing;
- texture, audio, video, mesh, and standard material assets have cooked paths;
- hot reload works for texture and material edits in at least one running demo;
- asset status and failures are visible through structured diagnostics;
- render caches handle reload/unload/replacement without leaking stale GPU
  resources;
- cooked output can be verified before shipping;
- package or virtual IO support exists for non-trivial distribution;
- tests cover dependency loading, unload, reload, failure, cycle detection,
  group readiness, and cache invalidation.
