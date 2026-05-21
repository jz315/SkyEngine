//! Kenney UI pack showcase built with `ui-neo`.
//!
//! ```bash
//! cargo run --example kenney_neo_ui_showcase --features ui-neo --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, Color as RenderColor, MainCamera, Projection, RenderPipelineAsset,
    RenderSettings, SpriteFeature, Transform, TransparentPhase,
};
use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::NeoUiPlugin;
use sky_engine::ui::neo::{
    AnimProperty, Color, Ease, HorizontalAlign, NeoState, Ui, VerticalAlign,
};

const WINDOW_W: u32 = 1280;
const WINDOW_H: u32 = 720;

const FONT: &str = "examples/assets/kenney_ui_pack/Font/Kenney Future.ttf";
const BLUE_BUTTON: &str =
    "examples/assets/kenney_ui_pack/PNG/Blue/Default/button_rectangle_depth_gloss.png";
const GREEN_BUTTON: &str =
    "examples/assets/kenney_ui_pack/PNG/Green/Default/button_rectangle_depth_gloss.png";
const RED_BUTTON: &str =
    "examples/assets/kenney_ui_pack/PNG/Red/Default/button_rectangle_depth_gloss.png";
const YELLOW_BUTTON: &str =
    "examples/assets/kenney_ui_pack/PNG/Yellow/Default/button_rectangle_depth_gloss.png";
const GREY_BUTTON: &str =
    "examples/assets/kenney_ui_pack/PNG/Grey/Default/button_rectangle_depth_gloss.png";
const BLUE_SQUARE: &str =
    "examples/assets/kenney_ui_pack/PNG/Blue/Default/button_square_depth_gloss.png";
const GREEN_SQUARE: &str =
    "examples/assets/kenney_ui_pack/PNG/Green/Default/button_square_depth_gloss.png";
const RED_SQUARE: &str =
    "examples/assets/kenney_ui_pack/PNG/Red/Default/button_square_depth_gloss.png";
const YELLOW_SQUARE: &str =
    "examples/assets/kenney_ui_pack/PNG/Yellow/Default/button_square_depth_gloss.png";
const STAR: &str = "examples/assets/kenney_ui_pack/PNG/Yellow/Default/star.png";
const STAR_OUTLINE: &str =
    "examples/assets/kenney_ui_pack/PNG/Yellow/Default/star_outline_depth.png";
const CHECK_ON: &str =
    "examples/assets/kenney_ui_pack/PNG/Green/Default/check_square_color_checkmark.png";
const CHECK_OFF: &str =
    "examples/assets/kenney_ui_pack/PNG/Grey/Default/check_square_grey_square.png";
const DIVIDER: &str = "examples/assets/kenney_ui_pack/PNG/Extra/Default/divider.png";
const INPUT: &str = "examples/assets/kenney_ui_pack/PNG/Extra/Default/input_rectangle.png";
const INPUT_OUTLINE: &str =
    "examples/assets/kenney_ui_pack/PNG/Extra/Default/input_outline_rectangle.png";
const ICON_PLAY: &str = "examples/assets/kenney_ui_pack/PNG/Extra/Default/icon_play_light.png";
const ICON_REPEAT: &str = "examples/assets/kenney_ui_pack/PNG/Extra/Default/icon_repeat_light.png";
const ICON_UP: &str = "examples/assets/kenney_ui_pack/PNG/Extra/Default/icon_arrow_up_light.png";
const ICON_DOWN: &str =
    "examples/assets/kenney_ui_pack/PNG/Extra/Default/icon_arrow_down_light.png";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DemoPage {
    Title,
    Hud,
    Loadout,
    Settings,
}

#[derive(Debug)]
struct DemoState {
    page: DemoPage,
    selected_slot: usize,
    health: f32,
    stamina: f32,
    shield: f32,
    credits: u32,
    wave: u32,
    music: bool,
    difficulty: f32,
    last_action: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct DemoSnapshot {
    page: DemoPage,
    selected_slot: usize,
    health: f32,
    stamina: f32,
    shield: f32,
    credits: u32,
    wave: u32,
    music: bool,
    difficulty: f32,
    last_action: &'static str,
}

struct KenneyNeoUiShowcase {
    time: f32,
    state: NeoState<DemoState>,
    screenshot: ScreenshotProbe,
}

impl Default for KenneyNeoUiShowcase {
    fn default() -> Self {
        Self {
            time: 0.0,
            state: NeoState::new(DemoState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            page: DemoPage::Title,
            selected_slot: 0,
            health: 0.86,
            stamina: 0.58,
            shield: 0.42,
            credits: 1280,
            wave: 3,
            music: true,
            difficulty: 0.45,
            last_action: "Ready in orbit",
        }
    }
}

impl From<&DemoState> for DemoSnapshot {
    fn from(value: &DemoState) -> Self {
        Self {
            page: value.page,
            selected_slot: value.selected_slot,
            health: value.health,
            stamina: value.stamina,
            shield: value.shield,
            credits: value.credits,
            wave: value.wave,
            music: value.music,
            difficulty: value.difficulty,
            last_action: value.last_action,
        }
    }
}

impl AppState for KenneyNeoUiShowcase {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: RenderColor::rgb(0.035, 0.055, 0.070),
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
        self.state.update(|state| {
            if state.page == DemoPage::Hud {
                state.stamina = (state.stamina + ctx.dt() * 0.07).min(1.0);
                if state.stamina >= 1.0 {
                    state.stamina = 0.30;
                    state.credits += 15;
                }
            }
        });

        let state = self.state.clone();
        let snapshot = self.state.read(|state| DemoSnapshot::from(state));
        let time = self.time;

        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            draw_showcase(ui, screen.width, screen.height, time, &state, snapshot);
        });

        ctx.set_title(match snapshot.page {
            DemoPage::Title => "SkyEngine - Kenney Neo UI",
            DemoPage::Hud => "SkyEngine - Kenney Neo UI HUD",
            DemoPage::Loadout => "SkyEngine - Kenney Neo UI Loadout",
            DemoPage::Settings => "SkyEngine - Kenney Neo UI Settings",
        });
        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        ctx.request_redraw();
    }
}

fn draw_showcase(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    time: f32,
    state: &NeoState<DemoState>,
    snapshot: DemoSnapshot,
) {
    draw_backdrop(ui, screen_w, screen_h, time);
    ui.stack("root")
        .size(screen_w, screen_h)
        .clip()
        .content(|ui| match snapshot.page {
            DemoPage::Title => draw_title(ui, screen_w, screen_h, state, snapshot),
            DemoPage::Hud => draw_hud(ui, screen_w, screen_h, time, state, snapshot),
            DemoPage::Loadout => draw_loadout(ui, screen_w, screen_h, state, snapshot),
            DemoPage::Settings => draw_settings(ui, screen_w, screen_h, state, snapshot),
        });
}

fn draw_backdrop(ui: &mut Ui, screen_w: f32, screen_h: f32, time: f32) {
    ui.rect("scene.sky")
        .size(screen_w, screen_h)
        .gradient(c(20, 48, 66, 255), c(62, 98, 86, 255))
        .build();

    let moon_x = screen_w * 0.72 + time.sin() * 10.0;
    ui.rect("scene.moon.glow")
        .position(moon_x - 86.0, 66.0)
        .size(172.0, 172.0)
        .radius(86.0)
        .color(Color::rgba(0.74, 0.90, 1.0, 0.13))
        .shadow(50.0, 0.0, 0.0, Color::rgba(0.56, 0.82, 1.0, 0.16))
        .build();
    ui.rect("scene.moon")
        .position(moon_x - 31.0, 121.0)
        .size(62.0, 62.0)
        .radius(31.0)
        .color(Color::rgba(0.86, 0.94, 0.98, 0.88))
        .build();

    ui.polygon("scene.mountain.back")
        .points([
            [0.0, screen_h * 0.66],
            [screen_w * 0.18, screen_h * 0.36],
            [screen_w * 0.38, screen_h * 0.68],
            [screen_w * 0.58, screen_h * 0.34],
            [screen_w * 0.80, screen_h * 0.69],
            [screen_w, screen_h * 0.47],
            [screen_w, screen_h],
            [0.0, screen_h],
        ])
        .color(Color::rgba(0.09, 0.16, 0.17, 0.92))
        .build();
    ui.polygon("scene.mountain.front")
        .points([
            [0.0, screen_h * 0.78],
            [screen_w * 0.20, screen_h * 0.53],
            [screen_w * 0.39, screen_h * 0.78],
            [screen_w * 0.60, screen_h * 0.52],
            [screen_w * 0.84, screen_h * 0.78],
            [screen_w, screen_h * 0.61],
            [screen_w, screen_h],
            [0.0, screen_h],
        ])
        .color(Color::rgba(0.045, 0.095, 0.095, 0.98))
        .build();
    ui.rect("scene.ground")
        .position(0.0, screen_h * 0.80)
        .size(screen_w, screen_h * 0.20)
        .gradient(c(37, 83, 58, 255), c(15, 37, 37, 255))
        .build();

    for i in 0..9 {
        let x = screen_w * (i as f32 / 8.0);
        ui.rect(format!("scene.scanline.{i}"))
            .position(x - 1.0, screen_h * 0.80)
            .size(2.0, screen_h * 0.20)
            .color(Color::rgba(0.8, 1.0, 0.9, 0.06))
            .build();
    }
}

fn draw_title(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    state: &NeoState<DemoState>,
    snapshot: DemoSnapshot,
) {
    let panel_w = 520.0;
    let panel_h = 456.0;
    let x = (screen_w - panel_w) * 0.5;
    let y = (screen_h - panel_h) * 0.5 - 18.0;

    panel(
        ui,
        "title.panel",
        x,
        y,
        panel_w,
        panel_h,
        c(17, 25, 32, 228),
    );
    ui.text("title.kicker")
        .position(x + 44.0, y + 34.0)
        .size(panel_w - 88.0, 28.0)
        .text("KENNEY UI PACK / UI-NEO")
        .font_source(asset(FONT))
        .font_size(15.0)
        .color(c(166, 220, 236, 255))
        .horizontal_align(HorizontalAlign::Center)
        .build();
    ui.text("title.name")
        .position(x + 34.0, y + 70.0)
        .size(panel_w - 68.0, 62.0)
        .text("FIELD HUD")
        .font_source(asset(FONT))
        .font_size(42.0)
        .line_height(48.0)
        .color(c(248, 253, 255, 255))
        .horizontal_align(HorizontalAlign::Center)
        .vertical_align(VerticalAlign::Center)
        .build();
    ui.image("title.divider")
        .position(x + 110.0, y + 142.0)
        .size(panel_w - 220.0, 8.0)
        .source(asset(DIVIDER))
        .stretch()
        .build();
    ui.text("title.body")
        .position(x + 58.0, y + 166.0)
        .size(panel_w - 116.0, 70.0)
        .text("Sprite-backed controls, HUD overlays, modals, sliders, callbacks, and image assets without turning UI into ECS plumbing.")
        .font_size(16.0)
        .line_height(22.0)
        .color(c(206, 225, 229, 255))
        .horizontal_align(HorizontalAlign::Center)
        .wrap(true)
        .build();

    let start = state.clone();
    image_button(
        ui,
        "title.start",
        GREEN_BUTTON,
        "START SORTIE",
        x + 116.0,
        y + 258.0,
        288.0,
        54.0,
        20.0,
        move || {
            start.update(|state| {
                state.page = DemoPage::Hud;
                state.last_action = "Sortie started";
            });
        },
    );
    let loadout = state.clone();
    image_button(
        ui,
        "title.loadout",
        BLUE_BUTTON,
        "LOADOUT",
        x + 116.0,
        y + 322.0,
        136.0,
        50.0,
        17.0,
        move || loadout.update(|state| state.page = DemoPage::Loadout),
    );
    let settings = state.clone();
    image_button(
        ui,
        "title.settings",
        YELLOW_BUTTON,
        "OPTIONS",
        x + 268.0,
        y + 322.0,
        136.0,
        50.0,
        17.0,
        move || settings.update(|state| state.page = DemoPage::Settings),
    );

    status_ribbon(
        ui,
        "title.ribbon",
        x + 62.0,
        y + 392.0,
        panel_w - 124.0,
        &format!("Last action: {}", snapshot.last_action),
    );
}

fn draw_hud(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    _time: f32,
    state: &NeoState<DemoState>,
    snapshot: DemoSnapshot,
) {
    draw_top_hud(ui, screen_w, state, snapshot);
    draw_status_bars(ui, screen_w, state, snapshot);
    draw_mission_card(ui, screen_h, state, snapshot);
    draw_action_bar(ui, screen_w, screen_h, state);
    draw_minimap(ui, screen_w);
}

fn draw_top_hud(ui: &mut Ui, screen_w: f32, state: &NeoState<DemoState>, snapshot: DemoSnapshot) {
    panel(
        ui,
        "hud.top.left",
        24.0,
        22.0,
        370.0,
        82.0,
        c(13, 22, 31, 220),
    );
    ui.text("hud.title")
        .position(46.0, 35.0)
        .size(220.0, 28.0)
        .text("SKY PATROL")
        .font_source(asset(FONT))
        .font_size(24.0)
        .color(c(244, 251, 255, 255))
        .build();
    ui.text("hud.wave")
        .position(48.0, 66.0)
        .size(240.0, 22.0)
        .text(format!(
            "Wave {:02} / credits {}",
            snapshot.wave, snapshot.credits
        ))
        .font_size(15.0)
        .color(c(183, 211, 218, 255))
        .build();
    ui.image("hud.coin.icon")
        .position(324.0, 40.0)
        .size(38.0, 38.0)
        .source(asset(STAR))
        .contain()
        .build();

    let title = state.clone();
    image_button(
        ui,
        "hud.pause",
        GREY_BUTTON,
        "MENU",
        screen_w - 140.0,
        24.0,
        112.0,
        44.0,
        15.0,
        move || title.update(|state| state.page = DemoPage::Title),
    );
}

fn draw_status_bars(
    ui: &mut Ui,
    screen_w: f32,
    state: &NeoState<DemoState>,
    snapshot: DemoSnapshot,
) {
    let x = screen_w - 390.0;
    panel(
        ui,
        "hud.status.panel",
        x,
        86.0,
        362.0,
        170.0,
        c(10, 17, 25, 218),
    );
    status_bar(
        ui,
        "hud.hp",
        x + 26.0,
        112.0,
        "HULL",
        snapshot.health,
        c(64, 218, 128, 255),
    );
    status_bar(
        ui,
        "hud.stamina",
        x + 26.0,
        162.0,
        "BOOST",
        snapshot.stamina,
        c(71, 167, 244, 255),
    );
    status_bar(
        ui,
        "hud.shield",
        x + 26.0,
        212.0,
        "SHIELD",
        snapshot.shield,
        c(252, 203, 73, 255),
    );

    let hit = state.clone();
    image_button(
        ui,
        "hud.hit",
        RED_BUTTON,
        "HIT",
        x + 26.0,
        268.0,
        98.0,
        42.0,
        15.0,
        move || {
            hit.update(|state| {
                state.health = (state.health - 0.12).max(0.0);
                state.shield = (state.shield - 0.08).max(0.0);
                state.last_action = "Incoming damage";
            });
        },
    );
    let repair = state.clone();
    image_button(
        ui,
        "hud.repair",
        GREEN_BUTTON,
        "REPAIR",
        x + 140.0,
        268.0,
        116.0,
        42.0,
        15.0,
        move || {
            repair.update(|state| {
                state.health = (state.health + 0.16).min(1.0);
                state.shield = (state.shield + 0.12).min(1.0);
                state.credits = state.credits.saturating_sub(40);
                state.last_action = "Field repairs applied";
            });
        },
    );
    let wave = state.clone();
    icon_button(
        ui,
        "hud.wave.next",
        x + 274.0,
        264.0,
        YELLOW_SQUARE,
        ICON_UP,
        move || {
            wave.update(|state| {
                state.wave += 1;
                state.credits += 125;
                state.last_action = "Advanced wave";
            });
        },
    );
}

fn draw_mission_card(
    ui: &mut Ui,
    screen_h: f32,
    state: &NeoState<DemoState>,
    snapshot: DemoSnapshot,
) {
    let y = (screen_h - 270.0) * 0.5;
    panel(
        ui,
        "mission.panel",
        28.0,
        y,
        312.0,
        270.0,
        c(13, 21, 29, 224),
    );
    ui.text("mission.title")
        .position(54.0, y + 26.0)
        .size(220.0, 30.0)
        .text("ACTIVE QUEST")
        .font_source(asset(FONT))
        .font_size(21.0)
        .color(c(248, 253, 255, 255))
        .build();
    ui.image("mission.divider")
        .position(54.0, y + 62.0)
        .size(226.0, 8.0)
        .source(asset(DIVIDER))
        .stretch()
        .build();
    ui.text("mission.copy")
        .position(54.0, y + 84.0)
        .size(232.0, 82.0)
        .text("Hold the ridge, gather salvage, and keep the convoy alive while the storm wall moves in.")
        .font_size(15.0)
        .line_height(21.0)
        .wrap(true)
        .color(c(204, 221, 224, 255))
        .build();
    status_ribbon(
        ui,
        "mission.log",
        54.0,
        y + 176.0,
        230.0,
        snapshot.last_action,
    );
    let loadout = state.clone();
    image_button(
        ui,
        "mission.loadout",
        BLUE_BUTTON,
        "LOADOUT",
        54.0,
        y + 224.0,
        110.0,
        38.0,
        13.0,
        move || loadout.update(|state| state.page = DemoPage::Loadout),
    );
    let settings = state.clone();
    image_button(
        ui,
        "mission.settings",
        YELLOW_BUTTON,
        "TUNE",
        174.0,
        y + 224.0,
        110.0,
        38.0,
        13.0,
        move || settings.update(|state| state.page = DemoPage::Settings),
    );
}

fn draw_action_bar(ui: &mut Ui, screen_w: f32, screen_h: f32, state: &NeoState<DemoState>) {
    let bar_w = 560.0;
    let x = (screen_w - bar_w) * 0.5;
    let y = screen_h - 106.0;
    panel(ui, "action.panel", x, y, bar_w, 82.0, c(11, 18, 26, 218));
    let actions = [
        ("action.0", BLUE_SQUARE, ICON_PLAY),
        ("action.1", GREEN_SQUARE, CHECK_ON),
        ("action.2", YELLOW_SQUARE, STAR),
        ("action.3", RED_SQUARE, ICON_REPEAT),
        ("action.4", BLUE_SQUARE, ICON_UP),
    ];
    for (index, (id, bg, icon)) in actions.into_iter().enumerate() {
        let select = state.clone();
        icon_button(
            ui,
            id,
            x + 34.0 + index as f32 * 100.0,
            y + 16.0,
            bg,
            icon,
            move || {
                select.update(|state| {
                    state.selected_slot = index;
                    state.last_action = match index {
                        0 => "Boost primed",
                        1 => "Drone linked",
                        2 => "Ultimate queued",
                        3 => "Reload cycle",
                        _ => "Signal ping",
                    };
                });
            },
        );
        if index == state.read(|state| state.selected_slot) {
            ui.image(format!("{id}.selected"))
                .position(x + 28.0 + index as f32 * 100.0, y + 10.0)
                .size(64.0, 64.0)
                .source(asset(STAR_OUTLINE))
                .contain()
                .opacity(0.65)
                .build();
        }
    }
}

fn draw_minimap(ui: &mut Ui, screen_w: f32) {
    let x = screen_w - 224.0;
    let y = 332.0;
    panel(ui, "map.panel", x, y, 196.0, 196.0, c(10, 18, 24, 218));
    ui.rect("map.inner")
        .position(x + 18.0, y + 18.0)
        .size(160.0, 160.0)
        .radius(80.0)
        .gradient(c(40, 88, 82, 255), c(18, 44, 55, 255))
        .border(2.0, c(121, 194, 196, 210))
        .build();
    for i in 0..4 {
        ui.rect(format!("map.blip.{i}"))
            .position(x + 62.0 + i as f32 * 22.0, y + 82.0 + (i % 2) as f32 * 24.0)
            .size(10.0, 10.0)
            .radius(5.0)
            .color(if i == 2 {
                c(238, 92, 82, 255)
            } else {
                c(252, 210, 76, 255)
            })
            .build();
    }
}

fn draw_loadout(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    state: &NeoState<DemoState>,
    snapshot: DemoSnapshot,
) {
    draw_hud(ui, screen_w, screen_h, 0.0, state, snapshot);
    modal_scrim(ui, screen_w, screen_h);
    let w = 742.0;
    let h = 478.0;
    let x = (screen_w - w) * 0.5;
    let y = (screen_h - h) * 0.5;
    panel(ui, "loadout.panel", x, y, w, h, c(15, 23, 31, 242));
    ui.text("loadout.title")
        .position(x + 34.0, y + 28.0)
        .size(360.0, 34.0)
        .text("LOADOUT GRID")
        .font_source(asset(FONT))
        .font_size(27.0)
        .color(c(246, 253, 255, 255))
        .build();
    ui.text("loadout.hint")
        .position(x + 36.0, y + 68.0)
        .size(500.0, 24.0)
        .text("Click a slot to mark the active command tile.")
        .font_size(15.0)
        .color(c(184, 205, 211, 255))
        .build();

    for row in 0..3 {
        for col in 0..5 {
            let index = row * 5 + col;
            let sx = x + 44.0 + col as f32 * 92.0;
            let sy = y + 120.0 + row as f32 * 92.0;
            let slot_state = state.clone();
            let bg = match index % 4 {
                0 => BLUE_SQUARE,
                1 => GREEN_SQUARE,
                2 => YELLOW_SQUARE,
                _ => RED_SQUARE,
            };
            icon_button(
                ui,
                format!("loadout.slot.{index}"),
                sx,
                sy,
                bg,
                STAR,
                move || {
                    slot_state.update(|state| {
                        state.selected_slot = index;
                        state.last_action = "Loadout slot selected";
                    });
                },
            );
            ui.text(format!("loadout.slot.{index}.label"))
                .position(sx - 8.0, sy + 62.0)
                .size(72.0, 20.0)
                .text(format!("SLOT {:02}", index + 1))
                .font_size(11.0)
                .color(c(210, 225, 228, 255))
                .horizontal_align(HorizontalAlign::Center)
                .build();
            if snapshot.selected_slot == index {
                ui.image(format!("loadout.slot.{index}.ring"))
                    .position(sx - 7.0, sy - 7.0)
                    .size(66.0, 66.0)
                    .source(asset(STAR_OUTLINE))
                    .contain()
                    .opacity(0.72)
                    .build();
            }
        }
    }

    panel(
        ui,
        "loadout.detail",
        x + 536.0,
        y + 116.0,
        162.0,
        258.0,
        c(22, 33, 41, 230),
    );
    ui.image("loadout.detail.icon")
        .position(x + 584.0, y + 150.0)
        .size(64.0, 64.0)
        .source(asset(STAR))
        .contain()
        .build();
    ui.text("loadout.detail.name")
        .position(x + 556.0, y + 236.0)
        .size(122.0, 28.0)
        .text(format!("TILE {:02}", snapshot.selected_slot + 1))
        .font_source(asset(FONT))
        .font_size(18.0)
        .color(c(248, 253, 255, 255))
        .horizontal_align(HorizontalAlign::Center)
        .build();
    ui.text("loadout.detail.copy")
        .position(x + 558.0, y + 278.0)
        .size(118.0, 70.0)
        .text("Neo keeps the UI state local and lets the renderer resolve image assets.")
        .font_size(13.0)
        .line_height(18.0)
        .wrap(true)
        .color(c(185, 205, 211, 255))
        .horizontal_align(HorizontalAlign::Center)
        .build();

    let back = state.clone();
    image_button(
        ui,
        "loadout.back",
        GREEN_BUTTON,
        "BACK TO HUD",
        x + w - 190.0,
        y + h - 72.0,
        150.0,
        44.0,
        14.0,
        move || back.update(|state| state.page = DemoPage::Hud),
    );
}

fn draw_settings(
    ui: &mut Ui,
    screen_w: f32,
    screen_h: f32,
    state: &NeoState<DemoState>,
    snapshot: DemoSnapshot,
) {
    draw_hud(ui, screen_w, screen_h, 0.0, state, snapshot);
    modal_scrim(ui, screen_w, screen_h);
    let w = 520.0;
    let h = 390.0;
    let x = (screen_w - w) * 0.5;
    let y = (screen_h - h) * 0.5;
    panel(ui, "settings.panel", x, y, w, h, c(15, 23, 31, 242));
    ui.text("settings.title")
        .position(x + 34.0, y + 28.0)
        .size(w - 68.0, 36.0)
        .text("OPTIONS")
        .font_source(asset(FONT))
        .font_size(28.0)
        .color(c(246, 253, 255, 255))
        .horizontal_align(HorizontalAlign::Center)
        .build();

    let music = state.clone();
    check_row(
        ui,
        "settings.music",
        x + 60.0,
        y + 96.0,
        "Music",
        snapshot.music,
        move || {
            music.update(|state| {
                state.music = !state.music;
                state.last_action = if state.music {
                    "Music on"
                } else {
                    "Music muted"
                };
            });
        },
    );

    ui.text("settings.diff.label")
        .position(x + 60.0, y + 166.0)
        .size(150.0, 28.0)
        .text("Difficulty")
        .font_source(asset(FONT))
        .font_size(17.0)
        .color(c(238, 247, 250, 255))
        .build();
    let difficulty = state.clone();
    widgets::slider(ui, "settings.diff.slider")
        .size(330.0, 30.0)
        .value(snapshot.difficulty)
        .style(widgets::SliderStyle {
            track: c(36, 53, 63, 255),
            fill: c(83, 170, 244, 255),
            knob: c(248, 253, 255, 255),
        })
        .on_change(move |value| {
            difficulty.update(|state| {
                state.difficulty = value;
                state.last_action = "Difficulty tuned";
            });
        })
        .build();
    ui.image("settings.slider.skin")
        .position(x + 58.0, y + 205.0)
        .size(332.0, 11.0)
        .source(asset(
            "examples/assets/kenney_ui_pack/PNG/Blue/Default/slide_horizontal_color.png",
        ))
        .stretch()
        .opacity(0.65)
        .build();

    let easier = state.clone();
    icon_button(
        ui,
        "settings.down",
        x + 408.0,
        y + 157.0,
        BLUE_SQUARE,
        ICON_DOWN,
        move || {
            easier.update(|state| {
                state.difficulty = (state.difficulty - 0.08).max(0.0);
                state.last_action = "Difficulty down";
            });
        },
    );
    let harder = state.clone();
    icon_button(
        ui,
        "settings.up",
        x + 458.0,
        y + 157.0,
        YELLOW_SQUARE,
        ICON_UP,
        move || {
            harder.update(|state| {
                state.difficulty = (state.difficulty + 0.08).min(1.0);
                state.last_action = "Difficulty up";
            });
        },
    );

    status_ribbon(
        ui,
        "settings.summary",
        x + 58.0,
        y + 254.0,
        w - 116.0,
        &format!(
            "Difficulty {:>3}% / {}",
            (snapshot.difficulty * 100.0).round() as i32,
            snapshot.last_action
        ),
    );
    let back = state.clone();
    image_button(
        ui,
        "settings.back",
        GREEN_BUTTON,
        "APPLY",
        x + 140.0,
        y + 316.0,
        116.0,
        44.0,
        14.0,
        move || back.update(|state| state.page = DemoPage::Hud),
    );
    let title = state.clone();
    image_button(
        ui,
        "settings.menu",
        GREY_BUTTON,
        "TITLE",
        x + 272.0,
        y + 316.0,
        108.0,
        44.0,
        14.0,
        move || title.update(|state| state.page = DemoPage::Title),
    );
}

fn status_bar(ui: &mut Ui, id: &str, x: f32, y: f32, label: &str, value: f32, fill: Color) {
    ui.text(format!("{id}.label"))
        .position(x, y - 22.0)
        .size(116.0, 20.0)
        .text(label)
        .font_source(asset(FONT))
        .font_size(14.0)
        .color(c(228, 239, 242, 255))
        .build();
    widgets::progress(ui, id)
        .size(278.0, 18.0)
        .value(value)
        .style(widgets::ProgressStyle {
            track: c(28, 42, 52, 235),
            fill,
        })
        .transition_seconds(0.20, Ease::OutCubic)
        .build();
    ui.text(format!("{id}.value"))
        .position(x + 286.0, y - 5.0)
        .size(48.0, 24.0)
        .text(format!("{:>3}%", (value * 100.0).round() as i32))
        .font_size(13.0)
        .color(c(202, 220, 225, 255))
        .build();
}

fn image_button<F>(
    ui: &mut Ui,
    id: &str,
    image: &'static str,
    label: impl Into<String>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    font_size: f32,
    on_click: F,
) where
    F: FnMut() + 'static,
{
    let label = label.into();
    ui.stack(id)
        .position(x, y)
        .size(w, h)
        .visual_state_from(format!("{id}.bg"), 0.96)
        .content(|ui| {
            ui.image(format!("{id}.bg"))
                .fill()
                .source(asset(image))
                .stretch()
                .states(
                    Color::WHITE,
                    Color::rgba(1.0, 1.0, 0.90, 1.0),
                    Color::rgba(0.74, 0.86, 0.92, 1.0),
                )
                .transition_seconds(0.09, Ease::OutCubic)
                .animate(AnimProperty::COLOR | AnimProperty::TRANSFORM)
                .on_click(on_click)
                .build();
            ui.text(format!("{id}.label"))
                .fill()
                .text(label)
                .font_source(asset(FONT))
                .font_size(font_size)
                .line_height(font_size + 4.0)
                .color(c(255, 255, 255, 255))
                .horizontal_align(HorizontalAlign::Center)
                .vertical_align(VerticalAlign::Center)
                .build();
        });
}

fn icon_button<F>(
    ui: &mut Ui,
    id: impl Into<String>,
    x: f32,
    y: f32,
    bg: &'static str,
    icon: &'static str,
    on_click: F,
) where
    F: FnMut() + 'static,
{
    let id = id.into();
    ui.stack(id.as_str())
        .position(x, y)
        .size(52.0, 52.0)
        .visual_state_from(format!("{id}.bg"), 0.92)
        .content(|ui| {
            ui.image(format!("{id}.bg"))
                .fill()
                .source(asset(bg))
                .stretch()
                .states(
                    Color::WHITE,
                    Color::rgba(1.0, 1.0, 0.90, 1.0),
                    Color::rgba(0.76, 0.86, 0.96, 1.0),
                )
                .transition_seconds(0.08, Ease::OutCubic)
                .on_click(on_click)
                .build();
            ui.image(format!("{id}.icon"))
                .position(13.0, 13.0)
                .size(26.0, 26.0)
                .source(asset(icon))
                .contain()
                .build();
        });
}

fn check_row<F>(ui: &mut Ui, id: &str, x: f32, y: f32, label: &str, checked: bool, on_click: F)
where
    F: FnMut() + 'static,
{
    ui.stack(id).position(x, y).size(360.0, 46.0).content(|ui| {
        ui.image(format!("{id}.box"))
            .position(0.0, 1.0)
            .size(42.0, 42.0)
            .source(asset(if checked { CHECK_ON } else { CHECK_OFF }))
            .contain()
            .states(
                Color::WHITE,
                Color::rgba(1.0, 1.0, 0.90, 1.0),
                Color::rgba(0.82, 0.92, 1.0, 1.0),
            )
            .on_click(on_click)
            .build();
        ui.text(format!("{id}.label"))
            .position(58.0, 0.0)
            .size(250.0, 46.0)
            .text(label)
            .font_source(asset(FONT))
            .font_size(18.0)
            .color(c(238, 247, 250, 255))
            .vertical_align(VerticalAlign::Center)
            .build();
    });
}

fn panel(ui: &mut Ui, id: &str, x: f32, y: f32, w: f32, h: f32, color: Color) {
    ui.stack(id).position(x, y).size(w, h).content(|ui| {
        ui.rect(format!("{id}.shadow"))
            .position(7.0, 9.0)
            .size(w, h)
            .radius(7.0)
            .color(Color::rgba(0.0, 0.0, 0.0, 0.22))
            .build();
        ui.rect(format!("{id}.bg"))
            .size(w, h)
            .radius(7.0)
            .color(color)
            .border(1.0, c(110, 166, 174, 120))
            .build();
        ui.image(format!("{id}.shine"))
            .position(12.0, 12.0)
            .size(w - 24.0, 22.0)
            .source(asset(INPUT_OUTLINE))
            .stretch()
            .opacity(0.20)
            .build();
    });
}

fn status_ribbon(ui: &mut Ui, id: &str, x: f32, y: f32, w: f32, label: &str) {
    ui.stack(id).position(x, y).size(w, 34.0).content(|ui| {
        ui.image(format!("{id}.bg"))
            .fill()
            .source(asset(INPUT))
            .stretch()
            .opacity(0.82)
            .build();
        ui.text(format!("{id}.text"))
            .position(12.0, 0.0)
            .size(w - 24.0, 34.0)
            .text(label)
            .font_size(13.0)
            .color(c(62, 76, 84, 255))
            .vertical_align(VerticalAlign::Center)
            .horizontal_align(HorizontalAlign::Center)
            .build();
    });
}

fn modal_scrim(ui: &mut Ui, screen_w: f32, screen_h: f32) {
    ui.rect("modal.scrim")
        .size(screen_w, screen_h)
        .color(Color::rgba(0.01, 0.02, 0.03, 0.54))
        .z(80)
        .build();
}

fn c(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::rgba8(r, g, b, a)
}

fn asset(path: &str) -> String {
    format!("{}/{}", env!("CARGO_MANIFEST_DIR").replace('\\', "/"), path)
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

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("SkyEngine - Kenney Neo UI", WINDOW_W, WINDOW_H)
                .with_vsync(false)
                .with_resizable(true),
        )
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(NeoUiPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(KenneyNeoUiShowcase::default());
}
