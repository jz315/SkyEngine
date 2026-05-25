//! Rust port of the heavier EUI-NEO `app/gallery.cpp` demo.
//!
//! This is intentionally an example, not a new UI framework layer. It keeps the
//! original gallery shape: persistent app state, a left sidebar, a scrollable
//! content surface, component sections, chart/table widgets, and overlay widgets.
//!
//! ```bash
//! cargo run --example ui_neo_eui_gallery --features ui-neo --release
//! ```

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteFeature,
    Transform, TransparentPhase,
};
use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::widgets::theme::{self, PageVisualTokens, ThemeColorTokens};
use sky_engine::ui::neo::{
    bind, bind_array, bind_clamped, bind_clone, bind_eq, bind_max, open_window, Align,
    AnimProperty, Binding, Color, Ease, HorizontalAlign, NeoState, NeoWindowConfig, Transition, Ui,
};

const WINDOW_W: u32 = 1600;
const WINDOW_H: u32 = 1100;
const SIDEBAR_WIDTH: f32 = 272.0;
const NAV_TOP: f32 = 128.0;
const NAV_HEIGHT: f32 = 50.0;
const NAV_GAP: f32 = 14.0;

const PAGE_TITLES: [&str; 6] = [
    "Controls",
    "Style",
    "Animation",
    "Settings",
    "Bing",
    "About",
];

const PAGE_SUBTITLES: [&str; 6] = [
    "Basic controls, states and visual properties in one surface.",
    "Text scales, icon text and theme color tokens for developers.",
    "Click and hover samples driven by DSL transitions.",
    "Interactive settings built with the same rect and text primitives.",
    "Bing daily images and API text requests in one composed page.",
    "A lightweight and elegant C++ GUI framework.",
];

struct EuiNeoGallery {
    state: NeoState<GalleryState>,
    bing_text_rx: Option<Receiver<String>>,
    screenshot: ScreenshotProbe,
}

#[derive(Debug)]
struct GalleryState {
    selected_page: i32,
    option_dense: bool,
    option_glass: bool,
    option_motion: bool,
    option_unlock_fps: bool,
    option_night: bool,
    animation_moved: bool,
    animation_rotated: bool,
    animation_faded: bool,
    animation_scaled: bool,
    animation_rounded: bool,
    animation_glowing: bool,
    sample_checked: bool,
    sample_switch: bool,
    sample_radio_a: bool,
    sample_input: String,
    sample_slider: f32,
    sample_segment: i32,
    sample_tab: i32,
    sample_dropdown: i32,
    sample_dropdown_open: bool,
    sample_dialog_open: bool,
    sample_toast_visible: bool,
    sample_context_menu_open: bool,
    sample_context_menu_pos: [f32; 2],
    sample_date_open: bool,
    sample_year: i32,
    sample_month: i32,
    sample_day: i32,
    sample_time_open: bool,
    sample_hour: i32,
    sample_minute: i32,
    sample_color_open: bool,
    sample_inspector_open: bool,
    sample_color: Color,
    sample_feedback: String,
    bing_api_text: String,
    page_scroll: [f32; 6],
}

impl Default for EuiNeoGallery {
    fn default() -> Self {
        Self {
            state: NeoState::new(GalleryState::default()),
            bing_text_rx: None,
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl Default for GalleryState {
    fn default() -> Self {
        Self {
            selected_page: env_i32("SKY_NEO_GALLERY_PAGE").unwrap_or(0).clamp(0, 5),
            option_dense: false,
            option_glass: env_flag("SKY_NEO_GALLERY_GLASS"),
            option_motion: true,
            option_unlock_fps: false,
            option_night: true,
            animation_moved: false,
            animation_rotated: false,
            animation_faded: false,
            animation_scaled: false,
            animation_rounded: false,
            animation_glowing: env_flag("SKY_NEO_GALLERY_ANIMATION_GLOWING"),
            sample_checked: true,
            sample_switch: true,
            sample_radio_a: true,
            sample_input: "EUI".to_string(),
            sample_slider: 0.44,
            sample_segment: 1,
            sample_tab: 0,
            sample_dropdown: 1,
            sample_dropdown_open: false,
            sample_dialog_open: env_flag("SKY_NEO_GALLERY_DIALOG_OPEN"),
            sample_toast_visible: env_flag("SKY_NEO_GALLERY_TOAST_VISIBLE"),
            sample_context_menu_open: env_flag("SKY_NEO_GALLERY_CONTEXT_OPEN"),
            sample_context_menu_pos: [820.0, 390.0],
            sample_date_open: env_flag("SKY_NEO_GALLERY_DATE_OPEN"),
            sample_year: 2026,
            sample_month: 4,
            sample_day: 28,
            sample_time_open: env_flag("SKY_NEO_GALLERY_TIME_OPEN"),
            sample_hour: 9,
            sample_minute: 30,
            sample_color_open: env_flag("SKY_NEO_GALLERY_COLOR_OPEN"),
            sample_inspector_open: false,
            sample_color: c(56.0 / 255.0, 113.0 / 255.0, 224.0 / 255.0, 1.0),
            sample_feedback: "Ready".to_string(),
            bing_api_text: "Loading Bing API text...".to_string(),
            page_scroll: [0.0; 6],
        }
    }
}

impl AppState for EuiNeoGallery {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: c(0.07, 0.08, 0.10, 1.0).into(),
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
        self.poll_bing_text();
        if self.state.read(|state| {
            state.selected_page == 4 && state.bing_api_text == "Loading Bing API text..."
        }) && self.bing_text_rx.is_none()
        {
            self.start_bing_text_request();
        }
        let state = GallerySnapshot::from_state(&self.state);
        let gallery_state = self.state.clone();
        ctx.set_frame_rate_limit(if state.option_unlock_fps { 0.0 } else { 90.0 });

        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            draw_gallery(ui, screen.width, screen.height, &gallery_state, &state);
        });

        if self.state.read(|state| state.sample_inspector_open) {
            self.state
                .update(|state| state.sample_inspector_open = false);
            let child_tokens = theme_tokens(&GallerySnapshot::from_state(&self.state));
            let clear_color = child_tokens.background;
            open_window(
                ctx,
                NeoWindowConfig::new("Inspector", 640, 420)
                    .page_id("inspector")
                    .modal(true)
                    .clear_color(clear_color),
                move |ui, screen| draw_inspector_window_content(ui, screen, child_tokens),
            );
        }

        let selected_page = self.state.read(|state| state.selected_page.clamp(0, 5));
        ctx.set_title(&format!(
            "EUI Gallery - {}",
            PAGE_TITLES[selected_page as usize]
        ));
        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        if self.state.read(|state| {
            state.option_motion || state.sample_toast_visible || state.selected_page == 4
        }) {
            ctx.request_redraw();
        }
    }
}

impl EuiNeoGallery {
    fn start_bing_text_request(&mut self) {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(bing_api_text());
        });
        self.bing_text_rx = Some(receiver);
    }

    fn poll_bing_text(&mut self) {
        let Some(receiver) = &self.bing_text_rx else {
            return;
        };
        match receiver.try_recv() {
            Ok(text) => {
                self.state.update(|state| state.bing_api_text = text);
                self.bing_text_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.state.update(|state| {
                    state.bing_api_text = "Network text request failed.".to_string()
                });
                self.bing_text_rx = None;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct GallerySnapshot {
    selected_page: i32,
    option_dense: bool,
    option_glass: bool,
    option_motion: bool,
    option_unlock_fps: bool,
    option_night: bool,
    animation_moved: bool,
    animation_rotated: bool,
    animation_faded: bool,
    animation_scaled: bool,
    animation_rounded: bool,
    animation_glowing: bool,
    sample_radio_a: bool,
    sample_slider: f32,
    sample_context_menu_pos: [f32; 2],
    sample_year: i32,
    sample_month: i32,
    sample_day: i32,
    sample_hour: i32,
    sample_minute: i32,
    sample_color: Color,
    sample_feedback: String,
    bing_api_text: String,
    page_scroll: [f32; 6],
}

impl GallerySnapshot {
    fn from_state(value: &NeoState<GalleryState>) -> Self {
        value.read(Self::from_gallery_state)
    }

    fn from_gallery_state(value: &GalleryState) -> Self {
        Self {
            selected_page: value.selected_page,
            option_dense: value.option_dense,
            option_glass: value.option_glass,
            option_motion: value.option_motion,
            option_unlock_fps: value.option_unlock_fps,
            option_night: value.option_night,
            animation_moved: value.animation_moved,
            animation_rotated: value.animation_rotated,
            animation_faded: value.animation_faded,
            animation_scaled: value.animation_scaled,
            animation_rounded: value.animation_rounded,
            animation_glowing: value.animation_glowing,
            sample_radio_a: value.sample_radio_a,
            sample_slider: value.sample_slider,
            sample_context_menu_pos: value.sample_context_menu_pos,
            sample_year: value.sample_year,
            sample_month: value.sample_month,
            sample_day: value.sample_day,
            sample_hour: value.sample_hour,
            sample_minute: value.sample_minute,
            sample_color: value.sample_color,
            sample_feedback: value.sample_feedback.clone(),
            bing_api_text: value.bing_api_text.clone(),
            page_scroll: value.page_scroll,
        }
    }
}

fn bind_page_scroll(state: &NeoState<GalleryState>, page: usize) -> Binding<GalleryState, f32> {
    state.bind(
        move |state| state.page_scroll[page],
        move |state, value| state.page_scroll[page] = value.max(0.0),
    )
}

fn draw_gallery(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
) {
    let tokens = theme_tokens(state);
    let page = theme::page_visuals(tokens);
    let motion = page_transition(state.option_motion);

    ui.rect("gallery.clear")
        .size(screen_width, screen_height)
        .color(tokens.background)
        .build();

    draw_background(ui, screen_width, screen_height, tokens, state.option_glass);

    let content_width = (screen_width - SIDEBAR_WIDTH).max(0.0);

    ui.row("gallery.root")
        .size(screen_width, screen_height)
        .content(|ui| {
            draw_sidebar(ui, screen_height, state_store, state, tokens, motion);
            draw_content(
                ui,
                content_width,
                screen_height,
                state_store,
                state,
                tokens,
                page,
                motion,
            );
        });

    draw_overlays(ui, screen_width, screen_height, state_store, state, tokens);
}

fn draw_background(ui: &mut Ui, width: f32, height: f32, tokens: ThemeColorTokens, glass: bool) {
    let top = if tokens.dark {
        c(0.08, 0.095, 0.125, 1.0)
    } else {
        c(0.91, 0.93, 0.97, 1.0)
    };
    let bottom = if tokens.dark {
        c(0.035, 0.04, 0.055, 1.0)
    } else {
        c(0.965, 0.972, 0.99, 1.0)
    };
    ui.rect("gallery.background.gradient")
        .size(width, height)
        .gradient(top, bottom)
        .build();

    if glass {
        ui.rect("gallery.background.glass.a")
            .x(width - 520.0)
            .y(86.0)
            .size(360.0, 360.0)
            .color(with_alpha(tokens.primary, 0.10))
            .radius(180.0)
            .build();
        ui.rect("gallery.background.glass.b")
            .x(width - 760.0)
            .y(height - 310.0)
            .size(450.0, 260.0)
            .color(c(0.20, 0.76, 0.58, 0.08))
            .radius(130.0)
            .build();
    }
}

fn draw_sidebar(
    ui: &mut Ui,
    screen_height: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
    motion: Transition,
) {
    let sidebar_fill = if state.option_night {
        theme::mix_color(tokens.background, c(0.0, 0.0, 0.0, 1.0), 0.24)
    } else {
        tokens.surface
    };
    ui.stack("sidebar")
        .size(SIDEBAR_WIDTH, screen_height)
        .content(|ui| {
            ui.rect("sidebar.bg")
                .size(SIDEBAR_WIDTH, screen_height)
                .color(sidebar_fill)
                .build();

            ui.rect("sidebar.accent")
                .x(0.0)
                .y(NAV_TOP)
                .size(4.0, 50.0)
                .color(tokens.primary)
                .radius(2.0)
                .translate_y(
                    nav_order_for_page(state.selected_page) as f32 * (NAV_HEIGHT + NAV_GAP),
                )
                .transition(motion)
                .animate(AnimProperty::TRANSFORM | AnimProperty::COLOR)
                .build();

            ui.column("sidebar.content")
                .size(SIDEBAR_WIDTH, (screen_height - 42.0).max(0.0))
                .margin_each(0.0, 30.0, 0.0, 0.0)
                .gap(14.0)
                .align_items(Align::Center)
                .content(|ui| {
                    ui.text("brand.icon")
                        .size(212.0, 34.0)
                        .text(icon(0xf5fd))
                        .icon_font()
                        .font_size(27.0)
                        .line_height(32.0)
                        .color(tokens.primary)
                        .transition(motion)
                        .horizontal_align(HorizontalAlign::Center)
                        .build();

                    ui.text("brand.title")
                        .size(212.0, 36.0)
                        .text("EUI Gallery")
                        .font_size(30.0)
                        .line_height(34.0)
                        .color(tokens.text)
                        .horizontal_align(HorizontalAlign::Center)
                        .build();

                    nav_item(
                        ui,
                        "nav.controls",
                        "Controls",
                        0xf1b2,
                        0,
                        state,
                        tokens,
                        motion,
                        state_store,
                    );
                    nav_item(
                        ui,
                        "nav.text",
                        "Style",
                        0xf1fc,
                        1,
                        state,
                        tokens,
                        motion,
                        state_store,
                    );
                    nav_item(
                        ui,
                        "nav.animation",
                        "Animation",
                        0xf2f1,
                        2,
                        state,
                        tokens,
                        motion,
                        state_store,
                    );
                    nav_item(
                        ui,
                        "nav.bing",
                        "Bing",
                        0xf1c5,
                        4,
                        state,
                        tokens,
                        motion,
                        state_store,
                    );
                    nav_item(
                        ui,
                        "nav.settings",
                        "Settings",
                        0xf013,
                        3,
                        state,
                        tokens,
                        motion,
                        state_store,
                    );
                    nav_item(
                        ui,
                        "nav.about",
                        "About",
                        0xf05a,
                        5,
                        state,
                        tokens,
                        motion,
                        state_store,
                    );
                });

            ui.stack("sidebar.theme")
                .x(30.0)
                .y((screen_height - 82.0).max(0.0))
                .size(212.0, 50.0)
                .content(|ui| {
                    widgets::button(ui, "nav.theme")
                        .size(212.0, 50.0)
                        .icon_codepoint(if state.option_night { 0xf185 } else { 0xf186 })
                        .icon_size(16.0)
                        .text(if state.option_night {
                            "Light Mode"
                        } else {
                            "Night Mode"
                        })
                        .font_size(17.0)
                        .colors(tokens.surface, tokens.surface_hover, tokens.surface_active)
                        .text_color(tokens.text)
                        .icon_color(tokens.primary)
                        .radius(12.0)
                        .border(1.0, with_alpha(tokens.border, 0.80))
                        .shadow(12.0, 0.0, 4.0, shadow_color(tokens, 0.18, 0.08))
                        .transition(motion)
                        .on_click({
                            let option_night = bind!(state_store, option_night);
                            let next = !state.option_night;
                            move || option_night.set(next)
                        })
                        .build();
                });
        });
}

fn nav_item(
    ui: &mut Ui,
    id: &'static str,
    label: &'static str,
    icon_code: u32,
    page: i32,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
    motion: Transition,
    state_store: &NeoState<GalleryState>,
) {
    let selected = state.selected_page == page;
    let active_accent = tokens.primary;
    let base = if selected {
        active_accent
    } else {
        tokens.surface
    };
    let hover = if selected {
        theme::button_hover(tokens, active_accent)
    } else {
        tokens.surface_hover
    };
    let pressed = if selected {
        theme::button_pressed(tokens, active_accent)
    } else {
        tokens.surface_active
    };
    let text_color = if selected || state.option_night {
        c(0.94, 0.97, 1.0, 1.0)
    } else {
        tokens.text
    };

    widgets::button(ui, id)
        .size(212.0, NAV_HEIGHT)
        .icon_codepoint(icon_code)
        .icon_size(16.0)
        .font_size(17.0)
        .text(label)
        .colors(base, hover, pressed)
        .text_color(text_color)
        .icon_color(text_color)
        .radius(12.0)
        .border(
            1.0,
            if selected {
                with_alpha(active_accent, 0.58)
            } else {
                with_alpha(tokens.border, 0.60)
            },
        )
        .shadow(12.0, 0.0, 4.0, shadow_color(tokens, 0.18, 0.08))
        .transition(motion)
        .on_click({
            let selected_page = bind_clamped!(state_store, selected_page, 0, 5);
            move || selected_page.set(page)
        })
        .build();
}

fn draw_content(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    let shell_width = (width - 72.0).max(0.0);
    let inner_width = (shell_width - 64.0).max(0.0);
    let shell_height = (height - 72.0).max(0.0);
    let inner_height = (shell_height - 64.0).max(0.0);
    let header_gap = if state.option_dense { 18.0 } else { 26.0 };
    let body_height = (inner_height - 46.0 - 30.0 - header_gap * 2.0).max(0.0);
    let content_height =
        page_body_content_height(state.selected_page, state.option_dense, body_height);
    let page_index = state.selected_page.clamp(0, 5) as usize;
    let page_scroll = bind_page_scroll(state_store, page_index);
    let max_scroll = (content_height - body_height).max(0.0);
    let scroll_offset = state.page_scroll[page_index].clamp(0.0, max_scroll);
    let scrollable = max_scroll > 0.0;
    let scroll_width = if scrollable { 8.0 } else { 0.0 };
    let scroll_gap = if scrollable { 16.0 } else { 0.0 };
    let body_content_width = (inner_width - scroll_width - scroll_gap).max(0.0);

    ui.stack("content.area").size(width, height).content(|ui| {
        ui.rect("content.bg")
            .size(width, height)
            .color(tokens.background)
            .build();

        ui.rect("page.shell")
            .size(shell_width, shell_height)
            .margin(36.0)
            .color(tokens.surface)
            .radius(26.0)
            .border(1.0, tokens.border)
            .shadow(30.0, 0.0, 16.0, shadow_color(tokens, 0.28, 0.14))
            .transition(motion)
            .build();

        ui.column("page.content")
            .size(inner_width, inner_height)
            .margin(68.0)
            .gap(header_gap)
            .content(|ui| {
                ui.text("page.title")
                    .size(inner_width, 46.0)
                    .text(PAGE_TITLES[page_index])
                    .font_size(38.0)
                    .line_height(44.0)
                    .color(tokens.primary)
                    .transition(motion)
                    .build();

                ui.text("page.subtitle")
                    .size(inner_width, 30.0)
                    .text(PAGE_SUBTITLES[page_index])
                    .font_size(20.0)
                    .line_height(28.0)
                    .color(page.subtitle_color)
                    .transition(motion)
                    .build();

                let body = ui
                    .stack("page.body.viewport")
                    .size(inner_width, body_height)
                    .clip();
                let body = if scrollable {
                    let scroll_action = page_scroll.clone();
                    body.on_scroll(move |event| {
                        let next = (scroll_offset - event.y * 48.0).clamp(0.0, max_scroll);
                        scroll_action.set(next);
                    })
                } else {
                    body
                };
                body.content(|ui| {
                    ui.column("page.body.content")
                        .y(-scroll_offset)
                        .size(body_content_width, content_height)
                        .gap(header_gap)
                        .content(|ui| match state.selected_page {
                            0 => draw_controls_page(
                                ui,
                                body_content_width,
                                state_store,
                                state,
                                tokens,
                                page,
                                motion,
                            ),
                            1 => draw_style_page(
                                ui,
                                body_content_width,
                                content_height,
                                tokens,
                                page,
                                motion,
                            ),
                            2 => draw_animation_page(
                                ui,
                                body_content_width,
                                state_store,
                                state,
                                tokens,
                            ),
                            3 => draw_settings_page(
                                ui,
                                body_content_width,
                                state_store,
                                state,
                                tokens,
                                page,
                            ),
                            4 => draw_bing_page(
                                ui,
                                body_content_width,
                                content_height,
                                state,
                                tokens,
                                page,
                                motion,
                            ),
                            _ => draw_about_page(
                                ui,
                                body_content_width,
                                content_height,
                                tokens,
                                page,
                                motion,
                            ),
                        });

                    if scrollable {
                        widgets::scrollbar(ui, "page.scrollbar")
                            .x((inner_width - scroll_width).max(0.0))
                            .size(scroll_width, body_height)
                            .viewport(body_height)
                            .content(content_height)
                            .offset_bind(page_scroll.clone())
                            .theme(tokens)
                            .z_index(10)
                            .build();
                    }
                });
            });
    });
}

fn draw_controls_page(
    ui: &mut Ui,
    width: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    draw_controls_page_originalish(ui, width, state_store, state, tokens, page, motion);
}

fn draw_controls_page_originalish(
    ui: &mut Ui,
    width: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    let card_gap = 18.0;
    let card_width = ((width - card_gap * 2.0) / 3.0).clamp(72.0, 204.0);
    let row_width = card_width * 3.0 + card_gap * 2.0;
    let row_height = 144.0;
    let button_width = ((width - 36.0) / 3.0).clamp(72.0, 178.0);
    let field_width = width.clamp(0.0, 680.0);
    let component_card_width = ((width - 20.0) * 0.5).clamp(120.0, 340.0);
    let component_row_width = component_card_width * 2.0 + 20.0;
    let feedback_width = ((field_width - 54.0) / 4.0).clamp(112.0, 176.0);
    let data_row_gap = 20.0;
    let dropdown_width = (field_width * 0.36).clamp(180.0, 260.0);
    let table_width = (field_width - dropdown_width - data_row_gap).max(260.0);
    let data_row_height = 200.0;
    let picker_gap = 18.0;
    let picker_width = ((field_width - picker_gap * 2.0) / 3.0).clamp(154.0, 210.0);
    let picker_row_width = picker_width * 3.0 + picker_gap * 2.0;
    let chart_gap = 18.0;
    let chart_width = ((field_width - chart_gap * 2.0) / 3.0).clamp(150.0, 206.0);
    let chart_height = 236.0;
    let chart_row_width = chart_width * 3.0 + chart_gap * 2.0;

    ui.text("controls.components.title")
        .size(width, 30.0)
        .text("Basic Components")
        .font_size(26.0)
        .line_height(30.0)
        .color(page.title_color)
        .build();

    ui.row("controls.buttons")
        .size(button_width * 3.0 + 36.0, 68.0)
        .gap(18.0)
        .content(|ui| {
            widgets::button(ui, "control.primary")
                .size(button_width, 54.0)
                .icon_codepoint(0xf00c)
                .text("Filled")
                .colors(
                    tokens.primary,
                    theme::button_hover(tokens, tokens.primary),
                    theme::button_pressed(tokens, tokens.primary),
                )
                .border(1.0, with_alpha(tokens.primary, 0.58))
                .shadow(14.0, 0.0, 5.0, shadow_color(tokens, 0.22, 0.10))
                .transition(motion)
                .build();

            widgets::button(ui, "control.soft")
                .size(button_width, 54.0)
                .icon_codepoint(0xf0c8)
                .text("Outline")
                .colors(
                    c(0.0, 0.0, 0.0, 0.0),
                    with_alpha(tokens.primary, 0.10),
                    with_alpha(tokens.primary, 0.18),
                )
                .text_color(tokens.primary)
                .icon_color(tokens.primary)
                .border(1.0, with_alpha(tokens.primary, 0.78))
                .transition(motion)
                .build();

            widgets::button(ui, "control.warn")
                .size(button_width, 54.0)
                .icon_codepoint(0xf1fc)
                .text("Ghost")
                .colors(
                    c(0.0, 0.0, 0.0, 0.0),
                    with_alpha(tokens.primary, 0.08),
                    with_alpha(tokens.primary, 0.14),
                )
                .text_color(tokens.primary)
                .icon_color(tokens.primary)
                .border(0.0, c(0.0, 0.0, 0.0, 0.0))
                .transition(motion)
                .build();
        });

    widgets::input(ui, "control.input")
        .size(field_width, 44.0)
        .text_bind(bind_clone!(state_store, sample_input))
        .placeholder("Type here")
        .theme(tokens)
        .build();

    ui.row("controls.toggles")
        .size(component_row_width, 92.0)
        .gap(20.0)
        .content(|ui| {
            ui.column("controls.checks")
                .size(component_card_width, 92.0)
                .gap(12.0)
                .content(|ui| {
                    widgets::checkbox(ui, "control.checkbox")
                        .size(component_card_width, 30.0)
                        .checked_bind(bind!(state_store, sample_checked))
                        .text("Checkbox")
                        .theme(tokens)
                        .transition(motion)
                        .build();
                    widgets::switch(ui, "control.switch")
                        .size(component_card_width, 32.0)
                        .checked_bind(bind!(state_store, sample_switch))
                        .text("Switch")
                        .theme(tokens)
                        .transition(motion)
                        .build();
                });

            ui.column("controls.radios")
                .size(component_card_width, 92.0)
                .gap(12.0)
                .content(|ui| {
                    widgets::radio(ui, "control.radio.a")
                        .size(component_card_width, 30.0)
                        .selected(state.sample_radio_a)
                        .text("Radio A")
                        .theme(tokens)
                        .transition(motion)
                        .selected_bind(bind_eq!(state_store, sample_radio_a, true))
                        .build();
                    widgets::radio(ui, "control.radio.b")
                        .size(component_card_width, 30.0)
                        .selected(!state.sample_radio_a)
                        .text("Radio B")
                        .theme(tokens)
                        .transition(motion)
                        .selected_bind(bind_eq!(state_store, sample_radio_a, false))
                        .build();
                });
        });

    widgets::progress(ui, "control.progress")
        .size(field_width, 14.0)
        .value(state.sample_slider)
        .theme(tokens)
        .transition(Transition::default())
        .build();

    widgets::slider(ui, "control.slider")
        .size(field_width, 32.0)
        .value_bind(bind_clamped!(state_store, sample_slider, 0.0, 1.0))
        .theme(tokens)
        .transition(motion)
        .build();

    ui.row("controls.choice")
        .size(field_width, 46.0)
        .gap(18.0)
        .align_items(Align::Center)
        .content(|ui| {
            widgets::segmented(ui, "control.segmented")
                .size(((field_width - 18.0) * 0.5).max(180.0), 38.0)
                .items(["Small", "Medium", "Large"])
                .selected_bind(bind_max!(state_store, sample_segment, 0))
                .theme(tokens)
                .transition(motion)
                .build();
            widgets::tabs(ui, "control.tabs")
                .size(((field_width - 18.0) * 0.5).max(180.0), 42.0)
                .items(["Overview", "Details", "Logs"])
                .selected_bind(bind_max!(state_store, sample_tab, 0))
                .theme(tokens)
                .transition(motion)
                .build();
        });

    ui.text("controls.feedback.title")
        .size(width, 30.0)
        .text("Feedback Components")
        .font_size(25.0)
        .line_height(30.0)
        .color(page.title_color)
        .build();

    ui.row("controls.feedback")
        .size(feedback_width * 4.0 + 54.0, 82.0)
        .gap(18.0)
        .content(|ui| {
            feedback_button(
                ui,
                "control.dialog",
                "Dialog",
                0xf2d0,
                feedback_width,
                tokens,
                motion,
                {
                    let gallery_state = state_store.clone();
                    move || {
                        gallery_state.update(|state| {
                            state.sample_dialog_open = true;
                            state.sample_feedback = "Dialog opened".to_string();
                        });
                    }
                },
            );
            feedback_button(
                ui,
                "control.toast",
                "Toast",
                0xf0f3,
                feedback_width,
                tokens,
                motion,
                {
                    let gallery_state = state_store.clone();
                    move || {
                        gallery_state.update(|state| {
                            state.sample_toast_visible = true;
                            state.sample_feedback = "Toast queued".to_string();
                        });
                    }
                },
            );
            widgets::button(ui, "control.context")
                .size(feedback_width, 54.0)
                .icon_codepoint(0xf0c9)
                .text("Right Click")
                .text_color(tokens.text)
                .icon_color(tokens.primary)
                .radius(12.0)
                .border(1.0, with_alpha(tokens.border, 0.70))
                .shadow(10.0, 0.0, 3.0, shadow_color(tokens, 0.16, 0.08))
                .transition(motion)
                .on_context_menu({
                    let gallery_state = state_store.clone();
                    move |event, _bounds| {
                        let position = event.position().unwrap_or([0.0, 0.0]);
                        gallery_state.update(|state| {
                            state.sample_context_menu_open = true;
                            state.sample_context_menu_pos = position;
                            state.sample_feedback = "Context menu opened".to_string();
                        });
                    }
                })
                .build();
            widgets::button(ui, "control.window")
                .size(feedback_width, 54.0)
                .icon_codepoint(0xf24d)
                .text("Window")
                .text_color(tokens.text)
                .icon_color(tokens.primary)
                .radius(12.0)
                .border(1.0, with_alpha(tokens.border, 0.70))
                .shadow(10.0, 0.0, 3.0, shadow_color(tokens, 0.16, 0.08))
                .transition(motion)
                .on_click({
                    let gallery_state = state_store.clone();
                    move || {
                        gallery_state.update(|state| {
                            state.sample_inspector_open = true;
                            state.sample_feedback = "Window opened".to_string();
                        })
                    }
                })
                .build();
        });

    ui.text("controls.feedback.state")
        .size(width, 22.0)
        .text(&state.sample_feedback)
        .font_size(15.0)
        .line_height(20.0)
        .color(page.subtitle_color)
        .build();

    ui.text("controls.data.title")
        .size(width, 30.0)
        .text("Selection & Data")
        .font_size(25.0)
        .line_height(30.0)
        .color(page.title_color)
        .build();

    ui.row("controls.pickers.row")
        .size(picker_row_width, 56.0)
        .gap(picker_gap)
        .content(|ui| {
            widgets::button(ui, "control.datepicker.open")
                .size(picker_width, 44.0)
                .icon_codepoint(0xf073)
                .text(date_text(state))
                .text_color(tokens.text)
                .icon_color(tokens.primary)
                .radius(12.0)
                .border(1.0, with_alpha(tokens.border, 0.70))
                .shadow(10.0, 0.0, 3.0, shadow_color(tokens, 0.16, 0.08))
                .transition(motion)
                .on_click({
                    let gallery_state = state_store.clone();
                    move || {
                        gallery_state.update(|state| {
                            state.sample_date_open = true;
                            state.sample_time_open = false;
                            state.sample_color_open = false;
                            state.sample_dropdown_open = false;
                            state.sample_feedback = "Date picker opened".to_string();
                        });
                    }
                })
                .build();
            widgets::button(ui, "control.timepicker.open")
                .size(picker_width, 44.0)
                .icon_codepoint(0xf017)
                .text(time_text(state))
                .text_color(tokens.text)
                .icon_color(tokens.primary)
                .radius(12.0)
                .border(1.0, with_alpha(tokens.border, 0.70))
                .shadow(10.0, 0.0, 3.0, shadow_color(tokens, 0.16, 0.08))
                .transition(motion)
                .on_click({
                    let gallery_state = state_store.clone();
                    move || {
                        gallery_state.update(|state| {
                            state.sample_time_open = true;
                            state.sample_date_open = false;
                            state.sample_color_open = false;
                            state.sample_dropdown_open = false;
                            state.sample_feedback = "Time picker opened".to_string();
                        });
                    }
                })
                .build();
            widgets::button(ui, "control.colorpicker.open")
                .size(picker_width, 44.0)
                .icon_codepoint(0xf53f)
                .text(color_hex(state.sample_color))
                .text_color(tokens.text)
                .icon_color(state.sample_color)
                .radius(12.0)
                .border(1.0, with_alpha(tokens.border, 0.70))
                .shadow(10.0, 0.0, 3.0, shadow_color(tokens, 0.16, 0.08))
                .transition(motion)
                .on_click({
                    let gallery_state = state_store.clone();
                    move || {
                        gallery_state.update(|state| {
                            state.sample_color_open = true;
                            state.sample_date_open = false;
                            state.sample_time_open = false;
                            state.sample_dropdown_open = false;
                            state.sample_feedback = "Color picker opened".to_string();
                        });
                    }
                })
                .build();
        });

    ui.row("controls.data.row")
        .size(dropdown_width + table_width + data_row_gap, data_row_height)
        .gap(data_row_gap)
        .content(|ui| {
            widgets::dropdown(ui, "control.dropdown")
                .size(dropdown_width, 44.0)
                .items(["Draft", "Review", "Published", "Archived"])
                .selected_bind(bind_max!(state_store, sample_dropdown, 0))
                .open_bind(bind!(state_store, sample_dropdown_open))
                .theme(tokens)
                .transition(motion)
                .on_open_change({
                    let gallery_state = state_store.clone();
                    move |open| {
                        if open {
                            gallery_state.update(|state| {
                                state.sample_date_open = false;
                                state.sample_time_open = false;
                                state.sample_color_open = false;
                            });
                        }
                    }
                })
                .on_change({
                    let gallery_state = state_store.clone();
                    move |_| {
                        gallery_state
                            .update(|state| state.sample_feedback = "Dropdown changed".to_string());
                    }
                })
                .build();

            widgets::data_table(ui, "control.table")
                .size(table_width, 174.0)
                .columns(["Name", "Status", "Owner"])
                .rows([
                    ["EUI Core", "Active", "Sudo"],
                    ["Gallery", "Review", "Design"],
                    ["Docs", "Draft", "DevRel"],
                    ["Runtime", "Stable", "Engine"],
                ])
                .theme(tokens)
                .transition(motion)
                .build();
        });

    ui.text("controls.charts.title")
        .size(width, 30.0)
        .text("Charts")
        .font_size(25.0)
        .line_height(30.0)
        .color(page.title_color)
        .build();

    ui.row("controls.charts.row")
        .size(chart_row_width, chart_height)
        .gap(chart_gap)
        .content(|ui| {
            widgets::linechart(ui, "control.chart.line")
                .size(chart_width, chart_height)
                .title("LineChart")
                .values([0.22, 0.30, 0.20, 0.55, 0.42, 0.86])
                .labels(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
                .theme(tokens)
                .transition(motion)
                .build();

            widgets::barchart(ui, "control.chart.bar")
                .size(chart_width, chart_height)
                .title("BarChart")
                .values([0.92, 0.36, 0.68, 0.52])
                .labels(["D1", "D2", "D3", "D4"])
                .theme(tokens)
                .transition(motion)
                .build();

            widgets::piechart(ui, "control.chart.pie")
                .size(chart_width, chart_height)
                .title("PieChart")
                .values([0.42, 0.24, 0.18, 0.16])
                .labels(["Blue", "Green", "Orange", "Pink"])
                .theme(tokens)
                .transition(motion)
                .build();
        });

    ui.text("controls.primitives.title")
        .size(width, 30.0)
        .text("Primitive Properties")
        .font_size(25.0)
        .line_height(30.0)
        .color(page.title_color)
        .build();

    ui.row("properties.a")
        .size(row_width, row_height)
        .gap(card_gap)
        .content(|ui| {
            primitive_property_card(
                ui,
                "prop.color",
                "Color",
                "hover + press",
                c(0.22, 0.48, 0.82, 1.0),
                "color",
                card_width,
                state.option_glass,
                tokens,
                page,
                motion,
            );
            primitive_property_card(
                ui,
                "prop.border",
                "Border",
                "animated edge",
                tokens.surface,
                "border",
                card_width,
                state.option_glass,
                tokens,
                page,
                motion,
            );
            primitive_property_card(
                ui,
                "prop.shadow",
                "Shadow",
                "elevation",
                tokens.surface_hover,
                "shadow",
                card_width,
                state.option_glass,
                tokens,
                page,
                motion,
            );
        });

    ui.row("properties.b")
        .size(row_width, row_height)
        .gap(card_gap)
        .content(|ui| {
            primitive_property_card(
                ui,
                "prop.alpha",
                "Opacity",
                "transparent fill",
                c(0.86, 0.38, 0.52, 0.58),
                "color",
                card_width,
                state.option_glass,
                tokens,
                page,
                motion,
            );
            primitive_property_card(
                ui,
                "prop.blur",
                "Blur",
                "glass card",
                c(0.78, 0.92, 1.0, 0.22),
                "blur",
                card_width,
                state.option_glass,
                tokens,
                page,
                motion,
            );
            primitive_property_card(
                ui,
                "prop.rotate",
                "Rotate",
                "transform",
                c(0.48, 0.64, 0.36, 1.0),
                "rotate",
                card_width,
                state.option_glass,
                tokens,
                page,
                motion,
            );
        });
}

fn draw_style_page(
    ui: &mut Ui,
    width: f32,
    height: f32,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    let text_width = width.clamp(240.0, 760.0);
    let icon_gap = 20.0;
    let icon_card_width = ((width - icon_gap * 3.0) / 4.0).clamp(60.0, 120.0);
    let icon_row_width = icon_card_width * 4.0 + icon_gap * 3.0;
    let swatch_gap = 14.0;
    let swatch_width = ((width - swatch_gap * 3.0) / 4.0).clamp(94.0, 142.0);
    let swatch_row_width = swatch_width * 4.0 + swatch_gap * 3.0;

    let text_samples_height = height.min(380.0);
    ui.column("text.samples")
        .size(width, text_samples_height)
        .gap(12.0)
        .content(|ui| {
            text_sample(
                ui,
                "txt.display",
                "Display 48 - Gallery Title",
                48.0,
                58.0,
                text_width,
                page.title_color,
                motion,
            );
            text_sample(
                ui,
                "txt.h1",
                "Heading 36 - Section Header",
                36.0,
                46.0,
                text_width,
                with_alpha(page.title_color, 0.92),
                motion,
            );
            text_sample(
                ui,
                "txt.h2",
                "Heading 28 - Component Name",
                28.0,
                38.0,
                text_width,
                with_alpha(page.title_color, 0.82),
                motion,
            );
            text_sample(
                ui,
                "txt.body",
                "Body 20 - Text can wrap, align and use custom colors.",
                20.0,
                30.0,
                text_width,
                page.body_color,
                motion,
            );
            text_sample(
                ui,
                "txt.small",
                "Small 15 - Secondary metadata and compact labels.",
                15.0,
                24.0,
                text_width,
                with_alpha(page.title_color, 0.58),
                motion,
            );

            ui.row("text.icons")
                .size(icon_row_width, 74.0)
                .gap(icon_gap)
                .content(|ui| {
                    let icons = [0xf015, 0xf1fc, 0xf013, 0xf05a];
                    let names = ["Home", "Theme", "Settings", "Info"];
                    for (index, (icon_code, name)) in icons.iter().zip(names).enumerate() {
                        ui.stack(format!("text.icon.card.{index}"))
                            .size(icon_card_width, 72.0)
                            .content(|ui| {
                                ui.text(format!("text.icon.{index}"))
                                    .size(icon_card_width, 36.0)
                                    .icon_codepoint(*icon_code)
                                    .font_size(28.0)
                                    .line_height(34.0)
                                    .color(tokens.primary)
                                    .horizontal_align(HorizontalAlign::Center)
                                    .transition(motion)
                                    .build();
                                caption(
                                    ui,
                                    format!("text.icon.label.{index}"),
                                    name,
                                    icon_card_width,
                                    40.0,
                                    page,
                                );
                            });
                    }
                });
        });

    ui.text("style.theme.title")
        .size(width, 30.0)
        .text("Theme Color Tokens")
        .font_size(25.0)
        .line_height(30.0)
        .color(page.title_color)
        .build();

    ui.row("style.theme.tokens.a")
        .size(swatch_row_width, 88.0)
        .gap(swatch_gap)
        .content(|ui| {
            theme_swatch(
                ui,
                "style.color.background",
                "background",
                tokens.background,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.primary",
                "primary",
                tokens.primary,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.surface",
                "surface",
                tokens.surface,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.surfaceHover",
                "surfaceHover",
                tokens.surface_hover,
                swatch_width,
                tokens,
                page,
            );
        });

    ui.row("style.theme.tokens.b")
        .size(swatch_row_width, 88.0)
        .gap(swatch_gap)
        .content(|ui| {
            theme_swatch(
                ui,
                "style.color.surfaceActive",
                "surfaceActive",
                tokens.surface_active,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.text",
                "text",
                tokens.text,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.border",
                "border",
                tokens.border,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.accent",
                "pickerAccent",
                tokens.primary,
                swatch_width,
                tokens,
                page,
            );
        });

    ui.text("style.visual.title")
        .size(width, 30.0)
        .text("Page Visual Colors")
        .font_size(25.0)
        .line_height(30.0)
        .color(page.title_color)
        .build();

    ui.row("style.theme.visuals")
        .size(swatch_row_width, 88.0)
        .gap(swatch_gap)
        .content(|ui| {
            theme_swatch(
                ui,
                "style.color.title",
                "titleColor",
                page.title_color,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.subtitle",
                "subtitleColor",
                page.subtitle_color,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.body",
                "bodyColor",
                page.body_color,
                swatch_width,
                tokens,
                page,
            );
            theme_swatch(
                ui,
                "style.color.softAccent",
                "softAccent",
                page.soft_accent_color,
                swatch_width,
                tokens,
                page,
            );
        });
}

fn draw_animation_page(
    ui: &mut Ui,
    width: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
) {
    let page = theme::page_visuals(tokens);
    let motion = page_transition(state.option_motion);
    let stage_width = width.clamp(280.0, 860.0);
    let stage_height = 268.0;
    let actor_width = if state.animation_rotated {
        150.0
    } else {
        118.0
    };
    let actor_height = if state.animation_rotated { 96.0 } else { 72.0 };
    let actor_scale = if state.animation_scaled { 1.18 } else { 1.0 };
    let actor_travel = (stage_width - actor_width * actor_scale - 58.0).max(46.0);
    let button_width = ((stage_width - 36.0) / 3.0).clamp(92.0, 166.0);
    let button_row_width = button_width * 3.0 + 36.0;
    let rotate_color = c(0.84, 0.46, 0.60, 1.0);
    let fade_color = c(0.50, 0.72, 0.34, 1.0);
    let scale_color = c(0.92, 0.62, 0.26, 1.0);
    let radius_color = c(0.50, 0.58, 0.94, 1.0);
    let glow_color = c(0.28, 0.76, 0.72, 1.0);

    ui.row("animation.controls")
        .size(button_row_width, 58.0)
        .gap(18.0)
        .content(|ui| {
            animation_button(
                ui,
                "anim.move",
                "Move",
                state.animation_moved,
                tokens.primary,
                button_width,
                tokens,
                page,
                motion,
                {
                    let action = bind!(state_store, animation_moved);
                    let next = !state.animation_moved;
                    move || action.set(next)
                },
            );
            animation_button(
                ui,
                "anim.rotate",
                "Rotate",
                state.animation_rotated,
                rotate_color,
                button_width,
                tokens,
                page,
                motion,
                {
                    let action = bind!(state_store, animation_rotated);
                    let next = !state.animation_rotated;
                    move || action.set(next)
                },
            );
            animation_button(
                ui,
                "anim.fade",
                "Fade",
                state.animation_faded,
                fade_color,
                button_width,
                tokens,
                page,
                motion,
                {
                    let action = bind!(state_store, animation_faded);
                    let next = !state.animation_faded;
                    move || action.set(next)
                },
            );
        });

    ui.row("animation.controls.extra")
        .size(button_row_width, 58.0)
        .gap(18.0)
        .content(|ui| {
            animation_button(
                ui,
                "anim.scale",
                "Scale",
                state.animation_scaled,
                scale_color,
                button_width,
                tokens,
                page,
                motion,
                {
                    let action = bind!(state_store, animation_scaled);
                    let next = !state.animation_scaled;
                    move || action.set(next)
                },
            );
            animation_button(
                ui,
                "anim.radius",
                "Radius",
                state.animation_rounded,
                radius_color,
                button_width,
                tokens,
                page,
                motion,
                {
                    let action = bind!(state_store, animation_rounded);
                    let next = !state.animation_rounded;
                    move || action.set(next)
                },
            );
            animation_button(
                ui,
                "anim.glow",
                "Glow",
                state.animation_glowing,
                glow_color,
                button_width,
                tokens,
                page,
                motion,
                {
                    let action = bind!(state_store, animation_glowing);
                    let next = !state.animation_glowing;
                    move || action.set(next)
                },
            );
        });

    ui.stack("animation.stage")
        .size(stage_width, stage_height)
        .content(|ui| {
            ui.rect("animation.stage.bg")
                .size(stage_width, stage_height)
                .color(tokens.surface)
                .radius(24.0)
                .border(1.0, tokens.border)
                .build();

            ui.rect("animation.stage.track")
                .x(38.0)
                .y(stage_height - 54.0)
                .size((stage_width - 76.0).max(0.0), 2.0)
                .color(with_alpha(tokens.border, 0.48))
                .radius(1.0)
                .build();

            ui.rect("animation.actor")
                .x(if state.animation_moved {
                    actor_travel
                } else {
                    46.0
                })
                .y(if state.animation_moved { 70.0 } else { 92.0 })
                .size(actor_width, actor_height)
                .color(if state.animation_moved {
                    tokens.primary
                } else {
                    radius_color
                })
                .radius(if state.animation_rounded {
                    actor_height * 0.5
                } else if state.animation_rotated {
                    30.0
                } else {
                    18.0
                })
                .rotate(if state.animation_rotated { 0.42 } else { 0.0 })
                .scale(actor_scale)
                .transform_origin(0.5, 0.5)
                .opacity(if state.animation_faded { 0.36 } else { 1.0 })
                .shadow(
                    if state.animation_glowing { 44.0 } else { 26.0 },
                    0.0,
                    if state.animation_glowing { 18.0 } else { 12.0 },
                    if state.animation_glowing {
                        with_alpha(glow_color, if tokens.dark { 0.42 } else { 0.26 })
                    } else {
                        shadow_color(tokens, 0.32, 0.16)
                    },
                )
                .transition(motion_transition(state.option_motion))
                .animate(
                    AnimProperty::FRAME
                        | AnimProperty::COLOR
                        | AnimProperty::OPACITY
                        | AnimProperty::RADIUS
                        | AnimProperty::SHADOW
                        | AnimProperty::TRANSFORM,
                )
                .build();
        });
}

fn draw_settings_page(
    ui: &mut Ui,
    width: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
) {
    let row_width = width.clamp(0.0, 720.0);
    let motion = page_transition(state.option_motion);
    ui.column("settings.list")
        .size(row_width, 430.0)
        .gap(14.0)
        .content(|ui| {
            setting_row(
                ui,
                "setting.dense",
                "Dense layout",
                "Use tighter spacing for gallery pages.",
                state.option_dense,
                row_width,
                tokens,
                page,
                motion,
                bind!(state_store, option_dense),
            );
            setting_row(
                ui,
                "setting.glass",
                "Glass surfaces",
                "Show transparent panel examples in controls.",
                state.option_glass,
                row_width,
                tokens,
                page,
                motion,
                bind!(state_store, option_glass),
            );
            setting_row(
                ui,
                "setting.motion",
                "Animated transitions",
                "Keep page and property transitions enabled.",
                state.option_motion,
                row_width,
                tokens,
                page,
                motion,
                bind!(state_store, option_motion),
            );
            setting_row(
                ui,
                "setting.unlockFps",
                "Unlock 90 FPS limit",
                "Let animation rendering use the display refresh rate.",
                state.option_unlock_fps,
                row_width,
                tokens,
                page,
                motion,
                bind!(state_store, option_unlock_fps),
            );
            setting_row(
                ui,
                "setting.night",
                "Night mode",
                "Switch gallery between light and dark theme tokens.",
                state.option_night,
                row_width,
                tokens,
                page,
                motion,
                bind!(state_store, option_night),
            );
        });
}

fn draw_bing_page(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    let content_width = width.clamp(260.0, 860.0);
    let card_gap = 20.0;
    let card_width = ((content_width - card_gap) * 0.5).clamp(116.0, 400.0);
    let row_width = card_width * 2.0 + card_gap;
    let media_height = 252.0;
    let media_image_height = 198.0;
    let api_height = 138.0;

    ui.column("bing.body")
        .size(width, height)
        .align_items(Align::Center)
        .gap(22.0)
        .content(|ui| {
            ui.row("bing.media")
                .size(row_width, media_height)
                .gap(card_gap)
                .content(|ui| {
                    bing_image_card(
                        ui,
                        "bing.media.today",
                        "Bing Today",
                        0,
                        "zh-CN",
                        card_width,
                        media_height,
                        media_image_height,
                        tokens,
                        page,
                        motion,
                    );
                    bing_image_card(
                        ui,
                        "bing.media.yesterday",
                        "Bing Yesterday",
                        1,
                        "zh-CN",
                        card_width,
                        media_height,
                        media_image_height,
                        tokens,
                        page,
                        motion,
                    );
                });

            ui.stack("bing.api")
                .size(content_width, api_height)
                .content(|ui| {
                    ui.rect("bing.api.bg")
                        .size(content_width, api_height)
                        .color(tokens.surface)
                        .radius(18.0)
                        .border(1.0, tokens.border)
                        .build();

                    ui.text("bing.api.title")
                        .x(22.0)
                        .y(18.0)
                        .size((content_width - 44.0).max(0.0), 30.0)
                        .text("Bing API text")
                        .font_size(22.0)
                        .line_height(26.0)
                        .color(page.title_color)
                        .build();

                    ui.text("bing.api.text")
                        .x(22.0)
                        .y(54.0)
                        .size(
                            (content_width - 44.0).max(0.0),
                            (api_height - 68.0).max(0.0),
                        )
                        .text(&state.bing_api_text)
                        .font_size(16.0)
                        .line_height(22.0)
                        .max_width((content_width - 44.0).max(0.0))
                        .wrap(true)
                        .color(page.subtitle_color)
                        .build();
                });
        });
}

fn draw_about_page(
    ui: &mut Ui,
    width: f32,
    height: f32,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    let content_width = width.clamp(280.0, 860.0);
    let compact = content_width < 620.0;
    let logo_size = if compact { 112.0 } else { 126.0 };
    let button_gap = if compact { 16.0 } else { 14.0 };
    let button_width = if compact {
        ((content_width - button_gap) * 0.5).clamp(124.0, 180.0)
    } else {
        162.0
    };
    let button_row_width = button_width * 2.0 + button_gap;
    let hero_height = if compact { 342.0 } else { 238.0 };
    let license_height = 92.0;

    ui.column("about.body")
        .size(width, height)
        .align_items(Align::Center)
        .gap(20.0)
        .content(|ui| {
            ui.stack("about.hero")
                .size(content_width, hero_height)
                .content(|ui| {
                    ui.rect("about.hero.bg")
                        .size(content_width, hero_height)
                        .color(tokens.surface)
                        .radius(22.0)
                        .border(1.0, tokens.border)
                        .build();

                    let logo_x = if compact {
                        (content_width - logo_size) * 0.5
                    } else {
                        24.0
                    };
                    let logo_y = if compact { 20.0 } else { 40.0 };
                    ui.rect("about.logo.frame")
                        .x(logo_x)
                        .y(logo_y)
                        .size(logo_size, logo_size)
                        .color(tokens.surface_hover)
                        .radius(28.0)
                        .shadow(20.0, 0.0, 10.0, shadow_color(tokens, 0.24, 0.12))
                        .build();

                    ui.image("about.logo.image")
                        .x(logo_x)
                        .y(logo_y)
                        .size(logo_size, logo_size)
                        .source("assets/icon.png")
                        .radius(28.0)
                        .cover()
                        .build();

                    let info_x = if compact {
                        24.0
                    } else {
                        logo_x + logo_size + 30.0
                    };
                    let info_y = if compact {
                        logo_y + logo_size + 18.0
                    } else {
                        38.0
                    };
                    let info_width = if compact {
                        (content_width - 48.0).max(0.0)
                    } else {
                        (content_width - info_x - 24.0).max(0.0)
                    };
                    ui.text("about.hero.title")
                        .x(info_x)
                        .y(info_y)
                        .size(info_width, 36.0)
                        .text("EUI Neo")
                        .font_size(32.0)
                        .line_height(36.0)
                        .color(page.title_color)
                        .horizontal_align(if compact {
                            HorizontalAlign::Center
                        } else {
                            HorizontalAlign::Left
                        })
                        .build();

                    ui.text("about.hero.copy")
                        .x(info_x)
                        .y(info_y + 42.0)
                        .size(info_width, 58.0)
                        .text("A lightweight C++ UI playground for themed controls, motion and image rendering.")
                        .font_size(17.0)
                        .line_height(24.0)
                        .max_width(info_width)
                        .wrap(true)
                        .color(page.subtitle_color)
                        .horizontal_align(if compact {
                            HorizontalAlign::Center
                        } else {
                            HorizontalAlign::Left
                        })
                        .build();

                    ui.row("about.actions")
                        .x(if compact {
                            (content_width - button_row_width) * 0.5
                        } else {
                            info_x
                        })
                        .y(if compact { hero_height - 74.0 } else { 162.0 })
                        .size(button_row_width, 52.0)
                        .gap(button_gap)
                        .content(|ui| {
                            widgets::button(ui, "about.github")
                                .size(button_width, 52.0)
                                .icon_codepoint(0xf0c1)
                                .icon_size(20.0)
                                .font_size(19.0)
                                .text("GitHub")
                                .colors(
                                    tokens.primary,
                                    theme::button_hover(tokens, tokens.primary),
                                    theme::button_pressed(tokens, tokens.primary),
                                )
                                .radius(12.0)
                                .border(1.0, with_alpha(tokens.primary, 0.58))
                                .shadow(14.0, 0.0, 5.0, shadow_color(tokens, 0.22, 0.10))
                                .transition(motion)
                                .on_click(|| {
                                    let _ = sky_engine::platform::open_url(
                                        "https://github.com/sudoevolve/EUI-NEO",
                                    );
                                })
                                .build();

                            widgets::button(ui, "about.group")
                                .size(button_width, 52.0)
                                .icon_codepoint(0xf0c0)
                                .icon_size(19.0)
                                .font_size(19.0)
                                .text("Group")
                                .colors(
                                    tokens.surface_hover,
                                    theme::button_hover(tokens, tokens.surface_hover),
                                    theme::button_pressed(tokens, tokens.surface_hover),
                                )
                                .text_color(tokens.text)
                                .icon_color(tokens.text)
                                .radius(12.0)
                                .border(1.0, tokens.border)
                                .shadow(12.0, 0.0, 4.0, shadow_color(tokens, 0.18, 0.08))
                                .transition(motion)
                                .on_click(|| {
                                    let _ = sky_engine::platform::open_url(
                                        "https://qm.qq.com/q/kaPB4paOpa",
                                    );
                                })
                                .build();
                        });
                });

            ui.stack("about.license")
                .size(content_width, license_height)
                .content(|ui| {
                    ui.rect("about.license.bg")
                        .size(content_width, license_height)
                        .color(tokens.surface)
                        .radius(18.0)
                        .border(1.0, tokens.border)
                        .build();

                    ui.text("about.license.title")
                        .x(22.0)
                        .y(12.0)
                        .size((content_width - 44.0).max(0.0), 26.0)
                        .text("License")
                        .font_size(22.0)
                        .line_height(25.0)
                        .color(page.title_color)
                        .build();

                    ui.text("about.license.copy")
                        .x(22.0)
                        .y(40.0)
                        .size((content_width - 44.0).max(0.0), 22.0)
                        .text("Copyright @2026 SudoEvolve")
                        .font_size(17.0)
                        .line_height(21.0)
                        .color(page.subtitle_color)
                        .build();

                    ui.text("about.license.type")
                        .x(22.0)
                        .y(64.0)
                        .size((content_width - 44.0).max(0.0), 22.0)
                        .text("Licensed under apache2.0")
                        .font_size(16.0)
                        .line_height(20.0)
                        .color(page.subtitle_color)
                        .build();
                });
        });
}

fn bing_image_card(
    ui: &mut Ui,
    id: &str,
    title: &str,
    index: i32,
    market: &str,
    width: f32,
    height: f32,
    image_height: f32,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    let label_y = (height - 30.0).max(14.0);
    ui.stack(id).size(width, height).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, height)
            .color(tokens.surface)
            .radius(18.0)
            .border(1.0, tokens.border)
            .build();

        ui.image(format!("{id}.image"))
            .x(14.0)
            .y(14.0)
            .size((width - 28.0).max(0.0), image_height)
            .bing_daily(index, market)
            .radius(14.0)
            .transition(motion)
            .build();

        ui.text(format!("{id}.label"))
            .x(14.0)
            .y(label_y)
            .size((width - 28.0).max(0.0), 22.0)
            .text(title)
            .font_size(16.0)
            .line_height(20.0)
            .color(page.subtitle_color)
            .horizontal_align(HorizontalAlign::Center)
            .build();
    });
}

fn draw_overlays(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    state_store: &NeoState<GalleryState>,
    state: &GallerySnapshot,
    tokens: ThemeColorTokens,
) {
    widgets::dialog(ui, "feedback.dialog")
        .open_bind(bind!(state_store, sample_dialog_open))
        .screen(screen_width, screen_height)
        .size(430.0, 228.0)
        .title("Dialog Component")
        .message("A modal surface for focused confirmation. It uses the same theme tokens, buttons and dirty-region rendering path as the rest of the gallery.")
        .primary_text("Confirm")
        .secondary_text("Cancel")
        .theme(tokens)
        .on_primary({
            let dialog = bind!(state_store, sample_dialog_open);
            let toast = bind!(state_store, sample_toast_visible);
            let feedback_state = state_store.clone();
            move || {
                dialog.set(false);
                toast.set(true);
                feedback_state.update(|state| state.sample_feedback = "Dialog confirmed".to_string());
            }
        })
        .on_secondary({
            let dialog = bind!(state_store, sample_dialog_open);
            let feedback_state = state_store.clone();
            move || {
                dialog.set(false);
                feedback_state.update(|state| state.sample_feedback = "Dialog cancelled".to_string());
            }
        })
        .on_close({
            let dialog = bind!(state_store, sample_dialog_open);
            let feedback_state = state_store.clone();
            move || {
                dialog.set(false);
                feedback_state.update(|state| state.sample_feedback = "Dialog closed".to_string());
            }
        })
        .build();

    widgets::context_menu(ui, "feedback.context")
        .open_bind(bind!(state_store, sample_context_menu_open))
        .screen(screen_width, screen_height)
        .position(
            state.sample_context_menu_pos[0],
            state.sample_context_menu_pos[1],
        )
        .items(["Inspect", "Duplicate", "Copy Token", "Dismiss"])
        .theme(tokens)
        .on_select({
            let open = bind!(state_store, sample_context_menu_open);
            let toast = bind!(state_store, sample_toast_visible);
            let feedback_state = state_store.clone();
            move |index| {
                open.set(false);
                toast.set(true);
                let message = match index {
                    0 => "Inspect selected",
                    1 => "Duplicate selected",
                    2 => "Copy Token selected",
                    _ => {
                        toast.set(false);
                        "Context menu dismissed"
                    }
                };
                feedback_state.update(|state| state.sample_feedback = message.to_string());
            }
        })
        .on_dismiss({
            let open = bind!(state_store, sample_context_menu_open);
            let feedback_state = state_store.clone();
            move || {
                open.set(false);
                feedback_state
                    .update(|state| state.sample_feedback = "Context menu dismissed".to_string());
            }
        })
        .build();

    widgets::date_picker(ui, "feedback.datepicker")
        .open_bind(bind!(state_store, sample_date_open))
        .screen(screen_width, screen_height)
        .size(420.0, 270.0)
        .date_bind(bind_array!(
            state_store,
            [sample_year, sample_month, sample_day]
        ))
        .theme(tokens)
        .transition(page_transition(state.option_motion))
        .z(1200)
        .on_change({
            let gallery_state = state_store.clone();
            move |_, _, _| {
                gallery_state.update(|state| state.sample_feedback = "Date changed".to_string())
            }
        })
        .build();

    widgets::time_picker(ui, "feedback.timepicker")
        .open_bind(bind!(state_store, sample_time_open))
        .screen(screen_width, screen_height)
        .size(330.0, 264.0)
        .time_bind(bind_array!(state_store, [sample_hour, sample_minute]))
        .minute_step(5)
        .theme(tokens)
        .transition(page_transition(state.option_motion))
        .z(1200)
        .on_change({
            let gallery_state = state_store.clone();
            move |_, _| {
                gallery_state.update(|state| state.sample_feedback = "Time changed".to_string())
            }
        })
        .build();

    widgets::color_picker(ui, "feedback.colorpicker")
        .open_bind(bind!(state_store, sample_color_open))
        .screen(screen_width, screen_height)
        .size(420.0, 320.0)
        .value_bind(bind!(state_store, sample_color))
        .theme(tokens)
        .transition(page_transition(state.option_motion))
        .z(1200)
        .on_change({
            let gallery_state = state_store.clone();
            move |_| {
                gallery_state.update(|state| state.sample_feedback = "Color changed".to_string())
            }
        })
        .build();

    widgets::toast(ui, "feedback.toast")
        .visible_bind(bind!(state_store, sample_toast_visible))
        .screen(screen_width, screen_height)
        .duration(3.0)
        .title("Gallery Feedback")
        .message(state.sample_feedback.clone())
        .theme(tokens)
        .on_dismiss({
            let feedback_state = state_store.clone();
            move || {
                feedback_state
                    .update(|state| state.sample_feedback = "Toast dismissed".to_string());
            }
        })
        .on_auto_dismiss({
            let feedback_state = state_store.clone();
            move || {
                feedback_state.update(|state| state.sample_feedback = "Ready".to_string());
            }
        })
        .build();
}

fn draw_inspector_window_content(
    ui: &mut Ui,
    screen: sky_engine::ui::neo::Screen,
    tokens: ThemeColorTokens,
) {
    let page = theme::page_visuals(tokens);
    let width = screen.width;
    let height = screen.height;
    ui.stack("inspector.root")
        .size(width, height)
        .content(|ui| {
            ui.rect("inspector.bg")
                .size(width, height)
                .color(tokens.background)
                .build();

            ui.text("inspector.title")
                .x(28.0)
                .y(24.0)
                .size((width - 96.0).max(0.0), 34.0)
                .text("Inspector Window")
                .font_size(28.0)
                .line_height(34.0)
                .color(page.title_color)
                .build();

            ui.text("inspector.note")
                .x(28.0)
                .y(70.0)
                .size((width - 56.0).max(0.0), 24.0)
                .text("This window was opened from a button callback.")
                .font_size(16.0)
                .line_height(22.0)
                .color(page.subtitle_color)
                .build();
        });
}

fn text_sample(
    ui: &mut Ui,
    id: &str,
    text: &str,
    font_size: f32,
    height: f32,
    width: f32,
    color: Color,
    motion: Transition,
) {
    ui.text(id)
        .size(width, height)
        .text(text)
        .font_size(font_size)
        .line_height(height)
        .color(color)
        .transition(motion)
        .build();
}

fn theme_swatch(
    ui: &mut Ui,
    id: &str,
    name: &str,
    color: Color,
    width: f32,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
) {
    ui.stack(id).size(width, 86.0).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, 86.0)
            .color(tokens.surface)
            .radius(12.0)
            .border(1.0, with_alpha(tokens.border, 0.72))
            .build();

        ui.rect(format!("{id}.chip"))
            .x(12.0)
            .y(12.0)
            .size((width - 24.0).max(0.0), 26.0)
            .color(color)
            .radius(8.0)
            .border(
                1.0,
                with_alpha(page.title_color, if color.a < 0.55 { 0.20 } else { 0.08 }),
            )
            .build();

        ui.text(format!("{id}.name"))
            .x(12.0)
            .y(44.0)
            .size((width - 24.0).max(0.0), 20.0)
            .text(name)
            .font_size(14.0)
            .line_height(18.0)
            .color(page.title_color)
            .horizontal_align(HorizontalAlign::Center)
            .build();

        ui.text(format!("{id}.value"))
            .x(12.0)
            .y(64.0)
            .size((width - 24.0).max(0.0), 18.0)
            .text(color_hex(color))
            .font_size(12.0)
            .line_height(15.0)
            .color(page.subtitle_color)
            .horizontal_align(HorizontalAlign::Center)
            .build();
    });
}

fn primitive_property_card(
    ui: &mut Ui,
    id: &str,
    title: &str,
    note: &str,
    color: Color,
    kind: &str,
    width: f32,
    glass: bool,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
) {
    ui.stack(id)
        .size(width, 144.0)
        .visual_state_from(format!("{id}.bg"), 0.95)
        .content(|ui| {
            if kind == "blur" {
                ui.rect(format!("{id}.circle.primary"))
                    .x(width * 0.14)
                    .y(20.0)
                    .size(64.0, 64.0)
                    .color(with_alpha(tokens.primary, 0.95))
                    .radius(32.0)
                    .build();

                ui.rect(format!("{id}.circle.warm"))
                    .x(width * 0.48)
                    .y(62.0)
                    .size(58.0, 58.0)
                    .color(c(1.0, 0.54, 0.18, 0.92))
                    .radius(29.0)
                    .build();

                ui.rect(format!("{id}.circle.cool"))
                    .x(width * 0.66)
                    .y(16.0)
                    .size(46.0, 46.0)
                    .color(c(0.16, 0.82, 0.72, 0.90))
                    .radius(23.0)
                    .build();
            }

            let rect = ui
                .rect(format!("{id}.bg"))
                .size(width, 144.0)
                .states(
                    color,
                    theme::button_hover(tokens, color),
                    theme::button_pressed(tokens, color),
                )
                .radius(18.0)
                .transition(motion);
            let rect = match kind {
                "border" => rect.border(3.0, tokens.primary),
                "shadow" => rect.shadow(28.0, 0.0, 12.0, shadow_color(tokens, 0.34, 0.18)),
                "blur" => rect
                    .opacity(if glass { 1.0 } else { 0.82 })
                    .blur(if glass { 18.0 } else { 0.0 })
                    .border(
                        1.0,
                        if glass {
                            with_alpha(page.title_color, 0.35)
                        } else {
                            with_alpha(tokens.border, 0.70)
                        },
                    ),
                "rotate" => rect.rotate(0.08).transform_origin(0.5, 0.5),
                _ => rect,
            };
            rect.build();

            ui.text(format!("{id}.title"))
                .size(width, 32.0)
                .margin_each(0.0, 36.0, 0.0, 0.0)
                .text(title)
                .font_size(22.0)
                .line_height(28.0)
                .color(page.title_color)
                .horizontal_align(HorizontalAlign::Center)
                .build();

            caption(ui, format!("{id}.note"), note, width, 90.0, page);
        });
}

fn caption(
    ui: &mut Ui,
    id: impl Into<String>,
    text: &str,
    width: f32,
    y: f32,
    page: PageVisualTokens,
) {
    ui.text(id)
        .y(y)
        .size(width, 24.0)
        .text(text)
        .font_size(16.0)
        .line_height(22.0)
        .color(page.subtitle_color)
        .horizontal_align(HorizontalAlign::Center)
        .build();
}

fn feedback_button(
    ui: &mut Ui,
    id: &str,
    text: &str,
    icon_codepoint: u32,
    width: f32,
    tokens: ThemeColorTokens,
    motion: Transition,
    on_click: impl FnMut() + 'static,
) {
    widgets::button(ui, id)
        .size(width, 54.0)
        .icon_codepoint(icon_codepoint)
        .text(text)
        .text_color(tokens.text)
        .icon_color(tokens.primary)
        .radius(12.0)
        .border(1.0, with_alpha(tokens.border, 0.70))
        .shadow(10.0, 0.0, 3.0, shadow_color(tokens, 0.16, 0.08))
        .transition(motion)
        .on_click(on_click)
        .build();
}

fn animation_button(
    ui: &mut Ui,
    id: &str,
    text: &str,
    active: bool,
    active_color: Color,
    width: f32,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
    on_click: impl FnMut() + 'static,
) {
    let normal = if active {
        active_color
    } else {
        tokens.surface_hover
    };
    let text_color = if active || tokens.dark {
        c(0.94, 0.97, 1.0, 1.0)
    } else {
        page.title_color
    };
    widgets::button(ui, id)
        .size(width, 50.0)
        .text(text)
        .colors(
            normal,
            theme::button_hover(tokens, normal),
            theme::button_pressed(tokens, normal),
        )
        .text_color(text_color)
        .icon_color(text_color)
        .border(
            1.0,
            if active {
                with_alpha(active_color, 0.58)
            } else {
                with_alpha(tokens.border, 0.70)
            },
        )
        .shadow(12.0, 0.0, 4.0, shadow_color(tokens, 0.18, 0.08))
        .transition(motion)
        .on_click(on_click)
        .build();
}

fn setting_row(
    ui: &mut Ui,
    id: &str,
    title: &str,
    note: &str,
    enabled: bool,
    width: f32,
    tokens: ThemeColorTokens,
    page: PageVisualTokens,
    motion: Transition,
    binding: Binding<GalleryState, bool>,
) {
    let toggle_x = (width - 80.0).max(0.0);
    let text_width = (width - 132.0).max(0.0);
    let mut switch_style = widgets::SwitchStyle::new(tokens);
    switch_style.on = tokens.primary;
    switch_style.knob = if tokens.dark {
        c(0.96, 0.98, 1.0, 1.0)
    } else {
        c(1.0, 1.0, 1.0, 1.0)
    };

    ui.stack(id).size(width, 72.0).content(|ui| {
        ui.rect(format!("{id}.hit"))
            .size(width, 72.0)
            .states(
                tokens.surface_hover,
                theme::button_hover(tokens, tokens.surface_hover),
                theme::button_pressed(tokens, tokens.surface_hover),
            )
            .radius(16.0)
            .transition(motion)
            .on_click({
                let binding = binding.clone();
                move || binding.set(!enabled)
            })
            .build();

        ui.text(format!("{id}.title"))
            .x(24.0)
            .y(12.0)
            .size(text_width, 28.0)
            .text(title)
            .font_size(20.0)
            .line_height(26.0)
            .color(page.title_color)
            .build();

        ui.text(format!("{id}.note"))
            .x(24.0)
            .y(42.0)
            .size(text_width, 22.0)
            .text(note)
            .font_size(15.0)
            .line_height(20.0)
            .color(page.subtitle_color)
            .build();

        ui.stack(format!("{id}.switch.wrap"))
            .x(toggle_x)
            .y(22.0)
            .size(46.0, 26.0)
            .content(|ui| {
                widgets::switch(ui, format!("{id}.switch"))
                    .size(46.0, 26.0)
                    .track_size(46.0, 26.0)
                    .checked_bind(binding.clone())
                    .style(switch_style)
                    .transition(motion)
                    .build();
            });
    });
}

fn page_body_content_height(page: i32, dense: bool, viewport_height: f32) -> f32 {
    let body_gap = if dense { 18.0 } else { 26.0 };
    let content_height = match page {
        0 => {
            30.0 + 68.0
                + 44.0
                + 92.0
                + 14.0
                + 32.0
                + 46.0
                + 30.0
                + 82.0
                + 22.0
                + 30.0
                + 56.0
                + 200.0
                + 30.0
                + 236.0
                + 30.0
                + 144.0
                + 144.0
                + body_gap * 17.0
                + 56.0
        }
        1 => 380.0 + 30.0 + 88.0 + 88.0 + 30.0 + 88.0 + body_gap * 5.0 + 40.0,
        2 => viewport_height,
        3 => 430.0,
        4 => 440.0,
        _ => viewport_height,
    };
    viewport_height.max(content_height)
}

fn theme_tokens(state: &GallerySnapshot) -> ThemeColorTokens {
    let mut tokens = if state.option_night {
        theme::dark_theme_colors()
    } else {
        theme::light_theme_colors()
    };
    tokens.primary = state.sample_color;
    tokens
}

fn page_transition(enabled: bool) -> Transition {
    if enabled {
        Transition::ease(0.28, Ease::OutCubic)
    } else {
        Transition::default()
    }
}

fn motion_transition(enabled: bool) -> Transition {
    if enabled {
        Transition::ease(0.42, Ease::OutBack)
    } else {
        Transition::default()
    }
}

fn nav_order_for_page(page: i32) -> i32 {
    match page {
        4 => 3,
        3 => 4,
        _ => page.clamp(0, 5),
    }
}

fn shadow_color(tokens: ThemeColorTokens, dark_alpha: f32, light_alpha: f32) -> Color {
    if tokens.dark {
        c(0.0, 0.0, 0.0, dark_alpha)
    } else {
        c(0.10, 0.14, 0.22, light_alpha)
    }
}

fn date_text(state: &GallerySnapshot) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        state.sample_year, state.sample_month, state.sample_day
    )
}

fn time_text(state: &GallerySnapshot) -> String {
    format!("{:02}:{:02}", state.sample_hour, state.sample_minute)
}

fn color_hex(color: Color) -> String {
    let r = (color.r.clamp(0.0, 1.0) * 255.0 + 0.5) as i32;
    let g = (color.g.clamp(0.0, 1.0) * 255.0 + 0.5) as i32;
    let b = (color.b.clamp(0.0, 1.0) * 255.0 + 0.5) as i32;
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn icon(codepoint: u32) -> String {
    char::from_u32(codepoint).unwrap_or('?').to_string()
}

fn bing_api_text() -> String {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .build();
    let Ok(response) = agent
        .get("https://www.bing.com/HPImageArchive.aspx?format=js&n=1&idx=0&mkt=zh-CN")
        .call()
    else {
        return "Network text request failed.".to_string();
    };
    let Ok(body) = response.into_string() else {
        return "Network text request failed.".to_string();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) else {
        return "Bing API returned text data.".to_string();
    };
    json.get("images")
        .and_then(|images| images.get(0))
        .and_then(|image| image.get("copyright"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("Bing API returned text data.")
        .to_string()
}

fn with_alpha(mut color: Color, alpha: f32) -> Color {
    color.a = alpha.clamp(0.0, 1.0);
    color
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

fn env_i32(key: &str) -> Option<i32> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("EUI Gallery", WINDOW_W, WINDOW_H)
                .with_vsync(true)
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

    App::new(world).run(EuiNeoGallery::default());
}
