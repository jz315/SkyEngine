# SkyEngine 统一 3D 可编程渲染架构

## 设计原则

1. **3D First** — 2D 只是正交相机 + quad mesh + sprite material
2. **用户可编程** — 用户可定义 Material、DrawFunction、RenderPass
3. **引擎不知道 sprite** — sprite 是引擎提供的一个 *内置 Material 实现*，不是架构的一部分
4. **渐进式复杂度** — 简单场景几行代码，复杂场景有完全控制权
5. **一行注册** — 用户新增 Material 类型只需 `renderer.register_material::<M>()`

---

## 用户视角 API（先看目标）

### 最简单的用法：sprite 游戏（体验和现在一样）

```rust
// 用户代码 — 和现在一模一样
world.spawn((
    Transform::new(0.0, 0.0),
    SpriteRenderer::new(64.0, 64.0).texture(player_tex),
    SortingLayer(0),
));

// 引擎内部: SpriteRenderer → Mesh::QUAD + SpriteMaterial
// 用户完全不需要知道 Mesh 和 Material 的存在
```

### 中级用法：自定义 3D mesh

```rust
// 加载 3D mesh
let mesh = Mesh::from_gltf(gpu, "assets/cube.glb")?;

// 注册 Material 类型（只需一次）
renderer.register_material::<StandardMaterial>();

// 创建 material 实例，存入 typed storage，获得 type-erased handle
let mat_handle = renderer.materials_mut::<StandardMaterial>().insert(StandardMaterial {
    albedo: Color::RED,
    albedo_texture: Some(brick_tex),
    metallic: 0.0,
    roughness: 0.8,
    ..Default::default()
});

world.spawn((
    Transform::from_xyz(0.0, 1.0, -5.0),
    MeshRenderer::new(mesh, mat_handle),
));

// 正交相机 → 2D 画面
// 透视相机 → 3D 画面
// 同一个管线，同一个 depth buffer
```

### 高级用法：自定义 Material

```rust
// 用户定义自己的 shader + 参数
struct HologramMaterial {
    color: Color,
    scan_speed: f32,
    noise_texture: Texture,
}

impl Material for HologramMaterial {
    fn shader_source(&self) -> ShaderSource {
        ShaderSource::Wgsl(include_str!("hologram.wgsl"))
    }

    fn vertex_layout(&self) -> &[VertexAttribute] {
        Mesh::VERTEX_LAYOUT_POSITION_NORMAL_UV
    }

    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        // 声明 shader 需要的 uniform/texture bindings
    }

    fn create_bind_group(&self, ctx: &MaterialBindContext) -> wgpu::BindGroup {
        // 创建当前参数的 bind group
    }

    fn render_state(&self) -> MaterialRenderState {
        MaterialRenderState::transparent() // alpha blend, depth read
    }

    fn pipeline_key(&self) -> u64 {
        // 相同 key = 可共用同一个 wgpu::RenderPipeline
        hash(Self::shader_source, Self::vertex_layout, Self::render_state)
    }
}

// 一行注册 — 自动注册 DrawFunction、Extract 逻辑、PipelineCache layout
renderer.register_material::<HologramMaterial>();

// 创建实例
let handle = renderer.materials_mut::<HologramMaterial>().insert(HologramMaterial { ... });
world.spawn((
    Transform::from_xyz(0.0, 0.0, 0.0),
    MeshRenderer::new(some_mesh, handle),
));
```

### 高级用法：自定义 Render Pass

```rust
let pipeline = RenderPipelineBuilder::new()
    .add_phase::<OpaquePhase>()           // 引擎内置
    .add_phase::<TransparentPhase>()       // 引擎内置
    .add_pass(OutlinePass::new())          // 用户自定义!
    .add_postfx(Bloom::default())          // 引擎内置
    .add_postfx(ToneMap::default())        // 引擎内置
    .add_pass(UIPass::new())               // 用户自定义!
    .build();

composer.set_pipeline(pipeline);
```

---

## 核心架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    用户 API 层                                  │
│ SpriteRenderer, MeshRenderer, PointLight, Transform, Camera     │
│ (ECS 组件，挂到 entity 上就完事)                                │
├─────────────────────────────────────────────────────────────────┤
│                    Material 注册系统                             │
│ renderer.register_material::<M>() — 一行注册，三件事:           │
│   1. DrawMesh<M> → DrawFunctionRegistry                         │
│   2. extract_meshes::<M>() → Extract Schedule                   │
│   3. M::bind_group_layout → PipelineCache                       │
│ 内置: SpriteMaterial, StandardMaterial, UnlitMaterial            │
│ 用户: impl Material for MyMaterial                              │
├─────────────────────────────────────────────────────────────────┤
│                    Material 存储                                 │
│ MaterialStorage<M> — per-type typed storage (在 Renderer 中)     │
│ MaterialHandle — type-erased handle, ECS 组件上只存 handle       │
│ PipelineCache — material key → wgpu::RenderPipeline 自动缓存    │
├─────────────────────────────────────────────────────────────────┤
│                    Mesh 系统                                    │
│ struct Mesh { vertex_buffer, index_buffer, sub_meshes }         │
│ 内置: Mesh::QUAD (sprite 用), Mesh::from_gltf(...)             │
│ 用户: Mesh::from_vertices(layout, data)                         │
├─────────────────────────────────────────────────────────────────┤
│                    Render Phase 系统                             │
│ OpaquePhase: front-to-back, depth write, auto-batch            │
│ TransparentPhase: back-to-front, alpha blend, depth read       │
│ 每个 phase 内部: sort key → DrawFunction dispatch              │
├─────────────────────────────────────────────────────────────────┤
│                    DrawFunction 注册表                           │
│ trait DrawFunction { fn draw(&self, ...); }                     │
│ 通用: DrawMesh<M: Material> (per-Material 自动注册)             │
│ 不透明: DrawLive2D (内部自管 drawable 排序/clipping)            │
│ 用户: impl DrawFunction for MyCustomDraw                        │
├─────────────────────────────────────────────────────────────────┤
│               RenderPipelineBuilder (声明式配置)                 │
│ phases + custom passes + postfx → build() → RenderPipelineAsset │
│ RenderPipelineAsset 由 FramePipeline 在 runtime 执行            │
├─────────────────────────────────────────────────────────────────┤
│            场景数据分层: cache / prepare / GPU upload            │
│ SceneCache: ECS extract cache (CPU)                             │
│ PreparedFrame/View: 可见集 / sort / draw spans / phase items    │
│ GpuTableManager: 通用 GPU 表注册 + 统一 upload                  │
│ GpuScene: GpuTableManager 的消费者，按需注册表                   │
│   view_uniforms (硬编码，唯一真正全局的数据)                     │
│   + 注册表: model_matrices, lights, ... (可扩展，零修改)        │
│ Material 参数: 由各 Material 自己通过 create_bind_group() 管理   │
├─────────────────────────────────────────────────────────────────┤
│ FramePipeline (执行引擎) │ RenderGraph (保留) │ GpuContext/wgpu │
└─────────────────────────────────────────────────────────────────┘
```

**关键分层说明:**

- `RenderPipelineBuilder` 是**声明式 API** — 用户用它描述「我想要什么管线」
- `RenderPipelineAsset` 是 `build()` 的产物 — 一份可序列化的管线描述
- `FramePipeline` 是**执行引擎** — 它读取 Asset，协调 GPU 资源、pass 执行、view 遍历

三者关系: **Builder（声明）→ Asset（数据）→ FramePipeline（执行）**

---

## 四个核心抽象

### 1. Mesh — 几何数据

```rust
/// 任意顶点格式的几何数据。
/// Sprite 是 Mesh::QUAD，3D 模型是 Mesh::from_gltf(...)。
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
    index_count: u32,
    vertex_layout: VertexLayout,
    sub_meshes: Vec<SubMesh>,     // 每个 sub-mesh 独立参与排序/合批
    bounding_sphere: BoundingSphere, // 用于 culling
}

/// Sub-mesh — 一个 Mesh 内共享同一个 material 的连续几何区段。
/// 每个 sub-mesh 在 Extract 阶段展开为独立的 PhaseItem，
/// 使得不同 entity 的相同 material sub-mesh 可以跨 entity 合批。
pub struct SubMesh {
    pub index_offset: u32,      // 在 index buffer 中的起始位置
    pub index_count: u32,       // 三角形索引数量
    pub vertex_offset: i32,     // base vertex offset
    pub material_index: u32,    // 在 MeshRenderer.materials[] 中的 slot
    pub bounding_sphere: BoundingSphere,
}

/// 顶点属性描述
pub struct VertexLayout {
    pub attributes: &'static [VertexAttribute],
    pub stride: u32,
}

pub struct VertexAttribute {
    pub semantic: VertexSemantic,
    pub format: wgpu::VertexFormat,
    pub offset: u32,
}

pub enum VertexSemantic {
    Position,    // vec3<f32>
    Normal,      // vec3<f32>
    Tangent,     // vec4<f32>
    UV0,         // vec2<f32>
    UV1,         // vec2<f32>
    Color,       // vec4<f32>
    Custom(u32), // 用户自定义
}

impl Mesh {
    /// 内置 quad mesh，sprite 用。每个 SpriteMaterial 实例共享这一个。
    pub const QUAD: MeshHandle = MeshHandle::BUILTIN_QUAD;

    /// 从 glTF 加载
    pub fn from_gltf(gpu: &GpuContext, path: &str) -> Result<Self, MeshError>;

    /// 从用户提供的顶点数据创建
    pub fn from_raw(gpu: &GpuContext, desc: MeshDescriptor) -> Self;
}
```

### 2. Material — Shader + 参数 + 渲染状态

```rust
/// 用户实现此 trait 来定义新的 material 类型。
/// 引擎通过 material 知道：用什么 shader、什么 blend mode、什么 bindings。
pub trait Material: Send + Sync + 'static {
    /// Shader 源码 (WGSL)
    fn shader_source(&self) -> ShaderSource;

    /// 顶点属性要求（必须和 Mesh 的 VertexLayout 兼容）
    fn vertex_layout(&self) -> &[VertexAttribute];

    /// Bind group layout — 声明 shader 需要的资源
    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout
    where Self: Sized;

    /// 创建当前参数的 bind group
    fn create_bind_group(&self, ctx: &MaterialBindContext) -> wgpu::BindGroup;

    /// 渲染状态（blend, depth, cull 等）
    fn render_state(&self) -> MaterialRenderState;

    /// Pipeline 缓存 key — 相同 key 的 material 实例共用一个 wgpu::RenderPipeline
    /// 通常基于 shader + vertex_layout + render_state 的 hash
    fn pipeline_key(&self) -> u64;

    /// Material 是透明的吗？决定放入 OpaquePhase 还是 TransparentPhase
    fn is_transparent(&self) -> bool {
        self.render_state().blend.is_some()
    }
}

/// 渲染状态描述
pub struct MaterialRenderState {
    pub blend: Option<wgpu::BlendState>,
    pub depth_write: bool,
    pub depth_compare: wgpu::CompareFunction,
    pub cull_mode: Option<wgpu::Face>,
    pub polygon_mode: wgpu::PolygonMode,
}

impl MaterialRenderState {
    pub fn opaque() -> Self { /* depth write, no blend, back-cull */ }
    pub fn transparent() -> Self { /* alpha blend, depth read only */ }
    pub fn additive() -> Self { /* additive blend, no depth write */ }
}
```

#### 内置 Material 实现

```rust
/// Sprite material — 纹理 + 颜色调制。SpriteRenderer 内部使用。
pub struct SpriteMaterial {
    pub color: Color,
    pub texture: Option<Texture>,
    pub uv_rect: [f32; 4],
}
// impl Material for SpriteMaterial { ... }
// 用户不需要直接用这个，SpriteRenderer 自动创建

/// 标准 PBR material — albedo, metallic, roughness, normal map
pub struct StandardMaterial {
    pub albedo: Color,
    pub albedo_texture: Option<Texture>,
    pub metallic: f32,
    pub roughness: f32,
    pub normal_texture: Option<Texture>,
    pub emissive: Color,
    pub emissive_texture: Option<Texture>,
    pub alpha_mode: AlphaMode,
}
// impl Material for StandardMaterial { ... }

/// 无光照 material — 纯颜色/纹理
pub struct UnlitMaterial {
    pub color: Color,
    pub texture: Option<Texture>,
}
// impl Material for UnlitMaterial { ... }
```

#### Material 存储与 Handle

```rust
/// Type-erased material handle。ECS 组件上只存这个。
/// 内部存储 type id + generation + index，可以路由到正确的 MaterialStorage<M>。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle {
    type_id: TypeId,        // 路由到哪个 MaterialStorage<M>
    index: u32,             // storage 内的 slot index
    generation: u32,        // 防止悬垂引用
}

/// Per-Material-type 的 typed storage。存在 Renderer 中，不在 ECS 中。
/// 每种注册过的 Material 类型有一个 MaterialStorage<M>。
pub struct MaterialStorage<M: Material> {
    materials: Vec<Option<M>>,
    generations: Vec<u32>,
    free_list: Vec<u32>,
}

impl<M: Material> MaterialStorage<M> {
    /// 插入 material 实例，返回 handle
    pub fn insert(&mut self, material: M) -> MaterialHandle;

    /// 通过 handle 获取 material 引用
    pub fn get(&self, handle: MaterialHandle) -> Option<&M>;

    /// 通过 handle 获取 material 可变引用
    pub fn get_mut(&mut self, handle: MaterialHandle) -> Option<&mut M>;

    /// 移除
    pub fn remove(&mut self, handle: MaterialHandle) -> Option<M>;
}
```

#### Material 注册 — 一行搞定

```rust
impl Renderer {
    /// 注册一种 Material 类型。一行调用，三件事全做完:
    ///   1. 创建 MaterialStorage<M>
    ///   2. 注册 DrawMesh<M> 到 DrawFunctionRegistry
    ///   3. 注册 extract_meshes::<M>() 到 Extract Schedule
    ///   4. 注册 M::bind_group_layout 到 PipelineCache
    pub fn register_material<M: Material>(&mut self) {
        // 创建 typed storage
        self.material_storages.insert(TypeId::of::<M>(), Box::new(MaterialStorage::<M>::new()));

        // 注册 draw function
        let draw_fn_id = self.draw_functions.register(DrawMesh::<M>::new());

        // 注册 extract 逻辑到 schedule
        self.extract_schedule.add(ExtractMeshes::<M>::new(draw_fn_id));

        // 缓存 bind group layout
        self.pipeline_cache.register_layout::<M>(self.device());
    }

    /// 获取某种 Material 的 typed storage
    pub fn materials<M: Material>(&self) -> &MaterialStorage<M>;
    pub fn materials_mut<M: Material>(&mut self) -> &mut MaterialStorage<M>;
}
```

**内置 Material 在引擎初始化时自动注册:**

```rust
// 引擎内部，用户不需要调用
renderer.register_material::<SpriteMaterial>();
renderer.register_material::<StandardMaterial>();
renderer.register_material::<UnlitMaterial>();
```

#### Pipeline Cache — 自动管理 + GC

```rust
/// 引擎自动管理 wgpu::RenderPipeline 的创建、缓存和回收。
/// 用户永远不需要手动创建 pipeline。
///
/// GC 策略: 每个缓存条目记录最后使用帧号。
/// 连续 N 帧未使用的 pipeline 在 garbage_collect() 时被释放。
/// 避免 Material 类型注销后 pipeline 泄漏。
struct PipelineCache {
    /// pipeline_key → (compiled pipeline + 最后使用帧号)
    cache: FxHashMap<u64, CachedPipeline>,
    /// TypeId → BindGroupLayout (per material type)
    layouts: FxHashMap<TypeId, wgpu::BindGroupLayout>,
    /// 当前帧号
    current_frame: u64,
}

struct CachedPipeline {
    pipeline: wgpu::RenderPipeline,
    last_used_frame: u64,
}

/// 未使用多少帧后回收 pipeline（默认 300 帧 ≈ 5 秒 @60fps）
const PIPELINE_TTL_FRAMES: u64 = 300;

impl PipelineCache {
    /// 获取或创建 pipeline（同时更新 last_used_frame）
    fn get_or_create<M: Material>(
        &mut self,
        device: &wgpu::Device,
        material: &M,
        target_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        let key = material.pipeline_key();
        let entry = self.cache.entry(key).or_insert_with(|| {
            CachedPipeline {
                pipeline: Self::compile_pipeline::<M>(device, material, ...),
                last_used_frame: self.current_frame,
            }
        });
        entry.last_used_frame = self.current_frame;
        &entry.pipeline
    }

    /// 每帧调用，推进帧计数器
    fn new_frame(&mut self) {
        self.current_frame += 1;
    }

    /// 回收长期未使用的 pipeline（建议每 N 帧调用一次，不必每帧）
    fn garbage_collect(&mut self) {
        self.cache.retain(|_, cached| {
            self.current_frame - cached.last_used_frame < PIPELINE_TTL_FRAMES
        });
    }
}
```

### 3. Render Phase — 排序 + 分发

```rust
/// Phase Item — 排序后等待执行的渲染项。
/// 每个 sub-mesh 展开为一个独立的 PhaseItem，不同 entity 的相同 material
/// sub-mesh 可以在排序后自然合批。
pub struct PhaseItem {
    pub sort_key: u64,
    pub draw_function_id: DrawFunctionId,
    pub entity: EntityId,           // for ECS lookup
    pub mesh_handle: MeshHandle,
    pub sub_mesh_index: u32,        // Mesh 内的 sub-mesh 索引
    pub material_handle: MaterialHandle,  // type-erased，DrawFunction 内部还原
    pub batch_key: u64,             // for auto-batching
}

/// Opaque Phase: front-to-back, depth write
pub struct OpaquePhase {
    items: Vec<PhaseItem>,
}

/// Transparent Phase: back-to-front, alpha blend
pub struct TransparentPhase {
    items: Vec<PhaseItem>,
}

/// Phase trait — 用户可以实现自定义 phase
pub trait RenderPhase: Send + 'static {
    fn name(&self) -> &'static str;

    /// 接收 PhaseItem
    fn add_item(&mut self, item: PhaseItem);

    /// 排序
    fn sort(&mut self);

    /// 执行（使用注册的 DrawFunction）
    fn render(
        &self,
        encoder: &mut wgpu::RenderPass,
        draw_functions: &DrawFunctionRegistry,
        world: &World,
        gpu_scene: &GpuScene,  // 通过 gpu_scene.table::<T>() 访问具体表
    );

    /// 深度/blend 配置
    fn depth_stencil_state(&self) -> Option<wgpu::DepthStencilState>;

    /// 清除状态，准备下一帧
    fn clear(&mut self);
}
```

### 4. DrawFunction — 可扩展的绘制方式

DrawFunction 分两类：

| 类型 | 描述 | 例子 |
|---|---|---|
| **通用 DrawFunction** | 走 Mesh + Material 系统，由 `register_material` 自动注册 | `DrawMesh<SpriteMaterial>`, `DrawMesh<StandardMaterial>` |
| **不透明 DrawFunction** | 内部自管渲染循环，不走 Mesh/Material | `DrawLive2D` (内部管 drawable 排序、clipping、state 切换) |

```rust
/// 注册一种"怎么画"的方式。
/// 引擎内置 DrawMesh<M: Material>，用户可以注册新的。
pub trait DrawFunction: Send + Sync + 'static {
    fn draw(
        &self,
        pass: &mut wgpu::RenderPass,
        item: &PhaseItem,
        world: &World,
        gpu_scene: &GpuScene,  // 通过 gpu_scene.table::<T>() 访问需要的表
        pipeline_cache: &PipelineCache,
    );
}

/// 通用 DrawFunction: mesh + material 绘制
/// 由 register_material::<M>() 自动注册，用户不需要手动创建
pub struct DrawMesh<M: Material> {
    _phantom: PhantomData<M>,
}

impl<M: Material> DrawFunction for DrawMesh<M> {
    fn draw(&self, pass: &mut wgpu::RenderPass, item: &PhaseItem, ...) {
        let mesh = renderer.meshes().get(item.mesh_handle).unwrap();
        let sub = &mesh.sub_meshes[item.sub_mesh_index as usize];
        // 通过 type-erased handle 从 MaterialStorage<M> 取回 typed material
        let material = renderer.materials::<M>().get(item.material_handle).unwrap();
        let pipeline = pipeline_cache.get_or_create(material, ...);

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &gpu_scene.view_bind_group(), &[]);
        pass.set_bind_group(1, &material.create_bind_group(ctx), &[]);
        pass.set_bind_group(2, gpu_scene.table::<ModelMatrixTable>().bind_group(), &[]);
        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        if let Some(ib) = &mesh.index_buffer {
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            // 只绘制当前 sub-mesh 的区段
            pass.draw_indexed(sub.index_offset..(sub.index_offset + sub.index_count),
                              sub.vertex_offset, instance_range);
        } else {
            pass.draw(0..mesh.vertex_count, instance_range);
        }
    }
}

/// 不透明 DrawFunction: Live2D
/// 在 Phase 排序层面占一个 slot（整个模型一个 sort key），
/// 内部自管 drawable 排序、clipping mask、state 切换。
/// 不走 Mesh/Material 系统。
pub struct DrawLive2D;
impl DrawFunction for DrawLive2D {
    fn draw(&self, pass: &mut wgpu::RenderPass, item: &PhaseItem, ...) {
        // 从 World 取 Live2D model instance
        // 内部迭代 drawables，管理自己的 vertex buffer、clipping mask、blend state
        // 对 Phase 来说这是一个原子操作
    }
}

/// DrawFunction 注册表 — 运行时注册
pub struct DrawFunctionRegistry {
    functions: Vec<Box<dyn DrawFunction>>,
}

impl DrawFunctionRegistry {
    /// 注册一个新的绘制方式，返回 ID
    pub fn register<F: DrawFunction>(&mut self, func: F) -> DrawFunctionId;

    /// 获取
    pub fn get(&self, id: DrawFunctionId) -> &dyn DrawFunction;
}
```

---

## ECS 组件设计

```rust
// === 用户面对的组件 (不变或小改) ===

pub struct Transform { ... }          // 已有，3D ready ✅
pub struct Camera { ... }             // 已有 ✅
pub struct Projection { ... }         // 已有, Ortho + Perspective ✅
pub struct SortingLayer(pub i32);     // 已有 ✅
pub struct OrderInLayer(pub i32);     // 已有 ✅
pub struct RenderLayerMask(pub u32);  // 已有 ✅

// Sprite (高级封装，内部自动创建 SpriteMaterial)
pub struct SpriteRenderer { ... }     // 已有，API 不变 ✅

// 通用 Mesh Renderer (非泛型！所有 material 类型共享同一个 archetype)
pub struct MeshRenderer {
    pub mesh: MeshHandle,
    /// Per-sub-mesh material 列表。
    /// materials[i] 对应 mesh.sub_meshes[i].material_index。
    /// 对于单 material mesh (如 sprite 的 QUAD)，只有一个元素。
    pub materials: Vec<MaterialHandle>,
    pub visible: bool,
}

// 灯光
pub struct PointLight { ... }         // 从 PointLight2D 重命名，加 z 位置
pub struct DirectionalLight { ... }   // 新增
```

**`MeshRenderer` 为什么不是泛型？**

`MeshRenderer<SpriteMaterial>` 和 `MeshRenderer<StandardMaterial>` 会是不同的 ECS 组件类型，
在 archetype-based ECS 中会产生不同的 archetype。这导致：
- 用户每注册一个新 Material → 一个新的 archetype 维度
- 无法写「查询所有 MeshRenderer，不管什么 Material」的通用查询
- 给编辑器、调试、visibility culling 带来不必要的复杂度

Handle 模式让 `MeshRenderer` 成为单一组件类型：
- 所有 mesh entity 在同一个 archetype 家族中
- Extract 可以用一个查询覆盖所有 mesh entity
- 通过 `MaterialHandle.type_id` 路由到对应的 DrawFunction

### SpriteRenderer → MeshRenderer 的内部映射

```rust
/// 引擎内部：提取 SpriteRenderer 时自动转为 PhaseItem
/// SpriteRenderer 是 MeshRenderer 的语法糖，用户不需要知道 Material 的存在
fn extract_sprites(
    world: &World,
    phase: &mut TransparentPhase,
    registry: &DrawFunctionRegistry,
    sprite_materials: &mut MaterialStorage<SpriteMaterial>,
) {
    let draw_fn_id = registry.id::<DrawMesh<SpriteMaterial>>();

    world.query::<(&Transform, &SpriteRenderer, Option<&SortingLayer>, Option<&OrderInLayer>)>()
        .for_each_with_entity(|entity, (transform, sprite, layer, order)| {
            if !sprite.visible { return; }

            // 从 SpriteRenderer 字段自动创建/更新 SpriteMaterial
            let mat_handle = sprite_materials.get_or_create(entity, SpriteMaterial {
                color: sprite.color,
                texture: sprite.texture.clone(),
                uv_rect: sprite.uv,
            });

            let sort_key = encode_sort_key(layer, order, sprite.texture_key(), transform.z());

            phase.add_item(PhaseItem {
                sort_key,
                draw_function_id: draw_fn_id,
                entity,
                mesh_handle: MeshHandle::BUILTIN_QUAD,
                material_handle: mat_handle,
                batch_key: sprite.batch_key(), // pipeline + texture
            });
        });
}
```

**用户写 `SpriteRenderer` 和以前一样简单，但引擎内部已经走通用管线。**

---

## Extract 注册机制

Extract 不是硬编码的函数列表，而是一个**可扩展的 schedule**：

```rust
/// Extract Schedule — 管理所有 extract 函数的执行
pub struct ExtractSchedule {
    extractors: Vec<Box<dyn Extractor>>,
}

/// Extractor trait — 从 ECS World → Phase Items
trait Extractor: Send + 'static {
    fn extract(
        &mut self,
        world: &World,
        phases: &mut PhaseSet,
        gpu_scene: &mut GpuScene,  // 通过 gpu_scene.table_mut::<T>() 写入具体表
        renderer: &Renderer,
    );
}

/// 通用 mesh extractor，由 register_material::<M>() 自动创建
struct ExtractMeshes<M: Material> {
    draw_fn_id: DrawFunctionId,
    _phantom: PhantomData<M>,
}

impl<M: Material> Extractor for ExtractMeshes<M> {
    fn extract(&mut self, world: &World, phases: &mut PhaseSet, ...) {
        world.query::<(&Transform, &MeshRenderer)>()
            .for_each_with_entity(|entity, (transform, mesh_renderer)| {
                if !mesh_renderer.visible { return; }

                let mesh = renderer.meshes().get(mesh_renderer.mesh).unwrap();

                // 每个 sub-mesh 展开为独立的 PhaseItem
                for (sub_idx, sub) in mesh.sub_meshes.iter().enumerate() {
                    let mat_handle = mesh_renderer.materials[sub.material_index as usize];
                    if mat_handle.type_id != TypeId::of::<M>() { continue; }

                    let material = renderer.materials::<M>().get(mat_handle).unwrap();
                    let phase = if material.is_transparent() {
                        phases.transparent_mut()
                    } else {
                        phases.opaque_mut()
                    };

                    phase.add_item(PhaseItem {
                        sort_key: compute_sort_key(transform, material),
                        draw_function_id: self.draw_fn_id,
                        entity,
                        mesh_handle: mesh_renderer.mesh,
                        sub_mesh_index: sub_idx as u32,
                        material_handle: mat_handle,
                        batch_key: material.pipeline_key(),
                    });
                }
            });
    }
}
```

**用户新增 Material 后，不需要手动写 extract、不需要修改 frame loop — `register_material` 全部搞定。**

---

## 帧执行流程

```
每帧:

1. EXTRACT (由 ExtractSchedule 驱动，可扩展)
   ├─ ExtractSprites          → SceneCache / TransparentPhaseInput
   ├─ ExtractMeshes<StdMat>   → SceneCache / Opaque+TransparentPhaseInput
   ├─ ExtractMeshes<Unlit>    → SceneCache / Opaque+TransparentPhaseInput
   ├─ ExtractMeshes<Custom>   → SceneCache / Opaque+TransparentPhaseInput
   ├─ ExtractLive2D           → Live2DPhaseInput
   └─ ExtractLights           → SceneCache / GpuScene light source

2. PREPARE (CPU, per frame / per view)
   ├─ build PreparedFrame / PreparedView payloads
   ├─ cull visible items per view
   ├─ opaque_phase.sort()        // front-to-back by depth
   ├─ transparent_phase.sort()   // back-to-front by (layer, order, depth)
   └─ build draw spans / texture tables / phase items

3. GPU UPLOAD (dirty tracking only)
   └─ gpu_scene.upload_all(queue)   // 遍历所有注册的 GpuTable，各自 upload 脏数据
   (Material 参数不在 GpuScene 中；mesh/material lookup 也不在这里)

4. RENDER (FramePipeline 执行 RenderPipelineAsset)
    ┌── RenderGraph pass: "Opaque" ──────────────────────────┐
    │ for item in opaque_phase.items:                        │
    │   draw_functions.get(item.draw_fn_id).draw(pass, item) │
   │   (auto-batch: skip set_pipeline if same as last)      │
   └────────────────────────────────────────────────────────┘
   ┌── RenderGraph pass: "Transparent" ─────────────────────┐
   │ for item in transparent_phase.items:                    │
   │   draw_functions.get(item.draw_fn_id).draw(pass, item) │
   └────────────────────────────────────────────────────────┘
   ┌── User passes (e.g., OutlinePass) ────────────────────┐
   │ user_pass.execute(ctx)                                 │
   └────────────────────────────────────────────────────────┘
   ┌── PostFx (保留现有) ──────────────────────────────────┐
   │ Bloom → ToneMap → Vignette                             │
   └────────────────────────────────────────────────────────┘

5. PRESENT
```

---

## 声明式管线 — Builder / Asset / FramePipeline 的关系

```
用户代码                       引擎内部
────────────                   ────────────
RenderPipelineBuilder          
  .add_phase::<Opaque>()       
  .add_phase::<Transparent>()  
  .add_pass(OutlinePass)       
  .add_postfx(Bloom)           
  .build()                     
     │                         
     ▼                         
RenderPipelineAsset ──────────→ FramePipeline::from_asset()
  (声明式数据,                     │
   可序列化,                       ▼
   可热重载)                   FramePipeline (执行引擎)
                                 ├─ 协调 RenderGraph pass 创建
                                 ├─ 管理 GPU 资源分配
                                 ├─ 驱动 per-view 遍历
                                 └─ 调用 Phase::render() / Pass::execute()
```

- **Builder** — 用户的声明式接口
- **Asset** — build() 的产物，纯数据，引擎执行时读取
- **FramePipeline** — runtime 执行者，分配 GPU 资源，驱动渲染循环
- **RenderGraph** — FramePipeline 内部使用，管理 pass 之间的资源依赖

```rust
/// 用户通过 Builder 定义帧渲染管线
pub struct RenderPipelineBuilder {
    phases: Vec<Box<dyn RenderPhase>>,
    custom_passes: Vec<Box<dyn RenderPass>>,
    postfx: Vec<Box<dyn PostFxPass>>,
    output_chain: OutputChainConfig,
}

impl RenderPipelineBuilder {
    pub fn new() -> Self;

    /// 添加一个排序+绘制 phase (Opaque, Transparent, etc.)
    pub fn add_phase<P: RenderPhase>(mut self, phase: P) -> Self;

    /// 添加用户自定义 pass (outline, shadow, etc.)
    pub fn add_pass<P: RenderPass>(mut self, pass: P) -> Self;

    /// 添加后处理
    pub fn add_postfx<F: PostFxPass>(mut self, fx: F) -> Self;

    /// 编译为 Asset
    pub fn build(self) -> RenderPipelineAsset;
}

/// 用户自定义 Render Pass
pub trait RenderPass: Send + 'static {
    fn name(&self) -> &'static str;

    /// 声明需要的 render targets, depth buffer 等
    fn setup(&mut self, ctx: &mut RenderPassSetupContext);

    /// 执行渲染
    fn execute(&mut self, ctx: &mut RenderPassExecuteContext) -> Result<(), RenderError>;
}

/// 内置预设管线
impl RenderPipelineAsset {
    /// 2D sprite 游戏 (和现在行为一致)
    pub fn forward_2d() -> Self {
        Self::builder()
            .add_phase(TransparentPhase::new())
            .add_postfx(Bloom::default())
            .add_postfx(ToneMap::default())
            .build()
    }

    /// 通用 3D forward rendering
    pub fn forward_3d() -> Self {
        Self::builder()
            .add_phase(OpaquePhase::new())
            .add_phase(TransparentPhase::new())
            .add_postfx(Bloom::default())
            .add_postfx(ToneMap::default())
            .build()
    }
}
```

---

## GpuTableManager + GpuScene — 通用 GPU 表系统

### GpuTable — 通用 GPU Buffer 抽象

```rust
/// 任何需要跨 pass 共享的 GPU 数据都实现此 trait。
/// 每种数据类型是一个独立的 GpuTable，自带 layout、upload、bind group 逻辑。
pub trait GpuTable: Send + Sync + 'static {
    /// 表名称（调试用）
    fn name(&self) -> &'static str;

    /// 上传脏数据到 GPU
    fn upload(&mut self, queue: &wgpu::Queue);

    /// 获取此表的 bind group（DrawFunction 按需取用）
    fn bind_group(&self) -> &wgpu::BindGroup;

    /// 获取此表的 bind group layout（PipelineCache 需要）
    fn bind_group_layout(&self) -> &wgpu::BindGroupLayout;
}
```

### GpuTableManager — 通用注册表

```rust
/// 管理所有注册的 GpuTable。
/// 添加新的共享 GPU 数据类型时，只需注册新 table，不需要修改任何现有代码。
pub struct GpuTableManager {
    tables: TypeMap,  // TypeId → Box<dyn GpuTable>
}

impl GpuTableManager {
    /// 注册一种共享 GPU 表
    pub fn register<T: GpuTable>(&mut self, table: T);

    /// 获取某种表的引用
    pub fn table<T: GpuTable>(&self) -> &T;

    /// 获取某种表的可变引用
    pub fn table_mut<T: GpuTable>(&mut self) -> &mut T;

    /// 统一上传所有脏数据
    pub fn upload_all(&mut self, queue: &wgpu::Queue) {
        // 遍历所有注册的 table，各自 upload
        for table in self.tables.values_mut() {
            table.upload(queue);
        }
    }
}
```

### 内置 GpuTable 实现

```rust
/// 所有可渲染实体的 4x4 model matrix (per-entity slot)
pub struct ModelMatrixTable {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    slot_allocator: SlotAllocator,
    dirty_slots: Vec<u32>,
    upload_scratch: Vec<[f32; 16]>,
}
impl GpuTable for ModelMatrixTable { ... }

impl ModelMatrixTable {
    /// 分配一个 slot，返回 slot index
    pub fn alloc_slot(&mut self) -> u32;
    /// 释放 slot
    pub fn free_slot(&mut self, slot: u32);
    /// 写入 model matrix 并标脏
    pub fn set(&mut self, slot: u32, matrix: [f32; 16]);
}

/// 全局光源数组
pub struct LightTable {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    lights: Vec<GpuLight>,
    dirty: bool,
}
impl GpuTable for LightTable { ... }

impl LightTable {
    pub fn set_lights(&mut self, lights: &[GpuLight]);
}
```

### GpuScene — GpuTableManager 的消费者

```rust
/// GPU 侧共享场景数据的入口。
/// 持有 GpuTableManager + 唯一真正全局的 view uniforms。
/// 不硬编码任何具体表类型 — 所有表通过 table_manager 注册和访问。
pub struct GpuScene {
    /// 通用表注册表 — model matrices, lights 等都在这里
    table_manager: GpuTableManager,

    /// Per-view uniforms (view_proj matrix, camera position, viewport size, time)
    /// 这是唯一硬编码的数据，因为它是渲染基础设施，任何 shader 都需要
    view_uniforms: wgpu::Buffer,
    view_bind_group: wgpu::BindGroup,
}

impl GpuScene {
    /// 初始化 — 注册内置表
    pub fn new(device: &wgpu::Device) -> Self {
        let mut scene = Self { ... };
        scene.register(ModelMatrixTable::new(device));
        scene.register(LightTable::new(device));
        scene
    }

    /// 注册新的共享 GPU 表（用户可扩展）
    pub fn register<T: GpuTable>(&mut self, table: T) {
        self.table_manager.register(table);
    }

    /// 获取某种表
    pub fn table<T: GpuTable>(&self) -> &T {
        self.table_manager.table::<T>()
    }

    /// 获取某种表（可变）
    pub fn table_mut<T: GpuTable>(&mut self) -> &mut T {
        self.table_manager.table_mut::<T>()
    }

    /// 统一上传所有脏数据
    pub fn upload_all(&mut self, queue: &wgpu::Queue) {
        self.upload_view_uniforms(queue);
        self.table_manager.upload_all(queue);
    }

    /// View uniforms bind group (所有 DrawFunction 共用 group 0)
    pub fn view_bind_group(&self) -> &wgpu::BindGroup {
        &self.view_bind_group
    }
}
```

**扩展示例 — 添加骨骼动画，GpuScene 代码零修改：**

```rust
pub struct BoneMatrixTable {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    // ...
}
impl GpuTable for BoneMatrixTable { ... }

// 注册
gpu_scene.register(BoneMatrixTable::new(device));

// DrawFunction 里使用
pass.set_bind_group(3, gpu_scene.table::<BoneMatrixTable>().bind_group(), &[]);
```

**不属于 `GpuScene` 的内容:**
- mesh / material / texture 资源查找
- `PreparedView`、draw spans、visible lists、phase items
- sprite/live2d 等 domain-specific 的排序和批处理结果
- ECS extract cache (`SceneCache2D` 这类 CPU 侧缓存)

**Bind Group 约定:**
- `group(0)` — view uniforms (camera, viewport, time) — 全帧共用，GpuScene 硬编码
- `group(1)` — material params — 各 Material 自己创建
- `group(2)` — model transform — `gpu_scene.table::<ModelMatrixTable>()`
- `group(3)` — reserved for custom tables (bones, particles, etc.)

**三层拆分:**
- `SceneCache*` / extracted scene: CPU 侧 extract cache，负责从 ECS 拉平和保留脏标记来源
- `PreparedFrame` / `PreparedView` / phase items: 每帧 prepare 结果，负责可见性、排序、draw spans、纹理表
- `GpuScene` + `GpuTableManager`: GPU 上传层，通用表注册 + view uniforms

---

## 与现有代码的关系

### 保留（重构后继续使用）

| 模块 | 角色变化 |
|---|---|
| `RenderGraph` | 不变 — 继续管理 pass 资源依赖 |
| `FramePipeline` / `FrameViewNode` | 角色明确: 执行 RenderPipelineAsset 产出的管线配置 |
| `PreparedFrame` / `PreparedView` | 不变 — payload 系统继续传递 GpuScene + per-view prepared 数据 |
| `TextureAtlas` | 不变 — SpriteMaterial 内部使用 |
| `PostFx` (Bloom, ToneMap, Vignette) | 不变 — 实现新 `PostFxPass` trait |
| `Transform`, `Camera`, `Projection` | 不变 — 已经 3D ready |
| `SpriteRenderer` 组件 | 不变 — 内部由 ExtractSprites 映射到 PhaseItem |

### 替换

| 旧 | 新 | 原因 |
|---|---|---|
| `RenderDomain` trait | `RenderPhase` trait + `DrawFunction` trait | Domain 是封闭的; Phase + DrawFunction 是开放的 |
| `SpriteDomain` | `DrawMesh<SpriteMaterial>` + `ExtractSprites` | Sprite 不再是特殊 domain |
| `Live2DDomain` | `DrawLive2D` (不透明 DrawFunction) | Live2D 是一种绘制方式，不是一个 domain |
| `GpuScene2D` | `GpuScene` + `GpuTableManager` | 通用表注册 + view uniforms，不再硬编码具体数据类型 |
| `PreparedView2D` + `draw_spans` + `textures` | `PreparedFrame` / `PreparedView` / phase payloads | 这些属于 prepare 层，不属于 GpuScene |
| `SceneCache2D` | 保留为 extract/cache 层，后续可泛化命名 | 它是 CPU 侧缓存，不应合并进 GpuScene |
| `SpriteBatch` | Phase 内部的 auto-batch | 排序后扫描连续相同 pipeline+binding 自动合并 |
| `SceneExtractor` | `ExtractSchedule` (注册式) | 每种 Material 类型有自己的 extractor，自动注册 |
| `RenderPipelineBuilder` | **重构** — 从 stage/queue/domain 模型改为 phase/pass/postfx 模型，输出 Asset → FramePipeline |

### 新增

| 模块 | 描述 |
|---|---|
| `src/render/mesh/` | Mesh, VertexLayout, SubMesh, MeshHandle, gltf 加载 |
| `src/render/material/mod.rs` | Material trait, MaterialHandle, MaterialRenderState |
| `src/render/material/storage.rs` | MaterialStorage\<M\>, 注册机制 |
| `src/render/material/pipeline_cache.rs` | PipelineCache (key → wgpu::RenderPipeline) |
| `src/render/material/sprite.rs` | SpriteMaterial 实现 |
| `src/render/material/standard.rs` | StandardMaterial (PBR) 实现 |
| `src/render/material/unlit.rs` | UnlitMaterial 实现 |
| `src/render/phase/mod.rs` | RenderPhase trait, OpaquePhase, TransparentPhase |
| `src/render/phase/draw.rs` | DrawFunction trait, DrawFunctionRegistry, DrawMesh\<M\> |
| `src/render/phase/sort_key.rs` | Sort key encoding |
| `src/render/extract.rs` | ExtractSchedule, Extractor trait, ExtractMeshes\<M\> |
| `src/render/gpu_table/mod.rs` | GpuTable trait, GpuTableManager, TypeMap 注册机制 |
| `src/render/gpu_table/model_matrix.rs` | ModelMatrixTable (内置 GpuTable 实现) |
| `src/render/gpu_table/light.rs` | LightTable (内置 GpuTable 实现) |
| `src/render/gpu_scene.rs` | GpuScene (GpuTableManager 消费者 + view uniforms) |
| `src/render/ecs.rs` += | MeshRenderer (非泛型), PointLight (3D), DirectionalLight |

---

## 迁移路径（7 步）

### Step 1: Mesh 系统
- 定义 `Mesh`, `VertexLayout`, `MeshHandle`
- 实现 `Mesh::QUAD` (内置 quad)
- 暂时不加 gltf
- **验证**: 编译通过

### Step 2: Material trait + Storage + SpriteMaterial
- 定义 `Material` trait
- 实现 `MaterialHandle`, `MaterialStorage<M>`
- 实现 `SpriteMaterial`（从现有 shader 提取）
- 实现 `PipelineCache`
- 实现 `register_material::<M>()` 注册机制
- **验证**: SpriteMaterial 能创建 pipeline，register 流程跑通

### Step 3: DrawFunction + Phase + ExtractSchedule
- 定义 `DrawFunction` trait
- 实现 `DrawMesh<SpriteMaterial>`
- 定义 `TransparentPhase`
- 实现 `ExtractSchedule` + `ExtractSprites`
- **验证**: sprite 通过新管线渲染，视觉一致

### Step 4: 替换 SpriteDomain
- Extract 从 SpriteRenderer → PhaseItem
- Phase sort + DrawFunction dispatch 替代旧的 SpriteSceneNode
- `MeshRenderer` (非泛型，Handle-based) 组件
- 保留旧代码作为 fallback
- **验证**: 所有 sprite examples 正常

### Step 5: OpaquePhase + StandardMaterial + Mesh 加载
- 添加 `OpaquePhase`
- 实现 `StandardMaterial`
- 添加 `SubMesh` 结构，`Mesh::from_gltf()` 按 primitive 生成 sub-meshes
- `MeshRenderer.materials: Vec<MaterialHandle>` 支持 per-sub-mesh material
- `ExtractMeshes` 展开 sub-mesh 为独立 PhaseItem（跨 entity 合批）
- **验证**: 能渲染一个多 material 的 glTF 模型

### Step 6: Live2D 迁移
- `Live2DDomain` → `DrawLive2D` (不透明 DrawFunction)
- Live2D 模型整体作为一个 PhaseItem 进入 TransparentPhase
- 内部 drawable 排序/clipping 由 DrawLive2D 自管
- **验证**: Live2D + sprite 在同一场景正确渲染

### Step 7: 用户自定义 + 清理
- 完善 RenderPipelineBuilder → Asset → FramePipeline 链路
- 删除旧的 Domain/SpriteDomain 代码
- 写文档和 examples
- **验证**: example 展示自定义 Material (一行 register，即可渲染)

---

## Sort Key 设计（TransparentPhase）

```
63        56 55       48 47        36 35           0
┌───────────┬───────────┬────────────┬──────────────┐
│ layer (8) │ order (8) │ batch (12) │ depth (36)   │
└───────────┴───────────┴────────────┴──────────────┘

batch = draw_function_id:4 | material_pipeline_key:8
depth = float_to_u36(!view_depth)  // back-to-front
```

## Sort Key 设计（OpaquePhase）

```
63        52 51                36 35               0
┌───────────┬──────────────────┬──────────────────┐
│ batch(12) │ depth(16)        │ entity_key(36)   │
└───────────┴──────────────────┘──────────────────┘

batch = draw_function_id:4 | material_pipeline_key:8
depth = float_to_u16(view_depth)  // front-to-back (最小化 overdraw)
```

---

## 关键设计决策总结

| 问题 | 决策 | 理由 |
|---|---|---|
| 扩展方式 | `DrawFunction` trait (开放注册) | enum 不可扩展；dyn trait 可以 |
| Pipeline 管理 | `PipelineCache` 自动缓存 | 用户不需要知道 wgpu pipeline 的存在 |
| Material 定义 | 用户 `impl Material` | 完全控制 shader 和 bindings |
| Material 存储 | `MaterialStorage<M>` + `MaterialHandle` | 非泛型 ECS 组件，避免 archetype 爆炸 |
| Material 参数 GPU | 各 Material 自己 `create_bind_group()` | 不同 Material layout 完全不同，不可能统一 buffer |
| Material 注册 | `register_material::<M>()` 一行搞定 | 自动注册 DrawFunction + Extract + Layout |
| Sprite 是什么 | `Mesh::QUAD + SpriteMaterial` | 不是特殊类型，是通用系统的一个实例化 |
| 2D 是什么 | 正交相机 | 不是特殊渲染路径 |
| Phase 数量 | 2 个内置 (Opaque + Transparent) | 用户可以添加自定义 phase |
| Extract 驱动 | `ExtractSchedule` 注册式 | 用户新增 Material 不需要改 frame loop |
| 排序策略 | Sort-key (u64) | 内联排序，无 allocation |
| Batch 策略 | 排序后扫描连续相同 batch_key | 自然 batch，不需要显式管理 |
| Sub-mesh 策略 | 展开为独立 PhaseItem (方案 B) | 跨 entity 合批；100 个角色的相同 material sub-mesh 可合并 |
| Live2D | 不透明 DrawFunction | 一个 sort key，内部自管 drawable/clipping |
| Builder/Pipeline 关系 | Builder（声明）→ Asset（数据）→ FramePipeline（执行） | 三层职责清晰 |
| GpuScene 职责 | GpuTableManager 消费者 + view uniforms | 不硬编码具体表类型，新增数据类型零修改 GpuScene |
