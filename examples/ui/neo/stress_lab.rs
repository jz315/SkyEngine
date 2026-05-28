//! Visual stress lab for the EUI-NEO-style UI backend.
//!
//! ```bash
//! cargo run --example ui_neo_stress_lab --features ui-neo --release
//! ```

use std::time::Instant;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteFeature,
    Transform, TransparentPhase,
};
use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{
    Align, AnimProperty, Color, Ease, HorizontalAlign, NeoUiBackend, PointerEvent, Signal, Size,
    State, Transition, Ui,
};

const WINDOW_W: u32 = 1180;
const WINDOW_H: u32 = 760;
const OUTER_PAD: f32 = 24.0;
const HEADER_H: f32 = 94.0;
const SIDE_W: f32 = 316.0;
const RIGHT_W: f32 = 342.0;
const PANEL_PAD: f32 = 18.0;
const PANEL_SCROLL_INSET: f32 = 10.0;
const SCROLLBAR_RESERVE: f32 = 18.0;
const CONTROL_FIELD_W: f32 =
    SIDE_W - PANEL_SCROLL_INSET * 2.0 - PANEL_PAD * 2.0 - SCROLLBAR_RESERVE;
const RIGHT_FIELD_W: f32 = RIGHT_W - PANEL_SCROLL_INSET * 2.0 - PANEL_PAD * 2.0 - SCROLLBAR_RESERVE;
const PERF_SAMPLE_COUNT: usize = 120;
const PERF_WARMUP_FRAMES: u64 = 30;
const FRAME_BUDGET_MS: f32 = 16.7;
const STUTTER_MS: f32 = 33.3;

#[derive(Debug)]
struct NeoUiStressLab {
    time: f32,
    title_timer: f32,
    state: State<LabState>,
    frame_monitor: FrameMonitor,
    screenshot: ScreenshotProbe,
    frame_index: u32,
    auto_chart_clicked: bool,
    layout_dumped: bool,
    draw_dumped: bool,
}

#[derive(Debug)]
struct LabState {
    clicks: u32,
    mode: i32,
    wobble: f32,
    density: f32,
    alarm: f32,
    glass: bool,
    lock: bool,
    reveal: bool,
    tab: i32,
    segment: i32,
    radio: i32,
    dropdown_open: bool,
    dropdown_selected: i32,
    dialog_open: bool,
    toast_visible: bool,
    context_menu_open: bool,
    context_menu_position: [f32; 2],
    input_text: String,
    control_scroll: f32,
    signal_scroll: f32,
    interaction_scroll: f32,
}

impl Default for NeoUiStressLab {
    fn default() -> Self {
        Self {
            time: 0.0,
            title_timer: 1.0,
            state: State::new(LabState::default()),
            frame_monitor: FrameMonitor::default(),
            screenshot: ScreenshotProbe::default(),
            frame_index: 0,
            auto_chart_clicked: false,
            layout_dumped: false,
            draw_dumped: false,
        }
    }
}

impl Default for LabState {
    fn default() -> Self {
        Self {
            clicks: 0,
            mode: 0,
            wobble: 0.42,
            density: 0.68,
            alarm: 0.27,
            glass: true,
            lock: false,
            reveal: true,
            tab: 0,
            segment: 1,
            radio: 0,
            dropdown_open: false,
            dropdown_selected: 1,
            dialog_open: env_flag("SKY_NEO_LAB_DIALOG_OPEN"),
            toast_visible: env_flag("SKY_NEO_LAB_TOAST_VISIBLE"),
            context_menu_open: env_flag("SKY_NEO_LAB_CONTEXT_OPEN"),
            context_menu_position: [780.0, 300.0],
            input_text: "EUI".to_string(),
            control_scroll: 0.0,
            signal_scroll: 0.0,
            interaction_scroll: 0.0,
        }
    }
}

impl AppState for NeoUiStressLab {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: sky_engine::render::Color::new(0.020, 0.024, 0.034, 1.0),
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
        let perf = self.frame_monitor.update(ctx.dt());
        self.time += ctx.dt();

        let compose_start = Instant::now();
        self.draw_ui(ctx, &perf);
        let compose_ms = elapsed_ms(compose_start);

        let (mode, clicks) = self.state.read(|state| (state.mode, state.clicks));
        self.title_timer += ctx.dt();
        if self.title_timer >= 0.25 || perf.trigger_active {
            ctx.set_title(&format!(
                "SkyEngine - Neo UI Stress Lab | mode {mode} | clicks {clicks} | {:.0} FPS | {:.1} ms",
                perf.fps, perf.frame_ms
            ));
            self.title_timer = 0.0;
        }

        // The lab is a fullscreen UI workload. The UI background is opaque and
        // covers the whole surface, so running an empty scene render first only
        // measures app overhead rather than Neo UI throughput.
        let render_ms = 0.0;

        let overlay_start = Instant::now();
        ctx.ui().render_overlays();
        let overlay_ms = elapsed_ms(overlay_start);

        self.frame_monitor
            .record_phases(compose_ms, render_ms, overlay_ms);
        self.screenshot.update(ctx);
        let signature = self.state.read(LabSignature::from_state);
        self.frame_monitor.observe_signature(signature);
        ctx.request_redraw();
        self.frame_index = self.frame_index.saturating_add(1);
    }
}

impl NeoUiStressLab {
    fn draw_ui(&mut self, ctx: &mut FrameContext<'_>, perf: &PerfSnapshot) {
        let time = self.time;
        let state_store = self.state.clone();
        let pointer_owned = ctx.ui().wants_pointer();
        let keyboard_owned = ctx.ui().wants_keyboard();
        let compose_state = state_store.clone();

        sky_engine::ui::neo::compose_state(ctx, &state_store, move |ui, screen| {
            draw_lab(
                ui,
                screen.width,
                screen.height,
                time,
                &compose_state,
                perf,
                pointer_owned,
                keyboard_owned,
            );
        });

        if env_flag("SKY_NEO_LAB_DUMP_LAYOUT") && !self.layout_dumped {
            let mut ui = ctx.ui();
            ui.with_backend_mut::<NeoUiBackend, _>(|backend| {
                dump_runtime_layout("stress_lab", backend.runtime());
            });
            self.layout_dumped = true;
        }

        if env_flag("SKY_NEO_LAB_DUMP_DRAW") && !self.draw_dumped {
            let mut ui = ctx.ui();
            ui.with_backend_mut::<NeoUiBackend, _>(|backend| {
                dump_runtime_draw("stress_lab", backend.runtime());
            });
            self.draw_dumped = true;
        }

        if env_flag("SKY_NEO_LAB_TRACE_CHART") {
            let tab = self.state.read(|state| state.tab);
            if tab.rem_euclid(3) == 1 {
                let mut ui = ctx.ui();
                ui.with_backend_mut::<NeoUiBackend, _>(|backend| {
                    trace_chart_runtime("stress_lab", tab, time, backend.runtime());
                });
            }
        }

        self.maybe_auto_click_chart(ctx);
    }

    fn maybe_auto_click_chart(&mut self, ctx: &mut FrameContext<'_>) {
        let Some(click_frame) = env_u32("SKY_NEO_LAB_AUTO_CLICK_CHART_FRAME") else {
            return;
        };
        if self.auto_chart_clicked || self.frame_index < click_frame {
            return;
        }

        let mut clicked = false;
        let mut missing = false;
        let mut ui = ctx.ui();
        ui.with_backend_mut::<NeoUiBackend, _>(|backend| {
            let target_frame = backend
                .runtime()
                .find("signals.tabs.hit.1")
                .map(|element| element.frame);
            if let Some(frame) = target_frame {
                let x = frame.x + frame.width * 0.5;
                let y = frame.y + frame.height * 0.5;
                let pressed = backend.runtime_mut().update_pointer(PointerEvent::pressed_at(x, y));
                let released = backend
                    .runtime_mut()
                    .update_pointer(PointerEvent::released_at(x, y));
                eprintln!(
                    "[neo auto click] target=signals.tabs.hit.1 frame=({:.2},{:.2},{:.2},{:.2}) point=({:.2},{:.2}) pressed_changed={} released_changed={} needs_compose={}",
                    frame.x,
                    frame.y,
                    frame.width,
                    frame.height,
                    x,
                    y,
                    pressed,
                    released,
                    backend.runtime().needs_compose()
                );
                clicked = true;
            } else {
                missing = true;
            }
        });

        if clicked {
            self.auto_chart_clicked = true;
        } else if missing {
            eprintln!("[neo auto click] missing target=signals.tabs.hit.1");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LabSignature {
    clicks: u32,
    mode: i32,
    glass: bool,
    lock: bool,
    reveal: bool,
    tab: i32,
    segment: i32,
    radio: i32,
    dropdown_open: bool,
    dropdown_selected: i32,
    dialog_open: bool,
    toast_visible: bool,
    context_menu_open: bool,
    alarm_bucket: i32,
}

impl LabSignature {
    fn from_state(value: &LabState) -> Self {
        Self {
            clicks: value.clicks,
            mode: value.mode,
            glass: value.glass,
            lock: value.lock,
            reveal: value.reveal,
            tab: value.tab,
            segment: value.segment,
            radio: value.radio,
            dropdown_open: value.dropdown_open,
            dropdown_selected: value.dropdown_selected,
            dialog_open: value.dialog_open,
            toast_visible: value.toast_visible,
            context_menu_open: value.context_menu_open,
            alarm_bucket: (value.alarm * 100.0).round() as i32,
        }
    }
}

#[derive(Debug, Clone)]
struct PerfSnapshot {
    frame: u64,
    fps: f32,
    frame_ms: f32,
    avg_ms: f32,
    lifetime_worst_ms: f32,
    compose_ms: f32,
    render_ms: f32,
    overlay_ms: f32,
    rest_ms: f32,
    stutter_count: u32,
    trigger_stutter_count: u32,
    trigger_active: bool,
    trigger_label: String,
    trigger_age_ms: f32,
}

#[derive(Debug)]
struct FrameMonitor {
    last_instant: Option<Instant>,
    frame: u64,
    samples: [f32; PERF_SAMPLE_COUNT],
    sample_count: usize,
    sample_index: usize,
    sample_sum_ms: f32,
    lifetime_worst_ms: f32,
    compose_ms: f32,
    render_ms: f32,
    overlay_ms: f32,
    stutter_count: u32,
    trigger_stutter_count: u32,
    trigger_timer: f32,
    trigger_age: f32,
    trigger_label: String,
    signature: Option<LabSignature>,
}

impl Default for FrameMonitor {
    fn default() -> Self {
        Self {
            last_instant: None,
            frame: 0,
            samples: [0.0; PERF_SAMPLE_COUNT],
            sample_count: 0,
            sample_index: 0,
            sample_sum_ms: 0.0,
            lifetime_worst_ms: 0.0,
            compose_ms: 0.0,
            render_ms: 0.0,
            overlay_ms: 0.0,
            stutter_count: 0,
            trigger_stutter_count: 0,
            trigger_timer: 0.0,
            trigger_age: 999.0,
            trigger_label: "idle".to_string(),
            signature: None,
        }
    }
}

impl FrameMonitor {
    fn update(&mut self, app_dt: f32) -> PerfSnapshot {
        let now = Instant::now();
        let wall_dt = self
            .last_instant
            .map(|last| now.saturating_duration_since(last).as_secs_f32())
            .unwrap_or(app_dt.max(1.0 / 60.0));
        self.last_instant = Some(now);
        self.frame = self.frame.saturating_add(1);
        let warming_up = self.frame <= PERF_WARMUP_FRAMES;

        if self.trigger_timer > 0.0 {
            self.trigger_timer = (self.trigger_timer - wall_dt).max(0.0);
            self.trigger_age += wall_dt;
        }

        let frame_ms = if warming_up {
            (app_dt.max(1.0 / 60.0) * 1000.0).clamp(0.0, 250.0)
        } else {
            (wall_dt * 1000.0).clamp(0.0, 250.0)
        };
        if self.sample_count < PERF_SAMPLE_COUNT {
            self.sample_count += 1;
        } else {
            self.sample_sum_ms -= self.samples[self.sample_index];
        }
        self.samples[self.sample_index] = frame_ms;
        self.sample_sum_ms += frame_ms;
        self.sample_index = (self.sample_index + 1) % PERF_SAMPLE_COUNT;
        if !warming_up {
            self.lifetime_worst_ms = self.lifetime_worst_ms.max(frame_ms);
        }

        if !warming_up && frame_ms >= STUTTER_MS {
            self.stutter_count = self.stutter_count.saturating_add(1);
            if self.trigger_timer > 0.0 {
                self.trigger_stutter_count = self.trigger_stutter_count.saturating_add(1);
            }
        }

        let avg_ms = if self.sample_count > 0 {
            self.sample_sum_ms / self.sample_count as f32
        } else {
            frame_ms
        };
        let fps = if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 };
        let measured_ms = self.compose_ms + self.render_ms + self.overlay_ms;
        let rest_ms = (frame_ms - measured_ms).max(0.0);

        PerfSnapshot {
            frame: self.frame,
            fps,
            frame_ms,
            avg_ms,
            lifetime_worst_ms: self.lifetime_worst_ms,
            compose_ms: self.compose_ms,
            render_ms: self.render_ms,
            overlay_ms: self.overlay_ms,
            rest_ms,
            stutter_count: self.stutter_count,
            trigger_stutter_count: self.trigger_stutter_count,
            trigger_active: self.trigger_timer > 0.0,
            trigger_label: self.trigger_label.clone(),
            trigger_age_ms: self.trigger_age * 1000.0,
        }
    }

    fn record_phases(&mut self, compose_ms: f32, render_ms: f32, overlay_ms: f32) {
        self.compose_ms = compose_ms.clamp(0.0, 250.0);
        self.render_ms = render_ms.clamp(0.0, 250.0);
        self.overlay_ms = overlay_ms.clamp(0.0, 250.0);
    }

    fn observe_signature(&mut self, next: LabSignature) {
        let Some(previous) = self.signature.replace(next) else {
            return;
        };
        if previous == next {
            return;
        }

        self.trigger_label = trigger_label(previous, next).to_string();
        self.trigger_timer = 0.75;
        self.trigger_age = 0.0;
    }
}

fn trigger_label(previous: LabSignature, next: LabSignature) -> &'static str {
    if previous.mode != next.mode {
        "mode animation"
    } else if previous.tab != next.tab {
        "tab switch"
    } else if previous.segment != next.segment {
        "segment switch"
    } else if previous.reveal != next.reveal {
        "reveal layout"
    } else if previous.dropdown_open != next.dropdown_open
        || previous.dropdown_selected != next.dropdown_selected
    {
        "dropdown"
    } else if previous.dialog_open != next.dialog_open {
        "dialog"
    } else if previous.toast_visible != next.toast_visible {
        "toast"
    } else if previous.clicks != next.clicks {
        "button"
    } else {
        "state change"
    }
}

fn clicks_signal(state: &State<LabState>) -> Signal<LabState, u32> {
    state.signal(
        "stress-lab.clicks",
        |state| state.clicks,
        |state, value| state.clicks = value,
    )
}

fn mode_signal(state: &State<LabState>) -> Signal<LabState, i32> {
    state.signal(
        "stress-lab.mode",
        |state| state.mode,
        |state, value| state.mode = value.rem_euclid(4),
    )
}

fn wobble_signal(state: &State<LabState>) -> Signal<LabState, f32> {
    state.signal(
        "stress-lab.wobble",
        |state| state.wobble,
        |state, value| state.wobble = value.clamp(0.0, 1.0),
    )
}

fn density_signal(state: &State<LabState>) -> Signal<LabState, f32> {
    state.signal(
        "stress-lab.density",
        |state| state.density,
        |state, value| state.density = value.clamp(0.0, 1.0),
    )
}

fn alarm_signal(state: &State<LabState>) -> Signal<LabState, f32> {
    state.signal(
        "stress-lab.alarm",
        |state| state.alarm,
        |state, value| state.alarm = value.clamp(0.0, 1.0),
    )
}

fn glass_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.glass",
        |state| state.glass,
        |state, value| state.glass = value,
    )
}

fn lock_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.lock",
        |state| state.lock,
        |state, value| state.lock = value,
    )
}

fn reveal_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.reveal",
        |state| state.reveal,
        |state, value| state.reveal = value,
    )
}

fn tab_signal(state: &State<LabState>) -> Signal<LabState, i32> {
    state.signal(
        "stress-lab.tab",
        |state| state.tab,
        |state, value| state.tab = value.max(0),
    )
}

fn segment_signal(state: &State<LabState>) -> Signal<LabState, i32> {
    state.signal(
        "stress-lab.segment",
        |state| state.segment,
        |state, value| state.segment = value.max(0),
    )
}

fn radio_value_signal(state: &State<LabState>, value: i32) -> Signal<LabState, bool> {
    state.signal(
        format!("stress-lab.radio.{value}"),
        move |state| state.radio == value,
        move |state, selected| {
            if selected {
                state.radio = value.max(0);
            }
        },
    )
}

fn dropdown_open_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.dropdown-open",
        |state| state.dropdown_open,
        |state, value| state.dropdown_open = value,
    )
}

fn dropdown_selected_signal(state: &State<LabState>) -> Signal<LabState, i32> {
    state.signal(
        "stress-lab.dropdown-selected",
        |state| state.dropdown_selected,
        |state, value| state.dropdown_selected = value.max(0),
    )
}

fn dialog_open_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.dialog-open",
        |state| state.dialog_open,
        |state, value| state.dialog_open = value,
    )
}

fn toast_visible_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.toast-visible",
        |state| state.toast_visible,
        |state, value| state.toast_visible = value,
    )
}

fn context_menu_open_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.context-menu-open",
        |state| state.context_menu_open,
        |state, value| state.context_menu_open = value,
    )
}

fn context_menu_position_signal(state: &State<LabState>) -> Signal<LabState, [f32; 2]> {
    state.signal(
        "stress-lab.context-menu-position",
        |state| state.context_menu_position,
        |state, value| state.context_menu_position = value,
    )
}

fn control_scroll_signal(state: &State<LabState>) -> Signal<LabState, f32> {
    state.signal(
        "stress-lab.control-scroll",
        |state| state.control_scroll,
        |state, value| state.control_scroll = value.max(0.0),
    )
}

fn signal_scroll_signal(state: &State<LabState>) -> Signal<LabState, f32> {
    state.signal(
        "stress-lab.signal-scroll",
        |state| state.signal_scroll,
        |state, value| state.signal_scroll = value.max(0.0),
    )
}

fn interaction_scroll_signal(state: &State<LabState>) -> Signal<LabState, f32> {
    state.signal(
        "stress-lab.interaction-scroll",
        |state| state.interaction_scroll,
        |state, value| state.interaction_scroll = value.max(0.0),
    )
}

fn input_text_signal(state: &State<LabState>) -> Signal<LabState, String> {
    state.signal(
        "stress-lab.input-text",
        |state| state.input_text.clone(),
        |state, value| state.input_text = value,
    )
}

fn draw_lab(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    time: f32,
    state_store: &State<LabState>,
    perf: &PerfSnapshot,
    pointer_owned: bool,
    keyboard_owned: bool,
) {
    let motion = Transition::ease(0.24, Ease::OutCubic);
    let body_height = (screen_height - OUTER_PAD * 2.0 - HEADER_H - 18.0).max(0.0);

    ui.scope("lab.background.scope", |ui| {
        let alarm = alarm_signal(state_store).watch(ui);
        draw_background(ui, screen_width, screen_height, alarm);
    });

    ui.column("lab.root")
        .size(screen_width, screen_height)
        .padding(OUTER_PAD)
        .gap(18.0)
        .content(|ui| {
            ui.live_scope("lab.header.scope", |ui| {
                draw_header(ui, state_store, perf, time, pointer_owned, keyboard_owned);
            });

            ui.row("lab.body")
                .size(Size::fill(), body_height)
                .gap(18.0)
                .content(|ui| {
                    ui.scope("lab.control.scope", |ui| {
                        draw_control_panel(ui, state_store, motion, body_height);
                    });
                    ui.live_scope("lab.signals.scope", |ui| {
                        draw_signal_panel(ui, state_store, time, motion, body_height);
                    });
                    ui.scope("lab.interactions.scope", |ui| {
                        draw_interaction_panel(
                            ui,
                            state_store,
                            pointer_owned,
                            keyboard_owned,
                            body_height,
                        );
                    });
                });
        });

    ui.scope("lab.overlays.scope", |ui| {
        draw_lab_overlays(ui, screen_width, screen_height, state_store);
    });
}

fn draw_background(ui: &mut Ui, width: f32, height: f32, alarm: f32) {
    ui.rect("background")
        .size(width, height)
        .gradient(
            mix(
                c(0.020, 0.024, 0.034, 1.0),
                c(0.070, 0.030, 0.040, 1.0),
                alarm * 0.38,
            ),
            mix(
                c(0.038, 0.052, 0.070, 1.0),
                c(0.125, 0.070, 0.090, 1.0),
                alarm * 0.42,
            ),
        )
        .build();
}

fn draw_header(
    ui: &mut Ui,
    state_store: &State<LabState>,
    perf: &PerfSnapshot,
    time: f32,
    pointer_owned: bool,
    keyboard_owned: bool,
) {
    let clicks = clicks_signal(state_store).watch(ui);
    let mode = mode_signal(state_store).watch(ui);
    let glass = glass_signal(state_store).watch(ui);
    let lock = lock_signal(state_store).watch(ui);
    let reveal = reveal_signal(state_store).watch(ui);
    let alarm = alarm_signal(state_store).watch(ui);
    let density = density_signal(state_store).watch(ui);
    let scan = (time * (0.22 + density * 0.72)).fract();

    ui.stack("header")
        .size(Size::fill(), HEADER_H)
        .content(|ui| {
            widgets::panel(ui, "header.bg")
                .fill()
                .radius(18.0)
                .gradient(c(0.076, 0.095, 0.132, 0.96), c(0.034, 0.043, 0.060, 0.98))
                .border(1.0, c(0.300, 0.410, 0.540, 0.62))
                .shadow(24.0, 0.0, 8.0, c(0.0, 0.0, 0.0, 0.28))
                .build();

            ui.row("header.content")
                .fill()
                .padding_xy(20.0, 16.0)
                .gap(18.0)
                .align_items(Align::Center)
                .content(|ui| {
                    ui.column("header.copy")
                        .size(300.0, Size::fill())
                        .grow(1.0)
                        .justify_content(Align::Center)
                        .gap(4.0)
                        .content(|ui| {
                            ui.text("header.title")
                                .size(Size::fill(), 34.0)
                                .text("Neo UI Stress Lab")
                                .font_size(28.0)
                                .line_height(34.0)
                                .color(c(0.945, 0.970, 1.0, 1.0))
                                .build();

                            ui.text("header.status")
                                .size(Size::fill(), 22.0)
                                .text(format!(
                                    "mode {}  clicks {}  glass {}  lock {}  reveal {}  pointer {}  keyboard {}",
                                    mode,
                                    clicks,
                                    on_off(glass),
                                    on_off(lock),
                                    on_off(reveal),
                                    on_off(pointer_owned),
                                    on_off(keyboard_owned),
                                ))
                                .font_size(13.0)
                                .line_height(18.0)
                                .wrap(true)
                                .color(c(0.660, 0.740, 0.840, 0.88))
                                .build();
                        });

                    ui.column("header.meters")
                        .size(240.0, Size::fill())
                        .justify_content(Align::Center)
                        .gap(7.0)
                        .content(|ui| {
                            compact_meter(
                                ui,
                                "header.scan",
                                "scan",
                                scan,
                                c(0.300, 0.780, 0.580, 1.0),
                            );
                            compact_meter(
                                ui,
                                "header.alarm",
                                "alarm",
                                alarm,
                                c(0.980, 0.520, 0.350, 1.0),
                            );
                        });

                    draw_perf_monitor(ui, perf);

                    ui.row("header.actions")
                        .size(188.0, 44.0)
                        .gap(10.0)
                        .content(|ui| {
                            widgets::button(ui, "header.mode")
                                .height(42.0)
                                .min_width(86.0)
                                .grow(1.0)
                                .text("Mode")
                                .font_size(14.0)
                                .radius(12.0)
                                .on_click({
                                    let clicks = clicks_signal(state_store);
                                    let mode = mode_signal(state_store);
                                    move || {
                                        clicks.update(|clicks| clicks.wrapping_add(1));
                                        mode.update(|mode| (mode + 1).rem_euclid(4));
                                    }
                                })
                                .build();

                            widgets::button(ui, "header.panic")
                                .height(42.0)
                                .min_width(92.0)
                                .grow(1.0)
                                .text("Panic")
                                .font_size(14.0)
                                .colors(
                                    c(0.650, 0.210, 0.310, 1.0),
                                    c(0.850, 0.280, 0.420, 1.0),
                                    c(0.440, 0.120, 0.200, 1.0),
                                )
                                .radius(12.0)
                                .on_click({
                                    let clicks = clicks_signal(state_store);
                                    let alarm = alarm_signal(state_store);
                                    move || {
                                        clicks.update(|clicks| clicks.wrapping_add(1));
                                        alarm.update(|alarm| (alarm + 0.22).fract());
                                    }
                                })
                                .build();
                        });
                });
        });
}

fn draw_control_panel(ui: &mut Ui, state_store: &State<LabState>, motion: Transition, height: f32) {
    let wobble = wobble_signal(state_store).watch(ui);
    let density = density_signal(state_store).watch(ui);
    let alarm = alarm_signal(state_store).watch(ui);

    ui.stack("controls").size(SIDE_W, height).content(|ui| {
        panel_shell(
            ui,
            "controls.bg",
            c(0.070, 0.086, 0.120, 0.96),
            c(0.030, 0.038, 0.055, 0.98),
        );

        ui.scroll_y("controls.scroll")
            .fill()
            .inset(PANEL_SCROLL_INSET)
            .padding(PANEL_PAD)
            .gap(12.0)
            .scrollbar_gap(8.0)
            .offset_signal(control_scroll_signal(state_store))
            .content(|ui| {
                section_title(
                    ui,
                    "controls.title",
                    "Control Cabinet",
                    "Signals and capture",
                );

                slider_row(
                    ui,
                    "controls.wobble",
                    "Wobble",
                    wobble,
                    c(0.340, 0.580, 0.980, 1.0),
                    wobble_signal(state_store),
                );
                slider_row(
                    ui,
                    "controls.density",
                    "Density",
                    density,
                    c(0.280, 0.850, 0.580, 1.0),
                    density_signal(state_store),
                );
                slider_row(
                    ui,
                    "controls.alarm",
                    "Alarm",
                    alarm,
                    c(1.000, 0.620, 0.300, 1.0),
                    alarm_signal(state_store),
                );

                ui.column("controls.toggles")
                    .size(Size::fill(), 104.0)
                    .gap(6.0)
                    .content(|ui| {
                        widgets::switch(ui, "controls.glass")
                            .size(230.0, 30.0)
                            .signal(glass_signal(state_store))
                            .text("Glass Tint")
                            .build();
                        widgets::checkbox(ui, "controls.lock")
                            .size(230.0, 30.0)
                            .signal(lock_signal(state_store))
                            .text("Disable Right Card")
                            .build();
                        widgets::checkbox(ui, "controls.reveal")
                            .size(230.0, 30.0)
                            .signal(reveal_signal(state_store))
                            .text("Reveal Extra Card")
                            .build();
                    });

                ui.row("controls.radios")
                    .size(Size::fill(), 32.0)
                    .gap(10.0)
                    .content(|ui| {
                        radio_item(ui, state_store, "controls.radio.a", "A", 0);
                        radio_item(ui, state_store, "controls.radio.b", "B", 1);
                        radio_item(ui, state_store, "controls.radio.c", "C", 2);
                    });

                ui.column("controls.log")
                    .size(Size::fill(), 312.0)
                    .gap(8.0)
                    .content(|ui| {
                        for index in 0..10 {
                            dense_row(ui, index, alarm, density, motion);
                        }
                    });
            });
    });
}

fn draw_signal_panel(
    ui: &mut Ui,
    state_store: &State<LabState>,
    time: f32,
    motion: Transition,
    height: f32,
) {
    let pulse = time.sin() * 0.5 + 0.5;
    let wobble = wobble_signal(state_store).watch(ui);
    let density = density_signal(state_store).watch(ui);
    let alarm = alarm_signal(state_store).watch(ui);
    let glass = glass_signal(state_store).watch(ui);
    let tab = tab_signal(state_store).watch(ui);
    let scan = (time * (0.22 + density * 0.72)).fract();

    ui.stack("signals")
        .size(360.0, height)
        .grow(1.0)
        .min_width(360.0)
        .content(|ui| {
            panel_shell(
                ui,
                "signals.bg",
                c(0.061, 0.083, 0.103, 0.96),
                c(0.028, 0.040, 0.052, 0.98),
            );

            ui.scroll_y("signals.scroll")
                .fill()
                .inset(PANEL_SCROLL_INSET)
                .padding(PANEL_PAD)
                .gap(10.0)
                .scrollbar_gap(8.0)
                .offset_signal(signal_scroll_signal(state_store))
                .content(|ui| {
                    section_title(ui, "signals.title", "Signal Board", "Growth, fill, stacks");

                    widgets::tabs(ui, "signals.tabs")
                        .size(360.0, 38.0)
                        .items(["Signals", "Charts", "Motion"])
                        .signal(tab_signal(state_store))
                        .build();

                    draw_stage(ui, glass, alarm, wobble, time, pulse, scan, motion);
                    draw_metric_strip(ui, wobble, pulse, scan);
                    draw_tab_body(ui, state_store, tab, wobble, density, alarm, pulse, scan);

                    widgets::button(ui, "signals.context.button")
                        .height(40.0)
                        .min_width(210.0)
                        .text("Context Menu")
                        .font_size(15.0)
                        .secondary_theme(widgets::theme::dark_theme_colors())
                        .radius(13.0)
                        .on_context_menu({
                            let open = context_menu_open_signal(state_store);
                            let position = context_menu_position_signal(state_store);
                            move |event, bounds| {
                                let point = event
                                    .position()
                                    .unwrap_or([bounds.x + bounds.width, bounds.y]);
                                open.set(true);
                                position.set(point);
                            }
                        })
                        .on_click({
                            let open = context_menu_open_signal(state_store);
                            let position = context_menu_position_signal(state_store);
                            move || {
                                open.set(true);
                                position.set([760.0, 310.0]);
                            }
                        })
                        .build();
                });
        });
}

fn draw_stage(
    ui: &mut Ui,
    glass: bool,
    alarm: f32,
    wobble: f32,
    time: f32,
    pulse: f32,
    scan: f32,
    motion: Transition,
) {
    ui.stack("signals.stage")
        .size(Size::fill(), 132.0)
        .align(Align::Center, Align::Center)
        .content(|ui| {
            widgets::panel(ui, "signals.stage.bg")
                .fill()
                .radius(16.0)
                .color(if glass {
                    c(0.070, 0.150, 0.180, 0.70)
                } else {
                    c(0.064, 0.080, 0.105, 0.94)
                })
                .border(1.0, c(0.300, 0.450, 0.560, 0.44))
                .build();

            ui.row("signals.stage.tiles")
                .size(Size::fill(), Size::fill())
                .padding(16.0)
                .gap(12.0)
                .align_items(Align::Center)
                .content(|ui| {
                    stage_tile(
                        ui,
                        "signals.stage.a",
                        "pulse",
                        pulse,
                        c(0.340, 0.590, 0.980, 0.90),
                        motion,
                    );
                    stage_tile(
                        ui,
                        "signals.stage.b",
                        "scan",
                        scan,
                        c(0.280, 0.820, 0.620, 0.90),
                        motion,
                    );
                    stage_tile(
                        ui,
                        "signals.stage.c",
                        "alarm",
                        alarm,
                        mix(
                            c(0.840, 0.440, 0.620, 0.88),
                            c(1.000, 0.540, 0.320, 0.96),
                            alarm,
                        ),
                        motion,
                    );

                    ui.stack("signals.stage.glyph")
                        .size(88.0, 88.0)
                        .align(Align::Center, Align::Center)
                        .content(|ui| {
                            ui.rect("signals.stage.glyph.outer")
                                .size(78.0, 78.0)
                                .radius(24.0)
                                .color(c(0.130, 0.190, 0.250, 0.82))
                                .rotate((time * 0.8).sin() * 0.18 * wobble)
                                .transform_origin(0.5, 0.5)
                                .transition(motion)
                                .animate(AnimProperty::TRANSFORM | AnimProperty::COLOR)
                                .build();
                            ui.rect("signals.stage.glyph.inner")
                                .size(42.0, 42.0)
                                .radius(999.0)
                                .color(c(0.950, 0.420, 0.540, 0.82))
                                .scale(0.84 + pulse * 0.24)
                                .transform_origin(0.5, 0.5)
                                .transition(motion)
                                .animate(AnimProperty::TRANSFORM)
                                .build();
                        });
                });
        });
}

fn draw_metric_strip(ui: &mut Ui, wobble: f32, pulse: f32, scan: f32) {
    ui.row("signals.metrics")
        .size(Size::fill(), 100.0)
        .gap(12.0)
        .content(|ui| {
            metric_card(
                ui,
                "signals.metric.wobble",
                "Wobble",
                percent(wobble),
                "slider bound",
                c(0.330, 0.600, 0.980, 1.0),
            );
            metric_card(
                ui,
                "signals.metric.scan",
                "Scan",
                percent(scan),
                "animated",
                c(0.280, 0.830, 0.600, 1.0),
            );
            metric_card(
                ui,
                "signals.metric.pulse",
                "Pulse",
                percent(pulse),
                "frame",
                c(0.900, 0.520, 0.760, 1.0),
            );
        });
}

fn draw_tab_body(
    ui: &mut Ui,
    state_store: &State<LabState>,
    tab: i32,
    wobble: f32,
    density: f32,
    alarm: f32,
    pulse: f32,
    scan: f32,
) {
    match tab.rem_euclid(3) {
        1 => draw_chart_tab(ui, wobble, density, alarm, pulse, scan),
        2 => draw_motion_tab(ui, wobble, density, alarm, pulse, scan),
        _ => draw_signal_tab(ui, state_store, wobble, density, alarm, pulse, scan),
    }
}

fn draw_signal_tab(
    ui: &mut Ui,
    state_store: &State<LabState>,
    wobble: f32,
    density: f32,
    alarm: f32,
    pulse: f32,
    scan: f32,
) {
    ui.column("signals.tab.signals")
        .size(Size::fill(), 124.0)
        .gap(6.0)
        .content(|ui| {
            meter_row(
                ui,
                "signals.fill.blue",
                "blue fill",
                (pulse * 0.55 + wobble * 0.45).fract(),
                c(0.330, 0.640, 0.980, 1.0),
            );
            meter_row(
                ui,
                "signals.fill.green",
                "green fill",
                (scan + density * 0.25).fract(),
                c(0.280, 0.850, 0.580, 1.0),
            );
            meter_row(
                ui,
                "signals.fill.alarm",
                "alarm fill",
                alarm.max((1.0 - pulse) * 0.35),
                c(1.000, 0.620, 0.300, 1.0),
            );

            ui.row("signals.quick.actions")
                .size(Size::fill(), 34.0)
                .gap(8.0)
                .content(|ui| {
                    widgets::button(ui, "signals.dialog.quick")
                        .height(34.0)
                        .min_width(120.0)
                        .grow(1.0)
                        .text("Dialog")
                        .font_size(14.0)
                        .on_click({
                            let dialog = dialog_open_signal(state_store);
                            move || dialog.set(true)
                        })
                        .build();
                    widgets::button(ui, "signals.toast.quick")
                        .height(34.0)
                        .min_width(120.0)
                        .grow(1.0)
                        .text("Toast")
                        .font_size(14.0)
                        .secondary_theme(widgets::theme::dark_theme_colors())
                        .on_click({
                            let toast = toast_visible_signal(state_store);
                            move || toast.set(true)
                        })
                        .build();
                });
        });
}

fn draw_chart_tab(ui: &mut Ui, wobble: f32, density: f32, alarm: f32, pulse: f32, scan: f32) {
    ui.row("signals.tab.charts")
        .size(Size::fill(), 132.0)
        .gap(12.0)
        .content(|ui| {
            widgets::bar_chart(ui, "signals.chart.bar")
                .size(184.0, 132.0)
                .title("Load")
                .values([wobble, density, alarm, pulse])
                .labels(["W", "D", "A", "P"])
                .build();
            widgets::pie_chart(ui, "signals.chart.pie")
                .size(132.0, 132.0)
                .title("Mix")
                .values([
                    (wobble * 0.55 + scan * 0.45).max(0.02),
                    (density * 0.70 + pulse * 0.30).max(0.02),
                    alarm.max(0.02),
                ])
                .labels(["wobble", "density", "alarm"])
                .build();
        });
}

fn draw_motion_tab(ui: &mut Ui, wobble: f32, density: f32, alarm: f32, pulse: f32, scan: f32) {
    ui.column("signals.tab.motion")
        .size(Size::fill(), 132.0)
        .gap(10.0)
        .content(|ui| {
            ui.row("signals.motion.weights")
                .size(Size::fill(), 34.0)
                .gap(10.0)
                .content(|ui| {
                    grow_block(
                        ui,
                        "signals.motion.a",
                        0.7 + wobble,
                        c(0.330, 0.520, 0.900, 0.94),
                    );
                    grow_block(
                        ui,
                        "signals.motion.b",
                        0.7 + density,
                        c(0.260, 0.730, 0.560, 0.94),
                    );
                    grow_block(
                        ui,
                        "signals.motion.c",
                        0.7 + alarm,
                        c(0.850, 0.420, 0.520, 0.94),
                    );
                });

            ui.row("signals.motion.tiles")
                .size(Size::fill(), 88.0)
                .gap(12.0)
                .content(|ui| {
                    motion_card(
                        ui,
                        "signals.motion.pulse",
                        "Pulse",
                        pulse,
                        c(0.340, 0.590, 0.980, 0.94),
                    );
                    motion_card(
                        ui,
                        "signals.motion.scan",
                        "Scan",
                        scan,
                        c(0.280, 0.820, 0.620, 0.94),
                    );
                });
        });
}

fn draw_interaction_panel(
    ui: &mut Ui,
    state_store: &State<LabState>,
    pointer_owned: bool,
    keyboard_owned: bool,
    height: f32,
) {
    let lock = lock_signal(state_store).watch(ui);
    let reveal = reveal_signal(state_store).watch(ui);
    let segment = segment_signal(state_store).watch(ui);
    let dropdown_selected = dropdown_selected_signal(state_store).watch(ui);

    ui.stack("interactions")
        .size(RIGHT_W, height)
        .content(|ui| {
            panel_shell(
                ui,
                "interactions.bg",
                c(0.086, 0.064, 0.106, 0.96),
                c(0.038, 0.034, 0.056, 0.98),
            );

            ui.scroll_y("interactions.scroll")
                .fill()
                .inset(PANEL_SCROLL_INSET)
                .padding(PANEL_PAD)
                .gap(10.0)
                .scrollbar_gap(8.0)
                .offset_signal(interaction_scroll_signal(state_store))
                .content(|ui| {
                    section_title(
                        ui,
                        "interactions.title",
                        "Interaction Bay",
                        "Forms and popups",
                    );

                    widgets::segmented(ui, "interactions.segment")
                        .size(RIGHT_FIELD_W, 36.0)
                        .items(["mild", "odd", "loud"])
                        .signal(segment_signal(state_store))
                        .build();

                    widgets::input(ui, "interactions.input")
                        .size(Size::fill(), 42.0)
                        .text_signal(input_text_signal(state_store))
                        .placeholder("type a strange word")
                        .build();

                    widgets::dropdown(ui, "interactions.dropdown")
                        .size(RIGHT_FIELD_W, 42.0)
                        .items(["Low hum", "Signal", "Unstable"])
                        .value_signal(dropdown_selected_signal(state_store))
                        .open_signal(dropdown_open_signal(state_store))
                        .build();

                    ui.row("interactions.actions")
                        .size(Size::fill(), 42.0)
                        .gap(10.0)
                        .content(|ui| {
                            widgets::button(ui, "interactions.dialog")
                                .height(38.0)
                                .min_width(120.0)
                                .grow(1.0)
                                .text("Dialog")
                                .font_size(14.0)
                                .on_click({
                                    let dialog = dialog_open_signal(state_store);
                                    move || dialog.set(true)
                                })
                                .build();
                            widgets::button(ui, "interactions.toast")
                                .height(38.0)
                                .min_width(120.0)
                                .grow(1.0)
                                .text("Toast")
                                .font_size(14.0)
                                .secondary_theme(widgets::theme::dark_theme_colors())
                                .on_click({
                                    let toast = toast_visible_signal(state_store);
                                    move || toast.set(true)
                                })
                                .build();
                        });

                    ui.row("interactions.cards")
                        .size(Size::fill(), 96.0)
                        .gap(12.0)
                        .content(|ui| {
                            action_card(ui, "interactions.locked", state_store, lock);
                            if reveal {
                                secret_card(ui, "interactions.secret", state_store);
                            }
                        });

                    ui.column("interactions.feed")
                        .size(Size::fill(), 459.0)
                        .gap(9.0)
                        .content(|ui| {
                            for index in 0..12 {
                                feed_row(ui, index, segment, dropdown_selected);
                            }
                        });

                    ui.row("interactions.capture")
                        .size(Size::fill(), 34.0)
                        .gap(10.0)
                        .content(|ui| {
                            widgets::badge(ui, "interactions.pointer")
                                .text(format!("pointer {}", on_off(pointer_owned)))
                                .accent(c(0.300, 0.620, 0.980, 1.0))
                                .grow(1.0)
                                .build();
                            widgets::badge(ui, "interactions.keyboard")
                                .text(format!("keyboard {}", on_off(keyboard_owned)))
                                .accent(c(0.440, 0.820, 0.640, 1.0))
                                .grow(1.0)
                                .build();
                        });
                });
        });
}

fn draw_lab_overlays(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    state_store: &State<LabState>,
) {
    let context_position = context_menu_position_signal(state_store).watch(ui);

    widgets::context_menu(ui, "lab.context")
        .open_signal(context_menu_open_signal(state_store))
        .screen(screen_width, screen_height)
        .position(context_position[0], context_position[1])
        .items(["Cycle mode", "Show toast", "Close menu"])
        .on_dismiss({
            let open = context_menu_open_signal(state_store);
            move || open.set(false)
        })
        .on_select({
            let mode = mode_signal(state_store);
            let toast = toast_visible_signal(state_store);
            let open = context_menu_open_signal(state_store);
            move |index| match index {
                0 => mode.update(|mode| (mode + 1).rem_euclid(4)),
                1 => toast.set(true),
                _ => open.set(false),
            }
        })
        .build();

    widgets::dialog(ui, "lab.dialog")
        .open_signal(dialog_open_signal(state_store))
        .screen(screen_width, screen_height)
        .title("Stress Lab Confirmation")
        .message("This modal is centered by the widget while the page beneath is pure layout flow.")
        .primary_text("Proceed")
        .secondary_text("Cancel")
        .on_primary({
            let dialog = dialog_open_signal(state_store);
            let toast = toast_visible_signal(state_store);
            move || {
                dialog.set(false);
                toast.set(true);
            }
        })
        .on_secondary({
            let dialog = dialog_open_signal(state_store);
            move || dialog.set(false)
        })
        .on_close({
            let dialog = dialog_open_signal(state_store);
            move || dialog.set(false)
        })
        .build();

    widgets::toast(ui, "lab.toast")
        .visible_signal(toast_visible_signal(state_store))
        .screen(screen_width, screen_height)
        .title("Layout held")
        .message("Rows, columns, grow, fill, clipping, and overlays survived the frame.")
        .duration(2.4)
        .on_dismiss({
            let visible = toast_visible_signal(state_store);
            move || visible.set(false)
        })
        .on_auto_dismiss({
            let visible = toast_visible_signal(state_store);
            move || visible.set(false)
        })
        .build();
}

fn panel_shell(ui: &mut Ui, id: &str, top: Color, bottom: Color) {
    widgets::panel(ui, id)
        .fill()
        .gradient(top, bottom)
        .border(1.0, c(0.280, 0.380, 0.500, 0.62))
        .shadow(22.0, 0.0, 8.0, c(0.0, 0.0, 0.0, 0.25))
        .radius(16.0)
        .build();
}

fn section_title(ui: &mut Ui, id: &str, title: &str, subtitle: &str) {
    ui.column(id)
        .size(Size::fill(), 52.0)
        .gap(3.0)
        .content(|ui| {
            ui.text(format!("{id}.title"))
                .size(Size::fill(), 28.0)
                .text(title)
                .font_size(21.0)
                .line_height(26.0)
                .color(c(0.930, 0.965, 1.0, 0.96))
                .build();
            ui.text(format!("{id}.subtitle"))
                .size(Size::fill(), 18.0)
                .text(subtitle)
                .font_size(12.0)
                .line_height(16.0)
                .color(c(0.620, 0.700, 0.820, 0.72))
                .build();
        });
}

fn slider_row(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: f32,
    color: Color,
    signal: Signal<LabState, f32>,
) {
    ui.column(id)
        .size(Size::fill(), 55.0)
        .gap(5.0)
        .content(|ui| {
            ui.row(format!("{id}.label.row"))
                .size(Size::fill(), 18.0)
                .gap(8.0)
                .content(|ui| {
                    ui.text(format!("{id}.label"))
                        .size(130.0, 18.0)
                        .grow(1.0)
                        .text(label)
                        .font_size(14.0)
                        .line_height(17.0)
                        .color(c(0.760, 0.840, 0.940, 0.88))
                        .build();
                    ui.text(format!("{id}.value"))
                        .size(64.0, 18.0)
                        .text(percent(value))
                        .font_size(12.0)
                        .line_height(15.0)
                        .color(c(0.580, 0.660, 0.780, 0.82))
                        .horizontal_align(HorizontalAlign::Right)
                        .build();
                });

            let mut style = widgets::SliderStyle::default();
            style.fill = color;
            widgets::slider(ui, format!("{id}.slider"))
                .size(CONTROL_FIELD_W, 26.0)
                .signal(signal)
                .style(style)
                .build();
        });
}

fn radio_item(ui: &mut Ui, state_store: &State<LabState>, id: &str, label: &str, value: i32) {
    widgets::radio(ui, id)
        .size(76.0, 28.0)
        .signal(radio_value_signal(state_store, value))
        .text(label)
        .font_size(15.0)
        .build();
}

fn dense_row(ui: &mut Ui, index: usize, alarm: f32, density: f32, motion: Transition) {
    let t = index as f32 / 9.0;
    ui.row(format!("controls.log.row.{index}"))
        .size(Size::fill(), 24.0)
        .gap(8.0)
        .align_items(Align::Center)
        .content(|ui| {
            widgets::badge(ui, format!("controls.log.badge.{index}"))
                .text(format!("row {:02}", index + 1))
                .accent(mix(
                    c(0.280, 0.760, 0.560, 1.0),
                    c(0.980, 0.420, 0.520, 1.0),
                    alarm * t,
                ))
                .min_width(82.0)
                .height(24.0)
                .build();

            ui.stack(format!("controls.log.bar.{index}"))
                .size(92.0, 12.0)
                .grow(1.0)
                .content(|ui| {
                    ui.rect(format!("controls.log.track.{index}"))
                        .fill()
                        .radius(999.0)
                        .color(c(0.080, 0.110, 0.150, 0.90))
                        .build();
                    ui.rect(format!("controls.log.fill.{index}"))
                        .size((48.0 + 118.0 * t + density * 18.0).min(178.0), 12.0)
                        .radius(999.0)
                        .color(mix(
                            c(0.260, 0.780, 0.540, 0.78),
                            c(0.950, 0.320, 0.520, 0.84),
                            alarm * t,
                        ))
                        .transition(motion)
                        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                        .build();
                });
        });
}

fn compact_meter(ui: &mut Ui, id: &str, label: &str, value: f32, color: Color) {
    ui.row(id)
        .size(Size::fill(), 17.0)
        .gap(8.0)
        .align_items(Align::Center)
        .content(|ui| {
            ui.text(format!("{id}.label"))
                .size(42.0, 16.0)
                .text(label)
                .font_size(12.0)
                .line_height(16.0)
                .color(c(0.650, 0.735, 0.830, 0.78))
                .build();

            let mut style = widgets::ProgressStyle::default();
            style.fill = color;
            style.track = c(0.070, 0.100, 0.135, 0.96);
            widgets::progress(ui, format!("{id}.progress"))
                .size(160.0, 10.0)
                .value(value)
                .style(style)
                .build();
        });
}

fn draw_perf_monitor(ui: &mut Ui, perf: &PerfSnapshot) {
    let severity = perf_severity(perf.frame_ms);
    let budget = (perf.frame_ms / FRAME_BUDGET_MS).clamp(0.0, 1.0);
    let trigger_text = if perf.trigger_active {
        format!("{} +{:.0}ms", perf.trigger_label, perf.trigger_age_ms)
    } else {
        format!("last {}", perf.trigger_label)
    };

    ui.stack("header.perf").size(232.0, 62.0).content(|ui| {
        ui.rect("header.perf.bg")
            .fill()
            .radius(14.0)
            .color(c(0.034, 0.046, 0.060, 0.82))
            .border(
                1.0,
                widgets::theme::with_alpha(severity, if perf.trigger_active { 0.58 } else { 0.28 }),
            )
            .build();

        ui.column("header.perf.content")
            .fill()
            .padding_xy(10.0, 5.0)
            .gap(1.0)
            .content(|ui| {
                ui.row("header.perf.top")
                    .size(Size::fill(), 16.0)
                    .gap(8.0)
                    .align_items(Align::Center)
                    .content(|ui| {
                        ui.text("header.perf.fps")
                            .size(62.0, 16.0)
                            .text(format!("{:>3.0} FPS", perf.fps))
                            .font_size(12.0)
                            .font_weight(740)
                            .line_height(16.0)
                            .color(severity)
                            .build();

                        ui.text("header.perf.ms")
                            .size(78.0, 16.0)
                            .text(format!("{:>4.1} ms", perf.frame_ms))
                            .font_size(12.0)
                            .line_height(16.0)
                            .color(c(0.820, 0.900, 0.930, 0.92))
                            .build();

                        ui.text("header.perf.frame")
                            .size(52.0, 16.0)
                            .text(format!("#{}", perf.frame))
                            .font_size(11.0)
                            .line_height(16.0)
                            .horizontal_align(HorizontalAlign::Right)
                            .color(c(0.560, 0.650, 0.720, 0.82))
                            .build();
                    });

                ui.stack("header.perf.budget")
                    .size(Size::fill(), 5.0)
                    .content(|ui| {
                        ui.rect("header.perf.budget.track")
                            .fill()
                            .radius(999.0)
                            .color(c(0.070, 0.095, 0.120, 0.96))
                            .build();
                        ui.rect("header.perf.budget.fill")
                            .size(198.0 * budget, 5.0)
                            .radius(999.0)
                            .color(severity)
                            .build();
                    });

                ui.row("header.perf.bottom")
                    .size(Size::fill(), 16.0)
                    .gap(8.0)
                    .align_items(Align::Center)
                    .content(|ui| {
                        ui.text("header.perf.compose")
                            .size(62.0, 16.0)
                            .text(format!("ui {:>4.1}", perf.compose_ms))
                            .font_size(11.0)
                            .line_height(16.0)
                            .color(phase_color(perf.compose_ms))
                            .build();

                        ui.text("header.perf.render")
                            .size(62.0, 16.0)
                            .text(format!("ren {:>4.1}", perf.render_ms))
                            .font_size(11.0)
                            .line_height(16.0)
                            .color(phase_color(perf.render_ms))
                            .build();

                        ui.text("header.perf.overlay")
                            .size(58.0, 16.0)
                            .text(format!("ov {:>4.1}", perf.overlay_ms))
                            .font_size(11.0)
                            .line_height(16.0)
                            .horizontal_align(HorizontalAlign::Right)
                            .color(phase_color(perf.overlay_ms))
                            .build();
                    });

                ui.text("header.perf.event")
                    .size(Size::fill(), 10.0)
                    .text(format!(
                        "{}  rest {:.1} avg {:.1} p{:.0} d{} t{}",
                        trigger_text,
                        perf.rest_ms,
                        perf.avg_ms,
                        perf.lifetime_worst_ms,
                        perf.stutter_count,
                        perf.trigger_stutter_count
                    ))
                    .font_size(9.0)
                    .line_height(10.0)
                    .color(c(0.540, 0.650, 0.710, 0.78))
                    .build();
            });
    });
}

fn perf_severity(frame_ms: f32) -> Color {
    if frame_ms >= STUTTER_MS {
        c(0.980, 0.350, 0.300, 1.0)
    } else if frame_ms > FRAME_BUDGET_MS {
        c(0.980, 0.700, 0.320, 1.0)
    } else {
        c(0.320, 0.860, 0.660, 1.0)
    }
}

fn phase_color(phase_ms: f32) -> Color {
    if phase_ms >= STUTTER_MS {
        c(0.980, 0.350, 0.300, 0.96)
    } else if phase_ms > FRAME_BUDGET_MS {
        c(0.980, 0.700, 0.320, 0.94)
    } else {
        c(0.600, 0.740, 0.800, 0.86)
    }
}

fn stage_tile(ui: &mut Ui, id: &str, label: &str, value: f32, color: Color, motion: Transition) {
    ui.stack(id)
        .size(74.0, 98.0)
        .grow(1.0)
        .align(Align::Center, Align::Center)
        .content(|ui| {
            ui.column(format!("{id}.content"))
                .fill()
                .gap(8.0)
                .align_items(Align::Center)
                .justify_content(Align::Center)
                .content(|ui| {
                    ui.stack(format!("{id}.orb"))
                        .size(54.0, 54.0)
                        .align(Align::Center, Align::Center)
                        .content(|ui| {
                            ui.rect(format!("{id}.orb.bg"))
                                .size(54.0, 54.0)
                                .radius(18.0)
                                .color(c(0.070, 0.100, 0.135, 0.96))
                                .border(1.0, c(0.360, 0.480, 0.620, 0.32))
                                .build();
                            ui.rect(format!("{id}.orb.fill"))
                                .size(
                                    16.0 + value.clamp(0.0, 1.0) * 32.0,
                                    16.0 + value.clamp(0.0, 1.0) * 32.0,
                                )
                                .radius(999.0)
                                .color(color)
                                .transition(motion)
                                .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                                .build();
                        });
                    ui.text(format!("{id}.label"))
                        .size(Size::fill(), 18.0)
                        .text(label)
                        .font_size(12.0)
                        .line_height(16.0)
                        .horizontal_align(HorizontalAlign::Center)
                        .color(c(0.750, 0.830, 0.920, 0.82))
                        .build();
                });
        });
}

fn metric_card(ui: &mut Ui, id: &str, title: &str, value: String, note: &str, accent: Color) {
    ui.stack(id)
        .size(104.0, Size::fill())
        .grow(1.0)
        .min_width(104.0)
        .content(|ui| {
            widgets::panel(ui, format!("{id}.bg"))
                .fill()
                .radius(13.0)
                .color(c(0.075, 0.100, 0.132, 0.88))
                .border(1.0, c(0.330, 0.430, 0.560, 0.28))
                .build();

            ui.column(format!("{id}.content"))
                .fill()
                .padding(12.0)
                .gap(5.0)
                .content(|ui| {
                    ui.text(format!("{id}.title"))
                        .size(Size::fill(), 18.0)
                        .text(title)
                        .font_size(13.0)
                        .line_height(17.0)
                        .color(c(0.730, 0.810, 0.900, 0.86))
                        .build();
                    ui.text(format!("{id}.value"))
                        .size(Size::fill(), 28.0)
                        .text(value)
                        .font_size(23.0)
                        .line_height(27.0)
                        .color(accent)
                        .build();
                    ui.text(format!("{id}.note"))
                        .size(Size::fill(), 18.0)
                        .text(note)
                        .font_size(12.0)
                        .line_height(16.0)
                        .color(c(0.580, 0.660, 0.760, 0.74))
                        .build();
                });
        });
}

fn meter_row(ui: &mut Ui, id: &str, label: &str, value: f32, color: Color) {
    let value = value.clamp(0.0, 1.0);
    ui.row(id)
        .size(Size::fill(), 24.0)
        .gap(8.0)
        .align_items(Align::Center)
        .content(|ui| {
            ui.text(format!("{id}.label"))
                .size(84.0, 18.0)
                .text(label)
                .font_size(12.0)
                .line_height(16.0)
                .color(c(0.680, 0.760, 0.880, 0.76))
                .build();

            let mut style = widgets::ProgressStyle::default();
            style.fill = color;
            style.track = c(0.075, 0.105, 0.145, 0.92);
            widgets::progress(ui, format!("{id}.progress"))
                .size(245.0, 10.0)
                .value(value)
                .style(style)
                .build();
        });
}

fn grow_block(ui: &mut Ui, id: &str, grow: f32, color: Color) {
    ui.rect(id)
        .size(46.0, Size::fill())
        .grow(grow)
        .radius(10.0)
        .color(color)
        .build();
}

fn motion_card(ui: &mut Ui, id: &str, title: &str, value: f32, color: Color) {
    ui.stack(id)
        .size(140.0, Size::fill())
        .grow(1.0)
        .content(|ui| {
            widgets::panel(ui, format!("{id}.bg"))
                .fill()
                .radius(14.0)
                .color(c(0.070, 0.094, 0.125, 0.90))
                .border(1.0, c(0.320, 0.440, 0.560, 0.26))
                .build();

            ui.column(format!("{id}.content"))
                .fill()
                .padding(14.0)
                .gap(10.0)
                .content(|ui| {
                    ui.text(format!("{id}.title"))
                        .size(Size::fill(), 20.0)
                        .text(title)
                        .font_size(14.0)
                        .line_height(18.0)
                        .color(c(0.780, 0.850, 0.930, 0.88))
                        .build();
                    ui.rect(format!("{id}.bar"))
                        .size(Size::fill(), 22.0 + value.clamp(0.0, 1.0) * 34.0)
                        .radius(12.0)
                        .color(color)
                        .transition(Transition::ease(0.22, Ease::OutCubic))
                        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                        .build();
                });
        });
}

fn action_card(ui: &mut Ui, id: &str, state_store: &State<LabState>, locked: bool) {
    ui.stack(id)
        .size(124.0, Size::fill())
        .grow(1.0)
        .content(|ui| {
            widgets::panel(ui, format!("{id}.bg"))
                .fill()
                .radius(13.0)
                .color(if locked {
                    c(0.175, 0.182, 0.204, 0.76)
                } else {
                    c(0.120, 0.260, 0.315, 0.72)
                })
                .border(1.0, c(0.340, 0.460, 0.580, 0.42))
                .build();

            ui.column(format!("{id}.content"))
                .fill()
                .padding(12.0)
                .gap(10.0)
                .content(|ui| {
                    ui.text(format!("{id}.title"))
                        .size(Size::fill(), 22.0)
                        .text(if locked { "Locked" } else { "Active" })
                        .font_size(15.0)
                        .line_height(20.0)
                        .color(c(0.920, 0.960, 1.0, 0.92))
                        .build();
                    widgets::button(ui, format!("{id}.button"))
                        .height(34.0)
                        .min_width(94.0)
                        .text("Count")
                        .font_size(13.0)
                        .disabled(locked)
                        .on_click({
                            let clicks = clicks_signal(state_store);
                            move || clicks.update(|clicks| clicks.wrapping_add(1))
                        })
                        .build();
                });
        });
}

fn secret_card(ui: &mut Ui, id: &str, state_store: &State<LabState>) {
    let time = ui.clock().seconds();
    ui.stack(id)
        .size(124.0, Size::fill())
        .grow(1.0)
        .content(|ui| {
            widgets::panel(ui, format!("{id}.bg"))
                .fill()
                .radius(13.0)
                .color(c(0.330, 0.190, 0.430, 0.86))
                .border(1.0, c(0.560, 0.420, 0.720, 0.48))
                .build();

            ui.column(format!("{id}.content"))
                .fill()
                .padding(12.0)
                .gap(10.0)
                .content(|ui| {
                    ui.text(format!("{id}.title"))
                        .size(Size::fill(), 22.0)
                        .text(format!("Live Probe {}", ((time * 2.0).sin() > 0.0) as i32))
                        .font_size(15.0)
                        .line_height(20.0)
                        .color(c(0.980, 0.920, 1.0, 0.94))
                        .build();
                    widgets::button(ui, format!("{id}.button"))
                        .height(34.0)
                        .min_width(94.0)
                        .text("Ping")
                        .font_size(13.0)
                        .on_click({
                            let clicks = clicks_signal(state_store);
                            move || clicks.update(|clicks| clicks.wrapping_add(1))
                        })
                        .build();
                });
        });
}

fn feed_row(ui: &mut Ui, index: usize, segment: i32, selected: i32) {
    let accent = match (index + segment.max(0) as usize + selected.max(0) as usize) % 4 {
        0 => c(0.300, 0.620, 0.980, 1.0),
        1 => c(0.440, 0.820, 0.640, 1.0),
        2 => c(0.900, 0.560, 0.260, 1.0),
        _ => c(0.850, 0.440, 0.620, 1.0),
    };
    ui.row(format!("interactions.feed.row.{index}"))
        .size(Size::fill(), 30.0)
        .gap(10.0)
        .align_items(Align::Center)
        .content(|ui| {
            widgets::badge(ui, format!("interactions.feed.badge.{index}"))
                .text(format!("#{:02}", index + 1))
                .accent(accent)
                .height(26.0)
                .min_width(58.0)
                .build();

            ui.text(format!("interactions.feed.text.{index}"))
                .size(180.0, 22.0)
                .grow(1.0)
                .text(feed_text(index))
                .font_size(13.0)
                .line_height(18.0)
                .color(c(0.800, 0.870, 0.940, 0.86))
                .build();
        });
}

fn feed_text(index: usize) -> &'static str {
    match index % 6 {
        0 => "Button, hover, press",
        1 => "Dropdown z-order",
        2 => "Keyboard focus",
        3 => "Scroll clipping",
        4 => "Modal blocking",
        _ => "Toast timer",
    }
}

fn percent(value: f32) -> String {
    format!("{:>3}%", (value.clamp(0.0, 1.0) * 100.0).round() as i32)
}

fn c(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

#[derive(Debug)]
struct ScreenshotProbe {
    path: Option<String>,
    frame: u32,
    second_path: Option<String>,
    second_frame: u32,
    frame_count: u32,
    taken: bool,
    second_taken: bool,
    exit_after: bool,
}

impl Default for ScreenshotProbe {
    fn default() -> Self {
        Self {
            path: std::env::var("SKY_NEO_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_NEO_SCREENSHOT_FRAME").unwrap_or(45),
            second_path: std::env::var("SKY_NEO_SCREENSHOT_PATH_2")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            second_frame: env_u32("SKY_NEO_SCREENSHOT_FRAME_2").unwrap_or(u32::MAX),
            frame_count: 0,
            taken: false,
            second_taken: false,
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
            }
        }
        if !self.second_taken && self.frame_count >= self.second_frame {
            if let Some(path) = self.second_path.as_ref() {
                ctx.request_screenshot(path);
                self.second_taken = true;
            }
        }
        let all_requested_shots_taken =
            self.taken && (self.second_path.is_none() || self.second_taken);
        if self.exit_after && all_requested_shots_taken {
            ctx.request_exit();
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

fn env_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn elapsed_ms(start: Instant) -> f32 {
    start.elapsed().as_secs_f32() * 1000.0
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color::new(
        a.r * inv + b.r * t,
        a.g * inv + b.g * t,
        a.b * inv + b.b * t,
        a.a * inv + b.a * t,
    )
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn dump_runtime_layout(label: &str, runtime: &sky_engine::ui::neo::Runtime) {
    let snapshot = runtime.debug_snapshot_current();
    eprintln!(
        "[neo layout dump] {label}: screen={:?} layout={:?} dirty={:?} live={:?} built={} reused={} active_animations={} needs_render={} needs_compose={} full_redraw={}",
        snapshot.screen,
        snapshot.layout_mode,
        snapshot.dirty_scopes,
        snapshot.live_scopes,
        snapshot.scope_stats.built,
        snapshot.scope_stats.reused,
        snapshot.active_animation_count,
        snapshot.needs_render,
        snapshot.needs_compose,
        snapshot.full_redraw
    );
    for root in runtime.roots() {
        dump_element_layout(root, 0);
    }
    eprintln!("[neo layout dump end] {label}");
}

fn dump_runtime_draw(label: &str, runtime: &sky_engine::ui::neo::Runtime) {
    let trace = runtime.draw_debug_trace();
    let filter = env_string("SKY_NEO_LAB_DUMP_DRAW_FILTER");
    eprintln!(
        "[neo draw dump] {label}: screen={:?} commands={} primitives={} push_clip={} pop_clip={} max_clip_depth={} unbalanced_pops={} remaining_clip_depth={} filter={:?}",
        trace.screen,
        trace.command_count,
        trace.primitive_count,
        trace.push_clip_count,
        trace.pop_clip_count,
        trace.max_clip_depth,
        trace.unbalanced_pops,
        trace.remaining_clip_depth,
        filter
    );

    if env_flag("SKY_NEO_LAB_DUMP_DRAW_JSON") && filter.is_none() {
        eprintln!("{}", trace.to_json_pretty());
    } else if let Some(filter) = filter.as_deref() {
        for command in trace.filtered(filter) {
            eprintln!("{command}");
        }
    } else {
        eprintln!("{trace}");
    }

    eprintln!("[neo draw dump end] {label}");
}

fn dump_element_layout(element: &sky_engine::ui::neo::eui::Element, depth: usize) {
    let indent = "  ".repeat(depth);
    eprintln!(
        "{indent}{} {:?} frame=({:.1},{:.1},{:.1},{:.1}) size={:?}x{:?} children={} interactive={} focusable={} z={} clip={} text={:?}",
        element.id,
        element.kind,
        element.frame.x,
        element.frame.y,
        element.frame.width,
        element.frame.height,
        element.width,
        element.height,
        element.children.len(),
        element.interactive,
        element.focusable,
        element.z_index,
        element.clip,
        element.text
    );
    for child in &element.children {
        dump_element_layout(child, depth + 1);
    }
}

fn trace_chart_runtime(label: &str, tab: i32, time: f32, runtime: &sky_engine::ui::neo::Runtime) {
    let snapshot = runtime.debug_snapshot_current();
    let target = runtime
        .find("signals.chart.bar.bar.3")
        .map(|element| element.frame);
    let draw = runtime.draw_list().commands().iter().find_map(|command| {
        if let sky_engine::ui::neo::expert::UiDrawCommand::Rect(rect) = command {
            rect.id
                .ends_with(".signals.chart.bar.bar.3")
                .then_some(rect.frame)
        } else {
            None
        }
    });
    eprintln!(
        "[neo chart trace] {label} app_time={time:.3} tab={tab} runtime_frame={} layout={:?} dirty={:?} live={:?} target={target:?} draw={draw:?} active_animations={} needs_render={} full_redraw={}",
        snapshot.frame_index,
        snapshot.layout_mode,
        snapshot.dirty_scopes,
        snapshot.live_scopes,
        snapshot.active_animation_count,
        snapshot.needs_render,
        snapshot.full_redraw
    );
}

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("SkyEngine - Neo UI Stress Lab", WINDOW_W, WINDOW_H)
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

    App::new(world).run(NeoUiStressLab::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use sky_engine::ui::neo::{Runtime, UiTestDriver};

    fn perf_snapshot() -> PerfSnapshot {
        PerfSnapshot {
            frame: 1,
            fps: 60.0,
            frame_ms: 16.0,
            avg_ms: 16.0,
            lifetime_worst_ms: 16.0,
            compose_ms: 0.0,
            render_ms: 0.0,
            overlay_ms: 0.0,
            rest_ms: 0.0,
            stutter_count: 0,
            trigger_stutter_count: 0,
            trigger_active: false,
            trigger_label: "idle".to_string(),
            trigger_age_ms: 0.0,
        }
    }

    fn compose_lab(runtime: &mut Runtime, state: &State<LabState>, dirty_scopes: Vec<String>) {
        let perf = perf_snapshot();
        let state = state.clone();
        runtime.compose_scoped(
            WINDOW_W as f32,
            WINDOW_H as f32,
            dirty_scopes,
            move |ui, screen| {
                draw_lab(
                    ui,
                    screen.width,
                    screen.height,
                    0.0,
                    &state,
                    &perf,
                    false,
                    false,
                );
            },
        );
    }

    fn compose_lab_driver(
        driver: &mut UiTestDriver,
        state: &State<LabState>,
        dirty_scopes: Vec<String>,
    ) {
        compose_lab_driver_at(driver, state, dirty_scopes, 0.0);
    }

    fn compose_lab_driver_at(
        driver: &mut UiTestDriver,
        state: &State<LabState>,
        dirty_scopes: Vec<String>,
        time: f32,
    ) {
        let perf = perf_snapshot();
        let state = state.clone();
        driver.compose_scoped(dirty_scopes, move |ui, screen| {
            draw_lab(
                ui,
                screen.width,
                screen.height,
                time,
                &state,
                &perf,
                false,
                false,
            );
        });
    }

    #[test]
    fn signals_tab_click_uses_driver_and_rebuilds_signals_scope() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        compose_lab_driver(&mut driver, &state, Vec::new());
        let label = driver
            .find("signals.tabs.label.1")
            .expect("charts tab label should exist")
            .frame;
        let trace = driver
            .click_at(
                "signals.tabs.label.1",
                sky_engine::ui::neo::TargetPoint::Center,
            )
            .expect("clicking over charts label should hit the tab");

        assert_eq!(state.read(|state| state.tab), 1, "{trace}");
        assert!(trace.point.is_some_and(|[x, y]| label.contains([x, y])));
        let dirty_scopes = state.take_dirty_scopes();
        assert_eq!(
            dirty_scopes,
            vec!["stress-lab.lab.signals.scope".to_string()],
            "{trace}"
        );
        compose_lab_driver(&mut driver, &state, dirty_scopes);

        assert!(driver.runtime().scope_compose_stats().built >= 1);
        assert!(driver.runtime().scope_compose_stats().reused >= 1);
        assert!(driver
            .debug_snapshot()
            .scope_events
            .iter()
            .any(|event| { event.scope.as_str() == "stress-lab.lab.signals.scope" }));
        let indicator = driver
            .find("signals.tabs.indicator")
            .expect("tabs indicator should exist")
            .frame;
        let charts_hit = driver
            .find("signals.tabs.hit.1")
            .expect("charts tab hit rect should still exist")
            .frame;
        assert!(indicator.x > charts_hit.x);
        assert!(indicator.x < charts_hit.x + charts_hit.width);

        compose_lab_driver(&mut driver, &state, state.take_dirty_scopes());
        assert!(
            driver.runtime().scope_compose_stats().partial_layout,
            "stable post-tab frame should return to partial layout: {:?}",
            driver.debug_snapshot()
        );
    }

    #[test]
    fn signals_tab_hit_rect_click_uses_driver_and_rebuilds_signals_scope() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        compose_lab_driver(&mut driver, &state, Vec::new());
        let trace = driver
            .click("signals.tabs.hit.1")
            .expect("charts tab hit rect should exist");

        assert_eq!(state.read(|state| state.tab), 1, "{trace}");
        let dirty_scopes = state.take_dirty_scopes();
        assert_eq!(
            dirty_scopes,
            vec!["stress-lab.lab.signals.scope".to_string()],
            "{trace}"
        );
        compose_lab_driver(&mut driver, &state, dirty_scopes);

        assert!(driver.runtime().scope_compose_stats().built >= 1);
        assert!(driver.runtime().scope_compose_stats().reused >= 1);
        assert!(driver
            .debug_snapshot()
            .scope_events
            .iter()
            .any(|event| { event.scope.as_str() == "stress-lab.lab.signals.scope" }));
        let indicator = driver
            .find("signals.tabs.indicator")
            .expect("tabs indicator should exist")
            .frame;
        let charts_hit = driver
            .find("signals.tabs.hit.1")
            .expect("charts tab hit rect should still exist")
            .frame;
        assert!(indicator.x > charts_hit.x);
        assert!(indicator.x < charts_hit.x + charts_hit.width);
    }

    #[test]
    fn signal_to_charts_keeps_live_animation_targets_advancing() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        compose_lab_driver_at(&mut driver, &state, Vec::new(), 0.0);
        let before_orb = driver
            .find("signals.stage.a.orb.fill")
            .expect("pulse orb fill should exist before tab switch")
            .frame;

        let trace = driver
            .click("signals.tabs.hit.1")
            .expect("charts tab hit rect should exist");
        assert_eq!(state.read(|state| state.tab), 1, "{trace}");

        let dirty_scopes = state.take_dirty_scopes();
        compose_lab_driver_at(&mut driver, &state, dirty_scopes, 0.0);
        assert!(
            driver.find("signals.tab.charts").is_some(),
            "charts body should be present after tab switch: {:?}",
            driver.debug_snapshot()
        );
        driver.runtime_mut().tick_animations(1.0);
        let before_bar = rect_draw(driver.runtime(), "signals.chart.bar.bar.3")
            .expect("pulse chart bar should draw after switching to charts")
            .frame;
        eprintln!(
            "[signal->charts] after click settled: tab={} pulse_bar_draw={before_bar:?} snapshot={:?}",
            state.read(|state| state.tab),
            driver.runtime().debug_snapshot_current()
        );

        compose_lab_driver_at(&mut driver, &state, state.take_dirty_scopes(), 1.570_796_4);
        let after_orb = driver
            .find("signals.stage.a.orb.fill")
            .expect("pulse orb fill should still exist on charts tab")
            .frame;
        let after_bar_target = driver
            .find("signals.chart.bar.bar.3")
            .expect("pulse chart bar should still exist on charts tab")
            .frame;
        let after_compose_bar = rect_draw(driver.runtime(), "signals.chart.bar.bar.3")
            .expect("pulse chart bar should draw immediately after live compose")
            .frame;
        let mut sampled_bars = Vec::new();
        for step in 1..=18 {
            driver.runtime_mut().tick_animations(1.0 / 60.0);
            if matches!(step, 1 | 6 | 12 | 18) {
                let frame = rect_draw(driver.runtime(), "signals.chart.bar.bar.3")
                    .expect("pulse chart bar should draw during animation ticks")
                    .frame;
                sampled_bars.push((step, frame));
            }
        }
        let after_tick_bar = sampled_bars
            .last()
            .map(|(_, frame)| *frame)
            .expect("pulse chart bar should draw after animation tick");
        let snapshot = driver.debug_snapshot();
        eprintln!(
            "[signal->charts] after live compose: orb_target={after_orb:?} pulse_bar_target={after_bar_target:?} pulse_bar_draw={after_compose_bar:?} sampled_ticks={sampled_bars:?} snapshot={:?}",
            driver.runtime().debug_snapshot_current()
        );

        assert!(
            after_orb.width > before_orb.width + 8.0,
            "live pulse target should advance after switching to charts: before={before_orb:?} after={after_orb:?} snapshot={snapshot:?}"
        );
        assert!(
            after_bar_target.height > before_bar.height + 8.0,
            "charts tab should receive live pulse values: before_bar={before_bar:?} after_target={after_bar_target:?} snapshot={snapshot:?}"
        );
        assert!(
            after_tick_bar.height > after_compose_bar.height,
            "charts bar draw frame should animate after a tick: after_compose={after_compose_bar:?} after_tick={after_tick_bar:?}"
        );
        assert!(
            snapshot
                .dirty_scopes
                .iter()
                .any(|scope| scope == "stress-lab.lab.signals.scope"),
            "signals live scope should be dirty on the post-switch frame: {snapshot:?}"
        );
    }

    #[test]
    fn signals_tabs_indicator_animates_between_adjacent_tabs() {
        fn sample_transition(start_tab: i32, hit_id: &str) -> (f32, f32, f32) {
            let state = State::new(LabState {
                tab: start_tab,
                ..LabState::default()
            });
            let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

            compose_lab_driver_at(&mut driver, &state, Vec::new(), 0.0);
            driver.runtime_mut().tick_animations(0.0);
            let start_draw = rect_draw_x(driver.runtime(), "signals.tabs.indicator")
                .expect("tabs indicator should draw before click");

            let trace = driver
                .click(hit_id)
                .expect("target tab hit rect should exist");
            let dirty_scopes = state.take_dirty_scopes();
            assert!(!dirty_scopes.is_empty(), "{trace}");
            compose_lab_driver_at(&mut driver, &state, dirty_scopes, 0.0);
            driver.runtime_mut().tick_animations(0.0);
            let after_compose_draw = rect_draw_x(driver.runtime(), "signals.tabs.indicator")
                .expect("tabs indicator should draw after click");
            let target = driver
                .find("signals.tabs.indicator")
                .expect("tabs indicator target should exist after click")
                .frame
                .x;

            driver.runtime_mut().tick_animations(1.0 / 60.0);
            let after_tick_draw = rect_draw_x(driver.runtime(), "signals.tabs.indicator")
                .expect("tabs indicator should draw after animation tick");

            eprintln!(
                "[tabs indicator] start_tab={start_tab} hit={hit_id} start_draw={start_draw:.3} after_compose={after_compose_draw:.3} after_tick={after_tick_draw:.3} target={target:.3} snapshot={:?}",
                driver.runtime().debug_snapshot_current()
            );

            (start_draw, after_compose_draw, after_tick_draw)
        }

        let signal_to_chart = sample_transition(0, "signals.tabs.hit.1");
        let chart_to_motion = sample_transition(1, "signals.tabs.hit.2");

        assert!(
            signal_to_chart.2 > signal_to_chart.1,
            "Signals -> Charts indicator should move after one tick: {signal_to_chart:?}"
        );
        assert!(
            chart_to_motion.2 > chart_to_motion.1,
            "Charts -> Motion indicator should move after one tick: {chart_to_motion:?}"
        );
    }

    #[test]
    fn interaction_segment_click_starts_indicator_animation_and_ticks_forward() {
        let state = State::new(LabState {
            segment: 0,
            ..LabState::default()
        });
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        compose_lab_driver(&mut driver, &state, Vec::new());
        driver.runtime_mut().tick_animations(0.0);
        let start_target = driver
            .find("interactions.segment.indicator")
            .expect("segment indicator should exist before click")
            .frame;
        let start_draw = rect_draw_x(driver.runtime(), "interactions.segment.indicator")
            .expect("segment indicator should draw before click");

        let trace = driver
            .click("interactions.segment.hit.1")
            .expect("odd segment hit rect should exist");
        assert_eq!(state.read(|state| state.segment), 1, "{trace}");
        let dirty_scopes = state.take_dirty_scopes();
        assert_eq!(
            dirty_scopes,
            vec!["stress-lab.lab.interactions.scope".to_string()],
            "{trace}"
        );

        compose_lab_driver(&mut driver, &state, dirty_scopes);
        driver.runtime_mut().tick_animations(0.0);
        let next_target = driver
            .find("interactions.segment.indicator")
            .expect("segment indicator should exist after click")
            .frame;
        let after_compose_draw = rect_draw_x(driver.runtime(), "interactions.segment.indicator")
            .expect("segment indicator should draw after click");
        let snapshot = driver.runtime().debug_snapshot_current();

        assert!(
            next_target.x > start_target.x,
            "indicator target should move from mild to odd: start={start_target:?} next={next_target:?}"
        );
        assert!(
            after_compose_draw < next_target.x,
            "drawn indicator should initially lag behind target while animating: draw={after_compose_draw} target={}",
            next_target.x
        );
        assert!(
            snapshot.active_animation_count > 0,
            "segment click should leave active animations: {snapshot:?}"
        );

        driver.runtime_mut().tick_animations(1.0 / 60.0);
        let after_tick_draw = rect_draw_x(driver.runtime(), "interactions.segment.indicator")
            .expect("segment indicator should draw after tick");
        assert!(
            after_tick_draw > after_compose_draw,
            "indicator draw frame should move forward after a tick: before={after_compose_draw} after={after_tick_draw}"
        );
        assert!(
            (after_tick_draw - start_draw).abs() > 0.001,
            "indicator draw frame should no longer be frozen at the old position"
        );
    }

    #[test]
    fn interaction_segment_click_draws_odd_as_selected() {
        let state = State::new(LabState {
            segment: 0,
            ..LabState::default()
        });
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        compose_lab_driver(&mut driver, &state, Vec::new());
        driver.runtime_mut().tick_animations(0.0);
        log_segment_draw_state("before click", &driver);

        let trace = driver
            .click("interactions.segment.hit.1")
            .expect("odd segment hit rect should exist");
        eprintln!("{trace}");
        eprintln!(
            "state.segment after click = {}",
            state.read(|state| state.segment)
        );

        let dirty_scopes = state.take_dirty_scopes();
        eprintln!("dirty scopes after click = {dirty_scopes:?}");
        compose_lab_driver(&mut driver, &state, dirty_scopes);
        driver.runtime_mut().tick_animations(0.0);
        log_segment_draw_state("after compose", &driver);

        for step in 1..=12 {
            driver.runtime_mut().tick_animations(1.0 / 60.0);
            if matches!(step, 1 | 3 | 6 | 12) {
                log_segment_draw_state(&format!("after tick {step}"), &driver);
            }
        }

        assert_eq!(state.read(|state| state.segment), 1);
        for _ in 0..48 {
            driver.runtime_mut().tick_animations(1.0 / 60.0);
        }
        log_segment_draw_state("after settle", &driver);
        let indicator = rect_draw(driver.runtime(), "interactions.segment.indicator")
            .expect("segment indicator should draw after settling");
        let odd_hit = driver
            .find("interactions.segment.hit.1")
            .expect("odd segment hit rect should exist after settling")
            .frame;
        assert!(
            indicator.frame.x > odd_hit.x && indicator.frame.x < odd_hit.right(),
            "indicator highlight should be drawn over odd: indicator={:?} odd_hit={odd_hit:?}",
            indicator.frame
        );
    }

    fn rect_draw_x(runtime: &Runtime, id: &str) -> Option<f32> {
        rect_draw(runtime, id).map(|rect| rect.frame.x)
    }

    fn rect_draw(runtime: &Runtime, id: &str) -> Option<sky_engine::ui::neo::expert::UiRectDraw> {
        runtime.draw_list().commands().iter().find_map(|command| {
            if let sky_engine::ui::neo::expert::UiDrawCommand::Rect(rect) = command {
                (rect.id == format!("stress-lab.{id}")).then_some(rect.clone())
            } else {
                None
            }
        })
    }

    fn text_draw_color(runtime: &Runtime, id: &str) -> Option<Color> {
        runtime.draw_list().commands().iter().find_map(|command| {
            if let sky_engine::ui::neo::expert::UiDrawCommand::Text(text) = command {
                (text.id == format!("stress-lab.{id}")).then_some(text.color)
            } else {
                None
            }
        })
    }

    fn log_segment_draw_state(label: &str, driver: &UiTestDriver) {
        let runtime = driver.runtime();
        let target = driver
            .find("interactions.segment.indicator")
            .map(|element| element.frame);
        let indicator = rect_draw(runtime, "interactions.segment.indicator");
        let mild = text_draw_color(runtime, "interactions.segment.label.0");
        let odd = text_draw_color(runtime, "interactions.segment.label.1");
        let loud = text_draw_color(runtime, "interactions.segment.label.2");
        let snapshot = runtime.debug_snapshot_current();
        eprintln!(
            "[segment draw] {label}: target={target:?} indicator_frame={:?} indicator_color={:?} label_colors mild={mild:?} odd={odd:?} loud={loud:?} active_animations={} needs_render={} layout={:?}",
            indicator.as_ref().map(|rect| rect.frame),
            indicator.as_ref().map(|rect| rect.color),
            snapshot.active_animation_count,
            snapshot.needs_render,
            snapshot.layout_mode
        );
    }

    #[test]
    fn live_scopes_rebuild_without_state_dirty() {
        let state = State::new(LabState::default());
        let mut runtime = Runtime::new("stress-lab");

        compose_lab(&mut runtime, &state, Vec::new());
        compose_lab(&mut runtime, &state, Vec::new());

        assert!(runtime.scope_compose_stats().built >= 2);
        assert!(runtime.scope_compose_stats().reused >= 1);
    }
}
