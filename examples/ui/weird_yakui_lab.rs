//! Visual stress lab for yakui.
//!
//! ```bash
//! cargo run --example weird_yakui_lab --features yakui-ui --release
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
use sky_engine::ecs::World;
use sky_engine::plugin::Plugin;
use sky_engine::render::{
    CameraMarker, Color as RenderColor, MainCamera, Projection, RenderPipelineAsset,
    RenderSettings, SpriteFeature, Transform, TransparentPhase,
};
use sky_engine::ui::YakuiUiPlugin;

const WINDOW_W: u32 = 1180;
const WINDOW_H: u32 = 760;

#[derive(Default)]
struct WeirdYakuiLab {
    time: f32,
    clicks: u32,
    mode: u32,
    wobble: f32,
    chaos: f32,
    alarm: f32,
    glass: bool,
    lock: bool,
    reveal: bool,
}

impl AppState for WeirdYakuiLab {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: RenderColor::rgb(0.018, 0.022, 0.03),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(760.0),
            MainCamera,
        ));
        YakuiUiPlugin.install(ctx.world).unwrap();

        self.wobble = 0.42;
        self.chaos = 0.68;
        self.alarm = 0.27;
        self.glass = true;
        self.lock = false;
        self.reveal = true;
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.ui().update();
        self.time += ctx.dt();
        self.draw_ui(ctx);
        ctx.render();
        ctx.ui().render_overlays();
    }
}

impl WeirdYakuiLab {
    fn draw_ui(&mut self, ctx: &mut FrameContext<'_>) {
        let time = self.time;
        let mut clicks = self.clicks;
        let mut mode = self.mode;
        let mut wobble = self.wobble;
        let mut chaos = self.chaos;
        let mut alarm = self.alarm;
        let mut glass = self.glass;
        let mut lock = self.lock;
        let mut reveal = self.reveal;

        let pointer_owned = ctx.ui().wants_pointer();
        let keyboard_owned = ctx.ui().wants_keyboard();

        sky_engine::ui::yakui::run(ctx, |backend| {
            backend.run(|| {
                use yakui::{
                    colors, Alignment, Color as YColor, Constraints, Dim2, Pivot, Vec2,
                };

                let pulse = time.sin() * 0.5 + 0.5;
                let scan = (time * (0.18 + chaos * 0.9)).fract();

                yakui::pad(yakui::widgets::Pad::all(22.0), || {
                    yakui::max_width(1120.0, || {
                        yakui::opaque(|| {
                            yakui::colored_box_container(colors::BACKGROUND_2, || {
                                yakui::pad(yakui::widgets::Pad::all(18.0), || {
                                    yakui::column(|| {
                                        yakui::colored_box_container(YColor::rgba(12, 18, 30, 226), || {
                                            yakui::pad(yakui::widgets::Pad::all(14.0), || {
                                                yakui::column(|| {
                                                    yakui::row(|| {
                                                        yakui::text(28.0, "WEIRD YAKUI LAB");
                                                        yakui::expanded(|| {
                                                            yakui::colored_box(
                                                                YColor::rgba(24, 36, 54, 220),
                                                                Vec2::new(1.0, 38.0),
                                                            );
                                                        });
                                                        yakui::colored_box(
                                                            mix(
                                                                YColor::rgba(72, 216, 154, 255),
                                                                YColor::rgba(255, 118, 94, 255),
                                                                alarm,
                                                            ),
                                                            Vec2::new(250.0 * scan.max(0.08), 18.0),
                                                        );
                                                        if yakui::button("Mode").clicked {
                                                            mode = (mode + 1) % 4;
                                                            clicks = clicks.wrapping_add(1);
                                                        }
                                                        if yakui::button("Panic").clicked {
                                                            alarm = (alarm + 0.22).fract();
                                                            clicks = clicks.wrapping_add(1);
                                                        }
                                                        if yakui::button("Zero").clicked {
                                                            wobble = 0.0;
                                                            chaos = 0.0;
                                                            alarm = 0.0;
                                                            clicks = clicks.wrapping_add(1);
                                                        }
                                                    });
                                                    yakui::label(format!(
                                                        "mode {}   clicks {}   wobble {:>3}%   chaos {:>3}%   alarm {:>3}%   glass {}   lock {}   reveal {}",
                                                        mode,
                                                        clicks,
                                                        (wobble * 100.0).round() as i32,
                                                        (chaos * 100.0).round() as i32,
                                                        (alarm * 100.0).round() as i32,
                                                        on_off(glass),
                                                        on_off(lock),
                                                        on_off(reveal),
                                                    ));
                                                });
                                            });
                                        });

                                        yakui::divider(colors::BACKGROUND_3, 12.0, 2.0);

                                        yakui::row(|| {
                                            fixed_panel(Vec2::new(330.0, 470.0), YColor::rgba(24, 30, 42, 232), || {
                                                yakui::pad(yakui::widgets::Pad::all(14.0), || {
                                                    yakui::column(|| {
                                                        yakui::text(21.0, "Control Cabinet");
                                                        yakui::constrained(
                                                            Constraints::tight(Vec2::new(300.0, 390.0)),
                                                            || {
                                                                yakui::scroll_vertical(|| {
                                                                    yakui::column(|| {
                                                                        slider_row(
                                                                            "Wobble",
                                                                            wobble,
                                                                            YColor::CORNFLOWER_BLUE,
                                                                            &mut wobble,
                                                                        );
                                                                        slider_row(
                                                                            "Chaos",
                                                                            chaos,
                                                                            YColor::hex(0x48d89a),
                                                                            &mut chaos,
                                                                        );
                                                                        slider_row(
                                                                            "Alarm",
                                                                            alarm,
                                                                            YColor::hex(0xffaa50),
                                                                            &mut alarm,
                                                                        );

                                                                        glass = checkbox_row("Glass Tint", glass);
                                                                        lock = checkbox_row("Disable Odd Panel", lock);
                                                                        reveal = checkbox_row("Reveal Secret", reveal);

                                                                        yakui::label("scroll pocket");
                                                                        for i in 0..12 {
                                                                            yakui::row(|| {
                                                                                yakui::label(format!("dense label row {:02}", i + 1));
                                                                                yakui::expanded(|| {
                                                                                    yakui::colored_box(
                                                                                        mix(
                                                                                            YColor::rgba(70, 205, 145, 190),
                                                                                            YColor::rgba(238, 94, 128, 190),
                                                                                            i as f32 / 11.0,
                                                                                        ),
                                                                                        Vec2::new(
                                                                                            70.0 + 13.0 * i as f32,
                                                                                            12.0,
                                                                                        ),
                                                                                    );
                                                                                });
                                                                            });
                                                                        }
                                                                    });
                                                                });
                                                            },
                                                        );
                                                    });
                                                });
                                            });

                                            fixed_panel(Vec2::new(360.0, 470.0), YColor::rgba(18, 24, 36, 226), || {
                                                yakui::pad(yakui::widgets::Pad::all(14.0), || {
                                                    yakui::column(|| {
                                                        yakui::text(21.0, "Stacking Context Trap");
                                                        yakui::stack(|| {
                                                            yakui::colored_box(
                                                                YColor::rgba(62, 78, 130, 224),
                                                                Vec2::new(210.0, 140.0),
                                                            );
                                                            yakui::offset(
                                                                Vec2::new(96.0 + time.cos() * 24.0, 62.0),
                                                                || {
                                                                    yakui::colored_box(
                                                                        YColor::rgba(255, 194, 86, 236),
                                                                        Vec2::new(132.0, 48.0),
                                                                    );
                                                                },
                                                            );
                                                            yakui::offset(Vec2::new(128.0, 70.0), || {
                                                                yakui::colored_box(
                                                                    YColor::rgba(30, 176, 152, 220),
                                                                    Vec2::new(190.0, 132.0),
                                                                );
                                                            });
                                                            yakui::offset(
                                                                Vec2::new(
                                                                    66.0 + time.cos() * (18.0 + wobble * 28.0),
                                                                    116.0 + time.sin() * 24.0,
                                                                ),
                                                                || {
                                                                    yakui::colored_box_container(
                                                                        YColor::rgba(238, 94, 128, 236),
                                                                        || {
                                                                            yakui::pad(yakui::widgets::Pad::all(8.0), || {
                                                                                yakui::label("wobble chip");
                                                                            });
                                                                        },
                                                                    );
                                                                },
                                                            );
                                                        });
                                                        yakui::divider(colors::BACKGROUND_3, 10.0, 2.0);
                                                        yakui::text(19.0, "Fill Weights And Bars");
                                                        weighted_lane(
                                                            YColor::rgba(54, 78, 124, 238),
                                                            YColor::rgba(58, 138, 112, 238),
                                                            YColor::rgba(148, 92, 126, 238),
                                                        );
                                                        meter_row("blue fill", (pulse * 0.55 + wobble * 0.45).fract(), YColor::hex(0x54a4f8));
                                                        meter_row("green fill", (scan + chaos * 0.25).fract(), YColor::hex(0x48d89a));
                                                        meter_row("alarm fill", alarm.max((1.0 - pulse) * 0.35), YColor::hex(0xffaa50));
                                                    });
                                                });
                                            });

                                            fixed_panel(Vec2::new(370.0, 470.0), YColor::rgba(26, 20, 34, 224), || {
                                                yakui::pad(yakui::widgets::Pad::all(14.0), || {
                                                    yakui::column(|| {
                                                        yakui::text(21.0, "Oddities Row");
                                                        yakui::row(|| {
                                                            yakui::colored_box_container(
                                                                if lock {
                                                                    YColor::rgba(56, 58, 64, 178)
                                                                } else if glass {
                                                                    YColor::rgba(42, 82, 98, 184)
                                                                } else {
                                                                    YColor::rgba(64, 74, 92, 236)
                                                                },
                                                                || {
                                                                    yakui::pad(yakui::widgets::Pad::all(8.0), || {
                                                                        yakui::column(|| {
                                                                            yakui::label(if lock {
                                                                                "Locked Panel"
                                                                            } else {
                                                                                "Disabled Panel"
                                                                            });
                                                                            if !lock && yakui::button("May Disable").clicked {
                                                                                clicks = clicks.wrapping_add(1);
                                                                            }
                                                                        });
                                                                    });
                                                                },
                                                            );

                                                            if reveal {
                                                                yakui::colored_box_container(
                                                                    YColor::rgba(96, 54, 116, 218),
                                                                    || {
                                                                        yakui::pad(yakui::widgets::Pad::all(8.0), || {
                                                                            yakui::column(|| {
                                                                                yakui::label("Secret");
                                                                                if yakui::button("Ghost Hit").clicked {
                                                                                    clicks = clicks.wrapping_add(1);
                                                                                }
                                                                            });
                                                                        });
                                                                    },
                                                                );
                                                            }
                                                        });

                                                        yakui::text(19.0, "Anchor Cards");
                                                        yakui::stack(|| {
                                                            yakui::colored_box(
                                                                YColor::rgba(16, 26, 30, 218),
                                                                Vec2::new(320.0, 112.0),
                                                            );
                                                            anchor_chip("TL", Vec2::new(18.0, 30.0), YColor::rgba(68, 96, 170, 232));
                                                            anchor_chip("TR", Vec2::new(244.0, 30.0), YColor::rgba(62, 156, 122, 232));
                                                            anchor_chip("BL", Vec2::new(18.0, 74.0), YColor::rgba(174, 94, 118, 232));
                                                            anchor_chip("BR", Vec2::new(244.0, 74.0), YColor::rgba(176, 136, 72, 232));
                                                        });

                                                        yakui::divider(colors::BACKGROUND_3, 10.0, 2.0);
                                                        yakui::label(format!(
                                                            "pointer {}   keyboard {}",
                                                            on_off(pointer_owned),
                                                            on_off(keyboard_owned),
                                                        ));
                                                        yakui::label(format!(
                                                            "ticker {:>02}   pulse {:>3}%   scan {:>3}%",
                                                            ((time * 8.0) as i32).rem_euclid(10),
                                                            (pulse * 100.0).round() as i32,
                                                            (scan * 100.0).round() as i32,
                                                        ));
                                                    });
                                                });
                                            });
                                        });

                                        yakui::reflow(
                                            Alignment::BOTTOM_RIGHT,
                                            Pivot::BOTTOM_RIGHT,
                                            Dim2::pixels(0.0, 0.0),
                                            || {
                                                yakui::label(format!(
                                                    "hover pressure {}",
                                                    if pointer_owned { "on" } else { "off" }
                                                ));
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

        self.mode = mode;
        self.wobble = wobble;
        self.chaos = chaos;
        self.alarm = alarm;
        self.glass = glass;
        self.lock = lock;
        self.reveal = reveal;
        self.clicks = clicks;
    }
}

fn slider_row(label: &str, current: f32, color: yakui::Color, out: &mut f32) {
    yakui::row(|| {
        yakui::label(label.to_string());
        if let Some(value) = yakui::slider(current as f64, 0.0, 1.0).value {
            *out = value as f32;
        } else {
            *out = current;
        }
        yakui::colored_box(color, yakui::Vec2::new(180.0 * (*out), 10.0));
    });
}

fn checkbox_row(label: &'static str, current: bool) -> bool {
    let mut value = current;
    yakui::row(|| {
        value = yakui::checkbox(value).checked;
        yakui::label(label);
    });
    value
}

fn fixed_panel(size: yakui::Vec2, color: yakui::Color, children: impl FnOnce()) {
    yakui::constrained(yakui::Constraints::tight(size), || {
        yakui::colored_box_container(color, children);
    });
}

fn weighted_lane(a: yakui::Color, b: yakui::Color, c: yakui::Color) {
    yakui::row(|| {
        yakui::colored_box(a, yakui::Vec2::new(48.0, 28.0));
        yakui::colored_box(b, yakui::Vec2::new(96.0, 28.0));
        yakui::colored_box(c, yakui::Vec2::new(144.0, 28.0));
    });
}

fn meter_row(label: &'static str, value: f32, color: yakui::Color) {
    let value = value.clamp(0.0, 1.0);
    yakui::row(|| {
        yakui::label(label);
        yakui::stack(|| {
            yakui::colored_box(yakui::colors::BACKGROUND_3, yakui::Vec2::new(220.0, 14.0));
            yakui::colored_box(color, yakui::Vec2::new(220.0 * value, 14.0));
        });
    });
}

fn anchor_chip(label: &'static str, offset: yakui::Vec2, color: yakui::Color) {
    yakui::offset(offset, || {
        yakui::colored_box_container(color, || {
            yakui::pad(yakui::widgets::Pad::balanced(14.0, 4.0), || {
                yakui::label(label);
            });
        });
    });
}

fn mix(a: yakui::Color, b: yakui::Color, t: f32) -> yakui::Color {
    let t = t.clamp(0.0, 1.0);
    yakui::Color::rgba(
        lerp_u8(a.r, b.r, t),
        lerp_u8(a.g, b.g, t),
        lerp_u8(a.b, b.b, t),
        lerp_u8(a.a, b.a, t),
    )
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn main() {
    App::new(
        AppConfig::new("SkyEngine - Weird Yakui Lab", WINDOW_W, WINDOW_H)
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
    .run(WeirdYakuiLab::default());
}
