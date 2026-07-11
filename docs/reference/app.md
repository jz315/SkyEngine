# SkyEngine App

`sky_engine::app` 是窗口、事件循环、输入同步、GPU frame lifecycle 和渲染调用的高层运行时。它在 `app` feature 后启用。

```toml
sky_engine = { version = "...", features = ["app"] }
```

常用入口：

```rust
use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, LogPlugin, RedrawMode,
    RenderPlugin, RunnerPlugin, WindowPlugin,
};
```

## 最小窗口

```rust,no_run
use sky_engine::app::{App, FrameContext, InputPlugin, RenderPlugin, WindowPlugin};
use sky_engine::ecs::World;

fn main() {
    let mut world = World::new();
    world.install(WindowPlugin::new("Hello", 960, 640)).unwrap();
    world.install(InputPlugin).unwrap();
    world.install(RenderPlugin::forward_2d()).unwrap();

    App::new(world).run(|ctx: &mut FrameContext| {
        ctx.render();
    });
}
```

闭包可以作为 app state；复杂项目推荐实现 `AppState`。

## AppState

```rust
pub trait AppState: 'static {
    fn setup(&mut self, _ctx: &mut SetupContext) {}
    fn update(&mut self, ctx: &mut FrameContext);
    fn on_resize(&mut self, _width: u32, _height: u32) {}
    fn shutdown(&mut self, _world: &mut World) {}
}
```

生命周期：

- `setup`：窗口和 GPU 已准备好；如果安装了 `InputPlugin`，`Input` resource 已存在。适合加载 GPU 资源、创建 texture、spawn 初始实体。
- `update`：每帧调用。默认情况下 ECS schedule 已经 tick 过。
- `on_resize`：窗口 resize 后调用。
- `shutdown`：退出前调用。

示例：

```rust,no_run
use sky_engine::app::{App, AppState, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin};
use sky_engine::ecs::World;

struct Game;

impl AppState for Game {
    fn setup(&mut self, ctx: &mut SetupContext) {
        // spawn entities, load textures, create resources
        let _world = &mut ctx.world;
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
    let mut world = World::new();
    world.install(WindowPlugin::new("Game", 1280, 720)).unwrap();
    world.install(InputPlugin).unwrap();
    world.install(RenderPlugin::forward_2d()).unwrap();

    App::new(world).run(Game);
}
```

## Capability Plugins

```rust
world.install(WindowPlugin::new("Game", 1280, 720).with_vsync(true))?;
world.install(RunnerPlugin::game().with_frame_rate_limit(120.0))?;
world.install(LogPlugin::new())?;
world.install(InputPlugin)?;
world.install(AssetPlugin::new("assets"))?;
world.install(RenderPlugin::forward_2d())?;
```

原则：

- 配置写在插件构造器或 builder 方法上。
- `World::install(...)` 是能力组合入口。
- `App::new(world)` 只消费已经安装的能力。
- 没安装某个可选能力，就没有对应行为。
- 插件自己的硬依赖由插件安装时检查。

`RunnerPlugin::game().with_max_delta(...)` 会 clamp frame delta，避免断点、系统卡顿导致物理或动画爆炸。

## 日志

SkyEngine 使用 Rust 标准 `log` facade。业务代码直接写：

```rust
log::info!("loaded level");
log::warn!("missing texture");
log::error!("renderer failed");
```

App runner 默认会安装一个轻量 logger；显式配置时使用 `LogPlugin`：

```rust,no_run
use sky_engine::app::{LogConsole, LogPlugin};

world
    .install(
        LogPlugin::new()
            .with_level(log::LevelFilter::Debug)
            .with_console(LogConsole::WarningsAndErrors)
            .with_capacity(2048),
    )
    .unwrap();
```

捕获到的日志保存在 app-owned `LogStore`，不是 ECS resource。`setup` 和 `update` 中可以读取：

```rust,no_run
fn update(ctx: &mut FrameContext<'_>) {
    for entry in ctx.logs().entries() {
        let file = entry.short_file().unwrap_or(&entry.target);
        let line = entry.line.unwrap_or(0);
        println!("{file}:{line} {}", entry.message);
    }
}
```

连续重复日志会默认折叠到同一条 `LogEntry`，通过 `repeat_count` 查看次数。
日志写入线程只投递到有界队列；app runner 每帧 drain 到 `LogStore`，因此热路径不会直接抢 `LogStore` 的锁。

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
- `auto_tick = false`：`dt` 是 runner 采样的 clamped delta；你需要调用 `ctx.tick()`，或直接处理 `world.tick_with_delta` / `tick_with_frame_delta` 返回的 `Result`。

## 自动 tick

默认 `RunnerPlugin::game()` 会启用自动 tick。每帧顺序大致是：

```text
如果安装了 InputPlugin，同步 winit 输入到 Input resource
world.tick_with_frame_delta(clamped_delta, raw_delta)
AppState::update(ctx)
render
```

这意味着：

- ECS systems 在 `update` 之前运行。
- 若整帧 preflight 发现缺失 resource，runner 会记录错误并有序退出，不会 panic，也不会调用本帧 `update/render`。
- 如果你在 `update` 中写组件，默认会在下一帧 schedule 生效。
- 对 physics 玩家控制这种手感敏感逻辑，把控制 system 注册在 `FixedUpdate` 的 physics system 之前，或关闭 auto tick 手动排序。

手动 tick：

```rust,no_run
world.install(RunnerPlugin::game().with_auto_tick(false)).unwrap();
```

```rust,no_run
fn update(&mut self, ctx: &mut FrameContext<'_>) {
    apply_input(ctx.world, ctx.input);
    let _report = ctx.tick().expect("manual ECS schedule tick failed");
    ctx.render();
}
```

## 输入资源

Runner 会维护：

- `FrameContext::input`
- `Input` ECS resource，如果安装了 `InputPlugin`

如果使用 action map，可以在 `setup` 插入 `InputActions` resource，然后在系统或 update 中读取。

## 渲染管线

典型 app 会安装一个 render pipeline：

```rust,no_run
world.install(RenderPlugin::forward_2d()).unwrap();
App::new(world).run(Game);
```

每帧调用：

```rust,no_run
ctx.render();
```

如果你手写 GPU pass，可以通过 `ctx.gpu()` 获取 `GpuContext`。

## Feature 关系

- `app` 会启用 `asset`、`wgpu`、`winit`、`pollster`、`bytemuck`、`gltf`。
- `input`、`gpu`、`render` 当前都在 `app` feature 下导出。
- `egui` feature 会在 `sky_engine::app::egui` 重新导出 egui crate。

## 测试和检查

```bash
cargo test --features app
cargo check --examples --features app
```
