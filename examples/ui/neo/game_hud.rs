//! Game HUD slice built with `ui-neo`.
//!
//! The layout is an original fantasy RPG mock UI intended to pressure-test
//! Neo UI for game HUD work: layered HUD chrome, party selection, command
//! buttons, status bars, and an inventory sheet.
//!
//! ```bash
//! cargo run --example ui_neo_game_hud --features ui-neo --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteFeature,
    Transform, TransparentPhase,
};
use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{
    AnimProperty, Color, Ease, HorizontalAlign, NeoState, Transition, Ui, VerticalAlign,
};

const WINDOW_W: u32 = 1280;
const WINDOW_H: u32 = 720;

struct NeoGameHudDemo {
    time: f32,
    state: NeoState<GameUiState>,
    screenshot: ScreenshotProbe,
}

#[derive(Debug)]
struct GameUiState {
    page: i32,
    active_party: usize,
    combo: u32,
    skill_pulse: bool,
}

#[derive(Debug, Clone)]
struct GameSnapshot {
    page: i32,
    active_party: usize,
    combo: u32,
    skill_pulse: bool,
}

impl Default for NeoGameHudDemo {
    fn default() -> Self {
        Self {
            time: 0.0,
            state: NeoState::new(GameUiState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl Default for GameUiState {
    fn default() -> Self {
        Self {
            page: env_i32("SKY_NEO_GAME_PAGE").unwrap_or(0).clamp(0, 1),
            active_party: 0,
            combo: 0,
            skill_pulse: false,
        }
    }
}

impl From<&GameUiState> for GameSnapshot {
    fn from(value: &GameUiState) -> Self {
        Self {
            page: value.page,
            active_party: value.active_party,
            combo: value.combo,
            skill_pulse: value.skill_pulse,
        }
    }
}

impl AppState for NeoGameHudDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: sky_engine::render::Color::new(0.040, 0.055, 0.070, 1.0),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(WINDOW_H as f32),
            MainCamera,
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.time += ctx.dt();
        let state = self.state.clone();
        let snapshot = self.state.read(|state| GameSnapshot::from(state));
        let title_page = snapshot.page;
        let draw_snapshot = snapshot.clone();
        let time = self.time;

        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            draw_demo(
                ui,
                screen.width,
                screen.height,
                time,
                &state,
                &draw_snapshot,
            );
        });

        ctx.set_title(if title_page == 0 {
            "SkyEngine Neo Game HUD"
        } else {
            "SkyEngine Neo Game HUD - Inventory"
        });
        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        ctx.request_redraw();
    }
}

fn draw_demo(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    time: f32,
    state: &NeoState<GameUiState>,
    snapshot: &GameSnapshot,
) {
    draw_scene_backdrop(ui, screen_w, screen_h, time);

    ui.stack("hud.root")
        .size(screen_w, screen_h)
        .padding(24.0)
        .content(|ui| {
            draw_view_switch(ui, screen_w, state, snapshot.page);
            if snapshot.page == 0 {
                draw_hud(ui, screen_w, screen_h, time, state, snapshot);
            } else {
                draw_inventory(ui, screen_w, screen_h, state, snapshot);
            }
        });
}

fn draw_scene_backdrop(ui: &mut Ui, screen_w: f32, screen_h: f32, time: f32) {
    ui.rect("scene.sky")
        .size(screen_w, screen_h)
        .gradient(c(0.070, 0.120, 0.170, 1.0), c(0.260, 0.350, 0.360, 1.0))
        .build();

    let sun_x = screen_w * 0.63 + time.sin() * 8.0;
    ui.rect("scene.sun.glow")
        .position(sun_x - 88.0, screen_h * 0.15 - 88.0)
        .size(176.0, 176.0)
        .radius(88.0)
        .color(c(0.980, 0.700, 0.270, 0.18))
        .shadow(42.0, 0.0, 0.0, c(0.980, 0.630, 0.240, 0.20))
        .build();
    ui.rect("scene.sun")
        .position(sun_x - 34.0, screen_h * 0.15 - 34.0)
        .size(68.0, 68.0)
        .radius(34.0)
        .color(c(1.000, 0.820, 0.430, 0.72))
        .build();

    ui.polygon("scene.mountain.back")
        .points([
            [0.0, screen_h * 0.62],
            [screen_w * 0.24, screen_h * 0.30],
            [screen_w * 0.48, screen_h * 0.62],
            [screen_w * 0.72, screen_h * 0.36],
            [screen_w, screen_h * 0.65],
            [screen_w, screen_h],
            [0.0, screen_h],
        ])
        .color(c(0.110, 0.170, 0.180, 0.88))
        .build();
    ui.polygon("scene.mountain.front")
        .points([
            [0.0, screen_h * 0.76],
            [screen_w * 0.18, screen_h * 0.49],
            [screen_w * 0.37, screen_h * 0.75],
            [screen_w * 0.58, screen_h * 0.47],
            [screen_w * 0.83, screen_h * 0.77],
            [screen_w, screen_h * 0.58],
            [screen_w, screen_h],
            [0.0, screen_h],
        ])
        .color(c(0.060, 0.115, 0.115, 0.95))
        .build();
    ui.rect("scene.ground")
        .position(0.0, screen_h * 0.78)
        .size(screen_w, screen_h * 0.22)
        .gradient(c(0.100, 0.160, 0.120, 1.0), c(0.045, 0.075, 0.060, 1.0))
        .build();
}

fn draw_view_switch(ui: &mut Ui, screen_w: f32, state: &NeoState<GameUiState>, page: i32) {
    let switch_state = state.clone();
    ui.stack("switch.wrap")
        .position((screen_w - 292.0) * 0.5, 24.0)
        .size(292.0, 46.0)
        .content(|ui| {
            widgets::segmented(ui, "switch")
                .size(292.0, 46.0)
                .items(["HUD", "Inventory"])
                .selected(page)
                .on_change(move |value| {
                    switch_state.update(|state| state.page = value.clamp(0, 1));
                })
                .build();
        });
}

fn draw_hud(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    _time: f32,
    state: &NeoState<GameUiState>,
    snapshot: &GameSnapshot,
) {
    draw_minimap(ui);
    draw_quest_panel(ui, screen_h);
    draw_party(ui, screen_w, state, snapshot.active_party);
    draw_status(ui, screen_w, screen_h, snapshot);
    draw_commands(ui, screen_w, screen_h, state, snapshot);
}

fn draw_minimap(ui: &mut Ui) {
    ui.stack("minimap")
        .position(24.0, 28.0)
        .size(170.0, 170.0)
        .content(|ui| {
            ui.rect("minimap.shadow")
                .fill()
                .radius(85.0)
                .color(c(0.030, 0.040, 0.050, 0.54))
                .shadow(28.0, 0.0, 12.0, c(0.0, 0.0, 0.0, 0.30))
                .build();
            ui.rect("minimap.map")
                .position(10.0, 10.0)
                .size(150.0, 150.0)
                .radius(75.0)
                .gradient(c(0.160, 0.300, 0.270, 0.92), c(0.070, 0.120, 0.140, 0.96))
                .border(2.0, c(0.870, 0.760, 0.500, 0.76))
                .clip()
                .build();
            ui.rect("minimap.road.a")
                .position(42.0, 74.0)
                .size(88.0, 8.0)
                .radius(4.0)
                .color(c(0.830, 0.720, 0.480, 0.50))
                .rotate(0.45)
                .build();
            ui.rect("minimap.road.b")
                .position(70.0, 40.0)
                .size(9.0, 94.0)
                .radius(5.0)
                .color(c(0.830, 0.720, 0.480, 0.40))
                .rotate(-0.18)
                .build();
            ui.rect("minimap.marker")
                .position(80.0, 76.0)
                .size(18.0, 18.0)
                .radius(9.0)
                .color(c(0.210, 0.720, 1.0, 0.96))
                .shadow(14.0, 0.0, 0.0, c(0.210, 0.720, 1.0, 0.60))
                .build();
            ui.text("minimap.north")
                .position(75.0, 10.0)
                .size(20.0, 18.0)
                .text("N")
                .font_size(15.0)
                .line_height(18.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.980, 0.930, 0.760, 1.0))
                .build();
        });
}

fn draw_quest_panel(ui: &mut Ui, screen_h: f32) {
    ui.stack("quest")
        .position(24.0, screen_h - 258.0)
        .size(310.0, 190.0)
        .content(|ui| {
            glass_panel(ui, "quest.bg", 310.0, 190.0, 16.0);
            ui.text("quest.title")
                .position(18.0, 16.0)
                .size(260.0, 24.0)
                .text("WINDWARD ERRAND")
                .font_size(16.0)
                .line_height(24.0)
                .color(c(0.970, 0.900, 0.650, 1.0))
                .build();
            quest_line(ui, "quest.a", 50.0, "Reach the old bridge", true);
            quest_line(ui, "quest.b", 82.0, "Collect 3 moon herbs", false);
            quest_line(ui, "quest.c", 114.0, "Report to the guild", false);
            ui.text("quest.reward")
                .position(18.0, 150.0)
                .size(260.0, 22.0)
                .text("Reward  1200 Mora  |  40 EXP")
                .font_size(14.0)
                .line_height(22.0)
                .color(c(0.760, 0.880, 0.840, 1.0))
                .build();
        });
}

fn quest_line(ui: &mut Ui, id: &str, y: f32, text: &str, active: bool) {
    ui.rect(format!("{id}.dot"))
        .position(20.0, y + 6.0)
        .size(9.0, 9.0)
        .radius(5.0)
        .color(if active {
            c(0.300, 0.880, 1.000, 1.0)
        } else {
            c(0.720, 0.740, 0.700, 0.72)
        })
        .build();
    ui.text(format!("{id}.text"))
        .position(38.0, y)
        .size(250.0, 22.0)
        .text(text)
        .font_size(14.0)
        .line_height(22.0)
        .color(if active {
            c(0.930, 0.980, 1.000, 1.0)
        } else {
            c(0.720, 0.750, 0.760, 0.86)
        })
        .build();
}

fn draw_party(ui: &mut Ui, screen_w: f32, state: &NeoState<GameUiState>, active: usize) {
    const MEMBERS: [(&str, &str, Color); 4] = [
        ("Astra", "Anemo", c(0.340, 0.900, 0.780, 1.0)),
        ("Lyra", "Pyro", c(1.000, 0.470, 0.300, 1.0)),
        ("Mira", "Hydro", c(0.270, 0.660, 1.000, 1.0)),
        ("Kade", "Geo", c(0.940, 0.720, 0.310, 1.0)),
    ];

    ui.column("party")
        .position(screen_w - 248.0, 112.0)
        .size(224.0, 300.0)
        .gap(10.0)
        .content(|ui| {
            for (index, (name, element, accent)) in MEMBERS.iter().enumerate() {
                let selected = index == active;
                let state_for_click = state.clone();
                let id = format!("party.{index}");
                ui.stack(id.clone()).size(224.0, 62.0).content(|ui| {
                    ui.rect(format!("{id}.hit"))
                        .fill()
                        .states(
                            if selected {
                                c(0.980, 0.880, 0.580, 0.24)
                            } else {
                                c(0.020, 0.030, 0.040, 0.40)
                            },
                            c(0.180, 0.260, 0.290, 0.72),
                            c(0.090, 0.130, 0.150, 0.90),
                        )
                        .radius(31.0)
                        .border(
                            if selected { 2.0 } else { 1.0 },
                            if selected {
                                c(0.980, 0.840, 0.520, 0.90)
                            } else {
                                c(0.620, 0.720, 0.740, 0.30)
                            },
                        )
                        .shadow(18.0, 0.0, 7.0, c(0.0, 0.0, 0.0, 0.22))
                        .transition(Transition::ease(0.16, Ease::OutCubic))
                        .animate(AnimProperty::COLOR | AnimProperty::BORDER)
                        .on_click(move || {
                            state_for_click.update(|state| state.active_party = index);
                        })
                        .build();
                    ui.rect(format!("{id}.avatar"))
                        .position(8.0, 8.0)
                        .size(46.0, 46.0)
                        .radius(23.0)
                        .gradient(*accent, c(0.070, 0.080, 0.090, 1.0))
                        .border(1.0, c(1.0, 1.0, 1.0, 0.34))
                        .build();
                    ui.text(format!("{id}.initial"))
                        .position(8.0, 17.0)
                        .size(46.0, 24.0)
                        .text(&name[0..1])
                        .font_size(20.0)
                        .line_height(24.0)
                        .horizontal_align(HorizontalAlign::Center)
                        .color(c(0.980, 0.990, 1.000, 1.0))
                        .build();
                    ui.text(format!("{id}.name"))
                        .position(66.0, 11.0)
                        .size(116.0, 24.0)
                        .text(*name)
                        .font_size(18.0)
                        .line_height(24.0)
                        .color(c(0.960, 0.980, 0.980, 1.0))
                        .build();
                    ui.text(format!("{id}.element"))
                        .position(66.0, 34.0)
                        .size(100.0, 18.0)
                        .text(*element)
                        .font_size(12.0)
                        .line_height(18.0)
                        .color(*accent)
                        .build();
                    ui.text(format!("{id}.key"))
                        .position(186.0, 17.0)
                        .size(24.0, 24.0)
                        .text((index + 1).to_string())
                        .font_size(15.0)
                        .line_height(24.0)
                        .horizontal_align(HorizontalAlign::Center)
                        .color(c(0.940, 0.880, 0.650, 0.95))
                        .build();
                });
            }
        });
}

fn draw_status(ui: &mut Ui, screen_w: f32, screen_h: f32, snapshot: &GameSnapshot) {
    let hp = [0.86, 0.72, 0.94, 0.61][snapshot.active_party.min(3)];
    let energy = 0.34 + (snapshot.combo.min(10) as f32 * 0.045);
    ui.stack("status")
        .position((screen_w - 520.0) * 0.5, screen_h - 112.0)
        .size(520.0, 74.0)
        .content(|ui| {
            glass_panel(ui, "status.bg", 520.0, 74.0, 18.0);
            ui.text("status.name")
                .position(22.0, 11.0)
                .size(160.0, 24.0)
                .text(match snapshot.active_party {
                    1 => "Lyra",
                    2 => "Mira",
                    3 => "Kade",
                    _ => "Astra",
                })
                .font_size(18.0)
                .line_height(24.0)
                .color(c(0.980, 0.940, 0.740, 1.0))
                .build();
            stat_bar(
                ui,
                "hp",
                104.0,
                18.0,
                360.0,
                14.0,
                hp,
                c(0.300, 0.940, 0.520, 1.0),
            );
            stat_bar(
                ui,
                "energy",
                104.0,
                42.0,
                260.0,
                9.0,
                energy,
                c(0.300, 0.720, 1.000, 1.0),
            );
            ui.text("status.level")
                .position(402.0, 38.0)
                .size(92.0, 20.0)
                .text("Lv. 70")
                .font_size(14.0)
                .line_height(20.0)
                .horizontal_align(HorizontalAlign::Right)
                .color(c(0.730, 0.810, 0.820, 1.0))
                .build();
        });
}

fn stat_bar(
    ui: &mut Ui,
    id: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    value: f32,
    color: Color,
) {
    ui.rect(format!("{id}.track"))
        .position(x, y)
        .size(width, height)
        .radius(height * 0.5)
        .color(c(0.020, 0.030, 0.035, 0.64))
        .build();
    ui.rect(format!("{id}.fill"))
        .position(x, y)
        .size(width * value.clamp(0.0, 1.0), height)
        .radius(height * 0.5)
        .color(color)
        .shadow(10.0, 0.0, 0.0, color.with_alpha(0.28))
        .transition(Transition::ease(0.20, Ease::OutCubic))
        .animate(AnimProperty::FRAME)
        .build();
}

fn draw_commands(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    state: &NeoState<GameUiState>,
    snapshot: &GameSnapshot,
) {
    ui.stack("commands")
        .position(screen_w - 310.0, screen_h - 210.0)
        .size(286.0, 172.0)
        .content(|ui| {
            command_button(ui, "attack", 98.0, 58.0, 76.0, "LMB", "ATK", false, state);
            command_button(
                ui,
                "skill",
                16.0,
                62.0,
                70.0,
                "E",
                "SKILL",
                snapshot.skill_pulse,
                state,
            );
            command_button(
                ui,
                "burst",
                177.0,
                34.0,
                86.0,
                "Q",
                "BURST",
                snapshot.combo > 3,
                state,
            );
            command_button(
                ui, "dash", 122.0, 132.0, 50.0, "Shift", "Dash", false, state,
            );
            ui.text("combo")
                .position(88.0, 10.0)
                .size(110.0, 26.0)
                .text(format!("Combo x{}", snapshot.combo.max(1)))
                .font_size(15.0)
                .line_height(26.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.980, 0.900, 0.650, 0.98))
                .build();
        });
}

fn command_button(
    ui: &mut Ui,
    id: &str,
    x: f32,
    y: f32,
    size: f32,
    key: &str,
    label: &str,
    ready: bool,
    state: &NeoState<GameUiState>,
) {
    let click_state = state.clone();
    let scale = if ready { 1.06 } else { 1.0 };
    ui.stack(id.to_string())
        .position(x, y)
        .size(size, size)
        .scale(scale)
        .transition(Transition::ease(0.18, Ease::OutBack))
        .animate(AnimProperty::TRANSFORM)
        .content(|ui| {
            ui.rect(format!("{id}.ring"))
                .fill()
                .states(
                    c(0.050, 0.065, 0.078, 0.72),
                    c(0.140, 0.200, 0.220, 0.86),
                    c(0.230, 0.180, 0.110, 0.96),
                )
                .radius(size * 0.5)
                .border(
                    2.0,
                    if ready {
                        c(0.990, 0.820, 0.360, 0.95)
                    } else {
                        c(0.740, 0.840, 0.820, 0.44)
                    },
                )
                .shadow(18.0, 0.0, 6.0, c(0.0, 0.0, 0.0, 0.30))
                .on_click(move || {
                    click_state.update(|state| {
                        state.combo = state.combo.saturating_add(1);
                        state.skill_pulse = !state.skill_pulse;
                    });
                })
                .build();
            ui.text(format!("{id}.key"))
                .position(0.0, size * 0.25)
                .size(size, 18.0)
                .text(key)
                .font_size(if size > 72.0 { 18.0 } else { 13.0 })
                .line_height(18.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.980, 0.960, 0.840, 1.0))
                .build();
            ui.text(format!("{id}.label"))
                .position(0.0, size * 0.52)
                .size(size, 18.0)
                .text(label)
                .font_size(if size > 72.0 { 12.0 } else { 10.0 })
                .line_height(18.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.730, 0.830, 0.850, 0.92))
                .build();
        });
}

fn draw_inventory(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    state: &NeoState<GameUiState>,
    snapshot: &GameSnapshot,
) {
    let panel_w = (screen_w - 96.0).min(980.0);
    let panel_h = (screen_h - 132.0).min(560.0);
    ui.stack("inventory")
        .position((screen_w - panel_w) * 0.5, 96.0)
        .size(panel_w, panel_h)
        .content(|ui| {
            glass_panel(ui, "inventory.bg", panel_w, panel_h, 24.0);
            ui.text("inventory.title")
                .position(30.0, 24.0)
                .size(320.0, 34.0)
                .text("TRAVELER SATCHEL")
                .font_size(25.0)
                .line_height(34.0)
                .color(c(0.980, 0.910, 0.690, 1.0))
                .build();
            ui.text("inventory.subtitle")
                .position(32.0, 59.0)
                .size(360.0, 22.0)
                .text("Prototype inventory surface composed in Neo UI")
                .font_size(14.0)
                .line_height(22.0)
                .color(c(0.720, 0.810, 0.820, 0.94))
                .build();
            draw_inventory_grid(ui, 32.0, 104.0);
            draw_item_detail(ui, panel_w - 318.0, 104.0, snapshot);

            ui.stack("inventory.back.wrap")
                .position(panel_w - 178.0, panel_h - 72.0)
                .size(140.0, 46.0)
                .content(|ui| {
                    let back_state = state.clone();
                    widgets::button(ui, "inventory.back")
                        .size(140.0, 46.0)
                        .text("Back to HUD")
                        .font_size(14.0)
                        .radius(12.0)
                        .colors(
                            c(0.620, 0.440, 0.200, 0.96),
                            c(0.760, 0.560, 0.270, 1.0),
                            c(0.420, 0.300, 0.150, 1.0),
                        )
                        .on_click(move || back_state.update(|state| state.page = 0))
                        .build();
                });
        });
}

fn draw_inventory_grid(ui: &mut Ui, x: f32, y: f32) {
    const ITEMS: [(&str, &str, Color); 15] = [
        ("Herb", "12", c(0.350, 0.820, 0.490, 1.0)),
        ("Ore", "7", c(0.410, 0.620, 0.820, 1.0)),
        ("Flame", "3", c(0.980, 0.430, 0.260, 1.0)),
        ("Shell", "9", c(0.820, 0.720, 0.520, 1.0)),
        ("Key", "1", c(0.960, 0.760, 0.310, 1.0)),
        ("Apple", "21", c(0.850, 0.220, 0.240, 1.0)),
        ("Tome", "2", c(0.610, 0.490, 0.900, 1.0)),
        ("Dew", "6", c(0.220, 0.760, 0.980, 1.0)),
        ("Mask", "4", c(0.720, 0.720, 0.760, 1.0)),
        ("Seed", "18", c(0.500, 0.720, 0.300, 1.0)),
        ("Gem", "5", c(0.380, 0.860, 0.900, 1.0)),
        ("Map", "1", c(0.760, 0.590, 0.400, 1.0)),
        ("Fish", "10", c(0.300, 0.560, 0.860, 1.0)),
        ("Horn", "2", c(0.870, 0.780, 0.620, 1.0)),
        ("Star", "1", c(1.000, 0.870, 0.380, 1.0)),
    ];

    let cell = 76.0;
    let gap = 12.0;
    for (index, (name, count, accent)) in ITEMS.iter().enumerate() {
        let col = index % 5;
        let row = index / 5;
        let id = format!("item.{index}");
        let px = x + col as f32 * (cell + gap);
        let py = y + row as f32 * (cell + gap);
        ui.stack(id.clone())
            .position(px, py)
            .size(cell, cell)
            .content(|ui| {
                ui.rect(format!("{id}.bg"))
                    .fill()
                    .states(
                        c(0.035, 0.050, 0.060, 0.70),
                        c(0.100, 0.150, 0.160, 0.92),
                        c(0.080, 0.110, 0.120, 1.0),
                    )
                    .radius(12.0)
                    .border(1.0, c(0.760, 0.820, 0.740, 0.22))
                    .build();
                ui.rect(format!("{id}.icon"))
                    .position(18.0, 11.0)
                    .size(40.0, 40.0)
                    .radius(12.0)
                    .gradient(*accent, c(0.040, 0.050, 0.060, 1.0))
                    .border(1.0, c(1.0, 1.0, 1.0, 0.24))
                    .build();
                ui.text(format!("{id}.name"))
                    .position(7.0, 53.0)
                    .size(62.0, 16.0)
                    .text(*name)
                    .font_size(11.0)
                    .line_height(16.0)
                    .horizontal_align(HorizontalAlign::Center)
                    .color(c(0.880, 0.910, 0.900, 1.0))
                    .build();
                ui.text(format!("{id}.count"))
                    .position(45.0, 7.0)
                    .size(24.0, 16.0)
                    .text(format!("x{count}"))
                    .font_size(10.0)
                    .line_height(16.0)
                    .horizontal_align(HorizontalAlign::Right)
                    .color(c(0.980, 0.910, 0.680, 1.0))
                    .build();
            });
    }
}

fn draw_item_detail(ui: &mut Ui, x: f32, y: f32, snapshot: &GameSnapshot) {
    ui.stack("detail")
        .position(x, y)
        .size(286.0, 340.0)
        .content(|ui| {
            ui.rect("detail.bg")
                .fill()
                .radius(18.0)
                .color(c(0.020, 0.030, 0.040, 0.38))
                .border(1.0, c(0.920, 0.820, 0.560, 0.24))
                .build();
            ui.rect("detail.icon")
                .position(93.0, 28.0)
                .size(100.0, 100.0)
                .radius(24.0)
                .gradient(c(0.340, 0.740, 0.900, 1.0), c(0.100, 0.100, 0.130, 1.0))
                .border(2.0, c(0.980, 0.850, 0.520, 0.54))
                .shadow(28.0, 0.0, 12.0, c(0.0, 0.0, 0.0, 0.26))
                .build();
            ui.text("detail.name")
                .position(28.0, 150.0)
                .size(230.0, 30.0)
                .text("Wind-Touched Gem")
                .font_size(20.0)
                .line_height(30.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.970, 0.930, 0.760, 1.0))
                .build();
            ui.text("detail.kind")
                .position(28.0, 184.0)
                .size(230.0, 22.0)
                .text("Quest Material")
                .font_size(13.0)
                .line_height(22.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.480, 0.820, 0.880, 1.0))
                .build();
            ui.text("detail.copy")
                .position(30.0, 224.0)
                .size(226.0, 68.0)
                .text("A luminous stone carried by highland winds. Useful for testing detail panels, wrapped text, and modal-like inventory surfaces.")
                .font_size(13.0)
                .line_height(18.0)
                .wrap(true)
                .horizontal_align(HorizontalAlign::Center)
                .vertical_align(VerticalAlign::Center)
                .color(c(0.740, 0.800, 0.810, 0.98))
                .build();
            ui.text("detail.combo")
                .position(28.0, 304.0)
                .size(230.0, 22.0)
                .text(format!("Command counter carried from HUD: {}", snapshot.combo))
                .font_size(12.0)
                .line_height(22.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.940, 0.760, 0.450, 0.95))
                .build();
        });
}

fn glass_panel(ui: &mut Ui, id: &str, width: f32, height: f32, radius: f32) {
    ui.rect(id.to_string())
        .size(width, height)
        .radius(radius)
        .gradient(c(0.030, 0.045, 0.055, 0.66), c(0.070, 0.095, 0.105, 0.54))
        .border(1.0, c(0.880, 0.820, 0.620, 0.28))
        .shadow(26.0, 0.0, 12.0, c(0.0, 0.0, 0.0, 0.28))
        .build();
}

const fn c(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

trait Alpha {
    fn with_alpha(self, alpha: f32) -> Self;
}

impl Alpha for Color {
    fn with_alpha(self, alpha: f32) -> Self {
        Color::new(self.r, self.g, self.b, alpha)
    }
}

#[derive(Debug)]
struct ScreenshotProbe {
    path: Option<String>,
    frame: u32,
    frame_count: u32,
    taken: bool,
    exit_after: bool,
}

impl Default for ScreenshotProbe {
    fn default() -> Self {
        Self {
            path: std::env::var("SKY_NEO_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_NEO_SCREENSHOT_FRAME").unwrap_or(30),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_NEO_EXIT_AFTER_SCREENSHOT"),
        }
    }
}

impl ScreenshotProbe {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        if !self.taken && self.frame_count >= self.frame {
            if let Some(path) = self.path.as_ref() {
                ctx.request_screenshot(path);
                self.taken = true;
                if self.exit_after {
                    ctx.request_exit();
                }
            }
        }
        self.frame_count = self.frame_count.saturating_add(1);
    }
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_i32(key: &str) -> Option<i32> {
    std::env::var(key).ok()?.parse().ok()
}

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("SkyEngine Neo Game HUD", WINDOW_W, WINDOW_H)
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

    App::new(world).run(NeoGameHudDemo::default());
}
