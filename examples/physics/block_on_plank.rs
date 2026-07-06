//! Analytic block-on-plank teaching demo built with `EduCanvas`.
//!
//! The physics is formula-driven, while the drawing layer is coordinate-first:
//! each frame rebuilds a small named canvas and lets `EduCanvasRuntime` sync it
//! into ECS sprite entities.
//!
//! ```bash
//! cargo run --example block_on_plank_demo --features app --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::edu_canvas::{Edge, EduCanvas, EduCanvasPlugin, EduCanvasRuntime};
use sky_engine::input::KeyCode;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings,
    SpriteFeature, Transform, TransparentPhase,
};
#[cfg(feature = "ui-neo")]
use sky_engine::ui::neo::NeoUiPlugin;
#[cfg(feature = "ui-neo")]
use sky_engine::ui::neo::{Align, Color as NeoColor, HorizontalAlign};

const PIXELS_PER_METER: f32 = 72.0;
const BASE_X: f32 = -260.0;
const PLANK_Y: f32 = -45.0;
const PLANK_W: f32 = 720.0;
const PLANK_H: f32 = 44.0;
const BLOCK_W: f32 = 78.0;
const BLOCK_H: f32 = 58.0;
const BLOCK_START_OFFSET_X: f32 = 220.0;
const LOOP_HOLD_SECONDS: f32 = 1.4;

const BROWN: Color = Color::rgb(0.78, 0.63, 0.42);
const BLUE: Color = Color::rgb(0.20, 0.72, 0.96);
const RED: Color = Color::rgb(1.0, 0.42, 0.36);
const YELLOW: Color = Color::rgb(1.0, 0.80, 0.30);
const GRID: Color = Color::new(0.38, 0.46, 0.56, 0.34);
const GUIDE: Color = Color::new(0.76, 0.80, 0.86, 0.28);

struct BlockOnPlankDemo {
    model: PlankModel,
    time: f32,
    paused: bool,
    frame_count: u32,
    screenshot_path: Option<String>,
    screenshot_frame: u32,
    screenshot_taken: bool,
    exit_after_screenshot: bool,
}

impl BlockOnPlankDemo {
    fn new() -> Self {
        Self {
            model: PlankModel::default(),
            time: 0.0,
            paused: false,
            frame_count: 0,
            screenshot_path: std::env::var("SKY_BLOCK_PLANK_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            screenshot_frame: env_u32("SKY_BLOCK_PLANK_SCREENSHOT_FRAME").unwrap_or(36),
            screenshot_taken: false,
            exit_after_screenshot: env_flag("SKY_BLOCK_PLANK_EXIT_AFTER_SCREENSHOT"),
        }
    }
}

impl AppState for BlockOnPlankDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.035, 0.04, 0.052),
            ..Default::default()
        });
        spawn_camera(world);
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        handle_controls(ctx, &mut self.model, &mut self.time, &mut self.paused);

        if !self.paused {
            self.time += ctx.dt;
        }
        let loop_seconds = self.model.common_time() + LOOP_HOLD_SECONDS + 2.4;
        if self.time > loop_seconds {
            self.time = 0.0;
        }

        let sample = self
            .model
            .sample(self.time.min(loop_seconds - LOOP_HOLD_SECONDS));
        let canvas = build_canvas(&self.model, sample);
        report_canvas_errors(&canvas, self.frame_count);
        EduCanvasRuntime::sync_world(ctx.world, &canvas);

        #[cfg(feature = "ui-neo")]
        compose_neo_panel(ctx, &self.model, sample);

        ctx.render();
        #[cfg(feature = "ui-neo")]
        ctx.ui().render_overlays();

        self.frame_count = self.frame_count.wrapping_add(1);
        maybe_capture_screenshot(self, ctx);
        update_title(self, ctx, sample);
    }
}

#[derive(Clone, Copy)]
struct PlankModel {
    block_mass: f32,
    plank_mass: f32,
    mu: f32,
    g: f32,
    initial_plank_velocity: f32,
}

impl PlankModel {
    fn block_acceleration(self) -> f32 {
        self.mu * self.g
    }

    fn plank_acceleration(self) -> f32 {
        -self.mu * self.block_mass * self.g / self.plank_mass
    }

    fn relative_deceleration(self) -> f32 {
        self.mu * self.g * (1.0 + self.block_mass / self.plank_mass)
    }

    fn common_time(self) -> f32 {
        self.initial_plank_velocity / self.relative_deceleration()
    }

    fn common_velocity(self) -> f32 {
        self.plank_mass * self.initial_plank_velocity / (self.plank_mass + self.block_mass)
    }

    fn sample(self, time: f32) -> MotionSample {
        let tc = self.common_time();
        let t = time.min(tc);
        let block_a = self.block_acceleration();
        let plank_a = self.plank_acceleration();

        let block_x_tc = 0.5 * block_a * tc * tc;
        let plank_x_tc = self.initial_plank_velocity * tc + 0.5 * plank_a * tc * tc;
        let common_v = self.common_velocity();

        if time <= tc {
            MotionSample {
                block_x: 0.5 * block_a * t * t,
                plank_x: self.initial_plank_velocity * t + 0.5 * plank_a * t * t,
                block_v: block_a * t,
                plank_v: self.initial_plank_velocity + plank_a * t,
                sliding: true,
            }
        } else {
            let after = time - tc;
            MotionSample {
                block_x: block_x_tc + common_v * after,
                plank_x: plank_x_tc + common_v * after,
                block_v: common_v,
                plank_v: common_v,
                sliding: false,
            }
        }
    }
}

impl Default for PlankModel {
    fn default() -> Self {
        Self {
            block_mass: 1.0,
            plank_mass: 3.0,
            mu: 0.30,
            g: 9.8,
            initial_plank_velocity: 4.0,
        }
    }
}

#[derive(Clone, Copy)]
struct MotionSample {
    block_x: f32,
    plank_x: f32,
    block_v: f32,
    plank_v: f32,
    sliding: bool,
}

fn build_canvas(model: &PlankModel, sample: MotionSample) -> EduCanvas {
    let mut canvas = EduCanvas::new();
    draw_guides(&mut canvas);

    let plank_x = BASE_X + sample.plank_x * PIXELS_PER_METER;
    let block_x = BASE_X + BLOCK_START_OFFSET_X + sample.block_x * PIXELS_PER_METER;
    let plank = canvas
        .rect("plank")
        .center(plank_x, PLANK_Y)
        .size(PLANK_W, PLANK_H)
        .color(BROWN)
        .layer(5)
        .bounds();
    let block_y = plank.top() + BLOCK_H * 0.5;
    let block = canvas
        .rect("block")
        .center(block_x, block_y)
        .size(BLOCK_W, BLOCK_H)
        .color(BLUE)
        .layer(12)
        .bounds();

    draw_measurements(&mut canvas, model, sample);
    draw_friction_arrows(&mut canvas, block, plank.top(), sample.sliding);
    canvas
}

fn draw_guides(canvas: &mut EduCanvas) {
    canvas
        .rect("floor")
        .center(0.0, -230.0)
        .size(860.0, 20.0)
        .color(Color::rgb(0.46, 0.50, 0.55))
        .layer(0);

    for i in -5..=5 {
        let x = i as f32 * 80.0;
        canvas
            .line(format!("grid_x_{i}"))
            .from(x, -220.0)
            .to(x, 170.0)
            .thickness(1.5)
            .color(GRID)
            .layer(-10);
    }

    for (id, y) in [
        ("guide_slip", -135.0),
        ("guide_plank_v", 195.0),
        ("guide_block_v", 235.0),
    ] {
        canvas
            .line(id)
            .from(-300.0, y)
            .to(300.0, y)
            .thickness(2.0)
            .color(GUIDE)
            .layer(1);
    }
}

fn draw_measurements(canvas: &mut EduCanvas, model: &PlankModel, sample: MotionSample) {
    let max_v = model.initial_plank_velocity.max(1.0);
    draw_bar(
        canvas,
        "block_velocity",
        -300.0,
        235.0,
        (sample.block_v / max_v * 250.0).max(1.0),
        13.0,
        BLUE,
        11,
    );
    draw_bar(
        canvas,
        "plank_velocity",
        -300.0,
        195.0,
        (sample.plank_v / max_v * 250.0).max(1.0),
        13.0,
        Color::rgb(0.94, 0.58, 0.22),
        11,
    );

    let slip_px = (sample.plank_x - sample.block_x).max(0.0) * PIXELS_PER_METER;
    draw_bar(
        canvas,
        "relative_slip",
        -300.0,
        -135.0,
        slip_px.max(1.0),
        5.0,
        YELLOW,
        20,
    );
}

fn draw_friction_arrows(
    canvas: &mut EduCanvas,
    block: sky_engine::edu_canvas::Bounds,
    contact_y: f32,
    sliding: bool,
) {
    let left = block.edge_point(Edge::Bottom, -BLOCK_W * 0.32);
    let right = block.edge_point(Edge::Bottom, BLOCK_W * 0.32);
    canvas
        .arrow("friction_on_block")
        .from(left[0], contact_y + 14.0)
        .to_offset(72.0, 0.0)
        .thickness(7.0)
        .head_length(25.0)
        .color(RED)
        .layer(18)
        .visible(sliding);
    canvas
        .arrow("friction_on_plank")
        .from(right[0], contact_y - 14.0)
        .to_offset(-72.0, 0.0)
        .thickness(7.0)
        .head_length(25.0)
        .color(YELLOW)
        .layer(18)
        .visible(sliding);
}

fn draw_bar(
    canvas: &mut EduCanvas,
    id: &str,
    left_x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Color,
    layer: i32,
) {
    canvas
        .rect(id)
        .center(left_x + width * 0.5, y)
        .size(width, height)
        .color(color)
        .layer(layer);
}

fn report_canvas_errors(canvas: &EduCanvas, frame_count: u32) {
    let report = canvas
        .checks()
        .rect_edges_touch("block", Edge::Bottom, "plank", Edge::Top, 0.5)
        .rect_inside_x("block", "plank", 0.0)
        .finish();
    if !report.is_ok() && frame_count % 30 == 0 {
        for error in report.errors() {
            eprintln!("[block_on_plank_demo][canvas-check] {error}");
        }
    }
}

#[cfg(feature = "ui-neo")]
fn compose_neo_panel(ctx: &mut FrameContext<'_>, model: &PlankModel, sample: MotionSample) {
    let phase = if sample.sliding {
        "sliding friction"
    } else {
        "common velocity"
    };
    let block_a = model.block_acceleration();
    let plank_a = model.plank_acceleration();
    let tc = model.common_time();
    let vc = model.common_velocity();

    sky_engine::ui::neo::compose(ctx, |ui, screen| {
        ui.stack("edu.overlay")
            .size(screen.width, screen.height)
            .padding(24.0)
            .align(Align::Start, Align::End)
            .content(|ui| {
                ui.stack("edu.panel").size(342.0, 232.0).content(|ui| {
                    ui.rect("edu.panel.bg")
                        .fill()
                        .color(nc(0.055, 0.070, 0.088, 0.86))
                        .border(1.0, nc(0.36, 0.45, 0.55, 0.34))
                        .radius(16.0)
                        .build();

                    ui.column("edu.panel.content")
                        .fill()
                        .padding(18.0)
                        .gap(8.0)
                        .content(|ui| {
                            neo_text(
                                ui,
                                "edu.title",
                                "Block on Plank",
                                306.0,
                                28.0,
                                22.0,
                                nc(0.92, 0.96, 1.0, 1.0),
                            );
                            neo_text(
                                ui,
                                "edu.phase",
                                format!("phase: {phase}"),
                                306.0,
                                22.0,
                                15.0,
                                nc(0.62, 0.74, 0.86, 1.0),
                            );
                            neo_text(
                                ui,
                                "edu.model",
                                format!(
                                    "m={:.1} kg   M={:.1} kg   mu={:.2}   v0={:.1} m/s",
                                    model.block_mass,
                                    model.plank_mass,
                                    model.mu,
                                    model.initial_plank_velocity
                                ),
                                306.0,
                                22.0,
                                14.0,
                                nc(0.76, 0.82, 0.88, 1.0),
                            );
                            neo_text(
                                ui,
                                "edu.accel",
                                format!(
                                    "a_block=mu*g={block_a:.2}   a_plank=-mu*m*g/M={plank_a:.2}"
                                ),
                                306.0,
                                22.0,
                                14.0,
                                nc(0.76, 0.82, 0.88, 1.0),
                            );
                            neo_text(
                                ui,
                                "edu.common",
                                format!("t_common={tc:.2} s   v_common={vc:.2} m/s"),
                                306.0,
                                22.0,
                                14.0,
                                nc(0.76, 0.82, 0.88, 1.0),
                            );
                            neo_text(
                                ui,
                                "edu.now",
                                format!(
                                    "v_block={:.2} m/s   v_plank={:.2} m/s",
                                    sample.block_v, sample.plank_v
                                ),
                                306.0,
                                22.0,
                                14.0,
                                nc(0.76, 0.82, 0.88, 1.0),
                            );
                        });
                });
            });
    });
}

#[cfg(feature = "ui-neo")]
fn neo_text(
    ui: &mut sky_engine::ui::neo::Ui,
    id: &str,
    text: impl Into<String>,
    width: f32,
    height: f32,
    font_size: f32,
    color: NeoColor,
) {
    ui.text(id)
        .size(width, height)
        .text(text)
        .font_size(font_size)
        .line_height(height)
        .wrap(false)
        .horizontal_align(HorizontalAlign::Left)
        .color(color)
        .build();
}

#[cfg(feature = "ui-neo")]
fn nc(r: f32, g: f32, b: f32, a: f32) -> NeoColor {
    NeoColor::new(r, g, b, a)
}

fn handle_controls(
    ctx: &FrameContext<'_>,
    model: &mut PlankModel,
    time: &mut f32,
    paused: &mut bool,
) {
    if ctx.input.key_pressed(KeyCode::Space) {
        *paused = !*paused;
    }
    if ctx.input.key_pressed(KeyCode::KeyR) {
        *time = 0.0;
    }
    if ctx.input.key_pressed(KeyCode::ArrowUp) {
        model.mu = (model.mu + 0.03).min(0.75);
        *time = 0.0;
    }
    if ctx.input.key_pressed(KeyCode::ArrowDown) {
        model.mu = (model.mu - 0.03).max(0.22);
        *time = 0.0;
    }
    if ctx.input.key_pressed(KeyCode::ArrowRight) {
        model.initial_plank_velocity = (model.initial_plank_velocity + 0.4).min(5.0);
        *time = 0.0;
    }
    if ctx.input.key_pressed(KeyCode::ArrowLeft) {
        model.initial_plank_velocity = (model.initial_plank_velocity - 0.4).max(1.2);
        *time = 0.0;
    }
}

fn maybe_capture_screenshot(demo: &mut BlockOnPlankDemo, ctx: &mut FrameContext<'_>) {
    if demo.screenshot_taken || demo.frame_count < demo.screenshot_frame {
        return;
    }
    if let Some(path) = demo.screenshot_path.as_ref() {
        ctx.request_screenshot(path);
        demo.screenshot_taken = true;
        if demo.exit_after_screenshot {
            ctx.request_exit();
        }
    }
}

fn update_title(demo: &mut BlockOnPlankDemo, ctx: &FrameContext<'_>, sample: MotionSample) {
    if demo.frame_count % 12 != 0 {
        return;
    }
    let phase = if sample.sliding {
        "sliding"
    } else {
        "common velocity"
    };
    ctx.set_title(&format!(
        "SkyEngine - Block on Plank | {phase} | m={:.1}kg M={:.1}kg mu={:.2} v0={:.1}m/s | tc={:.2}s vc={:.2}m/s | Space pause, R reset, Up/Down mu, Left/Right v0",
        demo.model.block_mass,
        demo.model.plank_mass,
        demo.model.mu,
        demo.model.initial_plank_velocity,
        demo.model.common_time(),
        demo.model.common_velocity(),
    ));
}

fn spawn_camera(world: &mut World) {
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic(760.0),
        MainCamera,
    ));
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new(
            "SkyEngine - Block on Plank Teaching Demo",
            1040,
            720,
        ))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(EduCanvasPlugin).unwrap();
    #[cfg(feature = "ui-neo")]
    world.install(NeoUiPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(BlockOnPlankDemo::new());
}
