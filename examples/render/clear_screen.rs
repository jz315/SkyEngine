//! Minimal example: open a window and clear to a colour via the GPU backend.
//!
//! ```bash
//! cargo run --example clear_screen --features app --release
//! ```

use sky_engine::app::{App, AppConfig};

fn main() {
    App::run(
        AppConfig::new("SkyEngine — Clear Screen", 960, 640),
        |_world, _gpu| {
            eprintln!("[clear_screen] Setup complete. Press Escape to exit.");
        },
        |ctx| {
            ctx.gpu.with_surface_pass(
                "clear_screen",
                Some(wgpu::Color {
                    r: 0.05,
                    g: 0.05,
                    b: 0.12,
                    a: 1.0,
                }),
                |_pass| {},
            );
        },
    );
}
