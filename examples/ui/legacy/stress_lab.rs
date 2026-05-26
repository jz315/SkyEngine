//! Visual stress lab for the legacy retained UI.
//!
//! ```bash
//! cargo run --example ui_legacy_stress_lab --features ui-legacy --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::{EntityId, World};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings,
    SpriteFeature, Transform, TransparentPhase,
};
use sky_engine::ui::{
    UiAlign, UiAnchor, UiButton, UiEventKind, UiEvents, UiId, UiLayout, UiLength, UiNode, UiPanel,
    UiProgressBar, UiRect, UiScroll, UiSlider, UiState, UiText, UiToggle,
};

const WINDOW_W: u32 = 1180;
const WINDOW_H: u32 = 760;

#[derive(Default)]
struct WeirdUiLab {
    ui: Option<UiRefs>,
    time: f32,
    clicks: u32,
    mode: u32,
    wobble: f32,
    chaos: f32,
    alarm: f32,
    glass: bool,
    lock: bool,
    reveal: bool,
}

#[derive(Clone, Copy)]
struct UiRefs {
    status_text: EntityId,
    hover_text: EntityId,
    ticker_text: EntityId,
    scan_bar: EntityId,
    fill_a: EntityId,
    fill_b: EntityId,
    fill_c: EntityId,
    stack_child: EntityId,
    low_parent_child: EntityId,
    disabled_panel: EntityId,
    secret_panel: EntityId,
    wobble_slider: EntityId,
    chaos_slider: EntityId,
    alarm_slider: EntityId,
    glass_toggle: EntityId,
    lock_toggle: EntityId,
    reveal_toggle: EntityId,
}

impl AppState for WeirdUiLab {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.018, 0.022, 0.03),
            ..Default::default()
        });
        world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(760.0),
            MainCamera,
        ));

        self.wobble = 0.42;
        self.chaos = 0.68;
        self.alarm = 0.27;
        self.glass = true;
        self.lock = false;
        self.reveal = true;
        self.ui = Some(spawn_ui(world));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.update_ui();
        self.handle_events(ctx.world);
        self.sync_controls(ctx.world);
        self.animate(ctx.world, ctx.dt);

        ctx.render();
        ctx.render_ui();
    }
}

impl WeirdUiLab {
    fn handle_events(&mut self, world: &mut World) {
        let mut clicked = Vec::new();
        if let Some(events) = world.get_resource_mut::<UiEvents>() {
            for event in events.drain() {
                if event.kind == UiEventKind::Clicked {
                    clicked.push(event.id);
                }
            }
        }

        for id in clicked.into_iter().flatten() {
            self.clicks = self.clicks.wrapping_add(1);
            match id.as_str() {
                "mode" => self.mode = (self.mode + 1) % 4,
                "panic" => self.alarm = (self.alarm + 0.22).fract(),
                "zero" => {
                    self.wobble = 0.0;
                    self.chaos = 0.0;
                    self.alarm = 0.0;
                    if let Some(slider) = world.get_mut::<UiSlider>(self.ui.unwrap().wobble_slider)
                    {
                        slider.set_value(self.wobble);
                    }
                    if let Some(slider) = world.get_mut::<UiSlider>(self.ui.unwrap().chaos_slider) {
                        slider.set_value(self.chaos);
                    }
                    if let Some(slider) = world.get_mut::<UiSlider>(self.ui.unwrap().alarm_slider) {
                        slider.set_value(self.alarm);
                    }
                }
                _ => {}
            }
        }
    }

    fn sync_controls(&mut self, world: &World) {
        let Some(ui) = self.ui else {
            return;
        };
        self.wobble = world
            .get::<UiSlider>(ui.wobble_slider)
            .map(|slider| slider.value)
            .unwrap_or(self.wobble);
        self.chaos = world
            .get::<UiSlider>(ui.chaos_slider)
            .map(|slider| slider.value)
            .unwrap_or(self.chaos);
        self.alarm = world
            .get::<UiSlider>(ui.alarm_slider)
            .map(|slider| slider.value)
            .unwrap_or(self.alarm);
        self.glass = world
            .get::<UiToggle>(ui.glass_toggle)
            .map(|toggle| toggle.checked)
            .unwrap_or(self.glass);
        self.lock = world
            .get::<UiToggle>(ui.lock_toggle)
            .map(|toggle| toggle.checked)
            .unwrap_or(self.lock);
        self.reveal = world
            .get::<UiToggle>(ui.reveal_toggle)
            .map(|toggle| toggle.checked)
            .unwrap_or(self.reveal);
    }

    fn animate(&mut self, world: &mut World, dt: f32) {
        let Some(ui) = self.ui else {
            return;
        };
        self.time += dt;
        let pulse = self.time.sin() * 0.5 + 0.5;
        let scan = (self.time * (0.18 + self.chaos * 0.9)).fract();

        if let Some(text) = world.get_mut::<UiText>(ui.status_text) {
            text.text = format!(
                "mode {}   clicks {}   wobble {:>3}%   chaos {:>3}%   alarm {:>3}%   glass {}   lock {}   reveal {}",
                self.mode,
                self.clicks,
                (self.wobble * 100.0).round() as i32,
                (self.chaos * 100.0).round() as i32,
                (self.alarm * 100.0).round() as i32,
                on_off(self.glass),
                on_off(self.lock),
                on_off(self.reveal),
            );
        }

        let hover = world
            .get_resource::<UiState>()
            .and_then(|state| state.hovered())
            .map(|entity| format!("hover entity #{}", entity.index()))
            .unwrap_or_else(|| "hover none".to_string());
        if let Some(text) = world.get_mut::<UiText>(ui.hover_text) {
            text.text = format!("{hover}   pointer capture follows topmost UI");
        }

        if let Some(text) = world.get_mut::<UiText>(ui.ticker_text) {
            let phase = ((self.time * 8.0) as i32).rem_euclid(6);
            text.text = format!(
                "strange ticker [{}]  local z stacks | fill lanes | anchors | disabled hit-test",
                "#".repeat(phase as usize + 1)
            );
        }

        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.scan_bar) {
            bar.value = scan;
            bar.fill_color = mix(
                Color::rgba8(72, 216, 154, 255),
                Color::rgba8(255, 118, 94, 255),
                self.alarm,
            );
        }
        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.fill_a) {
            bar.value = (pulse * 0.55 + self.wobble * 0.45).fract();
        }
        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.fill_b) {
            bar.value = (scan + self.chaos * 0.25).fract();
        }
        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.fill_c) {
            bar.value = self.alarm.max((1.0 - pulse) * 0.35);
            bar.fill_color = mix(
                Color::rgba8(84, 164, 248, 255),
                Color::rgba8(255, 208, 96, 255),
                self.alarm,
            );
        }

        if let Some(node) = world.get_mut::<UiNode>(ui.stack_child) {
            let radius = 18.0 + self.wobble * 28.0;
            node.position[0] = 68.0 + self.time.cos() * radius;
            node.position[1] = 58.0 + self.time.sin() * radius * 0.55;
        }
        if let Some(node) = world.get_mut::<UiNode>(ui.low_parent_child) {
            node.position[0] = 26.0 + (self.time * 1.7).sin() * 16.0;
        }
        if let Some(node) = world.get_mut::<UiNode>(ui.disabled_panel) {
            node.enabled = !self.lock;
        }
        if let Some(node) = world.get_mut::<UiNode>(ui.secret_panel) {
            node.visible = self.reveal;
            node.enabled = self.reveal;
        }

        recolor_panel(
            world,
            ui.disabled_panel,
            if self.lock {
                Color::rgba8(56, 58, 64, 178)
            } else if self.glass {
                Color::rgba8(42, 82, 98, 184)
            } else {
                Color::rgba8(64, 74, 92, 236)
            },
        );
    }
}

fn spawn_ui(world: &mut World) -> UiRefs {
    let root = world.spawn((UiNode::new()
        .anchor(UiAnchor::Stretch)
        .at(22.0, 18.0)
        .z(20)
        .layout(UiLayout::column(UiRect::ZERO, 12.0, UiAlign::Stretch)),));

    let top = world.spawn((
        UiNode::new()
            .child_of(root)
            .width(UiLength::Percent(1.0))
            .height(UiLength::Px(74.0))
            .z(8)
            .layout(UiLayout::row(
                UiRect::new(18.0, 12.0, 18.0, 12.0),
                14.0,
                UiAlign::Center,
            )),
        UiPanel::new(Color::rgba8(12, 18, 30, 226)),
    ));
    let title = world.spawn((
        UiNode::panel(290.0, 50.0).child_of(top),
        UiText::new("WEIRD UI LAB")
            .size(28.0)
            .color(Color::rgba8(244, 248, 252, 255)),
    ));
    world.spawn((
        UiNode::panel(20.0, 50.0)
            .child_of(top)
            .width(UiLength::Fill(1.0)),
        UiPanel::new(Color::rgba8(24, 36, 54, 220)),
    ));
    let scan_bar = world.spawn((
        UiNode::panel(250.0, 18.0).child_of(top),
        UiProgressBar {
            value: 0.0,
            max: 1.0,
            fill_color: Color::rgba8(72, 216, 154, 255),
            background_color: Color::rgba8(8, 13, 20, 230),
        },
    ));
    world.spawn((
        UiNode::panel(92.0, 42.0).id("mode").child_of(top),
        UiButton::new("Mode"),
    ));
    world.spawn((
        UiNode::panel(92.0, 42.0).id("panic").child_of(top),
        UiButton {
            normal_color: Color::rgba8(106, 54, 62, 230),
            hover_color: Color::rgba8(164, 72, 76, 242),
            pressed_color: Color::rgba8(82, 40, 48, 245),
            ..UiButton::new("Panic")
        },
    ));
    world.spawn((
        UiNode::panel(74.0, 42.0).id("zero").child_of(top),
        UiButton::new("Zero"),
    ));
    let _ = title;

    let ticker_text = world.spawn((
        UiNode::panel(1.0, 28.0)
            .child_of(root)
            .width(UiLength::Percent(1.0))
            .z(7),
        UiText::new("")
            .size(17.0)
            .color(Color::rgba8(156, 232, 205, 255)),
    ));

    let main_row = world.spawn((UiNode::panel(1.0, 1.0)
        .child_of(root)
        .width(UiLength::Percent(1.0))
        .height(UiLength::Fill(1.0))
        .layout(UiLayout::row(UiRect::ZERO, 14.0, UiAlign::Stretch)),));

    let controls = spawn_controls(world, main_row);
    let stack = spawn_stack_lab(world, main_row);
    let right_column = world.spawn((UiNode::panel(1.0, 1.0)
        .child_of(main_row)
        .width(UiLength::Fill(1.0))
        .layout(UiLayout::column(UiRect::ZERO, 8.0, UiAlign::Stretch)),));
    let fill = spawn_fill_lab(world, right_column);
    spawn_anchor_lab(world, right_column);

    let odd = spawn_oddities(world, root);

    let footer = world.spawn((UiNode::panel(1.0, 30.0)
        .child_of(root)
        .width(UiLength::Percent(1.0))
        .z(9)
        .layout(UiLayout::row(UiRect::ZERO, 16.0, UiAlign::Center)),));

    let status_text = world.spawn((
        UiNode::panel(1.0, 28.0)
            .child_of(footer)
            .width(UiLength::Fill(1.0)),
        UiText::new("")
            .size(16.0)
            .color(Color::rgba8(222, 232, 242, 255)),
    ));

    let hover_text = world.spawn((
        UiNode::panel(420.0, 26.0)
            .child_of(footer)
            .min_size(360.0, 24.0),
        UiText::new("")
            .size(16.0)
            .align(UiAlign::End)
            .color(Color::rgba8(170, 184, 202, 255)),
    ));

    UiRefs {
        status_text,
        hover_text,
        ticker_text,
        scan_bar,
        fill_a: fill.0,
        fill_b: fill.1,
        fill_c: fill.2,
        stack_child: stack.0,
        low_parent_child: stack.1,
        disabled_panel: odd.0,
        secret_panel: odd.1,
        wobble_slider: controls.0,
        chaos_slider: controls.1,
        alarm_slider: controls.2,
        glass_toggle: controls.3,
        lock_toggle: controls.4,
        reveal_toggle: controls.5,
    }
}

fn spawn_controls(
    world: &mut World,
    parent: EntityId,
) -> (EntityId, EntityId, EntityId, EntityId, EntityId, EntityId) {
    let panel = world.spawn((
        UiNode::panel(322.0, 1.0)
            .child_of(parent)
            .z(1)
            .layout(UiLayout::column(
                UiRect::new(18.0, 16.0, 18.0, 16.0),
                12.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(24, 30, 42, 232)),
    ));
    world.spawn((
        UiNode::panel(1.0, 32.0)
            .child_of(panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("Control Cabinet")
            .size(23.0)
            .color(Color::rgba8(246, 240, 210, 255)),
    ));
    let body = world.spawn((
        UiNode::panel(1.0, 1.0)
            .child_of(panel)
            .width(UiLength::Percent(1.0))
            .height(UiLength::Fill(1.0))
            .layout(UiLayout::column(UiRect::ZERO, 12.0, UiAlign::Stretch)),
        UiScroll::vertical().wheel_speed(34.0),
    ));
    let wobble = spawn_slider(world, body, "wobble", "Wobble", 0.42, accent_blue());
    let chaos = spawn_slider(world, body, "chaos", "Chaos", 0.68, accent_green());
    let alarm = spawn_slider(world, body, "alarm", "Alarm", 0.27, accent_orange());
    let glass = spawn_toggle(
        world,
        body,
        "glass",
        UiToggle::new(true).label("Glass Tint"),
    );
    let lock = spawn_toggle(
        world,
        body,
        "lock",
        UiToggle::new(false).label("Disable Odd Panel"),
    );
    let reveal = spawn_toggle(
        world,
        body,
        "reveal",
        UiToggle::new(true).label("Reveal Secret"),
    );
    world.spawn((
        UiNode::panel(1.0, 24.0)
            .child_of(body)
            .width(UiLength::Percent(1.0)),
        UiText::new("scroll pocket")
            .size(15.0)
            .color(Color::rgba8(158, 174, 190, 255)),
    ));
    for i in 0..5 {
        let row = world.spawn((UiNode::panel(1.0, 28.0)
            .child_of(body)
            .width(UiLength::Percent(1.0))
            .layout(UiLayout::row(UiRect::ZERO, 10.0, UiAlign::Center)),));
        world.spawn((
            UiNode::panel(58.0, 22.0).child_of(row),
            UiText::new(format!("slot {}", i + 1))
                .size(14.0)
                .color(Color::rgba8(182, 194, 210, 255)),
        ));
        world.spawn((
            UiNode::panel(1.0, 18.0)
                .child_of(row)
                .width(UiLength::Fill((i + 1) as f32)),
            UiPanel::new(mix(
                Color::rgba8(70, 205, 145, 190),
                Color::rgba8(238, 94, 128, 190),
                i as f32 / 4.0,
            )),
        ));
    }
    (wobble, chaos, alarm, glass, lock, reveal)
}

fn spawn_stack_lab(world: &mut World, parent: EntityId) -> (EntityId, EntityId) {
    let root = world.spawn((
        UiNode::panel(360.0, 1.0).child_of(parent).z(2),
        UiPanel::new(Color::rgba8(18, 24, 36, 226)),
    ));
    world.spawn((
        UiNode::panel(310.0, 30.0).child_of(root).at(18.0, 14.0),
        UiText::new("Stacking Context Trap")
            .size(22.0)
            .color(Color::rgba8(240, 244, 250, 255)),
    ));
    let low_parent = world.spawn((
        UiNode::panel(212.0, 140.0)
            .child_of(root)
            .at(20.0, 72.0)
            .z(4),
        UiPanel::new(Color::rgba8(62, 78, 130, 224)),
    ));
    let low_parent_child = world.spawn((
        UiNode::panel(132.0, 48.0)
            .child_of(low_parent)
            .at(26.0, 44.0)
            .z(900),
        UiPanel::new(Color::rgba8(255, 194, 86, 236)),
    ));
    world.spawn((
        UiNode::panel(118.0, 28.0)
            .child_of(low_parent_child)
            .at(8.0, 10.0),
        UiText::new("z=900 child")
            .size(15.0)
            .align(UiAlign::Center)
            .color(Color::rgba8(30, 24, 18, 255)),
    ));
    world.spawn((
        UiNode::panel(190.0, 132.0)
            .child_of(root)
            .at(132.0, 108.0)
            .z(8),
        UiPanel::new(Color::rgba8(30, 176, 152, 220)),
    ));
    world.spawn((
        UiNode::panel(148.0, 34.0)
            .child_of(root)
            .at(178.0, 174.0)
            .z(9),
        UiButton::new("Top sibling"),
    ));
    let stack_child = world.spawn((
        UiNode::panel(116.0, 38.0)
            .child_of(root)
            .at(72.0, 58.0)
            .z(12),
        UiPanel::new(Color::rgba8(238, 94, 128, 236)),
    ));
    world.spawn((
        UiNode::panel(100.0, 24.0)
            .child_of(stack_child)
            .at(8.0, 7.0),
        UiText::new("wobble chip").size(14.0).align(UiAlign::Center),
    ));
    (stack_child, low_parent_child)
}

fn spawn_fill_lab(world: &mut World, parent: EntityId) -> (EntityId, EntityId, EntityId) {
    let panel = world.spawn((
        UiNode::panel(1.0, 174.0)
            .child_of(parent)
            .width(UiLength::Percent(1.0))
            .z(2)
            .layout(UiLayout::column(
                UiRect::new(16.0, 12.0, 16.0, 10.0),
                6.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(28, 30, 42, 232)),
    ));
    world.spawn((
        UiNode::panel(1.0, 26.0)
            .child_of(panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("Fill Weights And Bars")
            .size(20.0)
            .color(Color::rgba8(230, 238, 248, 255)),
    ));
    let lane = world.spawn((UiNode::panel(1.0, 32.0)
        .child_of(panel)
        .width(UiLength::Percent(1.0))
        .layout(UiLayout::row(UiRect::ZERO, 8.0, UiAlign::Stretch)),));
    world.spawn((
        UiNode::panel(1.0, 1.0)
            .child_of(lane)
            .width(UiLength::Fill(1.0)),
        UiPanel::new(Color::rgba8(54, 78, 124, 238)),
    ));
    world.spawn((
        UiNode::panel(1.0, 1.0)
            .child_of(lane)
            .width(UiLength::Fill(2.0)),
        UiPanel::new(Color::rgba8(58, 138, 112, 238)),
    ));
    world.spawn((
        UiNode::panel(1.0, 1.0)
            .child_of(lane)
            .width(UiLength::Fill(3.0)),
        UiPanel::new(Color::rgba8(148, 92, 126, 238)),
    ));
    let fill_a = spawn_bar(world, panel, "blue fill", accent_blue());
    let fill_b = spawn_bar(world, panel, "green fill", accent_green());
    let fill_c = spawn_bar(world, panel, "alarm fill", accent_orange());
    (fill_a, fill_b, fill_c)
}

fn spawn_anchor_lab(world: &mut World, parent: EntityId) -> EntityId {
    let panel = world.spawn((
        UiNode::panel(1.0, 96.0)
            .child_of(parent)
            .width(UiLength::Percent(1.0))
            .z(1),
        UiPanel::new(Color::rgba8(16, 26, 30, 218)),
    ));
    world.spawn((
        UiNode::panel(154.0, 24.0).child_of(panel).at(18.0, 8.0),
        UiText::new("Anchor Cards").size(18.0),
    ));
    for (label, anchor, x, y, color) in [
        (
            "TL",
            UiAnchor::TopLeft,
            18.0,
            36.0,
            Color::rgba8(68, 96, 170, 232),
        ),
        (
            "TR",
            UiAnchor::TopRight,
            18.0,
            36.0,
            Color::rgba8(62, 156, 122, 232),
        ),
        (
            "BL",
            UiAnchor::BottomLeft,
            18.0,
            10.0,
            Color::rgba8(174, 94, 118, 232),
        ),
        (
            "BR",
            UiAnchor::BottomRight,
            18.0,
            10.0,
            Color::rgba8(176, 136, 72, 232),
        ),
    ] {
        let card = world.spawn((
            UiNode::panel(58.0, 24.0)
                .child_of(panel)
                .anchor(anchor)
                .at(x, y),
            UiPanel::new(color),
        ));
        world.spawn((
            UiNode::new()
                .child_of(card)
                .anchor(UiAnchor::Stretch)
                .at(0.0, 2.0),
            UiText::new(label).size(15.0).align(UiAlign::Center),
        ));
    }
    panel
}

fn spawn_oddities(world: &mut World, parent: EntityId) -> (EntityId, EntityId) {
    let panel = world.spawn((
        UiNode::panel(1.0, 168.0)
            .child_of(parent)
            .width(UiLength::Percent(1.0))
            .z(4),
        UiPanel::new(Color::rgba8(26, 20, 34, 224)),
    ));
    world.spawn((
        UiNode::panel(214.0, 28.0).child_of(panel).at(18.0, 14.0),
        UiText::new("Oddities Row").size(22.0),
    ));
    let disabled_panel = world.spawn((
        UiNode::panel(192.0, 92.0)
            .child_of(panel)
            .at(20.0, 72.0)
            .z(2),
        UiPanel::new(Color::rgba8(42, 82, 98, 184)),
    ));
    world.spawn((
        UiNode::panel(150.0, 38.0)
            .id("locked_button")
            .child_of(disabled_panel)
            .at(20.0, 30.0),
        UiButton::new("May Disable"),
    ));
    let secret_panel = world.spawn((
        UiNode::panel(176.0, 92.0)
            .child_of(panel)
            .at(246.0, 72.0)
            .z(3),
        UiPanel::new(Color::rgba8(96, 54, 116, 218)),
    ));
    world.spawn((
        UiNode::panel(132.0, 28.0)
            .child_of(secret_panel)
            .at(20.0, 18.0),
        UiText::new("Secret").size(20.0).align(UiAlign::Center),
    ));
    world.spawn((
        UiNode::panel(132.0, 30.0)
            .id("secret_button")
            .child_of(secret_panel)
            .at(20.0, 50.0),
        UiButton::new("Ghost Hit"),
    ));
    let dense = world.spawn((
        UiNode::panel(226.0, 126.0)
            .child_of(panel)
            .at(456.0, 44.0)
            .z(4)
            .layout(UiLayout::column(
                UiRect::new(12.0, 8.0, 12.0, 8.0),
                5.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(36, 42, 50, 236)),
        UiScroll::vertical().wheel_speed(24.0),
    ));
    for i in 0..10 {
        world.spawn((
            UiNode::panel(1.0, 20.0)
                .child_of(dense)
                .width(UiLength::Percent(1.0)),
            UiText::new(format!("dense label row {:02}", i + 1))
                .size(14.0)
                .color(Color::rgba8(190, 204, 218, 255)),
        ));
    }
    (disabled_panel, secret_panel)
}

fn spawn_slider(
    world: &mut World,
    parent: EntityId,
    id: &'static str,
    label: &'static str,
    value: f32,
    color: Color,
) -> EntityId {
    let row = world.spawn((UiNode::panel(1.0, 30.0)
        .child_of(parent)
        .width(UiLength::Percent(1.0))
        .layout(UiLayout::row(UiRect::ZERO, 10.0, UiAlign::Center)),));
    world.spawn((
        UiNode::panel(74.0, 28.0).child_of(row),
        UiText::new(label)
            .size(15.0)
            .color(Color::rgba8(184, 198, 212, 255)),
    ));
    world.spawn((
        UiNode::panel(1.0, 24.0)
            .id(UiId::new(id))
            .child_of(row)
            .width(UiLength::Fill(1.0)),
        UiSlider {
            fill_color: color,
            ..UiSlider::new(value, 0.0, 1.0).with_step(0.01)
        },
    ))
}

fn spawn_toggle(
    world: &mut World,
    parent: EntityId,
    id: &'static str,
    toggle: UiToggle,
) -> EntityId {
    world.spawn((
        UiNode::panel(1.0, 28.0)
            .id(UiId::new(id))
            .child_of(parent)
            .width(UiLength::Percent(1.0)),
        toggle,
    ))
}

fn spawn_bar(world: &mut World, parent: EntityId, label: &'static str, color: Color) -> EntityId {
    let row = world.spawn((UiNode::panel(1.0, 20.0)
        .child_of(parent)
        .width(UiLength::Percent(1.0))
        .layout(UiLayout::row(UiRect::ZERO, 8.0, UiAlign::Center)),));
    world.spawn((
        UiNode::panel(78.0, 18.0).child_of(row),
        UiText::new(label)
            .size(14.0)
            .color(Color::rgba8(186, 200, 216, 255)),
    ));
    world.spawn((
        UiNode::panel(1.0, 12.0)
            .child_of(row)
            .width(UiLength::Fill(1.0)),
        UiProgressBar {
            value: 0.25,
            max: 1.0,
            fill_color: color,
            background_color: Color::rgba8(12, 18, 24, 230),
        },
    ))
}

fn recolor_panel(world: &mut World, entity: EntityId, color: Color) {
    if let Some(panel) = world.get_mut::<UiPanel>(entity) {
        panel.color = color;
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn accent_blue() -> Color {
    Color::rgba8(84, 164, 248, 255)
}

fn accent_green() -> Color {
    Color::rgba8(72, 216, 154, 255)
}

fn accent_orange() -> Color {
    Color::rgba8(255, 170, 80, 255)
}

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("SkyEngine - Weird UI Lab", WINDOW_W, WINDOW_H)
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

    App::new(world).run(WeirdUiLab::default());
}
