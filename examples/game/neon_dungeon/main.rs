//! A small but complete top-down action game vertical slice.
//!
//! ```bash
//! cargo run --example neon_dungeon_game --features "app physics" --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, RunnerPlugin,
    SetupContext, WindowPlugin,
};
use sky_engine::asset::{Assets, Handle, TextureAsset};
use sky_engine::ecs::{EntityId, World};
use sky_engine::input::KeyCode;
use sky_engine::math::Vec2;
use sky_engine::physics::{
    Collider2D, PhysicsConfig2D, PhysicsEvent2D, PhysicsEvents, PhysicsPlugin, RigidBody2D,
    Velocity2D,
};
use sky_engine::plugin::Plugin;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SortingLayer,
    SpriteFeature, SpriteRenderer, Transform, TransparentPhase,
};

const ARENA_W: f32 = 980.0;
const ARENA_H: f32 = 610.0;
const PLAYER_SPEED: f32 = 285.0;
const BULLET_SPEED: f32 = 640.0;
const BULLET_TTL: f32 = 1.15;
const FIRE_INTERVAL: f32 = 0.18;
const TARGET_CORES: u32 = 7;
const MAX_ENEMIES: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameMode {
    Title,
    Playing,
    Paused,
    Victory,
    GameOver,
}

#[derive(Clone, Copy)]
struct Player;

#[derive(Clone, Copy)]
struct Enemy {
    hp: i32,
    speed: f32,
    damage_cooldown: f32,
    hue: f32,
}

#[derive(Clone, Copy)]
struct Bullet {
    damage: i32,
    ttl: f32,
}

#[derive(Clone, Copy)]
struct Wall;

#[derive(Clone, Copy)]
struct ExitPortal;

#[derive(Clone, Copy)]
struct Pickup {
    kind: PickupKind,
}

#[derive(Clone, Copy)]
enum PickupKind {
    Core,
    Heart,
}

#[derive(Clone, Copy)]
struct Pulse {
    base_width: f32,
    base_height: f32,
    amplitude: f32,
    speed: f32,
    phase: f32,
}

#[derive(Clone, Copy)]
struct Spark {
    velocity: Vec2,
    ttl: f32,
    spin: f32,
}

struct NeonDungeonGame {
    mode: GameMode,
    assets: GameAssets,
    rng: SimpleRng,
    level: LevelEntities,
    stats: RunStats,
    fire_timer: f32,
    spawn_timer: f32,
    title_timer: f32,
    hud_frame: u32,
}

impl NeonDungeonGame {
    fn new() -> Self {
        Self {
            mode: GameMode::Title,
            assets: GameAssets::default(),
            rng: SimpleRng::new(0xD00D_2026),
            level: LevelEntities::default(),
            stats: RunStats::default(),
            fire_timer: 0.0,
            spawn_timer: 2.2,
            title_timer: 0.0,
            hud_frame: 0,
        }
    }

    fn start_run(&mut self, world: &mut World) {
        self.level.clear(world);
        self.stats = RunStats::new();
        self.fire_timer = 0.0;
        self.spawn_timer = 1.4;
        self.mode = GameMode::Playing;

        set_clear_color(world, Color::rgb(0.025, 0.022, 0.04));
        spawn_level(world, &self.assets, &mut self.level, &mut self.rng);
    }

    fn update_title(&mut self, ctx: &mut FrameContext<'_>) {
        self.title_timer += ctx.dt;
        animate_background(ctx.world, self.title_timer);
        if ctx.input.key_pressed(KeyCode::Space) || ctx.input.key_pressed(KeyCode::Enter) {
            self.start_run(ctx.world);
        }
        ctx.render();
        self.set_title(ctx, "SPACE/ENTER start | WASD move | auto-fire | P pause");
    }

    fn update_playing(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::KeyP) {
            self.mode = GameMode::Paused;
            stop_player(ctx.world, self.level.player);
            ctx.render();
            self.set_title(ctx, "Paused | P resume | R restart | Esc exit");
            return;
        }
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }

        self.fire_timer = (self.fire_timer - ctx.dt).max(0.0);
        self.spawn_timer -= ctx.dt;
        self.stats.invuln = (self.stats.invuln - ctx.dt).max(0.0);
        if self.stats.invuln <= 0.0 {
            restore_player_color(ctx.world, self.level.player);
        }

        control_player(ctx.world, ctx.input, self.level.player);
        update_enemy_ai(ctx.world, self.level.player, ctx.dt);
        self.auto_fire(ctx.world);
        self.spawn_reinforcements(ctx.world);
        update_bullets(ctx.world, &mut self.level, ctx.dt);
        update_sparks(ctx.world, &mut self.level, ctx.dt);
        animate_background(ctx.world, self.title_timer + self.stats.elapsed);
        animate_pulses(ctx.world, ctx.dt);
        update_portal_visual(ctx.world, self.level.portal, self.stats.portal_open());

        self.stats.elapsed += ctx.dt;
        ctx.world.tick_with_delta(ctx.dt).unwrap();

        self.handle_physics_events(ctx.world);
        self.apply_player_touch_damage(ctx.world);
        self.level.retain_live(ctx.world);

        if self.stats.hp <= 0 {
            self.mode = GameMode::GameOver;
            stop_player(ctx.world, self.level.player);
            burst_at_player(ctx.world, &self.assets, &mut self.level, &mut self.rng);
        } else if self.stats.escaped {
            self.mode = GameMode::Victory;
            stop_player(ctx.world, self.level.player);
            celebrate(ctx.world, &self.assets, &mut self.level, &mut self.rng);
        }

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
        animate_pulses(ctx.world, ctx.dt);
        update_sparks(ctx.world, &mut self.level, ctx.dt);
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
                    "Victory in {:.1}s | score {} | R/Space restart | Esc exit",
                    self.stats.elapsed, self.stats.score
                ),
            ),
            GameMode::GameOver => self.set_title(
                ctx,
                &format!(
                    "Game Over | score {} | cores {}/{} | R/Space restart | Esc exit",
                    self.stats.score, self.stats.cores, TARGET_CORES
                ),
            ),
            _ => {}
        }
    }

    fn auto_fire(&mut self, world: &mut World) {
        if self.fire_timer > 0.0 {
            return;
        }
        let Some(player) = self.level.player else {
            return;
        };
        let Some(player_pos) = entity_position(world, player) else {
            return;
        };
        let Some(target) = nearest_enemy(world, player_pos) else {
            return;
        };
        let direction = (target - player_pos).normalized();
        if direction.length_squared() <= f32::EPSILON {
            return;
        }
        let bullet = spawn_bullet(
            world,
            &self.assets,
            player_pos + direction * 28.0,
            direction,
        );
        self.level.bullets.push(bullet);
        self.fire_timer = FIRE_INTERVAL;
    }

    fn spawn_reinforcements(&mut self, world: &mut World) {
        self.level.enemies.retain(|entity| world.contains(*entity));
        if self.spawn_timer > 0.0 || self.level.enemies.len() >= MAX_ENEMIES {
            return;
        }
        self.spawn_timer = self.rng.range(1.1, 2.0);
        self.stats.wave = 1 + self.stats.kills / 8;
        let position = random_spawn_edge(&mut self.rng);
        let enemy = spawn_enemy(
            world,
            &self.assets,
            position,
            &mut self.rng,
            self.stats.wave,
        );
        self.level.enemies.push(enemy);
        self.level.entities.push(enemy);
    }

    fn handle_physics_events(&mut self, world: &mut World) {
        let events = if let Some(events) = world.get_resource_mut::<PhysicsEvents>() {
            events.drain().collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let mut despawn = Vec::new();
        for event in events {
            match event {
                PhysicsEvent2D::TriggerEntered { trigger, other } => {
                    self.handle_trigger(world, trigger, other, &mut despawn);
                }
                PhysicsEvent2D::ContactStarted { a, b } => {
                    self.handle_contact(world, a, b);
                }
                PhysicsEvent2D::ContactStopped { .. } | PhysicsEvent2D::TriggerExited { .. } => {}
            }
        }

        for entity in despawn {
            if world.contains(entity) {
                let _ = world.despawn(entity);
            }
        }
        self.level.retain_live(world);
    }

    fn handle_trigger(
        &mut self,
        world: &mut World,
        trigger: EntityId,
        other: EntityId,
        despawn: &mut Vec<EntityId>,
    ) {
        if let Some((bullet, enemy)) = pair::<Bullet, Enemy>(world, trigger, other) {
            self.damage_enemy(world, bullet, enemy, despawn);
            return;
        }
        if let Some((bullet, wall)) = pair::<Bullet, Wall>(world, trigger, other) {
            let _ = wall;
            push_unique(despawn, bullet);
            return;
        }
        if let Some((player, pickup)) = pair::<Player, Pickup>(world, trigger, other) {
            let _ = player;
            self.collect_pickup(world, pickup, despawn);
            return;
        }
        if let Some((player, portal)) = pair::<Player, ExitPortal>(world, trigger, other) {
            let _ = (player, portal);
            if self.stats.portal_open() {
                self.stats.escaped = true;
            } else {
                flash_locked_portal(world, self.level.portal);
            }
        }
    }

    fn handle_contact(&mut self, world: &mut World, a: EntityId, b: EntityId) {
        if pair::<Player, Enemy>(world, a, b).is_some() && self.stats.invuln <= 0.0 {
            self.stats.hp -= 1;
            self.stats.invuln = 0.9;
            flash_player(world, self.level.player, self.stats.hp);
        }
    }

    fn damage_enemy(
        &mut self,
        world: &mut World,
        bullet: EntityId,
        enemy: EntityId,
        despawn: &mut Vec<EntityId>,
    ) {
        let damage = world
            .get::<Bullet>(bullet)
            .map(|bullet| bullet.damage)
            .unwrap_or(1);
        push_unique(despawn, bullet);

        let Some(enemy_pos) = entity_position(world, enemy) else {
            return;
        };
        let mut killed = false;
        let mut hue = 330.0;
        if let Some(enemy_data) = world.get_mut::<Enemy>(enemy) {
            enemy_data.hp -= damage;
            enemy_data.speed += 8.0;
            hue = enemy_data.hue;
            killed = enemy_data.hp <= 0;
        }

        if killed {
            self.stats.kills = self.stats.kills.saturating_add(1);
            self.stats.score = self.stats.score.saturating_add(100);
            push_unique(despawn, enemy);
            self.drop_reward(world, enemy_pos);
            spawn_hit_sparks(
                world,
                &self.assets,
                &mut self.level,
                &mut self.rng,
                enemy_pos,
                hue,
            );
        } else {
            self.stats.score = self.stats.score.saturating_add(12);
            if let Some(sprite) = world.get_mut::<SpriteRenderer>(enemy) {
                sprite.color = Color::rgb(1.0, 0.95, 0.82);
            }
            spawn_hit_sparks(
                world,
                &self.assets,
                &mut self.level,
                &mut self.rng,
                enemy_pos,
                hue,
            );
        }
    }

    fn drop_reward(&mut self, world: &mut World, position: Vec2) {
        let kind = if self.stats.kills % 6 == 0 {
            PickupKind::Heart
        } else {
            PickupKind::Core
        };
        let pickup = spawn_pickup(world, &self.assets, position, kind);
        self.level.pickups.push(pickup);
        self.level.entities.push(pickup);
    }

    fn collect_pickup(&mut self, world: &mut World, pickup: EntityId, despawn: &mut Vec<EntityId>) {
        let Some(kind) = world.get::<Pickup>(pickup).map(|pickup| pickup.kind) else {
            return;
        };
        match kind {
            PickupKind::Core => {
                self.stats.cores = self.stats.cores.saturating_add(1);
                self.stats.score = self.stats.score.saturating_add(250);
            }
            PickupKind::Heart => {
                self.stats.hp = (self.stats.hp + 1).min(5);
                self.stats.score = self.stats.score.saturating_add(75);
                restore_player_color(world, self.level.player);
            }
        }
        push_unique(despawn, pickup);
    }

    fn apply_player_touch_damage(&mut self, world: &mut World) {
        if self.stats.invuln > 0.0 {
            return;
        }
        let Some(player) = self.level.player else {
            return;
        };
        let Some(player_pos) = entity_position(world, player) else {
            return;
        };

        let mut touched = None;
        {
            let enemies = world.query::<(&Transform, &Enemy)>();
            enemies.for_each_with_entity(|entity, (transform, enemy)| {
                if touched.is_none()
                    && enemy.damage_cooldown <= 0.0
                    && (position_of(transform) - player_pos).length() < 34.0
                {
                    touched = Some(entity);
                }
            });
        }

        if let Some(enemy) = touched {
            self.stats.hp -= 1;
            self.stats.invuln = 0.9;
            flash_player(world, self.level.player, self.stats.hp);
            if let Some(enemy) = world.get_mut::<Enemy>(enemy) {
                enemy.damage_cooldown = 0.8;
            }
        }
    }

    fn update_hud(&mut self, ctx: &FrameContext<'_>) {
        self.hud_frame = self.hud_frame.wrapping_add(1);
        if self.hud_frame % 10 != 0 {
            return;
        }
        let status = if self.stats.portal_open() {
            "portal open"
        } else {
            "collect cores"
        };
        ctx.set_title(&format!(
            "Neon Dungeon | HP {} | cores {}/{} | kills {} | score {} | wave {} | {} | P pause",
            self.stats.hp,
            self.stats.cores,
            TARGET_CORES,
            self.stats.kills,
            self.stats.score,
            self.stats.wave,
            status,
        ));
    }

    fn set_title(&mut self, ctx: &FrameContext<'_>, title: &str) {
        self.hud_frame = self.hud_frame.wrapping_add(1);
        if self.hud_frame % 12 == 0 {
            ctx.set_title(&format!("Neon Dungeon | {title}"));
        }
    }
}

impl AppState for NeonDungeonGame {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        self.assets.load(world);
        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.018, 0.016, 0.03),
            ..Default::default()
        });
        PhysicsPlugin::new(PhysicsConfig2D {
            gravity: Vec2::ZERO,
            fixed_dt: 1.0 / 90.0,
            pixels_per_meter: 64.0,
        })
        .install(world)
        .unwrap();
        spawn_camera(world);
        spawn_background(world);
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        match self.mode {
            GameMode::Title => self.update_title(ctx),
            GameMode::Playing => self.update_playing(ctx),
            GameMode::Paused => self.update_paused(ctx),
            GameMode::Victory | GameMode::GameOver => self.update_finished(ctx),
        }
    }

    fn shutdown(&mut self, world: &mut World) {
        self.level.clear(world);
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
            .get_resource::<Assets>()
            .map(|server| server.insert_runtime(TextureAsset::circle(96)));
    }

    fn unload(&mut self, _world: &World) {
        self.circle.take();
    }

    fn sprite_circle(&self, size: f32, color: Color) -> SpriteRenderer {
        let mut sprite = SpriteRenderer::new(size, size).color(color);
        if let Some(circle) = &self.circle {
            sprite = sprite.texture(circle.clone());
        }
        sprite
    }
}

#[derive(Default)]
struct LevelEntities {
    entities: Vec<EntityId>,
    enemies: Vec<EntityId>,
    bullets: Vec<EntityId>,
    pickups: Vec<EntityId>,
    sparks: Vec<EntityId>,
    player: Option<EntityId>,
    portal: Option<EntityId>,
}

impl LevelEntities {
    fn clear(&mut self, world: &mut World) {
        for entity in self.entities.drain(..) {
            if world.contains(entity) {
                let _ = world.despawn(entity);
            }
        }
        self.enemies.clear();
        self.bullets.clear();
        self.pickups.clear();
        self.sparks.clear();
        self.player = None;
        self.portal = None;
    }

    fn retain_live(&mut self, world: &World) {
        self.entities.retain(|entity| world.contains(*entity));
        self.enemies.retain(|entity| world.contains(*entity));
        self.bullets.retain(|entity| world.contains(*entity));
        self.pickups.retain(|entity| world.contains(*entity));
        self.sparks.retain(|entity| world.contains(*entity));
        if self.player.is_some_and(|entity| !world.contains(entity)) {
            self.player = None;
        }
        if self.portal.is_some_and(|entity| !world.contains(entity)) {
            self.portal = None;
        }
    }

    fn track(&mut self, entity: EntityId) -> EntityId {
        self.entities.push(entity);
        entity
    }
}

#[derive(Clone, Copy)]
struct RunStats {
    hp: i32,
    cores: u32,
    kills: u32,
    score: u32,
    wave: u32,
    elapsed: f32,
    invuln: f32,
    escaped: bool,
}

impl RunStats {
    fn new() -> Self {
        Self {
            hp: 4,
            cores: 0,
            kills: 0,
            score: 0,
            wave: 1,
            elapsed: 0.0,
            invuln: 0.0,
            escaped: false,
        }
    }

    fn portal_open(self) -> bool {
        self.cores >= TARGET_CORES
    }
}

impl Default for RunStats {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("Neon Dungeon", 1280, 760).with_vsync(false))
        .unwrap();
    world
        .install(RunnerPlugin::game().with_auto_tick(false))
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

    App::new(world).run(NeonDungeonGame::new());
}

fn spawn_camera(world: &mut World) {
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        CameraMarker::new(),
        Projection::orthographic(720.0),
        MainCamera,
    ));
}

fn spawn_background(world: &mut World) {
    for i in -7..=7 {
        let x = i as f32 * 78.0;
        world.spawn((
            Transform::from_xyz(x, 0.0, -0.4),
            SpriteRenderer::new(1.5, 720.0).color(Color::new(0.1, 0.48, 0.72, 0.14)),
            SortingLayer(-80),
            Pulse {
                base_width: 1.5,
                base_height: 720.0,
                amplitude: 0.25,
                speed: 1.2,
                phase: i as f32 * 0.37,
            },
        ));
    }
    for i in -4..=4 {
        let y = i as f32 * 78.0;
        world.spawn((
            Transform::from_xyz(0.0, y, -0.4),
            SpriteRenderer::new(1160.0, 1.5).color(Color::new(0.12, 0.42, 0.65, 0.13)),
            SortingLayer(-80),
            Pulse {
                base_width: 1160.0,
                base_height: 1.5,
                amplitude: 0.25,
                speed: 1.4,
                phase: i as f32 * 0.41,
            },
        ));
    }
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -0.5),
        SpriteRenderer::new(1080.0, 670.0).color(Color::new(0.018, 0.015, 0.035, 0.88)),
        SortingLayer(-90),
    ));
}

fn spawn_level(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelEntities,
    rng: &mut SimpleRng,
) {
    spawn_arena(world, level);
    level.player = Some(level.track(spawn_player(world, assets)));
    level.portal = Some(level.track(spawn_portal(world, assets)));

    for position in [
        Vec2::new(-320.0, 210.0),
        Vec2::new(-80.0, 235.0),
        Vec2::new(260.0, 190.0),
        Vec2::new(330.0, -130.0),
        Vec2::new(-260.0, -190.0),
        Vec2::new(80.0, -250.0),
    ] {
        let enemy = spawn_enemy(world, assets, position, rng, 1);
        level.enemies.push(enemy);
        level.track(enemy);
    }

    for position in [
        Vec2::new(-390.0, 0.0),
        Vec2::new(410.0, 75.0),
        Vec2::new(0.0, 270.0),
    ] {
        let pickup = spawn_pickup(world, assets, position, PickupKind::Core);
        level.pickups.push(pickup);
        level.track(pickup);
    }
}

fn spawn_arena(world: &mut World, level: &mut LevelEntities) {
    let wall_color = Color::rgb(0.18, 0.86, 0.98);
    for entity in [
        spawn_wall(world, 0.0, ARENA_H * 0.5, ARENA_W, 26.0, wall_color),
        spawn_wall(world, 0.0, -ARENA_H * 0.5, ARENA_W, 26.0, wall_color),
        spawn_wall(world, -ARENA_W * 0.5, 0.0, 26.0, ARENA_H, wall_color),
        spawn_wall(world, ARENA_W * 0.5, 0.0, 26.0, ARENA_H, wall_color),
        spawn_wall(
            world,
            -210.0,
            78.0,
            210.0,
            24.0,
            Color::rgb(0.76, 0.25, 1.0),
        ),
        spawn_wall(
            world,
            205.0,
            -52.0,
            240.0,
            24.0,
            Color::rgb(1.0, 0.48, 0.18),
        ),
        spawn_wall(
            world,
            -18.0,
            -205.0,
            26.0,
            165.0,
            Color::rgb(0.9, 0.9, 0.24),
        ),
        spawn_wall(world, 24.0, 185.0, 26.0, 130.0, Color::rgb(0.42, 1.0, 0.68)),
    ] {
        level.track(entity);
    }
}

fn spawn_wall(
    world: &mut World,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Color,
) -> EntityId {
    world.spawn((
        Transform::from_xyz(x, y, 0.0),
        SpriteRenderer::new(width, height).color(color),
        SortingLayer(0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(width, height).friction(0.9),
        Wall,
        Pulse {
            base_width: width,
            base_height: height,
            amplitude: 0.04,
            speed: 2.0,
            phase: x * 0.01 + y * 0.02,
        },
    ))
}

fn spawn_player(world: &mut World, assets: &GameAssets) -> EntityId {
    world.spawn((
        Transform::from_xyz(0.0, -18.0, 0.2),
        assets.sprite_circle(40.0, Color::rgb(0.22, 0.95, 1.0)),
        SortingLayer(20),
        RigidBody2D::kinematic().lock_rotation(),
        Collider2D::circle(18.0).friction(0.0),
        Velocity2D::default(),
        Player,
    ))
}

fn spawn_enemy(
    world: &mut World,
    assets: &GameAssets,
    position: Vec2,
    rng: &mut SimpleRng,
    wave: u32,
) -> EntityId {
    let hue = rng.range(306.0, 356.0);
    let size = rng.range(30.0, 42.0);
    world.spawn((
        Transform::from_xyz(position.x(), position.y(), 0.15),
        assets.sprite_circle(size, Color::hsl(hue, 0.82, 0.58)),
        SortingLayer(12),
        RigidBody2D::kinematic().lock_rotation(),
        Collider2D::circle(size * 0.44).friction(0.0),
        Velocity2D::default(),
        Enemy {
            hp: 2 + (wave / 3) as i32,
            speed: rng.range(92.0, 136.0) + wave as f32 * 4.0,
            damage_cooldown: 0.0,
            hue,
        },
    ))
}

fn spawn_portal(world: &mut World, assets: &GameAssets) -> EntityId {
    world.spawn((
        Transform::from_xyz(415.0, -238.0, 0.05),
        assets.sprite_circle(86.0, Color::new(0.35, 0.2, 1.0, 0.28)),
        SortingLayer(4),
        RigidBody2D::static_body(),
        Collider2D::circle(42.0).sensor(true),
        ExitPortal,
        Pulse {
            base_width: 86.0,
            base_height: 86.0,
            amplitude: 0.18,
            speed: 4.0,
            phase: 0.0,
        },
    ))
}

fn spawn_pickup(
    world: &mut World,
    assets: &GameAssets,
    position: Vec2,
    kind: PickupKind,
) -> EntityId {
    let color = match kind {
        PickupKind::Core => Color::rgb(0.38, 1.0, 0.64),
        PickupKind::Heart => Color::rgb(1.0, 0.22, 0.34),
    };
    world.spawn((
        Transform::from_xyz(position.x(), position.y(), 0.1),
        assets.sprite_circle(25.0, color),
        SortingLayer(10),
        RigidBody2D::static_body(),
        Collider2D::circle(18.0).sensor(true),
        Pickup { kind },
        Pulse {
            base_width: 25.0,
            base_height: 25.0,
            amplitude: 0.14,
            speed: 4.8,
            phase: position.x() * 0.02,
        },
    ))
}

fn spawn_bullet(
    world: &mut World,
    assets: &GameAssets,
    position: Vec2,
    direction: Vec2,
) -> EntityId {
    world.spawn((
        Transform::from_xyz(position.x(), position.y(), 0.3),
        assets.sprite_circle(12.0, Color::rgb(1.0, 0.95, 0.4)),
        SortingLayer(30),
        RigidBody2D::kinematic().lock_rotation(),
        Collider2D::circle(6.0).sensor(true),
        Velocity2D::new(direction.x() * BULLET_SPEED, direction.y() * BULLET_SPEED),
        Bullet {
            damage: 1,
            ttl: BULLET_TTL,
        },
    ))
}

fn control_player(world: &mut World, input: &sky_engine::input::Input, player: Option<EntityId>) {
    let Some(player) = player else {
        return;
    };
    let mut direction = Vec2::ZERO;
    if input.key_held(KeyCode::KeyA) || input.key_held(KeyCode::ArrowLeft) {
        direction[0] -= 1.0;
    }
    if input.key_held(KeyCode::KeyD) || input.key_held(KeyCode::ArrowRight) {
        direction[0] += 1.0;
    }
    if input.key_held(KeyCode::KeyW) || input.key_held(KeyCode::ArrowUp) {
        direction[1] += 1.0;
    }
    if input.key_held(KeyCode::KeyS) || input.key_held(KeyCode::ArrowDown) {
        direction[1] -= 1.0;
    }
    if let Some(velocity) = world.get_mut::<Velocity2D>(player) {
        velocity.linear = direction.normalized() * PLAYER_SPEED;
        velocity.angular = 0.0;
    }
}

fn update_enemy_ai(world: &mut World, player: Option<EntityId>, dt: f32) {
    let Some(player) = player else {
        return;
    };
    let Some(player_pos) = entity_position(world, player) else {
        return;
    };

    let mut query = world.query_mut::<(&Transform, &mut Velocity2D, &mut Enemy)>();
    query.for_each(|(transform, velocity, enemy)| {
        enemy.damage_cooldown = (enemy.damage_cooldown - dt).max(0.0);
        let to_player = player_pos - position_of(transform);
        let wobble = Vec2::new((transform.position[1] * 0.013).sin(), 0.0) * 22.0;
        velocity.linear = (to_player.normalized() * enemy.speed) + wobble;
    });
}

fn update_bullets(world: &mut World, level: &mut LevelEntities, dt: f32) {
    let mut despawn = Vec::new();
    for &entity in &level.bullets {
        if let Some(bullet) = world.get_mut::<Bullet>(entity) {
            bullet.ttl -= dt;
            if bullet.ttl <= 0.0 {
                despawn.push(entity);
            }
        } else {
            despawn.push(entity);
        }
    }
    for entity in despawn {
        if world.contains(entity) {
            let _ = world.despawn(entity);
        }
    }
    level.retain_live(world);
}

fn update_sparks(world: &mut World, level: &mut LevelEntities, dt: f32) {
    let mut despawn = Vec::new();
    let mut query = world.query_mut::<(&mut Transform, &mut SpriteRenderer, &mut Spark)>();
    query.for_each_with_entity(|entity, (transform, sprite, spark)| {
        spark.ttl -= dt;
        transform.position[0] += spark.velocity.x() * dt;
        transform.position[1] += spark.velocity.y() * dt;
        transform.rotate_z(spark.spin * dt);
        sprite.color.a = spark.ttl.clamp(0.0, 1.0);
        if spark.ttl <= 0.0 {
            despawn.push(entity);
        }
    });
    for entity in despawn {
        if world.contains(entity) {
            let _ = world.despawn(entity);
        }
    }
    level.retain_live(world);
}

fn animate_pulses(world: &mut World, dt: f32) {
    let mut query = world.query_mut::<(&mut SpriteRenderer, &mut Pulse)>();
    query.for_each(|(sprite, pulse)| {
        pulse.phase += dt * pulse.speed;
        let scale = 1.0 + pulse.phase.sin() * pulse.amplitude;
        sprite.width = pulse.base_width * scale;
        sprite.height = pulse.base_height * scale;
    });
}

fn animate_background(world: &mut World, time: f32) {
    if let Some(settings) = world.get_resource_mut::<RenderSettings>() {
        let glow = time.sin() * 0.006;
        settings.clear_color = Color::rgb(0.018 + glow, 0.016, 0.032 + glow * 2.0);
    }
}

fn update_portal_visual(world: &mut World, portal: Option<EntityId>, open: bool) {
    let Some(portal) = portal else {
        return;
    };
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(portal) {
        sprite.color = if open {
            Color::new(0.26, 1.0, 0.72, 0.55)
        } else {
            Color::new(0.34, 0.18, 1.0, 0.28)
        };
    }
}

fn flash_locked_portal(world: &mut World, portal: Option<EntityId>) {
    let Some(portal) = portal else {
        return;
    };
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(portal) {
        sprite.color = Color::new(1.0, 0.18, 0.82, 0.5);
    }
}

fn flash_player(world: &mut World, player: Option<EntityId>, hp: i32) {
    let Some(player) = player else {
        return;
    };
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(player) {
        sprite.color = if hp <= 1 {
            Color::rgb(1.0, 0.18, 0.28)
        } else {
            Color::rgb(1.0, 0.82, 0.68)
        };
    }
}

fn restore_player_color(world: &mut World, player: Option<EntityId>) {
    let Some(player) = player else {
        return;
    };
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(player) {
        sprite.color = Color::rgb(0.22, 0.95, 1.0);
    }
}

fn stop_player(world: &mut World, player: Option<EntityId>) {
    let Some(player) = player else {
        return;
    };
    if let Some(velocity) = world.get_mut::<Velocity2D>(player) {
        velocity.linear = Vec2::ZERO;
        velocity.angular = 0.0;
    }
}

fn nearest_enemy(world: &World, from: Vec2) -> Option<Vec2> {
    let mut nearest = None;
    let mut nearest_distance = f32::MAX;
    let query = world.query::<(&Transform, &Enemy)>();
    query.for_each(|(transform, _enemy)| {
        let position = position_of(transform);
        let distance = (position - from).length_squared();
        if distance < nearest_distance {
            nearest_distance = distance;
            nearest = Some(position);
        }
    });
    nearest
}

fn entity_position(world: &World, entity: EntityId) -> Option<Vec2> {
    world.get::<Transform>(entity).map(position_of)
}

fn position_of(transform: &Transform) -> Vec2 {
    Vec2::new(transform.position[0], transform.position[1])
}

fn pair<A: 'static, B: 'static>(
    world: &World,
    a: EntityId,
    b: EntityId,
) -> Option<(EntityId, EntityId)> {
    if world.has::<A>(a) && world.has::<B>(b) {
        Some((a, b))
    } else if world.has::<A>(b) && world.has::<B>(a) {
        Some((b, a))
    } else {
        None
    }
}

fn push_unique(values: &mut Vec<EntityId>, entity: EntityId) {
    if !values.contains(&entity) {
        values.push(entity);
    }
}

fn random_spawn_edge(rng: &mut SimpleRng) -> Vec2 {
    match rng.next_u64() % 4 {
        0 => Vec2::new(rng.range(-430.0, 430.0), ARENA_H * 0.5 - 54.0),
        1 => Vec2::new(rng.range(-430.0, 430.0), -ARENA_H * 0.5 + 54.0),
        2 => Vec2::new(-ARENA_W * 0.5 + 54.0, rng.range(-260.0, 260.0)),
        _ => Vec2::new(ARENA_W * 0.5 - 54.0, rng.range(-260.0, 260.0)),
    }
}

fn spawn_hit_sparks(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelEntities,
    rng: &mut SimpleRng,
    position: Vec2,
    hue: f32,
) {
    for _ in 0..7 {
        let angle = rng.range(0.0, std::f32::consts::TAU);
        let speed = rng.range(75.0, 190.0);
        let velocity = Vec2::new(angle.cos() * speed, angle.sin() * speed);
        let spark = world.spawn((
            Transform::from_xyz(position.x(), position.y(), 0.4)
                .with_rotation(rng.range(-1.5, 1.5)),
            assets.sprite_circle(rng.range(4.0, 9.0), Color::hsl(hue, 0.9, 0.64)),
            SortingLayer(40),
            Spark {
                velocity,
                ttl: rng.range(0.25, 0.55),
                spin: rng.range(-8.0, 8.0),
            },
        ));
        level.sparks.push(spark);
        level.entities.push(spark);
    }
}

fn burst_at_player(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelEntities,
    rng: &mut SimpleRng,
) {
    if let Some(player) = level
        .player
        .and_then(|entity| entity_position(world, entity))
    {
        spawn_hit_sparks(world, assets, level, rng, player, 350.0);
    }
}

fn celebrate(
    world: &mut World,
    assets: &GameAssets,
    level: &mut LevelEntities,
    rng: &mut SimpleRng,
) {
    let origin = level
        .portal
        .and_then(|entity| entity_position(world, entity))
        .unwrap_or(Vec2::ZERO);
    for _ in 0..5 {
        let offset = Vec2::new(rng.range(-70.0, 70.0), rng.range(-50.0, 50.0));
        let hue = rng.range(120.0, 190.0);
        spawn_hit_sparks(world, assets, level, rng, origin + offset, hue);
    }
}

fn set_clear_color(world: &mut World, color: Color) {
    if let Some(settings) = world.get_resource_mut::<RenderSettings>() {
        settings.clear_color = color;
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
}
