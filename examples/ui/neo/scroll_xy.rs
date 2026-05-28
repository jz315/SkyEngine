//! Small `scroll_xy` validation demo.
//!
//! ```bash
//! cargo run --example ui_neo_scroll_xy --features ui-neo --release
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
const WINDOW_H: u32 = 620;

struct NeoScrollXYDemo {
    state: State<DemoState>,
    screenshot: ScreenshotProbe,
}

#[derive(Debug, Default)]
struct DemoState {
    canvas_scroll: (f32, f32),
}

impl Default for NeoScrollXYDemo {
    fn default() -> Self {
        Self {
            state: State::new(DemoState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl AppState for NeoScrollXYDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: sky_engine::render::Color::new(0.055, 0.065, 0.082, 1.0),
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
            let canvas_scroll = state.signal(
                "scroll-xy.canvas-scroll",
                |state| state.canvas_scroll,
                |state, value| state.canvas_scroll = (value.0.max(0.0), value.1.max(0.0)),
            );

            ui.rect("background")
                .size(screen.width, screen.height)
                .gradient(c(0.055, 0.065, 0.082, 1.0), c(0.095, 0.118, 0.145, 1.0))
                .build();

            ui.stack("stage")
                .size(screen.width, screen.height)
                .padding(34.0)
                .align(Align::Center, Align::Center)
                .content(|ui| {
                    widgets::panel(ui, "panel")
                        .fill()
                        .radius(30.0)
                        .gradient(c(0.105, 0.132, 0.165, 0.98), c(0.075, 0.087, 0.112, 0.99))
                        .border(1.0, c(0.440, 0.560, 0.660, 0.24))
                        .shadow(34.0, 0.0, 18.0, c(0.0, 0.0, 0.0, 0.30))
                        .build();

                    ui.column("panel.content")
                        .fill()
                        .padding(28.0)
                        .gap(20.0)
                        .content(|ui| {
                            ui.text("title")
                                .size(Size::fill(), 34.0)
                                .text("scroll_xy")
                                .font_size(28.0)
                                .line_height(34.0)
                                .color(c(0.945, 0.970, 1.0, 1.0))
                                .build();

                            ui.text("subtitle")
                                .size(Size::fill(), 22.0)
                                .text("A bidirectional scroll area for canvas-like UI without manual viewport math.")
                                .font_size(15.0)
                                .line_height(20.0)
                                .wrap(true)
                                .max_width(690.0)
                                .color(c(0.650, 0.725, 0.805, 1.0))
                                .build();

                            ui.scroll_xy("canvas")
                                .size(Size::fill(), Size::fill())
                                .content_size(980.0, 620.0)
                                .content_padding(18.0)
                                .scrollbar_size(8.0)
                                .scrollbar_gap(12.0)
                                .offset_signal(canvas_scroll)
                                .content(|ui| {
                                    draw_canvas(ui);
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

fn draw_canvas(ui: &mut sky_engine::ui::neo::Ui) {
    for y in 0..5 {
        for x in 0..7 {
            let id = format!("tile.{x}.{y}");
            let px = 22.0 + x as f32 * 132.0;
            let py = 22.0 + y as f32 * 112.0;
            let accent = match (x + y) % 5 {
                0 => c(0.280, 0.640, 0.960, 1.0),
                1 => c(0.420, 0.800, 0.620, 1.0),
                2 => c(0.760, 0.620, 0.960, 1.0),
                3 => c(0.920, 0.560, 0.320, 1.0),
                _ => c(0.950, 0.780, 0.350, 1.0),
            };

            ui.stack(id.clone())
                .position(px, py)
                .size(112.0, 86.0)
                .content(|ui| {
                    ui.rect(format!("{id}.bg"))
                        .size(Size::fill(), Size::fill())
                        .color(c(0.090, 0.115, 0.145, 0.94))
                        .radius(16.0)
                        .border(1.0, c(0.400, 0.510, 0.610, 0.22))
                        .build();
                    ui.rect(format!("{id}.accent"))
                        .position(14.0, 14.0)
                        .size(38.0, 8.0)
                        .color(accent)
                        .radius(4.0)
                        .build();
                    ui.text(format!("{id}.label"))
                        .position(14.0, 40.0)
                        .size(82.0, 22.0)
                        .text(format!("{x},{y}"))
                        .font_size(16.0)
                        .line_height(20.0)
                        .color(c(0.900, 0.940, 0.980, 1.0))
                        .build();
                });
        }
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
            WindowPlugin::new("Neo Scroll XY Demo", WINDOW_W, WINDOW_H)
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

    App::new(world).run(NeoScrollXYDemo::default());
}
