mod actions;
mod app;
mod locale;
mod model;
mod theme;
mod view;

use sky_engine::app::{App, AssetPlugin, InputPlugin, RenderPlugin, RunnerPlugin, WindowPlugin};
use sky_engine::ecs::World;

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("Neo Control Center", 1440, 920)
                .with_vsync(true)
                .with_resizable(true),
        )
        .unwrap();
    world
        .install(RunnerPlugin::game().with_frame_rate_limit(60.0))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::forward_2d()).unwrap();

    App::new(world).run(app::NeoControlCenter::default());
}
