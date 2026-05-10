# SkyEngine App

`sky_engine::app` 是窗口、事件循环、输入同步、GPU frame lifecycle 和渲染调用的高层运行时。它在 `app` feature 后启用。

```toml
sky_engine = { version = "...", features = ["app"] }
```

常用入口：

```rust
use sky_engine::app::{App, AppConfig, AppState, FrameContext, RedrawMode};
```

## 最小窗口

```rust,no_run
use sky_engine::app::{App, AppConfig, FrameContext};
use sky_engine::ecs::World;
use sky_engine::render::RenderPipelineAsset;

fn main() {
    App::new(AppConfig::new("Hello", 960, 640), World::new())
        .with_render_pipeline(RenderPipelineAsset::forward_2d())
        .run(|ctx: &mut FrameContext| {
            ctx.render();
        });
}
```

闭包可以作为 app state；复杂项目推荐实现 `AppState`。

## AppState

```rust
pub trait AppState: 'static {
    fn setup(&mut self, _world: &mut World, _gpu: &mut GpuContext) {}
    fn update(&mut self, ctx: &mut FrameContext);
    fn on_resize(&mut self, _width: u32, _height: u32) {}
    fn shutdown(&mut self, _world: &mut World) {}
}
```

生命周期：

- `setup`：窗口和 GPU 已准备好，`Input` resource 已存在。适合加载 GPU 资源、创建 texture、spawn 初始实体。
- `update`：每帧调用。默认情况下 ECS schedule 已经 tick 过。
- `on_resize`：窗口 resize 后调用。
- `shutdown`：退出前调用。

示例：

```rust,no_run
use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::World;
use sky_engine::gpu::GpuContext;
use sky_engine::render::RenderPipelineAsset;

struct Game;

impl AppState for Game {
    fn setup(&mut self, world: &mut World, _gpu: &mut GpuContext) {
        // spawn entities, load textures, create resources
    }

    fn update(&mut self, ctx: &mut FrameContext) {
        // read ctx.input, mutate ctx.world, then render
        ctx.render();
    }

    fn shutdown(&mut self, world: &mut World) {
        world.shutdown();
    }
}

fn main() {
    App::new(AppConfig::new("Game", 1280, 720), World::new())
        .with_render_pipeline(RenderPipelineAsset::forward_2d())
        .run(Game);
}
```

## AppConfig

```rust
AppConfig::new(title, width, height)
    .with_vsync(true)
    .with_resizable(true)
    .with_exit_on_escape(true)
    .with_max_delta(0.1)
    .with_auto_tick(true)
    .with_redraw_mode(RedrawMode::Continuous)
```

字段：

- `title`
- `width`
- `height`
- `vsync`
- `resizable`
- `exit_on_escape`
- `max_delta`
- `auto_tick`
- `redraw_mode`
- `diagnostic_console`

`max_delta` 会 clamp frame delta，避免断点、系统卡顿导致物理或动画爆炸。

## RedrawMode

```rust
RedrawMode::Continuous
RedrawMode::Reactive
```

- `Continuous`：游戏默认模式，窗口可见时持续 redraw。
- `Reactive`：工具/editor 更适合，只有 dirty 或显式请求时 redraw。

`FrameContext` 可请求 redraw：

```rust
ctx.request_redraw();
```

具体可用方法以 `FrameContext` 实现为准；使用时优先看 IDE completion。

## FrameContext

`FrameContext<'a>` 每帧传给 `AppState::update`。

主要字段：

```rust
pub world: &'a mut World
pub input: &'a Input
pub dt: f32
```

常见操作：

```rust
ctx.render();
ctx.render_stats() -> RenderStats
ctx.request_exit();
ctx.request_redraw();
ctx.set_title("...");
ctx.surface_size() -> [u32; 2]
ctx.time() -> &Time
ctx.dt() -> f32
ctx.gpu()
ctx.feature_mut::<T>()
ctx.with_feature_mut::<T, _>(...)
ctx.with_render_runtime_mut(...)
```

`dt` 语义：

- `auto_tick = true`：`dt` 是 `world.time.frame_delta`，会受 `time_scale` 影响。
- `auto_tick = false`：`dt` 是 runner 采样的 clamped delta；你需要自己调用 `world.tick_with_delta` 或 `tick_with_frame_delta`。

## 自动 tick

默认 `AppConfig::auto_tick = true`。每帧顺序大致是：

```text
同步 winit 输入到 Input resource
world.tick_with_frame_delta(clamped_delta, raw_delta)
AppState::update(ctx)
render
```

这意味着：

- ECS systems 在 `update` 之前运行。
- 如果你在 `update` 中写组件，默认会在下一帧 schedule 生效。
- 对 physics 玩家控制这种手感敏感逻辑，建议把输入写成 `pre_physics` system，或关闭 auto tick 手动排序。

手动 tick：

```rust,no_run
let config = AppConfig::new("Manual", 960, 720).with_auto_tick(false);
```

```rust,no_run
fn update(&mut self, ctx: &mut FrameContext<'_>) {
    apply_input(ctx.world, ctx.input);
    ctx.world.tick_with_delta(ctx.dt);
    ctx.render();
}
```

## 输入资源

Runner 会维护：

- `FrameContext::input`
- `Input` ECS resource

如果使用 action map，可以在 `setup` 插入 `InputActions` resource，然后在系统或 update 中读取。

## 渲染管线

典型 app 会安装一个 render pipeline：

```rust,no_run
App::new(config, world)
    .with_render_pipeline(RenderPipelineAsset::forward_2d())
    .run(Game);
```

每帧调用：

```rust,no_run
ctx.render();
```

如果你手写 GPU pass，可以通过 `ctx.gpu()` 获取 `GpuContext`。

## Diagnostics

`AppConfig::diagnostic_console` 控制哪些 diagnostics 会镜像到 stderr。

```rust,no_run
use sky_engine::app::{AppConfig, DiagnosticConsole};

let config = AppConfig::new("Tool", 1280, 720)
    .with_diagnostic_console(DiagnosticConsole::WarningsAndErrors);
```

结构化事件仍保存在 `Diagnostics` resource 中，不受 console 过滤影响。

## Feature 关系

- `app` 会启用 `asset`、`wgpu`、`winit`、`pollster`、`bytemuck`、`gltf`。
- `input`、`gpu`、`render` 当前都在 `app` feature 下导出。
- `egui` feature 会在 `sky_engine::app::egui` 重新导出 egui crate。

## 测试和检查

```bash
cargo test --features app
cargo check --examples --features app
```
