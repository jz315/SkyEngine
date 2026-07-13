//! Rust port of EUI-NEO's `app/demo.cpp`.
//!
//! ```bash
//! cargo run --example ui_serein_eui_demo --features ui-serein --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteFeature,
    Transform, TransparentPhase,
};
use sky_engine::ui::serein::widgets;
use sky_engine::ui::serein::{Align, Color, HorizontalAlign};

const WINDOW_W: u32 = 800;
const WINDOW_H: u32 = 600;

#[derive(Default)]
struct EuiSereinDemo {
    screenshot: ScreenshotProbe,
}

impl AppState for EuiSereinDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: sky_engine::render::Color::new(0.16, 0.18, 0.20, 1.0),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(WINDOW_H as f32),
            MainCamera,
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        sky_engine::ui::serein::compose(ctx, |ui, screen| {
            ui.rect("clear")
                .size(screen.width, screen.height)
                .color(c(0.16, 0.18, 0.20, 1.0))
                .build();

            ui.stack("root")
                .size(screen.width, screen.height)
                .align(Align::Center, Align::Center)
                .content(|ui| {
                    widgets::panel(ui, "card")
                        .size(360.0, 260.0)
                        .radius(18.0)
                        .gradient(c(0.10, 0.12, 0.16, 1.0), c(0.05, 0.07, 0.10, 1.0))
                        .border(1.0, c(0.23, 0.29, 0.38, 1.0))
                        .shadow(26.0, 0.0, 8.0, c(0.0, 0.0, 0.0, 0.26))
                        .build();

                    ui.column("content")
                        .size(360.0, 260.0)
                        .gap(8.0)
                        .justify_content(Align::Center)
                        .align_items(Align::Center)
                        .content(|ui| {
                            widgets::text(ui, "title")
                                .size(300.0, 38.0)
                                .text("Hello EUI")
                                .font_size(30.0)
                                .line_height(38.0)
                                .color(c(0.94, 0.97, 1.0, 1.0))
                                .horizontal_align(HorizontalAlign::Center)
                                .build();

                            widgets::text(ui, "subtitle")
                                .size(300.0, 30.0)
                                .margin_each(0.0, 0.0, 0.0, 16.0)
                                .text("Text Button Component")
                                .font_size(24.0)
                                .line_height(30.0)
                                .color(c(0.62, 0.70, 0.82, 1.0))
                                .horizontal_align(HorizontalAlign::Center)
                                .build();

                            widgets::button(ui, "primary")
                                .size(240.0, 70.0)
                                .text("Click Me")
                                .build();
                        });
                });
        });

        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        ctx.request_redraw();
    }
}

fn c(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

#[derive(Debug)]
struct ScreenshotProbe {
    path: Option<String>,
    frame: u32,
    frame_count: u32,
    taken: bool,
    exit_after: bool,
}

impl Default for ScreenshotProbe {
    fn default() -> Self {
        Self {
            path: std::env::var("SKY_SEREIN_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_SEREIN_SCREENSHOT_FRAME").unwrap_or(30),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_SEREIN_EXIT_AFTER_SCREENSHOT"),
        }
    }
}

impl ScreenshotProbe {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        if !self.taken && self.frame_count >= self.frame {
            if let Some(path) = self.path.as_ref() {
                ctx.request_screenshot(path);
                self.taken = true;
                if self.exit_after {
                    ctx.request_exit();
                }
            }
        }
        self.frame_count = self.frame_count.saturating_add(1);
    }
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("Hello EUI", WINDOW_W, WINDOW_H)
                .with_vsync(false)
                .with_resizable(true),
        )
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(EuiSereinDemo::default());
}
