//! A colorful 2D physics arcade for the optional Rapier-backed physics module.
//!
//! ```bash
//! cargo run --example physics_arcade_demo --features "app physics" --release
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
use sky_engine::asset::{AssetServer, Handle, TextureAsset};
use sky_engine::ecs::{EntityId, World};
use sky_engine::input::KeyCode;
use sky_engine::math::Vec2;
use sky_engine::physics::{
    install_physics, install_physics_debug_draw, sync_physics_debug_draw, Collider2D,
    PhysicsConfig2D, PhysicsDebugDraw2D, PhysicsDebugDrawOptions2D, PhysicsEvent2D, PhysicsEvents,
    PhysicsWorld2D, RigidBody2D, Velocity2D,
};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SortingLayer,
    SpriteFeature, SpriteRenderer, Transform, TransparentPhase,
};

const ARENA_W: f32 = 820.0;
const ARENA_H: f32 = 560.0;
const MAX_TOYS: usize = 180;
const BURST_COUNT: usize = 24;
const PADDLE_SPEED: f32 = 420.0;

#[derive(Clone, Copy)]
struct PhysicsToy;

#[derive(Clone, Copy)]
struct PlayerPaddle;

#[derive(Clone, Copy)]
struct MixerArm {
    speed: f32,
}

struct PhysicsArcadeDemo {
    assets: ArcadeAssets,
    scene: ArcadeScene,
    spawner: ToySpawner,
    gravity: GravityController,
    telemetry: ArcadeTelemetry,
    effects: ArcadeEffects,
    hud: ArcadeHud,
}

impl PhysicsArcadeDemo {
    fn new() -> Self {
        Self {
            assets: ArcadeAssets::default(),
            scene: ArcadeScene::default(),
            spawner: ToySpawner::new(0x5EED_2026),
            gravity: GravityController::default(),
            telemetry: ArcadeTelemetry::default(),
            effects: ArcadeEffects::default(),
            hud: ArcadeHud::default(),
        }
    }
}

impl AppState for PhysicsArcadeDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        self.gravity.install(world);
        install_physics_debug_draw(
            world,
            PhysicsDebugDrawOptions2D {
                enabled: false,
                line_thickness: 2.5,
                z: 0.9,
                ..Default::default()
            },
        );
        self.assets.load(world);

        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.025, 0.03, 0.045),
            ..Default::default()
        });
        spawn_camera(world);

        self.scene.spawn(world);
        self.spawner
            .spawn_burst(world, &self.assets, [-170.0, 205.0], BURST_COUNT);
        self.spawner
            .spawn_burst(world, &self.assets, [140.0, 240.0], BURST_COUNT);
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.spawner.clear_initial_velocities(ctx.world);
        update_controls(
            ctx,
            &self.scene,
            &mut self.spawner,
            &mut self.gravity,
            &self.assets,
        );
        self.telemetry.drain_events(ctx.world, &mut self.effects);
        self.spawner.prune(ctx.world);
        animate_mixers(ctx.world, ctx.dt);
        self.effects.update(ctx.world, self.scene.sensor, ctx.dt);
        toggle_debug_draw(ctx);

        ctx.render();
        self.hud.update(
            ctx,
            self.spawner.count(),
            &self.telemetry,
            self.gravity.mode(),
            debug_draw_enabled(ctx.world),
        );
    }

    fn shutdown(&mut self, world: &mut World) {
        self.spawner.clear(world);
        self.assets.unload(world);
    }
}

#[derive(Default)]
struct ArcadeAssets {
    circle_texture: Option<Handle<TextureAsset>>,
}

impl ArcadeAssets {
    fn load(&mut self, world: &World) {
        self.circle_texture = world
            .get_resource::<AssetServer>()
            .map(|server| server.insert_runtime(TextureAsset::circle(64)));
    }

    fn unload(&mut self, world: &World) {
        let Some(texture) = self.circle_texture.take() else {
            return;
        };
        if let Some(asset_server) = world.get_resource::<AssetServer>().cloned() {
            asset_server.unload(&texture);
        }
    }
}

#[derive(Default)]
struct ArcadeScene {
    paddle: Option<EntityId>,
    sensor: Option<EntityId>,
}

impl ArcadeScene {
    fn spawn(&mut self, world: &mut World) {
        spawn_grid(world);
        spawn_static_bar(
            world,
            0.0,
            -ARENA_H * 0.5,
            ARENA_W,
            32.0,
            0.0,
            Color::rgb(0.35, 0.42, 0.48),
        );
        spawn_static_bar(
            world,
            -ARENA_W * 0.5,
            0.0,
            32.0,
            ARENA_H,
            0.0,
            Color::rgb(0.22, 0.32, 0.42),
        );
        spawn_static_bar(
            world,
            ARENA_W * 0.5,
            0.0,
            32.0,
            ARENA_H,
            0.0,
            Color::rgb(0.22, 0.32, 0.42),
        );
        spawn_static_bar(
            world,
            -165.0,
            -105.0,
            210.0,
            18.0,
            0.36,
            Color::rgb(0.1, 0.82, 0.95),
        );
        spawn_static_bar(
            world,
            170.0,
            18.0,
            235.0,
            18.0,
            -0.42,
            Color::rgb(1.0, 0.42, 0.18),
        );
        spawn_static_bar(
            world,
            -255.0,
            118.0,
            160.0,
            16.0,
            -0.24,
            Color::rgb(0.92, 0.2, 0.88),
        );

        self.sensor = Some(spawn_sensor(world));
        spawn_mixer(world);
        self.paddle = Some(spawn_paddle(world));
    }
}

struct ToySpawner {
    rng: SimpleRng,
    toys: Vec<EntityId>,
    pending_velocity_clear: Vec<EntityId>,
}

impl ToySpawner {
    fn new(seed: u64) -> Self {
        Self {
            rng: SimpleRng::new(seed),
            toys: Vec::new(),
            pending_velocity_clear: Vec::new(),
        }
    }

    fn spawn_burst(
        &mut self,
        world: &mut World,
        assets: &ArcadeAssets,
        center: [f32; 2],
        count: usize,
    ) {
        for i in 0..count {
            let x = center[0] + self.rng.range(-74.0, 74.0);
            let y = center[1] + self.rng.range(-12.0, 68.0) + i as f32 * 1.5;
            let vx = self.rng.range(-210.0, 210.0);
            let vy = self.rng.range(90.0, 360.0);
            let hue = self.rng.range(0.0, 360.0);
            let entity = if self.rng.chance(0.46) {
                let radius = self.rng.range(13.0, 25.0);
                self.spawn_ball(world, assets, [x, y], radius, hue, [vx, vy])
            } else {
                let size = [self.rng.range(18.0, 42.0), self.rng.range(16.0, 36.0)];
                self.spawn_box(world, [x, y], size, hue, [vx, vy])
            };
            self.toys.push(entity);
            self.pending_velocity_clear.push(entity);
        }
        self.prune(world);
    }

    fn clear_initial_velocities(&mut self, world: &mut World) {
        for entity in self.pending_velocity_clear.drain(..) {
            if world.contains(entity) {
                let _ = world.remove::<Velocity2D>(entity);
            }
        }
    }

    fn prune(&mut self, world: &mut World) {
        let mut remove = Vec::new();
        for &entity in &self.toys {
            let should_remove = if let Some(transform) = world.get::<Transform>(entity) {
                transform.position[1] < -520.0
                    || transform.position[1] > 520.0
                    || transform.position[0].abs() > 720.0
            } else {
                true
            };
            if should_remove {
                remove.push(entity);
            }
        }
        for entity in remove {
            let _ = world.despawn(entity);
        }

        while self.toys.len() > MAX_TOYS {
            if let Some(entity) = self.toys.first().copied() {
                let _ = world.despawn(entity);
            }
            self.toys.remove(0);
        }
        self.toys.retain(|entity| world.contains(*entity));
    }

    fn clear(&mut self, world: &mut World) {
        for entity in self.toys.drain(..) {
            let _ = world.despawn(entity);
        }
        self.pending_velocity_clear.clear();
    }

    fn count(&self) -> usize {
        self.toys.len()
    }

    fn random_x(&mut self) -> f32 {
        self.rng.range(-260.0, 260.0)
    }

    fn spawn_ball(
        &mut self,
        world: &mut World,
        assets: &ArcadeAssets,
        position: [f32; 2],
        radius: f32,
        hue: f32,
        velocity: [f32; 2],
    ) -> EntityId {
        let mut sprite =
            SpriteRenderer::new(radius * 2.0, radius * 2.0).color(Color::hsl(hue, 0.86, 0.62));
        if let Some(texture) = assets.circle_texture {
            sprite = sprite.texture(texture);
        }

        world.spawn((
            Transform::from_xy(position[0], position[1]),
            sprite,
            SortingLayer(8),
            RigidBody2D::dynamic(),
            Collider2D::circle(radius).friction(0.52).restitution(0.7),
            Velocity2D::new(velocity[0], velocity[1]).with_angular(self.rng.range(-5.0, 5.0)),
            PhysicsToy,
        ))
    }

    fn spawn_box(
        &mut self,
        world: &mut World,
        position: [f32; 2],
        size: [f32; 2],
        hue: f32,
        velocity: [f32; 2],
    ) -> EntityId {
        world.spawn((
            Transform::from_xy(position[0], position[1]).with_rotation(self.rng.range(-0.7, 0.7)),
            SpriteRenderer::new(size[0], size[1]).color(Color::hsl(hue, 0.78, 0.58)),
            SortingLayer(7),
            RigidBody2D::dynamic(),
            Collider2D::rectangle(size[0], size[1])
                .friction(0.72)
                .restitution(0.28),
            Velocity2D::new(velocity[0], velocity[1]).with_angular(self.rng.range(-6.0, 6.0)),
            PhysicsToy,
        ))
    }
}

#[derive(Clone, Copy)]
enum GravityMode {
    Down,
    Up,
    Zero,
    Side,
}

impl GravityMode {
    fn next(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Zero,
            Self::Zero => Self::Side,
            Self::Side => Self::Down,
        }
    }

    fn vector(self) -> Vec2 {
        match self {
            Self::Down => Vec2::new(0.0, -900.0),
            Self::Up => Vec2::new(0.0, 900.0),
            Self::Zero => Vec2::ZERO,
            Self::Side => Vec2::new(700.0, -120.0),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
            Self::Zero => "zero",
            Self::Side => "side",
        }
    }
}

struct GravityController {
    mode: GravityMode,
}

impl GravityController {
    fn install(&self, world: &mut World) {
        install_physics(
            world,
            PhysicsConfig2D {
                gravity: self.mode.vector(),
                fixed_dt: 1.0 / 90.0,
                pixels_per_meter: 64.0,
            },
        );
    }

    fn cycle(&mut self, world: &mut World) {
        self.mode = self.mode.next();
        if let Some(physics) = world.get_resource_mut::<PhysicsWorld2D>() {
            let mut config = physics.config();
            config.gravity = self.mode.vector();
            physics.set_config(config);
        }
    }

    fn mode(&self) -> GravityMode {
        self.mode
    }
}

impl Default for GravityController {
    fn default() -> Self {
        Self {
            mode: GravityMode::Down,
        }
    }
}

#[derive(Default)]
struct ArcadeTelemetry {
    contacts: u32,
    triggers: u32,
}

impl ArcadeTelemetry {
    fn drain_events(&mut self, world: &mut World, effects: &mut ArcadeEffects) {
        let Some(events) = world.get_resource_mut::<PhysicsEvents>() else {
            return;
        };
        for event in events.drain() {
            match event {
                PhysicsEvent2D::ContactStarted { .. } => {
                    self.contacts = self.contacts.saturating_add(1);
                    effects.flash_contact();
                }
                PhysicsEvent2D::TriggerEntered { .. } => {
                    self.triggers = self.triggers.saturating_add(1);
                    effects.flash_sensor();
                }
                PhysicsEvent2D::ContactStopped { .. } | PhysicsEvent2D::TriggerExited { .. } => {}
            }
        }
    }
}

#[derive(Default)]
struct ArcadeEffects {
    contact_flash: f32,
    sensor_flash: f32,
}

impl ArcadeEffects {
    fn flash_contact(&mut self) {
        self.contact_flash = 1.0;
    }

    fn flash_sensor(&mut self) {
        self.sensor_flash = 1.0;
    }

    fn update(&mut self, world: &mut World, sensor: Option<EntityId>, dt: f32) {
        self.contact_flash = (self.contact_flash - dt * 2.8).max(0.0);
        self.sensor_flash = (self.sensor_flash - dt * 2.2).max(0.0);

        if let Some(settings) = world.get_resource_mut::<RenderSettings>() {
            settings.clear_color = Color::rgb(
                0.025 + self.contact_flash * 0.08,
                0.03 + self.sensor_flash * 0.04,
                0.045 + self.contact_flash * 0.12,
            );
        }
        if let Some(sensor) = sensor {
            if let Some(sprite) = world.get_mut::<SpriteRenderer>(sensor) {
                sprite.color = Color::new(
                    0.86,
                    0.12 + self.sensor_flash * 0.45,
                    1.0,
                    0.16 + self.sensor_flash * 0.24,
                );
            }
        }
    }
}

#[derive(Default)]
struct ArcadeHud {
    frame_count: u32,
}

impl ArcadeHud {
    fn update(
        &mut self,
        ctx: &FrameContext<'_>,
        toy_count: usize,
        telemetry: &ArcadeTelemetry,
        gravity_mode: GravityMode,
        debug_draw: bool,
    ) {
        self.frame_count = self.frame_count.wrapping_add(1);
        if self.frame_count % 15 != 0 {
            return;
        }
        ctx.set_title(&format!(
            "SkyEngine - Physics Arcade | toys {toy_count} | contacts {} | triggers {} | gravity {} | debug {} | WASD move, Space burst, G gravity, F debug, R reset",
            telemetry.contacts,
            telemetry.triggers,
            gravity_mode.label(),
            if debug_draw { "on" } else { "off" }
        ));
    }
}

fn update_controls(
    ctx: &mut FrameContext<'_>,
    scene: &ArcadeScene,
    spawner: &mut ToySpawner,
    gravity: &mut GravityController,
    assets: &ArcadeAssets,
) {
    if ctx.input.key_pressed(KeyCode::Space) {
        let x = spawner.random_x();
        spawner.spawn_burst(ctx.world, assets, [x, 250.0], BURST_COUNT);
    }
    if ctx.input.key_pressed(KeyCode::KeyR) {
        spawner.clear(ctx.world);
        spawner.spawn_burst(ctx.world, assets, [-120.0, 225.0], BURST_COUNT);
        spawner.spawn_burst(ctx.world, assets, [150.0, 250.0], BURST_COUNT);
    }
    if ctx.input.key_pressed(KeyCode::KeyG) {
        gravity.cycle(ctx.world);
    }

    let mut direction = Vec2::ZERO;
    if ctx.input.key_held(KeyCode::KeyA) || ctx.input.key_held(KeyCode::ArrowLeft) {
        direction[0] -= 1.0;
    }
    if ctx.input.key_held(KeyCode::KeyD) || ctx.input.key_held(KeyCode::ArrowRight) {
        direction[0] += 1.0;
    }
    if ctx.input.key_held(KeyCode::KeyW) || ctx.input.key_held(KeyCode::ArrowUp) {
        direction[1] += 1.0;
    }
    if ctx.input.key_held(KeyCode::KeyS) || ctx.input.key_held(KeyCode::ArrowDown) {
        direction[1] -= 1.0;
    }

    let Some(paddle) = scene.paddle else {
        return;
    };
    if let Some(velocity) = ctx.world.get_mut::<Velocity2D>(paddle) {
        velocity.linear = direction.normalized() * PADDLE_SPEED;
        velocity.angular = 0.0;
    }
}

fn toggle_debug_draw(ctx: &mut FrameContext<'_>) {
    if !ctx.input.key_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(debug) = ctx.world.get_resource_mut::<PhysicsDebugDraw2D>() else {
        return;
    };
    debug.set_enabled(!debug.options().enabled);
    sync_physics_debug_draw(ctx.world);
}

fn debug_draw_enabled(world: &World) -> bool {
    world
        .get_resource::<PhysicsDebugDraw2D>()
        .map(|debug| debug.options().enabled)
        .unwrap_or(false)
}

fn animate_mixers(world: &mut World, dt: f32) {
    let mut mixers = world.query::<(&mut Transform, &MixerArm)>();
    mixers.for_each(world, |(transform, mixer)| {
        transform.rotate_z(mixer.speed * dt);
    });
}

fn main() {
    App::new(
        AppConfig::new("SkyEngine - Physics Arcade", 1120, 760).with_vsync(false),
        World::new(),
    )
    .with_render_pipeline(
        RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build(),
    )
    .run(PhysicsArcadeDemo::new());
}

fn spawn_camera(world: &mut World) {
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        CameraMarker::new(),
        Projection::orthographic(700.0),
        MainCamera,
    ));
}

fn spawn_grid(world: &mut World) {
    for x in (-5..=5).map(|i| i as f32 * 80.0) {
        world.spawn((
            Transform::from_xy(x, 0.0),
            SpriteRenderer::new(1.5, ARENA_H).color(Color::new(0.18, 0.42, 0.58, 0.16)),
            SortingLayer(-30),
        ));
    }
    for y in (-3..=3).map(|i| i as f32 * 80.0) {
        world.spawn((
            Transform::from_xy(0.0, y),
            SpriteRenderer::new(ARENA_W, 1.5).color(Color::new(0.18, 0.42, 0.58, 0.16)),
            SortingLayer(-30),
        ));
    }
}

fn spawn_static_bar(
    world: &mut World,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation: f32,
    color: Color,
) -> EntityId {
    world.spawn((
        Transform::from_xy(x, y).with_rotation(rotation),
        SpriteRenderer::new(width, height).color(color),
        SortingLayer(2),
        RigidBody2D::static_body(),
        Collider2D::rectangle(width, height)
            .friction(0.82)
            .restitution(0.35),
    ))
}

fn spawn_sensor(world: &mut World) -> EntityId {
    world.spawn((
        Transform::from_xy(285.0, 116.0),
        SpriteRenderer::new(150.0, 170.0).color(Color::new(0.86, 0.12, 1.0, 0.16)),
        SortingLayer(-5),
        RigidBody2D::static_body(),
        Collider2D::rectangle(150.0, 170.0).sensor(true),
    ))
}

fn spawn_mixer(world: &mut World) -> EntityId {
    world.spawn((
        Transform::from_xy(0.0, 54.0).with_rotation(0.0),
        SpriteRenderer::new(230.0, 14.0).color(Color::rgb(0.94, 0.95, 0.28)),
        SortingLayer(4),
        RigidBody2D::kinematic().lock_rotation(),
        Collider2D::rectangle(230.0, 14.0)
            .friction(0.2)
            .restitution(0.45),
        MixerArm { speed: 1.8 },
    ))
}

fn spawn_paddle(world: &mut World) -> EntityId {
    world.spawn((
        Transform::from_xy(0.0, -210.0),
        SpriteRenderer::new(92.0, 24.0).color(Color::rgb(0.18, 0.92, 1.0)),
        SortingLayer(20),
        RigidBody2D::dynamic().lock_rotation(),
        Collider2D::rectangle(92.0, 24.0)
            .friction(0.0)
            .restitution(0.35),
        Velocity2D::default(),
        PlayerPaddle,
    ))
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
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

    fn chance(&mut self, probability: f32) -> bool {
        self.next_f32() < probability
    }
}
