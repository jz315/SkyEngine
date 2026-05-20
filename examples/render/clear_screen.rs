//! Clear screen — the simplest possible SkyEngine example.
//!
//! ```bash
//! cargo run --example clear_screen --features app
//! ```

use sky_engine::app::{App, AppState, AssetPlugin, FrameContext, InputPlugin, WindowPlugin};
use sky_engine::ecs::World;

struct ClearScreen;

impl AppState for ClearScreen {
    fn update(&mut self, ctx: &mut FrameContext) {
        let gpu = ctx.gpu();
        let mut frame = gpu.frame();
        let _pass = frame.begin_surface_pass(
            "clear",
            Some(wgpu::Color {
                r: 0.1,
                g: 0.15,
                b: 0.3,
                a: 1.0,
            }),
        );
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine — Clear Screen", 960, 640))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();

    App::new(world).run(ClearScreen);
}
