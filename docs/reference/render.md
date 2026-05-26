# SkyEngine Render

`sky_engine::render` 是高层渲染 facade，在 `app` feature 下启用。它建立在 `gpu` 模块之上，提供组件、camera/view、sprite、mesh、lighting、postfx、pipeline/phase、tilemap、Live2D 等能力。

更完整的低层 API 表格和 GPU 细节见 [Render Expert Reference](render-expert.md)。架构背景见 [Architecture](../architecture/architecture.md)。

## 常用入口

顶层 `sky_engine::render` 面向普通应用和 gameplay 代码：

```rust
use sky_engine::render::{
    Camera, Color, RenderPipelineAsset, RenderPipelineBuilder, SpriteFeature, Texture,
};
```

高级扩展代码也可以从顶层使用 feature / phase / pass / material 注册 API：

```rust
use sky_engine::render::{
    ComputePass, Material, PostFxPass, RenderFeature, RenderPass, RenderPhase,
};
```

专家层用于 frame execution、render graph、draw dispatch、GPU table、低层 mesh/target/readback 等 renderer 内部能力：

```rust
use sky_engine::render::expert::{
    FramePipeline, RenderGraph, DrawFunction, OpaquePhase, TransparentPhase,
};
```

规则很简单：写游戏内容优先用 `sky_engine::render`；写 renderer family、工具或底层 GPU 编排时用 `sky_engine::render::expert`。顶层暂时保留了一些低层兼容 re-export，但新代码不要把它们当作默认入口。

## API 分层

| 层级 | 入口 | 用途 |
|------|------|------|
| 稳定 gameplay API | `sky_engine::render` | camera、render components、sprite、tilemap、light、pipeline asset、backend、texture readiness、render stats |
| 高级扩展 API | `sky_engine::render` | `RenderFeature`、phase/pass/post-fx、material/shader 注册、自定义 renderer family |
| 专家 / 低层 API | `sky_engine::render::expert` | `FramePipeline`、`RenderGraph`、`PreparedFrame` / `PreparedView`、draw functions、GPU tables、low-level mesh/target/readback |

## 推荐高层路径

应用通常这样安装渲染：

```rust,no_run
use sky_engine::app::{App, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, WindowPlugin};
use sky_engine::ecs::World;

fn main() {
    let mut world = World::new();
    world.install(WindowPlugin::new("Render", 960, 640)).unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::forward_2d()).unwrap();

    App::new(world).run(|ctx: &mut FrameContext| {
        ctx.render();
    });
}
```

游戏逻辑只需要 spawn render components，render runtime 会 extract、prepare、draw。

## RenderRuntime

`RenderRuntime` 是高层渲染运行时。它负责：

- 管理 render features。
- 执行 frame prepare。
- 按 phase 排序和 draw。
- 和 `GpuContext` frame lifecycle 协作。

一般用户通过 `RenderPlugin` 间接使用它；自定义 runner 可以直接持有 `RenderRuntime`。

## Pipeline Asset / Builder

`RenderPipelineAsset` 描述一套可安装的渲染管线。

常见：

```rust,no_run
RenderPipelineAsset::forward_2d()
```

自定义：

```rust,no_run
let asset = RenderPipelineBuilder::new()
    .add_feature(SpriteFeature)
    .build();
```

具体 builder API 以 `src/render/pipeline` 为准。

## Components

render-facing ECS components 位于 `src/render/component`。

常见类别：

- Camera marker / viewport / projection
- Sprite components
- Mesh components
- Light components
- Render settings
- Tilemap components

典型 sprite 实体：

```rust,no_run
world.spawn((
    Transform::from_xy(0.0, 0.0),
    Sprite::from_color(Color::WHITE),
));
```

具体 component 名称以 `sky_engine::render` re-export 和 IDE completion 为准，因为 render 模块仍在快速演进。

## Camera / View

Camera 负责从 ECS transform 和 projection 生成 `SceneView` / GPU view uniform。

常规 2D：

```rust,no_run
world.spawn((
    Transform::from_xy(0.0, 0.0),
    Camera::orthographic(720.0),
));
```

约定：

- `Transform` 控制 camera pose。
- `Projection` 控制投影。
- viewport 可用于 split screen / render target 子区域。

## Sprite / Texture

Sprite renderer 是默认 2D path。

Texture 资源在 render/gpu 层，CPU-side cooked texture 在 asset 层。典型路径：

```text
AssetServer loads TextureAsset
render module creates Texture GPU resource
Sprite references Texture / atlas / material
```

具体 texture helper 见 [Render Expert Reference](render-expert.md)。

## Phases

渲染按 phase 组织：

- `OpaquePhase`
- `TransparentPhase`

Phase 负责 draw item 排序和 draw function 调度。自定义 feature 应把自己的 draw items 放进合适 phase，而不是绕过统一 runtime。

## RenderFeature

新增 renderer family 推荐走：

```text
component/resource input
-> extractor / prepare cache / upload
-> typed frame/view payload
-> feature registration
-> phase draw
```

不要把所有 renderer 状态塞进一个万能 scene struct。每个 renderer family 保持自己的 prepare/upload/cache，只有 camera/view/order/layer 这类真正跨 feature 的概念共享。

## RenderGraph

RenderGraph 位于 `render::expert`，用于声明式组织 render pass / compute pass / copy pass / transient resources。

适用：

- 多 pass 后处理
- offscreen render target
- resource aliasing
- copy/upload pass
- 复杂 GPU pipeline 原型

更详细规则见：

- [Render Expert Reference](render-expert.md)
- `src/render/graph/AGENTS.md`

## Tilemap / Tiled

Tilemap 在 `app` feature 下可用。Tiled import 和 physics collider 提取分别属于 render/tilemap 与 physics integration。

示例：

```bash
cargo run --example tilemap_demo --features app
cargo run --example tiled_import_demo --features app
cargo run --example tiled_browser_demo --features app
```

Tiled physics 需要：

```bash
cargo run --example tiled_physics_demo --features "app physics"
```

## Live2D

Live2D 通过 `live2d` feature 启用，走同一 render composition/transparent phase 思路。

```bash
cargo run --example live2d_demo --features "live2d egui"
```

## Debug / Stats

`RenderStats` 可从 render 调用返回，用于 UI 或日志。

```rust,no_run
ctx.render();
let stats = ctx.render_stats();
```

render timing instrumentation 由 `render-timings` feature 控制。

## 测试和检查

```bash
cargo test --features app
cargo test --features app graph
cargo check --examples --features app
```

如果改 RenderGraph internals，先读 `src/render/graph/AGENTS.md`。
