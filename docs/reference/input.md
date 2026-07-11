# SkyEngine Input

`sky_engine::input` 在 `app` feature 下启用。它分两层：

- Raw layer：`Input`、`KeyCode`、`MouseButton`，直接查询键盘和鼠标状态。
- Action layer：`InputActions`、`ActionMap`、`InputSource`，把物理输入映射成语义动作。

```rust
use sky_engine::input::{
    ActionKind, ActionMap, ActionValue, Input, InputActions, InputSource, KeyCode, MouseAxisKind,
    MouseButton,
};
```

## Raw Input

App runner 每帧从 winit window event 同步 raw input。

常用查询：

```rust
input.key_held(KeyCode::KeyW) -> bool
input.key_pressed(KeyCode::Space) -> bool
input.key_released(KeyCode::Escape) -> bool

input.mouse_position() -> [f32; 2]
input.mouse_delta() -> [f32; 2]
input.scroll_delta() -> [f32; 2]
input.mouse_in_window() -> bool

input.mouse_button_held(MouseButton::Left) -> bool
input.mouse_button_pressed(MouseButton::Left) -> bool
input.mouse_button_released(MouseButton::Left) -> bool
```

在 `AppState::update` 中：

```rust,no_run
fn update(&mut self, ctx: &mut FrameContext<'_>) {
    if ctx.input.key_pressed(KeyCode::Space) {
        // jump
    }

    let [mx, my] = ctx.input.mouse_position();
    ctx.render();
}
```

也可以从 ECS resource 读取：

```rust,no_run
let input = world.get_resource::<Input>().unwrap();
```

## KeyCode 和 MouseButton

`KeyCode` 对应 winit physical key code，适合游戏默认键位，不受键盘布局影响。

`MouseButton`：

- `Left`
- `Right`
- `Middle`
- `Other(u16)` 或内部等价表示，以当前源码为准。

## Action Map

Action layer 用于把“按键”翻译成“动作”。

```rust,no_run
use sky_engine::input::{ActionMap, InputActions, InputSource, KeyCode};

let mut map = ActionMap::new("player");

map.add_button("jump", [InputSource::Key(KeyCode::Space)]);
map.add_axis_1d(
    "throttle",
    InputSource::Key(KeyCode::KeyS),
    InputSource::Key(KeyCode::KeyW),
);
map.add_axis_2d(
    "move",
    InputSource::Key(KeyCode::KeyW),
    InputSource::Key(KeyCode::KeyS),
    InputSource::Key(KeyCode::KeyA),
    InputSource::Key(KeyCode::KeyD),
);

let mut actions = InputActions::new();
actions.add_map(map);
world.insert_resource(actions);
```

查询：

```rust,no_run
let actions = world.get_resource::<InputActions>().unwrap();

if actions.action_pressed("jump") {
    // jump
}

let throttle = actions.action_value("throttle");
let [dx, dy] = actions.axis_value("move");
```

## InputSource

常见 source：

```rust
InputSource::Key(KeyCode::KeyW)
InputSource::MouseButton(MouseButton::Left)
InputSource::MouseAxis(MouseAxisKind::DeltaX)
InputSource::MouseAxis(MouseAxisKind::DeltaY)
InputSource::MouseAxis(MouseAxisKind::ScrollY)
```

具体 variant 以 `src/input/source.rs` 为准。

## Rebinding

ActionMap 支持运行时 rebind：

```rust,no_run
let actions = world.get_resource_mut::<InputActions>().unwrap();
let map = actions.map_mut("player").unwrap();

map.rebind("jump", 0, InputSource::Key(KeyCode::Enter));
```

`binding_index` 是该 action 的第几个 binding。返回 `false` 表示 action 或 binding 不存在。

## Context / Map Enable

可以启用/禁用某个 action map：

```rust,no_run
actions.set_map_enabled("player", false);
actions.set_map_enabled("menu", true);
```

典型用法：

- gameplay map
- menu map
- debug camera map
- editor shortcut map

同名 action 在多个启用 map 中同时存在时，`InputActions` 会按内部 map 顺序聚合；长期项目建议避免语义冲突。

## 输入时序

默认 `App` 每帧顺序：

```text
winit events -> Input
world.tick_with_frame_delta(...)
AppState::update(ctx)
```

如果系统需要读取输入，应确保系统运行时 `Input` resource 已经同步。App runner 会在 tick 前同步输入，所以 system 可读到当前帧状态。

如果在 `AppState::update` 中直接读取 `ctx.input` 并写组件，默认会在下一帧 schedule 生效。需要同帧物理输入时，把控制 system 注册在 `FixedUpdate` 的 physics system 之前，或关闭 `auto_tick` 手动排序。

## 测试和检查

```bash
cargo test --features app input
cargo check --features app
```
