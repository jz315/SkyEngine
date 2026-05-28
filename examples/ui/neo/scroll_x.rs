//! Small `scroll_x` validation demo.
//!
//! ```bash
//! cargo run --example ui_neo_scroll_x --features ui-neo --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteFeature,
    Transform, TransparentPhase,
};
use sky_engine::ui::neo::{widgets, Align, Color, Size, State};

const WINDOW_W: u32 = 980;
const WINDOW_H: u32 = 520;

struct NeoScrollXDemo {
    state: State<DemoState>,
    screenshot: ScreenshotProbe,
}

#[derive(Debug, Default)]
struct DemoState {
    strip_scroll: f32,
}

impl Default for NeoScrollXDemo {
    fn default() -> Self {
        Self {
            state: State::new(DemoState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl AppState for NeoScrollXDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: sky_engine::render::Color::new(0.060, 0.075, 0.095, 1.0),
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
        let state = self.state.clone();
        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            let strip_scroll = state.signal(
                "scroll-x.strip-scroll",
                |state| state.strip_scroll,
                |state, value| state.strip_scroll = value.max(0.0),
            );

            ui.rect("background")
                .size(screen.width, screen.height)
                .gradient(c(0.060, 0.075, 0.095, 1.0), c(0.105, 0.135, 0.170, 1.0))
                .build();

            ui.stack("stage")
                .size(screen.width, screen.height)
                .padding(32.0)
                .align(Align::Center, Align::Center)
                .content(|ui| {
                    widgets::panel(ui, "panel")
                        .fill()
                        .radius(30.0)
                        .gradient(
                            c(0.115, 0.150, 0.185, 0.96),
                            c(0.075, 0.092, 0.125, 0.98),
                        )
                        .border(1.0, c(0.400, 0.510, 0.610, 0.24))
                        .shadow(32.0, 0.0, 16.0, c(0.0, 0.0, 0.0, 0.30))
                        .build();

                    ui.column("panel.content")
                        .fill()
                        .padding(28.0)
                        .gap(22.0)
                        .content(|ui| {
                            ui.text("title")
                                .size(Size::fill(), 34.0)
                                .text("scroll_x")
                                .font_size(28.0)
                                .line_height(34.0)
                                .color(c(0.945, 0.970, 1.0, 1.0))
                                .build();

                            ui.text("subtitle")
                                .size(Size::fill(), 22.0)
                                .text(
                                    "A panel-safe horizontal scroll area with explicit state and automatic clipping.",
                                )
                                .font_size(15.0)
                                .line_height(20.0)
                                .wrap(true)
                                .max_width(650.0)
                                .color(c(0.650, 0.725, 0.805, 1.0))
                                .build();

                            ui.scroll_x("cards")
                                .size(Size::fill(), 190.0)
                                .content_width(1120.0)
                                .content_padding_xy(18.0, 18.0)
                                .gap(16.0)
                                .scrollbar_height(8.0)
                                .scrollbar_gap(12.0)
                                .offset_signal(strip_scroll)
                                .content(|ui| {
                                    for index in 0..8 {
                                        feature_card(ui, index);
                                    }
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

fn feature_card(ui: &mut sky_engine::ui::neo::Ui, index: usize) {
    let id = format!("card.{index}");
    let accent = match index % 4 {
        0 => c(0.280, 0.640, 0.960, 1.0),
        1 => c(0.460, 0.820, 0.620, 1.0),
        2 => c(0.760, 0.620, 0.960, 1.0),
        _ => c(0.920, 0.560, 0.320, 1.0),
    };

    ui.stack(id.clone()).size(124.0, 118.0).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(Size::fill(), Size::fill())
            .color(c(0.095, 0.120, 0.155, 0.92))
            .radius(18.0)
            .border(1.0, c(0.400, 0.510, 0.610, 0.24))
            .build();
        ui.rect(format!("{id}.chip"))
            .position(16.0, 16.0)
            .size(44.0, 10.0)
            .color(accent)
            .radius(5.0)
            .build();
        ui.text(format!("{id}.title"))
            .position(16.0, 46.0)
            .size(92.0, 24.0)
            .text(format!("Card {:02}", index + 1))
            .font_size(17.0)
            .line_height(22.0)
            .color(c(0.900, 0.940, 0.980, 1.0))
            .build();
        ui.text(format!("{id}.meta"))
            .position(16.0, 76.0)
            .size(92.0, 20.0)
            .text("horizontal")
            .font_size(12.0)
            .line_height(16.0)
            .color(c(0.600, 0.700, 0.790, 1.0))
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
            frame: env_u32("SKY_NEO_SCREENSHOT_FRAME").unwrap_or(45),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_NEO_EXIT_AFTER_SCREENSHOT"),
        }
    }
}

impl ScreenshotProbe {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        if self.taken {
            return;
        }
        self.frame_count = self.frame_count.saturating_add(1);
        if self.frame_count >= self.frame {
            if let Some(path) = self.path.as_deref() {
                ctx.request_screenshot(path);
                self.taken = true;
                if self.exit_after {
                    ctx.request_exit();
                }
            }
        }
    }
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse().ok()
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("Neo Scroll X Demo", WINDOW_W, WINDOW_H)
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

    App::new(world).run(NeoScrollXDemo::default());
}
