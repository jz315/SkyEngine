# SkyEngine Render 深度讲解

本文是一份面向学习和维护的 render 运行机制说明。它不只列 API，而是把 SkyEngine 当前渲染系统从应用入口、ECS authoring、视图收集、抽取、GPU 上传、phase 排序、FramePipeline 执行、RenderGraph 资源调度、最终 present 串成一条完整链路。

适合按这个顺序阅读：

1. 如果你没有图形学基础，先读第 0 节。它会把后面反复出现的 GPU、纹理、pass、shader、depth、material 等词先讲成人话。
2. 再读第 1 到第 4 节，建立 SkyEngine render 的整体心智模型。
3. 再读第 5 到第 10 节，理解每帧到底做了什么。
4. 然后读第 11 到第 15 节，理解 2D、3D、材质、GI、阴影和 post-fx 如何插入同一套框架。
5. 最后读第 16 到第 20 节，学习如何扩展、调试和避免破坏关键不变量。

相关源码入口：

- `src/render/mod.rs`
- `src/render/AGENTS.md`
- `src/render/runtime/runtime.rs`
- `src/render/runtime/frame_coordinator.rs`
- `src/render/runtime/pipeline_runtime.rs`
- `src/render/execution/step_nodes/`
- `src/render/execution/`
- `src/render/graph/`
- `src/render/pipeline/`
- `src/render/phase/`
- `src/render/extract/`
- `src/gpu/context.rs`
- `src/app/runner.rs`
- `examples/render/three_d_demo.rs`

---

## 0. 没有图形学基础，先读这里

这一节不讲 SkyEngine 的具体代码，先建立最低限度的图形学直觉。后面看到 `RenderGraph`、`PhaseItem`、`GpuScene`、`Material`、`PostFxPass` 时，心里至少知道它们大概在解决哪类问题。

### 0.1 一帧画面本质上是什么

一帧画面就是一张二维图片。屏幕上每个像素最后都有一个颜色：

```text
像素 (x, y) -> RGBA
```

游戏引擎每帧做的事，就是把世界里的相机、物体、灯光、材质、贴图等数据，转换成这张图片。

可以把整个渲染过程想成：

```text
游戏世界里的对象
  -> 相机看到哪些对象
  -> GPU 把三角形/精灵投影到屏幕
  -> 每个像素算出颜色
  -> 把最终图片交给窗口系统显示
```

这里最容易混淆的一点是：CPU 不会逐像素把整张图手工画出来。CPU 主要负责准备数据和发命令，真正海量并行的顶点处理、像素计算、纹理采样发生在 GPU 上。

### 0.2 CPU 和 GPU 各自负责什么

粗略分工：

| 角色 | 负责什么 | 在 SkyEngine 里常见对象 |
|------|----------|-------------------------|
| CPU | ECS 查询、排序、剔除、准备 draw item、上传 buffer、组织 pass | `World`、`RenderRuntime`、`Extractor`、`FramePipeline`、`RenderGraph` |
| GPU | 跑 shader、画三角形、写 texture、做 compute、采样贴图 | `wgpu::Device`、`wgpu::Queue`、`Texture`、`RenderPass`、`ComputePass` |

CPU 对 GPU 说的不是“帮我画一个游戏场景”，而是一串更底层的命令：

```text
使用这套 pipeline
绑定这些 buffer / texture / uniform
把这批顶点按三角形画出来
结果写入这张 color texture
同时用这张 depth texture 做深度测试
```

SkyEngine 的 render 系统大部分复杂度，都来自“如何把 ECS 世界稳定、高效地翻译成这一串 GPU 命令”。

### 0.3 纹理、Render Target、Surface 是什么

纹理可以理解为 GPU 里的图片。它不一定来自 png/jpg，也可以是渲染中途产生的临时图片。

常见纹理类型：

- 贴图：角色、砖块、材质的图片，例如 sprite texture、albedo texture。
- 渲染目标：本帧渲染时写入的图片，例如 `scene_color`。
- 深度图：每个像素存距离，不直接显示颜色，例如 `scene_depth`。
- 法线图：每个像素存表面朝向，例如 `scene_normal`。
- 历史图：上一帧保存下来的结果，例如 TAA history。

`RenderTarget` 就是“这次 pass 要写入的目标纹理”。`Surface` 是窗口系统提供的最终显示目标，可以理解为“屏幕背后的那张图”。实际流程通常不是所有 pass 都直接写 surface，而是：

```text
先写若干中间 texture
  -> 做后处理
  -> 最后 blit / present 到 surface
```

SkyEngine 中常见对应关系：

| 图形学概念 | SkyEngine 名字 |
|------------|----------------|
| 中间颜色图 | `scene_color`、`CURRENT_COLOR` |
| 深度图 | `scene_depth` |
| 法线图 | `scene_normal` |
| 最终窗口目标 | `surface` |
| 临时渲染目标 | `RenderTarget`、RenderGraph virtual texture |

### 0.4 顶点、三角形、Mesh 和 Sprite

GPU 最擅长画三角形。一个 3D 模型通常是一堆顶点和索引：

```text
vertices: 位置、法线、UV、切线、颜色...
indices:  每三个索引组成一个三角形
```

`Mesh` 就是这些几何数据和子网格信息。即使你看到的是一个立方体、角色或地面，GPU 底层仍然是在画很多三角形。

`Sprite` 可以理解为一个贴了纹理的矩形。矩形本身也可以拆成两个三角形：

```text
quad = triangle 1 + triangle 2
```

所以 Sprite 和 Mesh 的差别更多在引擎抽象上：

- Sprite：使用起来像 2D 图片，常见字段是 size、color、texture、sorting layer。
- Mesh：使用起来像 3D 几何，常见字段是 vertex layout、sub mesh、material、bounding sphere。

但到了 GPU draw 阶段，它们都要变成“绑定资源 + 画顶点/索引”。

### 0.5 相机、矩阵和坐标变换

World 里的物体有自己的 `Transform`：

```text
position + rotation + scale
```

渲染时要把一个模型顶点从“模型自己的局部坐标”一路变到“屏幕坐标”。常见链路是：

```text
local space
  -> model matrix
  -> world space
  -> view matrix
  -> camera/view space
  -> projection matrix
  -> clip space
  -> viewport/screen space
```

用人话说：

- model matrix：这个物体在世界里放哪、转多少、缩放多少。
- view matrix：从相机角度看世界，相当于把整个世界搬到相机面前。
- projection matrix：把 3D 空间投到 2D 屏幕，决定透视或正交。
- view-proj matrix：view 和 projection 合起来，shader 常用。

SkyEngine 中：

- `Transform` 来自 ECS。
- `ResolvedSceneTransforms` 处理父子层级后的 world transform。
- `SceneView` 保存相机、projection、view-proj、viewport、frustum。
- `ViewUniform` 是给 GPU shader 用的相机数据。
- `ModelMatrixTable` 是给 GPU shader 用的物体矩阵表。

### 0.6 Shader 是什么

Shader 是跑在 GPU 上的小程序。最常见的是：

- Vertex shader：处理顶点，把模型顶点变换到屏幕相关坐标。
- Fragment shader：处理像素片段，算这个像素最后是什么颜色。
- Compute shader：不直接画图，做通用 GPU 计算，例如 GI probe 更新、某些后处理。

一个最简 draw 大概像这样：

```text
vertex shader:
  输入 mesh 顶点 + model/view/projection
  输出屏幕位置 + UV + normal

fragment shader:
  输入 UV / normal / light / texture
  输出颜色
```

在 SkyEngine 中，`Material` 会提供 shader source 和 pipeline 状态。`DrawMesh<M>` 或 `DrawSprite` 会在执行时绑定这些 shader 需要的资源。

### 0.7 Buffer、Uniform、Bind Group 是什么

GPU 不能直接读 Rust 里的任意对象。CPU 必须把数据整理成 GPU 能读的资源：

- Buffer：一段 GPU 内存，常放顶点、索引、矩阵数组、灯光数组。
- Uniform buffer：通常放少量每帧/每视图/每次 draw 的参数。
- Texture：图片或中间渲染结果。
- Sampler：告诉 GPU 采样纹理时怎么过滤、怎么处理越界 UV。
- Bind group：把一组 buffer/texture/sampler 绑定到 shader 可见的位置。

可以把 bind group 想成 shader 的“参数包”：

```text
shader 需要:
  camera uniform
  model matrix table
  light table
  material texture
  sampler

draw 前:
  创建/复用 bind group
  pass.set_bind_group(...)
```

SkyEngine 中：

- `GpuScene` 管共享 view uniform、model matrix table、light table。
- `Material::create_bind_group` 管材质自己的贴图/参数绑定。
- `DynamicUniformBuffer` 管带 dynamic offset 的 uniform。
- `FrameUploadArena` 管每帧临时 vertex/index 上传。

### 0.8 Draw Call、Render Pass 和 Pipeline

`draw call` 是 CPU 给 GPU 的一次绘制命令，例如“从这个 vertex buffer 画 36 个 index”。draw call 太多会有 CPU/GPU 调度成本，所以引擎会尽量排序和批处理。

`render pass` 是一段往某些 render target 写东西的 GPU 工作。例如：

```text
OpaquePass:
  color target = scene_color
  depth target = scene_depth
  draw opaque meshes
```

`pipeline` 是 GPU 绘制状态的打包，包括：

- 用哪个 shader。
- 顶点数据长什么样。
- color/depth 格式是什么。
- 是否开启 alpha blend。
- 是否做 depth test / depth write。
- 正面/背面剔除规则。

所以渲染时经常要避免频繁切 pipeline、切材质、切 mesh。SkyEngine 的 `PhaseItem`、`sort_key`、`batch_key` 就是在帮 CPU 把相似 draw 放在一起。

### 0.9 深度测试、Opaque 和 Transparent

3D 场景里，谁挡住谁不能只靠 draw 顺序。GPU 常用 depth buffer：

```text
每个像素记录当前已经画过的最近深度
新片段如果更近，就写入颜色和深度
新片段如果更远，就丢弃
```

这对不透明物体很好用，所以 opaque 通常可以前后顺序不那么严格，反而优先按材质/mesh 排序来减少状态切换。

透明物体麻烦一些。半透明颜色需要和背后的颜色混合：

```text
final = src * alpha + dst * (1 - alpha)
```

如果顺序错了，混合结果就会错。因此 transparent 通常要更重视从远到近或按 layer 的稳定排序。

SkyEngine 中：

- `OpaquePhase` 画不透明物体，依赖 depth。
- `TransparentPhase` 画透明物体，通常在 opaque 后面画。
- Sprite 默认进 transparent phase，因为 2D 图片经常有 alpha。
- `sort_key` 决定大顺序，`batch_key` 帮相似 draw 靠近。

### 0.10 Prepass、GBuffer 和 Forward Shading

有些信息不是最终颜色，但后续 pass 很需要。例如：

- 每个像素的深度。
- 每个像素的法线。
- 每个像素的基础颜色、粗糙度、金属度。
- 每个像素的运动速度。

提前画一遍，把这些信息写进纹理，就叫 prepass。保存这些“几何/材质属性”的一组纹理常叫 GBuffer。

SkyEngine 的 `modern_3d()` 中：

```text
SceneNormalPrepass
  -> scene_normal
  -> scene_velocity
  -> scene_depth

SceneMaterialPrepass
  -> scene_albedo
  -> scene_material
  -> scene_emissive
```

后面的 SSGI、contact shadows、TAA、debug view 都可以复用这些中间结果。

Forward shading 则是在真正画 opaque 时直接结合灯光、阴影、GI、材质，算出最终 `scene_color`。SkyEngine 当前 `StandardMaterial` 的主路径就是这种 forward shading 加上若干 prepass/post-fx 辅助。

### 0.11 后处理 Post-FX 是什么

后处理不是逐个物体画，而是对已经画好的整张图做处理。典型流程：

```text
输入: 当前 scene_color
画一个全屏三角形/矩形
fragment shader 对每个像素采样输入图
输出: 新的 scene_color
```

常见 post-fx：

- Bloom：让亮部泛光。
- ToneMap：把 HDR 颜色压到屏幕能显示的 LDR 范围。
- Sharpen：锐化。
- TAA：用历史帧和 velocity 抗锯齿。
- DebugView：把 depth/normal/albedo 等中间图显示出来。

SkyEngine 里的 `PostFxPass` 通常读 `CURRENT_COLOR` 和若干 scene textures，然后写一个新的 `CURRENT_COLOR`。

### 0.12 RenderGraph 为什么存在

如果只有一个 pass，可以直接手写：

```text
创建 texture A
画 opaque 到 A
画 postfx 到 B
拷贝 B 到 surface
```

但真实 renderer 会有很多 pass，且有依赖：

```text
normal prepass 写 scene_normal
SSGI 读 scene_normal + scene_depth + scene_color
TAA 读 velocity + history + current color
ToneMap 读 HDR color 写 LDR color
ViewportBlit 读 LDR color 写 surface
```

手写顺序和资源生命周期很容易错。`RenderGraph` 让每个 pass 先声明：

```text
我读哪些 virtual resources
我写哪些 virtual resources
```

然后 graph 统一决定：

- 哪些 pass 必须先跑。
- 哪些 pass 没有最终用途，可以剔除。
- 哪些临时 texture 可以复用同一块物理 GPU 内存。
- 执行时每个 virtual texture 对应哪个真实 GPU resource。

所以 `RenderGraph` 不是“画东西的 renderer family”，它更像 GPU 工作流调度器。

### 0.13 把基础概念映射回 SkyEngine

读后文时，可以先用这张表定位：

| 你看到的词 | 先把它理解成 |
|------------|--------------|
| `World` | 游戏世界数据仓库 |
| `Camera` / `SceneView` | 从哪个相机、哪个 viewport 看世界 |
| `Extractor` | 把 ECS 组件翻译成可绘制列表 |
| `PhaseItem` | 一个待绘制对象的轻量记录 |
| `OpaquePhase` | 不透明绘制队列 |
| `TransparentPhase` | 透明绘制队列 |
| `DrawFunction` | 真正发 draw call 的代码 |
| `Material` | shader + 材质参数 + GPU pipeline 规则 |
| `GpuScene` | 共享 GPU 数据表，例如相机、矩阵、灯光 |
| `PreparedFrame` | 本帧准备好的全局数据包 |
| `PreparedView` | 某个相机/view 准备好的数据包 |
| `FramePipeline` | 按 pipeline steps 组织本帧执行 |
| `RenderGraph` | 管 pass 依赖、临时资源和执行顺序 |
| `GpuContext` | wgpu device/queue/surface/encoder 的封装 |

如果只记一个大图，就是：

```text
ECS 数据
  -> CPU 准备可见对象和 GPU 参数
  -> GPU 按 pass 写中间纹理
  -> 后处理生成最终颜色
  -> present 到窗口
```

---

## 1. 一句话总览

SkyEngine 的 render 可以理解为：

```text
World 里的 ECS 渲染组件
  -> RenderRuntime 每帧读取 World
  -> 收集 SceneView
  -> Extractor 把实体变成 PhaseItem
  -> Runtime Feature 准备自有缓存和 payload
  -> GpuScene 上传共享 GPU 表
  -> PreparedFrame / PreparedView 携带本帧 typed payload
  -> PipelineStep 转成 FramePipeline 节点
  -> FramePipeline 用 RenderGraph 声明和执行 pass
  -> GpuContext 录制 wgpu command encoder
  -> end_frame submit + present
```

最重要的几个对象：

| 对象 | 角色 | 所在位置 |
|------|------|----------|
| `World` | ECS 数据源，保存相机、Transform、Sprite、Mesh、Light、RenderSettings 等 | `src/ecs/` |
| `RenderPipelineAsset` | 声明一套渲染管线有什么 feature、phase、pass、post-fx、material、draw function | `src/render/pipeline/pipeline_asset.rs` |
| `RenderRuntime` | 高层 wgpu render 运行时，负责把 `World` 准备成 `PreparedFrame` 并执行 | `src/render/runtime/runtime.rs` |
| `RenderFeature` | 一个 renderer family 的注册和每帧 hook，例如 Sprite、Live2D、Tilemap | `src/render/pipeline/features.rs` |
| `Extractor` | 从 ECS 查询渲染组件，把可见对象写进 `OpaquePhase` 或 `TransparentPhase` | `src/render/extract/` |
| `PhaseItem` | phase 中的一个可排序、可批处理 draw item | `src/render/phase/item.rs` |
| `DrawFunction` | 真正把一批 `PhaseItem` 画进 render pass 的执行器 | `src/render/phase/mesh_draw.rs / sprite_draw.rs` |
| `GpuScene` | 共享 GPU 表和 view uniform，例如 model matrix table、light table | `src/render/gpu/scene.rs` |
| `PreparedFrame` / `PreparedView` | render 组合边界，frame/view 级 typed payload 容器 | `src/render/execution/payload.rs` |
| `FramePipeline` | 每帧执行引擎，组织 setup/view/finalize node | `src/render/execution/frame_pipeline.rs` |
| `RenderGraph` | 虚拟资源和 pass 依赖图，负责排序、剔除、分配、别名、执行 | `src/render/graph/` |
| `GpuContext` | wgpu device/queue/surface/frame encoder 的包装 | `src/gpu/context.rs` |

如果上面这串名字还是有点抽象，可以先把它们压成四层：

```text
1. World:
   游戏和编辑层看得懂的数据，比如 Transform、Camera、SpriteRenderer、Light。

2. RenderRuntime:
   CPU 侧翻译官，把 World 里的实体变成“哪些 view 要画哪些 item”。

3. FramePipeline + RenderGraph:
   GPU 工作编排器，决定本帧有哪些 pass、读写哪些 texture、按什么顺序跑。

4. GpuContext:
   真正和 wgpu 打交道，录制 command encoder，最后 submit/present。
```

再换成更贴近画面的说法：

```text
你 spawn 的 sprite / mesh
  -> 经过相机筛选
  -> 进入 opaque 或 transparent 绘制队列
  -> draw function 绑定 shader、mesh、texture、uniform
  -> render pass 写入 scene_color / scene_depth 等纹理
  -> post-fx 处理 scene_color
  -> blit 到窗口 surface
```

---

## 2. 为什么不是“一个 Renderer 直接画所有东西”

当前 render 不是一个大 `Renderer::draw_scene(scene)`。它被拆成几层，是为了同时支持：

- 2D sprite
- 3D mesh
- tilemap
- Live2D
- shadow pass
- scene prepass
- GI compute
- post-fx
- 多 camera / viewport
- headless tests
- 未来 Kajiya / Renderling 这种后端

核心设计原则是：异构 renderer 不强行塞进一个万能 scene schema，而是在组合层统一。

这个组合层就是：

```text
RenderPipelineAsset
RenderRuntime
PreparedFrame
PreparedView
FramePipeline
RenderGraph
```

也就是说：

- Sprite 自己知道怎么从 `SpriteRenderer` 提取。
- Mesh 自己知道怎么从 `WgpuMeshRenderer` 提取。
- Live2D 可以保留自己的 Cubism runtime、mask、drawable 数据。
- Shadow 可以增加自己的 shadow view。
- PostFX 只关心前面产生的 scene color/depth/normal 等 texture slot。
- 最后所有东西通过 `PreparedFrame` / `PreparedView` 和 `FramePipeline` 接到同一条执行链。

初学者很容易问：为什么不直接把所有对象塞进一个 `Scene`，然后 `draw_scene(scene)`？

原因是这些 renderer family 需要的数据差异很大：

| renderer family | 它关心的数据 |
|-----------------|--------------|
| Sprite | texture、size、tint、sorting layer、2D-ish transform |
| Mesh | vertex/index buffer、sub mesh、material、bounding volume、normal/tangent/UV |
| Shadow | 从灯光角度看的 view、shadow map atlas、caster filtering |
| Live2D | Cubism model、drawable、mask、runtime state |
| PostFX | 已经渲染好的 scene texture，不关心单个实体 |
| GI / SSGI / DDGI | depth/normal/color、probe data、compute buffers |

如果强行做一个万能 scene schema，它会越来越大，很多字段只有某个 renderer 用，最后变成谁都不真正喜欢的中间结构。SkyEngine 选择让各 family 保留自己的数据模型，只在“本帧有哪些 view、有哪些 phase item、有哪些 pass/resource”这个层面统一。

这是理解整个系统的第一关键点。

---

## 3. 模块分层图

```text
app
  App / FrameContext / SceneRenderer trait
  负责窗口、事件循环、begin_frame、end_frame、调用 ctx.render()

gpu
  GpuContext / GpuFrame / upload arena / dynamic uniform buffer
  负责 wgpu device、queue、surface、encoder、submit、present

render public facade
  src/render/mod.rs
  对外 re-export RenderRuntime、RenderPipelineAsset、SpriteFeature、Camera 等

render backend
  WgpuSceneRenderer / KajiyaSceneRenderer / RenderlingSceneRenderer
  App 层通过 SceneRenderer trait 调用，默认 wgpu 后端内部使用 RenderRuntime

render pipeline
  RenderPipelineBuilder / RenderPipelineAsset / RenderFeature / RenderPhase / Pass traits
  声明“这一套渲染流程由哪些步骤组成”

render runtime
  RenderRuntime / frame_coordinator / pipeline_runtime / nodes / presentation
  每帧把 World 转成 PreparedFrame，再把 PipelineStep 转成 FramePipeline node 执行

render extract
  ExtractSprites / ExtractMeshes / ExtractSchedule
  从 ECS query 抽取可见对象到 phase

render phase
  OpaquePhase / TransparentPhase / PhaseItem / DrawFunctionRegistry
  排序、批处理、调度 draw function

render execution
  PreparedFrame / PreparedView / PhaseState / FramePipeline
  运行时组合层和 pass 间资源状态

render graph
  RenderGraph / virtual resources / dependency compile / allocation / execute
  pass 声明、资源依赖、物理资源分配和调度

render gpu/resources
  Texture / RenderTarget / GpuScene / GpuTable / Material / Mesh
  GPU 资源和渲染资源缓存
```

---

## 4. 从 `ctx.render()` 开始的调用链

普通应用通常这样运行：

```rust
world.install(WindowPlugin::new("Demo", 1280, 720)).unwrap();
world.install(InputPlugin).unwrap();
world.install(AssetPlugin::default()).unwrap();
world.install(RenderPlugin::modern_3d()).unwrap();

App::new(world).run(MyApp);
```

每帧在 `AppState::update` 里调用：

```rust
ctx.render();
```

调用链如下：

```text
FrameContext::render()
  -> self.renderer.render_world(self.world)
  -> WgpuSceneRenderer::render_world(world)
  -> RenderRuntime::render_world(&mut gpu, world)
```

其中 `FrameContext` 在 `src/app/runner.rs`，`WgpuSceneRenderer` 在 `src/render/backend/wgpu.rs`，`RenderRuntime::render_world` 在 `src/render/runtime/frame_coordinator.rs`。

App runner 在调用用户 `update` 之前已经做了这些事：

```text
winit RedrawRequested
  -> sync Input resource
  -> optional AssetServer::update()
  -> optional world.tick_with_delta(dt)
  -> renderer.begin_frame()
       -> GpuContext::begin_frame()
       -> acquire surface texture
       -> create command encoder
       -> reset frame upload arena
  -> AppState::update(FrameContext)
       -> 用户可改 World
       -> 用户调用 ctx.render()
  -> optional egui overlay
  -> renderer.end_frame()
       -> GpuContext::end_frame()
       -> queue.submit(encoder.finish())
       -> surface.present()
```

所以 `RenderRuntime::render_world` 运行时，一定处于一个 active GPU frame 内。它可以通过 `gpu.frame()` 开 render pass / compute pass，也可以通过 `gpu.encoder()` 走底层路径。

---

## 5. PipelineAsset 是“声明”，RenderRuntime 是“运行时”

`RenderPipelineAsset` 是静态或半静态的配置对象。它描述：

- 使用哪个后端：`Wgpu`、`Kajiya`、`Renderling`
- 有哪些 `RenderFeature`
- 有哪些 `PipelineStep`
- 有哪些 `Extractor`
- 有哪些 `DrawFunction`
- 有哪些 `GpuTable`
- 有哪些 `Material`

它不负责每帧执行。

可以把它当成菜单或蓝图：

```text
RenderPipelineAsset:
  这顿饭要做哪些菜
  每道菜大概按什么顺序
  需要哪些厨具和食材类型

RenderRuntime:
  每帧真的进厨房
  看 World 里今天有哪些实体
  准备 GPU 资源
  发起实际绘制
```

所以 `RenderPipelineAsset::modern_3d()` 只是说“我要 normal prepass、material prepass、shadow、DDGI、opaque、SSGI、transparent、TAA、bloom、tonemap、debug view 这些步骤”。它不会自己查询 ECS，也不会自己创建本帧的 `scene_color`。这些都发生在 `RenderRuntime::render_world` 和后续 `FramePipeline` 执行里。

`RenderRuntime` 是 wgpu 后端的运行时对象。它拥有：

```text
RuntimePlan
  runtime_features
  steps
  extractors
  gpu_tables
  materials

RenderResourceHub
  draw_functions
  material_registry
  mesh_registry

FrameRuntimeState
  last_stats
  surface_size
  frame_settings
  view_collector
  temporal tracker
  gpu_scene
  ddgi runtime
  fallback_texture
  history texture store
  asset_event_cursor
  previous_model_by_entity

ShadowRuntime
  shadow layouts
  compare sampler
  per-view shadow bindings
```

构造关系：

```text
RenderPipelineAsset::modern_3d()
  -> RenderPipelineBuilder builds asset
  -> WgpuSceneRenderer::try_new(..., Some(asset))
  -> RenderRuntime::from_asset(asset)
```

`from_asset` 会把 asset 中的 boxed draw functions 注册进 `DrawFunctionRegistry`，并把 feature、steps、extractors、materials 等转移进 composer。

---

## 6. 内建 Pipeline 预设

当前几个常用预设在 `src/render/pipeline/pipeline_asset.rs`。

### 6.1 `forward_2d()`

```text
SpriteFeature::lit_hdr()
TransparentPhase
Bloom
ToneMap
```

它适合基础 2D：sprite 提取进 transparent phase，然后可选 bloom，再 tonemap 到 surface。

### 6.2 `forward_3d()`

```text
SpriteFeature::lit_hdr()
DirectionalShadowPhase
DdgiUpdateCompute
OpaquePhase
TransparentPhase
Bloom
ToneMap
```

它是较简单的 3D forward path：先做方向光 shadow，更新 DDGI，再画 opaque/transparent，最后 post-fx。

### 6.3 `modern_3d()`

```text
SpriteFeature::lit_hdr()
SceneNormalPrepass
SceneMaterialPrepass
DirectionalShadowPhase
DdgiUpdateCompute
OpaquePhase
ContactShadows
SsgiPass
TransparentPhase
TemporalAntiAliasing
Sharpen
Bloom
ToneMap
DebugView
```

这是 `examples/render/three_d_demo.rs` 使用的管线。它会产生更多 scene textures：

- `scene_color`
- `scene_depth`
- `scene_normal`
- `scene_velocity`
- `scene_albedo`
- `scene_material`
- `scene_emissive`
- `scene_light`
- `scene_indirect_diffuse`

这些 texture slot 由 `PhaseState` / `SceneGBufferSlots` 维护，后续 pass 通过 execution context 读取。

这些 texture 不一定都会显示到屏幕上。它们更像渲染过程中的“草稿纸”和“中间表格”：

| texture | 里面大概存什么 | 谁会用 |
|---------|----------------|--------|
| `scene_color` | 当前已经算好的颜色 | 后续透明、TAA、bloom、tonemap、blit |
| `scene_depth` | 每个像素离相机多远 | depth test、contact shadows、SSGI、debug |
| `scene_normal` | 每个像素表面朝向 | SSGI、contact shadows、debug |
| `scene_velocity` | 像素从上一帧到这一帧移动了多少 | TAA、motion-aware effects |
| `scene_albedo` | 不含光照的基础颜色 | debug、未来 deferred/lighting 相关 pass |
| `scene_material` | roughness/metallic 等材质属性 | debug、GI/lighting 辅助 |
| `scene_emissive` | 自发光颜色 | bloom、debug、GI 辅助 |

最终用户看到的通常是经过 tone map 和 blit 后的颜色，不是这些中间 texture 本身。DebugView 的意义就是把这些平时藏起来的中间图临时显示出来，方便判断是哪一步错了。

### 6.4 非 wgpu 后端

`RenderPipelineAsset::kajiya_3d()` 和 `renderling_3d()` 只设置 `backend_kind`。App 层会通过 `create_scene_renderer` 选择后端。

默认 wgpu 后端：

```text
WgpuSceneRenderer
  -> GpuContext
  -> RenderRuntime
```

Kajiya / Renderling 后端不直接使用同一套 wgpu `RenderRuntime` 主路径，而是通过 backend-neutral scene snapshot 等方式同步场景。

---

## 7. ECS Authoring 层：World 中放什么

用户和 gameplay 一般不直接构造 `PhaseItem`。他们在 `World` 里 spawn 渲染组件：

### 7.1 Camera

常见相机实体：

```rust
world.spawn((
    Transform::default(),
    CameraMarker::new(),
    Projection::perspective(55.0_f32.to_radians(), 0.1, 80.0),
    MainCamera,
));
```

这里 `CameraMarker` 是 `src/render/component/camera.rs` 中的 ECS camera marker，`Projection` 和 `Transform` 来自 math/render facade。

相机相关组件：

- `Camera`: enabled marker
- `Projection`: orthographic / perspective
- `CameraViewport`: 多 viewport、order、layer mask
- `MainCamera`: 主相机 marker
- `Transform`: camera position/rotation/scale

### 7.2 Sprite

Sprite 典型实体：

```rust
world.spawn((
    Transform::from_xyz(0.0, 0.0, 0.0),
    SpriteRenderer::new(64.0, 64.0).color(Color::WHITE),
));
```

`ExtractSprites` 会查询：

```rust
(
    &Transform,
    &SpriteRenderer,
    Option<&SortingLayer>,
    Option<&RenderLayerMask>,
)
```

并写入 `TransparentPhase`。

### 7.3 Mesh

当前 wgpu composer 主路径使用 `WgpuMeshRenderer`：

```rust
world.spawn((
    Transform::from_xyz(0.0, 0.0, 0.0),
    WgpuMeshRenderer::new(mesh_handle, material_handle),
));
```

`WgpuMeshRenderer` 保存的是 renderer 内部的 GPU mesh/material handle，适合当前 wgpu pipeline 和 expert examples。

另一个 `MeshRenderer` 是 backend-neutral 场景组件，它引用 asset handles：

```rust
MeshRenderer::new(mesh_asset_handle, material_asset_handle)
```

这条路径更适合未来多后端统一场景同步。

### 7.4 Lights

灯光组件包括：

- `PointLight`
- `SpotLight`
- `DirectionalLight`

`RenderRuntime::render_world` 会通过 typed query 收集它们，转成 `GpuLight` 写进 `LightTable`。

### 7.5 RenderSettings

`RenderSettings` 是 world resource：

```rust
world.insert_resource(RenderSettings {
    clear_color: Color::rgb(0.0014, 0.0018, 0.0024),
    global_illumination: GlobalIlluminationSettings {
        enabled: true,
        mode: GlobalIlluminationMode::Ssgi,
        ..Default::default()
    },
    ..Default::default()
});
```

每帧 composer 会读取：

```text
world.get_resource::<RenderSettings>().copied().unwrap_or_default()
```

然后传入 frame payload。很多 pass 都从 `PreparedFrame` 的 `RenderSettings` payload 读开关和参数。

---

## 8. `RenderRuntime::render_world` 每帧步骤

这是最重要的函数，位于 `src/render/runtime/frame_coordinator.rs`。下面按真实执行顺序讲。

### 8.0 先看一版人话流程

如果把所有 Rust 类型先藏起来，`render_world` 每帧做的是：

```text
1. 确认 GPU 侧长期资源已经存在
   例如材质 storage、builtin quad、shadow layout、fallback texture。

2. 读取本帧 World 状态
   包括 RenderSettings、Transform 层级、Camera、Sprite、Mesh、Light。

3. 把 camera 变成 SceneView
   一个 camera/view 对应一次“从这个视角看世界”。

4. 把可见实体变成 phase item
   Sprite/Mesh extractor 只负责生成绘制队列，不直接画。

5. 排序和合批
   opaque 更重视减少 GPU 状态切换，transparent 更重视正确混合顺序。

6. 上传共享 GPU 表
   model matrix table、light table、view uniform 等。

7. 构造 PreparedFrame / PreparedView
   把准备好的 CPU/GPU 数据交给 execution 层。

8. 构造并执行 FramePipeline
   每个 step 声明 RenderGraph pass，RenderGraph 排序/分配资源，然后 execute 真正录制 wgpu 命令。
```

这个函数的重点不是“画某一种东西”，而是把很多不同 renderer family 准备出来的数据放进同一条 frame pipeline。

### 8.1 确保运行时资源存在

开头会做：

```text
ensure_registered_materials(gpu)
ensure_builtin_meshes(gpu)
ensure_phase_runtime(gpu)
ensure_shadow_runtime(gpu)
```

含义：

- material 类型如果在 pipeline asset 中注册过，这里确保 `MaterialRegistry` 里有对应 storage 和 pipeline cache。
- builtin quad mesh 确保存在，sprite draw 会用它。
- `GpuScene` 懒初始化，注册默认 `ModelMatrixTable` 和 `LightTable`，以及 pipeline 额外注册的 GPU table。
- fallback white texture 懒初始化，用于缺失 texture。
- shadow bind group layout、pass layout、compare sampler 懒初始化。

这一段保证后面的 extract、draw、shadow、material 都有运行时基础资源。

### 8.2 更新 frame-local runtime 状态

然后会：

```text
pipeline_cache.new_frame()
pipeline_cache.garbage_collect()
surface_size = gpu.surface_size()
history.begin_frame(gpu)
frame_settings = World 中 RenderSettings 或 default
asset_cache.begin_frame()
清空 SpriteMaterial 临时 storage
```

Sprite 材质比较特殊：sprite extractor 会按 texture 动态创建本帧临时 `SpriteMaterial`，所以每帧开始清空。

### 8.3 解析 scene transforms

```text
resolved_transforms = resolve_scene_transforms(world)
```

`SceneTransformResolver` 查询：

```rust
(&Transform, Option<&Parent>)
```

它会解析 parent-child 层级：

```text
local Transform
  -> parent world Transform
  -> world Transform
```

结果保存在 `ResolvedSceneTransforms` 中，后面 camera、sprite、mesh、light 都优先使用解析后的 world transform。

重要点：

- 有 `Parent` 时使用层级变换。
- 检测到循环时回退局部 transform，避免无限递归。
- 所有需要 transform 的 renderer 都从这里共享结果。

### 8.4 收集 SceneView

```text
views = collect_world_views(world, resolved_transforms)
```

`WorldViewCollector` 查询：

```rust
(
    &Transform,
    &Camera,
    Option<&Projection>,
    Option<&CameraViewport>,
    Option<&MainCamera>,
)
```

收集规则：

- disabled camera 跳过。
- 有 `CameraViewport` 的 camera 会直接产生一个 view。
- 没有 viewport 时，优先使用第一个 `MainCamera`，否则第一个 camera。
- 如果没有任何 view，后面会创建 fallback view。
- camera 没有 projection 时会报告 diagnostic，并使用默认 orthographic projection。

`SceneView` 包含：

- viewport rect
- target size
- order / execution_order
- layer mask
- view matrix
- projection matrix
- view-proj matrix
- inverse view
- camera position
- near/far/time/delta
- `ViewUniform`
- frustum
- temporal state
- 是否 2D planar
- 是否 shadow view

对初学者来说，`SceneView` 可以先理解成“CPU 算好的相机包”。ECS 里的 `Camera`、`Projection`、`Transform` 只是 authoring 数据，还不够 GPU 直接使用；`SceneView` 把它们变成渲染真正需要的形式：

```text
camera entity components
  -> camera world transform
  -> view matrix
  -> projection matrix
  -> view-projection matrix
  -> viewport/scissor/target size
  -> frustum
  -> GPU ViewUniform
```

一个 World 可以有多个 camera，因此本帧可以有多个 `SceneView`。每个 view 都会有自己的 phase item 列表、自己的 viewport、自己的 `current_color` 演进过程。

### 8.5 Runtime Feature 的 extract / collect_views

接着：

```text
for feature in runtime_features:
    feature.extract(world, transforms, surface_size)

for feature in runtime_features:
    feature.collect_views(&mut views)
```

这给 feature 一个机会：

- 读取 ECS，准备自己的临时 CPU 数据。
- 增加额外 view。

典型例子：

- Live2D feature 可以收集模型状态。
- Shadow 系统会在后面通过 `append_directional_shadow_views` 增加 shadow views。
- Tilemap 等 feature 可以保留自己 family 的 prepare/cache。

### 8.6 添加 directional shadow views

```text
shadow_setups = append_directional_shadow_views(world, &mut views)
```

方向光阴影会根据主视图和 `DirectionalLight` 配置追加额外 `SceneViewKind::DirectionalShadow` views。每个 cascade 可以有一个 shadow view。

这些 shadow view 后续也会跑 extractor，生成只属于 shadow pass 的 phase items。但它们不会 present 到 surface，也会跳过普通 opaque/transparent built-in phase 的 surface path。

### 8.7 finalize views

```text
views = finalize_scene_views(views, surface_size)
```

它做几件事：

- 如果 view 为空，创建 fallback view。
- 按 `(view.order, presents_to_surface ? 1 : 0)` 排序。
- 给每个 view 写入 `execution_order`。
- 标记第一个 presents-to-surface 的 view 负责 clear surface。
- target size 为 0 时使用 viewport size。

排序和 clear 规则很重要：

- 多 camera / split screen 时，后画的 view 可以 load surface。
- 只有第一个写 surface 的 view clear，避免后续 viewport 把前面的结果清掉。

### 8.8 Temporal view tracker

```text
temporal.update_views(
    &mut views,
    settings.temporal_aa.enabled,
    settings.temporal_aa.jitter_scale,
)
```

这里给 view 写入 temporal state：

- current view-proj
- previous view-proj
- jitter
- previous jitter
- history reset
- frame index

TAA、velocity、history texture 都依赖这份数据。

### 8.9 Runtime Feature 的 prepare

```text
for feature in runtime_features:
    feature.prepare(gpu, &views)
```

这是 feature 面向 GPU 的准备阶段。它已经知道本帧有哪些 view，可以：

- 上传 family-specific buffers。
- 为每个 view 准备 payload。
- 更新内部 cache。

### 8.10 处理 asset events 和 texture queue

composer 读取 `AssetServer` events，通知 `SharedRenderAssetCache`：

```text
asset_server.events_since(cursor)
  -> render_asset_cache.handle_asset_event(...)
```

extract sprite 时如果遇到 texture handle，会请求或解析 GPU texture。帧后面会：

```text
render_asset_cache.prepare_queued_textures(gpu)
```

这样 CPU-ready texture 可以在 render 线程上传到 GPU。

### 8.11 对每个 view 运行 Extractor

这是 ECS 到 phase 的主转换：

```text
for each view:
    opaque_phase = OpaquePhase::new()
    transparent_phase = TransparentPhase::new()

    for extractor in plan.extractors:
        extractor.extract(world, transforms, view, ExtractContext)

    for feature in runtime_features:
        feature.append_phase_items(view_index, opaque_phase, transparent_phase)

    opaque_phase.sort()
    transparent_phase.sort()
```

`ExtractContext` 提供：

- `gpu`
- `asset_server`
- `render_assets`
- `material_registry`
- `mesh_registry`
- `opaque_phase`
- `transparent_phase`
- `quad_mesh_handle`

这里最重要的心智模型是：Extractor 不画东西。Extractor 只是把 ECS 组件翻译成“后面要画什么”的小记录。

```text
ECS entity:
  Transform + WgpuMeshRenderer + RenderLayerMask

Extractor output:
  PhaseItem {
    entity,
    draw_function_id,
    batch_key,
    sort_key,
    payload: MeshDrawData { mesh, material, sub_mesh, model_slot }
  }
```

这样做的好处是：

- extraction 阶段可以做 culling、layer filtering、材质分类。
- phase 阶段可以统一排序和合批。
- draw 阶段只面对已经整理好的连续 item，不需要再跑复杂 ECS 查询。
- 新 renderer family 可以插入自己的 extractor，而不用改主执行器。

内建 extractor：

- `ExtractSprites`
- `ExtractMeshes<M>`

它们来自 material registration。`SpriteFeature` 注册 `SpriteMaterial`、`UnlitMaterial`、`StandardMaterial`，因此 builder 会自动添加对应 draw function 和 extractor。

### 8.12 Sprite extraction 如何工作

`ExtractSprites` 查询：

```rust
(&Transform, &SpriteRenderer, Option<&SortingLayer>, Option<&RenderLayerMask>)
```

每个实体：

1. `sprite.visible == false` 则跳过。
2. view layer mask 不匹配则跳过。
3. 取 resolved world transform。
4. 根据 sprite texture handle 解析 GPU texture。
5. 为该 texture 创建或复用本帧 `SpriteMaterial`。
6. 计算 batch key。
7. 计算 transparent sort key。
8. 写入 `TransparentPhase`。

写入的 payload 是 `SpriteDrawData`，里面压缩了：

- material handle
- size
- color rgba8
- uv rect

Sprite 走 transparent phase，因为 2D sprite 通常需要 alpha blend 和稳定排序。

### 8.13 Mesh extraction 如何工作

`ExtractMeshes<M>` 查询：

```rust
(&Transform, &WgpuMeshRenderer, Option<&SortingLayer>, Option<&RenderLayerMask>)
```

每个实体：

1. `visible == false` 跳过。
2. 如果是 shadow view，检查该 mesh 是否投射当前 cascade。
3. layer mask 不匹配跳过。
4. 取 resolved world transform。
5. 取 mesh registry 中的 mesh。
6. 用 mesh bounding sphere 和 view frustum 做粗剔除。
7. 遍历 sub mesh。
8. 找到 sub mesh material handle。
9. 检查 material 类型是否为当前 extractor 的 `M`。
10. 根据 material 是否透明，选择 opaque 或 transparent phase。
11. 计算 batch key 和 sort key。
12. 写入 `PhaseItem<MeshDrawData>`。

`MeshDrawData` 保存：

- mesh handle
- material handle
- sub mesh index
- model matrix slot

model matrix slot 起初为 0，后面统一分配。

### 8.14 Phase sort 和 batch key

`OpaquePhase` 和 `TransparentPhase` 都只是 `Vec<PhaseItem>` 的薄包装。

可以把 phase 想成“本 view 的待绘制清单”。它还不是 GPU command buffer，只是 CPU 上排好序的任务列表。

排序逻辑：

```text
sort_key
  -> batch_key
  -> entity_sort_key
```

opaque sort key 更偏向 batch locality：

```text
batch bucket
depth
entity
```

transparent sort key 更偏向 painter/order：

```text
sorting layer
batch bucket
depth
```

phase 执行时还会按连续相同 `(draw_function_id, batch_key)` 切 batch：

```text
items[cursor..batch_end]
  -> DrawFunctionRegistry::draw_batch(draw_function_id, ctx, items)
```

这就是 batch key 的意义：排序时让可合批对象靠近，执行时减少 pipeline/material/mesh 切换。

为什么 opaque 和 transparent 的排序目标不同？

```text
Opaque:
  depth buffer 能处理遮挡
  所以更希望相同 pipeline/material/mesh 靠近
  目标是少切 GPU 状态

Transparent:
  alpha blend 对绘制顺序敏感
  所以更希望先满足 layer/depth 的可见顺序
  目标是混合结果正确
```

这也是为什么 sprite 默认放 transparent phase：哪怕它是 2D，贴图边缘和半透明像素也通常需要 alpha blend。

### 8.15 分配 model matrix slots

phase item 都生成后：

```text
entity_to_model_slot = FxHashMap
model_matrices = [identity]

for phase in opaque_phases:
    draw_functions.assign_model_matrices(...)

for phase in transparent_phases:
    draw_functions.assign_model_matrices(...)
```

`DrawFunction` 可重写 `assign_model_matrix`。mesh draw 会把实体 transform 写进 model matrix table，并把 slot 写回 `MeshDrawData.model_slot`。

为什么不在 extractor 里立刻写 GPU table？

- 同一个实体可能出现在多个 view。
- 多个 phase 共享同一实体 model matrix。
- 需要统一去重和稳定 slot。
- previous model matrix 也要按同样 slot 对齐，用于 velocity/TAA。

### 8.16 previous model matrices

composer 保存：

```text
previous_model_by_entity: FxHashMap<EntityId, [f32; 16]>
```

本帧根据 `entity_to_model_slot` 创建 `previous_model_matrices`：

- 如果实体上一帧存在，用上一帧矩阵。
- 否则使用当前矩阵。

这对 velocity buffer、TAA、motion-aware effects 很重要。

帧末会用本帧 resolved transforms 更新 `previous_model_by_entity`。

### 8.17 收集 lights 并上传 GpuScene

`collect_gpu_lights` 查询：

- `(&Transform, &PointLight)`
- `(&Transform, &SpotLight)`
- `&DirectionalLight`

并生成 `Vec<GpuLight>`。

然后：

```text
gpu_scene.table_mut::<ModelMatrixTable>().set_all(gpu, &model_matrices)
gpu_scene.table_mut::<LightTable>().set_all(gpu, &lights)
gpu_scene.upload_all(gpu.queue())
```

`GpuScene` 自带：

- view uniform buffer + bind group
- `ModelMatrixTable`
- `LightTable`
- 额外注册的 `GpuTable`

每个 draw pass 执行前会写当前 view uniform：

```text
gpu_scene.write_view_uniform(gpu.queue(), &scene_view.view_uniform)
```

### 8.18 DDGI 和 shadow runtime 准备

composer 每帧会准备 DDGI runtime：

```text
ddgi.prepare(
    gpu,
    settings.global_illumination,
    views,
    opaque_phases,
    draw_functions,
    model_matrices,
    lights,
    material_registry,
    mesh_registry,
    ambient_color,
)
```

然后用 DDGI resources、light table、shadow setup 同步 shadow views：

```text
sync_shadow_views(...)
```

shadow binding 会作为 view payload 插入对应 shadow view 或主 view。`DirectionalShadowPhase` 和 `StandardMaterial` forward shading 都会使用这些资源。

### 8.19 构造 FramePipeline

```text
pipeline = self.build_runtime_pipeline(gpu)
```

`build_runtime_pipeline` 每帧把 `PipelineStep` 转成 `FramePipeline` 节点：

```text
SceneColorSeedNode
for step in steps:
    Phase      -> PhaseStepNode
    Compute    -> ComputeStepNode
    Graph      -> GraphPassStepNode
    Pass       -> RenderPassStepNode
    PostFx     -> PostFxStepNode
HeadlessKeepAliveNode
ViewportBlitNode
```

注意 `SceneColorSeedNode` 总是最先插入，它创建初始 `scene_color`。如果 pipeline 有需要 HDR input 的 post-fx，就用 `SCENE_HDR_FORMAT`，否则用 surface format。

### 8.20 构造 PreparedFrame / PreparedView

这是从 ECS/CPU preparation 进入 execution 层的边界。

Frame payload 会插入：

- `RenderSettings`
- `HistoryTextureStore`
- `GpuScene`
- `Vec<[f32; 16]>` model matrices
- `PreviousModelMatrices`
- `DdgiRuntime`
- shadow layouts
- optional shadow debug resources
- feature frame payloads

每个 view 会创建 `PreparedView`，并插入：

- `SceneView`
- `OpaquePhase`
- `TransparentPhase`
- optional `ShadowViewBinding`
- feature view payloads

`PreparedFrame` 和 `PreparedView` 的 payload store 是按 `TypeId` 索引的 typed map：

```rust
frame.payload::<RenderSettings>()
view.payload::<SceneView>()
view.payload::<OpaquePhase>()
```

这就是系统支持异构 renderer 的方式：共享 execution object 不需要知道所有 renderer 的具体字段。谁需要什么，就插入什么 typed payload。

### 8.21 执行 FramePipeline

最后：

```text
execution = pipeline.execute_frame(gpu, &frame)
```

执行完成后更新 `RenderStats`：

- step count
- view count
- light count
- draw calls
- shadow cascade count
- shadow caster count
- shadow atlas stats
- pass count
- render asset stats
- timing stats

---

## 9. PreparedFrame / PreparedView 详解

`PreparedFrame` 是本帧全局数据：

```text
surface_format
has_surface
frame payloads
views: Vec<PreparedView>
```

适合放：

- 全局设置
- 全局 GPU table
- 全局 runtime
- 跨 view 共享的数据
- history store
- material / lighting / GI runtime references

`PreparedView` 是单个 view 的数据：

```text
order
history_key
viewport
target_size
clear_surface
view payloads
```

适合放：

- `SceneView`
- 该 view 的 opaque phase
- 该 view 的 transparent phase
- 该 view 的 shadow binding
- 该 view 的 family-specific prepared payload

一个很重要的习惯：

```text
跨整个 frame 共享的数据 -> frame payload
只属于某个 view 的数据 -> view payload
pass 之间的 virtual GPU resource -> PhaseState / RenderGraph slot
renderer family 自己的 cache -> feature 或 family runtime 自己持有
```

不要把所有东西都塞进 `GpuScene`。`GpuScene` 是共享 GPU table 和 view uniform，不是万能 runtime 状态仓库。

更具体地说，`PreparedFrame` / `PreparedView` 是 SkyEngine 用来避免“一个巨型 FrameData struct”的办法。

如果直接写一个大结构，最后可能会变成：

```rust
struct FrameData {
    render_settings: RenderSettings,
    gpu_scene: GpuScene,
    sprite_data: SpritePreparedData,
    mesh_data: MeshPreparedData,
    live2d_data: Live2DPreparedData,
    tilemap_data: TilemapPreparedData,
    ddgi_data: DdgiRuntime,
    shadow_data: ShadowRuntime,
    // 以后每加一个 renderer family 都继续膨胀
}
```

这种结构会让每个 renderer family 都污染共享类型。当前做法是 typed payload：

```text
SpriteFeature 需要什么，就插入 SpriteFeature 自己的 payload。
Live2DFeature 需要什么，就插入 Live2DFeature 自己的 payload。
Shadow pass 需要什么，就插入 ShadowViewBinding。
```

读取方通过类型拿数据：

```rust
let settings = frame.payload::<RenderSettings>();
let view = prepared_view.payload::<SceneView>();
let transparent = prepared_view.payload::<TransparentPhase>();
```

所以它既保持强类型，又保持组合层不需要提前知道所有未来 renderer。

---

## 10. FramePipeline 如何执行

`FramePipeline` 在 `src/render/execution/frame_pipeline.rs`。

它有三类节点：

```text
setup_nodes
view_nodes
finalize_nodes
```

当前高层 runtime 主要使用：

- view nodes：绝大多数 phase / compute / post-fx / blit
- finalize nodes：`RenderPassStepNode`，用于 frame-level pass

执行分两大阶段：

1. `prepare_frame_graph(frame)`
2. `graph.execute(ctx, run_pass_closure)`

这两步非常关键：

```text
prepare_frame_graph:
  只声明“会有哪些 pass，它们读写哪些 virtual resources”。
  这一步通常还没有真正打开 wgpu render pass。

graph.execute:
  RenderGraph 已经排好顺序并分配好 physical resources。
  node.execute 才真正开始录制 draw/compute/copy 命令。
```

也就是说，`setup` 不是“先画一遍”，而是“把本帧计划写到图里”。真正发 GPU 命令发生在 `execute`。

### 10.1 prepare_frame_graph

`prepare_frame_graph` 会清空上一帧 graph 声明：

```text
graph.clear_frame()
pass_dispatch.clear()
completed_views.clear()
```

然后按三段声明 virtual resources 和 passes。

#### Setup nodes

```text
setup_state = PhaseState::new(surface_format, has_surface)
for setup_node:
    if enabled:
        node.setup(graph, setup_state, frame)
        register newly added graph passes -> DispatchEntry::Setup
```

setup nodes 产出的 resource slots 会作为 frame-level slots 传给后续 views。

#### View nodes

```text
ordered_view_indices = views sorted by view.order()
for view_index in ordered_view_indices:
    state = PhaseState::with_slots(frame_slots.clone(), setup scene gbuffer/shadows)
    for view_node:
        if enabled for frame and view:
            node.setup(graph, state, frame, view)
            register newly added passes -> DispatchEntry::View { node_index, view_index }
    completed_views.push(CompletedViewState::new(...state.into_parts()))
```

每个 view 有自己的 `PhaseState`。它记录：

- 当前 color slot
- scene gbuffer slots
- 自定义 resource slots
- scene shadow resources

节点按 pipeline step 顺序操作这个 state。例如：

```text
SceneColorSeedNode:
  create scene_color
  set current_color
  set scene_color

SceneNormalPrepass:
  create scene_normal / scene_velocity / scene_depth
  add render pass

OpaquePhase:
  require current_color
  ensure scene_depth
  add render pass

SsgiPass:
  read current_color, scene_depth, scene_normal
  write new current_color or indirect slot

ToneMap:
  read HDR current_color
  write LDR current_color

ViewportBlit:
  read current_color
  write surface
```

`CompletedViewState` 保存一个 view 跑完全部 view nodes 后的 resource slot 快照，finalize nodes 可以读取。

`PhaseState` 可以理解成“这个 view 当前渲染到哪一步了”的账本。比如一开始 `CURRENT_COLOR` 指向 seed 出来的 `scene_color`，经过 TAA 或 bloom 后，`CURRENT_COLOR` 可能指向一张新的 texture。后面的 pass 不需要知道前面具体创建了哪张图，只要问 state 当前颜色在哪里。

#### Finalize nodes

```text
for finalize_node:
    if enabled:
        state = FinalizePhaseState(frame_slots, frame_scene_gbuffer, completed_views)
        node.setup(graph, state, frame)
        register passes -> DispatchEntry::Finalize
```

finalize 适合需要看所有 views 结果的 pass。

### 10.2 graph compile 和 execute

`FramePipeline::execute_frame` 调用：

```text
graph.compile()
graph.execute(ctx, |compiled_pass, ctx, resources| { ... })
```

每个 `CompiledPass` 都能通过 `pass_dispatch` 找回哪个 node 负责执行：

```text
DispatchEntry::Setup
DispatchEntry::View
DispatchEntry::Finalize
```

然后构造对应 execution context：

- `SetupExecutionContext`
- `ViewExecutionContext`
- `FinalizeExecutionContext`

并调用 node 的 `execute`。

换句话说：

```text
node.setup 声明这个 pass 读写哪些 virtual resources
RenderGraph.compile 决定真实执行顺序
node.execute 收到 physical resources 并真正录制 wgpu command
```

这是第二个关键点：setup 阶段不是立刻画，而是在声明图；execute 阶段才真正画。

---

## 11. RenderGraph 在这里负责什么

`RenderGraph` 是 lower-level declarative GPU workload system。

它的输入：

```text
virtual textures
virtual buffers
render/compute/copy passes
each pass reads/writes resources
```

它的输出：

```text
compiled pass order
physical RenderTarget / Buffer allocation
PhysicalResources for execution
```

它解决的是“中间纹理和 pass 太多以后，人工管理会失控”的问题。

假设没有 RenderGraph，代码很容易变成：

```text
let normal = create_texture(...)
let depth = create_texture(...)
let hdr_a = create_texture(...)
let hdr_b = create_texture(...)

run_normal_prepass(normal, depth)
run_opaque(hdr_a, depth, normal)
run_ssgi(hdr_a, depth, normal, hdr_b)
run_taa(hdr_b, velocity, history, hdr_a)
run_tonemap(hdr_a, ldr)
run_blit(ldr, surface)
```

这在小 demo 里还行，但一旦有多 camera、shadow view、debug view、headless、不同 pipeline preset、可选 post-fx，就会出现一堆问题：

- 某个 pass 关掉后，后面的 texture 谁创建？
- 某张 texture 已经不再使用，什么时候回收？
- 两张临时 texture 生命周期不重叠，能不能复用内存？
- 某个 pass 读了一个还没写的 resource，谁报错？
- headless 没 surface 时，最后哪个 resource 算外部输出？

RenderGraph 的答案是：每个 node 只声明自己读写什么，图系统统一编译和分配。

### 11.1 Declaration

pass setup 代码通常长这样：

```rust
let color = graph.create_texture(|builder| {
    builder
        .name("scene_color")
        .size(TargetSize::Exact(width, height))
        .format(format);
});

graph.add_render_pass("opaque_phase", |setup| {
    setup.write_color_loaded(0, color);
    setup.set_depth_stencil(depth);
});
```

这时候还没有真实 GPU texture。只是声明：

- 有一个 virtual texture。
- 有一个 pass 会写它。

`virtual texture` 的意思是：它现在只是一个 handle，不是已经分配好的 `wgpu::Texture`。你可以把它当成“我要一张叫 `scene_color`、大小为 1280x720、格式为 HDR 的图”。至于它最终用哪块真实 GPU 内存，等 graph compile/allocation 后再决定。

声明阶段还会记录 pass 的访问方式：

| 访问方式 | 含义 |
|----------|------|
| write color clear | 写 color attachment，开始前清空 |
| write color loaded | 写 color attachment，保留原内容再继续画 |
| read texture | shader 采样或读取 texture |
| write storage | compute shader 写 storage texture/buffer |
| depth stencil | 作为深度/模板 attachment |

这些信息会变成依赖关系。比如一个 pass 读 `scene_normal`，那它必须排在写 `scene_normal` 的 pass 后面。

### 11.2 Compile

`compile()` 做：

1. dependency analysis：根据 reads/writes 建 pass 依赖。
2. topological sort：生成合法顺序。
3. dead-pass culling：没有外部 sink 的 pass 可以被剔除。
4. execution reorder：在不破坏依赖的前提下尝试改善 locality 和 lifetime。
5. lifetime analysis：记录每个 resource 第一次/最后一次使用。

最直观的是拓扑排序：

```text
NormalPrepass writes scene_normal
OpaquePhase reads scene_normal? no, writes scene_color/depth
SsgiPass reads scene_color + scene_depth + scene_normal

因此:
  NormalPrepass 必须在 SsgiPass 前
  OpaquePhase 必须在 SsgiPass 前
```

dead-pass culling 也很重要。一个 pass 如果写出来的结果既没有被后续 pass 读取，也没有写到 surface/imported output，graph 可以认为它对最终结果没有贡献。headless 模式下 `HeadlessKeepAliveNode` 的存在，就是为了告诉 graph：“这张最终 scene color 虽然不 present 到窗口，但测试/离屏仍然需要它，别剔除。”

### 11.3 Allocation

执行前会分配 physical resources：

- transient texture 走 pool。
- persistent texture 跨帧缓存。
- imported texture 使用外部资源。
- transient texture 可以做 memory alias：生命周期不重叠且格式/usage/尺寸条件合适时共享 physical target。

`physical resource` 才是真正的 GPU texture/buffer。一个 virtual texture 和 physical texture 的关系大概是：

```text
virtual scene_color_after_ssgi
  -> physical texture #3

virtual bloom_downsample_0
  -> physical texture #5

virtual taa_output
  -> physical texture #3   // 如果生命周期不重叠，可能复用
```

这就是 alias 的意义：中间图很多，但同一帧里并不是所有图同时活着。只要 lifetime 不重叠，graph 可以让它们共享一块物理内存，减少 GPU memory 压力。

### 11.4 Execution

`RenderGraph::execute` 会：

```text
compile
allocate_physical_resources
for compiled pass:
    if copy pass:
        graph 内部执行 copy
    else:
        调用 FramePipeline 提供的 closure
release transient resources
```

copy pass 特殊：它需要明确 submit boundary，所以可能调用 `GpuContext::flush`。

普通 draw path 不应该靠 flush 解决 buffer hazard，应该使用 frame-local upload arena 或 dynamic uniform offsets。

执行阶段发生的事情可以理解为：

```text
CompiledPass:
  我是第 N 个 pass
  我对应某个 FramePipeline node
  我的 virtual texture 已经映射到 physical texture

FramePipeline closure:
  找到这个 pass 属于哪个 node/view
  构造 execution context
  调 node.execute(...)

node.execute:
  从 PhysicalResources 拿真实 wgpu texture view
  打开 render pass / compute pass
  调 draw function 或 fullscreen pass
  录制命令到 GpuContext 的 encoder
```

所以 `RenderGraph` 不替代 draw code。它负责资源和顺序，真正怎么画仍然由对应的 node / pass / draw function 决定。

---

## 12. Phase 和 DrawFunction 如何真正画东西

以内建 `OpaquePhase` / `TransparentPhase` 为例。

先看完整路径：

```text
Extractor:
  ECS entity -> PhaseItem

Phase sort:
  Vec<PhaseItem> 按 sort_key / batch_key 排好

Phase setup:
  声明这个 phase 要写 current_color，可能要用 scene_depth

Phase execute:
  打开 wgpu render pass
  遍历 PhaseItem
  按 draw_function_id + batch_key 切批
  调 DrawFunction::draw_batch

DrawFunction:
  绑定 pipeline / bind groups / buffers
  发 draw / draw_indexed
```

`Phase` 负责“这一类绘制队列什么时候画、画到哪张 texture”。`DrawFunction` 负责“某个 item 类型具体怎么发 GPU draw call”。这两个职责不要混在一起。

### 12.1 setup

`PhaseStepNode` 调用 `RenderPhase::setup`。

内建 opaque：

- 如果 view 是 shadow view，跳过。
- 如果 opaque phase 没 item，跳过。
- 绑定当前 color 为 scene color。
- 确保有 scene depth。
- 添加一个 render pass，写 current color，带 depth。

内建 transparent：

- 如果 view 是 shadow view，跳过。
- 如果 transparent phase 没 item，跳过。
- 读取当前 color。
- 如果已有 scene depth，则 loaded depth。
- 添加一个 render pass，写 current color。

这里的 `loaded` 是 GPU render pass 里的 load 操作。比如 transparent phase 不想清掉 opaque 已经画好的颜色，所以它会 load 当前 color，然后把透明物体混上去。如果它 clear 了，前面画的不透明物体就没了。

### 12.2 execute

执行时：

1. 从 `PhysicalResources` 解析 color target 和 depth target。
2. 从 frame payload 取 `GpuScene`。
3. 从 view payload 取 `SceneView`。
4. 写 view uniform。
5. 从 view payload 取对应 phase items。
6. 遍历 phase items。
7. standalone draw function 单独处理。
8. 非 standalone items 会在一个 render pass 中分批执行。

批处理逻辑：

```text
while cursor < items.len():
    collect contiguous items with same draw_function_id and batch_key
    draw_functions.draw_batch(id, draw_ctx, batch_items)
```

`DrawContext` 提供 draw function 需要的所有 shared state：

- wgpu device
- samplers
- render pass
- view bind group/layout
- model bind group layout
- CPU model matrices
- GpuScene
- shadow resources
- material registry
- mesh registry
- fallback texture
- target/depth format

`DrawContext` 故意很“厚”，因为 draw function 处在真正发 draw call 的地方。它需要同时看见 GPU 设备、当前 render pass、材质库、mesh 库、shared scene bindings、shadow/GI 资源等。但这些东西通过上下文传进来，而不是散落成全局变量。

### 12.3 DrawMesh

`DrawMesh<M>` 是泛型 material mesh draw function。

它会：

- 根据 material 类型从 `MaterialRegistry` 找 storage。
- 根据 mesh handle 找 mesh。
- 根据 material + mesh vertex layout + target/depth format 确保 pipeline。
- 创建 material bind group。
- 绑定 view bind group。
- 绑定 model table / instance data。
- 绑定 mesh vertex/index buffer。
- 对 batch 中多个 item 做 instanced draw 或逐批 draw。

材质类型决定：

- shader source
- vertex layout contract
- bind group layout
- bind group
- render state
- scene bindings

这里的泛型 `M` 很关键：`DrawMesh<StandardMaterial>` 和 `DrawMesh<UnlitMaterial>` 是同一个绘制框架下的不同材质实例化。它们都知道怎么画 mesh，但 shader、bind group、pipeline state 可以由材质类型决定。

### 12.4 DrawSprite

`DrawSprite` 使用 builtin quad mesh 和 sprite material。

它会：

- 从 `SpriteMaterial` storage 找 texture。
- 没有 texture 时用 fallback white texture。
- 创建 texture bind group。
- 准备 sprite instance data。
- 用 nearest sampler 采样 sprite texture。
- 绘制一批 sprite。

Sprite 的 per-item 数据被压在 `SpriteDrawData` 中，phase item 本身保持小而可排序。

Sprite 批处理的直觉是：一堆 sprite 如果使用相同 texture/material/pipeline，就可以尽量放在一起画。每个 sprite 不需要单独创建一个 mesh，通常共用 builtin quad，再通过 instance data / per-item payload 区分位置、大小、颜色和 UV。

---

## 13. SceneView、viewport 和多相机

`SceneView` 是 render 的 view 级核心。

它不是 ECS component，而是每帧由 camera component + transform + projection 计算出来的 prepared view description。

生成流程：

```text
Camera entity
  -> Transform
  -> Projection
  -> optional CameraViewport
  -> build_scene_view(...)
  -> SceneView
```

`CameraViewport` 支持：

- `order`: 多 view 顺序
- `viewport`: surface 上的矩形区域
- `layer_mask`: 只渲染匹配 layer 的对象

`SceneView` 里同时有：

- CPU 侧 view/projection 数据
- GPU-ready `ViewUniform`
- frustum，用于 mesh culling
- `is_planar_2d`，用于 2D depth sort
- `TemporalViewState`，用于 TAA 和 velocity
- shadow binding / cascade 信息

present 规则：

- `SceneViewKind::Main` 会 `presents_to_surface()`。
- `SceneViewKind::DirectionalShadow` 不 present，只作为 shadow map view。
- `ViewportBlitNode` 会跳过不 present 的 view。

多相机可以用来做：

- 分屏：两个 camera 各自渲染到 surface 的不同 viewport。
- 小地图：主 camera 先画全屏，mini-map camera 后画右上角小 viewport。
- 编辑器：scene view、game view、preview view 各自独立。
- shadow：灯光视角也是 view，但它输出 shadow map，不输出到 surface。

`clear_surface` 的规则和多相机强相关。第一个真正 present 到 surface 的 view 通常负责 clear，后面的 view 应该 load 已有 surface 内容再覆盖自己的 viewport。如果每个 view 都 clear，分屏或小地图就会互相擦掉。

可以这样理解：

```text
Main camera:
  clear surface
  draw full screen

Mini-map camera:
  load surface
  draw top-right viewport
```

---

## 14. GPU 层：GpuContext 和 active frame

`GpuContext` 包装：

- `wgpu::Device`
- `wgpu::Queue`
- optional `wgpu::Surface`
- surface config
- active frame encoder
- surface texture/view
- frame upload arena
- default linear/nearest samplers

每帧生命周期：

```text
begin_frame()
  -> acquire surface texture if surface-backed
  -> create command encoder
  -> reset upload arena

render code
  -> gpu.frame().begin_render_pass(...)
  -> gpu.frame().begin_compute_pass(...)
  -> gpu.upload_vertices(...)
  -> gpu.upload_indices_u16(...)

end_frame()
  -> submit encoder
  -> present surface texture if any
```

`CommandEncoder` 可以理解为本帧 GPU 命令的录音带。render code 在 `begin_frame` 到 `end_frame` 之间不断往里面录：

```text
begin render pass
set pipeline
set bind group
set vertex buffer
draw indexed
end render pass
begin compute pass
dispatch workgroups
...
```

`end_frame()` 时 encoder 被 `finish()` 成 command buffer，交给 `queue.submit()`，GPU 才开始按这些命令工作。surface-backed context 还会 `present()`，把最终 surface texture 交给窗口系统显示。

`GpuContext::new_headless` 没有 surface，适合 render tests 和离屏工具。此时：

- `has_surface() == false`
- 仍然可以 begin/end frame。
- surface presentation path 必须跳过。
- `HeadlessKeepAliveNode` 会保留最后的 scene color，避免 graph 把无外部 sink 的 pass 全部剔除。

### 14.1 FrameUploadArena

用于 per-frame transient vertex/index uploads：

```rust
let upload = ctx.upload_vertices(&vertices);
pass.set_vertex_buffer(0, upload.slice());
```

特点：

- 每帧 reset cursor。
- buffer 容量跨帧复用。
- 不够时增长。
- index upload 会处理 copy alignment padding。

这比每个 renderer 自己手写临时 buffer 更统一。

它适合“这一帧用完就可以丢”的数据，比如临时生成的 sprite instance vertices、小批量 debug geometry。长期复用的 mesh vertex/index buffer 应该进 mesh registry，而不是每帧重新 upload。

### 14.2 DynamicUniformBuffer

用于 dynamic uniform offsets：

```rust
uniforms.clear();
let offset = uniforms.push(ctx, value);
pass.set_bind_group(0, uniforms.bind_group(), &[offset]);
```

它处理：

- `min_uniform_buffer_offset_alignment`
- stride
- buffer growth
- bind group rebuild

dynamic uniform 的意义是减少“一点点参数就创建一个 buffer/bind group”的开销。很多 draw 可以共享同一个大 uniform buffer，只是通过不同 offset 读取自己的那段数据。

---

## 15. 3D Demo 的完整运行故事

以 `examples/render/three_d_demo.rs` 为例。

读这个 demo 时，不要一开始就追 shader。先追数据形态变化：

```text
初始化:
  创建 mesh/material GPU 资源
  World 里 spawn WgpuMeshRenderer + Light + Camera

每帧 update:
  修改 Transform / RenderSettings

ctx.render:
  World 数据被 extract 成 phase items
  phase items 被 draw 成 scene_color
  post-fx 把 scene_color 变成最终 surface 颜色
```

### 15.1 main 阶段

demo 做了三件事：

1. 创建 `World`。
2. 插入 `RenderSettings`，开启 SSGI、contact shadows、bloom、sharpen、TAA、tonemap 等。
3. spawn 一个 camera entity。
4. 以 `RenderPipelineAsset::modern_3d()` 启动 App。

核心：

```rust
world.spawn((
    Transform::default(),
    CameraMarker::new(),
    Projection::perspective(55.0f32.to_radians(), 0.1, 80.0),
    MainCamera,
));

world
    .install(WindowPlugin::new("SkyEngine - 3D Demo", 1280, 720))
    .unwrap();
world.install(InputPlugin).unwrap();
world.install(AssetPlugin::default()).unwrap();
world.install(RenderPlugin::modern_3d()).unwrap();

App::new(world).run(ThreeDDemo::default());
```

### 15.2 第一次 update 初始化场景

`initialize_scene(ctx)` 通过：

```rust
ctx.with_render_runtime_mut(|renderer, gpu| { ... })
```

拿到：

- `RenderRuntime`
- `GpuContext`

然后创建：

- cube mesh
- ground mesh
- procedural textures
- normal textures
- `StandardMaterial` 实例

并把 mesh/material 插入 composer 的 registries：

```rust
renderer.insert_mesh(mesh)
renderer.insert_material::<StandardMaterial>(material)
```

之后向 `World` spawn：

- 地面、墙、方块等 `WgpuMeshRenderer`
- point light / spot light / directional light
- 动画 marker components

### 15.3 每帧 update

demo 每帧：

- 读取输入，更新 camera yaw/pitch/distance。
- 修改 camera `Transform`。
- query 并修改 block transforms。
- query 并修改 point light transforms/intensity。
- 更新 `RenderSettings` 中 debug mode。
- 调用 `ctx.render()`。

注意：render 本身不负责 gameplay 动画。它只读取 World 当前状态。

### 15.4 render 阶段

`modern_3d` 的实际 frame 会大致这样：

```text
World:
  camera + mesh renderers + lights + settings

RenderRuntime:
  resolve transforms
  collect main SceneView
  append directional shadow cascade views
  extract meshes into opaque phases for main/shadow views
  assign model matrix slots
  upload model matrix table and light table
  prepare DDGI and shadow bindings
  build PreparedFrame / PreparedView

FramePipeline:
  SceneColorSeedNode
  SceneNormalPrepass
  SceneMaterialPrepass
  DirectionalShadowPhase
  DdgiUpdateCompute
  OpaquePhase
  ContactShadows
  SsgiPass
  TransparentPhase
  TemporalAntiAliasing
  Sharpen
  Bloom
  ToneMap
  DebugView
  ViewportBlitNode
```

### 15.5 为什么 prepass 在 opaque 前

`SceneNormalPrepass` 产生：

- `scene_normal`
- `scene_velocity`
- `scene_depth`

`SceneMaterialPrepass` 产生：

- `scene_albedo`
- `scene_material`
- `scene_emissive`
- 也可确保 normal/depth/velocity 存在

后续 pass 使用它们：

- contact shadows 需要 depth/normal。
- SSGI 需要 current color、depth、normal。
- TAA 需要 velocity/history。
- debug view 可以显示这些 intermediate buffers。

opaque forward shading 再用 lights、shadow、DDGI 等资源画最终 scene color。

换句话说，prepass 是为了给后面“看屏幕空间信息”的效果铺路。它不是最终画面，但它让后面的 pass 能回答这些问题：

```text
这个像素离相机多远？
这个像素表面朝哪边？
这个像素属于粗糙还是光滑材质？
这个像素相对上一帧移动了多少？
```

没有这些信息，SSGI、contact shadows、TAA、debug view 要么做不了，要么需要在自己的 pass 里重复计算，成本和复杂度都会上升。

---

## 16. Material 系统怎么接入渲染

`Material` trait 在 `src/render/resources/material/traits.rs`。

材质不只是“颜色”。在 GPU renderer 里，材质更像一份绘制合约：

```text
这个 mesh 至少要有哪些顶点属性？
使用哪个 shader？
shader 需要哪些 texture / sampler / uniform？
是否透明？
是否写 depth？
使用什么 blend state？
是否需要 shadow/GI/scene bindings？
```

所以 `Material` 一边面向美术/渲染参数，一边也决定底层 pipeline 怎么创建。

一个 material 提供：

- shader source
- vertex layout requirement
- material interface and binding layout
- prepared material bind group creation
- render state
- shader entry names
- pipeline key
- scene bindings

关键方法：

```rust
fn interface() -> MaterialInterface;
fn shader_source(data: &Self::Data) -> ShaderSource;
fn vertex_layout(data: &Self::Data) -> VertexLayout;
fn prepare(data: &Self::Data, ctx: &mut MaterialPrepareContext<'_>) -> Result<PreparedMaterial, MaterialError>;
fn render_state(data: &Self::Data) -> MaterialRenderState;
```

`RenderPipelineBuilder::register_material::<M>()` 会：

1. 记录 material registration。
2. 注册 `DrawMesh<M>`。
3. 添加 `ExtractMeshes<M>`。

这意味着只要 material 注册了，带该 material handle 的 mesh 就能被对应 extractor 找到并进入 phase。

完整关系是：

```text
RenderPipelineBuilder::register_material::<StandardMaterial>()
  -> 注册 material 类型
  -> 注册 DrawMesh<StandardMaterial>
  -> 注册 ExtractMeshes<StandardMaterial>

World entity:
  WgpuMeshRenderer(mesh_handle, standard_material_handle)

ExtractMeshes<StandardMaterial>:
  发现这个 material handle 属于 StandardMaterial
  生成 PhaseItem<MeshDrawData>

DrawMesh<StandardMaterial>:
  使用 StandardMaterial 的 shader/layout/bind group 规则绘制
```

`SpriteFeature` 当前注册：

- `SpriteMaterial`
- `UnlitMaterial`
- `StandardMaterial`

所以 sprite、unlit mesh、standard mesh 都能走内建 pipeline。

### 16.1 Pipeline cache

material pipeline cache 的 key 来自：

- shader source
- vertex layout
- render state
- vertex entry
- fragment entry
- target format / depth format
- mesh actual vertex layout

这样相同 material pipeline 可以跨 draw 复用。

pipeline cache 很重要，因为创建 GPU render pipeline 不是一个适合在热路径里频繁做的轻操作。缓存 key 只要能准确描述“这次 draw 需要的 pipeline 状态”，相同状态就能复用已有 pipeline。

### 16.2 StandardMaterial

`StandardMaterial` 是当前 3D demo 主材质，支持：

- albedo
- albedo texture
- normal texture
- roughness
- metallic
- emissive
- shadow / lighting / GI scene binding

带 normal map 时需要 mesh 有 tangent：

```text
Position + Normal + Tangent + UV0
```

不带 normal map 时需要：

```text
Position + Normal + UV0
```

这些要求来自 shader 输入。比如没有 `Normal`，shader 就不知道表面朝向，无法正确算光照；有 normal map 时没有 `Tangent`，shader 就无法把切线空间的法线贴图转换到世界空间。

---

## 17. Shadow、DDGI、SSGI、TAA 在流水线中的位置

### 17.1 DirectionalShadowPhase

方向光 shadow 通过额外 shadow views 运行。

阴影贴图的直觉是：从灯光的位置或方向看世界，先画一张“离灯最近的深度图”。主相机画物体时，再问：

```text
这个像素从灯光看过去，是不是比 shadow map 里记录的深度更远？
如果更远，说明它被别的东西挡住了 -> 在阴影里
```

方向光没有普通意义上的位置，常用光方向加上相机视锥范围来构造 shadow view。为了远近都兼顾，现代 renderer 常用 cascade shadow map：把主相机可见范围切成几段，每段一张 shadow map。

大致流程：

```text
主 view + directional light
  -> append directional shadow cascade views
  -> 每个 shadow view 也运行 mesh extract
  -> DirectionalShadowPhase 渲染 shadow map atlas
  -> ShadowViewBinding 插入主 view payload
  -> StandardMaterial forward shading 采样 shadow resources
```

shadow view 是 `SceneViewKind::DirectionalShadow`，不 present 到 surface。

它仍然复用 view/extract/phase 的框架：只是这个 view 的目标不是最终颜色，而是 shadow depth atlas。

### 17.1.1 为什么最终画面里的“阴影”不是一个东西

调试 3D demo 时，最容易误判的一点是：屏幕上看起来发黑的区域，不一定都来自 directional shadow map。

可以先把最终颜色粗略拆成几层：

```text
final color =
  material base color
  * direct lighting
  * direct shadow factor
  + indirect lighting / GI
  + emissive
  + contact shadow / AO modulation
  + post-fx / temporal history
```

这里至少有四种东西都会让画面变暗：

| 层 | 典型来源 | 看起来像什么 | 是否来自 shadow map |
|----|----------|--------------|---------------------|
| direct shadow | directional CSM atlas | 有明确投影方向、跟太阳/方向光相关 | 是 |
| contact shadow | screen-space depth/normal ray march | 物体接触处、角落、缝隙更暗 | 不是 |
| ambient occlusion | contact shadow pass 的 AO 部分或类似项 | 角落/褶皱/贴近表面发暗 | 不是 |
| indirect/GI | DDGI、SSGI、ambient | 室内暗部、反弹光、环境亮度 | 不是 CSM |
| temporal artifact | TAA/history | 拖影、闪烁、边缘残留 | 不是 |

所以“室内物体没有影子”本身不一定是 bug。方向光如果没有直接照进室内，室内就不会有 direct shadow map 产生的投影。室内明暗主要应该由：

- ambient color；
- point/spot lights；
- emissive；
- DDGI / SSGI；
- contact shadow / AO；

共同决定。

这也是为什么 debug 时必须先分层：

```text
默认画面不对
  -> F12 direct-only 还不对：查 CSM / bias / PCF / PCSS / cascade
  -> F12 对，但默认不对：查 GI / contact / TAA / tone map
  -> G indirect-only 不对：查 GI / ambient / screen-space reconstruction
  -> 关 C 后好了：查 contact shadows / AO
  -> 关 V 后好了：查 GI provider / composite
```

### 17.1.2 当前 Directional Shadow 数据从哪里来

当前方向光阴影核心文件：

```text
src/render/lighting/shadow/
  atlas.rs       atlas rect、guard band、cascade 打包
  bindings.rs    shadow uniform / bind group layout
  view.rs        选择方向光、构造 cascade shadow views、写 ShadowViewBinding
  phase.rs       渲染 opaque / alpha-test / transparent casters 到 shadow atlas
  resources.rs   RenderGraph 资源暴露和 debug resources
  formula.rs     test-only 公式验证
```

运行时大致是：

```text
RenderRuntime::render_world
  -> 收集 main SceneView
  -> append_directional_shadow_views(...)
       -> 找到 DirectionalLight
       -> 解析 cascade_count / splits / resolution
       -> 根据 main view frustum 构造每个 cascade 的 shadow SceneView
  -> extract mesh phase items
       -> main view 有 opaque/transparent items
       -> shadow views 也有自己的 phase items
  -> sync_shadow_views(...)
       -> 创建/resize atlas
       -> 写 ShadowUniform
       -> 写每个 cascade 的 ShadowPassUniform
       -> 统计 caster count / update mask
  -> FramePipeline 执行 DirectionalShadowPhase
       -> 清 atlas rect
       -> 画 casters 到 depth atlas
       -> 可选 transparent shadow atlas
  -> OpaquePhase 中 StandardMaterial 采样 shadow bind group
```

这里的关键点是：shadow view 不是一个特殊旁路，它也是 `SceneView`。这让它能复用已有的 extraction、phase item、frustum culling、model matrix table、material registry。但它的输出不是 `scene_color`，而是 shadow atlas。

### 17.1.3 CSM 的三个空间

CSM 调试时要分清三个空间：

| 空间 | 作用 | 常见错误 |
|------|------|----------|
| camera/view space | 根据主相机深度选择 cascade | split 距离和 shader 选择不一致 |
| light space | 从光方向看 receiver/caster | light basis 翻转、depth extent 错 |
| atlas UV space | 在 shadow atlas 里采样某个 cascade rect | 越界采样、guard band 不够、cascade 串色 |

简单说：

```text
main camera 决定：这个像素属于哪个 cascade
light view 决定：这个像素从光看过去在哪里
atlas mapping 决定：去 atlas 的哪一块采样
```

如果 camera split 错，会表现为 cascade 边界突变。

如果 light view fitting 错，会表现为 caster/receiver 被裁掉、阴影缺块、随着相机转动突然跳。

如果 atlas mapping 错，会表现为采样到别的 cascade、边缘漏光、横向重复、debug cascade 和最终结果对不上。

### 17.1.4 Receiver extent 和 caster extent 不是一回事

这是当前 shadow 系统里非常重要的合同。

receiver 是“主相机这一段 cascade 能看到、需要接收阴影的区域”。它决定 shadow compare 时需要的深度精度。

caster 是“可能向 receiver 投影的物体”。它可能不在当前 camera split 里，但仍然在光线方向上挡住 receiver。

所以不能简单说：

```text
只渲染当前 cascade 视锥里的物体
```

这会漏掉这种情况：

```text
光线方向
  caster 在 camera split 外
      ↓
  receiver 在 camera split 内
```

正确思路是：

```text
receiver depth extent:
  尽量贴合接收区域，保证 depth precision

caster depth extent:
  沿 light direction 扩大，保留能投影到 receiver 的 caster
```

如果 receiver extent 太大，深度精度下降，容易 acne、peter-panning 或软阴影漂浮。

如果 caster extent 太小，会出现物体明明挡光，但 shadow atlas 里没有它。

当前测试里已经覆盖了这个合同：

```text
cargo test --features app render::lighting::shadow::view
```

其中 `directional_shadow_view_keeps_light_ray_casters_outside_receiver_slice` 专门验证 caster 可以在 receiver slice 外，但仍然保留在 shadow culling 范围内。

### 17.1.5 Bias 为什么难

Shadow acne 和 Peter Pan 现象是一对拉扯。

Shadow acne：

```text
接收面自己采样自己的 shadow depth
因为浮点误差 / depth precision / 斜率原因
被误判为在阴影里
表现为斑点、条纹、脏污
```

Peter Pan：

```text
bias 太大
阴影从物体脚下脱离
表现为物体像飘起来
```

SkyEngine 目前相关参数包括：

- material compare bias；
- receiver normal bias；
- raster depth bias；
- raster slope bias；
- texel world size；
- receiver depth range；
- PCF / PCSS filter radius。

真正的问题不是“bias 越大越干净”，而是单位必须统一：

```text
texel_world_size:
  一个 shadow texel 在世界里覆盖多大

receiver_depth_range:
  这个 cascade 的 shadow depth 范围

normal_bias_world:
  沿 normal 或 light 相关方向偏移多少世界单位

compare_bias_depth:
  最终变成 shadow map compare 使用的深度单位
```

如果这些单位混在一起，调参就会出现这种情况：

```text
近处 cascade 看起来好了
远处 cascade 坏了

正面光好了
斜面坏了

PCF 看起来好了
PCSS 又漂了
```

所以现在 `src/render/lighting/shadow/formula.rs` 里保留了 test-only 公式测试，用来把关键公式固定下来。

### 17.1.6 PCF 和 PCSS 不能修几何错误

PCF / PCSS 是过滤策略，不是 shadow view fitting 的替代品。

PCF：

```text
围绕当前 UV 多采样几次 shadow map
平均结果
边缘变软一点
```

PCSS：

```text
先找 blocker
根据 receiver 和 blocker 的深度差估计 penumbra
再扩大 filter radius
近处硬，远处软
```

PCSS 可以让边缘更自然，但也会掩盖问题：

- cascade fitting 错了，PCSS 可能把错边界糊开；
- bias 错了，PCSS 可能让接触处更漂；
- atlas clamp 错了，PCSS 扩大采样半径后更容易采到错误区域；
- blocker search 错了，软阴影半径会忽大忽小。

所以排查 CSM 时，优先使用硬一点、确定性更强的模式：

```text
filter radius = 0
contact shadows off
GI off
TAA off
direct-only
```

确认几何和 depth compare 对了，再判断 PCSS 质量。

### 17.1.7 WickedEngine 对齐到什么程度

WickedEngine 是参考，不是复制粘贴目标。

对齐重点：

- cascade selection 的边界行为；
- cascade edge fade 曲线；
- PCSS radius remap；
- blocker search 的判断方式；
- penumbra scale；
- atlas clamp / guard band；
- directional shadow camera 的稳定性；
- debug view 的可观察性。

不应该直接照搬：

- HLSL 资源绑定形状；
- DirectX clip-space 假设；
- Wicked 的全 renderer 架构；
- 与 SkyEngine ECS / RenderGraph / wgpu 不匹配的对象生命周期。

目前公式层对齐用 `src/render/lighting/shadow/formula.rs` 固定。例如 PCSS radius 使用 Wicked 风格的：

```text
filter_radius_texels = radius * 8 + 2
cap = 36 texels
```

PCSS penumbra 使用：

```text
penumbra_scale = receiver_blocker_gap * 200
cap = 4
```

这些不是“抄代码”，而是把 reference renderer 的数学合同翻译成 SkyEngine 可测试的 Rust/WGSL 合同。

### 17.1.8 当前阴影验证层级

不要只靠肉眼看 demo。当前验证分三层：

| 层级 | 文件 | 证明什么 |
|------|------|----------|
| CPU formula | `src/render/lighting/shadow/formula.rs` | 公式、边界、Wicked fixture 数字 |
| CPU view geometry | `src/render/lighting/shadow/view.rs` tests | cascade fitting、split corners、caster extent、atlas mapping |
| GPU readback | `src/render/runtime/tests/shadows.rs` | 实际 wgpu pass、shader、depth atlas、final color 是否工作 |

常用命令：

```bash
cargo test --features app render::lighting::shadow::formula
cargo test --features app render::lighting::shadow::view
cargo test --features app render::runtime::tests::shadows
```

现在 GPU readback 覆盖包括：

- perspective directional shadow view 是否启用；
- orthographic directional shadow view 是否启用；
- shadow atlas 是否写入 caster depth；
- caster 在 light ray 上但不在 near receiver slice 中时，仍能进入 shadow atlas；
- standard material final color 会被 directional shadow 压暗；
- far receiver pixel 不应被整片错误压暗；
- cascade boundary 附近不应出现明显亮度突变；
- per-cascade LOD mask stats 是否正确。

这套测试的意义是：当 demo 又出现“阴影突变”时，先看这些测试有没有坏。如果测试没坏，问题更可能在 demo 的 GI/contact/TAA/具体场景参数，而不是基础 CSM 公式。

### 17.1.9 Debug view 要回答的问题

当前 three_d_demo 的 shadow debug keys：

| 按键 | Debug view | 应该回答的问题 |
|------|------------|----------------|
| `F1` | lit scene | 默认最终画面是否正常 |
| `F2`-`F5` | raw directional shadow cascade 0-3 | atlas 中每个 cascade 有没有写入合理 depth |
| `F6` | sampled directional shadow cascade coverage | shader 最终采样认为哪些地方被 shadow 覆盖 |
| `F7` | view-depth split cascade coverage | 根据主相机 view depth，像素属于哪个 cascade |
| `F8` | cascade fade weight | cascade 边缘过渡是否在预期区域 |
| `F9` | shadow compare-depth delta | receiver compare depth 和 shadow map depth 差值是否合理 |
| `F10` | shadow bias / texel pressure | bias 是否过大或过小 |
| `F11` | PCSS blocker / penumbra / filter size | blocker search 和软阴影半径是否异常 |
| `F12` | direct lighting only | 只看直射光，排除 GI/contact/post-fx 干扰 |
| `G` | indirect lighting only | 只看间接光/GI/ambient 相关 |
| `C` | contact shadows toggle | 判断接触阴影是否制造条纹或过暗 |
| `V` | GI toggle | 判断 GI 是否改变室内暗部或间接亮度 |
| `R` | repro camera lock | 锁定固定复现相机，避免换角度误判 |

最重要的两个隔离视图是：

```text
F12 direct-only:
  如果这里已经错，优先查 direct shadow / CSM / bias / PCSS。

G indirect-only:
  如果这里错，优先查 GI / ambient / SSGI/DDGI composite。
```

### 17.1.10 为什么 F6 正常但最终画面仍可能不对

`F6` 看的是 directional shadow coverage，重点是 CSM 采样结果。

最终画面还会叠：

- BRDF diffuse/specular；
- light intensity/color；
- normal map；
- material albedo/roughness/metallic；
- indirect diffuse；
- contact shadows；
- GI composite；
- bloom/tone map/TAA/debug override。

所以这类现象不矛盾：

```text
F6 cascade coverage 看起来连续
但默认画面有室内暗块
```

这种情况下要先查：

```text
G indirect-only
C contact off
V GI off
SKY_DEMO_DISABLE_TAA=1
```

而不是马上去改 cascade split 或 bias。

### 17.2 DdgiUpdateCompute

`DdgiUpdateCompute` 是 `ComputePass`。它根据 `RenderSettings.global_illumination` 决定是否有效。

DDGI runtime 在 composer prepare 阶段已经用 mesh/lights/materials 准备好 probe update 所需数据。compute step 负责把本帧 probe update 工作写入 GPU resources。

DDGI 可以先粗略理解成“在世界里放很多探针，探针记录周围间接光”。它不是只看当前屏幕，所以理论上能提供屏幕外物体贡献的间接光，但需要维护 probe 数据，通常用 compute pass 更新。

### 17.3 SsgiPass

`SsgiPass` 是 `PostFxPass`，在 `src/render/gi/ssgi.rs`。

启用条件：

```text
RenderSettings.global_illumination.uses_ssgi()
并且 view 不是 shadow view
```

它要求 HDR input，并依赖：

- current scene color
- scene depth
- scene normal

现代 3D pipeline 中它放在 opaque 和 transparent 之间：

```text
OpaquePhase
ContactShadows
SsgiPass
TransparentPhase
```

这样不透明物体贡献屏幕空间 GI，透明物体后画。

SSGI 是 screen-space global illumination。它只使用当前屏幕上已经有的信息，比如 color、depth、normal，所以实现上更像后处理。优点是接入相对轻，缺点是屏幕外信息不可见：屏幕外的墙不会贡献反弹光，深度/法线不完整时也会有近似误差。

DDGI 和 SSGI 的直觉区别：

| 技术 | 依赖什么 | 优点 | 局限 |
|------|----------|------|------|
| SSGI | 当前屏幕的 color/depth/normal | 接近 post-fx，比较直接 | 看不到屏幕外信息 |
| DDGI | 世界空间 probes / compute 更新 | 可以表达屏幕外间接光 | 需要 probe 管理和更新成本 |

### 17.4 TemporalAntiAliasing

TAA 使用：

- `TemporalViewState`
- history textures
- velocity buffer
- current color

`RenderRuntime` 每帧先更新 temporal view state，并把 `HistoryTextureStore` 放进 frame payload。TAA pass 通过 payload 和 scene slots 找到所需资源。

TAA 的核心想法是：不要只看当前这一帧，而是把历史帧也拿来平均，减少锯齿和闪烁。难点是物体和相机在动，上一帧的像素不一定对应这一帧同一个位置，所以需要：

- jitter：每帧让投影有一点点亚像素偏移，积累更多采样。
- velocity：告诉当前像素上一帧大概在哪里。
- history texture：保存上一帧或历史累积颜色。
- reset：相机切换、viewport 改变、历史无效时不能继续混。

如果 velocity 或 history 管理错，常见现象就是拖影、抖动、边缘糊。

### 17.5 DebugView

`DebugView` 是最后的 post-fx，用 `RenderSettings.debug_view` 决定是否覆盖输出。它可以显示：

- scene depth
- normal
- albedo
- roughness
- metallic
- emissive
- velocity
- light
- indirect diffuse
- directional shadow cascade
- SSGI atlas

DebugView 对学习渲染也很有用。你可以把最终画面拆开看：

```text
depth 是否正常？
normal 是否方向合理？
albedo 有没有贴图？
roughness/metallic 是否写对？
velocity 是否只在运动处明显？
shadow cascade 是否覆盖相机视野？
SSGI atlas 是否有内容？
```

很多“最终画面黑了”的问题，往往不是最后 tone map 错，而是前面某张中间 texture 从一开始就是空的。

---

## 18. 如何新增一个 renderer family

假设你要加一个新的 text renderer、particle renderer 或特殊 2D renderer。

推荐路径：

```text
1. 在 src/render/component/ 增加 ECS authoring component
2. 在自己的 family 目录里写 prepare/cache/upload/runtime
3. 实现 RenderFeature
4. 在 register() 中注册 draw function、extractor、phase/pass/gpu table/material
5. 每帧 extract() 读取 World 或缓存 family 数据
6. prepare() 上传 family-owned GPU data
7. append_phase_items() 把 draw items 放进 OpaquePhase/TransparentPhase
8. insert_frame_payloads() / insert_view_payloads() 放入执行期需要的数据
9. DrawFunction 执行实际 draw
```

不要第一步就改 `RenderRuntime::render_world` 塞一个大分支。只有真正跨 renderer 的概念才应该进入 composer 或 shared execution state。

### 18.1 什么时候用 phase

用 phase 的场景：

- 你的 renderer 最终还是“把一些 item 画到 scene color/depth”。
- 需要和 sprite/mesh 一起排序。
- 需要复用 view uniform、model matrix table、material/mesh registry。

### 18.2 什么时候用 PostFxPass

用 post-fx 的场景：

- 输入是前面产生的 scene color/depth/normal 等 textures。
- 输出是新的 current color 或某个 scene texture。
- 典型 fullscreen pass 或 compute+fullscreen 后处理。

### 18.3 什么时候用 GraphPass

用 graph pass 的场景：

- 你需要直接声明复杂 RenderGraph resources。
- 你有自己的 render/compute pass 组合。
- 你想使用 transient texture、copy pass、custom resource slots。

### 18.4 什么时候用 GpuTable

用 GPU table 的场景：

- 数据会被多个 renderer 或多个 material 共享。
- 它适合作为 bind group/table 暴露。
- 例如 model matrices、lights。

如果只是某个 renderer family 内部使用的 buffer，留在该 family runtime 内即可。

---

## 19. 调试和理解 render 的实用方法

### 19.1 看 pipeline descriptor

`RenderPipelineAsset::descriptor()` 可以告诉你 pipeline 里有什么：

- backend kind
- feature names
- step names
- extractor names
- gpu table names
- draw function names
- material names

适合确认 builder 是否注册了你想要的东西。

### 19.2 看 RenderStats

每次 `ctx.render()` 后可以：

```rust
let stats = ctx.render_stats();
```

关注：

- `view_count`
- `step_count`
- `passes`
- `draw_calls`
- `light_count`
- `shadow_draw_calls`
- `shadow_cascade_count`
- render asset stats
- timings

如果画面空白，先看 draw_calls 和 view_count。

### 19.3 开启 debug log

部分路径支持：

```text
SKY_RENDER_DEBUG_LOG=1
```

`three_d_demo` 中也有几个环境变量：

```text
SKY_DEMO_DISABLE_SSGI
SKY_DEMO_DISABLE_GI
SKY_DEMO_DISABLE_TAA
SKY_DEMO_DISABLE_BLOOM
SKY_DEMO_DISABLE_CONTACT_SHADOWS
SKY_DEMO_LOCK_REPRO_CAMERA
```

用于快速二分某个 post-fx 是否造成问题。

### 19.4 three_d_demo 阴影/GI 排查手册

这一节是当前 3D demo 阴影问题的实战排查路线。目标不是“凭感觉调到好看”，而是快速定位问题属于哪一层。

先固定复现：

```powershell
$env:SKY_RENDER_DEBUG_LOG="1"
$env:SKY_DEMO_LOCK_REPRO_CAMERA="1"
cargo run --example three_d_demo --features app --release
```

也可以运行 demo 后按 `R` 锁定/解锁 canonical repro camera。

固定相机很重要。否则你每次看到的 cascade split、室内暗部、screen-space GI 输入都不一样，调试会变成“刚才那个角度好像不对”。

#### 19.4.1 第一步：先判断是不是 direct shadow

按 `F12`。

`F12` 是 direct lighting only。它的目的不是让画面好看，而是把 GI/contact/间接光干扰尽量剥掉。

判断：

```text
F12 仍然有阴影割裂、突变、漂浮、缺块
  -> 优先查 directional shadow / CSM / atlas / bias / PCF / PCSS

F12 看起来基本合理，默认画面不合理
  -> 不要先改 CSM
  -> 优先查 GI / contact shadows / TAA / tone map
```

这一步能避免最常见的误判：把室内 indirect/GI 暗部当成 directional shadow bug。

#### 19.4.2 第二步：只看 indirect

按 `G`。

`G` 是 indirect lighting only。它应该帮助你观察：

- ambient 是否太暗；
- DDGI/SSGI 是否给室内错误加暗或加亮；
- emissive/indirect 是否参与；
- 是否出现 direct shadow 那种硬边界。

判断：

```text
G 下也有明显硬边界
  -> indirect-only 分支可能混入了 direct shadow 或 direct light

G 下室内整体过黑/过亮
  -> 查 GI provider / ambient / composite intensity

G 下正常，默认画面不正常
  -> 查 direct + indirect 合成、contact shadows、tone map/TAA
```

#### 19.4.3 第三步：关 contact shadows

按 `C`，或者启动前：

```powershell
$env:SKY_DEMO_DISABLE_CONTACT_SHADOWS="1"
```

contact shadows 是 screen-space 的。它读 scene depth/normal，然后在屏幕空间做短距离 ray marching 或 AO。它不是 shadow map。

如果关 `C` 后问题明显消失：

```text
优先查:
  src/render/postfx/contact_shadows.rs
  src/render/shaders/postfx/contact_shadows.wgsl
  RenderSettings.contact_shadows

重点看:
  max_distance
  thickness
  ray_steps
  ao_intensity
  ao_radius_pixels
  depth discontinuity fade
```

常见 contact shadow 问题：

- 角落整片发黑；
- 物体边缘出现条纹；
- 随相机移动产生屏幕空间滑动；
- depth discontinuity 附近硬断；
- AO 和 direct shadow 叠乘后过暗。

#### 19.4.4 第四步：关 GI

按 `V`，或者启动前：

```powershell
$env:SKY_DEMO_DISABLE_GI="1"
```

历史兼容也支持：

```powershell
$env:SKY_DEMO_DISABLE_SSGI="1"
```

如果关 GI 后室内暗部明显变化，说明问题不在 shadow map 本体。

判断：

```text
V off 后 direct shadow 边界仍突变
  -> 查 CSM

V off 后室内暗部恢复合理
  -> 查 GI / ambient / indirect composite

V off 后画面过平但阴影边界正常
  -> GI 是主要贡献层，CSM 大概率不是根因
```

注意：`three_d_demo` 当前默认 runtime 会把 GI 设置成 DDGI provider，而启动时的 `RenderSettings` 可能配置过 SSGI。update loop 每帧会根据 demo 状态刷新 `settings.global_illumination`。所以排查时要看窗口标题或 `SKY_RENDER_DEBUG_LOG=1` 输出，确认当前实际 GI 是 off、ddgi、ssgi 还是 provider。

#### 19.4.5 第五步：关 TAA

启动前：

```powershell
$env:SKY_DEMO_DISABLE_TAA="1"
```

TAA 不应该改变真实光照关系，但会改变你看到的边缘和历史残留。

如果关 TAA 后问题改善：

```text
优先查:
  scene_velocity
  jittered vs unjittered view-proj
  temporal history reset
  history clamp
  post-fx order
```

常见 TAA 相关现象：

- 阴影边缘拖影；
- camera orbit 时暗块滞后；
- cascade 切换处残留上一帧；
- 细节变糊，看起来像 PCSS 过软。

当前 shadow view corner 计算有测试覆盖 TAA jitter：

```bash
cargo test --features app render::lighting::shadow::view
```

其中 `directional_shadow_corners_ignore_taa_jittered_view_projection` 保证 shadow cascade corners 使用 unjittered view-projection，不被 TAA jitter 直接污染。

#### 19.4.6 第六步：看 cascade debug

如果 `F12` 已经说明 direct shadow 有问题，进入 cascade debug：

| 按键 | 看什么 | 如果异常 |
|------|--------|----------|
| `F2`-`F5` | raw cascade atlas | 没写入、被裁、depth 大片清空 |
| `F6` | sampled coverage | shader 采样结果是否连续 |
| `F7` | split coverage | 主相机深度属于哪个 cascade |
| `F8` | fade weight | fade 是否只在边缘区域发生 |
| `F9` | compare-depth delta | receiver 和 atlas depth 差值是否突然跳 |
| `F10` | bias/texel pressure | bias 是否压过真实接触关系 |
| `F11` | PCSS blocker/penumbra | blocker search 是否忽大忽小 |

对应推理：

```text
F7 边界位置和 F6 突变位置一致
  -> cascade split/fade 相关

F2-F5 某个 cascade 没有 caster
  -> shadow view culling / caster extent / layer mask / cast_shadow flag

F9 在边界附近突然大跳
  -> light_view_proj / depth range / compare bias

F10 显示压力很大
  -> texel_world_size、depth_range、normal_bias、compare_bias 单位要查

F11 penumbra 忽然变大
  -> PCSS blocker search 或 depth gap 问题
```

#### 19.4.7 第七步：切换到最小测试，不在 demo 里硬猜

如果 demo 里看不清，跑最小测试：

```bash
cargo test --features app render::lighting::shadow::formula
cargo test --features app render::lighting::shadow::view
cargo test --features app render::runtime::tests::shadows
```

三类测试分别回答：

```text
formula:
  数学公式和 Wicked fixture 是否一致

view:
  cascade view 几何、split、caster extent、atlas mapping 是否成立

runtime shadows:
  GPU pass、shader、depth atlas、final color readback 是否成立
```

如果这些都过，但 demo 不对：

```text
优先怀疑:
  demo 场景参数
  GI/contact/TAA/post-fx
  material normal map
  特定角度的 screen-space artifact
```

如果这些测试有一个坏了：

```text
先修测试指向的层
不要在 demo 里继续调参
```

#### 19.4.8 现象到模块的快速映射

| 现象 | 优先查 |
|------|--------|
| cascade 边界硬跳 | `src/render/lighting/shadow/view.rs`、shader cascade selection |
| 物体脚下阴影漂浮 | bias、normal bias、PCSS filter radius |
| 表面有斑点/条纹 | compare bias、raster bias、depth precision |
| 某个物体完全不投影 | cast_shadow flag、layer mask、shadow view culling、phase extraction |
| 室内整体过暗 | GI、ambient、contact AO、tone map |
| 关 contact 后好了 | `contact_shadows.wgsl`、depth/normal 输入 |
| 关 GI 后好了 | DDGI/SSGI provider、GI composite |
| camera 动时残影 | TAA、velocity、history reset |
| F6 正常但默认不对 | direct shadow 基本正常，查 GI/contact/material/post-fx |
| raw atlas 对但 final 错 | material shader sampling/bias/atlas mapping/debug branch |

#### 19.4.9 排查时不要做什么

不要一上来做这些：

- 直接把 bias 调大；
- 直接把 PCSS radius 调小；
- 直接改 cascade distances；
- 看到室内暗就改 directional shadow；
- F6 看不懂就跳去重写 shadow phase；
- 不跑测试，只靠 demo 一帧肉眼判断。

可以临时调参做诊断，但不能把“看起来好一点”当成修复。真正要留下的修改，应该至少回答：

```text
修的是哪一层？
为什么是这层？
有没有对应测试？
是否影响 StandardMaterial 和 StandardMaterialNormalMapped 两条 shader？
是否影响 direct-only / indirect-only debug？
是否影响 contact/GI/TAA 分层？
```

#### 19.4.10 推荐的单次排查记录模板

每次遇到“阴影又不对”，建议记录：

```text
demo:
  example: three_d_demo
  camera: repro locked / free
  yaw:
  pitch:
  distance:

settings:
  GI: off / ddgi / ssgi / provider
  contact shadows: on / off
  TAA: on / off
  bloom: on / off
  debug view:

observations:
  default:
  F12 direct-only:
  G indirect-only:
  C off:
  V off:
  F6:
  F7:
  F9/F10/F11:

classification:
  direct shadow / GI / contact / TAA / material / unknown

commands:
  cargo test --features app render::lighting::shadow::formula
  cargo test --features app render::lighting::shadow::view
  cargo test --features app render::runtime::tests::shadows
```

这个模板的价值是强迫问题先归类。归类以后，代码修改范围会自然缩小。

### 19.5 常见空画面检查顺序

1. App 是否安装了 pipeline：`world.install(RenderPlugin::...)`。
2. 每帧是否调用了 `ctx.render()`。
3. World 是否有 enabled camera。
4. Camera 是否有合理 projection。
5. Mesh/Sprite entity 是否 visible。
6. Layer mask 是否匹配 view layer mask。
7. Mesh 是否在 frustum 内。
8. Material 类型是否注册。
9. Mesh vertex layout 是否满足 material requirement。
10. Texture asset 是否 GPU ready，或 fallback 是否正常。
11. Phase 是否有 items。
12. Draw function id 是否对应注册表。
13. RenderGraph pass 是否被 dead culling。
14. ViewportBlit 是否运行，view 是否 presents_to_surface。

### 19.6 常用验证命令

文档修改通常不需要跑测试。改 render 代码时按范围选择：

```bash
cargo test --features app
cargo test --features app graph
cargo test --features app render::runtime::tests
cargo test --features app render::extract
cargo check --examples --features app
```

改 high-level render API 或 demo 兼容面时，优先跑：

```bash
cargo check --examples --features app
```

改 directional shadow / GI / contact shadow 时，建议至少跑：

```bash
cargo fmt
cargo test --features app render::lighting::shadow::formula
cargo test --features app render::lighting::shadow::view
cargo test --features app render::runtime::tests::shadows
cargo test --features app render::postfx::contact_shadows
cargo check --examples --features app
```

这些命令对应的覆盖面：

| 命令 | 覆盖 |
|------|------|
| `shadow::formula` | 数学公式和 Wicked fixture |
| `shadow::view` | CSM 几何、split、caster extent、atlas mapping |
| `runtime::tests::shadows` | GPU shadow atlas、final color readback、cascade boundary |
| `postfx::contact_shadows` | screen-space contact/AO 合同 |
| `check --examples` | demo 和 public render API 兼容 |

---

## 20. 关键不变量

### 20.1 组合边界

- `RenderPipelineAsset` 是声明配置。
- `RenderRuntime` 是 runtime orchestrator。
- `PreparedFrame` / `PreparedView` 是异构 renderer 的组合边界。
- `FramePipeline` 是执行引擎。
- `RenderGraph` 是 pass/resource 后端。

### 20.2 ECS 和 render 的边界

- `World` 保存 gameplay 和 authoring 数据。
- renderer 每帧从 World extract，不把 gameplay 状态长期藏进 renderer cache。
- renderer cache 可以保存 GPU resources、pipeline cache、family-specific prepared data。

### 20.3 View 和 phase 的边界

- camera/projection/viewport/frustum 属于 `render/view`。
- 每个 view 有自己的 phase items。
- shadow view 也是 view，但不 present 到 surface。
- layer mask 和 frustum culling 应在 extraction 阶段处理。

### 20.4 Resource slots

- 当前 color 用 `ResourceSlotMap::CURRENT_COLOR`。
- canonical scene textures 用 `SceneGBufferSlots`。
- 临时自定义资源可以用 generic slot map。
- post-fx 应通过 context helper 读写 current color 和 scene textures。

### 20.5 RenderGraph

- handle validation 必须用 handle token。
- `compile()` 是 execution order 的单一来源。
- pass reads/writes 必须完整声明，否则依赖顺序会错。
- copy pass 要注册 reads/writes，并在需要时 flush active frame encoder。
- transient pool key 要便宜 hash。
- 不要在 compilation pipeline 引入重 per-frame allocation。

### 20.6 GPU frame

- 所有 draw/compute recording 必须在 `begin_frame` / `end_frame` 之间。
- headless context 没有 surface，surface path 必须检查 `has_surface()`。
- 普通 draw path 不应依赖 `flush()` 做同步。
- per-frame geometry 优先用 `FrameUploadArena`。
- per-draw uniform 优先用 `DynamicUniformBuffer`。

---

## 21. 建议继续阅读源码路径

按学习顺序：

1. `examples/render/three_d_demo.rs`
2. `src/app/runner.rs`
3. `src/render/backend/wgpu.rs`
4. `src/render/runtime/frame_coordinator.rs`
5. `src/render/runtime/view_collection.rs`
6. `src/render/extract/sprite.rs`
7. `src/render/extract/mesh.rs`
8. `src/render/phase/item.rs`
9. `src/render/phase/containers.rs`
10. `src/render/phase/mesh_draw.rs / sprite_draw.rs`
11. `src/render/runtime/pipeline_runtime.rs`
12. `src/render/execution/step_nodes/`
13. `src/render/execution/payload.rs`
14. `src/render/execution/frame_pipeline.rs`
15. `src/render/execution/slots.rs`
16. `src/render/graph/AGENTS.md`
17. `src/render/builtins/`
18. `src/render/gi/ssgi.rs`
19. `src/render/lighting/shadow/`
20. `src/gpu/context.rs`

读完这些，你基本就能回答：

- 一个 ECS mesh entity 如何变成 GPU draw call？
- 一个 camera 如何变成 view uniform？
- 一个 post-fx 如何找到前面的 scene color？
- 多 view 为什么不会互相覆盖？
- Shadow view 如何加入同一条 frame pipeline？
- 为什么 `PreparedFrame` / `PreparedView` 是扩展新 renderer family 的关键？

---

## 22. 最小心智模型复盘

最后用一个最短版本复盘：

```text
App 管 frame:
  begin_frame -> user update -> ctx.render -> end_frame

World 管数据:
  Transform / Camera / Renderer / Light / Settings

RenderRuntime 管准备:
  transforms -> views -> phases -> gpu tables -> prepared frame

PipelineAsset 管声明:
  features + steps + extractors + draw functions + materials

FramePipeline 管执行:
  nodes setup graph -> graph compile/allocate -> nodes execute

RenderGraph 管资源:
  virtual resources -> dependencies -> physical resources -> pass order

GpuContext 管 wgpu:
  device/queue/surface/encoder/upload/submit/present
```

如果以后你在 render 里迷路，先问自己这三个问题：

1. 我现在处理的是 ECS authoring 数据、prepared CPU 数据，还是 GPU resource？
2. 这个数据是 frame 级、view 级、pass/resource 级，还是某个 renderer family 私有？
3. 我要新增的是 feature、extractor、phase item、draw function、pipeline step，还是 RenderGraph pass？

这三个问题通常能把代码放回正确的层。
