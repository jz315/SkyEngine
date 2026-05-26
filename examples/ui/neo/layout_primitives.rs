//! Small `ui-neo` layout primitive demo.
//!
//! This example intentionally uses the direct DSL instead of page-specific
//! helpers, so it is easy to inspect `padding`, `max_width`, and `grow`.
//!
//! ```bash
//! cargo run --example ui_neo_layout_primitives --features ui-neo --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteFeature,
    Transform, TransparentPhase,
};
use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{Align, Color, Size};

const WINDOW_W: u32 = 1100;
const WINDOW_H: u32 = 760;

#[derive(Default)]
struct NeoLayoutPrimitives {
    screenshot: ScreenshotProbe,
}

impl AppState for NeoLayoutPrimitives {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: c(0.055, 0.070, 0.090, 1.0).into(),
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
        sky_engine::ui::neo::compose(ctx, |ui, screen| {
            ui.rect("background")
                .size(screen.width, screen.height)
                .gradient(c(0.055, 0.070, 0.090, 1.0), c(0.110, 0.145, 0.175, 1.0))
                .build();

            ui.stack("stage")
                .size(screen.width, screen.height)
                .padding(32.0)
                .align(Align::Center, Align::Center)
                .content(|ui| {
                    ui.stack("card")
                        .size(Size::fill(), 472.0)
                        .min_width(360.0)
                        .max_width(760.0)
                        .content(|ui| {
                            widgets::panel(ui, "card.bg")
                                .fill()
                                .radius(30.0)
                                .gradient(
                                    c(0.120, 0.155, 0.190, 0.96),
                                    c(0.075, 0.092, 0.125, 0.98),
                                )
                                .border(1.0, c(0.410, 0.520, 0.610, 0.24))
                                .shadow(36.0, 0.0, 18.0, c(0.0, 0.0, 0.0, 0.32))
                                .build();

                            ui.column("card.content")
                                .fill()
                                .padding(28.0)
                                .gap(18.0)
                                .content(|ui| {
                                    header(ui);
                                    toolbar(ui);
                                    card_grid(ui);
                                    footer(ui);
                                });
                        });
                });
        });

        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        ctx.request_redraw();
    }
}

fn header(ui: &mut sky_engine::ui::neo::Ui) {
    ui.row("header")
        .size(Size::fill(), 78.0)
        .gap(18.0)
        .align_items(Align::Center)
        .content(|ui| {
            ui.column("header.copy")
                .size(260.0, Size::fill())
                .grow(1.0)
                .justify_content(Align::Center)
                .gap(4.0)
                .content(|ui| {
                    ui.text("title")
                        .size(Size::fill(), 34.0)
                        .text("Layout primitives")
                        .font_size(28.0)
                        .line_height(34.0)
                        .color(c(0.940, 0.970, 1.0, 1.0))
                        .build();

                    ui.text("subtitle")
                        .size(Size::fill(), 22.0)
                        .text("Padding owns the content box. Grow owns leftover space.")
                        .font_size(15.0)
                        .line_height(22.0)
                        .max_width(520.0)
                        .wrap(true)
                        .color(c(0.660, 0.735, 0.820, 1.0))
                        .build();
                });

            widgets::badge(ui, "header.badge")
                .text("max 760px")
                .accent(c(0.280, 0.640, 0.960, 1.0))
                .min_width(128.0)
                .build();
        });
}

fn toolbar(ui: &mut sky_engine::ui::neo::Ui) {
    ui.row("toolbar")
        .size(Size::fill(), 52.0)
        .gap(12.0)
        .align_items(Align::Center)
        .content(|ui| {
            widgets::button(ui, "filter")
                .text("Filter")
                .height(44.0)
                .min_width(104.0)
                .secondary_theme(widgets::theme::dark_theme_colors())
                .build();

            widgets::input(ui, "search")
                .placeholder("Search tasks")
                .height(44.0)
                .grow(1.0)
                .min_width(160.0)
                .max_width(390.0)
                .build();

            widgets::button(ui, "new")
                .text("New Task")
                .height(44.0)
                .min_width(124.0)
                .build();
        });
}

fn card_grid(ui: &mut sky_engine::ui::neo::Ui) {
    ui.row("grid")
        .size(Size::fill(), 188.0)
        .gap(14.0)
        .content(|ui| {
            metric_card(ui, "alpha", "Alpha", "grow 1", "max 260", 1.0, 260.0);
            metric_card(ui, "beta", "Beta", "grow 1", "max 180", 1.0, 180.0);
            metric_card(ui, "gamma", "Gamma", "grow 2", "fills rest", 2.0, 0.0);
        });
}

fn metric_card(
    ui: &mut sky_engine::ui::neo::Ui,
    id: &str,
    title: &str,
    value: &str,
    note: &str,
    grow: f32,
    max_width: f32,
) {
    let card_id = format!("grid.{id}");
    let bg_id = format!("{card_id}.bg");
    let title_id = format!("{card_id}.title");
    let value_id = format!("{card_id}.value");
    let note_id = format!("{card_id}.note");

    let mut card = ui
        .stack(card_id)
        .size(120.0, Size::fill())
        .grow(grow)
        .min_width(120.0);
    if max_width > 0.0 {
        card = card.max_width(max_width);
    }

    card.content(|ui| {
        ui.rect(bg_id)
            .fill()
            .radius(22.0)
            .color(c(0.095, 0.125, 0.160, 0.92))
            .border(1.0, c(0.370, 0.480, 0.590, 0.20))
            .build();

        ui.column(format!("{id}.content"))
            .fill()
            .padding(18.0)
            .gap(10.0)
            .content(|ui| {
                ui.text(title_id)
                    .size(Size::fill(), 24.0)
                    .text(title)
                    .font_size(16.0)
                    .line_height(24.0)
                    .color(c(0.800, 0.865, 0.930, 1.0))
                    .build();

                ui.text(value_id)
                    .size(Size::fill(), 38.0)
                    .text(value)
                    .font_size(25.0)
                    .line_height(38.0)
                    .color(c(0.950, 0.975, 1.0, 1.0))
                    .build();

                ui.text(note_id)
                    .size(Size::fill(), 48.0)
                    .text(note)
                    .font_size(14.0)
                    .line_height(20.0)
                    .wrap(true)
                    .color(c(0.620, 0.700, 0.780, 1.0))
                    .build();
            });
    });
}

fn footer(ui: &mut sky_engine::ui::neo::Ui) {
    ui.row("footer")
        .size(Size::fill(), 42.0)
        .gap(10.0)
        .align_items(Align::Center)
        .content(|ui| {
            widgets::badge(ui, "footer.padding")
                .text("padding(28)")
                .accent(c(0.890, 0.560, 0.260, 1.0))
                .build();
            widgets::badge(ui, "footer.gap")
                .text("gap(18)")
                .accent(c(0.460, 0.820, 0.620, 1.0))
                .build();
            widgets::badge(ui, "footer.grow")
                .text("grow redistribution")
                .accent(c(0.760, 0.620, 0.960, 1.0))
                .build();
        });
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
            path: std::env::var("SKY_NEO_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_NEO_SCREENSHOT_FRAME").unwrap_or(20),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_NEO_EXIT_AFTER_SCREENSHOT"),
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
            WindowPlugin::new("Neo Layout Primitives", WINDOW_W, WINDOW_H)
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

    App::new(world).run(NeoLayoutPrimitives::default());
}
