# SkyEngine Asset

`sky_engine::asset` 是可选资源加载和 cooked asset 系统。`app` feature 会自动启用 `asset`。

```toml
sky_engine = { version = "...", features = ["asset"] }
```

常用入口：

```rust
use sky_engine::asset::{
    Asset, AssetConfig, AssetError, AssetEvent, AssetEventKind, AssetId, AssetRegistryManifest,
    AssetState, AssetStatus, Assets, Handle, LoadedAsset, TextureAsset, WeakHandle,
};
```

## 概念

- `AssetId`：稳定 UUID。
- `Handle<T>`：强 typed handle，持有 asset lease。最后一个强 handle drop 后，资源会在后续 `Assets::update()` 中自动释放。
- `WeakHandle<T>`：弱 typed asset identity，只保存 id，不保活资源，适合序列化、编辑器引用和资产描述符。
- `Asset`：可加载资源 trait。
- `AssetMeta`：源文件旁的 metadata。
- `AssetRegistryManifest`：cooked manifest。
- `Assets`：运行时加载、安装、查询和生命周期管理。
- `AssetRuntimeFactory`：按 asset 类型创建 runtime asset。

## AssetConfig

```rust,no_run
let config = AssetConfig::new("assets", AssetConfig::default_target())
    .with_background_loading(true)
    .with_install_budget_per_update(16);
```

字段：

- `asset_root`
- `target`
- `background_loading`
- `install_budget_per_update`

路径：

```rust,no_run
config.cooked_root()
config.manifest_path()
config.source_key(path)
```

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
let handle = server.load_id::<TextureAsset>(asset_id)?;
let handle = server.load_handle(weak_handle)?;
let texture = server.load_blocking::<TextureAsset>(asset_id)?;
# Ok::<(), sky_engine::asset::AssetError>(())
```

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
```

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
server.reload_manifest()?;
server.reload_changed()?;
# Ok::<(), sky_engine::asset::AssetError>(())
```

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
use sky_engine::asset::cook::{cook_all, cook_target, import_path, verify};

import_path(&config.asset_root, "player.png")?;
let manifest = cook_all(&config)?;
let manifest = cook_target(&config, "sprites/player.png")?;
let report = verify(&config)?;
# Ok::<(), sky_engine::asset::AssetError>(())
```

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
        AssetEventKind::Queued => {}
        AssetEventKind::Loaded => {}
        AssetEventKind::Installed => {}
        AssetEventKind::Failed => {}
        AssetEventKind::Unloaded => {}
    }
}
```

事件适合 UI 状态、加载进度、调试面板。

## Factory

自定义 asset 类型需要实现 `Asset` 并注册 runtime factory：

```rust,no_run
server.register_factory(MyAssetFactory);
```

Factory 负责把 cooked bytes / metadata 变成 runtime asset。加载线程和安装阶段通过 `AssetLoadContext`、`AssetInstallContext` 传上下文。

## Feature 关系

- `asset` 启用 `image`、`serde`、`serde_json`、`uuid`、`roxmltree`、`base64`、`flate2`。
- `app` 会启用 `asset`。
- `audio` 依赖 `asset`。

## 测试和检查

```bash
cargo test --features asset
cargo check --bin sky-cook --features asset
```
