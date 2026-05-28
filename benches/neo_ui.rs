use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sky_engine::ui::neo::{
    widgets, Align, AnimProperty, Color, HorizontalAlign, MotionPreset, PointerEvent, Runtime,
    Size, State, VerticalAlign,
};

#[path = "../examples/ui/neo/control_center/actions.rs"]
mod actions;
#[allow(dead_code)]
#[path = "../examples/ui/neo/control_center/locale.rs"]
mod locale;
#[path = "../examples/ui/neo/control_center/model.rs"]
mod model;
#[path = "../examples/ui/neo/control_center/theme.rs"]
mod theme;
#[path = "../examples/ui/neo/control_center/view/mod.rs"]
mod view;

const ITEM_COUNT: usize = 10_000;
const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 420.0;
const ITEM_HEIGHT: f32 = 32.0;
const GAP: f32 = 4.0;
const OFFSET: f32 = 96_000.0;
const CONTROL_ROWS: usize = 96;
const CONTROL_CENTER_WIDTH: f32 = 1366.0;
const CONTROL_CENTER_HEIGHT: f32 = 768.0;

fn list_content_height() -> f32 {
    ITEM_COUNT as f32 * ITEM_HEIGHT + ITEM_COUNT.saturating_sub(1) as f32 * GAP
}

fn compose_scroll_y(runtime: &mut Runtime) {
    runtime.compose(WIDTH, HEIGHT, |ui, _| {
        ui.scroll_y("list")
            .size(WIDTH, HEIGHT)
            .content_height(list_content_height())
            .offset(OFFSET)
            .gap(GAP)
            .scrollbar_gap(10.0)
            .content(|ui| {
                for index in 0..ITEM_COUNT {
                    render_row(ui, index, ITEM_HEIGHT);
                }
            });
    });
}

fn compose_virtual_list(runtime: &mut Runtime) {
    runtime.compose(WIDTH, HEIGHT, |ui, _| {
        widgets::virtual_list(ui, "list")
            .size(WIDTH, HEIGHT)
            .item_count(ITEM_COUNT)
            .item_height(ITEM_HEIGHT)
            .gap(GAP)
            .offset(OFFSET)
            .overscan_items(3)
            .scrollbar_gap(10.0)
            .content(|ui, item| {
                render_row(ui, item.index, item.height);
            });
    });
}

fn render_row(ui: &mut sky_engine::ui::neo::Ui, index: usize, height: f32) {
    let id = format!("row.{index}");
    ui.stack(id.clone())
        .size(Size::fill(), height)
        .content(|ui| {
            ui.rect(format!("{id}.bg"))
                .size(Size::fill(), height)
                .color(if index % 2 == 0 {
                    Color::new(0.10, 0.12, 0.15, 1.0)
                } else {
                    Color::new(0.12, 0.14, 0.18, 1.0)
                })
                .build();
            ui.text(format!("{id}.label"))
                .size(Size::fill(), height)
                .text(format!("Item {index:05}"))
                .font_size(14.0)
                .line_height(height)
                .horizontal_align(HorizontalAlign::Left)
                .vertical_align(VerticalAlign::Center)
                .color(Color::new(0.88, 0.91, 0.96, 1.0))
                .build();
        });
}

fn compose_common_controls(runtime: &mut Runtime) {
    runtime.compose(1280.0, 720.0, |ui, screen| {
        ui.column("controls")
            .size(screen.width, screen.height)
            .gap(6.0)
            .padding(10.0)
            .content(|ui| {
                widgets::tabs(ui, "tabs")
                    .size(420.0, 34.0)
                    .items(["Overview", "Tasks", "Settings", "Logs"])
                    .selected(1)
                    .build();

                for row in 0..CONTROL_ROWS {
                    ui.row(format!("row.{row}"))
                        .size(Size::fill(), 42.0)
                        .gap(8.0)
                        .align_items(Align::Center)
                        .content(|ui| {
                            ui.text(format!("row.{row}.label"))
                                .size(86.0, 32.0)
                                .text(format!("Item {row:03}"))
                                .font_size(14.0)
                                .line_height(32.0)
                                .color(Color::new(0.76, 0.82, 0.90, 1.0))
                                .vertical_align(VerticalAlign::Center)
                                .build();

                            widgets::button(ui, format!("row.{row}.button"))
                                .size(104.0, 32.0)
                                .text("Apply")
                                .build();
                            widgets::checkbox(ui, format!("row.{row}.check"))
                                .size(118.0, 28.0)
                                .checked(row % 2 == 0)
                                .text("Enabled")
                                .build();
                            widgets::radio(ui, format!("row.{row}.radio"))
                                .size(104.0, 28.0)
                                .selected(row % 3 == 0)
                                .text("Primary")
                                .build();
                            widgets::switch(ui, format!("row.{row}.switch"))
                                .size(110.0, 28.0)
                                .checked(row % 2 == 1)
                                .label("Live")
                                .build();
                            widgets::slider(ui, format!("row.{row}.slider"))
                                .size(128.0, 20.0)
                                .value((row % 100) as f32 / 100.0)
                                .build();
                            widgets::input(ui, format!("row.{row}.input"))
                                .size(160.0, 32.0)
                                .text(format!("value-{row:03}"))
                                .build();
                            widgets::segmented(ui, format!("row.{row}.segmented"))
                                .size(180.0, 30.0)
                                .items(["Low", "Med", "High"])
                                .selected((row % 3) as i32)
                                .build();
                            widgets::dropdown(ui, format!("row.{row}.dropdown"))
                                .size(132.0, 32.0)
                                .items(["Idle", "Running", "Done"])
                                .selected((row % 3) as i32)
                                .open(false)
                                .build();
                        });
                }
            });
    });
}

fn control_center_state(page: model::Page) -> State<model::AppModel> {
    let mut model = model::AppModel::default();
    model.page = page;
    State::new(model)
}

fn compose_control_center(runtime: &mut Runtime, state: &State<model::AppModel>) {
    let runtime_info = view::RuntimeInfo {
        uptime_seconds: 42.0,
        frame_count: 2_400,
    };

    runtime.compose(CONTROL_CENTER_WIDTH, CONTROL_CENTER_HEIGHT, |ui, screen| {
        state.read(|model| view::render(ui, screen, state, model, runtime_info));
    });
}

fn compose_motion_scene(runtime: &mut Runtime, expanded: bool) {
    let card_x = if expanded { 382.0 } else { 96.0 };
    let card_y = if expanded { 106.0 } else { 220.0 };
    let card_w = if expanded { 300.0 } else { 210.0 };
    let card_h = if expanded { 176.0 } else { 112.0 };
    let card_scale = if expanded { 1.0 } else { 0.94 };
    let accent_opacity = if expanded { 1.0 } else { 0.32 };

    runtime.compose(800.0, 480.0, |ui, screen| {
        ui.rect("background")
            .size(screen.width, screen.height)
            .color(Color::new(0.05, 0.06, 0.08, 1.0))
            .build();

        ui.stack("card")
            .position(card_x, card_y)
            .size(card_w, card_h)
            .scale(card_scale)
            .motion(MotionPreset::Smooth)
            .animate(AnimProperty::FRAME | AnimProperty::TRANSFORM)
            .content(|ui| {
                ui.rect("card.surface")
                    .size(Size::fill(), Size::fill())
                    .color(Color::new(0.13, 0.15, 0.19, 1.0))
                    .radius(22.0)
                    .border(1.0, Color::new(0.30, 0.35, 0.44, 1.0))
                    .shadow(28.0, 0.0, 14.0, Color::new(0.0, 0.0, 0.0, 0.28))
                    .motion(MotionPreset::Smooth)
                    .animate(AnimProperty::FRAME | AnimProperty::BORDER | AnimProperty::SHADOW)
                    .build();

                ui.rect("card.accent")
                    .x(18.0)
                    .y(18.0)
                    .size(if expanded { 96.0 } else { 48.0 }, 8.0)
                    .color(Color::new(0.42, 0.70, 1.0, 1.0))
                    .radius(4.0)
                    .opacity(accent_opacity)
                    .motion(MotionPreset::Responsive)
                    .animate(AnimProperty::FRAME | AnimProperty::OPACITY)
                    .build();

                ui.stack("card.button.slot")
                    .position(18.0, card_h - 52.0)
                    .size(128.0, 34.0)
                    .content(|ui| {
                        widgets::button(ui, "card.button")
                            .size(128.0, 34.0)
                            .text(if expanded { "Collapse" } else { "Expand" })
                            .build();
                    });
            });
    });
}

fn bench_neo_ui_lists(c: &mut Criterion) {
    let mut group = c.benchmark_group("neo_ui_list");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("compose_scroll_y_10k_full_tree", |b| {
        let mut runtime = Runtime::new("bench");
        b.iter(|| {
            compose_scroll_y(&mut runtime);
            black_box(runtime.roots().len());
        });
    });

    group.bench_function("compose_virtual_list_10k_visible_window", |b| {
        let mut runtime = Runtime::new("bench");
        b.iter(|| {
            compose_virtual_list(&mut runtime);
            black_box(runtime.roots().len());
        });
    });

    group.bench_function("draw_virtual_list_10k_visible_window", |b| {
        let mut runtime = Runtime::new("bench");
        compose_virtual_list(&mut runtime);
        b.iter(|| {
            black_box(runtime.draw_list());
        });
    });

    group.finish();
}

fn bench_neo_ui_common_controls(c: &mut Criterion) {
    let mut group = c.benchmark_group("neo_ui_common_controls");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("compose_96_rows", |b| {
        let mut runtime = Runtime::new("bench");
        b.iter(|| {
            compose_common_controls(&mut runtime);
            black_box(runtime.roots().len());
        });
    });

    group.bench_function("draw_96_rows", |b| {
        let mut runtime = Runtime::new("bench");
        compose_common_controls(&mut runtime);
        b.iter(|| {
            black_box(runtime.draw_list());
        });
    });

    group.bench_function("hover_hit_test_96_rows", |b| {
        let mut runtime = Runtime::new("bench");
        compose_common_controls(&mut runtime);
        b.iter(|| {
            black_box(runtime.update_pointer(PointerEvent::at(60.0, 68.0)));
        });
    });

    group.finish();
}

fn bench_neo_ui_control_center(c: &mut Criterion) {
    let mut group = c.benchmark_group("neo_ui_control_center_real");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("compose_overview", |b| {
        let state = control_center_state(model::Page::Overview);
        let mut runtime = Runtime::new("bench");
        b.iter(|| {
            compose_control_center(&mut runtime, &state);
            black_box(runtime.roots().len());
        });
    });

    group.bench_function("compose_tasks", |b| {
        let state = control_center_state(model::Page::Tasks);
        let mut runtime = Runtime::new("bench");
        b.iter(|| {
            compose_control_center(&mut runtime, &state);
            black_box(runtime.roots().len());
        });
    });

    group.bench_function("compose_settings", |b| {
        let state = control_center_state(model::Page::Settings);
        let mut runtime = Runtime::new("bench");
        b.iter(|| {
            compose_control_center(&mut runtime, &state);
            black_box(runtime.roots().len());
        });
    });

    group.bench_function("draw_overview", |b| {
        let state = control_center_state(model::Page::Overview);
        let mut runtime = Runtime::new("bench");
        compose_control_center(&mut runtime, &state);
        b.iter(|| {
            black_box(runtime.draw_list());
        });
    });

    group.bench_function("hover_hit_test_overview", |b| {
        let state = control_center_state(model::Page::Overview);
        let mut runtime = Runtime::new("bench");
        compose_control_center(&mut runtime, &state);
        b.iter(|| {
            black_box(runtime.update_pointer(PointerEvent::at(1120.0, 118.0)));
        });
    });

    group.finish();
}

fn bench_neo_ui_motion(c: &mut Criterion) {
    let mut group = c.benchmark_group("neo_ui_motion");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("spring_retarget_tick_and_draw", |b| {
        let mut runtime = Runtime::new("bench");
        let mut expanded = false;
        let mut frame = 0_u32;
        compose_motion_scene(&mut runtime, expanded);
        runtime.tick_animations(0.0);

        b.iter(|| {
            if frame % 12 == 0 {
                expanded = !expanded;
                compose_motion_scene(&mut runtime, expanded);
            }
            frame = frame.wrapping_add(1);
            black_box(runtime.tick_animations(1.0 / 120.0));
            black_box(runtime.draw_list());
        });
    });

    group.finish();
}

criterion_group!(
    neo_ui_benches,
    bench_neo_ui_lists,
    bench_neo_ui_common_controls,
    bench_neo_ui_control_center,
    bench_neo_ui_motion
);
criterion_main!(neo_ui_benches);
