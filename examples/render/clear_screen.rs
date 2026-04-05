//! Minimal example: open a window and clear to a colour via the GPU backend.
//!
//! ```bash
//! cargo run --example clear_screen --features app --release
//! ```

use sky_engine::app::{App, AppConfig};
use sky_engine::ecs::World;
use sky_engine::render::Renderer2DConfig;

fn main() {
    let mut world = World::new();
    world.insert_resource(Renderer2DConfig::unlit());

    App::new(AppConfig::new("SkyEngine — Clear Screen", 960, 640), world)
        .run(|ctx| {
            ctx.gpu().with_surface_pass(
                "clear_screen",
                Some(wgpu::Color {
                    r: 0.05,
                    g: 0.05,
                    b: 0.12,
                    a: 1.0,
                }),
                |_pass| {},
            );
        });
}
