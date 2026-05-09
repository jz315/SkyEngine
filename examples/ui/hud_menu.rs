//! Native retained UI demo: menu, HUD, buttons, progress bars, and text.
//!
//! ```bash
//! cargo run --example hud_menu --features ui --release
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
use sky_engine::ecs::{EntityId, World};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings,
    SpriteFeature, Transform, TransparentPhase,
};
use sky_engine::ui::{
    UiAlign, UiAnchor, UiButton, UiEventKind, UiEvents, UiId, UiLayout, UiLength, UiNode, UiPanel,
    UiProgressBar, UiRect, UiSlider, UiText, UiToggle,
};

const WINDOW_W: u32 = 960;
const WINDOW_H: u32 = 600;

#[derive(Default)]
struct HudMenuDemo {
    ui: Option<UiRefs>,
    running: bool,
    health: f32,
    energy: f32,
    volume: f32,
    threat: f32,
    assist: bool,
    reduced_motion: bool,
    score: u32,
    pulse: f32,
}

#[derive(Clone, Copy)]
struct UiRefs {
    title_panel: EntityId,
    status_text: EntityId,
    health_bar: EntityId,
    energy_bar: EntityId,
    volume_slider: EntityId,
    threat_slider: EntityId,
    assist_toggle: EntityId,
    motion_toggle: EntityId,
    menu_panel: EntityId,
    hint_text: EntityId,
}

impl AppState for HudMenuDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.028, 0.035, 0.045),
            ..Default::default()
        });
        world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(600.0),
            MainCamera,
        ));
        self.health = 0.82;
        self.energy = 0.45;
        self.volume = 0.65;
        self.threat = 0.35;
        self.assist = true;
        self.reduced_motion = false;
        self.ui = Some(spawn_ui(world));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.ui().update();
        self.handle_ui_events(ctx);
        self.animate(ctx.dt);
        self.sync_ui(ctx.world);

        ctx.render();
        ctx.ui().render_overlays();
    }
}

impl HudMenuDemo {
    fn handle_ui_events(&mut self, ctx: &mut FrameContext<'_>) {
        let mut clicked = Vec::new();
        if let Some(events) = ctx.world.get_resource_mut::<UiEvents>() {
            for event in events.drain() {
                if event.kind == UiEventKind::Clicked {
                    clicked.push(event.id);
                }
            }
        }

        for id in clicked.into_iter().flatten() {
            match id.as_str() {
                "start" | "resume" => self.running = true,
                "damage" => {
                    self.running = true;
                    self.health = (self.health - 0.13).max(0.0);
                }
                "charge" => {
                    self.running = true;
                    self.energy = (self.energy + 0.18).min(1.0);
                    self.score += 25;
                }
                "pause" => self.running = false,
                _ => {}
            }
        }
    }

    fn animate(&mut self, dt: f32) {
        self.pulse += dt;
        if self.running {
            self.energy = (self.energy + dt * 0.12).min(1.0);
            if self.energy >= 1.0 {
                self.energy = 0.18;
                self.score += 10;
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
        self.threat = world
            .get::<UiSlider>(ui.threat_slider)
            .map(|slider| slider.value)
            .unwrap_or(self.threat);
        self.assist = world
            .get::<UiToggle>(ui.assist_toggle)
            .map(|toggle| toggle.checked)
            .unwrap_or(self.assist);
        self.reduced_motion = world
            .get::<UiToggle>(ui.motion_toggle)
            .map(|toggle| toggle.checked)
            .unwrap_or(self.reduced_motion);
        if let Some(text) = world.get_mut::<UiText>(ui.status_text) {
            text.text = format!(
                "score {:04}   hp {:>3}%   energy {:>3}%   vol {:>3}%   threat {:>3}%   assist {}",
                self.score,
                (self.health * 100.0).round() as i32,
                (self.energy * 100.0).round() as i32,
                (self.volume * 100.0).round() as i32,
                (self.threat * 100.0).round() as i32,
                if self.assist { "on" } else { "off" }
            );
        }
        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.health_bar) {
            bar.value = self.health;
            bar.fill_color = if self.health > 0.35 {
                Color::rgba8(90, 224, 145, 255)
            } else {
                Color::rgba8(230, 86, 86, 255)
            };
        }
        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.energy_bar) {
            bar.value = self.energy;
        }
        if let Some(node) = world.get_mut::<UiNode>(ui.menu_panel) {
            node.visible = !self.running;
            node.enabled = !self.running;
        }
        if let Some(node) = world.get_mut::<UiNode>(ui.title_panel) {
            let wobble = if self.reduced_motion { 0.0 } else { 1.5 };
            node.position[1] = 16.0 + self.pulse.sin() * wobble;
        }
        if let Some(text) = world.get_mut::<UiText>(ui.hint_text) {
            text.text = if self.running {
                "buttons are live UI; hover/click events do not touch game input".to_string()
            } else {
                "click Start to enter the HUD; menu visibility is just ECS state".to_string()
            };
        }
    }
}

fn spawn_ui(world: &mut World) -> UiRefs {
    let title_panel = world.spawn((
        UiNode::panel(420.0, 64.0)
            .anchor(UiAnchor::TopLeft)
            .at(18.0, 16.0)
            .z(10),
        UiPanel::new(Color::rgba8(12, 18, 28, 220)),
    ));
    world.spawn((
        UiNode::new()
            .child_of(title_panel)
            .anchor(UiAnchor::Stretch)
            .at(18.0, 12.0)
            .z(11),
        UiText::new("SkyEngine Native UI").size(24.0),
    ));

    let status_text = world.spawn((
        UiNode::panel(690.0, 30.0)
            .anchor(UiAnchor::TopLeft)
            .at(24.0, 98.0)
            .z(10),
        UiText::new("")
            .size(18.0)
            .color(Color::rgba8(220, 232, 242, 255)),
    ));

    let bars = world.spawn((
        UiNode::panel(320.0, 76.0)
            .anchor(UiAnchor::TopRight)
            .at(22.0, 22.0)
            .z(10)
            .layout(UiLayout::column(
                UiRect::new(14.0, 12.0, 14.0, 12.0),
                10.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(14, 20, 30, 216)),
    ));
    let health_bar = world.spawn((
        UiNode::panel(1.0, 18.0)
            .child_of(bars)
            .width(UiLength::Percent(1.0)),
        UiProgressBar::new(0.82, 1.0),
    ));
    let energy_bar = world.spawn((
        UiNode::panel(1.0, 18.0)
            .child_of(bars)
            .width(UiLength::Percent(1.0)),
        UiProgressBar {
            value: 0.45,
            max: 1.0,
            fill_color: Color::rgba8(84, 164, 248, 255),
            background_color: Color::rgba8(24, 30, 40, 230),
        },
    ));

    let menu_panel = world.spawn((
        UiNode::panel(382.0, 452.0)
            .id("menu")
            .anchor(UiAnchor::Center)
            .z(40)
            .layout(UiLayout::column(
                UiRect::new(24.0, 22.0, 24.0, 22.0),
                10.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(15, 20, 31, 238)),
    ));
    world.spawn((
        UiNode::panel(1.0, 40.0)
            .child_of(menu_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("Tactical HUD")
            .size(26.0)
            .align(UiAlign::Center)
            .color(Color::rgba8(244, 248, 252, 255)),
    ));
    let volume_slider = spawn_slider_row(
        world,
        menu_panel,
        "volume",
        "Volume",
        UiSlider {
            fill_color: Color::rgba8(84, 164, 248, 255),
            ..UiSlider::new(0.65, 0.0, 1.0).with_step(0.05)
        },
    );
    let threat_slider = spawn_slider_row(
        world,
        menu_panel,
        "threat",
        "Threat",
        UiSlider {
            fill_color: Color::rgba8(244, 142, 73, 255),
            pressed_thumb_color: Color::rgba8(255, 214, 176, 255),
            ..UiSlider::new(0.35, 0.0, 1.0).with_step(0.05)
        },
    );
    let assist_toggle = spawn_toggle_row(
        world,
        menu_panel,
        "assist",
        UiToggle::new(true).label("Aim Assist"),
    );
    let motion_toggle = spawn_toggle_row(
        world,
        menu_panel,
        "reduced_motion",
        UiToggle::new(false).label("Reduced Motion"),
    );
    spawn_button(world, menu_panel, "start", "Start");
    spawn_button(world, menu_panel, "damage", "Take Damage");
    spawn_button(world, menu_panel, "charge", "Charge + Score");
    spawn_button(world, menu_panel, "pause", "Pause");

    let hint_text = world.spawn((
        UiNode::panel(680.0, 28.0)
            .anchor(UiAnchor::BottomLeft)
            .at(24.0, 22.0)
            .z(10),
        UiText::new("")
            .size(16.0)
            .color(Color::rgba8(168, 184, 200, 255)),
    ));

    UiRefs {
        title_panel,
        status_text,
        health_bar,
        energy_bar,
        volume_slider,
        threat_slider,
        assist_toggle,
        motion_toggle,
        menu_panel,
        hint_text,
    }
}

fn spawn_slider_row(
    world: &mut World,
    parent: EntityId,
    id: &'static str,
    label: &'static str,
    slider: UiSlider,
) -> EntityId {
    let row = world.spawn((UiNode::panel(1.0, 30.0)
        .child_of(parent)
        .width(UiLength::Percent(1.0))
        .layout(UiLayout::row(
            UiRect::new(0.0, 0.0, 0.0, 0.0),
            10.0,
            UiAlign::Center,
        )),));
    world.spawn((
        UiNode::panel(82.0, 30.0).child_of(row),
        UiText::new(label)
            .size(16.0)
            .color(Color::rgba8(188, 204, 218, 255)),
    ));
    world.spawn((
        UiNode::panel(1.0, 24.0)
            .id(UiId::new(id))
            .child_of(row)
            .width(UiLength::Fill(1.0)),
        slider,
    ))
}

fn spawn_toggle_row(
    world: &mut World,
    parent: EntityId,
    id: &'static str,
    toggle: UiToggle,
) -> EntityId {
    world.spawn((
        UiNode::panel(1.0, 30.0)
            .id(UiId::new(id))
            .child_of(parent)
            .width(UiLength::Percent(1.0)),
        toggle,
    ))
}

fn spawn_button(world: &mut World, parent: EntityId, id: &'static str, label: &'static str) {
    world.spawn((
        UiNode::panel(1.0, 38.0)
            .id(UiId::new(id))
            .child_of(parent)
            .width(UiLength::Percent(1.0)),
        UiButton::new(label),
    ));
}

fn main() {
    App::new(
        AppConfig::new("SkyEngine - Native UI", WINDOW_W, WINDOW_H)
            .with_vsync(false)
            .with_resizable(true),
        World::new(),
    )
    .with_render_pipeline(
        RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build(),
    )
    .run(HudMenuDemo::default());
}
