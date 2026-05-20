# SkyEngine UI 教程

这篇教程带你从零搭一个原生 retained-mode UI：顶部 HUD、居中菜单、按钮、进度条、滑杆、开关和可滚动面板。

原生 UI 的目标是游戏运行时界面，例如 HUD、暂停菜单、战斗面板、卡牌栏、物品栏。它不是 editor/debug overlay；调试工具仍然可以继续用 `egui`。

## 1. 启用 Feature

```toml
[dependencies]
sky_engine = { path = ".", features = ["ui"] }
```

`ui` 会自动启用 `app`，并启用 `glyphon` 文本渲染。公共 API 不暴露 glyphon 类型。

如果你把教程代码放到 `examples/ui/tutorial_ui.rs`，需要在 `Cargo.toml` 注册：

```toml
[[example]]
name = "tutorial_ui"
path = "examples/ui/tutorial_ui.rs"
required-features = ["ui"]
```

运行：

```bash
cargo run --example tutorial_ui --features ui --release
```

## 2. 最小 App 骨架

UI 仍然运行在普通 `AppState` 里。默认不需要手动安装 UI 资源；第一次调用 `ctx.update_ui()` 或 `ctx.render_ui()` 时会自动安装资源。每帧按固定顺序更新、读取事件、渲染场景、渲染 UI。

```rust
use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::{EntityId, World};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings,
    SpriteFeature, Transform, TransparentPhase,
};
use sky_engine::ui::{
    UiAlign, UiAnchor, UiButton, UiEventKind, UiEvents, UiId, UiLayout, UiLength,
    UiNode, UiPanel, UiProgressBar, UiRect, UiSlider, UiText, UiToggle,
};

#[derive(Default)]
struct TutorialUi {
    ui: Option<UiRefs>,
    paused: bool,
    health: f32,
    volume: f32,
    assist: bool,
}

#[derive(Clone, Copy)]
struct UiRefs {
    status: EntityId,
    health_bar: EntityId,
    volume_slider: EntityId,
    assist_toggle: EntityId,
    menu: EntityId,
}

impl AppState for TutorialUi {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;

        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.03, 0.035, 0.045),
            ..Default::default()
        });

        world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(600.0),
            MainCamera,
        ));

        self.paused = true;
        self.health = 0.8;
        self.volume = 0.6;
        self.assist = true;
        self.ui = Some(spawn_ui(world));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.update_ui();
        self.handle_ui_events(ctx.world);
        self.sync_ui(ctx.world);

        ctx.render();
        ctx.render_ui();
    }
}
```

关键点：

- `ctx.update_ui()`：计算布局、hover、pressed、clicked，并更新 `UiSlider` / `UiToggle` / `UiScroll`。
- 读取 `UiEvents` 应放在 `update_ui()` 之后。
- `ctx.render_ui()` 应放在 `ctx.render()` 之后，让 UI 覆盖在场景上。
- UI 默认会在第一次 `ctx.update_ui()` / `ctx.render_ui()` 时安装；需要自定义配置时使用 `UiPlugin`。

自定义 UI 配置时，在创建 App 前写：

```rust
let mut world = World::new();
world.install(sky_engine::ui::UiPlugin::new(sky_engine::ui::UiConfig {
    load_system_fonts: false,
}))?;
```

## 3. 创建 UI 树

UI 节点都是 ECS 实体。`UiNode` 描述位置、大小、父子关系、布局和层级；具体显示内容由 `UiPanel`、`UiText`、`UiButton` 等组件提供。

```rust
fn spawn_ui(world: &mut World) -> UiRefs {
    let hud = world.spawn((
        UiNode::panel(1.0, 78.0)
            .anchor(UiAnchor::TopLeft)
            .width(UiLength::Percent(1.0))
            .z(10)
            .layout(UiLayout::row(
                UiRect::new(18.0, 14.0, 18.0, 14.0),
                14.0,
                UiAlign::Center,
            )),
        UiPanel::new(Color::rgba8(12, 18, 28, 220)),
    ));

    let status = world.spawn((
        UiNode::panel(1.0, 34.0)
            .child_of(hud)
            .width(UiLength::Fill(1.0)),
        UiText::new("").size(18.0),
    ));

    let health_bar = world.spawn((
        UiNode::panel(220.0, 18.0).child_of(hud),
        UiProgressBar {
            value: 0.8,
            max: 1.0,
            fill_color: Color::rgba8(92, 220, 146, 255),
            background_color: Color::rgba8(28, 36, 48, 235),
        },
    ));

    let menu = world.spawn((
        UiNode::panel(360.0, 300.0)
            .anchor(UiAnchor::Center)
            .z(40)
            .layout(UiLayout::column(
                UiRect::new(24.0, 20.0, 24.0, 20.0),
                12.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(15, 20, 31, 238)),
    ));

    world.spawn((
        UiNode::panel(1.0, 38.0)
            .child_of(menu)
            .width(UiLength::Percent(1.0)),
        UiText::new("Pause Menu")
            .size(24.0)
            .align(UiAlign::Center),
    ));

    let volume_slider = spawn_slider_row(world, menu, "volume", "Volume", 0.6);
    let assist_toggle = world.spawn((
        UiNode::panel(1.0, 30.0)
            .id("assist")
            .child_of(menu)
            .width(UiLength::Percent(1.0)),
        UiToggle::new(true).label("Aim Assist"),
    ));

    spawn_button(world, menu, "resume", "Resume");
    spawn_button(world, menu, "damage", "Take Damage");

    UiRefs {
        status,
        health_bar,
        volume_slider,
        assist_toggle,
        menu,
    }
}
```

辅助函数可以把重复行收起来：

```rust
fn spawn_button(world: &mut World, parent: EntityId, id: &'static str, label: &'static str) {
    world.spawn((
        UiNode::panel(1.0, 38.0)
            .id(UiId::new(id))
            .child_of(parent)
            .width(UiLength::Percent(1.0)),
        UiButton::new(label),
    ));
}

fn spawn_slider_row(
    world: &mut World,
    parent: EntityId,
    id: &'static str,
    label: &'static str,
    value: f32,
) -> EntityId {
    let row = world.spawn((UiNode::panel(1.0, 30.0)
        .child_of(parent)
        .width(UiLength::Percent(1.0))
        .layout(UiLayout::row(UiRect::ZERO, 10.0, UiAlign::Center)),));

    world.spawn((
        UiNode::panel(82.0, 28.0).child_of(row),
        UiText::new(label)
            .size(16.0)
            .color(Color::rgba8(188, 204, 218, 255)),
    ));

    world.spawn((
        UiNode::panel(1.0, 24.0)
            .id(UiId::new(id))
            .child_of(row)
            .width(UiLength::Fill(1.0)),
        UiSlider::new(value, 0.0, 1.0).with_step(0.05),
    ))
}
```

## 4. 处理事件

给可交互实体设置 `UiNode::id(...)`，事件里会带回这个 id。按钮通常看 `Clicked`，滑杆和开关通常在每帧从组件读取当前值。

```rust
impl TutorialUi {
    fn handle_ui_events(&mut self, world: &mut World) {
        let mut clicked = Vec::new();
        if let Some(events) = world.get_resource_mut::<UiEvents>() {
            for event in events.drain() {
                if event.kind == UiEventKind::Clicked {
                    clicked.push(event.id);
                }
            }
        }

        for id in clicked.into_iter().flatten() {
            match id.as_str() {
                "resume" => self.paused = false,
                "damage" => self.health = (self.health - 0.15).max(0.0),
                _ => {}
            }
        }
    }

    fn sync_ui(&mut self, world: &mut World) {
        let Some(ui) = self.ui else {
            return;
        };

        self.volume = world
            .get::<UiSlider>(ui.volume_slider)
            .map(|slider| slider.value)
            .unwrap_or(self.volume);
        self.assist = world
            .get::<UiToggle>(ui.assist_toggle)
            .map(|toggle| toggle.checked)
            .unwrap_or(self.assist);

        if let Some(text) = world.get_mut::<UiText>(ui.status) {
            text.text = format!(
                "hp {:>3}%   volume {:>3}%   assist {}",
                (self.health * 100.0).round() as i32,
                (self.volume * 100.0).round() as i32,
                if self.assist { "on" } else { "off" }
            );
        }

        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.health_bar) {
            bar.value = self.health;
            bar.fill_color = if self.health > 0.35 {
                Color::rgba8(92, 220, 146, 255)
            } else {
                Color::rgba8(230, 86, 86, 255)
            };
        }

        if let Some(node) = world.get_mut::<UiNode>(ui.menu) {
            node.visible = self.paused;
            node.enabled = self.paused;
        }
    }
}
```

`visible = false` 会隐藏节点和子节点；`enabled = false` 会让节点和子节点不参与 hit-test。做暂停菜单、禁用面板时，这两个字段很常用。

## 5. 滚动容器

给一个带 layout 的父节点加 `UiScroll`，直接子节点超出父 rect 后会被裁剪，鼠标滚轮会改变 `offset`。

```rust
use sky_engine::ui::UiScroll;

let list = world.spawn((
    UiNode::panel(280.0, 150.0)
        .child_of(menu)
        .layout(UiLayout::column(
            UiRect::new(12.0, 10.0, 12.0, 10.0),
            8.0,
            UiAlign::Stretch,
        )),
    UiPanel::new(Color::rgba8(20, 28, 38, 230)),
    UiScroll::vertical().wheel_speed(32.0),
));

for i in 0..12 {
    world.spawn((
        UiNode::panel(1.0, 26.0)
            .child_of(list)
            .width(UiLength::Percent(1.0)),
        UiButton::new(format!("Inventory Slot {}", i + 1)),
    ));
}
```

实战建议：把标题和滚动 body 分开。外层面板负责标题和边框，内部 body 加 `UiScroll`，这样标题不会跟着滚走。

## 6. 输入穿透

UI 和游戏共享鼠标输入。游戏逻辑如果也使用鼠标，先检查 UI 是否要吃掉指针：

```rust
if ctx.ui_state().is_some_and(|state| state.wants_pointer()) {
    return;
}
```

常见用法是：先 `ctx.update_ui()`，再处理游戏输入；如果 UI 正在 hover 或 pressed，就跳过鼠标射线、攻击、拖拽等游戏侧操作。

## 7. 完整 Main

```rust
fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("SkyEngine - UI Tutorial", 960, 600)
                .with_vsync(false)
                .with_resizable(true),
        )
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(TutorialUi::default());
}
```

## 8. 调试 Checklist

- 点不到按钮：确认每帧调用了 `ctx.update_ui()`，并且鼠标坐标使用 logical pixels。
- UI 不显示：确认调用顺序是 `ctx.render()` 后 `ctx.render_ui()`。
- 子控件越界：给父容器加 `UiScroll`，或把面板高度改够；滚动 body 和固定标题分开。
- 布局顺序不对：`Row/Column` 按实体创建顺序排布，`z` 只影响绘制和 hit-test。
- 点击穿透到游戏：使用 `ctx.ui_state().is_some_and(|s| s.wants_pointer())` 屏蔽游戏鼠标逻辑。
- 文本没有出现：默认加载系统字体；如果目标环境没有字体，注册自己的 `UiFontBook` 字体。

## 9. 继续看

- `examples/ui/hud_menu.rs`：小型 HUD + 菜单范例。
- `examples/ui/weird_ui_lab.rs`：视觉压力测试，覆盖滚动、局部 z、Fill、anchor、禁用和隐藏。
- `docs/ui.md`：组件/API 速查。
