//! Visual stress lab for the EUI-NEO-style UI backend.
//!
//! ```bash
//! cargo run --example ui_neo_stress_lab --features ui-neo --release
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
use sky_engine::ui::neo::{
    AnimProperty, Binding, Color, Ease, HorizontalAlign, NeoState, Transition, Ui, VerticalAlign,
};

const WINDOW_W: u32 = 1180;
const WINDOW_H: u32 = 760;

#[derive(Debug)]
struct WeirdNeoLab {
    time: f32,
    state: NeoState<LabState>,
    screenshot: ScreenshotProbe,
}

#[derive(Debug)]
struct LabState {
    clicks: u32,
    mode: i32,
    wobble: f32,
    chaos: f32,
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
    scroll_offset: f32,
}

impl Default for WeirdNeoLab {
    fn default() -> Self {
        Self {
            time: 0.0,
            state: NeoState::new(LabState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl Default for LabState {
    fn default() -> Self {
        Self {
            clicks: 0,
            mode: 0,
            wobble: 0.42,
            chaos: 0.68,
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
            context_menu_position: [820.0, 300.0],
            input_text: "EUI".to_string(),
            scroll_offset: 0.0,
        }
    }
}

impl AppState for WeirdNeoLab {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.018, 0.022, 0.03).into(),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(760.0),
            MainCamera,
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.time += ctx.dt();
        self.draw_ui(ctx);
        let (mode, clicks) = self.state.read(|state| (state.mode, state.clicks));
        ctx.set_title(&format!(
            "SkyEngine - Weird Neo Lab | mode {} | clicks {}",
            mode, clicks
        ));

        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        if self.screenshot.active() {
            ctx.request_redraw();
        }
    }
}

impl WeirdNeoLab {
    fn draw_ui(&mut self, ctx: &mut FrameContext<'_>) {
        let time = self.time;
        let state = self.state.read(LabSnapshot::from_state);
        let state_store = self.state.clone();
        let pointer_owned = ctx.ui().wants_pointer();
        let keyboard_owned = ctx.ui().wants_keyboard();

        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            draw_lab(
                ui,
                screen.width,
                screen.height,
                time,
                &state_store,
                &state,
                pointer_owned,
                keyboard_owned,
            );
        });
    }
}

#[derive(Debug, Clone)]
struct LabSnapshot {
    clicks: u32,
    mode: i32,
    wobble: f32,
    chaos: f32,
    alarm: f32,
    glass: bool,
    lock: bool,
    reveal: bool,
    context_menu_position: [f32; 2],
    scroll_offset: f32,
}

impl LabSnapshot {
    fn from_state(value: &LabState) -> Self {
        Self {
            clicks: value.clicks,
            mode: value.mode,
            wobble: value.wobble,
            chaos: value.chaos,
            alarm: value.alarm,
            glass: value.glass,
            lock: value.lock,
            reveal: value.reveal,
            context_menu_position: value.context_menu_position,
            scroll_offset: value.scroll_offset,
        }
    }
}

fn bind_wobble(state: &NeoState<LabState>) -> Binding<LabState, f32> {
    state.bind(
        |state| state.wobble,
        |state, value| state.wobble = value.clamp(0.0, 1.0),
    )
}

fn bind_chaos(state: &NeoState<LabState>) -> Binding<LabState, f32> {
    state.bind(
        |state| state.chaos,
        |state, value| state.chaos = value.clamp(0.0, 1.0),
    )
}

fn bind_alarm(state: &NeoState<LabState>) -> Binding<LabState, f32> {
    state.bind(
        |state| state.alarm,
        |state, value| state.alarm = value.clamp(0.0, 1.0),
    )
}

fn bind_glass(state: &NeoState<LabState>) -> Binding<LabState, bool> {
    state.bind(|state| state.glass, |state, value| state.glass = value)
}

fn bind_lock(state: &NeoState<LabState>) -> Binding<LabState, bool> {
    state.bind(|state| state.lock, |state, value| state.lock = value)
}

fn bind_reveal(state: &NeoState<LabState>) -> Binding<LabState, bool> {
    state.bind(|state| state.reveal, |state, value| state.reveal = value)
}

fn bind_tab(state: &NeoState<LabState>) -> Binding<LabState, i32> {
    state.bind(|state| state.tab, |state, value| state.tab = value.max(0))
}

fn bind_segment(state: &NeoState<LabState>) -> Binding<LabState, i32> {
    state.bind(
        |state| state.segment,
        |state, value| state.segment = value.max(0),
    )
}

fn bind_radio_value(state: &NeoState<LabState>, value: i32) -> Binding<LabState, bool> {
    state.bind(
        move |state| state.radio == value,
        move |state, selected| {
            if selected {
                state.radio = value.max(0);
            }
        },
    )
}

fn bind_dropdown_open(state: &NeoState<LabState>) -> Binding<LabState, bool> {
    state.bind(
        |state| state.dropdown_open,
        |state, value| state.dropdown_open = value,
    )
}

fn bind_dropdown_selected(state: &NeoState<LabState>) -> Binding<LabState, i32> {
    state.bind(
        |state| state.dropdown_selected,
        |state, value| state.dropdown_selected = value.max(0),
    )
}

fn bind_dialog_open(state: &NeoState<LabState>) -> Binding<LabState, bool> {
    state.bind(
        |state| state.dialog_open,
        |state, value| state.dialog_open = value,
    )
}

fn bind_toast_visible(state: &NeoState<LabState>) -> Binding<LabState, bool> {
    state.bind(
        |state| state.toast_visible,
        |state, value| state.toast_visible = value,
    )
}

fn bind_context_menu_open(state: &NeoState<LabState>) -> Binding<LabState, bool> {
    state.bind(
        |state| state.context_menu_open,
        |state, value| state.context_menu_open = value,
    )
}

fn bind_scroll_offset(state: &NeoState<LabState>) -> Binding<LabState, f32> {
    state.bind(
        |state| state.scroll_offset,
        |state, value| state.scroll_offset = value.clamp(0.0, 240.0),
    )
}

fn bind_input_text(state: &NeoState<LabState>) -> Binding<LabState, String> {
    state.bind_clone(
        |state| state.input_text.clone(),
        |state, value| state.input_text = value,
    )
}

fn draw_lab(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    time: f32,
    state_store: &NeoState<LabState>,
    state: &LabSnapshot,
    pointer_owned: bool,
    keyboard_owned: bool,
) {
    let pulse = time.sin() * 0.5 + 0.5;
    let scan = (time * (0.18 + state.chaos * 0.9)).fract();
    let motion = Transition::make(0.24, Ease::OutCubic);

    ui.stack("root")
        .size(screen_width, screen_height)
        .clip()
        .content(|ui| {
            draw_background(ui, screen_width, screen_height, time, state.alarm);
            draw_top_bar(ui, state_store, screen_width, state, scan, motion);

            ui.row("body")
                .x(22.0)
                .y(104.0)
                .size(
                    (screen_width - 44.0).max(0.0),
                    (screen_height - 126.0).max(0.0),
                )
                .gap(16.0)
                .content(|ui| {
                    draw_control_cabinet(ui, state_store, state, motion);
                    draw_stacking_trap(ui, state_store, time, pulse, scan, state, motion);
                    draw_oddities(
                        ui,
                        state_store,
                        time,
                        pulse,
                        pointer_owned,
                        keyboard_owned,
                        state,
                    );
                });

            draw_lab_overlays(ui, screen_width, screen_height, state_store, state);

            ui.text("corner.readout")
                .x((screen_width - 276.0).max(0.0))
                .y((screen_height - 30.0).max(0.0))
                .size(250.0, 22.0)
                .text(format!(
                    "pointer {}   keyboard {}",
                    on_off(pointer_owned),
                    on_off(keyboard_owned)
                ))
                .font_size(13.0)
                .line_height(16.0)
                .color(c(0.64, 0.74, 0.86, 0.74))
                .horizontal_align(HorizontalAlign::Right)
                .build();
        });
}

fn draw_lab_overlays(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    state_store: &NeoState<LabState>,
    state: &LabSnapshot,
) {
    widgets::context_menu(ui, "strange.context")
        .open_bind(bind_context_menu_open(state_store))
        .screen(screen_width, screen_height)
        .position(
            state.context_menu_position[0],
            state.context_menu_position[1],
        )
        .items(["Invert Mood", "Summon Dialog", "Hide Menu"])
        .on_dismiss({
            let open = bind_context_menu_open(state_store);
            move || open.set(false)
        })
        .on_select({
            let state = state_store.clone();
            move |index| {
                state.update(|state| {
                    if index == 0 {
                        state.mode = (state.mode + 1).rem_euclid(4);
                    }
                    if index == 1 {
                        state.toast_visible = true;
                    }
                    if index == 2 {
                        state.context_menu_open = false;
                    }
                });
            }
        })
        .build();

    widgets::dialog(ui, "strange.dialog")
        .open_bind(bind_dialog_open(state_store))
        .screen(screen_width, screen_height)
        .title("Odd Confirmation")
        .message("This modal is intentionally too calm for the rest of the lab.")
        .primary_text("Proceed")
        .secondary_text("Back away")
        .on_primary({
            let state = state_store.clone();
            move || {
                state.update(|state| {
                    state.dialog_open = false;
                    state.toast_visible = true;
                });
            }
        })
        .on_secondary({
            let dialog = bind_dialog_open(state_store);
            move || dialog.set(false)
        })
        .on_close({
            let dialog = bind_dialog_open(state_store);
            move || dialog.set(false)
        })
        .build();

    widgets::toast(ui, "strange.toast")
        .visible_bind(bind_toast_visible(state_store))
        .screen(screen_width, screen_height)
        .title("Signal captured")
        .message("Neo widgets survived another frame of suspicious layout.")
        .duration(2.4)
        .on_dismiss({
            let visible = bind_toast_visible(state_store);
            move || visible.set(false)
        })
        .on_auto_dismiss({
            let visible = bind_toast_visible(state_store);
            move || visible.set(false)
        })
        .build();
}

fn draw_background(ui: &mut Ui, width: f32, height: f32, time: f32, alarm: f32) {
    ui.rect("bg")
        .size(width, height)
        .gradient(c(0.015, 0.019, 0.030, 1.0), c(0.050, 0.035, 0.070, 1.0))
        .build();

    for index in 0..8 {
        let t = index as f32 / 7.0;
        let x = 40.0 + t * (width - 120.0).max(0.0);
        let y = 60.0 + ((time * 0.7 + t * 6.0).sin() * 0.5 + 0.5) * (height - 140.0).max(0.0);
        ui.rect(format!("bg.spark.{index}"))
            .x(x)
            .y(y)
            .size(38.0 + 80.0 * t, 2.0 + alarm * 5.0)
            .color(mix(
                c(0.22, 0.62, 0.96, 0.22),
                c(0.96, 0.32, 0.46, 0.42),
                alarm,
            ))
            .radius(999.0)
            .opacity(0.35 + alarm * 0.35)
            .build();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_top_bar(
    ui: &mut Ui,
    state_store: &NeoState<LabState>,
    width: f32,
    state: &LabSnapshot,
    scan: f32,
    motion: Transition,
) {
    let panel_width = (width - 44.0).max(0.0);
    widgets::panel(ui, "top.panel")
        .x(22.0)
        .y(20.0)
        .size(panel_width, 68.0)
        .gradient(c(0.075, 0.095, 0.135, 0.94), c(0.032, 0.038, 0.056, 0.96))
        .border(1.0, c(0.28, 0.38, 0.50, 0.82))
        .shadow(26.0, 0.0, 9.0, c(0.0, 0.0, 0.0, 0.30))
        .radius(16.0)
        .build();

    ui.text("top.title")
        .x(44.0)
        .y(31.0)
        .size(300.0, 34.0)
        .text("WEIRD NEO LAB")
        .font_size(28.0)
        .line_height(34.0)
        .color(c(0.94, 0.97, 1.0, 1.0))
        .build();

    ui.text("top.status")
        .x(44.0)
        .y(62.0)
        .size(700.0, 18.0)
        .text(format!(
            "mode {}  clicks {}  wobble {:>3}%  chaos {:>3}%  alarm {:>3}%  glass {}  lock {}  reveal {}",
            state.mode,
            state.clicks,
            (state.wobble * 100.0).round() as i32,
            (state.chaos * 100.0).round() as i32,
            (state.alarm * 100.0).round() as i32,
            on_off(state.glass),
            on_off(state.lock),
            on_off(state.reveal)
        ))
        .font_size(13.0)
        .line_height(16.0)
        .color(c(0.66, 0.74, 0.86, 0.88))
        .build();

    ui.rect("top.scan.track")
        .x((width - 435.0).max(0.0))
        .y(46.0)
        .size(210.0, 14.0)
        .color(c(0.08, 0.12, 0.18, 0.92))
        .radius(999.0)
        .build();
    ui.rect("top.scan.fill")
        .x((width - 435.0).max(0.0))
        .y(46.0)
        .size((210.0 * scan.max(0.06)).max(8.0), 14.0)
        .color(mix(
            c(0.28, 0.80, 0.56, 1.0),
            c(1.0, 0.45, 0.34, 1.0),
            state.alarm,
        ))
        .radius(999.0)
        .transition(motion)
        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
        .build();

    ui.stack("top.mode.slot")
        .x((width - 210.0).max(0.0))
        .y(33.0)
        .size(72.0, 40.0)
        .content(|ui| {
            widgets::button(ui, "top.mode")
                .size(72.0, 40.0)
                .text("Mode")
                .font_size(14.0)
                .radius(11.0)
                .on_click({
                    let state = state_store.clone();
                    move || {
                        state.update(|state| {
                            state.clicks = state.clicks.wrapping_add(1);
                            state.mode = (state.mode + 1).rem_euclid(4);
                        });
                    }
                })
                .build();
        });

    ui.stack("top.panic.slot")
        .x((width - 130.0).max(0.0))
        .y(33.0)
        .size(82.0, 40.0)
        .content(|ui| {
            widgets::button(ui, "top.panic")
                .size(82.0, 40.0)
                .text("Panic")
                .font_size(14.0)
                .colors(
                    c(0.66, 0.20, 0.30, 1.0),
                    c(0.86, 0.28, 0.42, 1.0),
                    c(0.44, 0.12, 0.20, 1.0),
                )
                .radius(11.0)
                .on_click({
                    let state = state_store.clone();
                    move || {
                        state.update(|state| {
                            state.clicks = state.clicks.wrapping_add(1);
                            state.alarm = (state.alarm + 0.22).fract();
                        });
                    }
                })
                .build();
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_control_cabinet(
    ui: &mut Ui,
    state_store: &NeoState<LabState>,
    state: &LabSnapshot,
    motion: Transition,
) {
    let width = 330.0;
    let height = 560.0;
    widgets::panel(ui, "cabinet.panel")
        .size(width, height)
        .gradient(c(0.070, 0.086, 0.120, 0.94), c(0.030, 0.038, 0.055, 0.96))
        .border(1.0, c(0.25, 0.34, 0.46, 0.72))
        .shadow(22.0, 0.0, 7.0, c(0.0, 0.0, 0.0, 0.24))
        .radius(14.0)
        .build();

    ui.column("cabinet.content")
        .x(16.0)
        .y(16.0)
        .size(292.0, 410.0)
        .gap(12.0)
        .content(|ui| {
            section_title(ui, "cabinet.title", "Control Cabinet", 292.0);
            slider_row(
                ui,
                "cabinet.wobble",
                "Wobble",
                state.wobble,
                c(0.34, 0.58, 0.98, 1.0),
                bind_wobble(state_store),
            );
            slider_row(
                ui,
                "cabinet.chaos",
                "Chaos",
                state.chaos,
                c(0.28, 0.85, 0.58, 1.0),
                bind_chaos(state_store),
            );
            slider_row(
                ui,
                "cabinet.alarm",
                "Alarm",
                state.alarm,
                c(1.0, 0.62, 0.30, 1.0),
                bind_alarm(state_store),
            );

            widgets::switch(ui, "cabinet.glass")
                .size(230.0, 30.0)
                .checked_bind(bind_glass(state_store))
                .text("Glass Tint")
                .build();
            widgets::checkbox(ui, "cabinet.lock")
                .size(230.0, 30.0)
                .checked_bind(bind_lock(state_store))
                .text("Disable Odd Panel")
                .build();
            widgets::checkbox(ui, "cabinet.reveal")
                .size(230.0, 30.0)
                .checked_bind(bind_reveal(state_store))
                .text("Reveal Secret")
                .build();

            ui.row("cabinet.radios")
                .size(292.0, 32.0)
                .gap(10.0)
                .content(|ui| {
                    radio_item(ui, state_store, "cabinet.radio.a", "A", 0);
                    radio_item(ui, state_store, "cabinet.radio.b", "B", 1);
                    radio_item(ui, state_store, "cabinet.radio.c", "C", 2);
                });
        });

    let viewport_h = 96.0;
    let content_h = 220.0;
    ui.stack("cabinet.scroll.viewport")
        .x(16.0)
        .y(438.0)
        .size(278.0, viewport_h)
        .clip()
        .content(|ui| {
            ui.column("cabinet.scroll.content")
                .y(-state.scroll_offset)
                .size(260.0, content_h)
                .gap(8.0)
                .content(|ui| {
                    for index in 0..8 {
                        dense_row(ui, index, state.alarm, state.chaos, motion);
                    }
                });
        });

    widgets::scrollbar(ui, "cabinet.scrollbar")
        .x(300.0)
        .y(438.0)
        .size(8.0, viewport_h)
        .offset_bind(bind_scroll_offset(state_store))
        .viewport(viewport_h)
        .content(content_h)
        .build();
}

#[allow(clippy::too_many_arguments)]
fn draw_stacking_trap(
    ui: &mut Ui,
    state_store: &NeoState<LabState>,
    time: f32,
    pulse: f32,
    scan: f32,
    state: &LabSnapshot,
    motion: Transition,
) {
    let width = 360.0;
    let height = 560.0;
    widgets::panel(ui, "stack.panel")
        .size(width, height)
        .gradient(c(0.060, 0.075, 0.110, 0.93), c(0.024, 0.030, 0.050, 0.96))
        .border(1.0, c(0.23, 0.33, 0.49, 0.70))
        .shadow(24.0, 0.0, 8.0, c(0.0, 0.0, 0.0, 0.26))
        .radius(14.0)
        .build();

    ui.column("stack.content")
        .x(16.0)
        .y(16.0)
        .size(328.0, 528.0)
        .gap(13.0)
        .content(|ui| {
            section_title(ui, "stack.title", "Stacking Context Trap", 328.0);

            ui.stack("stack.stage")
                .size(328.0, 196.0)
                .clip()
                .content(|ui| {
                    ui.rect("stack.base")
                        .x(8.0)
                        .y(12.0)
                        .size(210.0, 136.0)
                        .color(c(0.24, 0.32, 0.55, 0.86))
                        .radius(16.0)
                        .build();
                    ui.rect("stack.gold")
                        .x(96.0 + time.cos() * (18.0 + state.wobble * 22.0))
                        .y(52.0)
                        .size(132.0, 50.0)
                        .color(c(1.0, 0.76, 0.34, 0.92))
                        .radius(12.0)
                        .transition(motion)
                        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                        .build();
                    ui.rect("stack.teal")
                        .x(122.0)
                        .y(74.0)
                        .size(188.0, 104.0)
                        .color(c(0.10, 0.70, 0.62, 0.82))
                        .radius(18.0)
                        .build();
                    ui.rect("stack.wobble")
                        .x(62.0 + time.sin() * (12.0 + state.wobble * 34.0))
                        .y(125.0 + time.cos() * (8.0 + state.chaos * 20.0))
                        .size(146.0, 42.0)
                        .color(mix(
                            c(0.92, 0.26, 0.46, 0.92),
                            c(1.0, 0.62, 0.30, 0.95),
                            state.alarm,
                        ))
                        .radius(999.0)
                        .rotate((time * 0.8).sin() * 0.10 * state.chaos)
                        .transform_origin(0.5, 0.5)
                        .transition(motion)
                        .animate(
                            AnimProperty::FRAME | AnimProperty::TRANSFORM | AnimProperty::COLOR,
                        )
                        .build();
                    ui.text("stack.chip.text")
                        .x(86.0)
                        .y(136.0)
                        .size(110.0, 20.0)
                        .text("wobble chip")
                        .font_size(14.0)
                        .line_height(17.0)
                        .color(c(1.0, 0.96, 0.98, 0.94))
                        .horizontal_align(HorizontalAlign::Center)
                        .build();
                });

            section_title(ui, "stack.meter.title", "Fill Weights And Bars", 328.0);
            ui.row("stack.weights")
                .size(328.0, 32.0)
                .gap(8.0)
                .content(|ui| {
                    meter_block(ui, "stack.weight.a", 54.0, c(0.30, 0.43, 0.72, 0.94));
                    meter_block(ui, "stack.weight.b", 100.0, c(0.24, 0.65, 0.52, 0.94));
                    meter_block(ui, "stack.weight.c", 148.0, c(0.72, 0.42, 0.60, 0.94));
                });

            meter_row(
                ui,
                "stack.blue",
                "blue fill",
                (pulse * 0.55 + state.wobble * 0.45).fract(),
                c(0.33, 0.64, 0.98, 1.0),
            );
            meter_row(
                ui,
                "stack.green",
                "green fill",
                (scan + state.chaos * 0.25).fract(),
                c(0.28, 0.85, 0.58, 1.0),
            );
            meter_row(
                ui,
                "stack.alarm",
                "alarm fill",
                state.alarm.max((1.0 - pulse) * 0.35),
                c(1.0, 0.62, 0.30, 1.0),
            );

            widgets::button(ui, "stack.context.button")
                .size(220.0, 44.0)
                .text("Right Click Me")
                .font_size(16.0)
                .secondary_theme(widgets::theme::dark_theme_colors())
                .on_context_menu({
                    let state = state_store.clone();
                    move |event, bounds| {
                        let point = event
                            .position()
                            .unwrap_or([bounds.x + bounds.width, bounds.y]);
                        state.update(|state| {
                            state.context_menu_open = true;
                            state.context_menu_position = point;
                        });
                    }
                })
                .on_click({
                    let state = state_store.clone();
                    move || {
                        state.update(|state| {
                            state.context_menu_open = true;
                            state.context_menu_position = [720.0, 280.0];
                        });
                    }
                })
                .build();
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_oddities(
    ui: &mut Ui,
    state_store: &NeoState<LabState>,
    time: f32,
    pulse: f32,
    pointer_owned: bool,
    keyboard_owned: bool,
    state: &LabSnapshot,
) {
    let width = 370.0;
    let height = 560.0;
    widgets::panel(ui, "odd.panel")
        .size(width, height)
        .gradient(c(0.085, 0.060, 0.110, 0.93), c(0.035, 0.030, 0.052, 0.97))
        .border(1.0, c(0.38, 0.28, 0.48, 0.70))
        .shadow(24.0, 0.0, 8.0, c(0.0, 0.0, 0.0, 0.26))
        .radius(14.0)
        .build();

    ui.column("odd.content")
        .x(16.0)
        .y(16.0)
        .size(338.0, 528.0)
        .gap(12.0)
        .content(|ui| {
            section_title(ui, "odd.title", "Oddities Row", 338.0);

            widgets::segmented(ui, "odd.segmented")
                .size(310.0, 36.0)
                .items(["mild", "odd", "feral"])
                .selected_bind(bind_segment(state_store))
                .build();

            widgets::tabs(ui, "odd.tabs")
                .size(310.0, 40.0)
                .items(["Signals", "Forms", "Popups"])
                .selected_bind(bind_tab(state_store))
                .build();

            ui.row("odd.cards")
                .size(338.0, 106.0)
                .gap(10.0)
                .content(|ui| {
                    widgets::panel(ui, "odd.locked")
                        .size(160.0, 106.0)
                        .color(if state.lock {
                            c(0.20, 0.21, 0.24, 0.74)
                        } else if state.glass {
                            c(0.13, 0.30, 0.36, 0.66)
                        } else {
                            c(0.22, 0.27, 0.36, 0.92)
                        })
                        .radius(12.0)
                        .border(1.0, c(0.34, 0.46, 0.58, 0.42))
                        .build();
                    ui.column("odd.locked.content")
                        .x(12.0)
                        .y(12.0)
                        .size(136.0, 82.0)
                        .gap(8.0)
                        .content(|ui| {
                            ui.text("odd.locked.title")
                                .size(136.0, 20.0)
                                .text(if state.lock {
                                    "Locked Panel"
                                } else {
                                    "Soft Panel"
                                })
                                .font_size(15.0)
                                .line_height(18.0)
                                .color(c(0.92, 0.96, 1.0, 0.92))
                                .build();
                            widgets::button(ui, "odd.locked.button")
                                .size(122.0, 34.0)
                                .text("May Disable")
                                .font_size(13.0)
                                .disabled(state.lock)
                                .on_click({
                                    let state = state_store.clone();
                                    move || {
                                        state.update(|state| {
                                            state.clicks = state.clicks.wrapping_add(1);
                                        });
                                    }
                                })
                                .build();
                        });

                    if state.reveal {
                        widgets::panel(ui, "odd.secret")
                            .size(160.0, 106.0)
                            .color(c(0.34, 0.20, 0.44, 0.86))
                            .radius(12.0)
                            .border(1.0, c(0.56, 0.42, 0.72, 0.48))
                            .build();
                        ui.column("odd.secret.content")
                            .x(182.0)
                            .y(12.0)
                            .size(136.0, 82.0)
                            .gap(8.0)
                            .content(|ui| {
                                ui.text("odd.secret.title")
                                    .size(136.0, 20.0)
                                    .text("Secret")
                                    .font_size(15.0)
                                    .line_height(18.0)
                                    .color(c(0.98, 0.92, 1.0, 0.94))
                                    .build();
                                widgets::button(ui, "odd.secret.button")
                                    .size(122.0, 34.0)
                                    .text("Ghost Hit")
                                    .font_size(13.0)
                                    .on_click({
                                        let state = state_store.clone();
                                        move || {
                                            state.update(|state| {
                                                state.clicks = state.clicks.wrapping_add(1);
                                            });
                                        }
                                    })
                                    .build();
                            });
                    }
                });

            widgets::input(ui, "odd.input")
                .size(310.0, 42.0)
                .text_bind(bind_input_text(state_store))
                .placeholder("type a strange word")
                .build();

            widgets::dropdown(ui, "odd.dropdown")
                .size(310.0, 42.0)
                .items(["Low hum", "Signal", "Unstable"])
                .selected_bind(bind_dropdown_selected(state_store))
                .open_bind(bind_dropdown_open(state_store))
                .build();

            ui.row("odd.actions")
                .size(338.0, 44.0)
                .gap(10.0)
                .content(|ui| {
                    widgets::button(ui, "odd.dialog.button")
                        .size(150.0, 42.0)
                        .text("Dialog")
                        .font_size(15.0)
                        .on_click({
                            let dialog = bind_dialog_open(state_store);
                            move || dialog.set(true)
                        })
                        .build();
                    widgets::button(ui, "odd.toast.button")
                        .size(150.0, 42.0)
                        .text("Toast")
                        .font_size(15.0)
                        .secondary_theme(widgets::theme::dark_theme_colors())
                        .on_click({
                            let toast = bind_toast_visible(state_store);
                            move || toast.set(true)
                        })
                        .build();
                });

            ui.stack("odd.anchor.stage")
                .size(320.0, 110.0)
                .content(|ui| {
                    ui.rect("odd.anchor.bg")
                        .size(320.0, 110.0)
                        .color(c(0.06, 0.10, 0.12, 0.82))
                        .radius(14.0)
                        .build();
                    anchor_chip(
                        ui,
                        "odd.anchor.tl",
                        "TL",
                        18.0,
                        22.0,
                        c(0.34, 0.46, 0.86, 0.92),
                    );
                    anchor_chip(
                        ui,
                        "odd.anchor.tr",
                        "TR",
                        244.0,
                        22.0,
                        c(0.24, 0.66, 0.52, 0.92),
                    );
                    anchor_chip(
                        ui,
                        "odd.anchor.bl",
                        "BL",
                        18.0,
                        70.0,
                        c(0.76, 0.36, 0.50, 0.92),
                    );
                    anchor_chip(
                        ui,
                        "odd.anchor.br",
                        "BR",
                        244.0 + (time * 1.6).sin() * 8.0 * pulse,
                        70.0,
                        c(0.78, 0.58, 0.30, 0.92),
                    );
                });

            ui.text("odd.capture")
                .size(320.0, 20.0)
                .text(format!(
                    "pointer {}   keyboard {}",
                    on_off(pointer_owned),
                    on_off(keyboard_owned)
                ))
                .font_size(13.0)
                .line_height(16.0)
                .color(c(0.68, 0.76, 0.88, 0.74))
                .build();
        });
}

fn section_title(ui: &mut Ui, id: &str, text: &str, width: f32) {
    ui.text(id)
        .size(width, 26.0)
        .text(text)
        .font_size(20.0)
        .line_height(24.0)
        .color(c(0.92, 0.96, 1.0, 0.94))
        .build();
}

fn slider_row(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: f32,
    color: Color,
    binding: Binding<LabState, f32>,
) {
    ui.column(id).size(292.0, 54.0).gap(5.0).content(|ui| {
        ui.row(format!("{id}.label.row"))
            .size(292.0, 18.0)
            .content(|ui| {
                ui.text(format!("{id}.label"))
                    .size(150.0, 18.0)
                    .text(label)
                    .font_size(14.0)
                    .line_height(17.0)
                    .color(c(0.76, 0.84, 0.94, 0.86))
                    .build();
                ui.text(format!("{id}.value"))
                    .size(110.0, 18.0)
                    .text(format!("{:>3}%", (value * 100.0).round() as i32))
                    .font_size(12.0)
                    .line_height(15.0)
                    .color(c(0.58, 0.66, 0.78, 0.82))
                    .horizontal_align(HorizontalAlign::Right)
                    .build();
            });
        let mut style = widgets::SliderStyle::default();
        style.fill = color;
        widgets::slider(ui, format!("{id}.slider"))
            .size(260.0, 26.0)
            .value_bind(binding)
            .style(style)
            .build();
    });
}

fn radio_item(ui: &mut Ui, state_store: &NeoState<LabState>, id: &str, label: &str, value: i32) {
    widgets::radio(ui, id)
        .size(72.0, 28.0)
        .selected_bind(bind_radio_value(state_store, value))
        .text(label)
        .font_size(15.0)
        .build();
}

fn dense_row(ui: &mut Ui, index: usize, alarm: f32, chaos: f32, motion: Transition) {
    let t = index as f32 / 7.0;
    ui.row(format!("cabinet.dense.{index}"))
        .size(258.0, 20.0)
        .gap(8.0)
        .content(|ui| {
            ui.text(format!("cabinet.dense.label.{index}"))
                .size(118.0, 18.0)
                .text(format!("dense row {:02}", index + 1))
                .font_size(12.0)
                .line_height(15.0)
                .color(c(0.62, 0.72, 0.84, 0.72))
                .build();
            ui.rect(format!("cabinet.dense.bar.{index}"))
                .size(52.0 + 86.0 * t + chaos * 24.0, 10.0)
                .color(mix(
                    c(0.26, 0.78, 0.54, 0.78),
                    c(0.95, 0.32, 0.52, 0.84),
                    alarm * t,
                ))
                .radius(999.0)
                .transition(motion)
                .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                .build();
        });
}

fn meter_block(ui: &mut Ui, id: &str, width: f32, color: Color) {
    ui.rect(id)
        .size(width, 28.0)
        .color(color)
        .radius(9.0)
        .build();
}

fn meter_row(ui: &mut Ui, id: &str, label: &str, value: f32, color: Color) {
    let value = value.clamp(0.0, 1.0);
    ui.column(id).size(328.0, 40.0).gap(5.0).content(|ui| {
        ui.text(format!("{id}.label"))
            .size(328.0, 16.0)
            .text(label)
            .font_size(13.0)
            .line_height(16.0)
            .color(c(0.68, 0.76, 0.88, 0.76))
            .build();
        ui.stack(format!("{id}.bar"))
            .size(250.0, 14.0)
            .content(|ui| {
                ui.rect(format!("{id}.track"))
                    .size(250.0, 14.0)
                    .color(c(0.08, 0.12, 0.18, 0.92))
                    .radius(999.0)
                    .build();
                ui.rect(format!("{id}.fill"))
                    .size(250.0 * value, 14.0)
                    .color(color)
                    .radius(999.0)
                    .transition(Transition::make(0.18, Ease::OutCubic))
                    .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                    .build();
            });
    });
}

fn anchor_chip(ui: &mut Ui, id: &str, label: &str, x: f32, y: f32, color: Color) {
    ui.rect(format!("{id}.bg"))
        .x(x)
        .y(y)
        .size(58.0, 28.0)
        .color(color)
        .radius(9.0)
        .build();
    ui.text(format!("{id}.text"))
        .x(x)
        .y(y + 5.0)
        .size(58.0, 18.0)
        .text(label)
        .font_size(13.0)
        .line_height(16.0)
        .color(c(1.0, 0.98, 0.96, 0.94))
        .horizontal_align(HorizontalAlign::Center)
        .vertical_align(VerticalAlign::Top)
        .build();
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
    fn active(&self) -> bool {
        self.path.is_some() && !self.taken
    }

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

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("SkyEngine - Weird Neo Lab", WINDOW_W, WINDOW_H)
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

    App::new(world).run(WeirdNeoLab::default());
}
