# SkyEngine Reflect

`sky_engine::reflect` 是基础设施，不是 editor、scene 或 ECS 的私有实现。它分两层：

- 基础 layout 层：`Type` / `TypeInfo` / `type_of::<T>()`，记录类型名、size、align、drop 函数和 Rust `TypeId`。ECS 使用这一层做组件存储和析构。
- Inspector 层：`#[derive(Reflect)]` / `ReflectRegistry` / `ReflectType` / `ReflectField` / `ReflectPath`，用于工具、调试、Inspector、AI 辅助编辑。

重点是分层：ECS 可以使用反射，但只依赖最薄的 layout 信息；字段元数据不进入 ECS 查询和 archetype 热路径。

## 导出

```rust
use sky_engine::reflect::{
    type_of, Reflect, ReflectAttrs, ReflectEnumValue, ReflectError, ReflectField,
    ReflectKind, ReflectPath, ReflectRegistry, ReflectStructValue, ReflectType,
    ReflectValue, Type, TypeInfo,
};
```

ECS 语义别名也从这里来：

```rust
use sky_engine::reflect::{component_type, ComponentType};
```

## 基础 Type 层

```rust
use sky_engine::reflect::type_of;

let ty = type_of::<Position>();

assert_eq!(ty.size, std::mem::size_of::<Position>());
assert_eq!(ty.align, std::mem::align_of::<Position>());
```

`Type` 是 copyable handle。它 deref 到 `TypeInfo`，可读：

- `name`
- `size`
- `align`
- `drop_fn`
- `rust_type_id()`
- `needs_drop()`

非 `Copy` 或需要析构的类型会保存 type-erased drop 函数。ECS 的 `despawn/remove/clear/World drop` 依赖这条信息保证组件生命周期正确。

动态 layout 注册：

```rust
use sky_engine::reflect::register;

let ty = register("script.Vec2Like", 8, 4);
```

普通 Rust 类型：

```rust
use sky_engine::reflect::{query_by_rust_type, type_of};

let ty = type_of::<Position>();
assert_eq!(query_by_rust_type::<Position>(), Some(ty));
```

ECS 语义入口：

- `component_type::<T>()` 等价于 `type_of::<T>()`，用于表达“这个 layout handle 正在作为 ECS 组件类型使用”。

## Inspector 反射

用户类型推荐使用 derive：

```rust
use sky_engine::reflect::{Reflect, ReflectRegistry, ReflectValue};

#[derive(Reflect)]
#[reflect(name = "game.Stats")]
struct Stats {
    #[reflect(label = "HP", min = 0, max = 100, step = 1)]
    hp: u32,

    #[reflect(readonly)]
    level: u32,

    #[reflect(skip)]
    cache: Vec<u8>,
}

let mut registry = ReflectRegistry::with_builtins();
registry.register::<Stats>()?;

let ty = registry.type_of::<Stats>().unwrap();
let hp = ty.field("hp").unwrap();

let mut stats = Stats { hp: 80, level: 3, cache: vec![] };
assert_eq!(hp.read(&stats)?, ReflectValue::U64(80));
hp.write(&mut stats, ReflectValue::U64(90))?;
# Ok::<(), sky_engine::reflect::ReflectError>(())
```

`#[reflect(name = "...")]` 给类型一个稳定路径。长期工具数据推荐写稳定名字，例如 `game.Stats`，不要依赖 Rust module path。

字段属性：

```rust
#[reflect(skip)]
#[reflect(readonly)]
#[reflect(label = "HP")]
#[reflect(category = "Combat")]
#[reflect(min = 0, max = 100, step = 1)]
```

## ReflectRegistry

`ReflectRegistry` 是显式对象，没有隐藏全局自动扫描。

```rust
let mut registry = ReflectRegistry::new();
let mut registry = ReflectRegistry::with_builtins();

registry.register::<Stats>()?;
registry.type_of::<Stats>() -> Option<&ReflectType>
registry.type_by_path("game.Stats") -> Option<&ReflectType>
registry.type_by_id(type_id) -> Option<&ReflectType>
```

行为：

- 重复注册同一个 Rust 类型是 idempotent。
- 同一个 type path 被不同 Rust 类型占用会返回 `ReflectError::DuplicateTypePath`。
- 注册 derived struct 会递归注册字段类型依赖。
- registry 不会自动枚举世界中所有组件；Rust 运行时无法安全知道“所有可能要 inspect 的类型”。

## ReflectType

`ReflectType` 是 Inspector metadata，不是 ECS layout handle。

可读：

- `layout() -> Type`
- `type_id() -> TypeId`
- `path() -> &str`
- `kind() -> &ReflectKind`
- `fields() -> &[ReflectField]`
- `field(name) -> Option<&ReflectField>`
- `variants() -> &[ReflectVariant]`
- `variant(name) -> Option<&ReflectVariant>`

`ReflectKind`：

- `Value`
- `Struct`
- `Enum`
- `Option`
- `List`
- `Array`
- `Opaque`

## ReflectField

字段可读元数据：

- `name()`
- `type_id()`
- `type_path()`
- `attrs()`

字段读写：

```rust
let value = field.read(&owner)?;
field.write(&mut owner, ReflectValue::U64(100))?;
```

readonly 字段会拒绝写入并返回 `ReflectError::ReadonlyField`。

## ReflectValue

`ReflectValue` 是 Inspector 编辑值，不是 scene/save 的持久化格式。

当前覆盖：

- `Unit`
- `Bool`
- `I64`
- `U64`
- `F32`
- `F64`
- `String`
- `Vec2([f32; 2])`
- `Vec3([f32; 3])`
- `Vec4([f32; 4])`
- `Quat([f32; 4])`
- `Struct(ReflectStructValue)`
- `Enum(ReflectEnumValue)`
- `Option(Option<Box<ReflectValue>>)`
- `List(Vec<ReflectValue>)`
- `Array(Vec<ReflectValue>)`

启用 `reflect-serde` 后，`ReflectValue`、`ReflectStructValue`、`ReflectEnumValue` 可 serde 序列化，适合调试快照、工具缓存、Inspector 状态，不用于 scene 主保存路径。

## 嵌套路径

`ReflectPath` 支持 `"stats.hp"` 这种嵌套字段访问。

```rust
use sky_engine::reflect::{Reflect, ReflectPath, ReflectRegistry, ReflectValue};

#[derive(Reflect)]
struct Stats {
    hp: u32,
}

#[derive(Reflect)]
struct Loadout {
    stats: Stats,
}

let mut registry = ReflectRegistry::with_builtins();
registry.register::<Loadout>()?;

let mut loadout = Loadout { stats: Stats { hp: 10 } };
let path = ReflectPath::new("stats.hp")?;

assert_eq!(path.read(&registry, &loadout)?, ReflectValue::U64(10));
path.write(&registry, &mut loadout, ReflectValue::U64(20))?;
# Ok::<(), sky_engine::reflect::ReflectError>(())
```

路径写入会逐层查 registry。如果中间类型没注册，会返回 `UnknownType`。

## Enum

derive enum 会产生 variant metadata，并能读当前 variant：

```rust
#[derive(Reflect)]
enum Mode {
    Idle,
    Running,
}
```

v1 支持直接写 unit variant。带 payload 的 variant 当前主要用于 inspect，复杂构造后续再扩展。

## Option / Vec / Array

内置泛型实现：

- `Option<T>`：可读 `Some/None`，可把现有 `Some` 的内部值写回，可写成 `None`。
- `Vec<T>`：可读列表，可原地写回相同长度列表，不负责 resize。
- `[T; N]`：可读写固定长度数组。

这些实现都要求 `T: Reflect`。

## Builtins

`ReflectRegistry::with_builtins()` 注册：

- bool
- signed / unsigned integers
- `f32` / `f64`
- `String`
- `Vec2` / `Vec3` / `Vec4`
- `Quat`
- `Transform`

`Transform` 作为 struct 反射：

- `position: Vec3`
- `scale: Vec3`
- `rotation: Quat`

## Scene 边界

Persistence 是 serde-first：

- 自定义保存组件使用 `#[persist(component)]` 声明保存意图。
- `Persistence::auto(namespace)` 管理自动注册、稳定类型名和加载插入。
- `reflect-serde` 不会被 scene 自动启用。
- Inspector 反射可以以后用于编辑界面或 AI 工具，但不是 scene 文件格式的底层。

这样可以避免把 editor/inspector 决策塞进 ECS 或 save format。

## 错误

常见 `ReflectError`：

- `DuplicateTypePath`
- `UnknownType`
- `UnknownField`
- `ReadonlyField`
- `ValueTypeMismatch`
- `IntegerOutOfRange`
- `StructTypeMismatch`
- `EnumTypeMismatch`
- `TypeReadUnsupported`
- `TypeWriteUnsupported`
- `Unsupported`

## 测试

```bash
cargo test reflect
cargo test --features reflect-serde
```
