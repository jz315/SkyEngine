//! Tiled TMX import demo using an official sample map.
//!
//! ```bash
//! cargo run --example tiled_import_demo --features app --release
//! cargo run --example tiled_import_demo --features app --release -- path/to/map.tmx
//! ```

use std::path::PathBuf;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings,
    SpriteFeature, TiledMapInstance, TiledSpawnOptions, TilemapFeature, Transform,
    TransparentPhase,
};

struct TiledImportDemo;

impl AppState for TiledImportDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        let map_path = map_path_from_args().unwrap_or_else(default_map_path);

        world.spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            CameraMarker::new(),
            Projection::orthographic(1320.0),
            MainCamera,
        ));

        let _map = TiledMapInstance::spawn(world, map_path, TiledSpawnOptions::centered())
            .expect("official Tiled sample should load and spawn");

        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.025, 0.03, 0.034),
            ..Default::default()
        });
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.render();
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine - Tiled Import Demo", 960, 720).with_vsync(false))
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

    App::new(world).run(TiledImportDemo);
}

fn map_path_from_args() -> Option<PathBuf> {
    std::env::args_os().nth(1).map(PathBuf::from)
}

fn default_map_path() -> PathBuf {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("tiled");
    let island = assets
        .join("tiled")
        .join("examples")
        .join("rpg")
        .join("island.tmx");
    if island.exists() {
        island
    } else {
        assets.join("sewers.tmx")
    }
}
