# SkyEngine 文档索引

这页只做导航。每个模块的 API、设计边界、常见用法和测试命令都拆到独立文档里，避免一个 `api.md` 变成新的 god document。

## 核心模块

- [ECS](ecs.md)：`World`、`EntityId`、`Bundle`、`PreparedQuery`、`Commands`、系统调度、dynamic/expert API。
- [Reflect](reflect.md)：底层 `Type` layout 反射、ECS component type 语义别名、`#[derive(Reflect)]` Inspector 反射。
- [Math](math.md)：`Vec2/Vec3/Vec4`、`Quat`、`Mat4`、`Transform`、`Projection`。

## 运行时和平台层

- [App](app.md)：winit runner、`AppState`、`FrameContext`、auto tick、redraw mode。
- [Input](input.md)：raw keyboard/mouse state、action map、rebinding、输入时序。
- [GPU](gpu.md)：`GpuContext`、frame lifecycle、upload arena、dynamic uniform buffer。
- [UI](ui.md)：原生 retained UI、ECS HUD/menu/buttons/text/progress、glyphon 文本 overlay。

## 内容与功能模块

- [Render](render.md)：高层渲染模块导览，指向更完整的 [Render API Reference](render_api.md)。
- [Scene / Save](scene.md)：AI-first scene JSON、`SceneRuntime`、Prefab、serde-first 保存/加载。
- [Physics](physics.md)：2D physics、Rapier 后端封装、Tiled physics、debug draw。
- [Asset](asset.md)：asset id、manifest、cooked files、`AssetServer`、runtime asset。
- [Audio](audio.md)：audio assets、`AudioServer`、commands、ECS emitter/listener。
- [Video](video.md)：`VideoClip`、`VideoFrameQueue`、`GpuVideoFrameBuffer`、`FfmpegVideoPlayer`、ECS `VideoPlayer2D`。
- [VN / Galgame](vn.md)：Yarn-compatible script loading、branching runtime、dialogue/scene state。

## 架构文档

- [Architecture](architecture.md)：更长的系统架构、渲染管线、RenderGraph、GPU 资源等背景说明。

## 常用验证命令

```bash
cargo test
cargo test --features scene
cargo test --features physics
cargo test --features reflect-serde
cargo test --features ui ui
cargo test --features vn vn
cargo check --features "app scene physics"
cargo run --example vn_runtime_minimal --features vn
cargo check --example hud_menu --features ui
cargo check --examples --features app
```

## 当前原则

- ECS 热路径优先：typed query、chunk iteration、archetype transition 不被工具层拖慢。
- Reflect 是基础设施：ECS 使用 layout 反射，Inspector 使用 derive 字段反射，Scene 不依赖 reflect 保存。
- UI 是游戏运行时模块：原生 retained UI 负责 HUD/Menu，egui 保持 tool/debug overlay 定位。
- 模块边界清晰：scene/save、physics、render、asset、audio、video 都通过自己的 public API 暴露能力，不塞进一个中心 schema。
- Render facade 分层：普通场景代码用 `sky_engine::render`，renderer 内部执行、RenderGraph、draw dispatch、GPU table 和低层 mesh/target/readback 用 `sky_engine::render::expert`。
- API 以人和 AI 都好用为目标：显式入口、稳定名字、少手写注册、文档按模块拆开。
