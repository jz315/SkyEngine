//! Small `scroll_y` validation demo.
//!
//! This keeps the scroll pattern explicit and thin: a fixed viewport, a known
//! content height, a little padding, and no page-specific helper wrapper.
//!
//! ```bash
//! cargo run --example ui_serein_scroll_y --features ui-serein --release
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
use sky_engine::ui::serein::{Align, Color, Size, State};

const WINDOW_W: u32 = 980;
const WINDOW_H: u32 = 680;

struct SereinScrollColumnDemo {
    state: State<DemoState>,
    screenshot: ScreenshotProbe,
}

#[derive(Debug, Default)]
struct DemoState {
    activity_scroll: f32,
}

impl Default for SereinScrollColumnDemo {
    fn default() -> Self {
        Self {
            state: State::new(DemoState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl AppState for SereinScrollColumnDemo {
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
        sky_engine::ui::serein::compose(ctx, move |ui, screen| {
            let activity_scroll = state.signal(
                "scroll-y.activity-scroll",
                |state| state.activity_scroll,
                |state, value| state.activity_scroll = value.max(0.0),
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
                        .gap(18.0)
                        .content(|ui| {
                            ui.column("header")
                                .size(Size::fill(), 74.0)
                                .gap(4.0)
                                .content(|ui| {
                                    ui.text("title")
                                        .size(Size::fill(), 34.0)
                                        .text("scroll_y")
                                        .font_size(28.0)
                                        .line_height(34.0)
                                        .color(c(0.945, 0.970, 1.0, 1.0))
                                        .build();

                                    ui.text("subtitle")
                                        .size(Size::fill(), 20.0)
                                        .text(
                                            "A panel-safe vertical scroll area with explicit state and automatic clipping.",
                                        )
                                        .font_size(15.0)
                                        .line_height(20.0)
                                        .wrap(true)
                                        .max_width(520.0)
                                        .color(c(0.650, 0.725, 0.805, 1.0))
                                        .build();
                                });

                            ui.scroll_y("activity")
                                .size(Size::fill(), 318.0)
                                .content_height(612.0)
                                .content_padding_xy(16.0, 16.0)
                                .gap(10.0)
                                .scrollbar_gap(10.0)
                                .offset_signal(activity_scroll)
                                .content(|ui| {
                                    activity_row(
                                        ui,
                                        "activity.0",
                                        "New task",
                                        "Ready to launch",
                                        c(0.280, 0.640, 0.960, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.1",
                                        "新任务",
                                        "已排队",
                                        c(0.460, 0.820, 0.620, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.2",
                                        "Sync",
                                        "保持滚动模板简单",
                                        c(0.760, 0.620, 0.960, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.3",
                                        "Control center",
                                        "Direct DSL, no manual viewport math",
                                        c(0.890, 0.560, 0.260, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.4",
                                        "本地化",
                                        "长标签也要稳",
                                        c(0.920, 0.430, 0.500, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.5",
                                        "Inspection",
                                        "No hidden helper layer",
                                        c(0.380, 0.760, 0.880, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.6",
                                        "Queue",
                                        "Scroll offset remains explicit",
                                        c(0.980, 0.760, 0.350, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.7",
                                        "发布",
                                        "细节继续可见",
                                        c(0.560, 0.760, 0.960, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.8",
                                        "Demo",
                                        "Small and honest",
                                        c(0.680, 0.880, 0.560, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.9",
                                        "Scroll",
                                        "Vertical only for now",
                                        c(0.960, 0.590, 0.430, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.10",
                                        "任务队列",
                                        "The template stays thin",
                                        c(0.720, 0.620, 0.960, 1.0),
                                    );
                                    activity_row(
                                        ui,
                                        "activity.11",
                                        "Wrap-up",
                                        "Ready for screenshot validation",
                                        c(0.420, 0.820, 0.740, 1.0),
                                    );
                                });

                            widgets::badge(ui, "footer.badge")
                                .text("scroll_y")
                                .accent(c(0.280, 0.640, 0.960, 1.0))
                                .min_width(138.0)
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

fn activity_row(
    ui: &mut sky_engine::ui::serein::Ui,
    id: &str,
    title: &str,
    subtitle: &str,
    accent: Color,
) {
    let row_id = format!("{id}.row");
    let detail_id = format!("{id}.detail");
    let status_id = format!("{id}.status");

    ui.row(row_id)
        .size(Size::fill(), 44.0)
        .gap(12.0)
        .align_items(Align::Center)
        .content(|ui| {
            widgets::badge(ui, format!("{id}.badge"))
                .text(title)
                .accent(accent)
                .min_width(104.0)
                .build();

            ui.text(detail_id)
                .size(Size::fill(), 20.0)
                .text(subtitle)
                .font_size(14.0)
                .line_height(20.0)
                .color(c(0.850, 0.905, 0.955, 1.0))
                .build();

            ui.text(status_id)
                .size(96.0, 20.0)
                .text("OK")
                .font_size(13.0)
                .line_height(20.0)
                .horizontal_align(sky_engine::ui::serein::HorizontalAlign::Right)
                .color(c(0.620, 0.710, 0.790, 1.0))
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
            path: std::env::var("SKY_SEREIN_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_SEREIN_SCREENSHOT_FRAME").unwrap_or(20),
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
            WindowPlugin::new("Serein Scroll Column Demo", WINDOW_W, WINDOW_H)
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

    App::new(world).run(SereinScrollColumnDemo::default());
}
