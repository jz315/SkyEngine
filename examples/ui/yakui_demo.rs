//! Experimental yakui backend demo.
//!
//! ```bash
//! cargo run --example yakui_demo --features yakui-ui --release
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
use sky_engine::ecs::World;
use sky_engine::plugin::Plugin;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings,
    SpriteFeature, Transform, TransparentPhase,
};
use sky_engine::ui::YakuiUiPlugin;

const WINDOW_W: u32 = 960;
const WINDOW_H: u32 = 600;

#[derive(Default)]
struct YakuiDemo {
    running: bool,
    energy: f32,
    volume: f64,
    assist: bool,
    score: u32,
}

impl AppState for YakuiDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.025, 0.03, 0.04),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(600.0),
            MainCamera,
        ));
        YakuiUiPlugin.install(ctx.world).unwrap();
        self.energy = 0.32;
        self.volume = 0.55;
        self.assist = true;
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.ui().update();
        self.animate(ctx.dt());
        self.draw_ui(ctx);

        ctx.render();
        ctx.ui().render_overlays();
        ctx.request_redraw();
    }
}

impl YakuiDemo {
    fn animate(&mut self, dt: f32) {
        if self.running {
            self.energy += dt * 0.18;
            if self.energy >= 1.0 {
                self.energy = 0.0;
                self.score += 10;
            }
        }
    }

    fn draw_ui(&mut self, ctx: &mut FrameContext<'_>) {
        let mut running = self.running;
        let mut energy = self.energy;
        let mut volume = self.volume;
        let mut assist = self.assist;
        let mut score = self.score;

        let pointer_owned = ctx.ui().wants_pointer();
        sky_engine::ui::yakui::run(ctx, |backend| {
            backend.run(|| {
                use yakui::{colors, Alignment, Color, Dim2, Pivot, Vec2};

                yakui::pad(yakui::widgets::Pad::all(22.0), || {
                    yakui::max_width(430.0, || {
                        yakui::opaque(|| {
                            yakui::colored_box_container(colors::BACKGROUND_2, || {
                                yakui::pad(yakui::widgets::Pad::all(18.0), || {
                                    yakui::column(|| {
                                        yakui::text(28.0, "SkyEngine yakui");
                                        yakui::label(format!(
                                            "score {:04}    energy {:>3}%",
                                            score,
                                            (energy * 100.0).round() as i32
                                        ));

                                        yakui::divider(colors::BACKGROUND_3, 16.0, 2.0);

                                        if yakui::button(if running { "Pause" } else { "Start" })
                                            .clicked
                                        {
                                            running = !running;
                                        }
                                        if yakui::button("Charge +25").clicked {
                                            energy = (energy + 0.25).min(1.0);
                                            score += 25;
                                        }

                                        yakui::row(|| {
                                            yakui::label("Assist");
                                            assist = yakui::checkbox(assist).checked;
                                        });

                                        yakui::label(format!(
                                            "Volume {:>3}%",
                                            (volume * 100.0).round() as i32
                                        ));
                                        if let Some(value) = yakui::slider(volume, 0.0, 1.0).value {
                                            volume = value;
                                        }

                                        yakui::label("Progress");
                                        yakui::stack(|| {
                                            yakui::colored_box(
                                                colors::BACKGROUND_3,
                                                Vec2::new(320.0, 16.0),
                                            );
                                            yakui::colored_box(
                                                Color::hex(0x80d4ff),
                                                Vec2::new(320.0 * energy, 16.0),
                                            );
                                        });

                                        yakui::reflow(
                                            Alignment::BOTTOM_RIGHT,
                                            Pivot::BOTTOM_RIGHT,
                                            Dim2::pixels(0.0, 0.0),
                                            || {
                                                yakui::label(if pointer_owned {
                                                    "UI owns pointer"
                                                } else {
                                                    "Scene input free"
                                                });
                                            },
                                        );
                                    });
                                });
                            });
                        });
                    });
                });
            });
        });

        self.running = running;
        self.energy = energy;
        self.volume = volume;
        self.assist = assist;
        self.score = score;
    }
}

fn main() {
    App::new(
        AppConfig::new("SkyEngine - yakui", WINDOW_W, WINDOW_H)
            .with_vsync(false)
            .with_resizable(true),
        World::new(),
    )
    .with_render_pipeline(
        RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build(),
    )
    .run(YakuiDemo::default());
}
