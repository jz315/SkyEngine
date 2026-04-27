//! A complete small platformer built with Kenney's New Platformer Pack.
//!
//! ```bash
//! cargo run --example kenney_platformer_game --features ui --release
//! ```

use std::path::{Path, PathBuf};

use image::ImageReader;
use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
use sky_engine::asset::{AssetServer, Handle, TextureAsset, TextureColorSpace};
use sky_engine::ecs::{EntityId, World};
use sky_engine::input::KeyCode;
use sky_engine::math::Vec2;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SortingLayer,
    SpriteFeature, SpriteRenderer, Transform, TransparentPhase,
};
use sky_engine::ui::{
    UiAlign, UiAnchor, UiButton, UiEventKind, UiEvents, UiId, UiLayout, UiLength, UiNode, UiPanel,
    UiRect, UiText,
};

const WINDOW_W: u32 = 1280;
const WINDOW_H: u32 = 720;
const ORTHO_HEIGHT: f32 = 640.0;
const TILE: f32 = 48.0;
const LEVEL_COLS: usize = 86;
const LEVEL_ROWS: usize = 14;
const LEVEL_W: f32 = LEVEL_COLS as f32 * TILE;
const LEVEL_H: f32 = LEVEL_ROWS as f32 * TILE;
const PLAYER_HALF_X: f32 = 16.0;
const PLAYER_HALF_Y: f32 = 30.0;
const PLAYER_DRAW_SIZE: f32 = 82.0;
const PLAYER_ACCEL: f32 = 2400.0;
const PLAYER_FRICTION: f32 = 1900.0;
const PLAYER_MAX_SPEED: f32 = 330.0;
const GRAVITY: f32 = -1850.0;
const JUMP_SPEED: f32 = 720.0;
const SPRING_SPEED: f32 = 980.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameMode {
    Title,
    Playing,
    Paused,
    Victory,
    GameOver,
}

#[derive(Clone, Copy)]
struct Player {
    velocity: Vec2,
    grounded: bool,
    facing: f32,
    hurt_timer: f32,
    coyote_timer: f32,
    jump_buffer: f32,
    walk_cycle: f32,
}

#[derive(Clone, Copy)]
struct Enemy {
    left: f32,
    right: f32,
    speed: f32,
    direction: f32,
    walk_cycle: f32,
}

#[derive(Clone, Copy)]
struct Door;

#[derive(Clone, Copy)]
struct Flag {
    phase: f32,
}

#[derive(Clone, Copy)]
struct Tile;

#[derive(Clone, Copy)]
struct HazardMarker;

#[derive(Clone, Copy)]
struct SpringMarker;

#[derive(Clone, Copy)]
struct CollectibleMarker;

#[derive(Clone, Copy)]
struct Rect {
    center: Vec2,
    half: Vec2,
}

impl Rect {
    fn new(center: Vec2, width: f32, height: f32) -> Self {
        Self {
            center,
            half: Vec2::new(width * 0.5, height * 0.5),
        }
    }

    fn from_entity(center: Vec2, half: Vec2) -> Self {
        Self { center, half }
    }

    fn left(self) -> f32 {
        self.center.x() - self.half.x()
    }

    fn right(self) -> f32 {
        self.center.x() + self.half.x()
    }

    fn bottom(self) -> f32 {
        self.center.y() - self.half.y()
    }

    fn top(self) -> f32 {
        self.center.y() + self.half.y()
    }

    fn intersects(self, other: Self) -> bool {
        self.left() < other.right()
            && self.right() > other.left()
            && self.bottom() < other.top()
            && self.top() > other.bottom()
    }
}

#[derive(Clone, Copy)]
enum CollectibleKind {
    Coin,
    GemBlue,
    GemYellow,
    Key,
    Heart,
}

struct CollectibleState {
    entity: EntityId,
    kind: CollectibleKind,
    base: Vec2,
    phase: f32,
}

#[derive(Clone, Copy)]
enum HazardKind {
    Lava,
    Spikes,
    Saw,
}

struct HazardState {
    entity: EntityId,
    rect: Rect,
    kind: HazardKind,
    phase: f32,
}

struct SpringState {
    entity: EntityId,
    rect: Rect,
    timer: f32,
}

struct ParallaxSprite {
    entity: EntityId,
    slot: i32,
    span: f32,
    factor: f32,
}

#[derive(Default)]
struct LevelState {
    solids: Vec<Rect>,
    hazards: Vec<HazardState>,
    springs: Vec<SpringState>,
    backgrounds: Vec<ParallaxSprite>,
    dynamic: Vec<EntityId>,
    collectibles: Vec<CollectibleState>,
    enemies: Vec<EntityId>,
    player: Option<EntityId>,
    camera: Option<EntityId>,
    door: Option<EntityId>,
    flag: Option<EntityId>,
}

impl LevelState {
    fn clear_dynamic(&mut self, world: &mut World) {
        for entity in self.dynamic.drain(..) {
            if world.contains(entity) {
                let _ = world.despawn(entity);
            }
        }
        self.collectibles.clear();
        self.enemies.clear();
        self.player = None;
        self.door = None;
        self.flag = None;
    }

    fn retain_live(&mut self, world: &World) {
        self.dynamic.retain(|entity| world.contains(*entity));
        self.collectibles.retain(|item| world.contains(item.entity));
        self.enemies.retain(|entity| world.contains(*entity));
    }
}

#[derive(Clone, Copy)]
struct RunState {
    lives: i32,
    score: u32,
    coins: u32,
    gems: u32,
    has_key: bool,
    elapsed: f32,
    checkpoint: Vec2,
}

impl RunState {
    fn fresh() -> Self {
        Self {
            lives: 3,
            score: 0,
            coins: 0,
            gems: 0,
            has_key: false,
            elapsed: 0.0,
            checkpoint: player_spawn(),
        }
    }
}

#[derive(Clone, Copy)]
struct UiRefs {
    hud_panel: EntityId,
    hud_text: EntityId,
    key_text: EntityId,
    menu_panel: EntityId,
    menu_title: EntityId,
    menu_body: EntityId,
    primary_button: EntityId,
    secondary_button: EntityId,
    pause_button: EntityId,
}

#[derive(Default)]
struct PlatformerAssets {
    textures: Option<TextureSet>,
    handles: Vec<Handle<TextureAsset>>,
}

#[derive(Clone, Copy)]
struct TextureSet {
    bg_clouds: Handle<TextureAsset>,
    bg_color_hills: Handle<TextureAsset>,
    bg_fade_hills: Handle<TextureAsset>,
    player_idle: Handle<TextureAsset>,
    player_walk_a: Handle<TextureAsset>,
    player_walk_b: Handle<TextureAsset>,
    player_jump: Handle<TextureAsset>,
    player_duck: Handle<TextureAsset>,
    player_hit: Handle<TextureAsset>,
    grass: Handle<TextureAsset>,
    grass_top: Handle<TextureAsset>,
    grass_left: Handle<TextureAsset>,
    grass_right: Handle<TextureAsset>,
    dirt: Handle<TextureAsset>,
    stone: Handle<TextureAsset>,
    bridge: Handle<TextureAsset>,
    lava: Handle<TextureAsset>,
    lava_top: Handle<TextureAsset>,
    spikes: Handle<TextureAsset>,
    spring: Handle<TextureAsset>,
    spring_out: Handle<TextureAsset>,
    coin: Handle<TextureAsset>,
    gem_blue: Handle<TextureAsset>,
    gem_yellow: Handle<TextureAsset>,
    key: Handle<TextureAsset>,
    heart: Handle<TextureAsset>,
    door_closed: Handle<TextureAsset>,
    door_open: Handle<TextureAsset>,
    flag_a: Handle<TextureAsset>,
    flag_b: Handle<TextureAsset>,
    slime_a: Handle<TextureAsset>,
    slime_b: Handle<TextureAsset>,
    saw_a: Handle<TextureAsset>,
    saw_b: Handle<TextureAsset>,
    bush: Handle<TextureAsset>,
    rock: Handle<TextureAsset>,
    mushroom: Handle<TextureAsset>,
    torch_a: Handle<TextureAsset>,
}

impl PlatformerAssets {
    fn load(&mut self, world: &World) {
        let server = world
            .get_resource::<AssetServer>()
            .expect("App should install AssetServer before setup")
            .clone();
        let root = kenney_asset_root();
        let mut load = |relative: &str| {
            let handle = load_png_texture(&server, root.join(relative));
            self.handles.push(handle);
            handle
        };

        self.textures = Some(TextureSet {
            bg_clouds: load("Sprites/Backgrounds/Default/background_clouds.png"),
            bg_color_hills: load("Sprites/Backgrounds/Default/background_color_hills.png"),
            bg_fade_hills: load("Sprites/Backgrounds/Default/background_fade_hills.png"),
            player_idle: load("Sprites/Characters/Default/character_green_idle.png"),
            player_walk_a: load("Sprites/Characters/Default/character_green_walk_a.png"),
            player_walk_b: load("Sprites/Characters/Default/character_green_walk_b.png"),
            player_jump: load("Sprites/Characters/Default/character_green_jump.png"),
            player_duck: load("Sprites/Characters/Default/character_green_duck.png"),
            player_hit: load("Sprites/Characters/Default/character_green_hit.png"),
            grass: load("Sprites/Tiles/Default/terrain_grass_block.png"),
            grass_top: load("Sprites/Tiles/Default/terrain_grass_block_top.png"),
            grass_left: load("Sprites/Tiles/Default/terrain_grass_block_top_left.png"),
            grass_right: load("Sprites/Tiles/Default/terrain_grass_block_top_right.png"),
            dirt: load("Sprites/Tiles/Default/terrain_dirt_block_center.png"),
            stone: load("Sprites/Tiles/Default/terrain_stone_block.png"),
            bridge: load("Sprites/Tiles/Default/bridge.png"),
            lava: load("Sprites/Tiles/Default/lava.png"),
            lava_top: load("Sprites/Tiles/Default/lava_top.png"),
            spikes: load("Sprites/Tiles/Default/spikes.png"),
            spring: load("Sprites/Tiles/Default/spring.png"),
            spring_out: load("Sprites/Tiles/Default/spring_out.png"),
            coin: load("Sprites/Tiles/Default/coin_gold.png"),
            gem_blue: load("Sprites/Tiles/Default/gem_blue.png"),
            gem_yellow: load("Sprites/Tiles/Default/gem_yellow.png"),
            key: load("Sprites/Tiles/Default/key_yellow.png"),
            heart: load("Sprites/Tiles/Default/heart.png"),
            door_closed: load("Sprites/Tiles/Default/door_closed.png"),
            door_open: load("Sprites/Tiles/Default/door_open.png"),
            flag_a: load("Sprites/Tiles/Default/flag_blue_a.png"),
            flag_b: load("Sprites/Tiles/Default/flag_blue_b.png"),
            slime_a: load("Sprites/Enemies/Default/slime_normal_walk_a.png"),
            slime_b: load("Sprites/Enemies/Default/slime_normal_walk_b.png"),
            saw_a: load("Sprites/Enemies/Default/saw_a.png"),
            saw_b: load("Sprites/Enemies/Default/saw_b.png"),
            bush: load("Sprites/Tiles/Default/bush.png"),
            rock: load("Sprites/Tiles/Default/rock.png"),
            mushroom: load("Sprites/Tiles/Default/mushroom_red.png"),
            torch_a: load("Sprites/Tiles/Default/torch_on_a.png"),
        });
    }

    fn textures(&self) -> TextureSet {
        self.textures
            .expect("platformer textures should be loaded before use")
    }

    fn unload(&mut self, world: &World) {
        let Some(server) = world.get_resource::<AssetServer>().cloned() else {
            return;
        };
        for handle in self.handles.drain(..) {
            server.unload(&handle);
        }
        self.textures = None;
    }
}

struct KenneyPlatformerGame {
    mode: GameMode,
    assets: PlatformerAssets,
    level: LevelState,
    run: RunState,
    ui: Option<UiRefs>,
    title_frame: u32,
}

impl KenneyPlatformerGame {
    fn new() -> Self {
        Self {
            mode: GameMode::Title,
            assets: PlatformerAssets::default(),
            level: LevelState::default(),
            run: RunState::fresh(),
            ui: None,
            title_frame: 0,
        }
    }

    fn start(&mut self) {
        if self.mode == GameMode::Title {
            self.mode = GameMode::Playing;
        }
    }

    fn restart(&mut self, world: &mut World) {
        self.level.clear_dynamic(world);
        self.run = RunState::fresh();
        spawn_run(
            world,
            &self.assets.textures(),
            &mut self.level,
            &mut self.run,
        );
        self.mode = GameMode::Playing;
        set_clear_color(world, Color::rgb(0.40, 0.72, 0.92));
    }

    fn update_title(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::Space) || ctx.input.key_pressed(KeyCode::Enter) {
            self.start();
        }
        animate_idle_world(ctx.world, &self.assets.textures(), &mut self.level, ctx.dt);
        self.update_camera(ctx);
        ctx.render();
    }

    fn update_playing(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }
        if ctx.input.key_pressed(KeyCode::KeyP) {
            self.mode = GameMode::Paused;
            ctx.render();
            return;
        }
        if ctx.input.key_pressed(KeyCode::KeyR) {
            self.restart(ctx.world);
        }

        self.run.elapsed += ctx.dt;
        update_player(ctx, &self.assets.textures(), &mut self.level, &mut self.run);
        if self.run.lives <= 0 {
            self.mode = GameMode::GameOver;
            set_clear_color(ctx.world, Color::rgb(0.40, 0.16, 0.20));
        }
        update_enemies(ctx.world, &self.assets.textures(), &mut self.level, ctx.dt);
        animate_static_interactives(ctx.world, &self.assets.textures(), &mut self.level, ctx.dt);
        update_collectibles(ctx.world, &mut self.level, &mut self.run, ctx.dt);
        resolve_player_contacts(
            ctx.world,
            &self.assets.textures(),
            &mut self.level,
            &mut self.run,
            &mut self.mode,
        );
        update_door(
            ctx.world,
            &self.assets.textures(),
            &mut self.level,
            &self.run,
        );
        self.update_camera(ctx);
        self.level.retain_live(ctx.world);
        ctx.render();
    }

    fn update_paused(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::KeyP) {
            self.mode = GameMode::Playing;
        }
        if ctx.input.key_pressed(KeyCode::KeyR) {
            self.restart(ctx.world);
        }
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }
        animate_idle_world(ctx.world, &self.assets.textures(), &mut self.level, ctx.dt);
        self.update_camera(ctx);
        ctx.render();
    }

    fn update_finished(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::KeyR)
            || ctx.input.key_pressed(KeyCode::Space)
            || ctx.input.key_pressed(KeyCode::Enter)
        {
            self.restart(ctx.world);
        }
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }
        animate_idle_world(ctx.world, &self.assets.textures(), &mut self.level, ctx.dt);
        self.update_camera(ctx);
        ctx.render();
    }

    fn update_camera(&mut self, ctx: &mut FrameContext<'_>) {
        let Some(camera) = self.level.camera else {
            return;
        };
        let surface = ctx.surface_size();
        let view_w = ORTHO_HEIGHT * surface[0].max(1) as f32 / surface[1].max(1) as f32;
        let player_x = self
            .level
            .player
            .and_then(|entity| ctx.world.get::<Transform>(entity).map(|t| t.x()))
            .unwrap_or_else(|| player_spawn().x());
        let player_y = self
            .level
            .player
            .and_then(|entity| ctx.world.get::<Transform>(entity).map(|t| t.y()))
            .unwrap_or_else(|| player_spawn().y());
        let mut camera_x = player_x + 90.0;
        let mut camera_y = (player_y + 82.0).clamp(ORTHO_HEIGHT * 0.5, LEVEL_H - 220.0);
        camera_x = camera_x.clamp(view_w * 0.5, LEVEL_W - view_w * 0.5);
        if LEVEL_H <= ORTHO_HEIGHT {
            camera_y = LEVEL_H * 0.5;
        }

        if let Some(transform) = ctx.world.get_mut::<Transform>(camera) {
            let current = Vec2::new(transform.x(), transform.y());
            let target = Vec2::new(camera_x, camera_y);
            let t = (ctx.dt * 8.0).clamp(0.0, 1.0);
            let next = current.lerp(target, t);
            transform.position[0] = next.x();
            transform.position[1] = next.y();
        }
        if let Some(projection) = ctx.world.get_mut::<Projection>(camera) {
            *projection = Projection::orthographic(ORTHO_HEIGHT);
        }
        update_parallax(ctx.world, &self.level);
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
                "start" | "resume" => self.mode = GameMode::Playing,
                "restart" => self.restart(ctx.world),
                "pause" if self.mode == GameMode::Playing => self.mode = GameMode::Paused,
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
                "lives {}   coins {}   gems {}   score {}   time {:04.1}",
                self.run.lives.max(0),
                self.run.coins,
                self.run.gems,
                self.run.score,
                self.run.elapsed
            ),
        );
        set_ui_text(
            world,
            ui.key_text,
            if self.run.has_key {
                "key ready"
            } else {
                "key missing"
            },
        );

        set_node_visible(world, ui.pause_button, self.mode == GameMode::Playing);
        set_node_visible(world, ui.hud_panel, self.mode != GameMode::Title);
        set_node_visible(world, ui.key_text, self.mode != GameMode::Title);

        let menu_visible = self.mode != GameMode::Playing;
        set_node_visible(world, ui.menu_panel, menu_visible);
        if let Some(node) = world.get_mut::<UiNode>(ui.menu_panel) {
            node.enabled = menu_visible;
        }

        match self.mode {
            GameMode::Title => {
                set_ui_text(world, ui.menu_title, "Sky Trails");
                set_ui_text(
                    world,
                    ui.menu_body,
                    "Collect the key, open the gate, and bring the gems home.",
                );
                set_button(world, ui.primary_button, "start", "Start");
                set_node_visible(world, ui.secondary_button, false);
            }
            GameMode::Paused => {
                set_ui_text(world, ui.menu_title, "Paused");
                set_ui_text(world, ui.menu_body, "The trail is waiting.");
                set_button(world, ui.primary_button, "resume", "Resume");
                set_button(world, ui.secondary_button, "restart", "Restart");
                set_node_visible(world, ui.secondary_button, true);
            }
            GameMode::Victory => {
                set_ui_text(world, ui.menu_title, "Trail Cleared");
                set_ui_text(
                    world,
                    ui.menu_body,
                    format!(
                        "Score {} with {} coins and {} gems.",
                        self.run.score, self.run.coins, self.run.gems
                    ),
                );
                set_button(world, ui.primary_button, "restart", "Play Again");
                set_node_visible(world, ui.secondary_button, false);
            }
            GameMode::GameOver => {
                set_ui_text(world, ui.menu_title, "Try Again");
                set_ui_text(
                    world,
                    ui.menu_body,
                    format!("Score {} before the trail won.", self.run.score),
                );
                set_button(world, ui.primary_button, "restart", "Retry");
                set_node_visible(world, ui.secondary_button, false);
            }
            GameMode::Playing => {}
        }
    }

    fn update_window_title(&mut self, ctx: &FrameContext<'_>) {
        self.title_frame = self.title_frame.wrapping_add(1);
        if self.title_frame % 20 != 0 {
            return;
        }
        ctx.set_title(&format!(
            "SkyEngine - Sky Trails | score {} | lives {}",
            self.run.score,
            self.run.lives.max(0)
        ));
    }
}

impl AppState for KenneyPlatformerGame {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        self.assets.load(world);
        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.40, 0.72, 0.92),
            ..Default::default()
        });
        spawn_camera(world, &mut self.level);
        spawn_static_level(world, &self.assets.textures(), &mut self.level);
        spawn_run(
            world,
            &self.assets.textures(),
            &mut self.level,
            &mut self.run,
        );
        self.ui = Some(spawn_ui(world));
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
        self.update_window_title(ctx);
    }

    fn shutdown(&mut self, world: &mut World) {
        self.level.clear_dynamic(world);
        self.assets.unload(world);
    }
}

fn spawn_camera(world: &mut World, level: &mut LevelState) {
    let camera = world.spawn((
        Transform::from_xyz(520.0, LEVEL_H * 0.5, 0.0),
        CameraMarker::new(),
        Projection::orthographic(ORTHO_HEIGHT),
        MainCamera,
    ));
    level.camera = Some(camera);
}

fn spawn_static_level(world: &mut World, textures: &TextureSet, level: &mut LevelState) {
    spawn_backgrounds(world, textures, level);

    for (start, end) in [(0, 18), (23, 36), (40, 58), (62, LEVEL_COLS)] {
        spawn_solid_run(world, textures, level, start, end, 0, TileStyle::Grass);
    }
    spawn_solid_run(world, textures, level, 7, 13, 3, TileStyle::Bridge);
    spawn_solid_run(world, textures, level, 16, 23, 6, TileStyle::Grass);
    spawn_solid_run(world, textures, level, 27, 35, 4, TileStyle::Stone);
    spawn_solid_run(world, textures, level, 39, 47, 7, TileStyle::Grass);
    spawn_solid_run(world, textures, level, 50, 57, 3, TileStyle::Bridge);
    spawn_solid_run(world, textures, level, 62, 69, 5, TileStyle::Stone);
    spawn_solid_run(world, textures, level, 72, 80, 2, TileStyle::Grass);

    for col in 18..23 {
        spawn_hazard_tile(world, textures, level, col, 0, HazardKind::Lava);
    }
    for col in 36..40 {
        spawn_hazard_tile(world, textures, level, col, 0, HazardKind::Lava);
    }
    for col in 58..62 {
        spawn_hazard_tile(world, textures, level, col, 0, HazardKind::Lava);
    }
    for col in [13, 33, 52, 77] {
        spawn_hazard_tile(world, textures, level, col, 1, HazardKind::Spikes);
    }
    spawn_saw(world, textures, level, grid_pos(43, 9));
    spawn_saw(world, textures, level, grid_pos(66, 7));
    spawn_spring(world, textures, level, 29, 1);
    spawn_spring(world, textures, level, 55, 1);
    spawn_decor(world, textures);
}

fn spawn_run(world: &mut World, textures: &TextureSet, level: &mut LevelState, run: &mut RunState) {
    run.checkpoint = player_spawn();
    let player = world.spawn((
        Transform::from_xyz(run.checkpoint.x(), run.checkpoint.y(), 0.42),
        SpriteRenderer::new(PLAYER_DRAW_SIZE, PLAYER_DRAW_SIZE).texture(textures.player_idle),
        SortingLayer(90),
        Player {
            velocity: Vec2::ZERO,
            grounded: false,
            facing: 1.0,
            hurt_timer: 0.0,
            coyote_timer: 0.0,
            jump_buffer: 0.0,
            walk_cycle: 0.0,
        },
    ));
    level.player = Some(player);
    level.dynamic.push(player);

    spawn_collectibles(world, textures, level);
    spawn_enemy(world, textures, level, grid_pos(9, 1), 6, 16, 82.0);
    spawn_enemy(world, textures, level, grid_pos(31, 5), 27, 34, 70.0);
    spawn_enemy(world, textures, level, grid_pos(45, 8), 39, 47, 78.0);
    spawn_enemy(world, textures, level, grid_pos(74, 3), 72, 80, 84.0);

    let door_pos = Vec2::new((LEVEL_COLS as f32 - 2.1) * TILE, TILE + 34.0);
    let door = world.spawn((
        Transform::from_xyz(door_pos.x(), door_pos.y(), 0.36),
        SpriteRenderer::new(72.0, 84.0).texture(textures.door_closed),
        SortingLayer(60),
        Door,
    ));
    level.door = Some(door);
    level.dynamic.push(door);

    let flag = world.spawn((
        Transform::from_xyz(door_pos.x() - 62.0, door_pos.y() + 7.0, 0.38),
        SpriteRenderer::new(64.0, 64.0).texture(textures.flag_a),
        SortingLayer(62),
        Flag { phase: 0.0 },
    ));
    level.flag = Some(flag);
    level.dynamic.push(flag);
}

fn spawn_collectibles(world: &mut World, textures: &TextureSet, level: &mut LevelState) {
    let coins = [
        (5, 2),
        (8, 4),
        (11, 4),
        (17, 7),
        (21, 7),
        (27, 6),
        (30, 6),
        (34, 6),
        (41, 9),
        (44, 9),
        (51, 5),
        (54, 5),
        (64, 7),
        (68, 7),
        (73, 4),
        (76, 4),
    ];
    for (i, (col, row)) in coins.into_iter().enumerate() {
        spawn_collectible(
            world,
            textures,
            level,
            CollectibleKind::Coin,
            grid_pos(col, row),
            i as f32 * 0.37,
        );
    }
    for (i, (col, row, kind)) in [
        (22, 8, CollectibleKind::GemBlue),
        (46, 9, CollectibleKind::GemYellow),
        (67, 7, CollectibleKind::GemBlue),
    ]
    .into_iter()
    .enumerate()
    {
        spawn_collectible(world, textures, level, kind, grid_pos(col, row), i as f32);
    }
    spawn_collectible(
        world,
        textures,
        level,
        CollectibleKind::Key,
        grid_pos(65, 8),
        1.4,
    );
    spawn_collectible(
        world,
        textures,
        level,
        CollectibleKind::Heart,
        grid_pos(55, 5),
        2.7,
    );
}

fn spawn_backgrounds(world: &mut World, textures: &TextureSet, level: &mut LevelState) {
    spawn_parallax_layer(
        world,
        level,
        textures.bg_color_hills,
        1120.0,
        700.0,
        0.18,
        LEVEL_H * 0.5 - 16.0,
        -60,
        Color::WHITE,
    );
    spawn_parallax_layer(
        world,
        level,
        textures.bg_fade_hills,
        1080.0,
        540.0,
        0.34,
        280.0,
        -50,
        Color::new(1.0, 1.0, 1.0, 0.72),
    );
    spawn_parallax_layer(
        world,
        level,
        textures.bg_clouds,
        980.0,
        490.0,
        0.08,
        415.0,
        -70,
        Color::new(1.0, 1.0, 1.0, 0.58),
    );
}

fn spawn_parallax_layer(
    world: &mut World,
    level: &mut LevelState,
    texture: Handle<TextureAsset>,
    width: f32,
    height: f32,
    factor: f32,
    y: f32,
    layer: i32,
    color: Color,
) {
    for slot in -2..=3 {
        let entity = world.spawn((
            Transform::from_xyz(slot as f32 * width, y, -0.8),
            SpriteRenderer::new(width, height)
                .texture(texture)
                .color(color),
            SortingLayer(layer),
        ));
        level.backgrounds.push(ParallaxSprite {
            entity,
            slot,
            span: width,
            factor,
        });
    }
}

#[derive(Clone, Copy)]
enum TileStyle {
    Grass,
    Stone,
    Bridge,
}

fn spawn_solid_run(
    world: &mut World,
    textures: &TextureSet,
    level: &mut LevelState,
    start: usize,
    end: usize,
    row: usize,
    style: TileStyle,
) {
    for col in start..end {
        let texture = match style {
            TileStyle::Bridge => textures.bridge,
            TileStyle::Stone => textures.stone,
            TileStyle::Grass if col == start && row > 0 => textures.grass_left,
            TileStyle::Grass if col + 1 == end && row > 0 => textures.grass_right,
            TileStyle::Grass if row > 0 => textures.grass_top,
            TileStyle::Grass => textures.grass,
        };
        let center = grid_pos(col, row);
        world.spawn((
            Transform::from_xyz(center.x(), center.y(), 0.1),
            SpriteRenderer::new(TILE, TILE).texture(texture),
            SortingLayer(10 + row as i32),
            Tile,
        ));
        level.solids.push(Rect::new(center, TILE, TILE));

        if matches!(style, TileStyle::Grass) && row > 0 {
            let fill = grid_pos(col, row - 1);
            world.spawn((
                Transform::from_xyz(fill.x(), fill.y(), 0.09),
                SpriteRenderer::new(TILE, TILE).texture(textures.dirt),
                SortingLayer(8 + row as i32),
                Tile,
            ));
        }
    }
}

fn spawn_hazard_tile(
    world: &mut World,
    textures: &TextureSet,
    level: &mut LevelState,
    col: usize,
    row: usize,
    kind: HazardKind,
) {
    let center = grid_pos(col, row);
    let texture = match kind {
        HazardKind::Lava => {
            if row == 0 {
                textures.lava_top
            } else {
                textures.lava
            }
        }
        HazardKind::Spikes => textures.spikes,
        HazardKind::Saw => textures.saw_a,
    };
    let entity = world.spawn((
        Transform::from_xyz(center.x(), center.y(), 0.32),
        SpriteRenderer::new(TILE, TILE).texture(texture),
        SortingLayer(44),
        HazardMarker,
    ));
    let rect = match kind {
        HazardKind::Lava => Rect::new(center, TILE * 0.9, TILE * 0.62),
        HazardKind::Spikes => Rect::new(Vec2::new(center.x(), center.y() - 4.0), TILE * 0.76, 22.0),
        HazardKind::Saw => Rect::new(center, 42.0, 42.0),
    };
    level.hazards.push(HazardState {
        entity,
        rect,
        kind,
        phase: 0.0,
    });
}

fn spawn_saw(world: &mut World, textures: &TextureSet, level: &mut LevelState, center: Vec2) {
    let entity = world.spawn((
        Transform::from_xyz(center.x(), center.y(), 0.5),
        SpriteRenderer::new(56.0, 56.0).texture(textures.saw_a),
        SortingLayer(72),
        HazardMarker,
    ));
    level.hazards.push(HazardState {
        entity,
        rect: Rect::new(center, 46.0, 46.0),
        kind: HazardKind::Saw,
        phase: 0.0,
    });
}

fn spawn_spring(
    world: &mut World,
    textures: &TextureSet,
    level: &mut LevelState,
    col: usize,
    row: usize,
) {
    let center = grid_pos(col, row);
    let entity = world.spawn((
        Transform::from_xyz(center.x(), center.y() - 8.0, 0.34),
        SpriteRenderer::new(54.0, 54.0).texture(textures.spring),
        SortingLayer(52),
        SpringMarker,
    ));
    level.springs.push(SpringState {
        entity,
        rect: Rect::new(Vec2::new(center.x(), center.y() + 4.0), 40.0, 26.0),
        timer: 0.0,
    });
}

fn spawn_decor(world: &mut World, textures: &TextureSet) {
    for (texture, col, row, size) in [
        (textures.bush, 3, 1, 54.0),
        (textures.rock, 15, 1, 42.0),
        (textures.mushroom, 25, 1, 40.0),
        (textures.bush, 41, 1, 54.0),
        (textures.rock, 71, 1, 42.0),
    ] {
        let pos = grid_pos(col, row);
        world.spawn((
            Transform::from_xyz(pos.x(), pos.y() - 3.0, 0.26),
            SpriteRenderer::new(size, size).texture(texture),
            SortingLayer(28),
        ));
    }
    for (col, row) in [(17, 7), (45, 8), (64, 6), (79, 3)] {
        let pos = grid_pos(col, row);
        world.spawn((
            Transform::from_xyz(pos.x(), pos.y(), 0.31),
            SpriteRenderer::new(48.0, 48.0).texture(textures.torch_a),
            SortingLayer(40),
        ));
    }
}

fn spawn_collectible(
    world: &mut World,
    textures: &TextureSet,
    level: &mut LevelState,
    kind: CollectibleKind,
    position: Vec2,
    phase: f32,
) {
    let (texture, size, layer) = match kind {
        CollectibleKind::Coin => (textures.coin, 34.0, 68),
        CollectibleKind::GemBlue => (textures.gem_blue, 38.0, 69),
        CollectibleKind::GemYellow => (textures.gem_yellow, 38.0, 69),
        CollectibleKind::Key => (textures.key, 44.0, 70),
        CollectibleKind::Heart => (textures.heart, 42.0, 70),
    };
    let entity = world.spawn((
        Transform::from_xyz(position.x(), position.y(), 0.48),
        SpriteRenderer::new(size, size).texture(texture),
        SortingLayer(layer),
        CollectibleMarker,
    ));
    level.collectibles.push(CollectibleState {
        entity,
        kind,
        base: position,
        phase,
    });
    level.dynamic.push(entity);
}

fn spawn_enemy(
    world: &mut World,
    textures: &TextureSet,
    level: &mut LevelState,
    position: Vec2,
    left_col: usize,
    right_col: usize,
    speed: f32,
) {
    let entity = world.spawn((
        Transform::from_xyz(position.x(), position.y() + 4.0, 0.44),
        SpriteRenderer::new(58.0, 58.0).texture(textures.slime_a),
        SortingLayer(64),
        Enemy {
            left: left_col as f32 * TILE + TILE * 0.5,
            right: right_col as f32 * TILE + TILE * 0.5,
            speed,
            direction: 1.0,
            walk_cycle: 0.0,
        },
    ));
    level.enemies.push(entity);
    level.dynamic.push(entity);
}

fn update_player(
    ctx: &mut FrameContext<'_>,
    textures: &TextureSet,
    level: &mut LevelState,
    run: &mut RunState,
) {
    let Some(player_entity) = level.player else {
        return;
    };
    if !ctx.world.contains(player_entity) {
        return;
    }

    let dt = ctx.dt.min(1.0 / 30.0);
    let mut player = match ctx.world.get::<Player>(player_entity) {
        Some(player) => *player,
        None => return,
    };
    let mut position = match ctx.world.get::<Transform>(player_entity) {
        Some(transform) => Vec2::new(transform.x(), transform.y()),
        None => return,
    };

    let mut axis = 0.0;
    if ctx.input.key_held(KeyCode::KeyA) || ctx.input.key_held(KeyCode::ArrowLeft) {
        axis -= 1.0;
    }
    if ctx.input.key_held(KeyCode::KeyD) || ctx.input.key_held(KeyCode::ArrowRight) {
        axis += 1.0;
    }
    if axis != 0.0 {
        player.velocity[0] += axis * PLAYER_ACCEL * dt;
        player.velocity[0] = player.velocity[0].clamp(-PLAYER_MAX_SPEED, PLAYER_MAX_SPEED);
        player.facing = axis.signum();
    } else {
        player.velocity[0] = approach(player.velocity[0], 0.0, PLAYER_FRICTION * dt);
    }

    if ctx.input.key_pressed(KeyCode::Space)
        || ctx.input.key_pressed(KeyCode::KeyW)
        || ctx.input.key_pressed(KeyCode::ArrowUp)
    {
        player.jump_buffer = 0.12;
    } else {
        player.jump_buffer = (player.jump_buffer - dt).max(0.0);
    }

    if player.grounded {
        player.coyote_timer = 0.1;
    } else {
        player.coyote_timer = (player.coyote_timer - dt).max(0.0);
    }

    if player.jump_buffer > 0.0 && player.coyote_timer > 0.0 {
        player.velocity[1] = JUMP_SPEED;
        player.grounded = false;
        player.coyote_timer = 0.0;
        player.jump_buffer = 0.0;
    }

    player.velocity[1] = (player.velocity[1] + GRAVITY * dt).max(-980.0);
    position = move_with_collisions(position, &mut player, &level.solids, dt);
    player.hurt_timer = (player.hurt_timer - dt).max(0.0);
    player.walk_cycle += dt * (player.velocity[0].abs() * 0.025 + 3.0);

    for spring in &mut level.springs {
        let player_rect = Rect::from_entity(position, player_half());
        if player.velocity[1] <= 120.0 && player_rect.intersects(spring.rect) {
            player.velocity[1] = SPRING_SPEED;
            player.grounded = false;
            player.coyote_timer = 0.0;
            spring.timer = 0.22;
        }
    }

    if player.grounded && position.x() > run.checkpoint.x() + 420.0 {
        run.checkpoint = position;
    }

    if position.y() < -140.0 {
        hurt_player(ctx.world, level, run);
        player = ctx
            .world
            .get::<Player>(player_entity)
            .copied()
            .unwrap_or(player);
        position = run.checkpoint;
    }

    if let Some(transform) = ctx.world.get_mut::<Transform>(player_entity) {
        transform.position[0] = position.x();
        transform.position[1] = position.y();
        transform.scale[0] = player.facing;
    }
    if let Some(player_mut) = ctx.world.get_mut::<Player>(player_entity) {
        *player_mut = player;
    }
    update_player_sprite(
        ctx.world,
        textures,
        player_entity,
        &player,
        ctx.input.key_held(KeyCode::ArrowDown) || ctx.input.key_held(KeyCode::KeyS),
    );
}

fn move_with_collisions(mut position: Vec2, player: &mut Player, solids: &[Rect], dt: f32) -> Vec2 {
    position[0] += player.velocity[0] * dt;
    let mut rect = Rect::from_entity(position, player_half());
    for solid in solids {
        if !rect.intersects(*solid) {
            continue;
        }
        if player.velocity[0] > 0.0 {
            position[0] = solid.left() - PLAYER_HALF_X - 0.01;
        } else if player.velocity[0] < 0.0 {
            position[0] = solid.right() + PLAYER_HALF_X + 0.01;
        }
        player.velocity[0] = 0.0;
        rect = Rect::from_entity(position, player_half());
    }

    player.grounded = false;
    position[1] += player.velocity[1] * dt;
    rect = Rect::from_entity(position, player_half());
    for solid in solids {
        if !rect.intersects(*solid) {
            continue;
        }
        if player.velocity[1] <= 0.0 {
            position[1] = solid.top() + PLAYER_HALF_Y + 0.01;
            player.grounded = true;
        } else {
            position[1] = solid.bottom() - PLAYER_HALF_Y - 0.01;
        }
        player.velocity[1] = 0.0;
        rect = Rect::from_entity(position, player_half());
    }
    position
}

fn update_player_sprite(
    world: &mut World,
    textures: &TextureSet,
    entity: EntityId,
    player: &Player,
    ducking: bool,
) {
    let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) else {
        return;
    };
    let texture = if player.hurt_timer > 0.0 {
        textures.player_hit
    } else if !player.grounded {
        textures.player_jump
    } else if ducking {
        textures.player_duck
    } else if player.velocity[0].abs() > 18.0 {
        if (player.walk_cycle * 5.0) as i32 % 2 == 0 {
            textures.player_walk_a
        } else {
            textures.player_walk_b
        }
    } else {
        textures.player_idle
    };
    sprite.texture = Some(texture);
    sprite.color = if player.hurt_timer > 0.0 && (player.hurt_timer * 18.0) as i32 % 2 == 0 {
        Color::new(1.0, 0.82, 0.82, 0.55)
    } else {
        Color::WHITE
    };
}

fn update_enemies(world: &mut World, textures: &TextureSet, level: &mut LevelState, dt: f32) {
    let enemies = level.enemies.clone();
    for entity in enemies {
        if !world.contains(entity) {
            continue;
        }
        let mut enemy = match world.get::<Enemy>(entity) {
            Some(enemy) => *enemy,
            None => continue,
        };
        let Some(transform) = world.get_mut::<Transform>(entity) else {
            continue;
        };
        transform.position[0] += enemy.direction * enemy.speed * dt;
        if transform.position[0] < enemy.left {
            transform.position[0] = enemy.left;
            enemy.direction = 1.0;
        }
        if transform.position[0] > enemy.right {
            transform.position[0] = enemy.right;
            enemy.direction = -1.0;
        }
        transform.scale[0] = enemy.direction.signum();
        enemy.walk_cycle += dt * 9.0;
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
            sprite.texture = Some(if (enemy.walk_cycle as i32) % 2 == 0 {
                textures.slime_a
            } else {
                textures.slime_b
            });
        }
        if let Some(enemy_mut) = world.get_mut::<Enemy>(entity) {
            *enemy_mut = enemy;
        }
    }
}

fn update_collectibles(world: &mut World, level: &mut LevelState, run: &mut RunState, dt: f32) {
    let Some(player_entity) = level.player else {
        return;
    };
    let Some(player_position) = entity_position(world, player_entity) else {
        return;
    };
    let player_rect = Rect::from_entity(player_position, player_half());
    let mut collected = Vec::new();

    for item in &mut level.collectibles {
        item.phase += dt * 3.2;
        let y = item.base.y() + item.phase.sin() * 7.0;
        if let Some(transform) = world.get_mut::<Transform>(item.entity) {
            transform.position[1] = y;
            transform.rotate_z(dt * 1.5);
        }
        let rect = Rect::new(Vec2::new(item.base.x(), y), 34.0, 34.0);
        if player_rect.intersects(rect) {
            collected.push(item.entity);
            match item.kind {
                CollectibleKind::Coin => {
                    run.coins += 1;
                    run.score += 25;
                }
                CollectibleKind::GemBlue => {
                    run.gems += 1;
                    run.score += 125;
                }
                CollectibleKind::GemYellow => {
                    run.gems += 1;
                    run.score += 175;
                }
                CollectibleKind::Key => {
                    run.has_key = true;
                    run.score += 250;
                }
                CollectibleKind::Heart => {
                    run.lives = (run.lives + 1).min(5);
                    run.score += 75;
                }
            }
        }
    }

    for entity in collected {
        if world.contains(entity) {
            let _ = world.despawn(entity);
        }
    }
    level.retain_live(world);
}

fn resolve_player_contacts(
    world: &mut World,
    textures: &TextureSet,
    level: &mut LevelState,
    run: &mut RunState,
    mode: &mut GameMode,
) {
    let Some(player_entity) = level.player else {
        return;
    };
    if !world.contains(player_entity) {
        return;
    }
    let Some(player_position) = entity_position(world, player_entity) else {
        return;
    };
    let player_rect = Rect::from_entity(player_position, player_half());

    let hazard_hit = level
        .hazards
        .iter()
        .any(|hazard| player_rect.intersects(hazard.rect));
    if hazard_hit {
        hurt_player(world, level, run);
        if run.lives <= 0 {
            *mode = GameMode::GameOver;
            set_clear_color(world, Color::rgb(0.40, 0.16, 0.20));
        }
        return;
    }

    let mut stomped = None;
    let mut hurt = false;
    let player_velocity_y = world
        .get::<Player>(player_entity)
        .map(|player| player.velocity.y())
        .unwrap_or(0.0);
    for enemy in level.enemies.clone() {
        if !world.contains(enemy) {
            continue;
        }
        let Some(enemy_position) = entity_position(world, enemy) else {
            continue;
        };
        let enemy_rect = Rect::new(enemy_position, 44.0, 38.0);
        if !player_rect.intersects(enemy_rect) {
            continue;
        }
        if player_velocity_y < -70.0 && player_rect.bottom() > enemy_position.y() {
            stomped = Some(enemy);
            break;
        }
        hurt = true;
        break;
    }

    if let Some(enemy) = stomped {
        if world.contains(enemy) {
            let _ = world.despawn(enemy);
            run.score += 150;
        }
        if let Some(player) = world.get_mut::<Player>(player_entity) {
            player.velocity[1] = 520.0;
            player.grounded = false;
        }
        level.retain_live(world);
        return;
    }

    if hurt {
        hurt_player(world, level, run);
        if run.lives <= 0 {
            *mode = GameMode::GameOver;
            set_clear_color(world, Color::rgb(0.40, 0.16, 0.20));
        }
    }

    let Some(door) = level.door else {
        return;
    };
    let Some(door_position) = entity_position(world, door) else {
        return;
    };
    let door_rect = Rect::new(door_position, 48.0, 72.0);
    if run.has_key && player_rect.intersects(door_rect) {
        *mode = GameMode::Victory;
        run.score += (run.lives.max(0) as u32) * 250;
        set_clear_color(world, Color::rgb(0.28, 0.66, 0.76));
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(door) {
            sprite.texture = Some(textures.door_open);
        }
    }
}

fn update_door(world: &mut World, textures: &TextureSet, level: &mut LevelState, run: &RunState) {
    if let Some(door) = level.door {
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(door) {
            sprite.texture = Some(if run.has_key {
                textures.door_open
            } else {
                textures.door_closed
            });
        }
    }
    if let Some(flag) = level.flag {
        let mut phase = world
            .get::<Flag>(flag)
            .map(|flag| flag.phase)
            .unwrap_or(0.0);
        phase += 0.08;
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(flag) {
            sprite.texture = Some(if (phase * 8.0) as i32 % 2 == 0 {
                textures.flag_a
            } else {
                textures.flag_b
            });
        }
        if let Some(flag_data) = world.get_mut::<Flag>(flag) {
            flag_data.phase = phase;
        }
    }
}

fn animate_static_interactives(
    world: &mut World,
    textures: &TextureSet,
    level: &mut LevelState,
    dt: f32,
) {
    for spring in &mut level.springs {
        spring.timer = (spring.timer - dt).max(0.0);
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(spring.entity) {
            sprite.texture = Some(if spring.timer > 0.0 {
                textures.spring_out
            } else {
                textures.spring
            });
        }
    }
    for hazard in &mut level.hazards {
        hazard.phase += dt;
        if !matches!(hazard.kind, HazardKind::Saw) {
            continue;
        }
        if let Some(transform) = world.get_mut::<Transform>(hazard.entity) {
            transform.rotate_z(dt * 8.0);
        }
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(hazard.entity) {
            sprite.texture = Some(if (hazard.phase * 12.0) as i32 % 2 == 0 {
                textures.saw_a
            } else {
                textures.saw_b
            });
        }
    }
}

fn animate_idle_world(world: &mut World, textures: &TextureSet, level: &mut LevelState, dt: f32) {
    animate_static_interactives(world, textures, level, dt);
    for item in &mut level.collectibles {
        item.phase += dt * 2.0;
        if let Some(transform) = world.get_mut::<Transform>(item.entity) {
            transform.position[1] = item.base.y() + item.phase.sin() * 5.0;
        }
    }
}

fn hurt_player(world: &mut World, level: &mut LevelState, run: &mut RunState) {
    let Some(player_entity) = level.player else {
        return;
    };
    let can_hurt = world
        .get::<Player>(player_entity)
        .map(|player| player.hurt_timer <= 0.0)
        .unwrap_or(false);
    if !can_hurt {
        return;
    }

    run.lives -= 1;
    if let Some(player) = world.get_mut::<Player>(player_entity) {
        player.velocity = Vec2::ZERO;
        player.hurt_timer = 1.4;
        player.grounded = false;
    }
    if let Some(transform) = world.get_mut::<Transform>(player_entity) {
        transform.position[0] = run.checkpoint.x();
        transform.position[1] = run.checkpoint.y();
    }
}

fn update_parallax(world: &mut World, level: &LevelState) {
    let Some(camera) = level.camera else {
        return;
    };
    let Some(camera_x) = world.get::<Transform>(camera).map(|t| t.x()) else {
        return;
    };
    for sprite in &level.backgrounds {
        if let Some(transform) = world.get_mut::<Transform>(sprite.entity) {
            let base = camera_x * sprite.factor;
            let origin = ((camera_x - base) / sprite.span).floor() * sprite.span;
            transform.position[0] = base + origin + sprite.slot as f32 * sprite.span;
        }
    }
}

fn player_spawn() -> Vec2 {
    Vec2::new(TILE * 2.5, TILE + PLAYER_HALF_Y + 0.01)
}

fn player_half() -> Vec2 {
    Vec2::new(PLAYER_HALF_X, PLAYER_HALF_Y)
}

fn grid_pos(col: usize, row: usize) -> Vec2 {
    Vec2::new((col as f32 + 0.5) * TILE, (row as f32 + 0.5) * TILE)
}

fn entity_position(world: &World, entity: EntityId) -> Option<Vec2> {
    world
        .get::<Transform>(entity)
        .map(|transform| Vec2::new(transform.x(), transform.y()))
}

fn approach(value: f32, target: f32, amount: f32) -> f32 {
    if value < target {
        (value + amount).min(target)
    } else {
        (value - amount).max(target)
    }
}

fn spawn_ui(world: &mut World) -> UiRefs {
    let hud_panel = world.spawn((
        UiNode::panel(610.0, 42.0)
            .anchor(UiAnchor::TopLeft)
            .at(18.0, 16.0)
            .z(20),
        UiPanel::new(Color::rgba8(18, 30, 38, 210)),
    ));
    let hud_text = world.spawn((
        UiNode::new()
            .child_of(hud_panel)
            .anchor(UiAnchor::Stretch)
            .at(16.0, 10.0)
            .z(21),
        UiText::new("")
            .size(18.0)
            .color(Color::rgba8(242, 248, 252, 255)),
    ));
    let key_text = world.spawn((
        UiNode::panel(132.0, 32.0)
            .anchor(UiAnchor::TopRight)
            .at(156.0, 20.0)
            .z(20),
        UiText::new("key missing")
            .size(17.0)
            .align(UiAlign::Center)
            .color(Color::rgba8(250, 225, 110, 255)),
    ));
    let pause_button = world.spawn((
        UiNode::panel(116.0, 36.0)
            .id("pause")
            .anchor(UiAnchor::TopRight)
            .at(24.0, 18.0)
            .z(20),
        UiButton::new("Pause"),
    ));

    let menu_panel = world.spawn((
        UiNode::panel(420.0, 258.0)
            .anchor(UiAnchor::Center)
            .z(50)
            .layout(UiLayout::column(
                UiRect::new(28.0, 26.0, 28.0, 26.0),
                14.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(15, 25, 34, 238)),
    ));
    let menu_title = world.spawn((
        UiNode::panel(1.0, 44.0)
            .child_of(menu_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("Sky Trails")
            .size(30.0)
            .align(UiAlign::Center)
            .color(Color::rgba8(248, 252, 255, 255)),
    ));
    let menu_body = world.spawn((
        UiNode::panel(1.0, 58.0)
            .child_of(menu_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("")
            .size(17.0)
            .align(UiAlign::Center)
            .color(Color::rgba8(190, 212, 220, 255)),
    ));
    let primary_button = spawn_menu_button(world, menu_panel, "start", "Start");
    let secondary_button = spawn_menu_button(world, menu_panel, "restart", "Restart");

    UiRefs {
        hud_panel,
        hud_text,
        key_text,
        menu_panel,
        menu_title,
        menu_body,
        primary_button,
        secondary_button,
        pause_button,
    }
}

fn spawn_menu_button(
    world: &mut World,
    parent: EntityId,
    id: &'static str,
    label: &'static str,
) -> EntityId {
    world.spawn((
        UiNode::panel(1.0, 42.0)
            .id(UiId::new(id))
            .child_of(parent)
            .width(UiLength::Percent(1.0)),
        UiButton::new(label),
    ))
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

fn set_clear_color(world: &mut World, color: Color) {
    if let Some(settings) = world.get_resource_mut::<RenderSettings>() {
        settings.clear_color = color;
    }
}

fn load_png_texture(server: &AssetServer, path: impl AsRef<Path>) -> Handle<TextureAsset> {
    let path = path.as_ref();
    let image = ImageReader::open(path)
        .unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()))
        .decode()
        .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()))
        .to_rgba8();
    let (width, height) = image.dimensions();
    server.insert_runtime(TextureAsset::new(
        width,
        height,
        TextureColorSpace::Srgb,
        image.into_raw(),
    ))
}

fn kenney_asset_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("kenney_new_platformer_pack")
}

fn main() {
    App::new(
        AppConfig::new("SkyEngine - Sky Trails", WINDOW_W, WINDOW_H)
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
    .run(KenneyPlatformerGame::new());
}
