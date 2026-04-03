//! Minimal example: open a window and clear to a colour via the GPU backend.
//!
//! ```bash
//! cargo run --example clear_screen --features app --release
//! ```

use sky_engine::app::{App, AppConfig};
use sky_engine::gpu::{Gpu, RenderPassDesc};

fn main() {
    App::run(
        AppConfig::new("SkyEngine — Clear Screen", 960, 640),
        |_world, _gpu| {
            eprintln!("[clear_screen] Setup complete. Press Escape to exit.");
        },
        |ctx| {
            ctx.gpu.with_render_pass(
                &RenderPassDesc::clear_surface([0.05, 0.05, 0.12, 1.0]),
                |_pass| {},
            );
        },
    );
}
