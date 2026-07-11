//! # egui Demo
//!
//! Minimal example showing the egui overlay on a SkyEngine application.
//!
//! ```bash
//! cargo run --example egui_demo --features egui
//! ```

use sky_engine::app::{egui, App, AppState, AssetPlugin, FrameContext, InputPlugin, WindowPlugin};
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
        {
            let gpu = ctx.gpu();
            let mut frame = gpu.frame();
            let _pass = frame.begin_surface_pass(
                "clear",
                Some(wgpu::Color {
                    r: 0.08,
                    g: 0.08,
                    b: 0.12,
                    a: 1.0,
                }),
            );
        }

        let dt = ctx.dt;
        let counter = &mut self.counter;
        let name = &mut self.name;
        let slider_val = &mut self.slider_val;

        // egui overlay
        ctx.egui(|root_ui| {
            egui::CentralPanel::default().show_inside(root_ui, |ui| {
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
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine — egui Demo", 960, 640))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();

    App::new(world).run(EguiDemo::new());
}
