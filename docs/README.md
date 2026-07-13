# SkyEngine Docs

这页只做导航。文档按用途分成 API Reference、Tutorials 和 Architecture，避免 API 文档和教程继续混在一起。

## 文档分层

- [API Reference](reference/index.md)：正式 API 入口、feature gate、公开类型、方法语义和模块边界。
- [Tutorials](tutorials/index.md)：一步步跑通的教程和学习路径。
- [Architecture](architecture/index.md)：内部架构、模块关系、运行链路和不变量。

## 核心模块

- [ECS](reference/ecs.md)：`World`、`View` / `ParView` / `Res` 系统参数、typed stages、查询、命令与 dynamic/expert API。
- [Reflect](reference/reflect.md)：底层 `Type` layout 反射、ECS component type 语义别名、`#[derive(Reflect)]` Inspector 反射。
- [Math](reference/math.md)：`Vec2/Vec3/Vec4`、`Quat`、`Mat4`、`Transform`、`Projection`。

## 运行时和平台层

- [App](reference/app.md)：winit runner、`AppState`、`FrameContext`、auto tick、redraw mode。
- [Input](reference/input.md)：raw keyboard/mouse state、action map、rebinding、输入时序。
- [GPU](reference/gpu.md)：`GpuContext`、frame lifecycle、upload arena、dynamic uniform buffer。
- [Legacy Retained UI](reference/ui.md)：显式 `ui-legacy` 旧 retained UI、ECS HUD/menu/buttons/text/progress、glyphon 文本 overlay。

## 内容与功能模块

- [Render](reference/render.md)：高层渲染模块导览，指向更完整的 [Render Expert Reference](reference/render-expert.md)。
- [Persistence](reference/scene.md)：`#[persist(component)]`、`Persistence`、World/Prefab 保存加载、文档中间层。
- [Physics](reference/physics.md)：2D physics、Rapier 后端封装、Tiled physics、debug draw。
- [Asset](reference/asset.md)：asset id、manifest、cooked files、`AssetServer`、runtime asset。
- [Audio](reference/audio.md)：audio assets、`AudioServer`、commands、ECS emitter/listener。
- [Video](reference/video.md)：`VideoClip`、`VideoFrameQueue`、`GpuVideoFrameBuffer`、`FfmpegVideoPlayer`、ECS `VideoPlayer2D`。
- [VN / Galgame](reference/vn.md)：Yarn-compatible script loading、branching runtime、dialogue/scene state。

## 架构文档

- [Architecture Index](architecture/index.md)：架构文档入口。
- [Architecture](architecture/architecture.md)：更长的系统架构、渲染管线、RenderGraph、GPU 资源等背景说明。
- [Render Deep Dive](architecture/render_deep_dive.md)：渲染管线深挖。

## 常用验证命令

```bash
cargo test
cargo test --features scene
cargo test --features physics
cargo test --features reflect-serde
cargo test --features ui-legacy ui
cargo test --features vn vn
cargo check --features "app scene physics"
cargo run --example vn_runtime_minimal --features vn
cargo check --example ui_legacy_hud_menu --features ui-legacy
cargo check --examples --features app
```

## 当前原则

- ECS 热路径优先：typed query、chunk iteration、archetype transition 不被工具层拖慢。
- Reflect 是基础设施：ECS 使用 layout 反射，Inspector 使用 derive 字段反射，Persistence 不依赖 reflect 保存。
- UI 后端必须显式选择：旧 retained UI 只通过 `ui-legacy` 启用，egui 保持 tool/debug overlay 定位。
- 模块边界清晰：scene/save、physics、render、asset、audio、video 都通过自己的 public API 暴露能力，不塞进一个中心 schema。
- Render facade 分层：普通场景代码用 `sky_engine::render`，renderer 内部执行、RenderGraph、draw dispatch、GPU table 和低层 mesh/target/readback 用 `sky_engine::render::expert`。
- API 以人和 AI 都好用为目标：显式入口、稳定名字、少手写注册、文档按模块拆开。
