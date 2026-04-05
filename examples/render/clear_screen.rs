//! Clear screen — the simplest possible SkyEngine example.
//!
//! ```bash
//! cargo run --example clear_screen --features app
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::World;

struct ClearScreen;

impl AppState for ClearScreen {
    fn update(&mut self, ctx: &mut FrameContext) {
        ctx.gpu().with_surface_pass(
            "clear",
            Some(wgpu::Color {
                r: 0.1,
                g: 0.15,
                b: 0.3,
                a: 1.0,
            }),
            |_| {},
        );
    }
}

fn main() {
    App::new(
        AppConfig::new("SkyEngine — Clear Screen", 960, 640),
        World::new(),
    )
    .run(ClearScreen);
}
