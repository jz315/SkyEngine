//! A lane-defense game inspired by classic garden tower defense games.
//!
//! This uses original procedural visuals and engine-owned gameplay code.
//!
//! ```bash
//! cargo run --example lawn_defense_game --features ui --release
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
use sky_engine::asset::{AssetServer, Handle, TextureAsset};
use sky_engine::ecs::{EntityId, World};
use sky_engine::input::{Input, KeyCode};
use sky_engine::math::Vec2;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SortingLayer,
    SpriteFeature, SpriteRenderer, Transform, TransparentPhase,
};
use sky_engine::ui::{
    UiAlign, UiAnchor, UiButton, UiEventKind, UiEvents, UiId, UiLayout, UiLength, UiNode, UiPanel,
    UiProgressBar, UiRect, UiText,
};

const ROWS: usize = 5;
const COLS: usize = 9;
const CELL_W: f32 = 88.0;
const CELL_H: f32 = 84.0;
const BOARD_LEFT: f32 = -410.0;
const BOARD_TOP: f32 = 214.0;
const ORTHO_HEIGHT: f32 = 720.0;
const WINDOW_W: u32 = 1280;
const WINDOW_H: u32 = 760;
const HOUSE_X: f32 = BOARD_LEFT - 85.0;
const SPAWN_X: f32 = BOARD_LEFT + CELL_W * COLS as f32 + 82.0;
const PROJECTILE_SPEED: f32 = 365.0;
const MOWER_SPEED: f32 = 520.0;
const FINAL_WAVE: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameMode {
    Title,
    Playing,
    Paused,
    Victory,
    GameOver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlantKind {
    Peashooter,
    Sunflower,
    Wallnut,
    Sprayer,
}

impl PlantKind {
    fn cost(self) -> i32 {
        match self {
            Self::Peashooter => 100,
            Self::Sunflower => 50,
            Self::Wallnut => 75,
            Self::Sprayer => 175,
        }
    }

    fn hp(self) -> i32 {
        match self {
            Self::Peashooter => 4,
            Self::Sunflower => 3,
            Self::Wallnut => 15,
            Self::Sprayer => 4,
        }
    }

    fn cooldown(self) -> f32 {
        match self {
            Self::Peashooter => 1.25,
            Self::Sunflower => 7.5,
            Self::Wallnut => 999.0,
            Self::Sprayer => 1.65,
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Peashooter => Color::rgb(0.28, 0.86, 0.36),
            Self::Sunflower => Color::rgb(1.0, 0.82, 0.18),
            Self::Wallnut => Color::rgb(0.72, 0.48, 0.25),
            Self::Sprayer => Color::rgb(0.22, 0.95, 0.82),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Peashooter => "Pea",
            Self::Sunflower => "Sun",
            Self::Wallnut => "Wall",
            Self::Sprayer => "Spray",
        }
    }

    fn all() -> [Self; 4] {
        [
            Self::Peashooter,
            Self::Sunflower,
            Self::Wallnut,
            Self::Sprayer,
        ]
    }
}

#[derive(Clone, Copy)]
struct Plant {
    kind: PlantKind,
    hp: i32,
    cooldown: f32,
    flash: f32,
}

#[derive(Clone, Copy)]
struct Zombie {
    row: usize,
    hp: i32,
    speed: f32,
    bite_timer: f32,
    flash: f32,
}

#[derive(Clone, Copy)]
struct Pea {
    row: usize,
    damage: i32,
    splash: bool,
}

#[derive(Clone, Copy)]
struct Sun {
    value: i32,
    ttl: f32,
    target_y: f32,
    fall_speed: f32,
}

#[derive(Clone, Copy)]
struct Mower {
    row: usize,
    active: bool,
}

#[derive(Clone, Copy)]
struct Card {
    kind: PlantKind,
}

#[derive(Clone, Copy)]
struct LawnPulse {
    base_width: f32,
    base_height: f32,
    phase: f32,
    speed: f32,
    amplitude: f32,
}

#[derive(Clone, Copy)]
struct LawnUi {
    hud_text: EntityId,
    house_bar: EntityId,
    wave_bar: EntityId,
    pause_button: EntityId,
    menu_panel: EntityId,
    menu_title: EntityId,
    menu_body: EntityId,
    primary_button: EntityId,
    secondary_button: EntityId,
}

struct LawnDefenseGame {
    mode: GameMode,
    assets: GameAssets,
    level: LevelState,
    run: RunState,
    rng: SimpleRng,
    hud_frame: u32,
    ui: Option<LawnUi>,
}

impl LawnDefenseGame {
    fn new() -> Self {
        Self {
            mode: GameMode::Title,
            assets: GameAssets::default(),
            level: LevelState::new(),
            run: RunState::new(),
            rng: SimpleRng::new(0x51A7_2026),
            hud_frame: 0,
            ui: None,
        }
    }

    fn start_run(&mut self, world: &mut World) {
        self.level.clear_dynamic(world);
        self.run = RunState::new();
        self.mode = GameMode::Playing;
        spawn_mowers(world, &self.assets, &mut self.level);
        set_clear_color(world, Color::rgb(0.035, 0.055, 0.035));
    }

    fn update_title(&mut self, ctx: &mut FrameContext<'_>) {
        animate_lawn(ctx.world, ctx.dt);
        if ctx.input.key_pressed(KeyCode::Space) || ctx.input.key_pressed(KeyCode::Enter) {
            self.start_run(ctx.world);
        }
        ctx.render();
        self.set_title(ctx, "SPACE start | 1 pea 2 sun 3 wall 4 spray | click lawn");
    }

    fn update_playing(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::KeyP) {
            self.mode = GameMode::Paused;
            ctx.render();
            self.set_title(ctx, "Paused | P resume | R restart | Esc exit");
            return;
        }
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }

        self.handle_keyboard(ctx.input);
        self.handle_mouse(ctx);

        self.run.elapsed += ctx.dt;
        self.run.sun_drop_timer -= ctx.dt;
        self.run.spawn_timer -= ctx.dt;
        self.run.wave_timer -= ctx.dt;

        update_cards(ctx.world, self.run.selected, self.run.sun);
        update_plants(
            ctx.world,
            &self.assets,
            &mut self.level,
            &mut self.run,
            &mut self.rng,
            ctx.dt,
        );
        update_zombies(ctx.world, &mut self.level, &mut self.run, ctx.dt);
        update_projectiles(ctx.world, &mut self.level, &mut self.run, ctx.dt);
        update_suns(ctx.world, &mut self.level, ctx.dt);
        update_mowers(ctx.world, &mut self.level, &mut self.run, ctx.dt);
        spawn_ambient_sun(
            ctx.world,
            &self.assets,
            &mut self.level,
            &mut self.run,
            &mut self.rng,
        );
        self.spawn_wave(ctx.world);
        animate_lawn(ctx.world, ctx.dt);

        self.level.retain_live(ctx.world);
        self.check_end_state(ctx.world);

        ctx.render();
        self.update_hud(ctx);
    }

    fn update_paused(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::KeyP) {
            self.mode = GameMode::Playing;
        }
        if ctx.input.key_pressed(KeyCode::KeyR) {
            self.start_run(ctx.world);
        }
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
        }
        ctx.render();
        self.set_title(ctx, "Paused | P resume | R restart | Esc exit");
    }

    fn update_finished(&mut self, ctx: &mut FrameContext<'_>) {
        animate_lawn(ctx.world, ctx.dt);
        update_suns(ctx.world, &mut self.level, ctx.dt);
        if ctx.input.key_pressed(KeyCode::KeyR)
            || ctx.input.key_pressed(KeyCode::Space)
            || ctx.input.key_pressed(KeyCode::Enter)
        {
            self.start_run(ctx.world);
        }
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
        }
        ctx.render();
        match self.mode {
            GameMode::Victory => self.set_title(
                ctx,
                &format!(
                    "Victory | score {} | sun {} | R/Space restart | Esc exit",
                    self.run.score, self.run.sun
                ),
            ),
            GameMode::GameOver => self.set_title(
                ctx,
                &format!(
                    "Lawn overrun | score {} | wave {} | R/Space restart | Esc exit",
                    self.run.score, self.run.wave
                ),
            ),
            _ => {}
        }
    }

    fn handle_keyboard(&mut self, input: &Input) {
        if input.key_pressed(KeyCode::Digit1) {
            self.run.selected = PlantKind::Peashooter;
        }
        if input.key_pressed(KeyCode::Digit2) {
            self.run.selected = PlantKind::Sunflower;
        }
        if input.key_pressed(KeyCode::Digit3) {
            self.run.selected = PlantKind::Wallnut;
        }
        if input.key_pressed(KeyCode::Digit4) {
            self.run.selected = PlantKind::Sprayer;
        }
        if input.key_pressed(KeyCode::KeyR) {
            self.run.restart_requested = true;
        }
    }

    fn handle_mouse(&mut self, ctx: &mut FrameContext<'_>) {
        if self.run.restart_requested {
            self.run.restart_requested = false;
            self.start_run(ctx.world);
            return;
        }

        if ctx.ui_state().is_some_and(|state| state.wants_pointer()) {
            return;
        }

        let mouse = mouse_world(ctx);
        if ctx.input.mouse_right_pressed() {
            if let Some((row, col)) = cell_at(mouse) {
                self.remove_plant(ctx.world, row, col);
            }
            return;
        }

        if !ctx.input.mouse_left_pressed() {
            return;
        }

        if self.collect_sun(ctx.world, mouse) {
            return;
        }
        if let Some(kind) = card_at(mouse) {
            self.run.selected = kind;
            return;
        }
        if let Some((row, col)) = cell_at(mouse) {
            self.place_selected(ctx.world, row, col);
        }
    }

    fn collect_sun(&mut self, world: &mut World, mouse: Vec2) -> bool {
        let mut collected = None;
        for &sun in &self.level.suns {
            let Some(position) = entity_position(world, sun) else {
                continue;
            };
            if (position - mouse).length() <= 32.0 {
                collected = Some(sun);
                break;
            }
        }
        let Some(sun) = collected else {
            return false;
        };
        let value = world.get::<Sun>(sun).map(|sun| sun.value).unwrap_or(25);
        if world.contains(sun) {
            let _ = world.despawn(sun);
        }
        self.run.sun += value;
        self.run.score += 5;
        self.level.retain_live(world);
        true
    }

    fn place_selected(&mut self, world: &mut World, row: usize, col: usize) {
        if self.level.plants[row][col].is_some() {
            return;
        }
        let kind = self.run.selected;
        if self.run.sun < kind.cost() {
            flash_cards(world, kind);
            return;
        }

        self.run.sun -= kind.cost();
        let plant = spawn_plant(world, &self.assets, row, col, kind);
        self.level.plants[row][col] = Some(plant);
        self.level.dynamic.push(plant);
    }

    fn remove_plant(&mut self, world: &mut World, row: usize, col: usize) {
        let Some(plant) = self.level.plants[row][col].take() else {
            return;
        };
        if world.contains(plant) {
            let _ = world.despawn(plant);
            self.run.sun += 15;
        }
        self.level.retain_live(world);
    }

    fn spawn_wave(&mut self, world: &mut World) {
        if self.run.wave > FINAL_WAVE {
            return;
        }
        if self.run.wave_timer <= 0.0 && self.run.spawned_this_wave >= self.run.wave_size {
            if self.level.zombies.is_empty() {
                self.run.wave += 1;
                self.run.spawned_this_wave = 0;
                self.run.wave_size = 6 + self.run.wave as usize * 4;
                self.run.wave_timer = 4.0;
                self.run.spawn_timer = 0.8;
                self.run.sun += 50;
            }
            return;
        }
        if self.run.spawned_this_wave >= self.run.wave_size || self.run.spawn_timer > 0.0 {
            return;
        }

        let row = self.rng.range_usize(0, ROWS);
        let tough = self.run.wave >= 3 && self.rng.chance(0.28);
        let zombie = spawn_zombie(world, &self.assets, row, self.run.wave, tough);
        self.level.zombies.push(zombie);
        self.level.dynamic.push(zombie);
        self.run.spawned_this_wave += 1;
        self.run.spawn_timer = self
            .rng
            .range(1.15, 2.05)
            .max(0.5 - self.run.wave as f32 * 0.03);
    }

    fn check_end_state(&mut self, world: &mut World) {
        if self.run.house_hp <= 0 {
            self.mode = GameMode::GameOver;
            set_clear_color(world, Color::rgb(0.09, 0.025, 0.035));
        } else if self.run.wave > FINAL_WAVE && self.level.zombies.is_empty() {
            self.mode = GameMode::Victory;
            set_clear_color(world, Color::rgb(0.03, 0.08, 0.045));
            spawn_victory_suns(world, &self.assets, &mut self.level);
        }
    }

    fn update_hud(&mut self, ctx: &FrameContext<'_>) {
        self.hud_frame = self.hud_frame.wrapping_add(1);
        if self.hud_frame % 12 != 0 {
            return;
        }
        ctx.set_title(&format!("Lawn Defense | score {}", self.run.score));
    }

    fn set_title(&mut self, ctx: &FrameContext<'_>, title: &str) {
        self.hud_frame = self.hud_frame.wrapping_add(1);
        if self.hud_frame % 12 == 0 {
            ctx.set_title(&format!("Lawn Defense | {title}"));
        }
    }

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
                "start" | "restart" => self.start_run(ctx.world),
                "resume" => {
                    self.mode = GameMode::Playing;
                    set_clear_color(ctx.world, Color::rgb(0.035, 0.055, 0.035));
                }
                "pause" if self.mode == GameMode::Playing => {
                    self.mode = GameMode::Paused;
                }
                _ => {}
            }
        }
    }

    fn sync_ui(&mut self, world: &mut World) {
        let Some(ui) = self.ui else {
            return;
        };

        set_ui_text(
            world,
            ui.hud_text,
            format!(
                "sun {}   selected {} ${}   wave {}/{}   zombies {}   score {}",
                self.run.sun,
                self.run.selected.name(),
                self.run.selected.cost(),
                self.run.wave.min(FINAL_WAVE),
                FINAL_WAVE,
                self.level.zombies.len(),
                self.run.score,
            ),
        );
        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.house_bar) {
            bar.value = self.run.house_hp.max(0) as f32;
            bar.fill_color = if self.run.house_hp >= 2 {
                Color::rgba8(85, 220, 135, 255)
            } else {
                Color::rgba8(230, 82, 82, 255)
            };
        }
        if let Some(bar) = world.get_mut::<UiProgressBar>(ui.wave_bar) {
            let wave_progress = if self.run.wave_size == 0 {
                0.0
            } else {
                self.run.spawned_this_wave as f32 / self.run.wave_size as f32
            };
            bar.value = wave_progress.clamp(0.0, 1.0);
        }

        set_node_visible(world, ui.pause_button, self.mode == GameMode::Playing);
        let menu_visible = self.mode != GameMode::Playing;
        set_node_visible(world, ui.menu_panel, menu_visible);
        if let Some(node) = world.get_mut::<UiNode>(ui.menu_panel) {
            node.enabled = menu_visible;
        }

        match self.mode {
            GameMode::Title => {
                set_ui_text(world, ui.menu_title, "Lawn Defense");
                set_ui_text(
                    world,
                    ui.menu_body,
                    "Plant defenders, collect sun, survive five waves.",
                );
                set_button(world, ui.primary_button, "start", "Start");
                set_button(world, ui.secondary_button, "restart", "Restart");
                set_node_visible(world, ui.secondary_button, false);
            }
            GameMode::Paused => {
                set_ui_text(world, ui.menu_title, "Paused");
                set_ui_text(world, ui.menu_body, "Resume or restart the run.");
                set_button(world, ui.primary_button, "resume", "Resume");
                set_button(world, ui.secondary_button, "restart", "Restart");
                set_node_visible(world, ui.secondary_button, true);
            }
            GameMode::Victory => {
                set_ui_text(world, ui.menu_title, "Victory");
                set_ui_text(
                    world,
                    ui.menu_body,
                    format!("Score {}. The lawn held.", self.run.score),
                );
                set_button(world, ui.primary_button, "restart", "Play Again");
                set_node_visible(world, ui.secondary_button, false);
            }
            GameMode::GameOver => {
                set_ui_text(world, ui.menu_title, "Overrun");
                set_ui_text(
                    world,
                    ui.menu_body,
                    format!("Wave {} reached the house.", self.run.wave),
                );
                set_button(world, ui.primary_button, "restart", "Retry");
                set_node_visible(world, ui.secondary_button, false);
            }
            GameMode::Playing => {}
        }
    }
}

impl AppState for LawnDefenseGame {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        self.assets.load(world);
        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.035, 0.055, 0.035),
            ..Default::default()
        });
        spawn_camera(world);
        spawn_lawn(world);
        spawn_cards(world);
        self.ui = Some(spawn_lawn_ui(world));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.update_ui();
        self.handle_ui_events(ctx);
        match self.mode {
            GameMode::Title => self.update_title(ctx),
            GameMode::Playing => self.update_playing(ctx),
            GameMode::Paused => self.update_paused(ctx),
            GameMode::Victory | GameMode::GameOver => self.update_finished(ctx),
        }
        self.sync_ui(ctx.world);
        ctx.render_ui();
    }

    fn shutdown(&mut self, world: &mut World) {
        self.level.clear_dynamic(world);
        self.assets.unload(world);
    }
}

#[derive(Default)]
struct GameAssets {
    circle: Option<Handle<TextureAsset>>,
}

impl GameAssets {
    fn load(&mut self, world: &World) {
        self.circle = world
            .get_resource::<AssetServer>()
            .map(|server| server.insert_runtime(TextureAsset::circle(96)));
    }

    fn unload(&mut self, world: &World) {
        let Some(circle) = self.circle.take() else {
            return;
        };
        if let Some(server) = world.get_resource::<AssetServer>().cloned() {
            server.unload(&circle);
        }
    }

    fn circle_sprite(&self, size: f32, color: Color) -> SpriteRenderer {
        let mut sprite = SpriteRenderer::new(size, size).color(color);
        if let Some(circle) = self.circle {
            sprite = sprite.texture(circle);
        }
        sprite
    }
}

struct LevelState {
    dynamic: Vec<EntityId>,
    zombies: Vec<EntityId>,
    peas: Vec<EntityId>,
    suns: Vec<EntityId>,
    mowers: Vec<EntityId>,
    plants: [[Option<EntityId>; COLS]; ROWS],
}

impl LevelState {
    fn new() -> Self {
        Self {
            dynamic: Vec::new(),
            zombies: Vec::new(),
            peas: Vec::new(),
            suns: Vec::new(),
            mowers: Vec::new(),
            plants: [[None; COLS]; ROWS],
        }
    }

    fn clear_dynamic(&mut self, world: &mut World) {
        for entity in self.dynamic.drain(..) {
            if world.contains(entity) {
                let _ = world.despawn(entity);
            }
        }
        self.zombies.clear();
        self.peas.clear();
        self.suns.clear();
        self.mowers.clear();
        self.plants = [[None; COLS]; ROWS];
    }

    fn retain_live(&mut self, world: &World) {
        self.dynamic.retain(|entity| world.contains(*entity));
        self.zombies.retain(|entity| world.contains(*entity));
        self.peas.retain(|entity| world.contains(*entity));
        self.suns.retain(|entity| world.contains(*entity));
        self.mowers.retain(|entity| world.contains(*entity));
        for row in 0..ROWS {
            for col in 0..COLS {
                if self.plants[row][col].is_some_and(|entity| !world.contains(entity)) {
                    self.plants[row][col] = None;
                }
            }
        }
    }
}

struct RunState {
    sun: i32,
    score: i32,
    house_hp: i32,
    wave: u32,
    wave_size: usize,
    spawned_this_wave: usize,
    spawn_timer: f32,
    wave_timer: f32,
    sun_drop_timer: f32,
    elapsed: f32,
    selected: PlantKind,
    restart_requested: bool,
}

impl RunState {
    fn new() -> Self {
        Self {
            sun: 150,
            score: 0,
            house_hp: 3,
            wave: 1,
            wave_size: 10,
            spawned_this_wave: 0,
            spawn_timer: 1.5,
            wave_timer: 1.5,
            sun_drop_timer: 5.0,
            elapsed: 0.0,
            selected: PlantKind::Peashooter,
            restart_requested: false,
        }
    }
}

fn main() {
    App::new(
        AppConfig::new("Lawn Defense", WINDOW_W, WINDOW_H)
            .with_vsync(false)
            .with_resizable(false),
        World::new(),
    )
    .with_render_pipeline(
        RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build(),
    )
    .run(LawnDefenseGame::new());
}

fn spawn_camera(world: &mut World) {
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        CameraMarker::new(),
        Projection::orthographic(ORTHO_HEIGHT),
        MainCamera,
    ));
}

fn spawn_lawn(world: &mut World) {
    world.spawn((
        Transform::from_xyz(10.0, 0.0, -0.5),
        SpriteRenderer::new(1080.0, 600.0).color(Color::rgb(0.04, 0.13, 0.045)),
        SortingLayer(-80),
    ));

    for row in 0..ROWS {
        for col in 0..COLS {
            let center = cell_center(row, col);
            let shade = if (row + col) % 2 == 0 { 0.0 } else { 0.018 };
            world.spawn((
                Transform::from_xyz(center.x(), center.y(), -0.1),
                SpriteRenderer::new(CELL_W - 5.0, CELL_H - 5.0).color(Color::rgb(
                    0.14 + shade,
                    0.39 + shade,
                    0.12 + shade,
                )),
                SortingLayer(-20),
                LawnPulse {
                    base_width: CELL_W - 5.0,
                    base_height: CELL_H - 5.0,
                    phase: (row * 7 + col * 3) as f32 * 0.17,
                    speed: 0.6,
                    amplitude: 0.01,
                },
            ));
        }
    }

    world.spawn((
        Transform::from_xyz(HOUSE_X - 38.0, 0.0, 0.0),
        SpriteRenderer::new(52.0, ROWS as f32 * CELL_H + 30.0).color(Color::rgb(0.36, 0.18, 0.11)),
        SortingLayer(-5),
    ));
}

fn spawn_cards(world: &mut World) {
    for (i, kind) in PlantKind::all().into_iter().enumerate() {
        let x = -462.0 + i as f32 * 96.0;
        world.spawn((
            Transform::from_xyz(x, 323.0, 0.0),
            SpriteRenderer::new(76.0, 62.0).color(kind.color()),
            SortingLayer(60),
            Card { kind },
            LawnPulse {
                base_width: 76.0,
                base_height: 62.0,
                phase: i as f32 * 0.4,
                speed: 1.2,
                amplitude: 0.02,
            },
        ));
    }
}

fn spawn_lawn_ui(world: &mut World) -> LawnUi {
    let hud_panel = world.spawn((
        UiNode::panel(760.0, 94.0)
            .anchor(UiAnchor::TopLeft)
            .at(16.0, 14.0)
            .z(100)
            .layout(UiLayout::column(
                UiRect::new(16.0, 12.0, 16.0, 12.0),
                10.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(16, 22, 26, 224)),
    ));
    let hud_text = world.spawn((
        UiNode::panel(1.0, 24.0)
            .child_of(hud_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("")
            .size(18.0)
            .color(Color::rgba8(238, 246, 220, 255)),
    ));
    let bars_row = world.spawn((UiNode::panel(1.0, 22.0)
        .child_of(hud_panel)
        .width(UiLength::Percent(1.0))
        .layout(UiLayout::row(UiRect::ZERO, 14.0, UiAlign::Center)),));
    let house_bar = world.spawn((
        UiNode::panel(230.0, 16.0).child_of(bars_row),
        UiProgressBar {
            value: 3.0,
            max: 3.0,
            fill_color: Color::rgba8(85, 220, 135, 255),
            background_color: Color::rgba8(36, 42, 32, 230),
        },
    ));
    let wave_bar = world.spawn((
        UiNode::panel(230.0, 16.0).child_of(bars_row),
        UiProgressBar {
            value: 0.0,
            max: 1.0,
            fill_color: Color::rgba8(94, 172, 250, 255),
            background_color: Color::rgba8(30, 36, 46, 230),
        },
    ));

    let pause_button = world.spawn((
        UiNode::panel(104.0, 40.0)
            .id(UiId::new("pause"))
            .anchor(UiAnchor::TopRight)
            .at(18.0, 18.0)
            .z(110),
        UiButton::new("Pause"),
    ));

    let menu_panel = world.spawn((
        UiNode::panel(440.0, 260.0)
            .anchor(UiAnchor::Center)
            .z(200)
            .layout(UiLayout::column(
                UiRect::new(28.0, 24.0, 28.0, 24.0),
                12.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(18, 25, 28, 242)),
    ));
    let menu_title = world.spawn((
        UiNode::panel(1.0, 42.0)
            .child_of(menu_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("")
            .size(28.0)
            .align(UiAlign::Center)
            .color(Color::rgba8(248, 248, 226, 255)),
    ));
    let menu_body = world.spawn((
        UiNode::panel(1.0, 52.0)
            .child_of(menu_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("")
            .size(17.0)
            .align(UiAlign::Center)
            .color(Color::rgba8(196, 212, 184, 255)),
    ));
    let primary_button = world.spawn((
        UiNode::panel(1.0, 42.0)
            .id(UiId::new("start"))
            .child_of(menu_panel)
            .width(UiLength::Percent(1.0)),
        UiButton::new("Start"),
    ));
    let secondary_button = world.spawn((
        UiNode::panel(1.0, 42.0)
            .id(UiId::new("restart"))
            .child_of(menu_panel)
            .width(UiLength::Percent(1.0)),
        UiButton::new("Restart"),
    ));

    LawnUi {
        hud_text,
        house_bar,
        wave_bar,
        pause_button,
        menu_panel,
        menu_title,
        menu_body,
        primary_button,
        secondary_button,
    }
}

fn spawn_mowers(world: &mut World, assets: &GameAssets, level: &mut LevelState) {
    for row in 0..ROWS {
        let y = lane_y(row);
        let mower = world.spawn((
            Transform::from_xyz(BOARD_LEFT - 48.0, y, 0.15),
            assets.circle_sprite(38.0, Color::rgb(0.9, 0.12, 0.12)),
            SortingLayer(35),
            Mower { row, active: false },
        ));
        level.mowers.push(mower);
        level.dynamic.push(mower);
    }
}

fn spawn_plant(
    world: &mut World,
    assets: &GameAssets,
    row: usize,
    col: usize,
    kind: PlantKind,
) -> EntityId {
    let center = cell_center(row, col);
    let size = match kind {
        PlantKind::Wallnut => 56.0,
        _ => 46.0,
    };
    world.spawn((
        Transform::from_xyz(center.x(), center.y(), 0.2),
        assets.circle_sprite(size, kind.color()),
        SortingLayer(20 + row as i32),
        Plant {
            kind,
            hp: kind.hp(),
            cooldown: kind.cooldown() * 0.5,
            flash: 0.0,
        },
    ))
}

fn spawn_zombie(
    world: &mut World,
    assets: &GameAssets,
    row: usize,
    wave: u32,
    tough: bool,
) -> EntityId {
    let hp = if tough { 9 } else { 5 + wave as i32 };
    let size = if tough { 58.0 } else { 47.0 };
    let speed = if tough {
        18.0
    } else {
        25.0 + wave as f32 * 1.5
    };
    let color = if tough {
        Color::rgb(0.58, 0.5, 0.72)
    } else {
        Color::rgb(0.62, 0.72, 0.58)
    };
    world.spawn((
        Transform::from_xyz(SPAWN_X, lane_y(row), 0.24),
        assets.circle_sprite(size, color),
        SortingLayer(30 + row as i32),
        Zombie {
            row,
            hp,
            speed,
            bite_timer: 0.0,
            flash: 0.0,
        },
    ))
}

fn spawn_pea(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelState,
    position: Vec2,
    row: usize,
    splash: bool,
) {
    let color = if splash {
        Color::rgb(0.24, 1.0, 0.74)
    } else {
        Color::rgb(0.5, 1.0, 0.25)
    };
    let pea = world.spawn((
        Transform::from_xyz(position.x(), position.y(), 0.32),
        assets.circle_sprite(if splash { 16.0 } else { 12.0 }, color),
        SortingLayer(50 + row as i32),
        Pea {
            row,
            damage: if splash { 2 } else { 1 },
            splash,
        },
    ));
    level.peas.push(pea);
    level.dynamic.push(pea);
}

fn spawn_sun(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelState,
    position: Vec2,
    target_y: f32,
    value: i32,
) {
    let sun = world.spawn((
        Transform::from_xyz(position.x(), position.y(), 0.4),
        assets.circle_sprite(34.0, Color::rgb(1.0, 0.82, 0.12)),
        SortingLayer(70),
        Sun {
            value,
            ttl: 9.5,
            target_y,
            fall_speed: 42.0,
        },
        LawnPulse {
            base_width: 34.0,
            base_height: 34.0,
            phase: position.x() * 0.03,
            speed: 4.0,
            amplitude: 0.14,
        },
    ));
    level.suns.push(sun);
    level.dynamic.push(sun);
}

fn update_plants(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelState,
    run: &mut RunState,
    rng: &mut SimpleRng,
    dt: f32,
) {
    let mut dead = Vec::new();
    for row in 0..ROWS {
        for col in 0..COLS {
            let Some(entity) = level.plants[row][col] else {
                continue;
            };
            if !world.contains(entity) {
                level.plants[row][col] = None;
                continue;
            }

            let (kind, x, y, should_fire, should_sun) = {
                let Some(transform) = world.get::<Transform>(entity) else {
                    continue;
                };
                let x = transform.x();
                let y = transform.y();
                let zombies_ahead = lane_has_zombie_ahead(world, row, x);
                let Some(plant) = world.get_mut::<Plant>(entity) else {
                    continue;
                };
                plant.cooldown -= dt;
                plant.flash = (plant.flash - dt * 4.0).max(0.0);
                let should_fire = matches!(plant.kind, PlantKind::Peashooter | PlantKind::Sprayer)
                    && zombies_ahead
                    && plant.cooldown <= 0.0;
                let should_sun = plant.kind == PlantKind::Sunflower && plant.cooldown <= 0.0;
                if should_fire || should_sun {
                    plant.cooldown = plant.kind.cooldown();
                }
                if plant.hp <= 0 {
                    dead.push((row, col, entity));
                }
                (plant.kind, x, y, should_fire, should_sun)
            };

            if should_fire {
                spawn_pea(
                    world,
                    assets,
                    level,
                    Vec2::new(x + 31.0, y + 8.0),
                    row,
                    kind == PlantKind::Sprayer,
                );
            }
            if should_sun {
                let offset = Vec2::new(rng.range(-18.0, 18.0), rng.range(-10.0, 18.0));
                spawn_sun(world, assets, level, Vec2::new(x, y) + offset, y - 8.0, 25);
                run.score += 2;
            }
        }
    }

    for (row, col, entity) in dead {
        level.plants[row][col] = None;
        if world.contains(entity) {
            let _ = world.despawn(entity);
        }
    }
}

fn update_zombies(world: &mut World, level: &mut LevelState, run: &mut RunState, dt: f32) {
    let mut dead_plants = Vec::new();
    let mut escaped = Vec::new();
    let zombies = level.zombies.clone();
    for zombie in zombies {
        if !world.contains(zombie) {
            continue;
        }
        let (row, speed) = {
            let Some(zombie_data) = world.get_mut::<Zombie>(zombie) else {
                continue;
            };
            zombie_data.bite_timer -= dt;
            zombie_data.flash = (zombie_data.flash - dt * 4.0).max(0.0);
            (zombie_data.row, zombie_data.speed)
        };

        let Some(zombie_x) = entity_position(world, zombie).map(|p| p.x()) else {
            continue;
        };
        let target = blocking_plant_in_row(world, level, row, zombie_x);
        if let Some((plant_entity, plant_row, plant_col, plant_x)) = target {
            if zombie_x - plant_x < 42.0 {
                if let Some(zombie_data) = world.get_mut::<Zombie>(zombie) {
                    if zombie_data.bite_timer <= 0.0 {
                        zombie_data.bite_timer = 0.78;
                        if let Some(plant) = world.get_mut::<Plant>(plant_entity) {
                            plant.hp -= 1;
                            plant.flash = 1.0;
                            if plant.hp <= 0 {
                                dead_plants.push((plant_row, plant_col, plant_entity));
                            }
                        }
                    }
                }
                continue;
            }
        }

        if let Some(transform) = world.get_mut::<Transform>(zombie) {
            transform.position[0] -= speed * dt;
            if transform.position[0] < HOUSE_X {
                escaped.push(zombie);
            }
        }
    }

    for (row, col, plant) in dead_plants {
        level.plants[row][col] = None;
        if world.contains(plant) {
            let _ = world.despawn(plant);
        }
    }
    for zombie in escaped {
        if world.contains(zombie) {
            let _ = world.despawn(zombie);
            run.house_hp -= 1;
        }
    }
}

fn update_projectiles(world: &mut World, level: &mut LevelState, run: &mut RunState, dt: f32) {
    let mut remove_peas = Vec::new();
    let mut remove_zombies = Vec::new();
    let peas = level.peas.clone();

    for pea in peas {
        if !world.contains(pea) {
            continue;
        }
        let (row, damage, splash) = match world.get::<Pea>(pea) {
            Some(pea_data) => (pea_data.row, pea_data.damage, pea_data.splash),
            None => continue,
        };
        let Some(transform) = world.get_mut::<Transform>(pea) else {
            continue;
        };
        transform.position[0] += PROJECTILE_SPEED * dt;
        let pea_pos = Vec2::new(transform.position[0], transform.position[1]);
        if pea_pos.x() > SPAWN_X + 70.0 {
            remove_peas.push(pea);
            continue;
        }

        let Some(hit) = first_zombie_hit(world, row, pea_pos) else {
            continue;
        };
        remove_peas.push(pea);
        damage_zombie(world, hit, damage, &mut remove_zombies, run);
        if splash {
            let splash_hits = zombies_near(world, row, pea_pos, 58.0);
            for zombie in splash_hits {
                if zombie != hit {
                    damage_zombie(world, zombie, 1, &mut remove_zombies, run);
                }
            }
        }
    }

    for pea in remove_peas {
        if world.contains(pea) {
            let _ = world.despawn(pea);
        }
    }
    for zombie in remove_zombies {
        if world.contains(zombie) {
            let _ = world.despawn(zombie);
            run.score += 100;
        }
    }
}

fn damage_zombie(
    world: &mut World,
    zombie: EntityId,
    damage: i32,
    remove_zombies: &mut Vec<EntityId>,
    run: &mut RunState,
) {
    let Some(zombie_data) = world.get_mut::<Zombie>(zombie) else {
        return;
    };
    zombie_data.hp -= damage;
    zombie_data.flash = 1.0;
    run.score += 8;
    if zombie_data.hp <= 0 && !remove_zombies.contains(&zombie) {
        remove_zombies.push(zombie);
    }
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(zombie) {
        sprite.color = Color::rgb(0.98, 0.95, 0.72);
    }
}

fn update_suns(world: &mut World, level: &mut LevelState, dt: f32) {
    let suns = level.suns.clone();
    let mut remove = Vec::new();
    for sun in suns {
        if !world.contains(sun) {
            continue;
        }
        let (target_y, fall_speed, ttl) = {
            let Some(sun_data) = world.get_mut::<Sun>(sun) else {
                continue;
            };
            sun_data.ttl -= dt;
            (sun_data.target_y, sun_data.fall_speed, sun_data.ttl)
        };
        if ttl <= 0.0 {
            remove.push(sun);
            continue;
        }
        if let Some(transform) = world.get_mut::<Transform>(sun) {
            if transform.position[1] > target_y {
                transform.position[1] = (transform.position[1] - fall_speed * dt).max(target_y);
            }
        }
    }
    for sun in remove {
        if world.contains(sun) {
            let _ = world.despawn(sun);
        }
    }
    level.retain_live(world);
}

fn update_mowers(world: &mut World, level: &mut LevelState, run: &mut RunState, dt: f32) {
    let mowers = level.mowers.clone();
    let mut remove_mowers = Vec::new();
    let mut remove_zombies = Vec::new();
    for mower in mowers {
        if !world.contains(mower) {
            continue;
        }
        let (row, active) = match world.get::<Mower>(mower) {
            Some(mower_data) => (mower_data.row, mower_data.active),
            None => continue,
        };
        let should_start = !active && lane_has_zombie_past(world, row, BOARD_LEFT - 16.0);
        if should_start {
            if let Some(mower_data) = world.get_mut::<Mower>(mower) {
                mower_data.active = true;
            }
        }

        let active = world
            .get::<Mower>(mower)
            .map(|mower| mower.active)
            .unwrap_or(false);
        if !active {
            continue;
        }
        let Some(transform) = world.get_mut::<Transform>(mower) else {
            continue;
        };
        transform.position[0] += MOWER_SPEED * dt;
        let mower_x = transform.position[0];
        for zombie in zombies_near(world, row, Vec2::new(mower_x, lane_y(row)), 56.0) {
            if !remove_zombies.contains(&zombie) {
                remove_zombies.push(zombie);
            }
        }
        if mower_x > SPAWN_X + 80.0 {
            remove_mowers.push(mower);
        }
    }

    for zombie in remove_zombies {
        if world.contains(zombie) {
            let _ = world.despawn(zombie);
            run.score += 80;
        }
    }
    for mower in remove_mowers {
        if world.contains(mower) {
            let _ = world.despawn(mower);
        }
    }
}

fn spawn_ambient_sun(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelState,
    run: &mut RunState,
    rng: &mut SimpleRng,
) {
    if run.sun_drop_timer > 0.0 {
        return;
    }
    run.sun_drop_timer = rng.range(7.0, 10.0);
    let x = rng.range(BOARD_LEFT + 20.0, BOARD_LEFT + CELL_W * COLS as f32 - 20.0);
    let target_y = rng.range(BOARD_TOP - CELL_H * ROWS as f32 + 20.0, BOARD_TOP - 30.0);
    spawn_sun(world, assets, level, Vec2::new(x, 310.0), target_y, 25);
}

fn update_cards(world: &mut World, selected: PlantKind, sun: i32) {
    let mut cards = world.query::<(&mut SpriteRenderer, &Card)>();
    cards.for_each(world, |(sprite, card)| {
        let affordable = sun >= card.kind.cost();
        let mut color = card.kind.color();
        if !affordable {
            color.r *= 0.45;
            color.g *= 0.45;
            color.b *= 0.45;
        }
        if card.kind == selected {
            color.a = 1.0;
            sprite.width = 86.0;
            sprite.height = 70.0;
        } else {
            color.a = 0.72;
            sprite.width = 76.0;
            sprite.height = 62.0;
        }
        sprite.color = color;
    });
}

fn flash_cards(world: &mut World, kind: PlantKind) {
    let mut cards = world.query::<(&mut SpriteRenderer, &Card)>();
    cards.for_each(world, |(sprite, card)| {
        if card.kind == kind {
            sprite.color = Color::rgb(1.0, 0.25, 0.25);
        }
    });
}

fn spawn_victory_suns(world: &mut World, assets: &GameAssets, level: &mut LevelState) {
    for row in 0..ROWS {
        for col in [1, 3, 5, 7] {
            let center = cell_center(row, col);
            spawn_sun(
                world,
                assets,
                level,
                center + Vec2::new(0.0, 36.0),
                center.y(),
                25,
            );
        }
    }
}

fn animate_lawn(world: &mut World, dt: f32) {
    let mut pulses = world.query::<(&mut SpriteRenderer, &mut LawnPulse)>();
    pulses.for_each(world, |(sprite, pulse)| {
        pulse.phase += pulse.speed * dt;
        let scale = 1.0 + pulse.phase.sin() * pulse.amplitude;
        sprite.width = pulse.base_width * scale;
        sprite.height = pulse.base_height * scale;
    });
}

fn lane_has_zombie_ahead(world: &World, row: usize, x: f32) -> bool {
    let mut found = false;
    let mut query = world.query::<(&Transform, &Zombie)>();
    query.for_each(world, |(transform, zombie)| {
        if zombie.row == row && transform.x() > x {
            found = true;
        }
    });
    found
}

fn blocking_plant_in_row(
    world: &World,
    level: &LevelState,
    row: usize,
    zombie_x: f32,
) -> Option<(EntityId, usize, usize, f32)> {
    let mut best = None;
    let mut best_x = f32::NEG_INFINITY;
    for col in 0..COLS {
        let Some(plant) = level.plants[row][col] else {
            continue;
        };
        let Some(x) = entity_position(world, plant).map(|p| p.x()) else {
            continue;
        };
        if x <= zombie_x && x > best_x {
            best_x = x;
            best = Some((plant, row, col, x));
        }
    }
    best
}

fn first_zombie_hit(world: &World, row: usize, pea_pos: Vec2) -> Option<EntityId> {
    let mut hit = None;
    let mut hit_x = f32::MAX;
    let mut query = world.query::<(&Transform, &Zombie)>();
    query.for_each_with_entity(world, |entity, (transform, zombie)| {
        let x = transform.x();
        if zombie.row == row
            && x >= pea_pos.x() - 8.0
            && (x - pea_pos.x()).abs() < 26.0
            && x < hit_x
        {
            hit = Some(entity);
            hit_x = x;
        }
    });
    hit
}

fn zombies_near(world: &World, row: usize, center: Vec2, radius: f32) -> Vec<EntityId> {
    let mut hits = Vec::new();
    let radius_sq = radius * radius;
    let mut query = world.query::<(&Transform, &Zombie)>();
    query.for_each_with_entity(world, |entity, (transform, zombie)| {
        let position = Vec2::new(transform.x(), transform.y());
        if zombie.row == row && (position - center).length_squared() <= radius_sq {
            hits.push(entity);
        }
    });
    hits
}

fn lane_has_zombie_past(world: &World, row: usize, x: f32) -> bool {
    let mut found = false;
    let mut query = world.query::<(&Transform, &Zombie)>();
    query.for_each(world, |(transform, zombie)| {
        if zombie.row == row && transform.x() < x {
            found = true;
        }
    });
    found
}

fn entity_position(world: &World, entity: EntityId) -> Option<Vec2> {
    world
        .get::<Transform>(entity)
        .map(|transform| Vec2::new(transform.x(), transform.y()))
}

fn mouse_world(ctx: &FrameContext<'_>) -> Vec2 {
    let [mx, my] = ctx.input.mouse_position();
    let width = WINDOW_W as f32;
    let height = WINDOW_H as f32;
    let world_h = ORTHO_HEIGHT;
    let world_w = ORTHO_HEIGHT * width / height;
    Vec2::new((mx / width - 0.5) * world_w, (0.5 - my / height) * world_h)
}

fn card_at(position: Vec2) -> Option<PlantKind> {
    for (i, kind) in PlantKind::all().into_iter().enumerate() {
        let x = -462.0 + i as f32 * 96.0;
        if (position.x() - x).abs() <= 43.0 && (position.y() - 323.0).abs() <= 38.0 {
            return Some(kind);
        }
    }
    None
}

fn cell_at(position: Vec2) -> Option<(usize, usize)> {
    let x = position.x() - BOARD_LEFT;
    let y = BOARD_TOP - position.y();
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let col = (x / CELL_W).floor() as usize;
    let row = (y / CELL_H).floor() as usize;
    if row < ROWS && col < COLS {
        Some((row, col))
    } else {
        None
    }
}

fn cell_center(row: usize, col: usize) -> Vec2 {
    Vec2::new(
        BOARD_LEFT + col as f32 * CELL_W + CELL_W * 0.5,
        BOARD_TOP - row as f32 * CELL_H - CELL_H * 0.5,
    )
}

fn lane_y(row: usize) -> f32 {
    cell_center(row, 0).y()
}

fn set_clear_color(world: &mut World, color: Color) {
    if let Some(settings) = world.get_resource_mut::<RenderSettings>() {
        settings.clear_color = color;
    }
}

fn set_ui_text(world: &mut World, entity: EntityId, text: impl Into<String>) {
    if let Some(ui_text) = world.get_mut::<UiText>(entity) {
        ui_text.text = text.into();
    }
}

fn set_button(world: &mut World, entity: EntityId, id: &'static str, label: &'static str) {
    if let Some(node) = world.get_mut::<UiNode>(entity) {
        node.id = Some(UiId::new(id));
        node.visible = true;
        node.enabled = true;
    }
    if let Some(button) = world.get_mut::<UiButton>(entity) {
        button.label = label.to_string();
    }
}

fn set_node_visible(world: &mut World, entity: EntityId, visible: bool) {
    if let Some(node) = world.get_mut::<UiNode>(entity) {
        node.visible = visible;
        node.enabled = visible;
    }
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }

    fn range_usize(&mut self, min: usize, max: usize) -> usize {
        min + (self.next_u64() as usize % (max - min).max(1))
    }

    fn chance(&mut self, probability: f32) -> bool {
        self.next_f32() < probability
    }
}
