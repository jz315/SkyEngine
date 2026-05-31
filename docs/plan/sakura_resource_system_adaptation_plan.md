# SakuraEngine Resource System Adaptation Plan

Status: implementation in progress
Scope: compare SakuraEngine's resource system with SkyEngine's current asset/runtime design, identify gaps, and define a staged plan for borrowing the useful parts without copying unsuitable C++ architecture.

## 0. Implementation Progress

Last updated: 2026-05-27

Implemented so far:

- Phase 1 partial: `Assets::load_blocking` no longer depends on a fixed 64-iteration update loop. It now drives a blocking-specific update path, sleeps between pending polls, and adds `load_blocking_with_timeout`.
- Phase 1/server-boundary partial: blocking-load deadline, polling, timeout, and terminal-state error mapping now live in `src/asset/blocking.rs` through `deadline_from_timeout` / `drive_until_ready`. `server.rs` keeps only the facade handle acquisition and per-tick record driving hook, avoiding another coordinator class.
- Phase 1 partial: asset failures now record a queryable `AssetFailurePhase` through `Assets::failure_phase` / `failure_phase_untyped`, covering read/dependency/install failures.
- Phase 1/5/7 verification slice: blocking loads now have provider-seam regressions for memory-provider resolve and read failures. Both paths record the correct lookup/read failure phase, emit failed events, retain failed request snapshots, and keep the blocking path covered as provider implementations evolve.
- Phase 2 initial slice: background asset loads now submit to an internal bounded `AssetIoService` worker pool instead of spawning one OS thread per resource load. Queue-full submissions are deferred to later updates.
- Phase 2 partial: I/O worker count and queue capacity are now configurable through `AssetConfig` and `AssetPlugin`.
- Phase 2 partial: `AssetIoService` now uses a bounded priority queue with stable FIFO ordering inside equal priorities. `AssetConfig::with_io_default_priority` / `AssetPlugin::with_io_default_priority` wire request priority into background load submission.
- Phase 2 partial: `AssetConfig::with_io_shutdown_timeout` / `AssetPlugin::with_io_shutdown_timeout` now configure asset worker teardown. `AssetIoService` closes the queue on drop, discards queued-but-not-started jobs, and either joins running workers indefinitely or detaches after the configured timeout instead of leaving shutdown behavior implicit.
- Phase 2 partial: `AssetIoCancelToken` now lets the worker queue skip jobs that are canceled before they start. `AssetLoadQueue` maps inflight asset/generation loads to cancel tokens, and `AssetsInner` cancels no-longer-loading inflight work after release/state advancement, while already-running file/factory work may still finish and be discarded by generation/state checks.
- Phase 2 partial: canceled queued I/O jobs are now pruned before bounded queue capacity checks, so canceled work no longer causes a misleading `QueueFull` back-pressure response while diagnostics report an empty worker queue.
- Phase 2 stress slice: a public `Assets` burst-load regression now requests 12 assets against a 2-worker / 3-queue background configuration and verifies in-flight source work never exceeds worker-plus-queue capacity, with remaining loads deferred. This proves the facade path is using bounded I/O/backpressure instead of recreating thread-per-load behavior, without racing on a transient worker/queue snapshot.
- Phase 3 initial slice: asset load requests now live in `src/asset/request.rs` with an internal `AssetRequestPhase` boundary, preparing the later request-state-machine extraction from `server.rs`.
- Phase 4 partial: runtime factories now use `begin_install -> AssetInstallResult`, with `Ready` for immediate installs and `Pending(AssetInstallTask)` for cross-frame installs. The old erased synchronous install path has been removed.
- Phase 4 partial: `SoundClipFactory` and `MusicTrackFactory` now use real `AssetInstallTask`s. Audio decode/stream validation is deferred from `begin_install` into budget-aware task polling, and a `SoundClip` asset-server regression proves the real audio path advances from `Installing` to `Installed` across updates.
- Phase 4 partial: runtime factories now also have an optional `uninstall` lifecycle hook plus `AssetUninstallContext`. The record driver calls the factory hook while advancing `Uninstalling -> Unloading`, and runtime-inserted assets stay on the plain Rust drop path instead of requiring a central factory or god-class resource manager.
- Phase 4/7 partial: install-stage precondition failures now use the same install failure lifecycle as factory `begin_install` / task polling failures. Missing install payloads and other install setup errors record `AssetFailurePhase::Install`, emit `Failed` events, and retain failed request snapshots instead of escaping with an unannotated `Installing` record.
- Phase 2/3 partial: background load completions for released assets are now covered by an explicit regression test; stale completion discard is enforced through generation mismatch.
- Phase 2/3/7 partial: background load submission failures now enter the same failure lifecycle as synchronous load failures. Pre-worker errors such as missing runtime factories record `failure_phase`, emit `Failed` events, and retain failed request snapshots instead of leaving records stuck in `Loading`.
- Phase 7 initial slice: `Assets::stats()` now exposes queue depth, in-flight loads, retained events, reference counts, and per-state record counts.
- Phase 1/7 partial: `AssetEvent` now carries `failure_phase`, so `Failed` events directly explain lookup/read/decode/dependency/install/uninstall/runtime/verification failures, including failed reloads that keep the last good installed payload. Server regressions now cover failed reload decode events and failed factory uninstall events.
- Phase 5 initial slice: `src/asset/provider.rs` now owns local source resolution. Cooked and raw runtime loads resolve a `ResolvedAssetSource` before reading bytes, giving future package/memory providers a real integration point.
- Phase 5 partial: `src/asset/provider.rs` now also owns record-to-manifest-entry and record-to-source resolution through `record_manifest_entry` / `resolve_record_source`, including raw texture/font manifest-entry fallback. `server.rs` still selects factories and schedules loads, but no longer has private `entry_for_record` / `resolve_record_source` helpers.
- Phase 5 partial: raw runtime path normalization now lives in `provider::RawSourceRequest`, so `server.rs` no longer owns local path/key canonicalization for texture/font raw requests.
- Phase 5 partial: `src/asset/registry.rs` now owns `AssetFactories`, factory registration, manifest-entry factory lookup, typed product validation, and built-in raw texture/font factory product checks. `server.rs` no longer directly stores the factory map or hand-rolls factory product-type mismatch checks.
- Phase 5 partial: `src/asset/registry.rs` now also owns manifest source-path lookup and manifest refresh reconciliation through `ManifestIndex::lookup_source_asset`, `refresh_manifest_records`, and `refresh_manifest_records_or_fail`, including manifest index replacement, manifest-bound record asset-type refresh, runtime/raw record skip rules, and missing-manifest lifecycle failure/events/request snapshots for records that disappeared from the manifest.
- Phase 5 partial: manifest file loading and version validation now live in `registry::load_manifest`, keeping registry persistence rules out of `server.rs`.
- Phase 4 partial: install tasks now receive an `AssetInstallBudget`; `AssetConfig::with_install_time_budget` and `AssetPlugin::with_install_time_budget` can opt into per-update install time slicing, while blocking loads use an unlimited install budget to preserve explicit blocking semantics.
- Phase 2/3 partial: queued requests whose last strong/dependency lease is gone before activation are canceled before any I/O submission; `Assets::stats()` now reports cumulative `canceled_requests`.
- Phase 5 partial: resolved asset sources can be backed by test memory bytes/read failures/delayed reads, and `Assets` has a test-only provider injection path so provider/request tests can cover success, provider resolve failure, source read failure, delayed background in-flight behavior, dependency chains, release cascades, and failed request snapshots without manufacturing files for every scenario.
- Phase 5 partial: `AssetConfig::with_package_root` / `with_package_roots` now mount directory package roots for cooked artifacts. `LocalAssetProvider` prefers loose cooked files and falls back to package roots without changing the `Assets` facade or making asset core own a VFS/DirectStorage implementation.
- Phase 5 partial: `AssetConfig::with_package_file` / `with_package_files` now mount simple indexed cooked bundle files. `LocalAssetProvider` can read bundle entries by manifest `cooked_path` after loose cooked files and package roots miss, giving Sky a package-file seam without moving package lifecycle into `Assets`.
- Phase 5/7 partial: `LocalAssetProvider` now owns a small package-file bundle index cache plus provider-local invalidation hooks. Watcher path changes, manifest reloads, rescan/full reload scans, and force reloads clear only provider-owned package lookup state, with manifest-refresh invalidation centralized at `set_manifest` so reload scans and forced reloads do not scatter duplicate provider-management calls through `server.rs`.
- Phase 5/7 partial: watch-event-to-provider-cache invalidation now lives in `provider::invalidate_from_watch_events`, so `server.rs` no longer translates watcher `Changed` / `Rescan` events into package cache mutation policy.
- Phase 5 partial: `src/asset/registry.rs` now exposes a focused internal `AssetRegistry` read model for manifest entry, metadata, dependencies, registry location resolution, cooked/source/package path lookup, and watch-key construction. Dependency, reload, and public watch-path snapshots consume this registry seam without making registry own loading, hot reload, backend residency, or package lifecycle.
- Phase 5/server-boundary partial: typed source-path resolution, typed request validation, and typed `AssetPath<T>` reconstruction now live in `src/asset/registry.rs` through `resolve_typed_source_asset`, `validate_typed_asset_request`, and `typed_asset_path`. `server.rs` no longer hand-composes manifest lookup plus factory product checks for typed entrypoints.
- Phase 5/server-boundary partial: `LocalManifestRegistry` now keeps its manifest and lookup indexes private behind registry-owned snapshot/read helpers. Public `Assets::manifest`, path resolution, metadata, and watch-path calls consume that read model instead of reaching into the manifest index representation from `server.rs`.
- Phase 5 partial: the local manifest-backed registry is now named explicitly as `LocalManifestRegistry` (a thin internal alias over `ManifestIndex`), and `AssetsInner` stores that role name instead of a generic index name. This aligns the code with the plan's registry/provider vocabulary without creating a new public registry facade or god-class loader.
- Phase 5 partial: runtime tooling now has public read-only `AssetMetadata` and `AssetWatchPaths` snapshots through `Assets::metadata` / `Assets::watch_paths`, exposing manifest metadata and registry-derived watch paths without making `Assets` own provider or watcher policy.
- Phase 5 partial: dependency and reload helpers now accept the `AssetRegistry` read interface instead of binding their signatures to `ManifestIndex`, keeping future package/editor/test registries out of `AssetServer` and avoiding a single god-class registry.
- Phase 5 partial: provider record-to-entry/source resolution and load orchestration now also accept the `AssetRegistry` read interface, so local manifest lookup stays replaceable while provider I/O, factory dispatch, and store mutation remain separate responsibilities.
- Phase 5 partial: `AssetRegistry::asset_type` is now the dependency asset-type query seam used by `server.rs`, dependency resolution, and reload failure handling. `AssetsInner` no longer hands store/lease/failure/install helpers closures that crack open manifest entries just to clone `asset_type`.
- Phase 5 partial: manifest loading now goes through an internal `AssetRegistryLoader` seam. `LocalManifestRegistryLoader` preserves the existing local manifest behavior, while tests can supply a static registry loader and memory provider without a local manifest file; `AssetsInner` uses the loader for initial and reload manifest refreshes.
- Phase 5/7 partial: `ManifestIndex::lookup_watch_asset` now resolves source, loose cooked, and package-root cooked watch paths to asset ids. Unknown paths and manifest/package-index level changes still request a scan, but ordinary package-root artifact writes no longer force `AssetServer` into a broad fallback path.
- Phase 7 partial: hot reload now has `Assets::reload_changed_with_report()`, returning direct changed roots separately from the full dependent reload closure.
- Phase 7 partial: `AssetConfig::with_auto_reload` / `AssetPlugin::with_auto_reload` now enable interval-based automatic reload checks during `Assets::update()`, with `Assets::last_reload_report()` exposing the most recent reload roots and dependent closure.
- Phase 7 partial: manual reload scans now return skipped explanations in `AssetReloadReport::skipped`, including unchanged content, missing manifest entries, and untracked records. Automatic polling still only enters pending reload when changed roots exist, so the extra explanation does not turn every idle scan into diagnostics noise.
- Phase 7 partial: app `asset.reload` diagnostics now publish reload explanation fields, including changed/impacted asset id lists, skipped reason counts, and `asset_id:reason` skipped details, so editor/dev tooling can inspect reload closure output without parsing logs or reaching into asset internals.
- Phase 9 partial: asset examples now cover `asset_load_texture`, `asset_hot_reload_texture`, `asset_load_with_dependency`, and `asset_custom_factory`. `README.md`, `README_zh.md`, `examples/README.md`, and `docs/reference/asset.md` point users at the same public `Assets` / strong `Handle<T>` / `AssetPath<T>` / custom factory surface instead of reviving older `AssetServer` or weak-handle terminology.
- Phase 9 partial: older broad asset planning docs that conflicted with current strong-handle semantics have been removed, so this Sakura adaptation plan plus the strong-handle and residency-cache plans remain the current planning authority.
- Phase 9 partial: `docs/plan/world_resource_governance_plan.md` now uses the current strong `Handle<T>` plus weak `WeakHandle<T>` / `AssetPath<T>` terminology in its active checklist/standard sections, instead of preserving the obsolete weak-`Handle<T>` / strong-`AssetRef<T>` target model.
- Phase 9 verification slice: the broad default and app-feature gates have been re-run on the current tree (`cargo test`, `cargo test --features app`, and `cargo check --examples --features app`), covering the public facade, asset internals, app diagnostics, render runtime asset paths, and example compatibility.
- Phase 1-5/7/8 verification slice: the current asset-feature suite has been re-run (`cargo test --features asset`; 257 unit tests plus the asset doctest), covering blocking loads, bounded I/O, request/lease/store/load/install/reload/provider/registry/cooking/diagnostics paths while keeping runtime residency outside asset core and avoiding a centralized resource god class.
- Phase 5/7 verification slice: the current watcher-enabled asset suite has been re-run (`cargo test --features asset-watch asset::`; 250 asset tests), covering file-watcher auto reload, external package roots/files, watch-path resolution, provider invalidation, and reload scan handoff without moving watcher policy into a monolithic resource manager.
- Phase 6 partial: `AssetEvent` now carries backend residency context: load generation, asset type string, manifest fingerprint, content hash, dependencies, and reload marker. Texture residency event handling ignores non-texture asset events.
- Phase 6 partial: `RenderAssetCache` now tracks resident GPU texture bytes and per-frame budget evictions, exposes resident/uploaded/evicted counts and byte totals through `RenderAssetStats` / app-facing `RenderStats`, and supports an optional texture memory budget with LRU eviction plus backend-owned texture pin/unpin. Pinned textures are skipped by budget eviction, and unpinning reapplies the budget without moving GPU residency into `asset`.
- Phase 6 partial: texture GPU preparation now validates dimensions and RGBA8 byte size before upload. Prepare failures are cached per current CPU asset generation/source, reported through `TextureReadiness::Failed`, render failed-asset stats, and `cached_failed_render_assets`, and do not requeue forever or masquerade as CPU-ready assets.
- Phase 6 partial: `WgpuRenderAssetCache` now consumes asset events for `MeshAsset` and `StandardMaterialAsset` residency. It keeps initial `Installed` events for the same CPU source, invalidates changed `Installed` sources, and removes cached GPU mesh/material handles on `ReloadQueued`, `Unloaded`, and real `Failed` events through a renderer-owned event cursor.
- Phase 6 partial: `WgpuRenderAssetCache` now exposes backend-local resident mesh/material counts, and `WgpuSceneRenderer::stats()` folds those resident entries into `RenderStats` alongside the existing texture cache stats. This keeps WGPU mesh/material residency diagnostics in the renderer backend instead of asset core.
- Phase 6 partial: `RenderlingSceneRenderer` mesh/material caches are now source-aware and consume asset events. Runtime replacements rebuild cached Renderling mesh/material slabs, missing/default material placeholders can be replaced by later installed sources, and stale entries are removed on reload/unload/failure without involving asset core.
- Phase 6 partial: `KajiyaRenderAssetCache` now consumes backend-local asset events for `MeshAsset`, `StandardMaterialAsset`, and dependent `TextureAsset` sources. Installed events invalidate only when the current CPU source no longer matches the cached source, while reload/unload/real-failed events remove affected cached mesh keys and their scene instances so the next sync rebakes from current assets. This keeps Kajiya residency policy inside the Kajiya backend.
- Phase 6 partial: `VideoServer` now owns a backend-local asset event cursor for `VideoClip` playback residency. Installed clip events refresh active instance frame indices, unload/failure events stop affected instances, failed reloads that preserved the last good installed clip are ignored, and `VideoPlayer2D` world sync now updates sprites when a clip reload swaps the texture handle without changing the frame index.
- Phase 6 partial: `AudioServer` now consumes `SoundClip` / `MusicTrack` events for ECS emitter bindings. Installed/unloaded/real-failed audio asset events invalidate matching long-lived emitter instances so the next world sync can restart from the current asset, while failed reloads that keep the last good installed asset are left alone. This stays in the audio backend and does not turn `Assets` into an audio residency manager.
- Phase 6 partial: direct/non-ECS audio playback now has an explicit backend-local policy. `AudioServer` tracks direct `play_sound` / `play_music` instance sources, keeps one-shot/direct playback untouched on `Installed` reload events, and forgets/stops affected direct instances on `Unloaded` or real `Failed` events. Natural completion is pruned during audio update.
- Phase 6 partial: backend residency cache rules are now captured in `docs/plan/backend_residency_cache_contract.md`. The contract standardizes event handling, source identity checks, local memory budgets/pinning, failed-reload behavior, and diagnostics while explicitly keeping GPU/audio/video/native residency out of asset core and avoiding a universal god-class cache.
- Phase 6/7 partial: app render diagnostics now emit `render.asset.uploaded` when renderer-owned residency enters an upload burst. This exposes GPU upload pressure as a render-owned diagnostic signal rather than folding it into asset request state.
- Phase 6/7 partial: app render diagnostics now emit `render.asset.evicted` when renderer-owned residency enters an eviction burst. This exposes texture/render cache memory pressure without routing GPU cache policy through `AssetStats` or `Assets`.
- Phase 6/7 partial: app render diagnostics now emit `render.asset.missing` when missing renderer-owned residency counts change, including fallback/loading/queued/visible queued/failed/cached-failed context. Missing GPU/native residency remains a renderer diagnostic rather than an asset-core failure policy.
- Phase 6/7 partial: `render.asset.failed` diagnostics now include previous/current failed counts, failed delta, and cached-failed/missing/fallback/loading/queued context, so render failure triage does not require manual `render.stats` diffing or asset-core state.
- Phase 6/7 partial: app render diagnostics now emit `render.asset.fallback` when fallback renderer-owned residency counts change, including loading/queued/visible queued/missing/failed context. Fallback usage stays an informational renderer signal rather than asset-core policy.
- Phase 6 verification slice: the current audio/video feature boundary has been rechecked with `cargo check --features audio,video`, `cargo test --features audio audio::`, and `cargo test --features video video::`, proving these backend-local residency and diagnostic paths still build and test outside asset-core ownership.
- Phase 7 partial: failed reloads now preserve the last good installed asset. Reload load/dependency/install failures restore the previous payload and dependency leases, keep the handle readable, and retain error/failure-phase diagnostics for tooling.
- Phase 0/5 partial: first-class `AssetPath<T>` now exists as a typed, serializable source-path reference. `Assets::load_path` resolves it to a strong handle, and `Assets::resolve_asset_path` resolves it to a typed weak identity with manifest type validation.
- Phase 0/5 partial: asset id/path migration helpers now let editor tooling query a manifest source path from an `AssetId` and rebuild a typed `AssetPath<T>` with the same product-type validation used by normal loads.
- Phase 3/store partial: dependency leases are now represented by an internal `AssetDependencyLeases` record that owns held dependency ids, deduplicates in insertion order, and centralizes reload lease preservation/replacement.
- Phase 3/lease partial: `src/asset/lease.rs` now owns handle release draining/application through `apply_handle_releases`, converting dropped strong-handle release channel messages into store direct-reference releases, canceling stale in-flight loads after release state changes, and emitting release/unloaded events. `server.rs` keeps only the channel field and delegates the focused release lifecycle step.
- Phase 3/lease partial: direct strong-handle lease acquisition and request enqueue generation capture now live in `src/asset/lease.rs` through `retain_direct_lease`, `retain_existing_direct_lease`, `acquire_direct_lease`, `acquire_manifest_direct_lease`, `acquire_typed_manifest_direct_lease`, `acquire_typed_manifest_source_lease`, and `enqueue_load_request`. `server.rs` no longer creates manifest-backed records, manually sequences typed manifest validation plus lease acquisition, supplies manifest asset-type closures for direct lease creation, fakes existing-only retain with a missing-manifest closure, or calls the request queue directly for normal strong-handle acquisition.
- Phase 3/lease partial: raw texture/font source acquisition now also goes through `lease.rs`. It handles manifest source hits, product validation, raw record retention, and request enqueueing for `load_texture` / `load_font`, leaving `server.rs` as a facade coordinator instead of the owner of raw source request mechanics.
- Phase 1/lease partial: handle release is now explicitly documented and tested as direct-lease accounting rather than `load_generation` ownership. A queued release from an old loaded generation is allowed to decrement only the old direct lease; if a weak handle is reacquired across a forced reload before that release drains, the new strong handle keeps the latest generation installed. This avoids turning generation mismatch into a no-op that would leak long-lived handles across reload.
- Phase 1/lease stress slice: a same-frame drop-and-reacquire regression now covers the case where the last strong handle is dropped, the asset is immediately loaded again before the release channel drains, and `update()` processes both edges in one tick. The latest lease remains live, `strong_references` stays accurate, and only the final drop unloads the asset, preserving correctness without a generation-owned handle model or a god-class lease manager.
- Phase 3/dependency stress slice: a 63-node binary dependency graph regression now loads the root, verifies every transitive dependency installs, rewrites all leaves, requires reload reporting to expand the changed roots back to the full dependent closure, verifies every node reloads, and then drops the root to prove the dependency lease cascade unloads the full graph. This covers Sakura-style large dependency graph pressure while keeping graph traversal in `dependency.rs` and lifecycle/reference mutation in `AssetStore`.
- Phase 3/runtime partial: `src/asset/runtime.rs` now owns runtime asset insertion/replacement semantic application, including installed event emission and dependency release fanout for replacements. Runtime replacement now consumes the registry read interface directly for dependency lease updates, so `server.rs` no longer emits runtime `Installed` events or supplies manifest asset-type closures for runtime replacement.
- Phase 3/dependency partial: `src/asset/dependency.rs` now owns dependency-resolution failure application through `resolve_dependencies_or_fail`, so missing/failed/cyclic dependencies mark the record, emit dependency-phase failure events, and retain failed request snapshots at the dependency boundary instead of routing that policy through `server.rs`.
- Phase 8 partial: cooked asset dispatch now goes through a built-in cooker descriptor registry for texture, font, audio, music, and video clip assets. The registry owns importer/cooker/version metadata, cooked output path conventions, dependency update hooks, and cook functions; `verify` now reports registered cooker version drift.
- Phase 3/7 partial: `AssetRequest` now has a stable internal request id and queued timestamp. `AssetStats` exposes cumulative submitted/activated request counts and the oldest queued request age, giving diagnostics a request-level foothold.
- Phase 8 partial: `CookRegistry` / `CookerDescriptor` can now be supplied to `import_path_with_registry`, `cook_all_with_registry`, `cook_target_with_registry`, and `verify_with_registry`. A custom `.blob` cooker regression proves new asset kinds can be imported, cooked, manifested, and verified without editing the built-in central match.
- Phase 8 partial: source extensions can now be shared by multiple registered cookers without adding a central kind-specific branch. A `.meta` file can set `import_settings.asset_type` to select the intended runtime asset type, and a custom shared-extension regression covers stable asset ids, manifest output, and verification. This creates a render-local seam for future glTF mesh/material split work without turning asset core into a multi-product god class.
- Phase 8 partial: built-in audio source selection now uses the same `import_settings.asset_type` mechanism instead of a registry-level sound/music special case. Default audio import settings write both `asset_type` and legacy `stream`, and normalization migrates old stream-only `.meta` files into the shared selector while preserving stable ids.
- Phase 8 partial: runtime factories can now optionally declare an `AssetCookedSchema` with cooker name, cooker version, and dependency schema. Cooked/package/bundle loads validate that manifest schema against the runtime factory before decode and return `AssetError::CookedSchemaMismatch` on drift, while raw source loads keep their bypass path. Built-in texture, font, audio, music, and video factories now declare their schema without creating a central asset god class.
- Phase 8 partial: cooked manifests now carry `AssetManifestProvenance` records with source hash, cooked hash, dependency hash, target platform, and `AssetConfig::profile`. `verify` checks this provenance and reports manifest provenance drift, keeping build/audit metadata in the cooking layer instead of pushing it into runtime residency or a global resource manager.
- Phase 8 partial: render-owned material assets now register through render-local extension points instead of asset built-ins. `render::asset::render_cook_registry()` adds a `.skymaterial` `StandardMaterialAsset` cooker, `register_render_asset_factories(&Assets)` installs the matching runtime factory, and an app-feature regression covers cooking/loading a standard material without making `asset` depend on render.
- Phase 8 partial: `.skymaterial` now has render-local texture dependency extraction. `albedo_texture`, `normal_texture`, and `emissive_texture` refs can be source-relative paths or asset ids; the material cooker imports path refs through the built-in texture cooker and writes texture dependencies into the manifest so load/reload dependency closure works without making asset core understand material fields.
- Phase 8 partial: install-time dependency handle creation is now available through `AssetInstallContext::dependency_handle`. It is a scoped install helper, not a global resource manager: it only creates strong handles for dependencies declared by the current manifest entry, validates type/state, and is used by `StandardMaterialAssetFactory` to bind cooked material texture ids to real `Handle<TextureAsset>` fields without fabricating weak handles or moving render semantics into asset core.
- Phase 8 partial: render-owned mesh assets now follow the same render-local registration model. `render_cook_registry()` adds a `.skymesh` CPU `MeshAsset` JSON cooker, `register_render_asset_factories(&Assets)` installs the matching runtime factory, and an app-feature regression covers vertex layout, vertex bytes, indices, submeshes, and bounds loading without touching GPU residency.
- Phase 8 partial: `.gltf` / `.glb` mesh sources now use the same render-local `MeshAsset` cooker instead of a second asset-core path. The cooker imports glTF triangle primitives into normalized CPU-side `.skymesh` JSON, preserving submesh material slots and bounds while leaving GPU residency for later backend/tooling slices.
- Phase 8 partial: glTF material import now uses the shared-extension selector instead of an asset-core multi-product pipeline. A `.gltf/.glb.meta` with `import_settings.asset_type = "standard_material"` cooks one selected glTF material into the same `.skymaterial` CPU schema, imports external URI base-color/normal/emissive textures as dependencies, and installs them as strong `Handle<TextureAsset>` fields through the render-local material factory.
- Phase 3/7 partial: `Assets::queued_request_snapshots()` exposes request id, asset id, priority, status, and queued age for currently queued requests, moving request diagnostics beyond aggregate counters.
- Phase 3/7 partial: `AssetRequestSnapshot` now carries a coarse `AssetRequestProgress` label/step/percent derived from the request phase, and app slow-queue diagnostics publish that progress. Slow queued request warnings are keyed by request identity, so a distinct queued request can still be reported even if the queue never drops below the threshold. This fills Sakura's "last progress" request diagnostic slot without turning `Assets` into a central progress manager.
- Phase 3/4/7 partial: `AssetInstallTask` now has an optional progress hook. Active request snapshots prefer task-provided install progress while a record is `Installing`, and audio install tasks report decode/stream-validation progress through the same diagnostic path.
- Phase 3/7 partial: app asset diagnostics now emit `asset.request.slow` for long-running active requests, including request status, failure phase, progress, dependency blocker count, and last error. This makes "why is this request stuck?" visible for active dependency/install phases without moving backend residency or editor policy into asset core.
- Phase 2/3/7 partial: queued requests canceled before activation now retain bounded recent `AssetRequestSnapshot`s. `Assets::canceled_request_snapshots()` and `AssetDiagnosticsSnapshot::canceled_requests` expose request id, asset id, generation, priority, queued age, and `Canceled` progress, while app diagnostics emits one-shot `asset.request.canceled` events. This makes release-before-activation cancellation observable without widening `server.rs` or adding a scheduler.
- Phase 2/3/7 partial: request timing stats now also report average queue wait for requests canceled before activation, and app `asset.stats` mirrors it as `average_canceled_request_queue_wait_ms`. This keeps canceled-request pressure visible as an aggregate signal without mixing it into successful completion latency.
- Phase 3/7 partial: `AssetRequests` now tracks cumulative failed request count separately from the bounded recent failed snapshots. `AssetStats::failed_requests` and app `asset.stats` expose the aggregate, while `failed_request_snapshots()` stays the recent evidence path with blocker/error details. Failure-boundary snapshot recording counts the active request once even when the record then moves through unload instead of a later `Failed` refresh.
- Phase 3/7 partial: failed `AssetRequestSnapshot`s now carry the structured `AssetFailurePhase` from the backing record, and app `asset.request.failed` events mirror it as `failure_phase`. Request-level triage no longer has to infer lookup/read/decode/dependency/install failures from error strings or separate failed-asset snapshots.
- Phase 3/7 partial: failed request snapshots now preserve failure phase and error text at the failure boundary instead of depending on later hydration from the live record. Diagnostics still enrich those snapshots with current dependency blocker details, but a retry or failed-reload restore can no longer erase the original request failure context.
- Phase 3/7 partial: `AssetStats` now reports `oldest_active_request_age`, and app `asset.stats` mirrors it as `oldest_active_request_ms`. This gives tools a continuous active-request aging signal before the thresholded `asset.request.slow` event fires.
- Phase 2/3 partial: per-request priority is now exposed through `Assets::*_with_priority` load variants for manifest id/path/weak-handle requests plus raw texture/font convenience loads. The default methods still use `AssetConfig::io_default_priority`, while tooling/game code can now send visible or urgent assets into the existing priority queue without creating a god-class scheduler.
- Phase 2/3 partial: the normal asset record driver now submits `Loading` records to background I/O in descending `load_priority` order before lower-priority loads. This closes the gap between request-level priority and the bounded I/O queue without adding a global scheduler or widening the public `Assets` facade.
- Phase 1/6 partial: `AssetEventKind::Loaded` now marks CPU/runtime payload load completion separately from install, and `AssetEventKind::Reloaded` marks successful reload install separately from first install. Backend residency caches explicitly no-op `Loaded` and treat `Installed` / `Reloaded` as backend-local resync signals, closing the loaded-vs-installed-vs-reloaded event observability gap without moving GPU/audio/video residency into asset core.
- Phase 3/7 partial: active request snapshots now include dependency blockers and a stringified last error when present. This fills the Sakura-style "why is this request waiting?" diagnostic gap without making the request manager reach into backend residency or app state.
- Phase 3/7 partial: active request snapshots now also hydrate the backing record's `AssetFailurePhase`, and app `asset.request.slow` diagnostics publish it as `failure_phase`. Slow active request triage can now distinguish dependency/install/read/decode context before the request reaches the bounded failed-request list.
- Phase 3/7 partial: source-load timing diagnostics now track read, decode/factory, and total source-load duration across background and synchronous load paths. `AssetStats` and app `asset.stats` events expose completed/failed source-load counts plus average read/decode/total timings without making `Assets` own backend residency or editor policy.
- Phase 3/7 partial: active request diagnostics now distinguish source-worker queue wait from active source reads. `AssetLoadQueue` still owns worker phase tracking, while `src/asset/diagnostics.rs` maps `Queued` to `queued for source worker`, `Reading` to `reading source`, and `Decoding` to `decoding source` progress labels for snapshots. This makes "is it waiting for an I/O slot or reading bytes?" visible without turning request state into an I/O scheduler.
- Phase 3/7 partial: source-load phase visibility now has aggregate stats as well as per-request labels. `AssetStats::source_load_phases` and app `asset.stats` expose queued/reading/decoding source-load counts, so tools can distinguish worker-slot pressure from read/decode saturation without inspecting every request snapshot.
- Phase 3/7 partial: `AssetStats` and app `asset.stats` now expose `running_load_jobs`, `queued_load_jobs`, and `load_queue_capacity`, separating running background load work from bounded I/O worker queue depth/capacity after canceled jobs are filtered out. This separates request queue depth, in-flight load count, worker queue pressure, and active worker work without adding a global resource scheduler.
- Phase 2/3/7 partial: bounded I/O worker jobs now track queued age. `AssetStats::oldest_queued_load_job_age` and app `asset.stats` expose `oldest_queued_load_job_ms`, making worker-slot starvation visible separately from request queue age and active request age.
- Phase 7 partial: app diagnostics now emit `asset.load.queue.slow` when the oldest bounded I/O source-load job exceeds the app diagnostic threshold. The event includes worker/queue/in-flight counts plus queued/reading/decoding source-load phase counts, making worker starvation actionable without moving scheduling policy into `Assets`.
- Phase 3/7 partial: `AssetRecord` now tracks current-state entry time, and `AssetStats::active_state_ages` exposes the oldest active record age for loading/loaded/waiting-dependencies/installing/uninstalling/unloading states. App `asset.stats` mirrors these as `oldest_state_*_ms`, adding Sakura-style "how long has this resource been stuck in this state?" visibility without moving request or backend policy into a central manager.
- Phase 7 partial: app diagnostics now emit `asset.state.slow` for the slowest active asset record lifecycle state when it crosses the app threshold. This catches record-level stalls that are not necessarily visible as request or worker-queue stalls while keeping lifecycle mutation in `AssetStore` and observability in the app diagnostics mirror.
- Phase 7 partial: `AssetDiagnosticsSnapshot` now carries the full `AssetReloadStatus`, and app diagnostics emits `asset.reload.status` when auto-reload, watcher, freeze, or pending-root state changes. Editor/dev tooling can observe pending reload batches and freeze/debounce state through diagnostics without reaching into `AssetsInner` or creating a reload manager.
- Phase 3/7 partial: `running_load_jobs` now comes from `AssetIoService` worker execution counters instead of being inferred from in-flight minus queued loads, so completed-but-not-drained loads no longer appear as actively running worker work.
- Phase 3/7 partial: `AssetStats` and app `asset.stats` now expose `load_worker_threads`, so tooling can tell whether `running_load_jobs` has saturated the configured bounded I/O worker pool without reaching into `AssetConfig` or the private worker service.
- Phase 3/7 partial: `AssetLoadQueue` now tracks cumulative `deferred_load_submissions` when bounded I/O capacity reports `QueueFull`; `AssetStats` and app diagnostics expose the counter so back-pressure is visible even after the queue drains.
- Phase 7 partial: app diagnostics now emit `asset.load.backpressure` when `deferred_load_submissions` increases, including deferred delta, running worker load count, worker pool size, queued load jobs, queue capacity, and in-flight loads. This makes bounded I/O pressure visible without widening `Assets` into a scheduler.
- Phase 5/7 partial: provider-owned cache/package diagnostics now flow through read-only stats. `AssetProvider::stats()` defaults to empty, `LocalAssetProvider` reports package root/file counts, cached bundle-index and index-entry counts, source resolution counts by raw/local-cooked/package-root/bundle location, provider resolve errors, and provider cache/full-cache invalidation counts. `AssetStats` carries those provider stats, and app `asset.stats` mirrors them without exposing cache control APIs or making `Assets` own package policy.
- Phase 5/7 partial: app diagnostics now emit `asset.provider.resolve.failed` when provider resolve-error counters increase, including the delta, total resolve errors, source-resolution counters, and package/cache counters. This gives editors an actionable failure signal while provider policy stays inside `AssetProvider` and the app mirror owns presentation.
- Phase 5/7 partial: app diagnostics now emit `asset.provider.cache.invalidated` when provider cache-invalidation counters increase, including normal/full invalidation deltas, totals, and package/cache counters. Package/cache visibility stays read-only and event-driven instead of exposing mutation controls through `Assets`.
- Phase 5/9 partial: `AssetProviderStats` is now re-exported from `sky_engine::asset`, matching the already-public `AssetStats::provider` field and keeping provider diagnostics nameable without exposing provider mutation APIs.
- Phase 6/7 partial: audio/video backend-local stats now live on `AudioServer::stats()` and `VideoServer::stats()`. Audio reports backend availability, configured buses, live/spatial backend instances, direct playback, and ECS emitter bindings; video reports playback state counts, distinct clips, current frame textures, and current-frame texture bytes. These diagnostics stay out of `AssetStats`, so native residency remains backend-owned instead of becoming a god-class asset manager.
- Phase 6/7 partial: app service diagnostics now mirror audio/video backend stats as separate `audio.stats` and `video.stats` events when those stats change. The publisher lives in `src/app/media_diagnostics.rs`, keeping app-facing observability out of asset core and avoiding a catch-all resource diagnostics object.
- Phase 6/7 partial: audio/video backend stats now include failed play-request counters. Failed audio backend starts and failed video play setup are counted locally and mirrored through `audio.stats` / `video.stats`, satisfying the backend-failure diagnostic requirement without adding a central native-resource failure store.
- Phase 6/7 partial: app media diagnostics now emits backend-category warning events (`audio.play.failed` / `video.play.failed`) when failed play-request counters increase. Tooling can react to failure deltas without diffing stats manually and without routing backend failures through asset diagnostics.
- Phase 6/7 partial: audio backend availability diagnostics now include `AudioServerStats::disabled_reason`, app `audio.stats` mirrors it, and `audio.backend.unavailable` fires when the backend enters unavailable state. Audio device/config failure remains audio-owned and is not promoted into asset-core failure state.
- Phase 6/7 partial: audio/video backend stats and failed-play warning events now include last play-failure context strings. This covers the backend contract's "last failure context" diagnostic slot while keeping the error summary local to audio/video and the app diagnostics mirror.
- Phase 6/7 partial: `VideoServerStats` now reports current-frame texture bytes in addition to texture count, and app diagnostics mirrors it as `current_frame_texture_bytes`. The value is computed from video playback/frame sources inside `VideoServer`, using installed texture payload length when available and the clip RGBA frame footprint as a fallback, so asset core still does not own video/native residency accounting.
- Phase 6/7 partial: app media diagnostics now emit `video.frame.resident` when current-frame video texture bytes become resident or change size, including texture count, instance count, playing count, and distinct clip count. This keeps streamed-frame residency visibility in the video backend/app mirror rather than `AssetStats`.
- Phase 6 partial: `Texture` now exposes `resident_bytes()` from GPU-side format/extent metadata for common uncompressed formats. `RenderAssetCache` uses that value for resident/upload byte accounting, and `GpuVideoFrameBuffer` reuses it for its own stable streamed-frame texture. This keeps GPU memory diagnostics backend-local without moving streamed video or render texture residency into `Assets` or a universal resource cache.
- Phase 6/7 partial: app render diagnostics now mirror backend-local `RenderStats` as `render.stats` and emit `render.asset.failed` when render asset failure counts change. The publisher lives in `src/app/render_diagnostics.rs`, so GPU/native residency remains renderer-owned and `AssetStats` stays focused on asset-core state.
- Phase 7 partial: `Assets::failed_asset_snapshots()` exposes current failed assets and failed-reload diagnostics, including asset id/type, state, load generation, failure phase, error, and reload flag.
- Phase 7 partial: `src/diagnostics/` now provides a structured app-facing diagnostic event queue. `src/app/services.rs` publishes asset stats/reload/failure/slow-queue events after asset updates and mirrors notable asset diagnostics through the existing `log` facade.
- Phase 7 partial: `Assets::diagnostics_snapshot()` now returns one coherent asset diagnostics snapshot containing aggregate stats, queued requests, active failures, and the latest reload report.
- Phase 7 partial: `src/asset/diagnostics.rs` now owns asset stats, queued/active request snapshots, failed asset snapshots, diagnostics snapshot assembly, and reload-status projection as read-only views over store/request/load/event/reload state. `server.rs` no longer hand-builds these diagnostic DTOs.
- Phase 7 partial: automatic reload now has config-controlled debounce and pending-root batching. `Assets::reload_status()` exposes pending reload roots and freeze state, `Assets::set_auto_reload_frozen()` pauses editor transactions, and `Assets::force_reload()` forces a reload even when hashes are unchanged.
- Phase 7 partial: optional native file watching is available through the `asset-watch` feature plus `AssetConfig::with_file_watcher(true)` / `AssetPlugin::with_file_watcher(true)`. Watch events trigger the same pending/debounce reload path rather than bypassing the asset state machine.
- Phase 5/7 partial: native watch events now carry changed paths, `ManifestIndex::lookup_watch_asset` resolves source/cooked/package-root watch paths to asset ids, and auto reload uses targeted watch roots when possible while falling back to full scans for manifest/package-index/unknown changes.
- Phase 5/7 partial: native file watching now computes focused watch roots from `asset_root` plus external package roots and external package-file parent directories. Package roots/files nested under `asset_root` rely on the existing recursive watch, while external mounts get their own watcher registration.
- Phase 7/reload partial: `src/asset/reload.rs` now owns reload-root application through `apply_reload_roots`: dependency closure preparation, queued-reload event fanout, missing-manifest failure reporting, and last-report recording. `server.rs` delegates this focused lifecycle slice instead of hand-assembling reload semantics.
- Phase 7/reload partial: `src/asset/reload.rs` now owns automatic reload driving through `drive_auto_reload` and manifest-refresh scan construction through `refresh_manifest_and_detect_scan`: watcher-root targeting, provider cache invalidation for watch events, interval-scan fallback, pending-root debounce, due-root application, manifest reload, and missing-record refresh failure application are coordinated in the reload slice instead of `server.rs`.
- Phase 7/reload partial: manual reload orchestration now also lives in `src/asset/reload.rs` through `reload_changed_with_report` and `force_reload_root`. Manifest refresh, provider invalidation, changed-root scan/report assembly, forced-root pending discard, forced reload queuing, and last-report recording are handled by the reload slice while `server.rs` remains a facade/coordinator.
- Phase 3/store partial: `src/asset/store.rs` now owns `AssetStore`, `AssetRecord`, `AssetReloadBackup`, and the raw texture/font source indexes. `AssetsInner` keeps a single store field instead of directly carrying record/index maps, which is the first real split of record storage out of `server.rs`.
- Phase 3/7 partial: active requests are now tracked separately from queued requests. `AssetRequest` records generation plus active timing, `Assets::active_request_snapshots()` exposes in-progress load/dependency/install phases, and app diagnostics publish an `active_requests` count.
- Phase 3/7 partial: `AssetRequests` now records completed request timing totals, phase timing samples for loading/decoding/dependency-wait/ready-to-install/installing/unloading, and current phase age for snapshots. `AssetStats::request_timings` exposes completed count plus average queue-wait, active, total, and per-phase request duration. App diagnostics mirror these averages plus uninstalling/unloading state counts and slow-request phase age as structured event fields.
- Phase 3/request partial: `src/asset/request.rs` now owns queued-request activation and active-request refresh against `AssetStore`, including canceling unreferenced queued requests, activating referenced records for load, canceling stale source-load jobs after record driving, deriving active request phases from record/source-load state, and retiring terminal active requests. `server.rs` now delegates request-state advancement instead of manually driving the request manager internals or reaching into diagnostics for request progression.
- Phase 3/7 partial: `AssetRequests` now retains a bounded recent failed-request list. `Assets::failed_request_snapshots()` and `AssetDiagnosticsSnapshot::failed_requests` expose request id, generation, priority, terminal status, timings, dependency blockers, and last error, while app diagnostics publish `asset.request.failed` events.
- Phase 3/7 partial: request snapshots now expose structured dependency blocker details (`Missing` / `Failed` / `Waiting` plus dependency state) and dependency cycle paths. App slow/failed request diagnostics publish blocker reason counts and cycle length, improving dependency wait explanations without turning `Assets` into a global scheduler or backend residency manager.
- Phase 3/7 partial: `AssetRequestPhase` now includes `Decoding`, and `AssetLoadQueue` tracks per in-flight source-load read/decode phase through a lightweight worker-side phase tracker. Active request snapshots and app stats can distinguish source reading from factory decode without moving I/O/provider policy or backend residency into `Assets`.
- Phase 3/driver partial: `src/asset/driver.rs` now owns the normal and blocking record-iteration loops through `drive_records` / `AssetDriveMode`, including iteration caps, blocking target validation, in-flight skip behavior, install limiter use, dependency-state application, unload completion, and load-completion progress checks. `server.rs` keeps the semantic callbacks for I/O, dependency resolution, install, events, and reload orchestration.
- Phase 3/failure partial: `src/asset/failure.rs` now owns failure application as a focused lifecycle slice: record failure/failed-reload restore, dependency lease update application, `Failed` event emission, unused-release scheduling, and failed active-request snapshot retention. `server.rs` now delegates failure semantics instead of coordinating store/events/release/request diagnostics directly.
- Phase 3/store partial: direct/dependency reference count retain/release rules now live on `AssetStore`, including saturating release behavior and runtime-asset-safe retain of existing records.
- Phase 3/store partial: raw texture/font source index creation and direct retain now live on `AssetStore::ensure_raw_texture_record` / `ensure_raw_font_record` plus `retain_raw_texture_record` / `retain_raw_font_record`, so `server.rs` no longer directly inserts raw source map entries, raw source records, or raw retain fallback records.
- Phase 3 partial: `AssetRequests` now owns the queued request list, active request map, stable request id allocation, and submitted/activated/canceled counters. `AssetsInner` delegates request bookkeeping to this request manager instead of carrying those fields directly.
- Phase 3/store partial: request activation state changes now live on `AssetRecord::activate_for_load`, moving the `Unloaded`/`Failed`/`Unloading` -> load-state transition out of `server.rs`.
- Phase 4 partial: `src/asset/install.rs` now owns `AssetInstallContext`, `AssetInstallBudget`, `AssetInstallPoll`, `AssetInstallTask`, and `AssetInstallResult`. The public `sky_engine::asset::*` re-export is preserved while the install protocol gets its own internal module.
- Phase 4 partial: `src/asset/install.rs` now also owns install driving through `drive_record_install` / `AssetInstallRecordOutcome`, including pending-task polling, factory `begin_install`, and ready/pending record mutation. It also owns install-success semantic application through `apply_installed_record`: dependency lease reconciliation, `Installed` event emission, and unused-release scheduling.
- Phase 4/server-boundary partial: `src/asset/install.rs` now owns manifest-entry lookup, factory lookup, loaded-payload lookup, install-success event/dependency application, manifest-backed uninstall dispatch, and install/uninstall failure lifecycle wrappers for record/event/request diagnostics. `server.rs` delegates install/uninstall record semantics instead of assembling success or failure policy in the coordinator.
- Phase 4 partial: `src/asset/install.rs` now owns `AssetInstallLimiter`, centralizing per-update install count limits, install time-budget remaining-time calculation, and one-poll-per-asset-per-pass deduplication. `server.rs` still iterates records, but no longer hand-rolls install budget gates inside the update loops.
- Phase 3/store partial: release/uninstall/unload state transitions now live on `AssetRecord::begin_release`, `advance_uninstall`, and `finish_unload`, removing duplicate unload transition code from the normal and blocking update loops.
- Phase 3/4 partial: install pending/ready record mutations now live on `AssetRecord::take_install_task`, `defer_install`, and `finish_install`; the install driver owns polling and factory begin while `server.rs` keeps install-loop budgeting and events.
- Phase 3/dependency partial: `src/asset/dependency.rs` now owns dependency graph traversal, blocking dependency closure, dependent reload closure, missing/failed dependency error mapping, and cycle detection. `server.rs` delegates dependency resolution to this module and only records failure/events.
- Phase 3/store partial: dependency readiness now runs through `AssetStore::evaluate_dependency_records`, which creates/activates missing dependency records and reports raw readiness back to the dependency resolver for error semantics and cycle checks.
- Phase 3/store partial: load completion, failure, failed-reload restore, and reload preparation record mutations now live on `AssetStore` / `AssetRecord` helpers, including `finish_loaded_record`, `fail_record`, `queue_record_reload`, `prepare_record_reload`, `queue_reload`, and `prepare_reload`. `server.rs` keeps I/O/factory orchestration and semantic events.
- Phase 3/store partial: completed loaded payload application now runs through `AssetStore::finish_loaded_record`, which owns the normal-vs-reload dependency lease replacement/extension decision plus the final `AssetRecord::finish_load` mutation. `server.rs` still provides manifest/factory context and semantic events around the returned update.
- Phase 3/store partial: failure lifecycle orchestration now runs through `AssetStore::fail_record`, which owns reload-backup restoration, plain failure payload clearing, held-dependency replacement/release calculation, and the event state to report. `server.rs` now emits the semantic failure event and schedules release.
- Phase 3/store partial: runtime insertion/replacement now runs through `AssetStore::insert_runtime_asset` / `replace_runtime_asset` and `AssetRecord::replace_runtime`, so runtime asset payload replacement, type validation, and dependency lease clearing no longer require direct `server.rs` record field writes.
- Phase 3/store audit: `server.rs` no longer directly assigns the core record-owned lifecycle fields already covered by store helpers: loaded payload, installed payload, pending install task, reload backup, loaded fingerprints/hashes, and common load/install/unload state variants. The remaining server responsibility is orchestration around manifests, I/O, install-loop budgeting, and event emission.
- Phase 3/store partial: unused-release scheduling and dependency-release cascading now live in `AssetStore::schedule_release_if_unused` plus the direct/dependency release-and-schedule helpers. `server.rs` emits the returned unload events, while the store owns the referenced/reload-pending gate, held-dependency clearing, dependency ref decrement, cascade order, and record release transition.
- Phase 3/store partial: held dependency lease replacement now lives on `AssetStore::replace_held_dependencies` and `extend_held_dependencies`, and dependency lease update application now runs through `AssetStore::apply_dependency_lease_update`, including dependency retain/create and load activation. `server.rs` provides manifest asset-type lookup as a closure and emits release events.
- Phase 3/store partial: handle and public status read semantics now live on `AssetStore::state_for_handle`, `error_for_handle`, `failure_phase`, `get_for_handle`, and `is_runtime_record`. The public `Assets` facade still implements `AssetHandleProvider`, but installed-payload/type/error/failure-phase/runtime lookup is owned by the record store instead of adding more direct record reads to `server.rs`.
- Phase 3/server-boundary partial: public `Assets` facade reads and controls now route through focused `AssetsInner` methods for config snapshots, auto-reload freeze/report, handle state/error/failure phase, and installed payload access. The facade no longer reaches directly through the mutex into `config`, `reload`, or `store` fields for those paths, and the handle-provider implementation uses the same inner read boundary.
- Phase 3/store partial: remaining production read helpers for record existence, load generation, and background-load preference now live on `AssetStore`, leaving `server.rs` to compose reload, request, and load-queue policy without reaching into record fields.
- Phase 3/store + load partial: load/install read decisions now use `AssetStore` helpers for generation lookup, load priority, current-load completion acceptance, cancellation checks, and install loaded-payload lookup. `load.rs` and `server.rs` keep I/O/install orchestration, while record field semantics stay in the store.
- Phase 3/store + driver partial: normal record-drive enumeration, loading-priority ordering, per-record state lookup, and normal iteration-limit calculation now live on `AssetStore`. `driver.rs` owns scheduling control flow, but no longer reaches into the record map for production drive decisions.
- Phase 3/store + reload partial: reload scan record selection now comes from `AssetStore::reload_scan_records`, which exposes tracked fingerprints/hashes or untracked ids without leaking runtime/raw/reference/fingerprint rules into `reload.rs`. The reload module still owns source comparison, skip reasons, dependency closure, and report assembly.
- Phase 3/store + diagnostics partial: `AssetStore` now exposes focused read-only diagnostic record snapshots through `diagnostic_record(s)`, so `diagnostics.rs` assembles stats, request hydration, dependency blockers, failure lists, and request phases without reaching into `records` internals. This remains a diagnostic read model only; residency, load policy, reload comparison, and backend semantics stay in their existing slices.
- Phase 3/store + dependency partial: dependency graph code now asks `AssetStore` for record dependency links and record ids that reference a dependency, rather than scanning `records` directly. `dependency.rs` still owns graph traversal, manifest fallback, cycle detection, and dependent-reload closure ordering, so the store remains a record relation boundary rather than a graph god class.
- Phase 3/store + registry partial: manifest refresh now gets refreshable record ids and applies manifest asset-type updates through `AssetStore` helpers. `registry.rs` keeps manifest replacement and missing-record reporting, while store-owned runtime/raw skip rules and record field mutation stay hidden behind the record boundary.
- Phase 3/store + events partial: event emission now gets load generation, asset type, fingerprints, dependencies, reload marker, and failure phase through `AssetStore::event_record_context`. `events.rs` still owns event sequencing/retention/fanout helpers, but no longer cracks open record internals to build event payloads.
- Phase 3/store + request/lease partial: queued request activation now runs through `AssetStore::activate_record_for_load`, returning only the post-activation state/generation needed by `request.rs`; lease enqueue generation lookup now uses the store generation helper. Request timing/counters and lease acquisition remain in their focused modules.
- Phase 3/store + install partial: install driving now uses store helpers for dependency-handle installed/type validation, pending install-task take/defer, install completion mutation, and uninstall payload lookup. `install.rs` keeps manifest/factory/loaded-payload lookup, factory calls, budgeted task polling, install event/dependency application, and uninstall hook orchestration without direct record field access.
- Phase 3/store + provider partial: raw source fallback now uses store read-model helpers for raw asset type/path lookup. `provider.rs` still owns raw/cooked/package/bundle source resolution and manifest-entry fallback construction, but no longer reaches into record internals for raw texture/font records.
- Phase 3/store audit: the remaining install-precondition regression in `server.rs` now goes through a test-only `AssetStore::force_missing_loaded_payload_for_install_test` helper. The residual server lifecycle-field scan no longer finds direct record-map or record-field writes, including test code.
- Phase 6/7 partial: `src/asset/events.rs` now owns `AssetEventLog`, including event sequence allocation, capacity retention, cursor reads, record-context snapshotting, and semantic helpers for `Loaded`, `Installed`, `Unloaded`, `Failed`, and reload-queued event emission. Callers express lifecycle outcomes instead of hand-pairing event kinds and states.
- Phase 3/7 partial: `src/asset/events.rs` now also owns focused reload/unload/loaded/installed/failed event fanout helpers. `server.rs`, install, runtime, and failure slices delegate semantic event emission instead of retaining local `push_from_store` wiring.
- Phase 2/3 partial: `src/asset/load.rs` now owns `AssetLoadQueue` and `CompletedLoad`, including the worker service, completion channel, in-flight generation tracking, duplicate submit suppression, cancel-token mapping, queue-full rollback, ready completion draining, successful load application, dependency lease updates, and loaded event fanout. It also owns raw manifest-entry helpers, resolved-source loading, content hashing, and manifest-entry fingerprinting. `server.rs` no longer owns the I/O worker/channel/in-flight/source-load helper mechanics or the success-side load application path.
- Phase 2/3 partial: `src/asset/load.rs` now also owns loaded-payload preparation through `prepare_loaded_asset`, including loaded-vs-manifest dependency fallback, dependency deduplication, manifest fingerprinting, and content-hash packaging. `server.rs` now applies a prepared load payload instead of duplicating this logic across async completions and blocking loads.
- Phase 2/3 partial: `src/asset/load.rs` now owns record-level load orchestration through `submit_record_load`, `load_record_now`, and `finish_completed_load`: provider/source resolution, factory lookup, async worker completion construction, blocking load application, stale completion filtering, and completion failure phase mapping are out of `server.rs`.
- Phase 2/3/server-boundary partial: `src/asset/load.rs` now also owns load application and load-failure lifecycle through `load_record_now_and_apply_or_fail`, `submit_record_load_or_fail`, `apply_completed_load`, `drain_completed_loads_or_fail`, and related helpers: dependency lease updates, loaded-event emission, release-event fanout, completion draining, stale-completion skip reporting, source-load timing, failure-phase mapping, failed record/event/request snapshot retention, and load-queue cancellation after dependency changes. `server.rs` delegates both success and returned-failure load semantics instead of assembling them in the coordinator.
- Phase 2/3/load-policy partial: `src/asset/load.rs` now owns the load scheduling read decisions for background-load eligibility and current in-flight generation checks. `server.rs` still satisfies the `AssetRecordDriver` trait, but no longer hand-composes global background config, store preferences, and load-queue generation membership.
- Phase 7 partial: `src/asset/reload.rs` now owns `AssetReloadController`, including automatic reload freeze state, scan interval gating, debounce/pending-root batching, forced-root pending discard, last report retention, and reload status projection. It also owns changed-root detection by comparing tracked records against current manifest fingerprints/source content hashes, automatic reload driving from watcher events or interval scans, manual reload report assembly, forced reload root application, plus reload root preparation through `prepare_reload_roots`: deduping roots, computing dependent closure, queueing/prepare record reload state, and returning queued/missing-manifest outcomes for focused reload application.
- Phase 7/provider partial: `reload::detect_reload_scan_from_provider` now owns the provider-backed source resolution used by reload hash/fingerprint scans. `server.rs` refreshes the manifest and delegates scan construction instead of supplying an ad hoc provider closure.
- Phase 5/7/server-boundary partial: `src/asset/reload.rs` now owns manifest reload replacement through `reload_manifest_records` / `replace_manifest_records`, including registry-loader reads, provider full-cache invalidation, and manifest-record refresh failure application. Public `Assets::reload_manifest`, auto scan refresh, and forced reload now share that lifecycle instead of duplicating manifest refresh glue in `server.rs`.
- Phase 9 partial: sections 1, 4, 5, 8, and 9 now describe the current implementation instead of the original gap state. The plan no longer claims thread-per-load, fixed-count blocking load, synchronous-only install, manual-only hot reload, or asset-event-only diagnostics as current SkyEngine limitations; those are now recorded as implemented slices with remaining work scoped to focused modules and backend-local residency.

Verified:

- `cargo test --features asset asset::blocking`
- `cargo test --features asset asset::registry`
- `cargo test diagnostics::`
- `cargo test --features app app::asset_diagnostics`
- `cargo test --features app auto_reload_debounces_modified_asset_on_update`
- `cargo test --features app auto_reload_freeze_delays_scan_until_unfrozen`
- `cargo test --features app force_reload_reloads_asset_even_when_hash_is_unchanged`
- `cargo test --features asset-watch file_watcher_triggers_auto_reload_before_poll_interval`
- `cargo test --features asset asset::io`
- `cargo test --features asset asset::dependency`
- `cargo test --features asset asset::events`
- `cargo test --features asset asset_events_include_backend_residency_context`
- `cargo test --features asset asset::reload`
- `cargo test --features asset reload_changed`
- `cargo test --features asset asset::server::tests`
- `cargo test --features asset asset::failure`
- `cargo test --features asset failure`
- `cargo test --features asset asset::lease`
- `cargo test --features asset typed_asset_path_serializes_resolves_and_loads`
- `cargo test --features asset asset::install`
- `cargo test --features asset install`
- `cargo test --features asset dropping_last_handle_calls_factory_uninstall_before_unload`
- `cargo test --features asset normal_driver_finishes_uninstall_and_unload_with_event_callback`
- `cargo test --features asset asset::install::tests`
- `cargo test --features asset asset::load`
- `cargo test --features asset load_queue_reports_active_source_load_phase`
- `cargo test --features asset resolved_source_load_marks_decode_phase_for_diagnostics`
- `cargo test --features asset asset::server`
- `cargo test --features asset asset::driver`
- `cargo test --features asset assets_progress_to_installed_and_can_be_read`
- `cargo test --features asset load_blocking_installs_dependency_chain`
- `cargo test --features asset asset::provider`
- `cargo test --features asset provider_invalidation_from_watch_events_routes_changed_paths_and_rescans`
- `cargo test --features asset reload_scan_from_provider_resolves_current_source`
- `cargo test --features asset local_provider_stats_report_package_mounts_and_bundle_index_cache`
- `cargo test --features asset memory`
- `cargo test --features asset package_root_cooked_artifact_loads_through_normal_assets_facade`
- `cargo test --features asset local_provider_falls_back_to_package_files_for_cooked_artifacts`
- `cargo test --features asset local_provider_invalidates_cached_bundle_index_for_changed_bundle_path`
- `cargo test --features asset package_file_cooked_artifact_loads_through_normal_assets_facade`
- `cargo test --features asset force_reload_reads_rewritten_package_file_bundle`
- `cargo test --features asset reload_manifest_invalidates_provider_package_cache`
- `cargo test --features asset reload_scan_and_force_reload_invalidate_provider_once_per_manifest_refresh`
- `cargo test --features asset manifest_index_resolves_source_cooked_and_package_watch_paths`
- `cargo test --features asset watch_events_resolve_known_paths_and_request_scan_for_unknown_paths`
- `cargo test --features asset apply_reload_roots_records_report_events_and_missing_manifest_failure`
- `cargo test --features asset configured_watch_roots_include_external_package_roots_without_nested_duplicates`
- `cargo test --features asset-watch file_watcher_triggers_auto_reload_for_external_package_root`
- `cargo test --features asset-watch file_watcher_triggers_auto_reload_for_external_package_file`
- `cargo test --features asset asset::registry`
- `cargo test --features asset asset::runtime`
- `cargo test --features asset asset::diagnostics`
- `cargo test --features asset active_request_snapshot_uses_background_decode_phase`
- `cargo test --features asset active_request_snapshot_reports_dependency_blockers_and_last_error`
- `cargo test --features asset request_manager_tracks_queued_active_and_canceled_counts`
- `cargo test --features asset asset_stats_report_queue_inflight_and_state_counts`
- `cargo test --features app app::asset_diagnostics`
- `cargo check --features app`
- `cargo test --features asset typed_handles_reject_manifest_type_mismatch`
- `cargo test --features asset load_uses_manifest_source_lookup`
- `cargo test --features asset asset::reload`
- `cargo test --features asset reload_changed`
- `cargo test --features asset auto_reload`
- `cargo test --features asset asset::store`
- `cargo test --features asset load_texture`
- `cargo test --features asset load_font`
- `cargo test --features asset auto_reload`
- `cargo test --features asset force_reload`
- `cargo test --features asset background_loading_completes_on_later_updates`
- `cargo test --features asset failed_reload_keeps_last_good_installed_asset`
- `cargo test --features app texture_memory_budget_evicts_least_recently_used_resident_texture`
- `cargo test --features app texture_memory_budget_preserves_pinned_resident_texture`
- `cargo test --features app unpin_texture_reapplies_texture_memory_budget`
- `cargo test --features app invalid_runtime_texture_prepare_reports_failed_readiness`
- `cargo test --features app wgpu_asset_cache_invalidates_mesh_and_material_from_asset_events`
- `cargo test --features app render::backend::tests`
- `cargo test --features app,renderling-renderer renderling_cache_resyncs_replaced_runtime_assets`
- `cargo test --features app,renderling-renderer renderling::tests`
- `cargo check --features app,renderling-renderer`
- `cargo test --features app,kajiya-renderer kajiya::assets::tests`
- `cargo test --features app,kajiya-renderer kajiya::`
- `cargo test --features app,kajiya-renderer kajiya_scene_renderer_uses_neutral_scene_snapshot`
- `cargo check --features app,kajiya-renderer`
- `cargo test --features video video::server::tests`
- `cargo test --features video video::`
- `cargo test --features audio audio::assets::tests`
- `cargo test --features audio audio::`
- `cargo test --features audio audio::server::tests`
- `cargo test --features app,audio audio::server::tests`
- `cargo test --features app,audio audio::server::tests::installed_audio_event_invalidates_matching_emitter_binding`
- `cargo test --features asset asset::`
- `cargo test --features app render::runtime::tests::texture_assets`
- `cargo test --features app render::runtime::tests`
- `cargo test --features app asset::`
- `cargo test --features asset`
- `cargo test --features app`
- `cargo test --features asset asset::server::tests::asset_stats_report_queue_inflight_and_state_counts`
- `cargo test --features asset asset::request`
- `cargo test --features asset asset::diagnostics`
- `cargo test --features asset asset::store`
- `cargo test --features app app::asset_diagnostics`
- `cargo test --features asset queued_request_is_canceled_when_last_lease_is_dropped_before_update`
- `cargo test --features asset asset::server::tests`
- `cargo test --features asset asset::`
- `cargo test --features asset load_texture_missing_raw_path_fails_without_panic`
- `cargo test --features app app::asset_diagnostics`
- `cargo test --features asset asset::store`
- `cargo test --features asset asset::server::tests`
- `cargo test --features asset asset::`
- `cargo test --features app asset::`
- `cargo test --features asset-watch asset::`
- `cargo test --features asset asset::install`
- `cargo test --features asset raw`
- `cargo test --features asset dependencies_load_transitively_and_release_with_parent`
- `cargo test --features asset dropping_last_handle_returns_asset_to_unloaded_state`
- `cargo test --features asset missing_dependency_marks_asset_failed`
- `cargo test --features asset dependency_cycle_marks_asset_failed`
- `cargo test --features asset failed_reload_keeps_last_good_installed_asset`
- `cargo test --features asset force_reload_reloads_asset_even_when_hash_is_unchanged`
- `cargo test --features asset replace_runtime_keeps_handle_and_updates_payload`
- `cargo check --features asset`
- `rg -n "\.state\s*=|\.loaded\s*=|\.installed\s*=|\.pending_install|\.reload_backup|\.loaded_payload|\.installed_payload|\.error\s*=|\.failure_phase\s*=|\.dependencies\s*=|records\[|records\.get_mut" src\asset\server.rs` (no current hits; the install-precondition regression now uses a test-only `AssetStore` helper instead of cracking open record fields from `server.rs`)
- `rg -n "factory\.begin_install|poll_install\(|factory\.uninstall\(" src\asset\server.rs` (current hit is a test `AssetInstallTask::poll_install` implementation, not production install driving)
- trailing-whitespace scan over new/changed asset request/store/install/watcher/diagnostics files
- `git diff --check` over the asset implementation and documentation files
- `cargo test`
- `cargo check --features audio,video`
- `cargo check --examples --features asset`
- `cargo test --features asset cooked_runtime_load_reports_schema_mismatch_before_decode`
- `cargo test --features asset asset::server::tests`
- `cargo test --features asset raw`
- `cargo test --features asset asset::cook`
- `cargo test --features asset import_settings_asset_type`
- `cargo test --features app app::render_diagnostics`
- `cargo check --features app`
- `git diff --check -- src\app\render_diagnostics.rs docs\reference\render.md docs\plan\backend_residency_cache_contract.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features app app::render_diagnostics` (4 tests, including render upload/eviction burst diagnostics)
- `cargo check --features app`
- `git diff --check -- src\app\render_diagnostics.rs docs\reference\render.md docs\plan\backend_residency_cache_contract.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features app app::render_diagnostics` (5 tests, including render missing-residency diagnostics)
- `cargo check --features app`
- `git diff --check -- src\app\render_diagnostics.rs docs\reference\render.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features app app::render_diagnostics` (5 tests, including enriched render failure context diagnostics)
- `cargo check --features app`
- `git diff --check -- src\app\render_diagnostics.rs docs\reference\render.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features app app::render_diagnostics` (6 tests, including render fallback diagnostics)
- `cargo check --features app`
- `git diff --check -- src\app\render_diagnostics.rs docs\reference\render.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features app,audio,video app::media_diagnostics` (5 tests, including video frame residency diagnostics)
- `cargo check --features app,audio,video`
- `git diff --check -- src\app\media_diagnostics.rs docs\reference\video.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features app,audio app::media_diagnostics` (3 audio app diagnostics tests, including backend-unavailable warning)
- `cargo test --features app,audio audio::server::tests` (6 audio server tests, including backend-local audio stats)
- `cargo check --features app,audio,video`
- `git diff --check -- src\audio\types.rs src\audio\server.rs src\app\media_diagnostics.rs docs\reference\audio.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features asset load_blocking_provider_resolve_failure_records_lookup_phase_and_failed_request`
- `cargo test --features asset load_blocking_provider_read_failure_records_read_phase_and_failed_request`
- `cargo test --features asset asset::server::tests` (63 tests, including blocking provider lookup/read failure diagnostics)
- `cargo check --features asset`
- `git diff --check -- src\asset\server.rs docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features asset install_precondition_failure_records_install_phase_and_failed_request`
- `cargo test --features asset asset::store`
- `cargo check --features asset`
- `rg -n "\.state\s*=|\.loaded\s*=|\.installed\s*=|\.pending_install|\.reload_backup|\.loaded_payload|\.installed_payload|\.error\s*=|\.failure_phase\s*=|\.dependencies\s*=|records\[|records\.get_mut" src\asset\server.rs` (no hits)
- `cargo test --features asset import_audio`
- `cargo test --features asset cook_all_writes_manifest_provenance`
- `cargo test --features asset verify_reports_manifest_provenance_drift`
- `cargo test --features asset install_context_dependency_handle_retains_declared_dependency`
- `cargo test --features app standard_material_cooker_and_factory_load_runtime_asset`
- `cargo test --features app standard_material_cooker_extracts_texture_dependencies`
- `cargo test --features app mesh_cooker_and_factory_load_runtime_asset`
- `cargo test --features app gltf_mesh_cooker_and_factory_load_runtime_asset`
- `cargo test --features app gltf_material_cooker_extracts_texture_dependencies`
- `cargo test --features app render::asset`
- `cargo test --features audio audio::assets`
- `cargo check`
- `cargo check --features audio`
- `cargo check --features video`
- `cargo check --features app`
- `cargo check --examples --features app`
- `cargo test --features app render::gpu::texture::tests`
- `cargo test --features video video::streaming`
- `cargo check --features asset`
- `cargo check --examples --features asset`
- `cargo test --features app app::asset_diagnostics` (12 tests, including provider resolve-failure and cache-invalidation diagnostics)
- `cargo check --features app`
- `cargo fmt --check`
- `git diff --check -- src\asset\mod.rs src\app\asset_diagnostics.rs docs\reference\asset.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features asset asset::diagnostics` (7 tests, including failed request failure-phase snapshots)
- `cargo test --features app app::asset_diagnostics` (12 tests, including failed request failure-phase event fields)
- `cargo check --features asset`
- `cargo check --features app`
- `git diff --check -- src\asset\types.rs src\asset\request.rs src\asset\diagnostics.rs src\app\asset_diagnostics.rs docs\reference\asset.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features asset active_request_snapshot_reports_dependency_blockers_and_last_error` (active request snapshots hydrate failure phase with blocker/error context)
- `cargo test --features app publish_snapshot_reports_slow_active_request_progress` (slow active request diagnostics include `failure_phase`)
- `cargo test --features asset asset::diagnostics` (7 tests)
- `cargo test --features app app::asset_diagnostics` (12 tests)
- `cargo fmt --check`
- `git diff --check -- src\asset\diagnostics.rs src\app\asset_diagnostics.rs docs\reference\asset.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features asset failed_request_snapshot_preserves_failure_context_after_record_recovers`
- `cargo test --features asset failure_helper_records_active_request_snapshot`
- `cargo test --features asset asset::failure` (2 tests)
- `cargo test --features asset asset::diagnostics` (8 tests, including preserved failed-request failure context after recovery)
- `cargo test --features asset asset::request` (7 tests)
- `cargo check --features asset`
- `cargo fmt --check`
- `git diff --check -- src\asset\request.rs src\asset\failure.rs src\asset\diagnostics.rs docs\reference\asset.md docs\plan\sakura_resource_system_adaptation_plan.md`
- `cargo test --features asset burst_background_loads_stay_bounded_by_worker_pool_and_queue`
- `cargo test --features asset asset::server::tests` (64 tests, including bounded burst-load stress regression)
- `cargo check --features asset`
- `cargo test --features asset same_frame_drop_and_reacquire_keeps_latest_lease_alive`
- `cargo test --features asset asset::server::tests` (65 tests, including same-frame load/drop/reacquire lease stress)
- `cargo check --features asset`
- `cargo test --features asset large_dependency_graph_loads_reloads_and_releases_closure`
- `cargo test --features asset burst_background_loads_stay_bounded_by_worker_pool_and_queue` (bounded worker-plus-queue capacity invariant)
- `cargo test --features asset asset::server::tests` (66 tests, including burst backpressure, same-frame lease, and large dependency graph stress)
- `cargo check --features asset`
- `git diff --check -- src/asset/install.rs src/asset/server.rs src/audio/assets.rs src/render/asset/runtime_factory.rs src/render/asset/tests.rs src/render/asset/mod.rs docs/reference/asset.md docs/plan/sakura_resource_system_adaptation_plan.md`
- `git diff --check -- src/asset/types.rs src/asset/mod.rs src/asset/registry.rs src/asset/load.rs src/asset/texture.rs src/asset/font.rs src/asset/cook.rs src/asset/server.rs src/audio/assets.rs src/video/assets.rs docs/reference/asset.md docs/plan/sakura_resource_system_adaptation_plan.md`
- `git diff --check -- docs/plan/world_resource_governance_plan.md docs/plan/sakura_resource_system_adaptation_plan.md`
- `rg --glob '!docs/plan/sakura_resource_system_adaptation_plan.md' -n "Handle<T> is a typed weak|AssetRef<T> is the runtime strong|Keep Handle<T> as weak|Handle<T> is weak identity|AssetRef<T> is strong" docs README.md README_zh.md examples`
- `rg -n "Sky 最大短[板]|背景加载直[接]|install 同步完[成]|AssetEvent 有[限]|还没有 request object [化]|install 目前只有数量预[算]" docs/plan/sakura_resource_system_adaptation_plan.md`
- `git diff --check -- docs/plan/sakura_resource_system_adaptation_plan.md`

### 0.1 Asset Documentation Reconciliation

Current authority order for asset-resource work:

1. Current source code in `src/asset/`, `src/app/services.rs`, and backend caches.
2. This Sakura adaptation plan, because it tracks the active implementation.
3. `docs/plan/asset_smart_handle_migration_plan.md` for handle lifetime semantics.
4. `docs/plan/backend_residency_cache_contract.md` for backend-owned GPU/audio/video residency rules.

Reconciled decisions:

- `Handle<T>` is the normal strong runtime handle. It is `Clone`, not `Copy`, and owns a lease through `AssetLease`.
- `WeakHandle<T>` is the weak identity handle. Serialized/editor references should use `AssetId`, `WeakHandle<T>`, or a later typed `AssetPath<T>`, not a strong runtime handle.
- Older asset planning drafts that described `Handle<T>` as weak and `AssetRef<T>` as strong have been removed; the current implementation and strong-handle plan are authoritative.
- `Assets` remains the only normal public facade. Internal pieces such as request queue, provider, I/O service, install queue, and diagnostics should stay private unless exposed as snapshots/reports. New execution logic should land in focused internal modules or backend caches, not broaden `Assets` / `server.rs` into a god class.
- No god-class rule: asset core owns identity, CPU payload state, dependencies, leases, request progress, and semantic events only; GPU/audio/video/native residency, memory pressure policy, playback refresh, and renderer-specific cache invalidation must remain in their backend modules.
- `AssetRuntimeFactory::begin_install` is the single runtime install entry, and `AssetRuntimeFactory::uninstall` is the optional per-type uninstall hook. The older synchronous erased install compatibility path has been intentionally removed.
- Runtime asset identity/lifecycle remains in `asset`; GPU/audio/video residency remains owned by render/audio/video backends.
- Hot reload is now manual (`reload_changed_with_report` / `force_reload`), config-controlled automatic polling (`with_auto_reload`), and optionally native event driven (`asset-watch` + `with_file_watcher(true)`) with path-carrying watch events, source/cooked/package-root path-to-asset resolution, external package-root and package-file watch registration, debounce, pending-root batching, freeze, and status snapshots.
- The current provider layer supports local cooked/raw sources, package-root fallback for cooked artifacts, simple indexed package-file fallback with provider-owned bundle-index caching/invalidation plus read-only provider stats, raw source request key/path normalization, test memory sources, record-to-entry fallback for raw runtime assets, record-to-source resolution through an `AssetRegistry` read interface, and watcher-event-to-provider-cache invalidation. The current manifest registry exposes focused metadata, dependency, lookup, and watch-key queries; app/tooling code can query read-only metadata/watch-path snapshots without taking over registry ownership. Provider stats now identify which source locations are actually being resolved and when provider caches are invalidated, so package behavior is observable without exposing cache mutation APIs. Compressed/encrypted/archive-backed VFS packages are still future work; package-file/manifest-level watch events intentionally remain scan triggers instead of becoming `Assets`-owned package managers.
- The current registry layer owns manifest file loading/version validation through an internal `AssetRegistryLoader` seam, manifest indexing through `LocalManifestRegistry`, source-path lookup, manifest refresh reconciliation including missing-record failure application, runtime factory registration/lookup, and product-type validation. Richer non-local registry implementations can now plug into the loader boundary, while compressed/encrypted/package-catalog registries remain future work.
- Cooking extensibility now supports custom `CookRegistry` instances in addition to built-in cooker descriptors.

Still open from the old plans:

- First-class `AssetPath<T>` has been introduced for source-path references; id/path migration helpers now cover manifest `AssetId -> source_path` and typed `AssetId -> AssetPath<T>` conversions for editor tooling.
- Request state is partially objectified with request ids, phases, priorities, generation, queued/active/phase timestamps, queued and active snapshots, dependency blocker diagnostics, last-error diagnostics, stats counters, completed timing averages, per-phase timing averages, source-load read/decode/total timing averages, optional install-task progress, queued activation/active refresh helpers, post-drive stale source-load cancellation before phase refresh, an internal `AssetRequests` manager, and a focused record-iteration driver; `server.rs` still coordinates high-level request facade calls, provider/registry/factory orchestration, delegated completion application, and semantic event emission points.
- Dependency graph traversal/error semantics/cycle checks plus dependency-failure record/event/request application now live in `src/asset/dependency.rs`. Request activation/active refresh, post-drive stale source-load cancellation, and request-phase derivation now live in `src/asset/request.rs`. Handle release channel draining/application, post-release stale-load cancellation, direct strong-handle lease acquisition/request enqueueing, and raw texture/font source acquisition now live in `src/asset/lease.rs`. Normal/blocking record-iteration scheduling now lives in `src/asset/driver.rs`. Focused failure lifecycle application and failed-request snapshot retention now live in `src/asset/failure.rs`. Runtime insertion/replacement event and dependency-release semantics, including registry-backed dependency lease updates, now live in `src/asset/runtime.rs`. Manifest file loading/version validation, indexing/source lookup/cooked and package-root watch-path resolution/refresh reconciliation, missing-record refresh failure application, factory registration, manifest-entry lookup, typed source-path resolution, metadata/dependency/watch-key read queries, typed `AssetPath<T>` reconstruction, and typed product validation now live in `src/asset/registry.rs`; manifest-backed install/uninstall record orchestration, factory begin/pending-task polling, install budget gating, install success/failure dependency/event/release application, and factory uninstall success/failure driving now live in `src/asset/install.rs`. Event retention/cursor mechanics plus release, unload, loaded, installed, failed, and reload-queued semantic event helpers now live in `src/asset/events.rs`. Diagnostic DTO assembly now lives in `src/asset/diagnostics.rs`. Record-to-source resolution, raw source request normalization, package-root cooked fallback, simple package-file bundle fallback, raw manifest-entry fallback, and watch-event cache invalidation now live in `src/asset/provider.rs`. I/O worker/channel/in-flight mechanics, source-load helpers, load scheduling read decisions, record-level load orchestration, loaded-payload preparation, load success/failure application, dependency lease update fanout, stale completion filtering, and completion failure phase mapping now live in `src/asset/load.rs`. Manifest reload replacement, auto-reload scheduler state, changed-root fingerprint/hash comparison, provider-backed reload scan construction, watch-event targeted roots, dependent reload preparation, reload-root event/failure application, and reload report assembly now live in `src/asset/reload.rs`. Native watcher event draining and multi-root watcher registration now live in `src/asset/watcher.rs`. Dependency leases/replacement/update application, record/index ownership including raw source indexes, direct/dependency reference count mutation, unused-release scheduling/cascading, load activation/completion transitions, loaded payload completion mutation, failure record mutation, release/uninstall/unload record transitions, dependency readiness, install pending/ready record mutation, failed-reload restore, reload preparation record mutation, and runtime payload insertion/replacement are now inside `AssetStore`/`AssetRecord`; future work can keep shrinking `server.rs` around orchestration boundaries rather than adding a god class.
- Failed reload now keeps the last good installed payload for the asset core path; backend residency policies still need broader cross-backend standardization.
- Texture residency now has byte accounting, optional memory budget, LRU eviction, pin/unpin, asset-event generation/type filtering, and cached GPU-prepare failure reporting. Wgpu, Renderling, and Kajiya mesh/material residency now respond to source changes and asset event invalidation through their backend caches. Video playback residency now consumes `VideoClip` asset events and refreshes/stops active instances locally. Audio ECS emitters and direct playback now consume `SoundClip` / `MusicTrack` events locally, with direct playback intentionally not auto-restarted on installed reload events. `docs/plan/backend_residency_cache_contract.md` now defines the shared backend-residency rules; future renderer families and broader memory-pressure policy beyond texture budgets still need to apply and extend that contract locally.
- Built-in and custom cooker metadata are registry-driven; custom cooker version drift is covered by the same verify path.

## 1. 结论先行

SakuraEngine 对 SkyEngine 有参考价值，但最值得“抄”的不是具体代码，而是资源系统的分层和生命周期协议：

- `ResourceSystem` 统一调度资源请求。
- `ResourceRegistry` 负责把资源标识解析到真实文件、依赖和 metadata。
- `ResourceFactory` 明确拆开 I/O、反序列化、依赖等待、安装、卸载、跨帧安装轮询。
- 资源记录有显式状态机，能表达 `Loading -> Loaded -> WaitingDependencies -> Installing -> Installed`。
- I/O 服务和资源系统分离，理论上支持优先级、批处理、取消、RAM/VRAM staging。
- handle 持有引用，释放通过系统队列回收，避免直接把释放逻辑散落到调用者。

SkyEngine 已经有一套相当接近的雏形，不是空白：

- `src/asset/` 已有 `Assets` facade、`Handle<T>`、manifest/cook、依赖计数、reload closure、事件队列、后台加载开关、安装预算。
- `src/render/resources/texture_cache.rs` 已有 texture GPU residency、prepare budget、优先队列和事件驱动失效。
- `src/app/services.rs` / `src/app/lifecycle.rs` 已经把 asset/audio/video service update 纳入 app lifecycle。
- RenderGraph 已经实际借鉴过 Sakura 的 reorder、aliasing、blackboard 思路，说明“参考 Sakura 但 Rust 化落地”这条路线可行。

核心差距已经从“缺少运行时调度层”收窄为“继续压实内部边界和跨后端一致性”。最初暴露出的 thread-per-load、固定轮询 `load_blocking`、同步 install、缺少基础 diagnostics、hot reload 只能手动触发等问题已经被 bounded I/O service、blocking deadline、install task、request/asset diagnostics、自动 reload / watcher 和后端 residency contract 覆盖。剩余重点是：继续缩小 `server.rs` 编排面、让未来 registry/provider/cooker 扩展不回流成中心分支、把新 renderer/audio/video/native residency 路径按 backend-local contract 补齐，并保持 `Assets` 只负责身份、CPU payload、依赖、lease、request progress 和语义事件。

所以计划应当是：保留 SkyEngine 当前 Rust API 和 ECS/app ownership 模型，吸收 Sakura 的资源生命周期协议和调度分层，逐步把现有 `asset` 模块从“manifest + handle + factory loader”升级成“可诊断、可预算、可取消、可热重载、可跨帧安装”的资源运行时。

## 2. 不照抄的边界

这些内容不建议照搬：

- 不引入 Sakura 那种全局 singleton `resource_system`。SkyEngine 应继续通过 `World` 资源和 app services 管理生命周期。
- 不照搬 C++ pointer/record 双态 handle。SkyEngine 的 `Handle<T>` / `WeakHandle<T>` 应保持类型安全、generation 校验和 Rust ownership 语义。
- 不把 DirectStorage、CGPU、Sakura VFS 的具体实现移植过来。SkyEngine 现在需要的是抽象 seam，不是平台专用实现。
- 不把 renderer backend 细节塞进 `asset`。GPU residency 应由 render/audio/video 等后端资源层消费 asset events 后自行管理。
- 不把资源系统重写成一个大而全的公共 service API。先保持 `Assets` 作为 app-facing facade，逐步拆内部模块。
- 不把 Sakura 目前也未完成的地方当成标准。例如 Sakura snapshot 中 async serde 被硬关、`FlushResource` 未实现、local registry cancel 是空实现，这些只能作为 warning，不是模板。

## 3. 参考源清单

SakuraEngine 重点参考：

- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_system.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_handle.h`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_factory.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_header.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_system.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_request_impl.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_request.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_handle.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/local_resource_registry.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_factory.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_header.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/agent_docs/core_systems/resource_system.md`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/agent_docs/core_systems/io_service.md`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/io/ram_io.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/io/vram_io.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/io/ram/ram_service.cpp`

SkyEngine 当前实现重点对照：

- `src/asset/mod.rs`
- `src/asset/types.rs`
- `src/asset/server.rs`
- `src/asset/registry.rs`
- `src/asset/texture.rs`
- `src/asset/cook.rs`
- `src/render/resources/texture_cache.rs`
- `src/render/runtime/frame/extract_frame.rs`
- `src/render/runtime/frame/prepare_frame_resources.rs`
- `src/app/services.rs`
- `src/app/lifecycle.rs`
- `src/app/frame.rs`
- `src/audio/assets.rs`
- `src/video/assets.rs`
- `docs/plan/asset_smart_handle_migration_plan.md`
- `docs/plan/world_resource_governance_plan.md`

## 4. Sakura 和 SkyEngine 逐项对比

| 维度 | SakuraEngine | SkyEngine 当前状态 | 差距判断 |
| --- | --- | --- | --- |
| 资源入口 | `ResourceSystem` 统一拥有 request queue、registry、factory、records | `Assets` facade 保持 public 入口；store/request/io/load/install/events/diagnostics/reload/provider/registry 已拆为内部模块 | 方向一致，Sky 更 Rust 化；后续重点是继续让 `server.rs` 只做高层编排 |
| 资源标识 | `GUID` / `SResourceRecord*` 双态 handle | `AssetId`、`Handle<T>`、`WeakHandle<T>`、generation、typed runtime id | Sky 更安全，不需要照搬 Sakura handle |
| handle 引用 | `SResourceHandle` RAII 引用计数，drop 释放 | `Handle<T>` 持有 `AssetLease`，drop 走 release channel | 思路一致；需继续巩固 strong/weak 语义文档 |
| 资源状态 | `Unloaded, Loading, Loaded, WaitingDependencies, Installing, Installed, Uninstalling, Unloading, Error` | `AssetState` 已覆盖 load/dependency/install/uninstall/unload/failure，失败 phase 和 request phase 可诊断 | 状态语义已基本对齐，继续保持迁移集中在 store/request/install/failure/reload slice |
| 加载请求 | `ResourceRequest` 独立对象，`Update()` 驱动状态机 | `AssetRequests` 管 queued/active/recent failed request、priority、timing、blocker/progress 快照；driver/load/request 模块驱动主要状态推进，post-drive source-load cleanup 也回到 request 边界 | 已吸收 Sakura 的 request-object 思路，剩余是继续减少 `server.rs` 高层 glue |
| I/O | Registry 生成 IO request，RAM/VRAM service 分层 | `AssetIoService` bounded worker pool + priority queue + cancellation + queue/backpressure stats；provider 解析 source 后提交 load queue | 原 thread-per-load 短板已关闭；VRAM/native I/O 仍属于 backend，不进 asset core |
| 反序列化 | Factory `Deserialize`，理论支持 async serde | Factory `load(ctx)` 仍保持同步 decode，但 source-load read/decode timing、worker phase 和 progress 已可诊断 | 简洁 API 保留；超重 decode 可继续通过 worker/progress 和未来专用 factory 策略演进 |
| 依赖 | `LoadResource` 收集 dependencies，`WaitingDependencies` 等待 | `LoadedAsset.dependencies` + dependency_ref_count + cycle check | Sky 已有基础，需更好诊断和 request-level 解释 |
| 安装 | `Install` 可返回 installing，`UpdateInstall` 跨帧轮询 | `AssetRuntimeFactory::begin_install` 可返回 ready 或 pending `AssetInstallTask`，`AssetInstallBudget` 支持跨帧 polling；audio 已作为真实试点 | Sakura install 协议已 Rust 化落地，未来按资源类型 opt-in |
| 卸载 | `Unload/Uninstall` 进入 request 状态机 | release queue + dependency release + factory uninstall hook + unload events | Sky 已具备基础生命周期 hook，未来可继续补 provider/package 级 flush |
| 热重载 | Resource registry 和 request 体系可支撑 | manual reload、forced reload、auto polling、debounce、freeze、optional watcher、targeted watch roots、reload reports 都已存在 | 基础能力已补齐；复杂 package catalog/watch 仍保持 provider/registry 后续扩展 |
| GPU/native residency | 有 RAM/VRAM IO 概念，资源安装可贴近 GPU | texture cache、WGPU/Renderling/Kajiya mesh/material caches、audio/video playback residency 均按 backend-local event cursor/diagnostics 管理 | 边界正确：继续按 contract 扩新后端，不把 GPU/audio/video 放进 asset core |
| 诊断 | request/state/counter 可观测 | `AssetStats`、request timings、queued/active/failed snapshots、reload/failure/backpressure/provider stats、render/audio/video stats 已有 app diagnostic events | Sakura 的可观测性已大体吸收；后续是补新路径诊断而不是做中心监控类 |
| app 集成 | 系统 update 驱动 request queue | App lifecycle 每帧 update asset/audio/video | Sky 模型更符合当前 engine，继续沿用 |
| Rust 安全 | C++ raw pointer/atomic/counter | Rust typed handle / channels / Result | Sky 不应回退到 pointer 风格 |

## 5. 对 SkyEngine 当前问题的反思

### 5.1 `server.rs` 正在承担太多职责

`src/asset/server.rs` 同时做了 facade、record store、manifest lookup、factory dispatch、background load spawning、load completion、dependency reference、reload closure、release queue、events 和 state transition。短期能迭代很快，但继续加 hot reload、priority、cancel、cross-frame install 后会变成风险点。

需要拆的不是 public API，而是内部职责：

- `store`: record/generation/state/leases。
- `request`: request state machine。
- `io`: bounded background read/decode worker。
- `registry`: manifest/provider lookup。
- `install`: install budget and poll protocol。
- `events`: typed asset event generation。
- `diagnostics`: stats and tracing payload。

### 5.2 后台加载策略已经有引擎级骨架

最初的问题是“有线程但没有资源调度器”：thread-per-load、无 back pressure、无优先级、无取消、无进度、无 shutdown 语义。当前实现已经把这条线迁到 `src/asset/io.rs` / `src/asset/load.rs`：

- bounded worker pool 控制并发和队列容量。
- priority queue 支持 request priority，同优先级保持 FIFO。
- cancel token 让释放后的未启动任务可跳过，完成后的 stale generation 也会丢弃。
- worker-side read/decode phase、source-load timing、deferred submission/backpressure 都进入 stats/diagnostics。
- shutdown timeout 明确 worker teardown 行为。

后续不要再把 I/O 逻辑塞回 `server.rs`。如果接入压缩包、远程包、平台 I/O 或 DirectStorage-like 路径，应扩 provider/load/backend seam，而不是让 `Assets` 变成文件系统或 native I/O god class。

### 5.3 `load_blocking` 已从固定轮询改为显式 blocking path

早期 `load_blocking` 通过固定次数 `update()` 等待 background completion，不可靠且可能忙等。当前实现已经改为 blocking-specific update path，并提供 `load_blocking_with_timeout`：

- 支持 deadline / timeout。
- pending 时 sleep，避免纯忙等。
- 等待期间持续推进依赖和 install state。
- blocking path 使用无限 install budget，保留显式阻塞语义。
- 失败通过 `AssetFailurePhase` / failed request snapshot / failed asset snapshot 解释 lookup/read/decode/dependency/install/uninstall/runtime/verification 等阶段。

剩余工作不是再重写 blocking API，而是确保新引入的 provider、factory、install task 或 watcher 路径都能被 blocking path 和失败诊断覆盖。

### 5.4 install 已支持跨帧 task，但必须保持 opt-in

`install_budget_per_update` 只能限制每帧完成几个安装，不能拆分单个超重安装；这一点已经通过 `AssetRuntimeFactory::begin_install -> AssetInstallResult` 和 `AssetInstallTask::poll_install` 补齐：

- 默认 factory 仍可返回 `Ready`，简单资源不承担复杂协议。
- 高风险资源类型可返回 `Pending(Box<dyn AssetInstallTask>)`。
- `AssetInstallBudget` 支持按 update 剩余时间预算推进。
- `AssetState::Installing` 持有 pending task，请求快照可报告当前 install phase age 和 task progress。
- 音频 `SoundClip` / `MusicTrack` 已作为真实跨帧 install 试点。

后续 mesh/material/audio/video 等资源只有在确实需要拆分重安装时才 opt-in。不要为了统一形式把所有 factory 都包装成异步对象。

### 5.5 CPU asset 和 backend residency 的边界要更明确

SkyEngine texture 已经有 `RenderAssetCache`，这是正确方向：asset core 管 CPU 侧身份、bytes、metadata、events，render runtime 管 GPU residency、upload、eviction。

不要把 Sakura 的 VRAM IO 直接变成 `asset` 的 GPU owning 层。应该定义 backend-neutral hooks：

- asset event 告诉后端“某 asset installed/reloaded/unloaded/failed”。
- render/audio/video 后端各自维护 native resource cache。
- asset core 提供 pin/lease/priority metadata，而不直接拥有 GPU/audio/video device 对象。

### 5.6 文档存在代际冲突，需要先统一口径

现有计划文档中有一处需要重新定稿：旧的 asset standard 曾把 `Handle<T>` 定义为弱身份、`AssetRef<T>` 定义为强引用；较新的 smart handle migration 和当前代码已经倾向 `Handle<T>` 为强 handle、`WeakHandle<T>` 为弱身份。

本计划建议选择当前实现方向：

- `Handle<T>`: strong lease，clone 增加 lease，drop 进入 release queue。
- `WeakHandle<T>`: identity only，不保持资源存活，需要通过 `Assets` upgrade/resolve。
- 如果需要直接访问 installed asset，可另设 `AssetRead<T>` / `AssetView<T>` 这类临时 borrow guard，而不要把 `AssetRef<T>` 再作为主概念。

后续应更新旧计划，避免后续实现者照旧文档反向迁移。

## 6. 目标架构

目标不是重写成 Sakura，而是把 SkyEngine 资产系统演进为下面这组内部层：

```text
App / World
  |
  v
Assets facade
  |
  +-- AssetStore
  |     - records
  |     - generations
  |     - leases
  |     - typed asset slots
  |
  +-- AssetRegistry
  |     - manifest lookup
  |     - file/provider resolution
  |     - dependency metadata
  |
  +-- AssetRequestQueue
  |     - request ids
  |     - state machine
  |     - priority/cancel/progress
  |
  +-- AssetIoService
  |     - bounded worker pool
  |     - read/decode jobs
  |     - completion channel
  |
  +-- AssetInstallQueue
  |     - per-frame budget
  |     - install tasks
  |     - dependency waiting
  |
  +-- AssetEvents / AssetDiagnostics
        - loaded/installed/reloaded/unloaded/failed
        - queue depth/state time/failure context

RenderRuntime / Audio / Video / Tools
  |
  v
Backend-owned residency caches
  - texture GPU cache
  - mesh/material GPU cache
  - audio buffer/stream cache
  - video frame queues
```

Public API 目标：

- 继续以 `sky_engine::asset::Assets` 作为主入口。
- 保留 `Handle<T>` / `WeakHandle<T>` 类型安全能力。
- 保留 `AssetRuntimeFactory` 简洁路径，同时扩展 optional async/install task 能力。
- 不要求游戏代码知道 request object、worker pool、registry provider 的内部结构。

## 7. 分阶段执行计划

### Phase 0: 文档和现状对齐

目标：在动代码前，先消除文档漂移，避免越改越乱。

工作项：

1. 对 `docs/plan/asset_smart_handle_migration_plan.md`、`docs/plan/backend_residency_cache_contract.md`、`docs/plan/world_resource_governance_plan.md` 做一次只读审计，列出已完成、过时、仍有效的条目。
2. 明确 strong handle 标准：`Handle<T>` 保持资源 lease，`WeakHandle<T>` 仅身份。
3. 明确 `Assets` 是 app-facing facade，不新增全局 `ResourceSystem`。
4. 给未来代码改造建立 issue-style checklist，避免同一问题散落多个计划文件。
5. 为当前 asset 模块补一张实际状态图，和本计划目标状态图对齐。

交付物：

- 更新或追加一份 asset docs reconciliation note。
- 不改变运行时代码。

验收：

- 计划文档之间不再互相矛盾。
- 新实现者能从 docs 判断 `Handle<T>` 到底是不是 strong。

### Phase 1: 修正 correctness 和阻塞语义

目标：先修会导致误判、卡顿或难排查的基础问题。

工作项：

1. 重写 `load_blocking`：
   - 不再用固定 `0..64` 轮询作为完成条件。
   - 增加明确的 blocking read/decode path，或等待 request completion。
   - 增加 timeout/deadline variant，例如 `load_blocking_with_timeout`。
   - 等待期间推进依赖和 install。
   - 错误中带上 asset id、kind、path、current state。
2. 梳理 stale generation 行为：
   - `Handle<T>` release 应按 direct lease 计数处理，而不是按 `load_generation` 自动 no-op；跨 reload 仍然存活的强 handle 仍必须能释放自己的 lease，否则会泄漏。
   - queued old release 到达时，如果同一 `AssetId` 已通过旧 `WeakHandle<T>` 重新获得新强 handle，只能释放旧 lease，不能误卸载新 handle 保持的最新 generation。
   - reload 后旧 weak handle resolve/load 必须按稳定 `AssetId` 语义拿到当前 generation，并继续执行 manifest type 校验。
3. 强化 `AssetEvent`：
   - 区分 loaded/installed/reloaded/unloaded/failed。
   - failure event 带 phase：lookup/read/decode/dependency/install。
4. 为 dependency wait 添加诊断：
   - 依赖缺失、依赖失败、循环依赖、仍在等待应能区分。
5. 补测试：
   - blocking load 成功。
   - blocking load 失败。
   - blocking load 依赖链。
   - release stale generation 不误删新资源。
   - dependency failure 正确传播。

建议涉及文件：

- `src/asset/server.rs`
- `src/asset/types.rs`
- `src/asset/registry.rs`
- `src/asset/mod.rs`

验收命令：

```powershell
cargo test asset::
cargo test
```

### Phase 2: 引入 bounded Asset I/O service

目标：把每资源 `std::thread::spawn` 改成统一、有 back pressure 的 I/O 层。

工作项：

1. 新增 `src/asset/io.rs`。
2. 定义内部类型：
   - `AssetIoService`
   - `AssetIoRequest`
   - `AssetIoResponse`
   - `AssetIoPriority`
   - `AssetIoCancelToken`
3. worker pool 配置进入 `AssetConfig`：
   - `io_worker_threads`
   - `io_queue_capacity`
   - `default_priority`
   - `shutdown_timeout`
4. `AssetsInner` 不再直接 `std::thread::spawn`。
5. worker job 初期只做 file read + factory load，先不拆 async serde。
6. 支持取消：
   - handle 释放且无 lease 时，可标记未开始 request canceled。
   - 已开始的 file read 可允许跑完，但 completion 到达时丢弃。
7. 支持优先级：
   - 初期可先排序 pending queue。
   - 后续由 render/camera/tooling 提供 priority hints。
8. 支持 app shutdown：
   - drop `Assets` 时关闭 worker sender。
   - join workers 或明确 detach 策略。

建议涉及文件：

- `src/asset/io.rs` 新增
- `src/asset/server.rs`
- `src/asset/types.rs`
- `src/app/services.rs`

验收：

- 大量 asset 请求不会创建大量 OS threads。
- worker shutdown 测试稳定。
- 释放未开始资源后不会继续安装。
- 背景加载行为和现有 public API 兼容。

验收命令：

```powershell
cargo test asset::
cargo test --features app asset::
```

### Phase 3: 把资源请求状态机对象化

目标：借鉴 Sakura `ResourceRequest::Update()` 的结构，把状态迁移从 `server.rs` 巨型流程中拆出来。

工作项：

1. 新增 `src/asset/request.rs`。
2. 定义内部 `AssetRequest`：
   - request id
   - asset id
   - generation
   - current phase
   - priority
   - dependency handles
   - started/completed timestamps
   - last progress
   - cancel flag
3. 定义 `AssetRequestPhase`，和 `AssetState` 对齐但不完全等价：
   - `Queued`
   - `Loading`
   - `Decoding`
   - `WaitingDependencies`
   - `ReadyToInstall`
   - `Installing`
   - `Installed`
   - `Unloading`
   - `Failed`
   - `Canceled`
4. `AssetState` 保持 public/debug-facing 简化状态。
5. `AssetsInner::update()` 改成：
   - drain releases
   - submit new requests
   - poll I/O completions
   - advance requests
   - apply install budget
   - emit events
6. request transition 必须集中写测试，不依赖真实文件系统。

验收：

- `server.rs` 复杂度下降。
- 每个 request 可以解释自己为什么等待。
- 后续 install task 和 diagnostics 有落点。

验收命令：

```powershell
cargo test asset::request
cargo test asset::
```

### Phase 4: 引入跨帧 install task

目标：补上 Sakura `Install -> Installing -> UpdateInstall` 的能力，但保持 Rust API 简洁。

建议 API：

```rust
pub enum AssetInstallResult<T> {
    Ready(T),
    Pending(Box<dyn AssetInstallTask<Output = T> + Send>),
}

pub trait AssetInstallTask {
    type Output;

    fn poll_install(
        &mut self,
        ctx: &mut AssetInstallContext<'_>,
        budget: AssetInstallBudget,
    ) -> AssetInstallPoll<Self::Output>;
}
```

实际落地可以更保守：

- 第一版不必让 trait object 暴露到 public API。
- 先在内部支持 `InstallTask`，同步 factory 自动包装成 immediate ready。
- 资源类型需要时再 opt-in。

工作项：

1. 将 `AssetRuntimeFactory` 收敛到跨帧安装模型：
   - 使用 `begin_install(...) -> AssetInstallResult<Asset>` 作为唯一 runtime install 入口。
   - 同步安装返回 `AssetInstallResult::Ready(asset)`。
   - 跨帧安装返回 `AssetInstallResult::Pending(task)`。
   - 不保留旧 erased 同步 `install(...)` 兼容路径。
2. `AssetState::Installing` 持有 install task。
3. `AssetConfig` 增加时间预算：
   - `install_budget_per_update` 保留。
   - 新增 `install_time_budget`。
4. `update()` 每帧 poll installing tasks。
5. 首批迁移高风险类型：
   - audio decode/install。
   - large texture CPU preparation。
   - 后续 mesh/material/gltf。
6. Factory 生命周期包含可选 `uninstall`：
   - manifest/raw 资产在 `Uninstalling -> Unloading` 时调用 factory hook。
   - runtime-inserted assets 不强制要求 factory，仍依靠 Rust drop 和 backend event policy。

验收：

- 一个超重 asset install 不会独占整帧。
- 现有简单 asset factory 不需要改代码。
- installing 状态可被 diagnostics 观测。
- 需要释放 runtime-side hooks 的 factory 可以观察到 uninstall。

验收命令：

```powershell
cargo test asset::
cargo test --features app
```

### Phase 5: Registry / Provider / VFS 层

目标：吸收 Sakura registry 的好处，让资源定位不再只等于本地 manifest + path。

工作项：

1. 新增或重构 `AssetRegistry` trait：
   - `resolve(asset_id) -> AssetLocation`
   - `read_metadata(asset_id) -> AssetMetadata`
   - `dependencies(asset_id) -> Vec<AssetId>`
   - `watch_key(asset_id) -> Option<WatchKey>`
2. 当前 manifest registry 作为 `LocalManifestRegistry`。
3. 支持多 provider：
   - local file provider。
   - cooked package provider / package root fallback。
   - simple package-file fallback。
   - package-root watch-path lookup。
   - memory/test provider。
4. `AssetIoService` 只接受 resolved location，不做 manifest lookup。
5. cook manifest 和 runtime manifest 字段统一命名。
6. 测试用 memory provider 替代临时文件，减少 asset 单测对 filesystem 的依赖。

验收：

- runtime load 不关心资产来自 loose file 还是 package。
- tests 可以用 memory provider 构造失败/延迟/依赖场景。
- hot reload 能从 registry 拿到 watch key。

验收命令：

```powershell
cargo test asset::registry
cargo test asset::
```

### Phase 6: 后端 residency 标准化

目标：不把 GPU/audio/video native resource 放进 asset core，但建立统一约定，让后端缓存可观测、可预算、可响应 reload。

工作项：

1. 将 texture cache 的经验整理为 `BackendResidencyCache` 设计约定，而不是立即抽 trait。
   - 已建立：`docs/plan/backend_residency_cache_contract.md`。
   - 后续 backend 先按该契约补齐本地 cache、事件游标、source identity、失败缓存和诊断，再评估是否真的需要共享 trait。
2. 为 texture cache 补齐：
   - memory budget。
   - LRU/priority eviction。
   - pinning。
   - reload generation check。
   - upload failure event。
3. 资产事件携带 enough context：
   - asset id。
   - generation。
   - kind/type id。
   - dependency version/reload marker。
4. 后续新增：
   - mesh GPU cache。
   - material/pipeline cache。
   - audio buffer/stream cache。
   - video frame/source cache。
5. 保持 ownership：
   - `Assets`: CPU asset identity and lifecycle。
   - `RenderRuntime`: GPU texture/mesh/material residency。
   - `AudioServer`: decoded/streaming audio residency。
   - `VideoServer`: video source/frame residency。

验收：

- texture reload 后不会使用旧 generation GPU resource。
- residency eviction 不影响 strong asset handle 的 CPU 语义。
- render/audio/video backend 可以独立测试。
- 新增 backend residency 不需要改 `Assets` 或增加一个全局 native resource manager。

验收命令：

```powershell
cargo test --features app render::runtime::tests
cargo test --features app
```

### Phase 7: Hot reload 自动化和诊断

目标：把 `reload_changed()` 从手动工具函数升级为 editor/dev 可依赖的服务。

工作项：

1. 增加可选 file watcher：
   - feature gated 或 config controlled。
   - debounce。
   - batch changes per frame。
2. reload 过程输出解释：
   - changed asset。
   - affected dependent closure。
   - skipped reason。
   - failed phase。
3. diagnostics 增加：
   - queue depth。
   - active request count。
   - per-state count。
   - average queue-wait / active / total request time。
   - per-phase request timing plus source-load read/decode/total timing。
   - failed request list。
4. app diagnostics 集成：
   - `src/diagnostics/` event。
   - console output 可选择展示 asset stats。
5. editor-facing API：
   - query reload status。
   - force reload asset。
   - freeze reload during edit transaction。

验收：

- 修改 loose file 后，dev config 下自动 reload。
- 依赖资源会按闭包 reload。
- 失败不会让旧 installed asset 立即消失，除非策略明确要求。

验收命令：

```powershell
cargo test asset::
cargo test --features app
```

### Phase 8: Cooking 扩展点和 runtime factory 对齐

目标：把 cooked asset pipeline 从 hard-coded kind 分支升级为可扩展注册。

工作项：

1. 定义 cooker registry：
   - runtime asset kind。
   - source extensions。
   - output cooked kind/version。
   - dependency extraction。
   - incremental hash。
2. 让 runtime `AssetRuntimeFactory` 和 cooker metadata 对齐：
   - type id。
   - kind string。
   - version。
   - dependency schema。
   - 已建立：`AssetRuntimeFactory::cooked_schema()` / `AssetCookedSchema` 让 factory 可声明可消费的 cooked schema；load path 对 cooked/package/bundle 源做 manifest schema mismatch 检查，raw 源不受影响。
3. 逐步迁移：
   - texture。
   - font。
   - audio。
   - mesh/material/gltf。
   - material 已建立 render-local `StandardMaterialAsset` cooker/factory 注册点，并已为 `.skymaterial` 纹理引用补上 render-local dependency extraction 和 install-time strong `Handle<TextureAsset>` 绑定；mesh 已建立 render-local `.skymesh` CPU `MeshAsset` cooker/factory；gltf/glb 默认接入 render-local mesh cooker 并输出 `.skymesh` CPU schema，显式 `import_settings.asset_type = "standard_material"` 时接入 render-local material cooker 并输出 `.skymaterial` CPU schema，不放进 asset 内置中心表。
4. cook manifest 记录：
   - source hash。
   - cooker version。
   - dependency hash。
   - platform/profile。
   - 已建立：`AssetManifestProvenance` 随 generated manifest 写入 source/cooked/dependency hash、target platform 和 `AssetConfig::profile`，`verify` 会检查 provenance drift。
5. verify 不再只检查固定 kind，而是询问 registered cooker。

验收：

- 新 asset kind 不需要改 cook 中央 match。
- cook verify 能发现 cooker version drift。
- runtime load error 能指出 cooked schema mismatch。

验收命令：

```powershell
cargo test asset::cook
cargo test asset::
```

### Phase 9: API 收口和文档清理

目标：避免实现完成后留下多套相互竞争的名词。

工作项：

1. 更新 `README.md` / `README_zh.md` 的 asset quick-start。
2. 更新 `docs/` 中 asset 标准文档：
   - `Assets`
   - `Handle<T>`
   - `WeakHandle<T>`
   - factory。
   - cooker。
   - hot reload。
3. 删除或标记过期的执行计划段落。
4. 给 examples 增加最小覆盖：
   - load texture。
   - hot reload texture。
   - load asset with dependency。
   - custom asset factory。
5. 对 app/render example 做 compatibility check。

验收命令：

```powershell
cargo test
cargo test --features app
cargo check --examples --features app
```

## 8. 最小可行路线

这条 MVP 已基本按阶段落地，后续实现不要再回到大一统重写：

1. Phase 1: 修 `load_blocking`、错误语义、dependency failure。
2. Phase 2: 引入 bounded I/O service，替换 thread-per-load。
3. Phase 3: request state machine 对象化。
4. Phase 4: install task 协议，但只迁移一个真实重资源类型。
5. Phase 7: 基础 diagnostics。

这五步让 SkyEngine 的资源系统已经从“可用模块”进入“引擎级调度层”的形态。现在的路线应转为：

1. 继续缩小 `server.rs`，但只沿已经存在的 focused modules 拆，不引入新总管。
2. Phase 5/8 的 registry/provider/cooker 扩展继续走局部 extension point。
3. Phase 6 的 native residency 继续由 render/audio/video/backend cache 自己消费 asset events。
4. Phase 9 持续清理旧文档和例子，避免后续实现者按过期术语反向迁移。

## 9. 具体代码落点建议

已新增并应继续维护的内部文件：

- `src/asset/io.rs`: bounded worker pool、I/O request/response、priority/cancel。
- `src/asset/load.rs`: worker channel/in-flight mechanics、source-load helper、record-level load orchestration。
- `src/asset/request.rs`: request state machine、phase transition、progress、queued/active/failed snapshots。
- `src/asset/install.rs`: install task、budget、poll result。
- `src/asset/store.rs`: record/generation/lease 管理，逐步从 `server.rs` 拆出。
- `src/asset/events.rs`: AssetEvent 和 failure phase 细化。
- `src/asset/diagnostics.rs`: stats snapshot、state time、queue depth。
- `src/asset/provider.rs`: registry/provider/VFS abstraction。
- `src/asset/reload.rs`: reload scan/status/report/debounce/freeze/root application。
- `src/asset/watcher.rs`: optional native watcher root selection and event drain。
- `src/asset/failure.rs`, `src/asset/lease.rs`, `src/asset/runtime.rs`, `src/asset/dependency.rs`, `src/asset/driver.rs`: focused lifecycle slices。

优先修改：

- `src/asset/server.rs`: 只做 facade 和高层 coordinator，新增语义优先放进上述 focused modules。
- `src/asset/types.rs`: 仅放 public DTO/config/error/handle semantics，不塞调度逻辑。
- `src/asset/registry.rs`: manifest/loader/factory/schema/read model 边界。
- `src/asset/provider.rs`: local/package/bundle source resolution 与 provider-owned cache stats。
- `src/asset/cook.rs` 和 render-local cooker registries: 继续避免中心 multi-product branch。
- `src/render/resources/texture_cache.rs`、render backend caches、`src/audio/server.rs`、`src/video/server.rs`: backend-owned residency diagnostics/budget/reload invalidation。
- `src/app/asset_diagnostics.rs` / `src/app/render_diagnostics.rs` / `src/app/media_diagnostics.rs`: app-facing diagnostics mirror，不回写 asset core。

不建议优先修改：

- `src/main.rs`: scratch/local playground，不作为 API 方向。
- ECS query/chunk 热路径：资源计划不应牵连 ECS hot path。
- RenderGraph internals：除非 texture residency 需要 asset event 接口，否则不要动 graph compilation/alias/reorder。

## 10. 测试矩阵

### Unit tests

- handle strong lease clone/drop。
- weak handle resolve/failed resolve。
- release queue stale generation。
- request phase transition。
- dependency success/failure/cycle。
- blocking load timeout。
- background worker cancellation。
- install task polling。
- reload changed dependent closure。
- registry provider memory/local lookup。
- cook registry version mismatch。

### Integration tests

- app lifecycle 每帧 update assets。
- texture asset installed 后 render cache upload。
- texture reload 后 render cache invalidation。
- audio asset load/install 不阻塞 frame。
- hot reload debounce。

### Stress tests

- 1000 small assets background load，不创建 1000 OS threads。
- 大依赖图 load/reload。
- load 和 release 同帧交错。
- load failure + reload success。
- install task 超预算多帧完成。

### Commands

```powershell
cargo test asset::
cargo test
cargo test --features app
cargo test --features app render::runtime::tests
cargo check --examples --features app
```

如果改到 UI/neo/yakui/vn/audio/video feature，再按 AGENTS.md 中对应命令补跑。

## 11. 设计验收标准

完成后应满足：

- Public API 仍然以 `sky_engine::asset::Assets` 为主入口。
- `Handle<T>` strong、`WeakHandle<T>` weak 的语义清晰且有测试。
- 同时请求大量资源时线程数量有上限。
- asset load 可以被取消或至少 completion 被安全丢弃。
- 单个重 install 可以跨帧推进。
- asset failure 能说明失败阶段和具体资源。
- dependency wait 可诊断，不再只是“没装好”。
- hot reload 可以自动触发，并能解释 reload closure。
- render/audio/video native residency 不在 asset core 中乱耦合。
- texture cache 至少有 generation-safe reload。
- cooking pipeline 能注册新 asset kind，不需要持续改中心 match。
- docs 不再出现 `Handle<T>` strong/weak 两套说法。

## 12. 风险和应对

| 风险 | 表现 | 应对 |
| --- | --- | --- |
| 过度抽象 | 为了像 Sakura 引入太多 trait/service，简单资源也变复杂 | public API 保持 `Assets`，复杂度放内部 optional path |
| 文档漂移 | 新计划和旧计划互相打架 | Phase 0 先做 reconciliation |
| handle 语义反复 | `Handle<T>` strong/weak 来回摇摆 | 选择当前实现方向：strong `Handle<T>` + weak `WeakHandle<T>` |
| worker 生命周期 bug | app shutdown 卡住或后台线程访问已释放状态 | worker pool 明确 drop/join/cancel policy |
| install task 泛型复杂 | trait object + associated type 难落地 | 使用统一 `begin_install -> AssetInstallResult`，同步 factory 显式返回 `Ready` |
| 后端耦合 | asset core 开始拥有 GPU/audio/video device | residency cache 归 backend，asset 只发事件和 metadata |
| 热重载破坏旧资源 | reload 失败导致可用资源消失 | 默认保留 last good installed asset |
| 锁竞争 | asset update 每帧锁太多 | request queue 单 owner update，worker 只通过 channel 交付 completion |
| 测试依赖文件系统 | 单测慢且 flaky | memory provider + fake IO service |
| 兼容性破坏 | examples / user code 编译失败 | 用户已接受不兼容改动；以更干净的 runtime factory API 为准，并跑 examples check |

## 13. Sakura 可直接借鉴的点

### 13.1 状态机命名和阶段

SkyEngine 已经有类似状态，可以保留并补全 request-level phase。建议直接采用 Sakura 的大阶段：

- unloaded。
- loading。
- loaded/decoded。
- waiting dependencies。
- installing。
- installed。
- uninstalling/unloading。
- error/failed。

但 Rust 代码里应区分：

- `AssetState`: record 的外部可见状态。
- `AssetRequestPhase`: request 的内部详细状态。

### 13.2 Factory 生命周期拆分

Sakura 的 factory 把加载、反序列化、安装、卸载拆开，这一点应借鉴。SkyEngine 可以演化为：

- `load`: bytes/source -> loaded CPU intermediate。
- `dependencies`: loaded intermediate -> dependency handles。
- `begin_install`: loaded + installed dependencies -> installed asset 或 install task。
- `poll_install`: 跨帧推进。
- `uninstall`: 释放 installed asset 的 runtime-side hooks。

Current Sky status: `load` / `begin_install` / `poll_install` / optional `uninstall` are now represented in `AssetRuntimeFactory`; dependency extraction remains attached to `LoadedAsset` and manifest fallback instead of a separate public factory callback.

### 13.3 Request queue 作为诊断中心

Sakura 的 request object 使每个资源请求可以单独 update、wait、计数。SkyEngine 可以借鉴为 diagnostics：

- request id。
- asset id。
- state enter time。
- current phase。
- wait reason。
- dependency blockers。
- last error。

### 13.4 I/O 和 resource system 分离

Sakura 的 IO service 不是必须照抄，但“resource system 提交 I/O request，不直接文件读取”的边界应采用。这样未来能自然接入：

- package file。
- memory provider。
- editor virtual file。
- network/download cache。
- platform-specific fast path。

## 14. Sakura 不成熟之处的警示

这次参考也暴露了 Sakura snapshot 自身的问题：

- async serde 代码路径被硬关，说明设计有但落地未完全稳定。
- local registry cancel 是空实现，说明取消语义不能只靠接口存在。
- `FlushResource` 未实现，说明 unload/flush 边界是难点。
- C++ raw pointer handle 需要非常谨慎的 lifetime 管理，Rust 不应复制。
- IO service 文档强，但具体平台能力和 engine runtime 绑定较深，不能直接移植。

对 SkyEngine 的启发是：每个新增接口都必须有一个真实资源类型和测试来证明，而不是先铺完整宏伟架构。

## 15. 建议第一批 PR 切分

### PR 1: Asset docs reconciliation

- 更新 asset 相关计划文档。
- 固化 `Handle<T>` strong 语义。
- 增加当前/目标状态图。
- 不改 runtime。

### PR 2: Blocking load and error diagnostics

- 修正 `load_blocking`。
- 增加 failure phase。
- 补 dependency failure tests。

### PR 3: Bounded I/O service

- 新增 `asset::io` 内部模块。
- 替换 `std::thread::spawn` per asset。
- 增加 cancellation/drop tests。

### PR 4: Request state machine

- 新增 `asset::request`。
- 从 `server.rs` 移出状态迁移。
- 保持 public API 不变。

### PR 5: Cross-frame install

- 新增 install task 协议。
- 同步 factory 显式返回 `AssetInstallResult::Ready`。
- 迁移一个重资源作为示例。

### PR 6: Diagnostics and hot reload

- 增加 asset stats。
- 可选 watcher。
- reload explanation events。

### PR 7: Registry/provider and cooker registry

- provider abstraction。
- memory provider tests。
- cooker registry。

## 16. 最终判断

SkyEngine 不需要“照抄 SakuraEngine 的资源系统”，因为 SkyEngine 已经有更符合 Rust、ECS app lifecycle 和 typed handle 的基础。但 SkyEngine 应该认真抄 Sakura 的三件事：

1. 资源请求必须成为显式状态机，而不是散在 `server.rs` 的流程判断。
2. 加载、依赖等待、安装、卸载必须是可预算、可诊断、可跨帧推进的生命周期。
3. I/O/registry/backend residency 必须分层，asset core 只做身份、状态、依赖、事件和 CPU 侧生命周期。

按照本计划推进，SkyEngine 的资源系统可以保持当前 API 亲和力，同时补上引擎级资源调度能力。这比直接移植 Sakura 更稳，也更符合 SkyEngine 现有架构。
