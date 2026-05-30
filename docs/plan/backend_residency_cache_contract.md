# Backend Residency Cache Contract

Status: active design contract
Scope: renderer/audio/video/native-resource caches that consume `sky_engine::asset` events while keeping device/backend residency outside asset core.

## Goal

SkyEngine asset core owns identity, typed handles, CPU/runtime payload lifecycle, dependencies, leases, semantic events, request progress, and diagnostics. Backend residency caches own native resources derived from those assets: GPU textures, GPU meshes, GPU materials, decoded or streaming audio handles, video frame queues, and future backend-specific slabs.

This contract captures the Phase 6 rule from the Sakura resource-system adaptation plan: standardize backend residency behavior without introducing a central `ResourceSystem` god class and without forcing every backend cache behind one trait too early.

## Ownership Boundary

- `Assets` owns CPU/runtime asset state and emits semantic `AssetEvent`s.
- Backend caches own device objects, upload/decode/playback residency, cache eviction, native-memory accounting, and backend-specific failure cache entries.
- `Assets` must not store `wgpu`, audio backend, video decoder, render slab, or native-device handles.
- Backend caches may hold strong typed asset handles only when backend behavior needs CPU asset residency to remain alive. GPU/native eviction must not imply asset-core unload.
- Backend caches may expose stats snapshots, but native memory pressure policy remains backend-local.

## Event Semantics

Every backend cache that consumes asset events should maintain its own `AssetEventCursor` or equivalent cursor state. Shared global event-draining is avoided because each backend has different residency semantics.

Required event handling:

- `Loaded`: no-op for native residency. CPU payload exists, but install/backend residency is not guaranteed.
- `Installed`: resync the backend cache only if the current CPU/runtime source differs from the cached source.
- `Reloaded`: same as `Installed`, but treated as an explicit reload completion for diagnostics.
- `ReloadQueued`: invalidate or quarantine stale native residency for affected assets before the new payload installs.
- `Unloaded`: release backend-owned native residency associated with the asset.
- `Failed` with `event.state == AssetState::Installed`: preserve existing backend residency. This represents a failed reload where asset core kept the last good payload.
- Other `Failed`: release affected backend residency and clear stale failure-prone cache entries.

Event filtering:

- Backends must filter by `event.asset_type` before acting.
- Empty `asset_type` should be treated as conservative/legacy context only where old records can still emit it.
- Dependent caches, such as material caches that reference texture assets, may react to dependency asset events, but that policy belongs in the backend cache.

## Source Identity

Caches should not infer staleness from `AssetId` alone. A cache entry should retain enough source identity to tell whether an installed payload changed:

- CPU asset pointer identity when the cache is fed by `Arc<T>`.
- Asset generation and content/manifest hashes from `AssetEvent` when pointer identity is insufficient.
- Backend-specific dependency keys for composite resources such as materials.

On `Installed` / `Reloaded`, reuse existing native residency only if the cached source is still current. Otherwise remove or rebuild the backend entry locally.

## Budget And Eviction

Eviction is a backend policy, not an asset-core policy.

- GPU texture budgets, LRU order, pins, and upload failures live in render asset caches.
- Mesh/material slab eviction lives in render backend caches.
- Audio buffer/stream residency lives in `AudioServer`.
- Video source/frame residency lives in `VideoServer`.

Evicting native residency must leave `Handle<T>` and `Assets` state unchanged. A later draw/playback/sync path may recreate backend residency from the installed CPU/runtime asset.

Pinned backend entries are backend-owned pins. They are not strong asset handles unless the backend explicitly needs CPU asset residency.

## Failure And Diagnostics

Backend caches should cache prepare/upload/decode failures by current source identity so they do not retry every frame for the same bad payload.

Diagnostics should expose backend-local facts:

- resident entry count.
- resident native bytes where measurable.
- queued prepare/upload count where applicable.
- failed native-prepare count or last failure context where applicable.
- evictions and uploads when useful for frame diagnostics.

Asset diagnostics should not become the owner of those native details. Asset events provide the trigger; backend stats explain backend residency. App-facing diagnostic events may mirror those backend stats as separate category events, such as `render.stats`, `audio.stats`, or `video.stats`, and may emit backend-category warnings such as `render.asset.failed`, `audio.play.failed`, or `video.play.failed`, without routing the fields through `AssetStats`.

## Implementation Checklist

When adding a backend residency cache, verify:

- The cache has a local event cursor or documented event-consumption path.
- `Loaded` is not treated as native-ready.
- `Installed` and `Reloaded` resync only changed sources.
- `ReloadQueued`, `Unloaded`, and real `Failed` remove stale native residency.
- Failed reloads that preserve last good installed assets do not destroy native residency.
- Cache eviction does not release asset-core strong handles unless that is an explicit backend lease policy.
- Native prepare failures are cached per current source and do not spin every frame.
- Tests cover event invalidation, reload replacement, failure preservation, and backend-local eviction behavior.

## Current Mapped Caches

- `RenderAssetCache` / `SharedRenderAssetCache`: texture GPU residency, memory budget, LRU, pin/unpin, cached prepare failures, asset-event invalidation, resident/upload byte accounting from GPU texture metadata where available, per-frame upload/eviction counts/bytes, and current cached-prepare-failure count. App diagnostics mirror renderer-owned `RenderStats` through `render.stats`, failure-count changes through `render.asset.failed`, upload bursts through `render.asset.uploaded`, and eviction bursts through `render.asset.evicted`.
- `WgpuRenderAssetCache`: WGPU mesh/material residency, source-aware invalidation, and backend-local resident mesh/material counts folded into renderer stats.
- `RenderlingSceneRenderer` asset caches: Renderling mesh/material slabs and source-aware invalidation.
- `KajiyaRenderAssetCache`: Kajiya mesh/material/texture-dependent scene residency.
- `AudioServer`: direct playback and ECS-emitter audio asset event policy, plus backend-local stats for configured buses, live backend instances, spatial instances, direct playback, emitter bindings, failed play-request count, and last play-failure context. App diagnostics mirror changes as `audio.stats` and failed-start deltas as `audio.play.failed`.
- `VideoServer`: video clip playback/frame-source refresh and stop policy, plus backend-local stats for playback state counts, distinct clips, current frame textures, current frame texture bytes, failed play-request count, and last play-failure context. App diagnostics mirror changes as `video.stats` and failed-start deltas as `video.play.failed`.
- `GpuVideoFrameBuffer`: streamed video GPU texture ownership. It reuses `Texture::resident_bytes()` for a local resident-byte estimate of its own stable frame texture without routing streamed frame residency through `Assets` or `AssetStats`.

Future mesh, material, audio, video, Live2D, particle, and text residency paths should follow this contract before adding new shared abstractions.
