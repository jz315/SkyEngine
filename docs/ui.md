# UI

SkyEngine 的原生 UI 是 retained-mode、ECS-first 的游戏 UI。它用于 HUD、菜单、按钮、文本、进度条、卡牌栏这类运行时界面；`egui` 仍然适合作为 debug/tool overlay，不是游戏 UI 主路径。

如果你想跟着做一个完整小界面，先看 [`ui_tutorial.md`](ui_tutorial.md)。本文更像组件/API 速查。

## Feature

```toml
sky_engine = { path = "...", features = ["ui"] }
```

`ui` 会启用 `app`，并使用 `glyphon 0.8` 渲染文本。glyphon 类型不会出现在 public API 里。

## Frame Order

推荐顺序：

```rust
ctx.update_ui();
// read UiEvents / UiState here
ctx.render();
ctx.render_ui();
```

`update_ui` 使用逻辑像素和左上角原点做布局、hit-test、hover/press/click 事件，并会更新交互控件（例如 `UiSlider`、`UiToggle`）的值。`render_ui` 在当前 surface frame 上追加 overlay pass，所以 UI 不受游戏 camera 影响。

## Configuration

默认不需要手动 install。`ctx.update_ui()` / `ctx.render_ui()` 会在第一次使用时自动插入 UI 资源。

需要自定义 UI 配置时，把 `UiPlugin` 安装进 `World`：

```rust
use sky_engine::ecs::World;
use sky_engine::ui::{UiConfig, UiPlugin};

let mut world = World::new();
UiPlugin::new(UiConfig {
    load_system_fonts: false,
})
.install(&mut world);
```

自动安装会插入 `UiConfig`、`UiTheme`、`UiState`、`UiEvents`、`UiFontBook`。默认会尝试系统字体；没有可用字体时 quad UI 仍然渲染。

底层/非 App 用法仍然可以直接调用 `install_ui(world, config)`，但普通游戏和示例应走 `UiPlugin::install`。

## Components

UI 节点都是普通 ECS 实体：

```rust
use sky_engine::render::Color;
use sky_engine::ui::*;

let panel = world.spawn((
    UiNode::panel(360.0, 220.0)
        .anchor(UiAnchor::Center)
        .z(100)
        .layout(UiLayout::column(
            UiRect::new(24.0, 20.0, 24.0, 20.0),
            12.0,
            UiAlign::Stretch,
        )),
    UiPanel::new(Color::rgba8(16, 22, 30, 230)),
));

world.spawn((
    UiNode::panel(1.0, 42.0)
        .id("start")
        .child_of(panel)
        .width(UiLength::Percent(1.0)),
    UiButton::new("Start"),
));
```

核心组件：

- `UiNode`：位置、尺寸、anchor、z、visible/enabled、父子关系、row/column layout。
- `UiPanel`：彩色矩形。
- `UiText`：文本、字号、颜色、对齐。
- `UiButton`：按钮 label 和状态色。
- `UiProgressBar`：水平进度条。
- `UiSlider`：水平滑杆，支持点击/拖动更新 `value`，可选 `step` 量化。
- `UiToggle`：二值开关/复选框风格控件，点击后更新 `checked`。
- `UiScroll`：滚动容器行为，子节点会按 `offset` 偏移并裁剪在父 rect 内，鼠标滚轮会更新滚动偏移。

父子关系放在 `UiNode::child_of(parent)`，不侵入 ECS 本身。父节点 hidden/disabled 会影响子节点的 hit-test 和渲染。
`z` 是局部层级：父节点形成 stacking context，子节点会绘制在父面板上方并优先接收命中，但不会越过父节点所在层级去盖住更高层的兄弟面板。
`Row/Column` 子节点按创建顺序排列；`z` 只影响绘制和命中，不改变布局顺序。

## Layout

坐标是 logical pixels，原点在窗口左上角。

- `UiAnchor::TopLeft/TopRight/BottomLeft/BottomRight/Center/Stretch`
- `UiLength::Px`
- `UiLength::Percent`：父 rect 的 0.0 到 1.0 比例。
- `UiLength::Fill(weight)`：在 `Row/Column` 主轴上按权重分配剩余空间；在普通节点上等同于填满父 rect 的对应轴。
- `UiLength::Auto`：v1 主要用于 text/button 的 preferred size。
- `UiLayout::Row/Column`：支持 padding、gap、cross-axis align。
- `UiScroll::vertical()/horizontal()/both()`：给同一个实体加滚动行为；内容尺寸会从直接子节点自动测量，也可以用 `content_size(width, height)` 设置最小内容尺寸。

v1 不做 CSS/flexbox，不做输入框、dropdown。复杂布局后续应作为新 layout primitive 增量加入。

## Events

`UiEvents` 是 drainable resource：

```rust
use sky_engine::ui::{UiEventKind, UiEvents};

if let Some(events) = world.get_resource_mut::<UiEvents>() {
    for event in events.drain() {
        if event.kind == UiEventKind::Clicked {
            if event.id.as_ref().is_some_and(|id| id.as_str() == "start") {
                // start game
            }
        }
    }
}
```

事件包括：

- `HoverStarted`
- `HoverEnded`
- `Pressed`
- `Released`
- `Clicked`
- `ValueChanged`：由 `UiSlider`、`UiToggle`、`UiScroll` 发出；事件只标识实体/id，新的值从对应实体组件读取。

滑杆示例：

```rust
let volume = world.spawn((
    UiNode::panel(220.0, 24.0).id("volume"),
    UiSlider::new(0.65, 0.0, 1.0).with_step(0.05),
));

if let Some(slider) = world.get::<UiSlider>(volume) {
    let volume_value = slider.value;
}
```

开关示例：

```rust
let assist = world.spawn((
    UiNode::panel(180.0, 30.0).id("assist"),
    UiToggle::new(true).label("Aim Assist"),
));

if let Some(toggle) = world.get::<UiToggle>(assist) {
    let enabled = toggle.checked;
}
```

`UiState::wants_pointer()` 可以让游戏输入屏蔽鼠标穿透：

```rust
if ctx.ui_state().is_some_and(|state| state.wants_pointer()) {
    return;
}
```

## Fonts

```rust
use sky_engine::ui::UiFontBook;

let fonts = world.get_resource_mut::<UiFontBook>().unwrap();
fonts.add_font_bytes("ui", include_bytes!("MyFont.ttf").as_slice());
```

`UiFontBook::load_system_fonts()` 默认启用。v1 只提供字体来源注册，不暴露 glyphon buffer/cache/atlas。

## Examples

```bash
cargo run --example hud_menu --features ui --release
cargo run --example weird_ui_lab --features ui --release
cargo run --example lawn_defense_game --features ui --release
```

`hud_menu` 是最小原生 UI 示例；`weird_ui_lab` 是视觉压力测试场，用来检查 anchor、局部 z、Fill、disabled/hidden、overlap 和交互控件；`lawn_defense_game` 使用 UI 做顶部 HUD、血量/波次条、暂停按钮和标题/暂停/胜负菜单。

## Tests

```bash
cargo test --features ui ui
cargo check --example hud_menu --features ui
cargo check --example weird_ui_lab --features ui
cargo check --example lawn_defense_game --features ui
```
