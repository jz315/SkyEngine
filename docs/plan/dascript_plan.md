# SkyEngine Mod System — daScript 集成方案

## 为什么选 daScript

### 设计约束

SkyEngine 的 mod 系统需要满足：

1. **性能** — mod 逻辑必须接近 native 速度，不能浪费 ECS 的 chunk/columnar 优化
2. **Hook 能力** — mod 必须能拦截、替换、扩展引擎已有的 System
3. **完整编程能力** — 支持大型 mod（暮色森林、工业、枪械级别）
4. **易用性** — mod 作者的开发体验必须流畅

### 方案评估

| 方案 | 性能 | Hook | 大型 mod 能力 | Rust 集成 |
|---|---|---|---|---|
| JSON 数据驱动 | N/A | ❌ | ❌ 表达力不够 | ✅ |
| Lua / Rhai | 差 (native 10-30%) | ✅ | ✅ | ✅ mlua 成熟 |
| LuaJIT + FFI | 中 (native 50-80%) | ✅ | ✅ | ✅ mlua 成熟 |
| WASM (wasmtime) | 好 (native 70-95%) | ✅ | ✅ | ✅ wasmtime 成熟 |
| **daScript AOT/JIT** | **极好 (≈ native)** | ✅ | ✅ | ⚠️ 需 C FFI 桥接 |

### 决定性优势

- **AOT 编译后接近 native C++**，JIT 模式甚至超过 C++（LLVM 后端）
- **静态强类型** — mod 中的 bug 在编译期捕获，不是运行时崩溃
- **热重载** — 修改脚本即时生效，改过的函数回退解释器，未改的保持 AOT
- **ECS 原生设计** — daScript 诞生于 Gaijin（War Thunder），为 ECS 数据流优化
- **无 GC** — context-reset 内存管理，无垃圾回收停顿
- **SkyEngine 的 type-erased ECS 消除了互操作瓶颈** — chunk column 就是裸字节，任何能操作指针的语言都能零拷贝访问

### C FFI 集成不是障碍

daScript 提供 `c_api.h` 给非 C++ 宿主。SkyEngine 的 ECS 底层是 type-erased（`reflect::register(name, size, align)` + 裸 `*mut u8` 列指针），不需要暴露 Rust 泛型。绑定面其实很薄。

加上 proc-macro 自动生成绑定代码，工程量可控。

---

## 架构总览

```
┌─────────────────────────────────────────────────────┐
│  Mod 代码 (.das 文件)                                │
│  struct Poison { dps:float; duration:float }         │
│  def apply_poison(...)  ...                          │
│  [hook] def on_death(...)  ...                       │
├─────────────────────────────────────────────────────┤
│  sky_mod.das (自动生成的引擎 API 模块)               │
│  Vec3, Color, Transform, Camera, World, Query, ...  │
├─────────────────────────────────────────────────────┤
│  daScript Runtime (解释器 / AOT / JIT)               │
│  ├─ 开发: 解释器 (热重载)                             │
│  ├─ 发布: AOT 编译 → C → 机器码                      │
│  └─ 可选: JIT (LLVM, 运行时编译)                     │
├─────────────────────────────────────────────────────┤
│  C FFI 层 (#[das_bind] proc-macro 自动生成)          │
│  extern "C" fn das_Vec3_length(...)                  │
│  extern "C" fn das_World_spawn(...)                  │
├─────────────────────────────────────────────────────┤
│  SkyEngine (Rust)                                    │
│  ECS · Render · Audio · Input · Physics · ...        │
└─────────────────────────────────────────────────────┘
```

---

## Mod 作者视角

### 简单 mod — 加一个新效果

```python
require sky_mod       # 引擎 API
require sky_ecs       # ECS 操作

# 定义新组件（引擎自动注册到 ECS）
struct Poison
    dps : float
    duration : float

# 定义 system
[system(group="combat")]
def apply_poison(var poison : Poison; var health : Health; dt : float)
    health.value -= poison.dps * dt
    poison.duration -= dt
    if poison.duration <= 0.0
        remove_component(self_entity, type_of(Poison))
```

### 中型 mod — Hook 已有系统 + 自定义渲染

```python
require sky_mod
require sky_ecs
require sky_render

# 新组件
struct Shield
    charges : int
    cooldown : float

# Hook: 在引擎 take_damage 之前拦截
[hook(target="take_damage", phase="before")]
def shield_absorb(var health : Health; var shield : Shield; var incoming : IncomingDamage)
    if shield.charges > 0 && incoming.amount > 0.0
        let absorbed = min(incoming.amount, 50.0)
        incoming.amount -= absorbed
        shield.charges -= 1

# 自定义 material
[material(name="shield_bubble")]
def shield_material() : MaterialDef
    return <- MaterialDef(
        shader = "shaders/shield_bubble.wgsl",
        blend = BlendMode Transparent,
        uniforms = {{ "color" => float4(0.3, 0.7, 1.0, 0.5), "pulse_speed" => 2.0 }}
    )
```

### 大型 mod — Boss AI + 世界生成

```python
require sky_mod
require sky_ecs
require sky_render
require sky_audio

# 复杂的 Boss AI (完整编程语言能力)
struct NagaBoss
    phase : int
    charge_timer : float
    segments : array<EntityId>
    target : EntityId

enum NagaPhase
    Circle = 0
    Charge = 1
    Stunned = 2
    Enraged = 3

[system(group="boss_ai")]
def naga_ai(var naga : NagaBoss;
            var transform : Transform;
            var velocity : Velocity;
            health : Health)

    // 找最近玩家
    var nearest_dist = FLT_MAX
    var nearest_pos = float3(0)
    query() <| $(player_t : Transform; _ : Player)
        let d = distance_sq(transform.position, player_t.position)
        if d < nearest_dist
            nearest_dist = d
            nearest_pos = player_t.position

    // 状态机
    if naga.phase == int(NagaPhase Circle)
        // 绕玩家转圈
        let angle = get_time() * 2.0
        let radius = 8.0
        velocity.x = (cos(angle) * radius - transform.x) * 3.0
        velocity.y = (sin(angle) * radius - transform.y) * 3.0
        naga.charge_timer -= get_dt()
        if naga.charge_timer <= 0.0
            naga.phase = int(NagaPhase Charge)

    elif naga.phase == int(NagaPhase Charge)
        let dir = normalize(nearest_pos - transform.position)
        velocity.x = dir.x * 20.0
        velocity.y = dir.y * 20.0

    elif naga.phase == int(NagaPhase Stunned)
        velocity.x = 0.0
        velocity.y = 0.0
        naga.charge_timer -= get_dt()
        if naga.charge_timer <= 0.0
            naga.phase = health.value < 0.3 ? int(NagaPhase Enraged) : int(NagaPhase Circle)

    elif naga.phase == int(NagaPhase Enraged)
        // 更快更猛，分裂蛇身 segments
        // ... 复杂逻辑
        pass

    // 更新蛇身跟随
    for seg_id, i in naga.segments, range(length(naga.segments))
        set_component(seg_id, [[SegmentTarget
            follow = i == 0 ? self_entity : naga.segments[i-1],
            delay = 0.1 * float(i)
        ]])
```

---

## 绑定生成器 — `#[das_bind]`

### 目标

用一个 proc-macro，在 Rust 代码上加 `#[das_bind]`，自动生成 daScript 可调用的完整绑定。引擎开发者不需要手写任何 C FFI 代码。

### 输入

```rust
#[das_bind]
#[repr(C)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[das_bind]
impl Vec3 {
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalize(&mut self) {
        let l = self.length();
        self.x /= l;
        self.y /= l;
        self.z /= l;
    }

    pub fn dot(&self, other: &Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
}
```

### 自动生成 — C FFI wrapper（Rust 侧）

```rust
// ---- auto-generated by #[das_bind] ----

#[no_mangle]
extern "C" fn das_Vec3_length(self_: *const Vec3) -> f32 {
    unsafe { (*self_).length() }
}

#[no_mangle]
extern "C" fn das_Vec3_normalize(self_: *mut Vec3) {
    unsafe { (*self_).normalize() }
}

#[no_mangle]
extern "C" fn das_Vec3_dot(self_: *const Vec3, other: *const Vec3) -> f32 {
    unsafe { (*self_).dot(&*other) }
}
```

### 自动生成 — daScript 模块定义

```python
# ---- auto-generated: sky_math.das ----

struct Vec3
    x : float
    y : float
    z : float

def length(self : Vec3) : float
    return _builtin_Vec3_length(self)

def normalize(var self : Vec3) : void
    _builtin_Vec3_normalize(self)

def dot(self : Vec3; other : Vec3) : float
    return _builtin_Vec3_dot(self, other)
```

### 自动生成 — 注册函数

```rust
// ---- auto-generated ----
pub fn register_das_bindings(ctx: &mut DasContext) {
    // 注册 struct layout
    ctx.register_struct("Vec3", size_of::<Vec3>(), align_of::<Vec3>(), &[
        DasField::new("x", DasType::Float, offset_of!(Vec3, x)),
        DasField::new("y", DasType::Float, offset_of!(Vec3, y)),
        DasField::new("z", DasType::Float, offset_of!(Vec3, z)),
    ]);

    // 注册 extern 函数
    ctx.register_extern("_builtin_Vec3_length",
        das_Vec3_length as *const (),
        DasSig::new(&[DasType::Ptr("Vec3")], DasType::Float));
    ctx.register_extern("_builtin_Vec3_normalize",
        das_Vec3_normalize as *const (),
        DasSig::new(&[DasType::MutPtr("Vec3")], DasType::Void));
    ctx.register_extern("_builtin_Vec3_dot",
        das_Vec3_dot as *const (),
        DasSig::new(&[DasType::Ptr("Vec3"), DasType::Ptr("Vec3")], DasType::Float));
}
```

### 类型映射表

| Rust 类型 | daScript 类型 | 传递方式 | 零拷贝？ |
|---|---|---|---|
| `f32` | `float` | 值传递 | ✅ |
| `f64` | `double` | 值传递 | ✅ |
| `i32` / `u32` | `int` / `uint` | 值传递 | ✅ |
| `i64` / `u64` | `int64` / `uint64` | 值传递 | ✅ |
| `bool` | `bool` | 值传递 | ✅ |
| `#[repr(C)]` struct | 同名 struct | 指针 | ✅ 内存布局一致 |
| `&str` | `string` | C 字符串转换 | ❌ 需拷贝 |
| `String` | `string` | C 字符串转换 | ❌ 需拷贝 |
| `EntityId` | `uint64` | 值传递 | ✅ |
| `Option<&T>` | 可空指针 | 指针 | ✅ |

### 不可直接映射的类型 — opaque handle 模式

```rust
// Vec<T>, HashMap<K,V> 等 Rust 特有类型
// 用 opaque handle 包装，只暴露方法

#[das_bind(opaque)]
pub struct Inventory {
    items: Vec<Item>,
}

#[das_bind]
impl Inventory {
    pub fn count(&self) -> i32 { self.items.len() as i32 }
    pub fn get(&self, index: i32) -> &Item { &self.items[index as usize] }
    pub fn add(&mut self, item: Item) { self.items.push(item) }
    pub fn remove(&mut self, index: i32) { self.items.remove(index as usize); }
}
```

daScript 侧：

```python
# Inventory 是 opaque — 只能通过方法操作
var inv : Inventory&  # 引用，不是值
let n = inv |> count()
let item = inv |> get(0)
inv |> add([[Item name="sword", damage=10.0]])
```

---

## ECS 集成 — 利用 Type-Erased 架构

### SkyEngine ECS 底层回顾

```rust
// reflect/registry.rs — 无需 Rust 类型，只需 name + size + align
pub fn register(name: &str, size: usize, align: usize) -> Type;

// chunk.rs — 存储就是裸字节列
pub fn column_ptr(&self, component_index: usize) -> *mut u8;
```

ECS 存储层完全不关心 Rust 类型。这是 mod 系统的根基。

### Mod 注册组件

daScript mod 中 `struct Poison { dps:float; duration:float }` 经引擎桥接后：

```
daScript struct 定义
    → name="Poison", size=8, align=4, fields=[{dps,f32,0},{duration,f32,4}]
    → 引擎调用 reflect::register("Poison", 8, 4)
    → Archetype 系统自动识别
    → Chunk 分配对应列
    → daScript 拿到 column_ptr，直接操作裸字节
```

**零拷贝，零序列化。** daScript 操作的内存就是 ECS chunk 的内存。

### 引擎暴露给 daScript 的 ECS C API

```rust
// ---- 核心 ECS 操作（约 15 个函数）----

/// 注册一个新组件类型（mod 定义的 struct）
extern "C" fn sky_register_component(
    name: *const c_char,
    size: u32,
    align: u32,
    fields: *const FieldDesc,  // 字段描述数组
    field_count: u32,
) -> ComponentId;

/// Spawn entity，返回 EntityId
extern "C" fn sky_spawn(
    component_ids: *const ComponentId,
    component_count: u32,
    data: *const u8,  // 连续的组件数据
) -> EntityId;

/// Despawn entity
extern "C" fn sky_despawn(entity: EntityId);

/// 添加组件到已有 entity
extern "C" fn sky_insert_component(
    entity: EntityId,
    component_id: ComponentId,
    data: *const u8,
);

/// 移除组件
extern "C" fn sky_remove_component(entity: EntityId, component_id: ComponentId);

/// 创建 query handle
extern "C" fn sky_query_create(
    component_ids: *const ComponentId,
    component_count: u32,
    filter_with: *const ComponentId,
    filter_with_count: u32,
    filter_without: *const ComponentId,
    filter_without_count: u32,
) -> QueryHandle;

/// 遍历 query 的每个 chunk，返回列指针和 entity 数量
/// daScript 拿到后直接用指针访问 ECS 数据
extern "C" fn sky_query_next_chunk(
    query: QueryHandle,
    out_columns: *mut *mut u8,     // 输出: 每个组件的列指针
    out_entities: *mut *const u64,  // 输出: entity ID 数组
    out_count: *mut u32,           // 输出: 本 chunk entity 数量
) -> bool; // true = 还有下一个 chunk

/// 读取 resource
extern "C" fn sky_get_resource(name: *const c_char) -> *mut u8;
```

### daScript 侧的 ECS 封装

```python
# sky_ecs.das — 引擎提供的 ECS 高级 API

# [system] 宏展开: 自动生成 query + chunk 遍历
# 用户写:
[system(group="combat")]
def apply_poison(var poison : Poison; var health : Health)
    health.value -= poison.dps * get_dt()

# 宏展开为:
def _system_apply_poison()
    var q = sky_query_create([poison_id, health_id], 2, null, 0, null, 0)
    var columns : array<void?>
    var entities : void?
    var count : uint
    while sky_query_next_chunk(q, addr(columns[0]), entities, count)
        var poison_col = reinterpret<Poison?>(columns[0])
        var health_col = reinterpret<Health?>(columns[1])
        for i in range(count)
            // 直接操作 ECS 内存 — 零拷贝
            health_col[i].value -= poison_col[i].dps * get_dt()
```

**关键：`poison_col[i]` 不是脚本变量，是直接指向 ECS chunk column 的指针解引用。** 配合 AOT 编译，这个循环体的性能和 Rust 手写几乎一样。

---

## Hook 系统

### 设计

Hook 基于 SkyEngine 的 System schedule。每个注册的 System 都有一个名字，mod 可以在任意 System 的前/后注入代码，或完全替换。

```
引擎 System 调度链:

  [input_system]
       ↓
  [movement_system]     ← mod hook: Before → shield_check()
       ↓
  [take_damage_system]  ← mod hook: Before → shield_absorb()
       ↓                  mod hook: After  → damage_vfx()
  [death_system]        ← mod hook: Replace → custom_death()
       ↓
  [cleanup_system]
```

### 引擎侧实现

```rust
// system.rs 扩展

pub(crate) struct RegisteredSystem {
    pub name: String,                           // 新增: 系统名
    pub initialized: bool,
    pub system: Box<dyn System>,
    pub hooks_before: Vec<DasHook>,             // 新增: 前置 hook
    pub hooks_after: Vec<DasHook>,              // 新增: 后置 hook
    pub replacement: Option<DasHook>,           // 新增: 替换 hook
}

pub(crate) struct DasHook {
    pub mod_name: String,
    pub function: DasFunctionPtr,  // daScript 函数指针
    pub priority: i32,             // hook 排序（多个 mod hook 同一 system）
}
```

### 引擎 tick 执行逻辑（修改）

```rust
// world.rs tick 执行时
for system in &mut group.systems {
    // 1. 执行 Before hooks（按 priority 排序）
    for hook in &system.hooks_before {
        das_context.call(hook.function, world);
    }

    // 2. 执行 System 本体（或替换）
    if let Some(replacement) = &system.replacement {
        das_context.call(replacement.function, world);
    } else {
        system.system.run(world);
    }

    // 3. 执行 After hooks（按 priority 排序）
    for hook in &system.hooks_after {
        das_context.call(hook.function, world);
    }
}
```

### Mod 侧使用

```python
# Hook: 在 take_damage 之前插入
[hook(target="take_damage", phase="before", priority=100)]
def shield_absorb(var health : Health; var shield : Shield; var dmg : IncomingDamage)
    if shield.charges > 0
        let absorbed = min(dmg.amount, 50.0)
        dmg.amount -= absorbed
        shield.charges -= 1

# Hook: 完全替换 death_system
[hook(target="death_system", phase="replace")]
def custom_death(health : Health; transform : Transform)
    if health.value <= 0.0
        // 不直接 despawn，播放死亡动画
        insert_component(self_entity, [[DeathAnimation timer=2.0]])
        remove_component(self_entity, type_of(PlayerControl))
```

---

## 渲染集成

mod 通过 daScript 可以：

### 注册自定义 Material

```python
[material]
def register_hologram() : MaterialDef
    return <- MaterialDef(
        name = "hologram",
        shader_path = "mods/my_mod/shaders/hologram.wgsl",
        blend = BlendMode Transparent,
        depth_write = false,
        uniforms = {{
            "color" => float4(0.0, 1.0, 0.8, 0.6),
            "scan_speed" => 2.0,
            "noise_tex" => load_texture("mods/my_mod/textures/noise.png")
        }}
    )
```

### 添加 Post-processing

```python
[postfx(after="bloom")]
def register_crt_effect() : PostFxDef
    return <- PostFxDef(
        name = "crt_scanlines",
        shader_path = "mods/my_mod/shaders/crt.wgsl",
        uniforms = {{
            "scanline_count" => 240.0,
            "curvature" => 0.03
        }}
    )
```

### 使用引擎渲染 API

```python
# 创建 entity 用自定义 material
let mat = get_material("hologram")
spawn_entity(
    Transform(float3(0, 5, 0)),
    MeshRenderer(mesh = "cube", material = mat),
    SortingLayer(10)
)
```

---

## Mod 加载系统

### Mod 目录结构

```
game/
  mods/
    twilight_forest/
      mod.json              ← 元数据
      scripts/
        init.das             ← 入口脚本
        bosses/naga.das
        bosses/lich.das
        worldgen/biomes.das
      assets/
        textures/
        shaders/
        models/
        sounds/
      
    industrial/
      mod.json
      scripts/init.das
      assets/...
```

### mod.json

```json
{
  "name": "Twilight Forest",
  "version": "1.0.0",
  "author": "ModTeam",
  "entry": "scripts/init.das",
  "dependencies": [],
  "load_order": 100,
  "hooks": {
    "allow_replace": true,
    "max_priority": 1000
  }
}
```

### 加载流程

```
引擎启动
   │
   ├─ 1. 扫描 mods/ 目录
   ├─ 2. 读取 mod.json，拓扑排序依赖
   ├─ 3. 创建 daScript context
   ├─ 4. 注册引擎 API (register_das_bindings)
   │
   for each mod (按 load_order):
   │  ├─ 5. 编译 init.das (解释器模式 / AOT)
   │  ├─ 6. 执行 mod 入口 → 注册组件、系统、hooks
   │  ├─ 7. 挂载资产 overlay (虚拟文件系统)
   │  └─ 8. 调用 mod 的 on_init()
   │
   ├─ 9. 所有 mod 加载完成
   └─ 10. 开始游戏循环
```

### 热重载

开发模式下：

```
文件监视器检测到 bosses/naga.das 修改
   │
   ├─ 1. 重新编译该文件
   ├─ 2. 修改过的函数 → 解释器模式
   ├─ 3. 未修改的函数 → 保持 AOT
   └─ 4. 下一帧自动使用新代码
```

---

## 资产 Overlay 系统

mod 的资产通过虚拟文件系统层叠在引擎资产之上。

```
查找 "textures/sword.png" 的顺序:

  1. mods/industrial/assets/textures/sword.png     ← 最高优先级 mod
  2. mods/twilight_forest/assets/textures/sword.png
  3. game/assets/textures/sword.png                ← 原版资产（fallback）
```

### 实现

```rust
// asset/mod_overlay.rs

pub struct ModAssetOverlay {
    /// mod 资产路径，按优先级排列（高优先级在前）
    layers: Vec<PathBuf>,
    /// 原版资产根目录
    base_path: PathBuf,
}

impl ModAssetOverlay {
    pub fn resolve(&self, asset_path: &str) -> PathBuf {
        for layer in &self.layers {
            let full = layer.join(asset_path);
            if full.exists() {
                return full;
            }
        }
        self.base_path.join(asset_path)
    }
}
```

---

## daScript 引擎绑定层 — DasContext

### Rust 侧封装（安全层 over C API）

```rust
// src/scripting/context.rs

/// 封装 daScript C API 的安全 Rust 接口
pub struct DasContext {
    raw: *mut das_context,   // daScript C API 指针
    module: *mut das_module,
}

impl DasContext {
    pub fn new() -> Self { ... }

    /// 注册 struct 类型
    pub fn register_struct(&mut self, name: &str, size: usize, align: usize, fields: &[DasField]) { ... }

    /// 注册 extern 函数
    pub fn register_extern(&mut self, name: &str, fn_ptr: *const (), sig: DasSig) { ... }

    /// 编译脚本
    pub fn compile(&mut self, source: &str) -> Result<DasProgram, DasError> { ... }

    /// 调用 daScript 函数
    pub fn call(&mut self, func: DasFunctionPtr, args: &[DasValue]) -> DasValue { ... }
}

pub struct DasField {
    pub name: String,
    pub ty: DasType,
    pub offset: usize,
}

pub enum DasType {
    Float, Double, Int, UInt, Int64, UInt64, Bool,
    String, Ptr(String), MutPtr(String), Void,
}
```

---

## proc-macro: `#[das_bind]`

### 实现概述

```
sky_engine_macros/
  src/
    lib.rs          ← proc-macro 入口
    das_bind.rs     ← 核心: 解析 struct/impl → 生成代码
    type_map.rs     ← Rust 类型 → daScript 类型映射
    codegen.rs      ← 生成 extern "C" + .das 模块 + register 函数
```

### 核心逻辑

```rust
// das_bind.rs (伪代码)

fn process_struct(input: DeriveInput) -> TokenStream {
    let name = &input.ident;
    let fields = extract_fields(&input);

    // 1. 检查 #[repr(C)]
    assert!(has_repr_c(&input), "#[das_bind] struct 必须是 #[repr(C)]");

    // 2. 生成 daScript struct 定义字符串 (编译时常量)
    let das_def = generate_das_struct_def(name, &fields);

    // 3. 生成注册函数
    let register_fn = generate_register_struct(name, &fields);

    quote! {
        #input  // 保留原始 struct

        // 自动生成的注册代码
        #register_fn

        // daScript 模块定义（编译时常量字符串）
        impl #name {
            pub const DAS_MODULE_DEF: &'static str = #das_def;
        }
    }
}

fn process_impl(input: ItemImpl) -> TokenStream {
    let type_name = &input.self_ty;
    let methods = extract_methods(&input);

    let mut extern_fns = Vec::new();
    let mut das_defs = Vec::new();
    let mut registrations = Vec::new();

    for method in &methods {
        // 生成 extern "C" wrapper
        extern_fns.push(generate_extern_c_wrapper(type_name, method));
        // 生成 .das 函数定义
        das_defs.push(generate_das_method_def(type_name, method));
        // 生成注册调用
        registrations.push(generate_method_registration(type_name, method));
    }

    quote! {
        #input  // 保留原始 impl

        #(#extern_fns)*

        // 注册所有方法
        pub fn register_das_methods_for_#type_name(ctx: &mut DasContext) {
            #(#registrations)*
        }
    }
}
```

### 特殊属性

```rust
// 整个 struct 直接映射
#[das_bind]
#[repr(C)]
pub struct Vec3 { pub x: f32, pub y: f32, pub z: f32 }

// Opaque handle — 只暴露方法，不暴露字段
#[das_bind(opaque)]
pub struct Inventory { items: Vec<Item> }

// 重命名
#[das_bind(name = "Sound")]
pub struct AudioHandle { ... }

// 跳过某个方法
#[das_bind]
impl Vec3 {
    pub fn length(&self) -> f32 { ... }

    #[das_bind(skip)]
    pub fn internal_debug(&self) { ... }  // 不暴露给 daScript
}
```

---

## 执行模式

### 开发模式

```
daScript 文件 → 解释器执行
  ✅ 热重载: 改完即生效
  ✅ 调试友好: 行号、堆栈、断点
  ⚠️ 性能: 比 native 慢，但够开发用
```

### 发布模式 — AOT

```
daScript 文件 → AOT 编译 → C++ 代码 → 机器码
  ✅ 性能: ≈ native C++
  ✅ 平台兼容: 所有平台 (iOS, 主机等)
  ❌ 不支持热重载
```

### 可选 — JIT

```
daScript 文件 → LLVM JIT → 机器码
  ✅ 性能: 可超越 C++ (LLVM 有更多优化信息)
  ✅ 运行时编译
  ⚠️ 不是所有平台支持 (iOS 禁止 JIT)
```

### 混合模式（推荐）

```
开发阶段:
  所有脚本 → 解释器
  热重载 ON

发布阶段:
  核心 mod 脚本 → AOT (预编译进游戏)
  用户 mod 脚本 → 解释器 (运行时加载)
  
  性能敏感的 mod 可由 mod 作者自行 AOT:
    dascript_aot my_mod/scripts/ --output my_mod/compiled/
```

---

## 实现路线图

### Phase 1: 基础集成（1-2 周）

- [ ] 编译 daScript C++ 库，build.rs 链接到 Rust 项目
- [ ] 封装 `DasContext`（Rust 安全层 over `c_api.h`）
- [ ] 实现基础类型注册（float, int, bool, string）
- [ ] 实现 extern 函数注册 + 调用
- [ ] 验证: 从 Rust 加载一个 .das 文件，调用其中的函数

### Phase 2: ECS 桥接（1-2 周）

- [ ] 实现 ECS C API（spawn, despawn, query, insert/remove component）
- [ ] 实现 mod 组件注册（daScript struct → `reflect::register`）
- [ ] 实现 chunk-based query 迭代（返回列指针给 daScript）
- [ ] 实现 `[system]` 宏（daScript 侧）
- [ ] 验证: daScript 定义组件 + system，ECS 正常迭代

### Phase 3: 绑定生成器（1-2 周）

- [ ] 实现 `#[das_bind]` proc-macro（struct 绑定）
- [ ] 实现 `#[das_bind]` proc-macro（impl 方法绑定）
- [ ] 实现 opaque handle 模式
- [ ] 类型映射表完善
- [ ] 验证: `#[das_bind]` 标注引擎核心类型（Vec3, Color, Transform, Camera）

### Phase 4: Hook 系统（1 周）

- [ ] 扩展 `RegisteredSystem` 支持 named systems
- [ ] 实现 hook 注册（Before / After / Replace）
- [ ] 实现 hook 调度（tick 执行时检查 hook 链）
- [ ] 实现 `[hook]` 宏（daScript 侧）
- [ ] 验证: mod hook 一个引擎 system，正常拦截

### Phase 5: Mod 加载器（1 周）

- [ ] 实现 mod 目录扫描 + mod.json 解析
- [ ] 实现依赖排序 + 加载顺序
- [ ] 实现资产 overlay 虚拟文件系统
- [ ] 实现热重载（开发模式）
- [ ] 验证: 放一个 mod 文件夹，引擎自动加载

### Phase 6: 渲染集成（1-2 周）

- [ ] 绑定渲染 API（Material 注册、Texture 加载、PostFx 添加）
- [ ] `[material]` 和 `[postfx]` 宏支持
- [ ] 绑定音频、输入、相机等引擎子系统
- [ ] 验证: mod 注册自定义 material + shader，正常渲染

### Phase 7: 完善 + 文档（1-2 周）

- [ ] mod 作者文档 + API reference
- [ ] 示例 mod（简单效果 mod + 中型内容 mod）
- [ ] AOT 编译工具链
- [ ] 错误处理完善（编译错误信息友好化）
- [ ] 性能基准测试（daScript system vs Rust native system）

**总计: 约 8-12 周**

---

## 关键设计决策总结

| 问题 | 决策 | 理由 |
|---|---|---|
| 脚本语言 | daScript | 性能最强 (≈ native)，静态类型，ECS 原生设计 |
| Rust 集成方式 | C API (`c_api.h`) + proc-macro 自动绑定 | daScript 无 Rust crate，C API 是官方推荐路径 |
| 绑定生成 | `#[das_bind]` proc-macro | 一个属性宏自动生成 FFI wrapper + .das 模块 + 注册代码 |
| ECS 数据互操作 | 列指针直接暴露给 daScript，零拷贝 | type-erased 架构天然支持 |
| 组件注册 | daScript struct → `reflect::register(name, size, align)` | ECS 无关类型，只需 layout |
| Hook 机制 | System schedule 注入 (Before/After/Replace) | 复用已有 schedule 系统 |
| 执行模式 | 开发: 解释器 (热重载) / 发布: AOT (native 性能) | 两全其美 |
| Mod 安全性 | 静态类型 + API 边界控制（不暴露 unsafe 操作） | daScript 自身是类型安全的 |
| 资产系统 | 虚拟文件系统 overlay | Mod 可替换/扩展资产，不修改原版文件 |
| 大型 mod 支持 | 完整编程语言 + 模块系统 + 多文件项目 | daScript 支持 require、模块、包管理 |
