//! Top-down Tiled physics demo.
//!
//! ```bash
//! cargo run --example tiled_physics_demo --features "app physics" --release
//! cargo run --example tiled_physics_demo --features "app physics" -- path/to/map.tmx
//! ```

use std::path::PathBuf;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
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
    SpriteFeature, SpriteRenderer, TiledImport, TiledMapInstance, TiledPhysicsInstance,
    TiledPhysicsOptions, TiledSpawnOptions, TilemapFeature, Transform, TransparentPhase,
};

const PLAYER_SPEED: f32 = 160.0;
const PLAYER_SIZE: f32 = 16.0;

struct TiledPhysicsDemo {
    map: Option<TiledMapInstance>,
    physics: Option<TiledPhysicsInstance>,
    player: Option<EntityId>,
    camera: Option<EntityId>,
    last_trigger: String,
}

impl TiledPhysicsDemo {
    fn new() -> Self {
        Self {
            map: None,
            physics: None,
            player: None,
            camera: None,
            last_trigger: "none".to_string(),
        }
    }

    fn set_player_velocity(&self, ctx: &mut FrameContext<'_>) {
        let Some(player) = self.player else {
            return;
        };

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

        let velocity = direction.normalized() * PLAYER_SPEED;
        if let Some(player_velocity) = ctx.world.get_mut::<Velocity2D>(player) {
            player_velocity.linear = velocity;
            player_velocity.angular = 0.0;
        }
    }

    fn follow_player(&self, world: &mut World) {
        let (Some(player), Some(camera)) = (self.player, self.camera) else {
            return;
        };
        let Some(player_position) = world
            .get::<Transform>(player)
            .map(|transform| transform.position)
        else {
            return;
        };
        if let Some(camera_transform) = world.get_mut::<Transform>(camera) {
            camera_transform.position[0] = player_position[0];
            camera_transform.position[1] = player_position[1];
        }
    }

    fn drain_trigger_events(&mut self, world: &mut World) {
        let Some(events) = world.get_resource_mut::<PhysicsEvents>() else {
            return;
        };

        for event in events.drain() {
            let message = match event {
                PhysicsEvent2D::TriggerEntered { trigger, other } => {
                    format!("entered trigger {:?} with {:?}", trigger, other)
                }
                PhysicsEvent2D::TriggerExited { trigger, other } => {
                    format!("exited trigger {:?} with {:?}", trigger, other)
                }
                PhysicsEvent2D::ContactStarted { .. } | PhysicsEvent2D::ContactStopped { .. } => {
                    continue;
                }
            };
            eprintln!("[Tiled Physics] {message}");
            self.last_trigger = message;
        }
    }

    fn update_title(&self, ctx: &FrameContext<'_>) {
        ctx.set_title(&format!(
            "SkyEngine - Tiled Physics | WASD/Arrows move | Trigger: {}",
            self.last_trigger
        ));
    }
}

impl AppState for TiledPhysicsDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        PhysicsPlugin::new(PhysicsConfig2D::default())
            .install(world)
            .unwrap();

        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.035, 0.04, 0.045),
            ..Default::default()
        });

        let map_path = map_path_from_args().unwrap_or_else(default_map_path);
        let import = TiledImport::from_file(&map_path).expect("Tiled physics demo map should load");
        let map = TiledMapInstance::spawn_import(
            world,
            &import,
            TiledSpawnOptions::centered().with_parallax(false),
        )
        .expect("Tiled physics demo map should spawn");
        let physics = TiledPhysicsInstance::spawn(
            world,
            &import,
            map.origin(),
            TiledPhysicsOptions::default(),
        )
        .expect("Tiled physics colliders should spawn");

        let spawn = player_spawn_position(&import, map.origin());
        let player = world.spawn((
            Transform::from_xy(spawn[0], spawn[1]),
            SpriteRenderer::new(PLAYER_SIZE + 2.0, PLAYER_SIZE + 2.0)
                .color(Color::rgb(0.1, 0.8, 1.0)),
            SortingLayer(10_000),
            RigidBody2D::kinematic().lock_rotation(),
            Collider2D::rectangle(PLAYER_SIZE, PLAYER_SIZE).friction(0.0),
            Velocity2D::default(),
        ));
        let camera = world.spawn((
            Transform::from_xy(spawn[0], spawn[1]),
            CameraMarker::new(),
            Projection::orthographic(420.0),
            MainCamera,
        ));

        eprintln!(
            "[Tiled Physics] Loaded {}. Move with WASD/arrows, hit solid tiles, and step on trigger tiles.",
            map_path.display()
        );

        self.map = Some(map);
        self.physics = Some(physics);
        self.player = Some(player);
        self.camera = Some(camera);
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.set_player_velocity(ctx);
        self.follow_player(ctx.world);
        self.drain_trigger_events(ctx.world);
        ctx.render();
        self.update_title(ctx);
    }

    fn shutdown(&mut self, world: &mut World) {
        if let Some(player) = self.player.take() {
            let _ = world.despawn(player);
        }
        if let Some(camera) = self.camera.take() {
            let _ = world.despawn(camera);
        }
        if let Some(physics) = self.physics.take() {
            physics.despawn(world);
        }
        if let Some(map) = self.map.take() {
            map.despawn(world);
        }
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine - Tiled Physics", 960, 720).with_vsync(false))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(TilemapFeature::unlit())
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(TiledPhysicsDemo::new());
}

fn map_path_from_args() -> Option<PathBuf> {
    std::env::args_os().nth(1).map(PathBuf::from)
}

fn default_map_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("tiled")
        .join("physics_topdown.tmx")
}

fn player_spawn_position(import: &TiledImport, origin: [f32; 2]) -> [f32; 2] {
    [
        origin[0] + import.map.width() as f32 * import.tile_size[0] as f32 * 0.5,
        origin[1] + import.map.height() as f32 * import.tile_size[1] as f32 * 0.5,
    ]
}
