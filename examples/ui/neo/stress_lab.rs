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
#[cfg(feature = "egui")]
use sky_engine::ui::neo::LayoutRect;
use sky_engine::ui::neo::{
    Align, AnimProperty, Color, Ease, FrameInput, HorizontalAlign, NeoUiBackend, PointerEvent,
    Signal, Size, State, Transition, Ui,
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
    agent_debug: Option<sky_engine::ui::neo::expert::AgentDebugService>,
    #[cfg(feature = "egui")]
    debug: NeoEguiDebug,
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
    control_dropdown_open: bool,
    control_dropdown_selected: i32,
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
            agent_debug: sky_engine::ui::neo::expert::AgentDebugService::from_env("stress_lab"),
            #[cfg(feature = "egui")]
            debug: NeoEguiDebug::default(),
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
            control_dropdown_open: false,
            control_dropdown_selected: 0,
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

        let overlay_start = Instant::now();
        ctx.ui().render_overlays();
        let overlay_ms = elapsed_ms(overlay_start);

        self.frame_monitor.record_phases(compose_ms, overlay_ms);
        maybe_log_stutter(&perf, compose_ms, overlay_ms);
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
        self.update_agent_debug(ctx);
        #[cfg(feature = "egui")]
        self.draw_egui_debug(ctx);
    }

    fn update_agent_debug(&mut self, ctx: &mut FrameContext<'_>) {
        let Some(service) = self.agent_debug.as_mut() else {
            return;
        };
        let state = self.state.read(|state| {
            (
                state.control_dropdown_open,
                state.control_dropdown_selected,
                state.dropdown_open,
                state.dropdown_selected,
                state.dialog_open,
                state.toast_visible,
                state.context_menu_open,
                state.control_scroll,
                state.signal_scroll,
                state.interaction_scroll,
            )
        });
        let context = sky_engine::ui::neo::expert::AgentDebugContext::new()
            .state("frame_index", self.frame_index)
            .state("control_dropdown_open", state.0)
            .state("control_dropdown_selected", state.1)
            .state("dropdown_open", state.2)
            .state("dropdown_selected", state.3)
            .state("dialog_open", state.4)
            .state("toast_visible", state.5)
            .state("context_menu_open", state.6)
            .state("control_scroll", format!("{:.1}", state.7))
            .state("signal_scroll", format!("{:.1}", state.8))
            .state("interaction_scroll", format!("{:.1}", state.9))
            .dropdown("controls.preset.dropdown", Some(state.0), Some(state.1))
            .dropdown("interactions.dropdown", Some(state.2), Some(state.3));

        let effects = {
            let mut ui = ctx.ui();
            ui.with_backend_mut::<NeoUiBackend, _>(|backend| {
                service.update(backend.runtime_mut(), context)
            })
            .unwrap_or_default()
        };
        for effect in effects {
            match effect {
                sky_engine::ui::neo::expert::AgentDebugEffect::Screenshot { path } => {
                    ctx.request_screenshot(path);
                }
            }
        }
    }

    #[cfg(feature = "egui")]
    fn draw_egui_debug(&mut self, ctx: &mut FrameContext<'_>) {
        let element_filter = self.debug.element_filter.clone();
        let draw_filter = self.debug.draw_filter.clone();
        let max_rows = self.debug.max_rows;
        let lab_state = self.state.read(LabDebugState::from_state);
        let mut data = None;
        {
            let mut ui = ctx.ui();
            ui.with_backend_mut::<NeoUiBackend, _>(|backend| {
                data = Some(NeoDebugPanelData::collect(
                    backend.runtime(),
                    lab_state,
                    &element_filter,
                    &draw_filter,
                    max_rows,
                ));
            });
        }

        let Some(data) = data else {
            return;
        };
        let debug = &mut self.debug;
        ctx.egui(move |root_ui| {
            debug.show(root_ui, &data);
        });
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
            let target_frame =
                backend.runtime().diagnostics().find("signals.tabs.hit.1").map(|element| element.frame);
            if let Some(frame) = target_frame {
                let x = frame.x + frame.width * 0.5;
                let y = frame.y + frame.height * 0.5;
                let screen = backend.runtime().screen();
                let changed = backend.runtime_mut().dispatch_frame_input(
                    FrameInput::new(screen, 0.0).pointer_events([
                        PointerEvent::pressed_at(x, y),
                        PointerEvent::released_at(x, y),
                    ]),
                );
                eprintln!(
                    "[neo auto click] target=signals.tabs.hit.1 frame=({:.2},{:.2},{:.2},{:.2}) point=({:.2},{:.2}) input_changed={} needs_compose={}",
                    frame.x,
                    frame.y,
                    frame.width,
                    frame.height,
                    x,
                    y,
                    changed,
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
    control_dropdown_open: bool,
    control_dropdown_selected: i32,
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
            control_dropdown_open: value.control_dropdown_open,
            control_dropdown_selected: value.control_dropdown_selected,
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
        let measured_ms = self.compose_ms + self.overlay_ms;
        let rest_ms = (frame_ms - measured_ms).max(0.0);

        PerfSnapshot {
            frame: self.frame,
            fps,
            frame_ms,
            avg_ms,
            lifetime_worst_ms: self.lifetime_worst_ms,
            compose_ms: self.compose_ms,
            overlay_ms: self.overlay_ms,
            rest_ms,
            stutter_count: self.stutter_count,
            trigger_stutter_count: self.trigger_stutter_count,
            trigger_active: self.trigger_timer > 0.0,
            trigger_label: self.trigger_label.clone(),
            trigger_age_ms: self.trigger_age * 1000.0,
        }
    }

    fn record_phases(&mut self, compose_ms: f32, overlay_ms: f32) {
        self.compose_ms = compose_ms.clamp(0.0, 250.0);
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
        || previous.control_dropdown_open != next.control_dropdown_open
        || previous.control_dropdown_selected != next.control_dropdown_selected
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

fn control_dropdown_open_signal(state: &State<LabState>) -> Signal<LabState, bool> {
    state.signal(
        "stress-lab.control-dropdown-open",
        |state| state.control_dropdown_open,
        |state, value| {
            state.control_dropdown_open = value;
        },
    )
}

fn control_dropdown_selected_signal(state: &State<LabState>) -> Signal<LabState, i32> {
    state.signal(
        "stress-lab.control-dropdown-selected",
        |state| state.control_dropdown_selected,
        |state, value| {
            state.control_dropdown_selected = value.max(0);
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
    state_store: &State<LabState>,
    perf: &PerfSnapshot,
    pointer_owned: bool,
    keyboard_owned: bool,
) {
    let motion = Transition::ease(0.24, Ease::OutCubic);
    let body_height = (screen_height - OUTER_PAD * 2.0 - HEADER_H - 18.0).max(0.0);

    ui.stack("lab.background")
        .size(screen_width, screen_height)
        .content(|ui| {
            let alarm = alarm_signal(state_store).watch(ui);
            draw_background(ui, screen_width, screen_height, alarm);
        });

    ui.column("lab.root")
        .size(screen_width, screen_height)
        .padding(OUTER_PAD)
        .gap(18.0)
        .content(|ui| {
            draw_header(ui, state_store, perf, pointer_owned, keyboard_owned);

            ui.row("lab.body")
                .size(Size::fill(), body_height)
                .gap(18.0)
                .content(|ui| {
                    draw_control_panel(ui, state_store, motion, body_height);
                    draw_signal_panel(ui, state_store, motion, body_height);
                    draw_interaction_panel(
                        ui,
                        state_store,
                        pointer_owned,
                        keyboard_owned,
                        body_height,
                    );
                });
        });

    draw_lab_overlays(ui, screen_width, screen_height, state_store);
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
    pointer_owned: bool,
    keyboard_owned: bool,
) {
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
                    draw_header_copy(ui, state_store, pointer_owned, keyboard_owned);
                    draw_header_meters(ui, state_store);
                    draw_perf_monitor(ui, perf);
                    draw_header_actions(ui, state_store);
                });
        });
}

fn draw_header_copy(
    ui: &mut Ui,
    state_store: &State<LabState>,
    pointer_owned: bool,
    keyboard_owned: bool,
) {
    ui.column("header.copy")
        .size(300.0, Size::fill())
        .grow(1.0)
        .justify_content(Align::Center)
        .gap(4.0)
        .content(|ui| {
            let _frame = ui.clock().frame_index();
            let clicks = clicks_signal(state_store).watch(ui);
            let mode = mode_signal(state_store).watch(ui);
            let glass = glass_signal(state_store).watch(ui);
            let lock = lock_signal(state_store).watch(ui);
            let reveal = reveal_signal(state_store).watch(ui);

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
}

fn draw_header_meters(ui: &mut Ui, state_store: &State<LabState>) {
    ui.column("header.meters")
        .size(240.0, Size::fill())
        .justify_content(Align::Center)
        .gap(7.0)
        .content(|ui| {
            let time = ui.clock().seconds();
            let density = density_signal(state_store).watch(ui);
            let alarm = alarm_signal(state_store).watch(ui);
            let scan = (time * (0.22 + density * 0.72)).fract();

            compact_meter(ui, "header.scan", "scan", scan, c(0.300, 0.780, 0.580, 1.0));
            compact_meter(
                ui,
                "header.alarm",
                "alarm",
                alarm,
                c(0.980, 0.520, 0.350, 1.0),
            );
        });
}

fn draw_header_actions(ui: &mut Ui, state_store: &State<LabState>) {
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
}

fn draw_control_panel(ui: &mut Ui, state_store: &State<LabState>, motion: Transition, height: f32) {
    ui.stack("controls").size(SIDE_W, height).content(|ui| {
        let wobble = wobble_signal(state_store).watch(ui);
        let density = density_signal(state_store).watch(ui);
        let alarm = alarm_signal(state_store).watch(ui);
        let control_preset = control_dropdown_selected_signal(state_store).watch(ui);

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

                widgets::dropdown(ui, "controls.preset.dropdown")
                    .size(CONTROL_FIELD_W, 42.0)
                    .items(["Drift", "Burst", "Quiet"])
                    .value_signal(control_dropdown_selected_signal(state_store))
                    .open_signal(control_dropdown_open_signal(state_store))
                    .build();

                ui.column("controls.log")
                    .size(Size::fill(), 312.0)
                    .gap(8.0)
                    .content(|ui| {
                        for index in 0..10 {
                            dense_row(ui, index, alarm, density, control_preset, motion);
                        }
                    });
            });
    });
}

fn draw_signal_panel(ui: &mut Ui, state_store: &State<LabState>, motion: Transition, height: f32) {
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

                    draw_stage(ui, state_store, motion);
                    draw_metric_strip(ui, state_store);
                    draw_tab_body(ui, state_store);

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

fn draw_stage(ui: &mut Ui, state_store: &State<LabState>, motion: Transition) {
    ui.stack("signals.stage")
        .size(Size::fill(), 132.0)
        .align(Align::Center, Align::Center)
        .content(|ui| {
            let time = ui.clock().seconds();
            let pulse = time.sin() * 0.5 + 0.5;
            let wobble = wobble_signal(state_store).watch(ui);
            let density = density_signal(state_store).watch(ui);
            let alarm = alarm_signal(state_store).watch(ui);
            let glass = glass_signal(state_store).watch(ui);
            let scan = (time * (0.22 + density * 0.72)).fract();

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

fn draw_metric_strip(ui: &mut Ui, state_store: &State<LabState>) {
    ui.row("signals.metrics")
        .size(Size::fill(), 100.0)
        .gap(12.0)
        .content(|ui| {
            let time = ui.clock().seconds();
            let pulse = time.sin() * 0.5 + 0.5;
            let wobble = wobble_signal(state_store).watch(ui);
            let density = density_signal(state_store).watch(ui);
            let scan = (time * (0.22 + density * 0.72)).fract();

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

fn draw_tab_body(ui: &mut Ui, state_store: &State<LabState>) {
    ui.stack("signals.tab.body")
        .size(Size::fill(), 132.0)
        .content(|ui| {
            let time = ui.clock().seconds();
            let pulse = time.sin() * 0.5 + 0.5;
            let wobble = wobble_signal(state_store).watch(ui);
            let density = density_signal(state_store).watch(ui);
            let alarm = alarm_signal(state_store).watch(ui);
            let tab = tab_signal(state_store).watch(ui);
            let scan = (time * (0.22 + density * 0.72)).fract();

            match tab.rem_euclid(3) {
                1 => draw_chart_tab(ui, wobble, density, alarm, pulse, scan),
                2 => draw_motion_tab(ui, wobble, density, alarm, pulse, scan),
                _ => draw_signal_tab(ui, state_store, wobble, density, alarm, pulse, scan),
            }
        });
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
    ui.stack("interactions")
        .size(RIGHT_W, height)
        .content(|ui| {
            let lock = lock_signal(state_store).watch(ui);
            let reveal = reveal_signal(state_store).watch(ui);
            let segment = segment_signal(state_store).watch(ui);
            let dropdown_selected = dropdown_selected_signal(state_store).watch(ui);

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
    ui.stack("lab.overlays")
        .size(screen_width, screen_height)
        .content(|ui| {
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
                .message(
                    "This modal is centered by the widget while the page beneath is pure layout flow.",
                )
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
        });
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

fn dense_row(ui: &mut Ui, index: usize, alarm: f32, density: f32, preset: i32, motion: Transition) {
    let t = index as f32 / 9.0;
    let preset_shift = preset.rem_euclid(3) as f32 * 0.08;
    ui.row(format!("controls.log.row.{index}"))
        .size(Size::fill(), 24.0)
        .gap(8.0)
        .align_items(Align::Center)
        .content(|ui| {
            widgets::badge(ui, format!("controls.log.badge.{index}"))
                .text(format!(
                    "row {:02}",
                    index + 1 + preset.rem_euclid(3) as usize * 10
                ))
                .accent(mix(
                    c(0.280, 0.760, 0.560, 1.0),
                    c(0.980, 0.420, 0.520, 1.0),
                    (alarm * (t + preset_shift)).fract(),
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
                    let fill_width = (48.0 + 118.0 * t + density * 18.0).min(178.0);
                    ui.rect(format!("controls.log.fill.{index}"))
                        .size(178.0, 12.0)
                        .radius(999.0)
                        .color(mix(
                            c(0.260, 0.780, 0.540, 0.78),
                            c(0.950, 0.320, 0.520, 0.84),
                            (alarm * (t + preset_shift)).fract(),
                        ))
                        .scale_xy(fill_width / 178.0, 1.0)
                        .transform_origin(0.0, 0.5)
                        .transition(motion)
                        .animate(AnimProperty::TRANSFORM | AnimProperty::COLOR)
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
        let _frame = ui.clock().frame_index();

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
                            .size(198.0, 5.0)
                            .radius(999.0)
                            .color(severity)
                            .scale_xy(budget, 1.0)
                            .transform_origin(0.0, 0.5)
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

                        ui.text("header.perf.overlay")
                            .size(62.0, 16.0)
                            .text(format!("ov {:>4.1}", perf.overlay_ms))
                            .font_size(11.0)
                            .line_height(16.0)
                            .color(phase_color(perf.overlay_ms))
                            .build();

                        ui.text("header.perf.rest")
                            .size(58.0, 16.0)
                            .text(format!("oth {:>4.1}", perf.rest_ms))
                            .font_size(11.0)
                            .line_height(16.0)
                            .horizontal_align(HorizontalAlign::Right)
                            .color(phase_color(perf.rest_ms))
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
                                .size(48.0, 48.0)
                                .radius(999.0)
                                .color(color)
                                .scale((16.0 + value.clamp(0.0, 1.0) * 32.0) / 48.0)
                                .transform_origin(0.5, 0.5)
                                .transition(motion)
                                .animate(AnimProperty::TRANSFORM | AnimProperty::COLOR)
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
                        .size(Size::fill(), 56.0)
                        .radius(12.0)
                        .color(color)
                        .scale_xy(1.0, (22.0 + value.clamp(0.0, 1.0) * 34.0) / 56.0)
                        .transform_origin(0.5, 1.0)
                        .transition(Transition::ease(0.22, Ease::OutCubic))
                        .animate(AnimProperty::TRANSFORM | AnimProperty::COLOR)
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

#[cfg(feature = "egui")]
#[derive(Debug)]
struct NeoEguiDebug {
    visible: bool,
    element_filter: String,
    draw_filter: String,
    max_rows: usize,
    show_elements: bool,
    show_draw: bool,
    show_events: bool,
}

#[cfg(feature = "egui")]
impl Default for NeoEguiDebug {
    fn default() -> Self {
        Self {
            visible: !env_flag("SKY_NEO_LAB_EGUI_DEBUG_OFF"),
            element_filter: env_string("SKY_NEO_LAB_EGUI_FILTER")
                .unwrap_or_else(|| "controls.preset.dropdown".to_string()),
            draw_filter: env_string("SKY_NEO_LAB_EGUI_DRAW_FILTER")
                .unwrap_or_else(|| "controls.preset.dropdown".to_string()),
            max_rows: 80,
            show_elements: true,
            show_draw: true,
            show_events: true,
        }
    }
}

#[cfg(feature = "egui")]
impl NeoEguiDebug {
    fn show(&mut self, root_ui: &mut sky_engine::app::egui::Ui, data: &NeoDebugPanelData) {
        use sky_engine::app::egui;

        let toggle_requested = root_ui.input(|input| {
            input.key_pressed(egui::Key::F12)
                || (input.modifiers.ctrl && input.key_pressed(egui::Key::D))
        });
        if toggle_requested {
            self.visible = !self.visible;
        }

        if !self.visible {
            egui::Area::new(egui::Id::new("neo_debug_toggle"))
                .order(egui::Order::Foreground)
                .fixed_pos([12.0, 12.0])
                .show(root_ui.ctx(), |ui| {
                    if ui.button("Neo Debug").clicked() {
                        self.visible = true;
                    }
                });
            return;
        }

        egui::Window::new("Neo Debug")
            .default_pos([12.0, 12.0])
            .default_size([560.0, 680.0])
            .resizable(true)
            .show(root_ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.visible, "visible");
                    if ui.button("dropdown").clicked() {
                        self.element_filter = "controls.preset.dropdown".to_string();
                        self.draw_filter = "controls.preset.dropdown".to_string();
                    }
                    if ui.button("popup").clicked() {
                        self.element_filter = "popup".to_string();
                        self.draw_filter = "popup".to_string();
                    }
                    if ui.button("clear").clicked() {
                        self.element_filter.clear();
                        self.draw_filter.clear();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("elements");
                    ui.text_edit_singleline(&mut self.element_filter);
                });
                ui.horizontal(|ui| {
                    ui.label("draw");
                    ui.text_edit_singleline(&mut self.draw_filter);
                });
                ui.add(egui::Slider::new(&mut self.max_rows, 10..=400).text("max rows"));

                ui.separator();
                show_runtime_summary(ui, data);
                ui.separator();

                egui::CollapsingHeader::new("State")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("neo_debug_state")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                row(ui, "control open", data.state.control_dropdown_open);
                                row(ui, "control selected", data.state.control_dropdown_selected);
                                row(ui, "interaction open", data.state.dropdown_open);
                                row(ui, "interaction selected", data.state.dropdown_selected);
                                row(ui, "dialog", data.state.dialog_open);
                                row(ui, "toast", data.state.toast_visible);
                                row(ui, "context", data.state.context_menu_open);
                                row(
                                    ui,
                                    "control scroll",
                                    format!("{:.1}", data.state.control_scroll),
                                );
                            });
                    });

                egui::CollapsingHeader::new(format!("Layers ({})", data.layers.len()))
                    .default_open(true)
                    .show(ui, |ui| show_layers(ui, data));

                egui::CollapsingHeader::new(format!(
                    "Elements ({}/{})",
                    data.elements.len(),
                    data.element_total
                ))
                .default_open(self.show_elements)
                .show(ui, |ui| show_elements(ui, data));

                egui::CollapsingHeader::new(format!(
                    "Draw Order ({}/{})",
                    data.draw.len(),
                    data.draw_total
                ))
                .default_open(self.show_draw)
                .show(ui, |ui| show_draw(ui, data));

                egui::CollapsingHeader::new("Input")
                    .default_open(false)
                    .show(ui, |ui| {
                        egui::Grid::new("neo_debug_input")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                row(ui, "hover", data.hover.as_deref().unwrap_or("-"));
                                row(ui, "active", data.active.as_deref().unwrap_or("-"));
                                row(ui, "capture", data.capture.as_deref().unwrap_or("-"));
                                row(ui, "keyboard", data.keyboard.as_deref().unwrap_or("-"));
                                row(ui, "text", data.text.as_deref().unwrap_or("-"));
                                row(ui, "scroll", data.scroll.as_deref().unwrap_or("-"));
                            });
                    });

                egui::CollapsingHeader::new(format!("Events ({})", data.events.len()))
                    .default_open(self.show_events)
                    .show(ui, |ui| {
                        for event in &data.events {
                            ui.monospace(event);
                        }
                    });

                egui::CollapsingHeader::new(format!(
                    "Invalidations ({})",
                    data.invalidations.len()
                ))
                .default_open(false)
                .show(ui, |ui| {
                    for invalidation in &data.invalidations {
                        ui.monospace(invalidation);
                    }
                });
            });
    }
}

#[cfg(feature = "egui")]
#[derive(Debug, Clone, Copy)]
struct LabDebugState {
    control_dropdown_open: bool,
    control_dropdown_selected: i32,
    dropdown_open: bool,
    dropdown_selected: i32,
    dialog_open: bool,
    toast_visible: bool,
    context_menu_open: bool,
    control_scroll: f32,
}

#[cfg(feature = "egui")]
impl LabDebugState {
    fn from_state(state: &LabState) -> Self {
        Self {
            control_dropdown_open: state.control_dropdown_open,
            control_dropdown_selected: state.control_dropdown_selected,
            dropdown_open: state.dropdown_open,
            dropdown_selected: state.dropdown_selected,
            dialog_open: state.dialog_open,
            toast_visible: state.toast_visible,
            context_menu_open: state.context_menu_open,
            control_scroll: state.control_scroll,
        }
    }
}

#[cfg(feature = "egui")]
#[derive(Debug)]
struct NeoDebugPanelData {
    frame_index: u64,
    screen: String,
    layout_mode: String,
    retained: String,
    render_flags: String,
    pass_flags: String,
    state: LabDebugState,
    layers: Vec<LayerSummary>,
    elements: Vec<ElementSummary>,
    element_total: usize,
    draw: Vec<DrawSummary>,
    draw_total: usize,
    events: Vec<String>,
    invalidations: Vec<String>,
    layer_pointer: Vec<String>,
    hover: Option<String>,
    active: Option<String>,
    capture: Option<String>,
    keyboard: Option<String>,
    text: Option<String>,
    scroll: Option<String>,
}

#[cfg(feature = "egui")]
impl NeoDebugPanelData {
    fn collect(
        runtime: &sky_engine::ui::neo::Runtime,
        state: LabDebugState,
        element_filter: &str,
        draw_filter: &str,
        max_rows: usize,
    ) -> Self {
        let snapshot = runtime.diagnostics().current_snapshot();
        let mut elements = Vec::new();
        let mut element_total = 0;
        for root in runtime.diagnostics().roots() {
            collect_element_summary(
                root,
                None,
                element_filter,
                max_rows,
                &mut element_total,
                &mut elements,
            );
        }

        let draw_list = runtime.draw_list();
        let draw_total = draw_list.commands().len();
        let draw = draw_list
            .commands()
            .iter()
            .enumerate()
            .filter_map(|(index, command)| draw_summary(index, command, draw_filter))
            .take(max_rows)
            .collect();

        Self {
            frame_index: snapshot.frame_index,
            screen: format!(
                "{:.0} x {:.0}",
                snapshot.screen.width, snapshot.screen.height
            ),
            layout_mode: format!("{:?}", snapshot.layout_mode),
            retained: format!(
                "built={} reused={} animations={}",
                snapshot.retained_stats.built,
                snapshot.retained_stats.reused,
                snapshot.active_animation_count
            ),
            render_flags: format!(
                "render={} compose={} full={}",
                snapshot.needs_render, snapshot.needs_compose, snapshot.full_redraw
            ),
            pass_flags: format!("{:?}", snapshot.pass_flags),
            state,
            layers: snapshot
                .layers
                .iter()
                .map(LayerSummary::from_record)
                .collect(),
            elements,
            element_total,
            draw,
            draw_total,
            events: snapshot
                .events
                .iter()
                .map(|event| format!("{:?}", event))
                .take(max_rows)
                .collect(),
            invalidations: snapshot
                .invalidations
                .iter()
                .map(|invalidation| format!("{:?}", invalidation))
                .take(max_rows)
                .collect(),
            layer_pointer: snapshot
                .layer_pointer
                .iter()
                .map(|record| format!("{:?}", record))
                .take(max_rows)
                .collect(),
            hover: snapshot.pointer_hover_id,
            active: snapshot.pointer_active_id,
            capture: snapshot.pointer_capture_id,
            keyboard: snapshot.keyboard_focus_id,
            text: snapshot.text_focus_id,
            scroll: snapshot.scroll_owner_id,
        }
    }
}

#[cfg(feature = "egui")]
#[derive(Debug)]
struct LayerSummary {
    id: String,
    open: bool,
    z_index: i32,
    kind: String,
    action: String,
    anchor_source: String,
    anchor: String,
    outside_click: String,
}

#[cfg(feature = "egui")]
impl LayerSummary {
    fn from_record(record: &sky_engine::ui::neo::expert::LayerDebugRecord) -> Self {
        Self {
            id: record.id.clone(),
            open: record.open,
            z_index: record.z_index,
            kind: format!("{:?}", record.kind),
            action: format!("{:?}", record.action),
            anchor_source: format!("{:?}", record.anchor_source),
            anchor: record.anchor.clone().unwrap_or_else(|| "-".to_string()),
            outside_click: format!("{:?}", record.outside_click),
        }
    }
}

#[cfg(feature = "egui")]
#[derive(Debug)]
struct ElementSummary {
    id: String,
    parent: String,
    kind: String,
    frame: LayoutRect,
    z_index: i32,
    clip: bool,
    interactive: bool,
    focusable: bool,
    text: String,
}

#[cfg(feature = "egui")]
#[derive(Debug)]
struct DrawSummary {
    index: usize,
    kind: &'static str,
    id: String,
    frame: Option<LayoutRect>,
    extra: String,
}

#[cfg(feature = "egui")]
fn collect_element_summary(
    element: &sky_engine::ui::neo::expert::Element,
    parent: Option<&str>,
    filter: &str,
    max_rows: usize,
    total: &mut usize,
    out: &mut Vec<ElementSummary>,
) {
    *total += 1;
    if (filter.is_empty() || element.id.contains(filter)) && out.len() < max_rows {
        out.push(ElementSummary {
            id: element.id.clone(),
            parent: parent.unwrap_or("-").to_string(),
            kind: format!("{:?}", element.kind),
            frame: element.frame,
            z_index: element.z_index,
            clip: element.clip,
            interactive: element.interactive,
            focusable: element.focusable,
            text: element.text.clone(),
        });
    }
    for child in &element.children {
        collect_element_summary(child, Some(&element.id), filter, max_rows, total, out);
    }
}

#[cfg(feature = "egui")]
fn draw_summary(
    index: usize,
    command: &sky_engine::ui::neo::expert::UiDrawCommand,
    filter: &str,
) -> Option<DrawSummary> {
    use sky_engine::ui::neo::expert::UiDrawCommand;
    let (kind, id, frame, extra) = match command {
        UiDrawCommand::Rect(draw) => (
            "rect",
            draw.id.clone(),
            Some(draw.frame),
            format!("opacity={:.2} radius={:.1}", draw.opacity, draw.radius),
        ),
        UiDrawCommand::Text(draw) => (
            "text",
            draw.id.clone(),
            Some(draw.frame),
            format!("opacity={:.2} text={:?}", draw.opacity, draw.text),
        ),
        UiDrawCommand::Image(draw) => (
            "image",
            draw.id.clone(),
            Some(draw.frame),
            format!("opacity={:.2} radius={:.1}", draw.opacity, draw.radius),
        ),
        UiDrawCommand::NineSlice(draw) => (
            "nine",
            draw.id.clone(),
            Some(draw.frame),
            format!("opacity={:.2}", draw.opacity),
        ),
        UiDrawCommand::Polygon(draw) => (
            "poly",
            draw.id.clone(),
            Some(draw.frame),
            format!("opacity={:.2} points={}", draw.opacity, draw.points.len()),
        ),
        UiDrawCommand::PushClip(clip) => (
            "push_clip",
            String::new(),
            Some(clip.rect),
            format!("radius={:.1}", clip.radius),
        ),
        UiDrawCommand::PopClip => ("pop_clip", String::new(), None, String::new()),
    };
    if !filter.is_empty() && !id.contains(filter) {
        return None;
    }
    Some(DrawSummary {
        index,
        kind,
        id,
        frame,
        extra,
    })
}

#[cfg(feature = "egui")]
fn show_runtime_summary(ui: &mut sky_engine::app::egui::Ui, data: &NeoDebugPanelData) {
    sky_engine::app::egui::Grid::new("neo_debug_summary")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            row(ui, "frame", data.frame_index);
            row(ui, "screen", &data.screen);
            row(ui, "layout", &data.layout_mode);
            row(ui, "retained", &data.retained);
            row(ui, "render", &data.render_flags);
            row(ui, "pass", &data.pass_flags);
        });
    if !data.layer_pointer.is_empty() {
        ui.separator();
        for record in &data.layer_pointer {
            ui.monospace(record);
        }
    }
}

#[cfg(feature = "egui")]
fn show_layers(ui: &mut sky_engine::app::egui::Ui, data: &NeoDebugPanelData) {
    sky_engine::app::egui::Grid::new("neo_debug_layers")
        .num_columns(8)
        .striped(true)
        .show(ui, |ui| {
            header(ui, "open");
            header(ui, "z");
            header(ui, "kind");
            header(ui, "action");
            header(ui, "anchor");
            header(ui, "source");
            header(ui, "outside");
            header(ui, "id");
            ui.end_row();
            for layer in &data.layers {
                ui.label(layer.open.to_string());
                ui.monospace(layer.z_index.to_string());
                ui.label(&layer.kind);
                ui.label(&layer.action);
                ui.monospace(&layer.anchor);
                ui.label(&layer.anchor_source);
                ui.label(&layer.outside_click);
                ui.monospace(&layer.id);
                ui.end_row();
            }
        });
}

#[cfg(feature = "egui")]
fn show_elements(ui: &mut sky_engine::app::egui::Ui, data: &NeoDebugPanelData) {
    sky_engine::app::egui::ScrollArea::vertical()
        .max_height(260.0)
        .show(ui, |ui| {
            sky_engine::app::egui::Grid::new("neo_debug_elements")
                .num_columns(9)
                .striped(true)
                .show(ui, |ui| {
                    header(ui, "z");
                    header(ui, "kind");
                    header(ui, "frame");
                    header(ui, "clip");
                    header(ui, "hit");
                    header(ui, "focus");
                    header(ui, "text");
                    header(ui, "parent");
                    header(ui, "id");
                    ui.end_row();
                    for element in &data.elements {
                        ui.monospace(element.z_index.to_string());
                        ui.label(&element.kind);
                        ui.monospace(rect_text(element.frame));
                        ui.label(element.clip.to_string());
                        ui.label(element.interactive.to_string());
                        ui.label(element.focusable.to_string());
                        ui.label(short_text(&element.text, 28));
                        ui.monospace(&element.parent);
                        ui.monospace(&element.id);
                        ui.end_row();
                    }
                });
        });
}

#[cfg(feature = "egui")]
fn show_draw(ui: &mut sky_engine::app::egui::Ui, data: &NeoDebugPanelData) {
    sky_engine::app::egui::ScrollArea::vertical()
        .max_height(260.0)
        .show(ui, |ui| {
            sky_engine::app::egui::Grid::new("neo_debug_draw")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    header(ui, "#");
                    header(ui, "kind");
                    header(ui, "frame");
                    header(ui, "extra");
                    header(ui, "id");
                    ui.end_row();
                    for draw in &data.draw {
                        ui.monospace(draw.index.to_string());
                        ui.label(draw.kind);
                        ui.monospace(draw.frame.map(rect_text).unwrap_or_else(|| "-".to_string()));
                        ui.label(&draw.extra);
                        ui.monospace(&draw.id);
                        ui.end_row();
                    }
                });
        });
}

#[cfg(feature = "egui")]
fn row(ui: &mut sky_engine::app::egui::Ui, label: &str, value: impl ToString) {
    ui.label(label);
    ui.monospace(value.to_string());
    ui.end_row();
}

#[cfg(feature = "egui")]
fn header(ui: &mut sky_engine::app::egui::Ui, label: &str) {
    ui.strong(label);
}

#[cfg(feature = "egui")]
fn rect_text(rect: LayoutRect) -> String {
    format!(
        "{:.1},{:.1} {:.1}x{:.1}",
        rect.x, rect.y, rect.width, rect.height
    )
}

#[cfg(feature = "egui")]
fn short_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
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

fn env_f32(key: &str) -> Option<f32> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn maybe_log_stutter(perf: &PerfSnapshot, compose_ms: f32, overlay_ms: f32) {
    let Some(threshold_ms) = env_f32("SKY_NEO_LAB_STUTTER_LOG_MS") else {
        return;
    };
    if perf.frame_ms < threshold_ms {
        return;
    }

    let rest_ms = (perf.frame_ms - compose_ms - overlay_ms).max(0.0);
    eprintln!(
        "[neo stutter] frame={} frame_ms={:.3} avg_ms={:.3} compose={:.3} overlay={:.3} rest={:.3} lifetime_worst={:.3} trigger={} trigger_age_ms={:.1} threshold={:.3}",
        perf.frame,
        perf.frame_ms,
        perf.avg_ms,
        compose_ms,
        overlay_ms,
        rest_ms,
        perf.lifetime_worst_ms,
        perf.trigger_label,
        perf.trigger_age_ms,
        threshold_ms,
    );
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
    let snapshot = runtime.diagnostics().current_snapshot();
    eprintln!(
        "[neo layout dump] {label}: screen={:?} layout={:?} dirty={:?} live={:?} built={} reused={} active_animations={} needs_render={} needs_compose={} full_redraw={}",
        snapshot.screen,
        snapshot.layout_mode,
        snapshot.dirty_ids,
        snapshot.live_ids,
        snapshot.retained_stats.built,
        snapshot.retained_stats.reused,
        snapshot.active_animation_count,
        snapshot.needs_render,
        snapshot.needs_compose,
        snapshot.full_redraw
    );
    for root in runtime.diagnostics().roots() {
        dump_element_layout(root, 0);
    }
    eprintln!("[neo layout dump end] {label}");
}

fn dump_runtime_draw(label: &str, runtime: &sky_engine::ui::neo::Runtime) {
    let trace = runtime.diagnostics().draw_trace();
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

fn dump_element_layout(element: &sky_engine::ui::neo::expert::Element, depth: usize) {
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
    let snapshot = runtime.diagnostics().current_snapshot();
    let target = runtime
        .diagnostics()
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
        snapshot.dirty_ids,
        snapshot.live_ids,
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
    use sky_engine::ui::neo::{FrameInput, Runtime, Screen, UiTestDriver};

    fn perf_snapshot() -> PerfSnapshot {
        PerfSnapshot {
            frame: 1,
            fps: 60.0,
            frame_ms: 16.0,
            avg_ms: 16.0,
            lifetime_worst_ms: 16.0,
            compose_ms: 0.0,
            overlay_ms: 0.0,
            rest_ms: 0.0,
            stutter_count: 0,
            trigger_stutter_count: 0,
            trigger_active: false,
            trigger_label: "idle".to_string(),
            trigger_age_ms: 0.0,
        }
    }

    #[test]
    fn frame_monitor_accounts_only_measured_ui_phases() {
        let mut monitor = FrameMonitor::default();
        monitor.record_phases(4.0, 3.0);

        let perf = monitor.update(0.020);

        assert!((perf.compose_ms - 4.0).abs() < 0.001, "{perf:?}");
        assert!((perf.overlay_ms - 3.0).abs() < 0.001, "{perf:?}");
        assert!((perf.rest_ms - 13.0).abs() < 0.001, "{perf:?}");
        assert!((perf.frame_ms - 20.0).abs() < 0.001, "{perf:?}");
    }

    fn frame_lab(runtime: &mut Runtime, state: &State<LabState>) {
        let perf = perf_snapshot();
        let compose_state = state.clone();
        runtime.frame_state(
            FrameInput::new(Screen::new(WINDOW_W as f32, WINDOW_H as f32), 0.0),
            state,
            move |ui, screen| {
                draw_lab(
                    ui,
                    screen.width,
                    screen.height,
                    &compose_state,
                    &perf,
                    false,
                    false,
                );
            },
        );
    }

    fn frame_lab_driver(driver: &mut UiTestDriver, state: &State<LabState>) {
        frame_lab_driver_at(driver, state, 0.0);
    }

    fn frame_lab_driver_at(driver: &mut UiTestDriver, state: &State<LabState>, time: f32) {
        let perf = perf_snapshot();
        let compose_state = state.clone();
        driver.frame_state_with_delta(time, state, move |ui, screen| {
            draw_lab(
                ui,
                screen.width,
                screen.height,
                &compose_state,
                &perf,
                false,
                false,
            );
        });
    }

    fn dirty_contains(dirty: &[(String, sky_engine::ui::neo::DirtyFlags)], id: &str) -> bool {
        dirty.iter().any(|(record_id, _)| record_id == id)
    }

    #[test]
    fn signals_tab_click_uses_driver_and_rebuilds_precise_signal_owners() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        frame_lab_driver(&mut driver, &state);
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
        let dirty_ids = state.dirty_flags();
        assert!(
            dirty_contains(&dirty_ids, "stress-lab.signals.tabs")
                && dirty_contains(&dirty_ids, "stress-lab.signals.tab.body"),
            "tab click should dirty only the tab controls and tab body: dirty={dirty_ids:?} trace={trace}"
        );
        frame_lab_driver(&mut driver, &state);

        assert!(driver.runtime().retained_compose_stats().built >= 1);
        assert!(driver.runtime().retained_compose_stats().reused >= 1);
        assert!(driver
            .debug_snapshot()
            .retained_events
            .iter()
            .any(|event| { event.id.as_str() == "stress-lab.signals" }));
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

        frame_lab_driver(&mut driver, &state);
        assert!(
            driver.runtime().retained_compose_stats().partial_layout,
            "stable post-tab frame should return to partial layout: {:?}",
            driver.debug_snapshot()
        );
    }

    #[test]
    fn signals_tab_hit_rect_click_uses_driver_and_rebuilds_precise_signal_owners() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        frame_lab_driver(&mut driver, &state);
        let trace = driver
            .click("signals.tabs.hit.1")
            .expect("charts tab hit rect should exist");

        assert_eq!(state.read(|state| state.tab), 1, "{trace}");
        let dirty_ids = state.dirty_flags();
        assert!(
            dirty_contains(&dirty_ids, "stress-lab.signals.tabs")
                && dirty_contains(&dirty_ids, "stress-lab.signals.tab.body"),
            "tab hit rect should dirty only the tab controls and tab body: dirty={dirty_ids:?} trace={trace}"
        );
        frame_lab_driver(&mut driver, &state);

        assert!(driver.runtime().retained_compose_stats().built >= 1);
        assert!(driver.runtime().retained_compose_stats().reused >= 1);
        assert!(driver
            .debug_snapshot()
            .retained_events
            .iter()
            .any(|event| { event.id.as_str() == "stress-lab.signals" }));
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

        frame_lab_driver_at(&mut driver, &state, 0.0);
        let before_orb_scale = driver
            .find("signals.stage.a.orb.fill")
            .expect("pulse orb fill should exist before tab switch")
            .transform
            .scale[0];

        let trace = driver
            .click("signals.tabs.hit.1")
            .expect("charts tab hit rect should exist");
        assert_eq!(state.read(|state| state.tab), 1, "{trace}");

        frame_lab_driver_at(&mut driver, &state, 0.0);
        assert!(
            driver.find("signals.tab.charts").is_some(),
            "charts body should be present after tab switch: {:?}",
            driver.debug_snapshot()
        );
        driver.advance_animations(1.0);
        let before_bar = rect_draw(driver.runtime(), "signals.chart.bar.bar.3")
            .expect("pulse chart bar should draw after switching to charts")
            .transform
            .scale[1];
        eprintln!(
            "[signal->charts] after click settled: tab={} pulse_bar_scale={before_bar:?} snapshot={:?}",
            state.read(|state| state.tab),
            driver.runtime().diagnostics().current_snapshot()
        );

        frame_lab_driver_at(&mut driver, &state, 1.570_796_4);
        let after_orb = driver
            .find("signals.stage.a.orb.fill")
            .expect("pulse orb fill should still exist on charts tab")
            .frame;
        let after_orb_scale = driver
            .find("signals.stage.a.orb.fill")
            .expect("pulse orb fill should still exist on charts tab")
            .transform
            .scale[0];
        let after_bar_target = driver
            .find("signals.chart.bar.bar.3")
            .expect("pulse chart bar should still exist on charts tab")
            .transform
            .scale[1];
        let after_frame_bar = rect_draw(driver.runtime(), "signals.chart.bar.bar.3")
            .expect("pulse chart bar should draw immediately after live frame")
            .transform
            .scale[1];
        let mut sampled_bars = Vec::new();
        for step in 1..=18 {
            driver.advance_animations(1.0 / 60.0);
            if matches!(step, 1 | 6 | 12 | 18) {
                let scale = rect_draw(driver.runtime(), "signals.chart.bar.bar.3")
                    .expect("pulse chart bar should draw during animation ticks")
                    .transform
                    .scale[1];
                sampled_bars.push((step, scale));
            }
        }
        let after_tick_bar = sampled_bars
            .last()
            .map(|(_, scale)| *scale)
            .expect("pulse chart bar should draw after animation tick");
        let snapshot = driver.debug_snapshot();
        eprintln!(
            "[signal->charts] after live frame: orb_frame={after_orb:?} orb_scale={after_orb_scale:?} pulse_bar_target_scale={after_bar_target:?} pulse_bar_draw_scale={after_frame_bar:?} sampled_ticks={sampled_bars:?} snapshot={:?}",
            driver.runtime().diagnostics().current_snapshot()
        );

        assert!(
            (after_orb.width - 48.0).abs() < 0.001,
            "pulse orb layout should stay stable while visual scale changes: after={after_orb:?} snapshot={snapshot:?}"
        );
        assert!(
            after_orb_scale > before_orb_scale + 0.2,
            "live pulse target scale should advance after switching to charts: before_scale={before_orb_scale:?} after_scale={after_orb_scale:?} snapshot={snapshot:?}"
        );
        assert!(
            after_bar_target > before_bar + 0.2,
            "charts tab should receive live pulse scale values: before_bar_scale={before_bar:?} after_target_scale={after_bar_target:?} snapshot={snapshot:?}"
        );
        assert!(
            after_tick_bar > after_frame_bar,
            "charts bar draw scale should animate after a tick: after_frame={after_frame_bar:?} after_tick={after_tick_bar:?}"
        );
        assert!(
            snapshot
                .dirty_ids
                .iter()
                .any(|id| id == "stress-lab.signals.stage")
                && snapshot
                    .dirty_ids
                    .iter()
                    .any(|id| id == "stress-lab.signals.tab.body"),
            "specific live signal owners should be dirty on the post-switch frame: {snapshot:?}"
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

            frame_lab_driver_at(&mut driver, &state, 0.0);
            driver.advance_animations(0.0);
            let start_draw = rect_draw_x(driver.runtime(), "signals.tabs.indicator")
                .expect("tabs indicator should draw before click");

            let trace = driver
                .click(hit_id)
                .expect("target tab hit rect should exist");
            let dirty_ids = state.dirty_flags();
            assert!(!dirty_ids.is_empty(), "{trace}");
            frame_lab_driver_at(&mut driver, &state, 0.0);
            driver.advance_animations(0.0);
            let after_frame_draw = rect_draw_x(driver.runtime(), "signals.tabs.indicator")
                .expect("tabs indicator should draw after click");
            let target = driver
                .find("signals.tabs.indicator")
                .expect("tabs indicator target should exist after click")
                .frame
                .x;

            driver.advance_animations(1.0 / 60.0);
            let after_tick_draw = rect_draw_x(driver.runtime(), "signals.tabs.indicator")
                .expect("tabs indicator should draw after animation tick");

            eprintln!(
                "[tabs indicator] start_tab={start_tab} hit={hit_id} start_draw={start_draw:.3} after_frame={after_frame_draw:.3} after_tick={after_tick_draw:.3} target={target:.3} snapshot={:?}",
                driver.runtime().diagnostics().current_snapshot()
            );

            (start_draw, after_frame_draw, after_tick_draw)
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

        frame_lab_driver(&mut driver, &state);
        driver.advance_animations(0.0);
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
        let dirty_ids = state.dirty_flags();
        assert!(
            dirty_contains(&dirty_ids, "stress-lab.interactions"),
            "interactions panel should be dirty after segment click: dirty={dirty_ids:?} trace={trace}"
        );

        frame_lab_driver(&mut driver, &state);
        driver.advance_animations(0.0);
        let next_target = driver
            .find("interactions.segment.indicator")
            .expect("segment indicator should exist after click")
            .frame;
        let after_frame_draw = rect_draw_x(driver.runtime(), "interactions.segment.indicator")
            .expect("segment indicator should draw after click");
        let snapshot = driver.runtime().diagnostics().current_snapshot();

        assert!(
            next_target.x > start_target.x,
            "indicator target should move from mild to odd: start={start_target:?} next={next_target:?}"
        );
        assert!(
            after_frame_draw < next_target.x,
            "drawn indicator should initially lag behind target while animating: draw={after_frame_draw} target={}",
            next_target.x
        );
        assert!(
            snapshot.active_animation_count > 0,
            "segment click should leave active animations: {snapshot:?}"
        );

        driver.advance_animations(1.0 / 60.0);
        let after_tick_draw = rect_draw_x(driver.runtime(), "interactions.segment.indicator")
            .expect("segment indicator should draw after tick");
        assert!(
            after_tick_draw > after_frame_draw,
            "indicator draw frame should move forward after a tick: before={after_frame_draw} after={after_tick_draw}"
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

        frame_lab_driver(&mut driver, &state);
        driver.advance_animations(0.0);
        log_segment_draw_state("before click", &driver);

        let trace = driver
            .click("interactions.segment.hit.1")
            .expect("odd segment hit rect should exist");
        eprintln!("{trace}");
        eprintln!(
            "state.segment after click = {}",
            state.read(|state| state.segment)
        );

        let dirty_ids = state.dirty_flags();
        eprintln!("dirty records after click = {dirty_ids:?}");
        frame_lab_driver(&mut driver, &state);
        driver.advance_animations(0.0);
        log_segment_draw_state("after compose", &driver);

        for step in 1..=12 {
            driver.advance_animations(1.0 / 60.0);
            if matches!(step, 1 | 3 | 6 | 12) {
                log_segment_draw_state(&format!("after tick {step}"), &driver);
            }
        }

        assert_eq!(state.read(|state| state.segment), 1);
        for _ in 0..48 {
            driver.advance_animations(1.0 / 60.0);
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

    #[test]
    fn stress_lab_dropdowns_open_and_select_through_root_popovers() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        frame_lab_driver(&mut driver, &state);

        let control_trace = driver
            .click("controls.preset.dropdown.field")
            .expect("control dropdown field should be clickable");
        assert!(
            state.read(|state| state.control_dropdown_open),
            "{control_trace}"
        );
        frame_lab_driver(&mut driver, &state);
        assert!(
            driver.find("controls.preset.dropdown.item.1").is_some(),
            "control dropdown popup items should exist after opening: {:?}",
            driver.debug_snapshot()
        );

        let control_item_trace = driver
            .click("controls.preset.dropdown.item.2")
            .expect("control dropdown popup item should be clickable");
        assert_eq!(
            state.read(|state| state.control_dropdown_selected),
            2,
            "{control_item_trace}"
        );
        assert!(
            !state.read(|state| state.control_dropdown_open),
            "{control_item_trace}"
        );

        frame_lab_driver(&mut driver, &state);
        let quiet_label = driver
            .find("controls.preset.dropdown.label")
            .expect("control dropdown label should exist after selecting Quiet");
        assert_eq!(quiet_label.text, "Quiet");
        let quiet_badge = driver
            .find("controls.log.badge.0.text")
            .expect("first control log badge should exist after selecting Quiet");
        assert_eq!(quiet_badge.text, "row 21");

        let reopen_trace = driver
            .click("controls.preset.dropdown.field")
            .expect("control dropdown should reopen after selecting Quiet");
        assert!(
            state.read(|state| state.control_dropdown_open),
            "{reopen_trace}"
        );
        frame_lab_driver(&mut driver, &state);
        let drift_trace = driver
            .click("controls.preset.dropdown.item.0")
            .expect("Drift dropdown item should be clickable");
        assert_eq!(
            state.read(|state| state.control_dropdown_selected),
            0,
            "{drift_trace}"
        );
        assert!(
            !state.read(|state| state.control_dropdown_open),
            "{drift_trace}"
        );
        frame_lab_driver(&mut driver, &state);
        let drift_label = driver
            .find("controls.preset.dropdown.label")
            .expect("control dropdown label should exist after selecting Drift");
        assert_eq!(drift_label.text, "Drift");
        let drift_badge = driver
            .find("controls.log.badge.0.text")
            .expect("first control log badge should exist after selecting Drift");
        assert_eq!(drift_badge.text, "row 01");

        frame_lab_driver(&mut driver, &state);
        let interaction_trace = driver
            .click("interactions.dropdown.field")
            .expect("interaction dropdown field should be clickable");
        assert!(
            state.read(|state| state.dropdown_open),
            "{interaction_trace}"
        );
        frame_lab_driver(&mut driver, &state);
        let interaction_field = driver
            .find("interactions.dropdown.field")
            .expect("interaction dropdown field should stay in its panel")
            .frame;
        let interaction_popup = driver
            .find("interactions.dropdown.popup")
            .expect("interaction dropdown popup root should exist after opening")
            .frame;
        assert!(
            (interaction_popup.x - interaction_field.x).abs() < 0.001,
            "interaction dropdown popup should stay in the field column: field={interaction_field:?} popup={interaction_popup:?}"
        );
        assert!(
            (interaction_popup.y - (interaction_field.bottom() + 8.0)).abs() < 0.001,
            "interaction dropdown popup should sit below its field: field={interaction_field:?} popup={interaction_popup:?}"
        );
        assert!(
            (interaction_popup.width - interaction_field.width).abs() < 0.001,
            "interaction dropdown popup should keep field width: field={interaction_field:?} popup={interaction_popup:?}"
        );
        assert!(
            driver.find("interactions.dropdown.item.1").is_some(),
            "interaction dropdown popup items should exist after opening: {:?}",
            driver.debug_snapshot()
        );

        let interaction_item_trace = driver
            .click("interactions.dropdown.item.1")
            .expect("interaction dropdown popup item should be clickable");
        assert_eq!(
            state.read(|state| state.dropdown_selected),
            1,
            "{interaction_item_trace}"
        );
        assert!(
            !state.read(|state| state.dropdown_open),
            "{interaction_item_trace}"
        );
    }

    #[test]
    fn stress_lab_control_dropdown_stays_open_after_app_style_pointer_frame() {
        let state = State::new(LabState::default());
        let mut runtime = Runtime::new("neo");

        frame_lab(&mut runtime, &state);
        let field = runtime
            .diagnostics()
            .find("controls.preset.dropdown.field")
            .expect("control dropdown field should exist")
            .frame;
        let x = field.x + field.width * 0.5;
        let y = field.y + field.height * 0.5;

        let perf = perf_snapshot();
        let compose_state = state.clone();
        runtime.frame_state(
            FrameInput::new(Screen::new(WINDOW_W as f32, WINDOW_H as f32), 0.0).pointer_events([
                PointerEvent::pressed_at(x, y),
                PointerEvent::released_at(x, y),
            ]),
            &state,
            move |ui, screen| {
                draw_lab(
                    ui,
                    screen.width,
                    screen.height,
                    &compose_state,
                    &perf,
                    false,
                    false,
                );
            },
        );

        assert!(state.read(|state| state.control_dropdown_open));
        assert!(
            runtime
                .diagnostics()
                .find("controls.preset.dropdown.popup.surface")
                .is_some(),
            "popup should exist immediately after app-style pointer frame: {:?}",
            runtime.diagnostics().current_snapshot()
        );

        let perf = perf_snapshot();
        let compose_state = state.clone();
        runtime.frame_state(
            FrameInput::new(Screen::new(WINDOW_W as f32, WINDOW_H as f32), 0.0)
                .pointer(PointerEvent::at(x, y)),
            &state,
            move |ui, screen| {
                draw_lab(
                    ui,
                    screen.width,
                    screen.height,
                    &compose_state,
                    &perf,
                    false,
                    false,
                );
            },
        );

        assert!(state.read(|state| state.control_dropdown_open));
        assert!(
            runtime
                .diagnostics()
                .find("controls.preset.dropdown.popup.surface")
                .is_some(),
            "popup should survive an idle app redraw at the field pointer: {:?}",
            runtime.diagnostics().current_snapshot()
        );
    }

    #[test]
    fn stress_lab_input_accepts_text_after_focus_click() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        frame_lab_driver(&mut driver, &state);
        let focus_trace = driver
            .click("interactions.input.hit")
            .expect("input hit rect should be clickable");
        assert_eq!(
            driver.runtime().diagnostics().focused_id(),
            Some("stress-lab.interactions.input.hit"),
            "{focus_trace}"
        );

        let type_trace = driver
            .type_text("signal")
            .expect("focused input should accept keyboard text");
        assert_eq!(state.read(|state| state.input_text.clone()), "EUIsignal");
        let dirty_ids = state.dirty_flags();
        assert!(
            dirty_contains(&dirty_ids, "stress-lab.interactions.input"),
            "typing should dirty the input owner: dirty={dirty_ids:?} trace={type_trace}"
        );
        frame_lab_driver(&mut driver, &state);

        let text = driver
            .find("interactions.input.text")
            .expect("input text element should exist");
        assert_eq!(text.text, "EUIsignal");
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
        let snapshot = runtime.diagnostics().current_snapshot();
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
    fn interaction_scroll_moves_active_and_live_probe_cards() {
        let state = State::new(LabState::default());
        let mut driver = UiTestDriver::new("stress-lab", WINDOW_W as f32, WINDOW_H as f32);

        frame_lab_driver(&mut driver, &state);
        frame_lab_driver(&mut driver, &state);
        driver.advance_animations(0.0);

        let active_before = driver
            .element_frame("interactions.locked")
            .expect("active card should exist before scroll");
        let probe_before = driver
            .element_frame("interactions.secret")
            .expect("live probe card should exist before scroll");
        let active_draw_before = rect_draw(driver.runtime(), "interactions.locked.bg")
            .expect("active card background should draw before scroll")
            .frame;
        let probe_draw_before = rect_draw(driver.runtime(), "interactions.secret.bg")
            .expect("live probe background should draw before scroll")
            .frame;

        let trace = driver
            .scroll("interactions.scroll.viewport", 0.0, -2.0)
            .expect("interaction viewport should receive scroll");
        let dirty_ids = state.dirty_flags();
        eprintln!(
            "[interaction scroll] trace={trace}\n  dirty={dirty_ids:?}\n  offset={}",
            state.read(|state| state.interaction_scroll)
        );
        assert!(
            dirty_contains(&dirty_ids, "stress-lab.interactions.scroll"),
            "scrolling should dirty the interaction scroll owner: dirty={dirty_ids:?} trace={trace}"
        );

        frame_lab_driver(&mut driver, &state);
        driver.advance_animations(0.0);

        let active_after = driver
            .element_frame("interactions.locked")
            .expect("active card should exist after scroll");
        let probe_after = driver
            .element_frame("interactions.secret")
            .expect("live probe card should exist after scroll");
        let active_draw_after = rect_draw(driver.runtime(), "interactions.locked.bg")
            .expect("active card background should draw after scroll")
            .frame;
        let probe_draw_after = rect_draw(driver.runtime(), "interactions.secret.bg")
            .expect("live probe background should draw after scroll")
            .frame;
        let snapshot = driver.runtime().diagnostics().current_snapshot();
        eprintln!(
            "[interaction scroll] active target {active_before:?} -> {active_after:?}, draw {active_draw_before:?} -> {active_draw_after:?}\n  probe target {probe_before:?} -> {probe_after:?}, draw {probe_draw_before:?} -> {probe_draw_after:?}\n  normalized={:?} layout={:?}",
            snapshot.normalized_dirty_ids,
            snapshot.layout_mode
        );

        assert!(
            active_after.y < active_before.y - 1.0,
            "active card target frame should move upward when scrolled down: before={active_before:?} after={active_after:?} snapshot={snapshot:?}"
        );
        assert!(
            probe_after.y < probe_before.y - 1.0,
            "live probe target frame should move upward when scrolled down: before={probe_before:?} after={probe_after:?} snapshot={snapshot:?}"
        );
        assert!(
            active_draw_after.y < active_draw_before.y - 1.0,
            "active card draw frame should move upward when scrolled down: before={active_draw_before:?} after={active_draw_after:?} snapshot={snapshot:?}"
        );
        assert!(
            probe_draw_after.y < probe_draw_before.y - 1.0,
            "live probe draw frame should move upward when scrolled down: before={probe_draw_before:?} after={probe_draw_after:?} snapshot={snapshot:?}"
        );
    }

    #[test]
    fn live_ids_rebuild_without_state_dirty() {
        let state = State::new(LabState::default());
        let mut runtime = Runtime::new("stress-lab");

        frame_lab(&mut runtime, &state);
        frame_lab(&mut runtime, &state);

        assert!(runtime.retained_compose_stats().built >= 2);
        assert!(runtime.retained_compose_stats().reused >= 1);
    }
}
