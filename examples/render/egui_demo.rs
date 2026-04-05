//! # egui Demo
//!
//! Minimal example showing the egui overlay on a SkyEngine application.
//!
//! ```bash
//! cargo run --example egui_demo --features egui
//! ```

use sky_engine::app::{egui, App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::World;

struct EguiDemo {
    counter: u32,
    name: String,
    slider_val: f32,
}

impl EguiDemo {
    fn new() -> Self {
        Self {
            counter: 0,
            name: String::from("SkyEngine User"),
            slider_val: 0.5,
        }
    }
}

impl AppState for EguiDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        // Clear to a nice dark background
        ctx.gpu().with_surface_pass(
            "clear",
            Some(wgpu::Color {
                r: 0.08,
                g: 0.08,
                b: 0.12,
                a: 1.0,
            }),
            |_| {},
        );

        let dt = ctx.dt;
        let counter = &mut self.counter;
        let name = &mut self.name;
        let slider_val = &mut self.slider_val;

        // egui overlay
        ctx.egui(|egui_ctx| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.heading("🚀 SkyEngine + egui");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Your name:");
                    ui.text_edit_singleline(name);
                });

                ui.add(egui::Slider::new(slider_val, 0.0..=1.0).text("Value"));

                if ui.button("Click me!").clicked() {
                    *counter += 1;
                }
                ui.label(format!("Button clicked {} times", counter));

                ui.separator();
                ui.label(format!("Frame time: {:.2}ms", dt * 1000.0));
                ui.label(format!("Hello, {}!", name));
            });
        });
    }
}

fn main() {
    let config = AppConfig::new("SkyEngine — egui Demo", 960, 640);
    App::new(config, World::new()).run(EguiDemo::new());
}
