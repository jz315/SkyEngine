# World Resource Governance Plan

## Purpose

This document defines how SkyEngine should use ECS `World` resources after the asset/resource-system rewrite.

The current problem is not that `World` resources are bad. The problem is that `World` is currently used for too many unrelated ownership roles:

- user-facing singleton state,
- subsystem public services,
- subsystem private internals,
- backend GPU/audio/video caches,
- plugin installation sentinels,
- command handles,
- and temporary borrow-workaround storage.

That makes ownership, initialization order, lifetime, and API intent hard to reason about.

The target is a smaller, clearer rule:

> `World` may contain gameplay state and public subsystem facades. It must not become a dumping ground for backend internals, install markers, caches, or arbitrary private stores.

## Non-Goals

- This is not a proposal to remove ECS resources.
- This is not a proposal to make every service global.
- This is not a proposal to hide all engine state outside `World`.
- This is not a compatibility-preserving migration plan. The preferred direction is a clean break where the old mixed patterns are retired.

## Resource Classes

### 1. Public ECS Resources

Public ECS resources are typed singleton state that game code is expected to read, write, or configure.

Examples:

- `Input`
- `InputActions`
- `InteractionContext`
- `RenderSettings`
- user-defined game state resources

Rules:

- Public ECS resources may live directly in `World`.
- They must have clear documentation describing who creates them and who updates them.
- They should be cheap to reason about from a system function.
- They should not hide large backend ownership graphs unless they are explicitly a facade.

### 2. Subsystem Facades

A subsystem facade is the single public resource representing a whole engine subsystem.

Examples:

- future `Assets`
- existing `VnResource`
- possible future `Audio`
- possible future `Video`
- possible future `Ui`
- possible future `Physics2D`

Rules:

- A subsystem should expose at most one primary public facade resource.
- Internal stores, queues, registries, diagnostics, events, and caches should be fields inside the facade or owned by the backend behind it.
- Users should not need to manually install or coordinate the facade's internals.
- Facades may provide scoped accessors for advanced use, but those accessors should preserve the facade as the ownership boundary.

### 3. Private Runtime State

Private runtime state is implementation detail required by a subsystem.

Examples from the current codebase:

- `TileRuntime`
- legacy UI state bundles such as `UiState`, `UiEvents`, `UiFontBook`
- physics installation markers
- low-level loader queues or registries

Rules:

- Private runtime state should not be installed as many independent public `World` resources.
- It should be grouped under the subsystem facade.
- If a subsystem genuinely needs multiple internal stores, they should remain private fields or private modules.
- Tests may inspect internals through crate-private helpers, not by relying on public `World` resource layout.

### 4. Backend Caches

Backend caches are runtime caches tied to a rendering, audio, video, or platform backend.

Examples from the current codebase:

- `SharedRenderAssetCache`
- `UiRenderer`
- possible future GPU texture/material/mesh residency caches
- audio decode/playback backend state
- video decode queues and frame pools

Rules:

- Backend caches should not be general `World` resources.
- Renderer-owned caches should live under the renderer or render runtime.
- UI renderer caches should live under the UI backend.
- Audio/video backend caches should live under their subsystem service/facade or backend implementation.
- `World` may contain stable IDs, handles, components, and user-facing settings that refer to backend resources, but not the backend cache itself.

### 5. Command Handles

Command handles are user-facing write APIs that enqueue work into an engine service.

Examples from the current codebase:

- `AudioCommands`
- `VideoCommands`

Rules:

- Prefer command methods on the subsystem facade when there is a clear facade.
- A separate command resource is acceptable only when it is intentionally a lightweight user-facing handle.
- Command handles must not be the only way to discover or initialize the subsystem.
- Command handles should not duplicate lifecycle ownership held elsewhere.

### 6. Install Markers

Install markers are resources whose only job is to remember whether a plugin or subsystem has been installed.

Examples from the current codebase:

- `PhysicsInstalled2D`
- `PhysicsDebugDrawInstalled2D`

Rules:

- Avoid public install-marker resources.
- Prefer idempotent facade installation.
- If a marker is absolutely needed, keep it crate-private and document why ordinary ownership cannot express the state.
- Do not expose install markers as part of the engine's conceptual API.

## Target Ownership Model

The target model is:

```text
App
  owns event loop, window, frame lifecycle, service update order

World
  owns ECS entities, components, public resources, subsystem facades

Subsystem Facade
  owns subsystem runtime internals, queues, registries, events, diagnostics

Backend / Runtime
  owns platform/backend caches and execution objects
```

For example:

```text
World
  Assets
    AssetDatabase
    AssetStore
    AssetRegistry
    AssetLoadQueue
    AssetEvents

RenderRuntime
  RenderAssetCache
  GpuScene
  graph/runtime caches

World
  Ui
    active backend
    public UI config/state facade

UiBackend
  renderer cache
  font atlas GPU cache
```

## Concrete Migration Plan

### Phase 1: Establish the Standard

1. Add this governance plan.
2. Audit all built-in resources and classify each as:
   - keep as public ECS resource,
   - move under facade,
   - move under backend/runtime,
   - delete/replace marker,
   - keep temporarily for migration.
3. Add a short resource ownership table to the relevant subsystem plan before changing that subsystem.

Definition of done:

- Every engine-owned resource has an assigned class.
- New subsystem work can cite this document before adding a new `World` resource.

### Phase 2: Asset System Cleanup

1. Replace `AssetServer` with the planned `Assets` facade.
2. Move asset database, stores, queues, registry, events, and diagnostics under `Assets`.
3. Remove scattered `AssetServer` lazy initialization.
4. Make asset factory registration explicit through asset module/plugin installation, not audio/video server constructors.
5. Keep `Handle<T>` as weak identity and `AssetRef<T>` as strong runtime lifetime.

Definition of done:

- There is one public asset facade in `World`.
- Audio/video/render code does not create or secretly mutate asset-system registration as a side effect of server construction.

### Phase 3: Render Cache Boundary

1. Move `SharedRenderAssetCache` ownership out of `World`.
2. Place render asset residency under `RenderRuntime` or a renderer-owned cache object.
3. Pass explicit cache access into UI/render overlay paths instead of having UI backends fetch it from `World`.
4. Keep `RenderSettings` as a public ECS resource because it is user-facing configuration.

Definition of done:

- Render runtime no longer uses `World` as a service locator for GPU caches.
- UI render paths no longer insert render caches into `World`.

### Phase 4: UI Resource Consolidation

1. Introduce a single UI facade resource or make `UiHost` the explicit facade.
2. Move legacy UI internals under the facade/backend:
   - config,
   - theme,
   - state,
   - events,
   - font book,
   - renderer.
3. Align legacy and neo backend ownership:
   - backend owns renderer/backend cache,
   - facade owns public UI state and backend registration.
4. Remove remove/reinsert patterns where possible by reshaping APIs around facade methods or scoped backend execution.

Definition of done:

- Legacy and neo UI follow the same ownership rule.
- UI does not expose a pile of independent implementation resources.

### Phase 5: Audio and Video Facades

1. Replace `AudioServer` + `AudioCommands` split with a clearer public `Audio` facade, or document `AudioCommands` as the only public write handle if the split is kept.
2. Do the same for video.
3. Move backend/decode/playback state behind the facade/backend.
4. Keep asset type registration independent from server construction.

Definition of done:

- User code has one obvious place to control audio.
- User code has one obvious place to control video.
- Backend state is not exposed as ordinary ECS resources.

### Phase 6: Physics Resource Cleanup

1. Replace `PhysicsInstalled2D` and `PhysicsDebugDrawInstalled2D` with idempotent installation state hidden inside a facade or plugin install path.
2. Group physics world, events, debug draw state, and config under a public `Physics2D` facade if physics remains a built-in subsystem.
3. Remove remove/reinsert resource mutation where feasible.

Definition of done:

- Physics exposes one coherent public resource.
- Installation markers are not part of the public resource model.

### Phase 7: Tile Runtime Split

1. Keep high-level tile scene/editor/runtime state under the tile subsystem.
2. Keep low-level render tilemap storage under render runtime or an explicit render tilemap facade.
3. Document the bridge from `tile::TileMap` to render tilemap payloads.
4. Avoid having both high-level tile runtime and low-level render storage look like equal public game resources.

Definition of done:

- Game/editor code uses the high-level tile API.
- Render-only tilemap storage is clearly renderer-facing.

## Borrowing Policy

Remove/reinsert should be treated as a smell for engine-owned resources.

Allowed cases:

- isolated tests,
- short transitional code during migration,
- genuinely movable one-shot resources.

Disallowed as a permanent pattern:

- removing a subsystem owner from `World`,
- mutating it while also passing `&mut World`,
- reinserting it after execution.

Preferred alternatives:

- facade methods that own the internal borrow,
- split read-only frame inputs from mutable backend state,
- command queues,
- scoped execution APIs,
- moving backend state out of `World`.

## Naming Policy

Resource names should reveal their role:

- public facades: simple nouns, such as `Assets`, `Audio`, `Video`, `Ui`, `Physics2D`.
- user config: explicit config names, such as `RenderSettings`.
- private internals: module-private names, not public resource API.
- backend caches: backend/runtime-owned names, not inserted into `World`.

Avoid public resource names that expose implementation mechanics:

- `Shared*Cache`
- `*Installed`
- `*Runtime` when it is private subsystem state
- `*Storage` when it is a low-level backend store

## Configuration Policy

Configuration must follow plugin ownership, not app centralization.

### App Capability Plugins

App-owned capabilities are installed explicitly through `World::install`.
The plugin constructor is the configuration surface:

```rust
let mut world = World::new();

world.install(WindowPlugin::new("Game", 1280, 720))?;
world.install(RunnerPlugin::game().with_frame_rate_limit(120.0))?;
world.install(InputPlugin)?;
world.install(AssetPlugin::new("assets"))?;
world.install(RenderPlugin::forward_2d())?;

App::new(world).run(Game);
```

Rules:

- Do not introduce a new `configure(...)` API.
- Do not reintroduce a central app config object with subsystem-specific fields.
- Do not add a `DefaultPlugins` bundle while the capability boundaries are still being clarified.
- Required configuration for a capability belongs on that capability plugin's constructor or builder methods.
- `App` discovers installed capabilities and owns heavy runtime lifetimes; it does not decide that every app must have window, render, audio, video, or UI.

Current app capability plugins:

- `WindowPlugin` installs `WindowOptions`.
- `RunnerPlugin` installs `RunnerOptions`.
- `InputPlugin` enables ECS input resource synchronization.
- `AssetPlugin` installs `AssetConfig`; the runner creates the public `Assets` facade.
- `RenderPlugin` installs the selected `RenderPipelineAsset`.
- `AudioPlugin` installs `AudioConfig` and depends on `AssetPlugin`.
- `VideoPlugin` enables video services and depends on `AssetPlugin`.

### Plugin Dependencies

Plugins own their local dependency checks.

Examples:

- `AudioPlugin` requires `AssetPlugin`.
- `VideoPlugin` requires `AssetPlugin`.
- A rendered UI plugin may require `InputPlugin` and `RenderPlugin`.

Rules:

- Do not centralize all dependency validation in `App`.
- The app runner should only execute capabilities that have been installed.
- Missing optional capabilities should mean absent behavior, not a startup error.
- If a plugin has a hard dependency, it should fail during `world.install(...)` with a local, actionable error.

### World Installation Facts

`World` records plugin installation facts for duplicate detection and dependency checks.

Rules:

- Installation facts are not user configuration.
- Installation facts are not a manifest that decides app policy.
- Public runtime resources should not be used as hidden installation markers.

### Plugin Configuration

World-local plugin configuration belongs on the plugin value.

Examples:

- `UiPlugin { config: UiConfig }`
- `NeoUiPlugin { config: NeoUiConfig }`
- `PhysicsPlugin { config: PhysicsConfig2D }`
- `PhysicsDebugPlugin { options: PhysicsDebugDrawOptions2D }`

Rules:

- Plugin config should be passed at installation time.
- Plugin config may be copied into a public facade resource when users need runtime mutation.
- Plugin config should not be mirrored into a central app config object.

### Runtime Settings

Runtime settings that game systems are expected to mutate while the app is running may stay as public ECS resources.

Examples:

- `RenderSettings`
- user game settings resources

Rules:

- Runtime settings should be explicitly documented as mutable ECS state.
- Runtime settings should not secretly initialize backend caches or private stores.

## Review Checklist for New World Resources

Before adding an engine-owned `World` resource, answer:

1. Is this state meant for game systems to access directly?
2. Is it a public facade for a subsystem?
3. Who creates it?
4. Who updates it?
5. Who owns its lifetime?
6. Does it contain backend-specific state?
7. Is it only an installation marker?
8. Is it only there to avoid a borrow conflict?
9. Could it be a field inside an existing facade?
10. Could it be owned by App, RenderRuntime, UiBackend, AudioBackend, or VideoBackend instead?

If the answer to 6, 7, or 8 is yes, it probably should not be a public `World` resource.

## Recommended First Cut

Do not try to fix every subsystem at once.

The best first sequence is:

1. Asset facade rewrite.
2. Move render asset cache out of `World`.
3. Consolidate UI resource ownership.
4. Clean up audio/video facade boundaries.
5. Clean up physics markers and borrow workarounds.
6. Clarify tile high-level/runtime/render storage boundaries.

This sequence reduces the most visible confusion first because asset/render/UI currently cross each other through `World` the most.

## Final Target

After the migration, reading engine code should feel like this:

- `World` contains gameplay ECS state and a small number of obvious subsystem facades.
- Each subsystem facade owns its internals.
- Backend caches live with backends.
- App lifecycle owns update order.
- Systems do not need to know where private queues, registries, caches, and install markers live.
- Users do not manually manage asset, audio, video, UI, or render cache lifetimes.
