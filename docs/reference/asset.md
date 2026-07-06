# SkyEngine Asset

`sky_engine::asset` 是可选资源加载和 cooked asset 系统。`app` feature 会自动启用 `asset`。

```toml
sky_engine = { version = "...", features = ["asset"] }
# Native file-watch hot reload:
sky_engine = { version = "...", features = ["app", "asset-watch"] }
```

常用入口：

```rust
use sky_engine::asset::{
    Asset, AssetConfig, AssetCookedSchema, AssetDiagnosticsSnapshot, AssetError, AssetEvent,
    AssetEventKind, AssetId, AssetManifestProvenance, AssetMetadata, AssetRegistryManifest,
    AssetPath, AssetProviderStats, AssetReloadStatus, AssetState, AssetStatus, AssetWatchPaths,
    Assets, Handle, LoadedAsset, TextureAsset, WeakHandle,
};
```

## 概念

- `AssetId`：稳定 UUID。
- `Handle<T>`：强 typed handle，持有 asset lease。最后一个强 handle drop 后，资源会在后续 `Assets::update()` 中自动释放。
- `WeakHandle<T>`：弱 typed asset identity，只保存 id，不保活资源，适合序列化、编辑器引用和资产描述符。
- `AssetPath<T>`：typed source path，不保活资源，适合序列化和编辑器引用；通过 `Assets` 解析成 `WeakHandle<T>` 或加载成 `Handle<T>`。
- `Asset`：可加载资源 trait。
- `AssetMeta`：源文件旁的 metadata。
- `AssetMetadata`：运行时从 manifest 查询到的只读 asset metadata 快照。
- `AssetWatchPaths`：某个 manifest asset 对应的 source/cooked/package watch path 快照。
- `AssetRegistryManifest`：cooked manifest。
- `Assets`：运行时加载、安装、查询和生命周期管理。
- `AssetRuntimeFactory`：按 asset 类型创建 runtime asset。

## AssetConfig

```rust,no_run
let config = AssetConfig::new("assets", AssetConfig::default_target())
    .with_background_loading(true)
    .with_install_budget_per_update(16)
    .with_auto_reload(true)
    .with_auto_reload_debounce(std::time::Duration::from_millis(100))
    .with_file_watcher(true)
    .with_package_root("packages/base")
    .with_package_file("packages/base.skybundle")
    .with_io_shutdown_timeout(std::time::Duration::from_secs(2));
```

字段：

- `asset_root`
- `target`
- `background_loading`
- `install_budget_per_update`
- `install_time_budget`
- `auto_reload`
- `auto_reload_interval`
- `auto_reload_debounce`
- `file_watcher`
- `package_roots`
- `package_files`
- `io_worker_threads`
- `io_queue_capacity`
- `io_default_priority`
- `io_shutdown_timeout`

路径：

```rust,no_run
config.cooked_root()
config.manifest_path()
config.package_roots()
config.package_files()
config.source_key(path)
```

Package roots are directory mounts for cooked artifacts. Package files are read-only cooked bundles with an indexed entry per manifest `cooked_path`. Loose cooked files under `config.cooked_root()` win; if they are missing, the provider checks each package root, then package files, for the same manifest `cooked_path`. Relative package roots and files are resolved under `asset_root`.

## Assets

构造：

```rust,no_run
let server = Assets::new(config)?;
let server = Assets::with_empty_manifest(config);
# Ok::<(), sky_engine::asset::AssetError>(())
```

加载：

```rust,no_run
let handle: Handle<TextureAsset> = server.load("sprites/player.png")?;
let path = AssetPath::<TextureAsset>::new("sprites/player.png");
let handle = server.load_path(&path)?;
let handle = server.load_id::<TextureAsset>(asset_id)?;
let urgent = server.load_id_with_priority::<TextureAsset>(asset_id, 100)?;
let handle = server.load_handle(weak_handle)?;
let texture = server.load_blocking::<TextureAsset>(asset_id)?;
# Ok::<(), sky_engine::asset::AssetError>(())
```

`*_with_priority` variants are available for id/path/weak-handle loads and for raw texture/font convenience loads. Higher priority requests enter the bounded I/O queue ahead of lower-priority queued work; the normal methods use `AssetConfig::io_default_priority`.

Dropped handles are observed during `Assets::update()`. If a background load is still queued and the asset is no longer in `Loading`, its worker-queue cancel token is tripped so the job is skipped before start. If the worker already started reading/decoding, it is allowed to finish and the completion is discarded by generation/state validation.

每帧推进：

```rust,no_run
server.update()?;
# Ok::<(), sky_engine::asset::AssetError>(())
```

查询：

```rust,no_run
server.state(&handle) -> AssetState
server.status(&handle) -> AssetStatus
server.is_installed(&handle) -> bool
server.get(&handle) -> Result<Arc<T>, AssetError>
server.try_get(&handle) -> Option<Arc<T>>
server.error(&handle) -> Option<AssetError>
server.stats() -> AssetStats
server.queued_request_snapshots() -> Vec<AssetRequestSnapshot>
server.active_request_snapshots() -> Vec<AssetRequestSnapshot>
server.canceled_request_snapshots() -> Vec<AssetRequestSnapshot>
server.failed_request_snapshots() -> Vec<AssetRequestSnapshot>
server.failed_asset_snapshots() -> Vec<AssetFailureSnapshot>
server.diagnostics_snapshot() -> AssetDiagnosticsSnapshot
server.reload_status() -> AssetReloadStatus
```

`AssetStats` includes record counts, queued/active/in-flight request counts, load worker thread count, running and queued background load job counts, load queue capacity, oldest queued worker-load job age, aggregate source-load phase counts for queued/reading/decoding work, provider-reported package/cache stats such as package mount count and cached bundle index/entry count, provider source-resolution counts for raw/local-cooked/package-root/bundle locations, provider resolve-error count, cumulative submitted/activated/canceled/failed/completed/deferred request counters, request timing averages including canceled-before-activation queue wait, per-phase request timing averages for loading/decoding/dependency-wait/ready-to-install/installing/unloading, source-load read/decode/total timing averages, oldest queued and active request ages, retained event count, reference counts, and per-state counts. `AssetRequestSnapshot` is used for queued, active, recent canceled, and recent failed request diagnostics, including request id, asset id, generation, priority, status, failure phase, progress label/percent (coarse phase progress, source-worker wait/read/decode progress, or install-task supplied progress), queued age, active age when the request has started, current phase age, dependency blockers, dependency blocker details (`Missing` / `Failed` / `Waiting` plus current dependency state), dependency cycles, and last error text when present. Active snapshots hydrate failure phase and error from the current backing record; failed snapshots preserve the phase/error captured at the original failure boundary and only use the current record to add dependency blocker details. `AssetFailureSnapshot` lists current failed assets and failed reloads with phase/error context. `AssetDiagnosticsSnapshot` combines those diagnostic views with the current reload status and latest reload report.

在 `app` feature 下，`src/app/services.rs` 会在每次 asset update 后发布结构化诊断事件到 `sky_engine::diagnostics::Diagnostics`：

- `asset.stats`：request queue depth、active request count、oldest queued/active request ages、in-flight load count、load worker thread count、running worker load jobs、queued worker load jobs/capacity/oldest queued job age、source-load queued/reading/decoding phase counts、deferred load submissions、provider package/cache/source-resolution/cache-invalidation counters、request counters、completed/canceled queue-wait timing averages、total/phase timing averages（含 loading/decoding/dependency-wait/ready-to-install/installing/unloading）、source-load read/decode/total timing averages、reference counters、per-state count。
- `asset.provider.resolve.failed`: emitted when provider source-resolution errors increase; includes the error delta, total resolve-error count, source-resolution counters, and package/cache counters.
- `asset.provider.cache.invalidated`: emitted when provider cache-invalidation counters increase; includes normal/full invalidation deltas, totals, and package/cache counters.
- `asset.reload.status`：auto reload / file watcher / freeze 状态、pending root count/id 列表、pending age，以及最近一次 reload report 的 changed/impacted/skipped 计数。
- `asset.reload`：最近一次 hot reload 的 changed root count、impacted count、skipped count、changed/impacted asset id 列表、skipped reason counts，以及 `asset_id:reason` 形式的 skipped details。
- `asset.failure`：当前失败资源或失败 reload 的 asset id、类型、状态、generation、phase、error。
- `asset.request.canceled`：最近被取消的 queued request 的 request id、asset id、generation、priority、status、progress 和 queued time。
- `asset.request.failed`：最近失败 request 的 request id、asset id、generation、priority、status、failure phase、timing、dependency blocker count、missing/failed/waiting blocker counts、dependency cycle length 和 last error。
- `asset.queue.slow`：排队超过 app 诊断阈值的最老请求，包含 request id、asset id、generation、priority、progress 和 queued time。
- `asset.request.slow`：active 时间超过 app 诊断阈值的请求，包含 status、failure phase、progress、当前 phase age、dependency blocker 数、missing/failed/waiting blocker counts、dependency cycle length 和 last error。
- `asset.state.slow`：某个 asset record 生命周期状态停留超过 app 诊断阈值时触发，包含最慢状态、停留时间、该状态记录数、request/load 计数和最老 queued/active request age。
- `asset.load.backpressure`：bounded I/O queue 推迟 load submit 时触发，包含 deferred delta、累计 deferred submissions、running worker load count、worker pool size、queued load jobs、queue capacity 和 in-flight load count。
- `asset.load.queue.slow`：bounded I/O queue 中最老 source-load worker job 超过 app 诊断阈值时触发，包含 oldest queued job age、worker/queue/in-flight counts 和 source-load queued/reading/decoding phase counts。

这些事件适合编辑器/overlay 读取；`FrameContext::diagnostics()` / `SetupContext::diagnostics()` 可获取 app 当前的 `Diagnostics` 队列。失败和慢请求也会通过 `log` facade 输出，是否显示到 console 由 `LogPlugin` / `LogOptions` 决定。

生命周期：

```rust,no_run
let weak: WeakHandle<TextureAsset> = handle.downgrade();
drop(handle);
server.update()?; // final strong handle drop is observed here
# Ok::<(), sky_engine::asset::AssetError>(())
```

Manifest：

```rust,no_run
server.manifest() -> AssetRegistryManifest
server.resolve_path("sprites/player.png") -> Option<AssetId>
server.resolve_asset_path(&path) -> Result<WeakHandle<T>, AssetError>
server.source_path(asset_id) -> Option<PathBuf>
server.asset_path::<TextureAsset>(asset_id) -> Result<AssetPath<TextureAsset>, AssetError>
server.metadata(asset_id) -> Option<AssetMetadata>
server.watch_paths(asset_id) -> Option<AssetWatchPaths>
server.reload_manifest()?;
server.reload_changed()?; // changed dependent closure only
server.reload_changed_with_report()?; // changed roots + impacted closure + skipped reasons
server.force_reload(asset_id)?; // reload even when hashes are unchanged
server.set_auto_reload_frozen(true); // pause auto reload during editor transactions
# Ok::<(), sky_engine::asset::AssetError>(())
```

`auto_reload_interval` controls how often the runtime scans loaded records for changes. `auto_reload_debounce` delays the actual reload after the first detected change so multiple file writes can be batched into one reload closure. `with_file_watcher(true)` enables native filesystem events when the `asset-watch` feature is compiled; watcher events trigger the same pending/debounce reload path instead of bypassing normal lifecycle rules. Source, loose cooked, and package-root cooked file events are resolved to targeted reload roots when possible; unknown, manifest, or package-file events request a normal scan. External package roots and external package-file parent directories are watched alongside `asset_root`, while package roots/files nested under `asset_root` use the existing recursive watch. `reload_status()` exposes pending roots, pending age, watcher enabled state, freeze state, and the last report. Manual reload reports include skipped asset reasons such as unchanged content, missing manifest entries, or untracked records.

## Runtime Asset

不经过文件，直接插入运行时资源：

```rust,no_run
let texture = TextureAsset::white_pixel();
let handle = server.insert_runtime(texture);
```

这适合 generated texture、procedural asset、测试和 fallback。

## TextureAsset

`TextureAsset` 是 asset 模块内置的 CPU-side RGBA texture。

```rust,no_run
TextureAsset::new(width, height, TextureColorSpace::Srgb, pixels)
TextureAsset::white_pixel()
TextureAsset::checkerboard(size, tile_size, color_a, color_b)
TextureAsset::circle(size)
```

读取：

```rust,no_run
texture.width()
texture.height()
texture.color_space()
texture.pixels()
```

GPU texture 创建属于 render/gpu 层，不在 asset 层直接完成。

## Cooking

Cook API：

```rust,no_run
use sky_engine::asset::cook::{
    cook_all, cook_all_with_registry, cook_target, import_path, verify, verify_with_registry,
    CookRegistry, CookerDescriptor,
};

import_path(&config.asset_root, "player.png")?;
let manifest = cook_all(&config)?;
let manifest = cook_target(&config, "sprites/player.png")?;
let report = verify(&config)?;
# Ok::<(), sky_engine::asset::AssetError>(())
```

`cook_all` / `verify` use built-in cookers. For custom cooked asset kinds, build a `CookRegistry` and pass it to the `*_with_registry` variants:

```rust,ignore
let registry = CookRegistry::with_builtins()
    .with_registered(&MY_COOKER);
let manifest = cook_all_with_registry(&config, &registry)?;
let report = verify_with_registry(&config, &registry)?;
```

`CookerDescriptor` records the cooked schema that runtime factories can consume: asset type, importer, cooker name, cooker version, source extensions, optional dependency schema, and the cook/dependency hooks. `verify_with_registry` checks registered cooker metadata instead of a fixed central kind match. When multiple registered cookers support the same source extension, set `import_settings.asset_type` in the source `.meta` file to select the intended runtime asset type without adding another central branch. Built-in audio metadata still writes the legacy `stream` flag for readability, but `asset_type` is the canonical selector.

Generated manifests also include `AssetManifestProvenance` records. Each record stores the asset id, source hash, cooked hash, dependency hash, target platform, and `AssetConfig::profile`; this is a cooking/audit trail for verification and tools, not ownership of runtime residency. Use `AssetConfig::with_profile(...)` when an editor/build profile should be visible in the manifest.

Render-owned asset kinds keep their cooker/factory registration in `sky_engine::render::asset`, so the asset core does not depend on renderer types. With `app` enabled, `render_cook_registry()` returns the built-in asset cookers plus render cookers such as `.skymesh`, `.gltf`/`.glb` mesh sources, and `.skymaterial`, and `register_render_asset_factories(&assets)` registers the matching runtime factories. glTF mesh sources cook into the same CPU-side `.skymesh` schema consumed by the `MeshAsset` factory; GPU upload remains backend-owned. A `.gltf/.glb.meta` file can set `import_settings.asset_type = "standard_material"` plus an optional `material_index` to cook one glTF PBR material into the `.skymaterial` schema instead of the default mesh schema. glTF material texture imports currently support external image URI dependencies.

`.skymaterial` can declare `albedo_texture`, `normal_texture`, and `emissive_texture` as asset ids or source-relative texture paths. The render-local material cooker imports those references as texture dependencies and writes cooked asset ids back into the `.skymaterial` artifact. During install, the `StandardMaterialAsset` factory turns those dependency ids into real strong `Handle<TextureAsset>` values, so dependency loading, reload closure, and material texture access share the same handle semantics without moving material or texture residency policy into asset core.

命令行工具：

```bash
cargo run --bin sky-cook --features asset -- ...
```

具体命令参数以 `src/bin/sky-cook.rs` 为准。

## Events

`Assets` 提供 cursor 风格事件读取：

```rust,no_run
let mut cursor = server.event_cursor();

for event in server.events_since(&mut cursor) {
    match event.kind {
        AssetEventKind::ReloadQueued => {}
        AssetEventKind::Loaded => {}
        AssetEventKind::Installed => {}
        AssetEventKind::Reloaded => {}
        AssetEventKind::Unloaded => {}
        AssetEventKind::Failed => {}
    }

    let id = event.id;
    let generation = event.generation;
    let asset_type = &event.asset_type;
    let failure_phase = event.failure_phase;
}
```

事件适合 UI 状态、加载进度、调试面板，也会提供后端缓存可用于失效判断的 generation、asset type、hash、依赖上下文和失败阶段。`AssetEventKind::Failed` 会带 `failure_phase`；失败 reload 保留 last-good installed payload 时，事件可能同时是 `kind = Failed`、`state = Installed`。

## Factory

自定义 asset 类型需要实现 `Asset` 并注册 runtime factory：

```rust,no_run
server.register_factory(MyAssetFactory);
```

Factory 负责把 cooked bytes / metadata 变成 runtime asset。加载线程和安装阶段通过 `AssetLoadContext`、`AssetInstallContext` 传上下文。

Factories can optionally declare the cooked schema they accept:

```rust,no_run
fn cooked_schema(&self) -> Option<AssetCookedSchema> {
    Some(AssetCookedSchema::new("my_asset.cooked", 1))
}
```

For cooked, package, and bundle sources, `Assets` validates that manifest `cooker` / `version` matches this schema before decode. Raw source loads are not blocked by this check. A mismatch returns `AssetError::CookedSchemaMismatch`, which makes stale manifest or cooker-version drift visible during runtime load instead of surfacing as an opaque decode failure.

`AssetInstallTask::progress()` is optional; heavy install tasks can report a current label/step for editor diagnostics while simple synchronous factories can ignore it.

## Examples

最小示例都只需要 `asset` feature，会在临时目录中生成源文件、cooked 文件和 manifest：

```bash
cargo run --example asset_cook_smoke --features asset
cargo run --example asset_load_texture --features asset
cargo run --example asset_hot_reload_texture --features asset
cargo run --example asset_load_with_dependency --features asset
cargo run --example asset_custom_factory --features asset
```

- `asset_cook_smoke`：最小 cook/load smoke check。
- `asset_load_texture`：PNG import/cook -> `AssetPath<TextureAsset>` -> `Handle<TextureAsset>`。
- `asset_hot_reload_texture`：修改源 PNG、重新 cook、`reload_manifest`、`reload_changed_with_report`。
- `asset_load_with_dependency`：自定义 factory 在 load 阶段返回 `LoadedAsset::with_dependencies(...)`。
- `asset_custom_factory`：注册一个 game-defined runtime asset factory。

## Feature 关系

- `asset` 启用 `image`、`serde`、`serde_json`、`uuid`、`roxmltree`、`base64`、`flate2`。
- `app` 会启用 `asset`。
- `audio` 依赖 `asset`。

## 测试和检查

```bash
cargo test --features asset
cargo check --bin sky-cook --features asset
```
