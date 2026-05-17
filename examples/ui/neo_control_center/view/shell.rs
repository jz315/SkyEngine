use sky_engine::render::Color;
use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{Align, NeoState, Screen, Ui};

use crate::actions;
use crate::locale;
use crate::model::{AppModel, Page};
use crate::theme::{self, AppTheme};
use crate::view::components;
use crate::view::pages;
use crate::view::RuntimeInfo;

const OUTER_PAD: f32 = 24.0;
const SHELL_GAP: f32 = 20.0;
const SIDEBAR_W: f32 = 246.0;
const HEADER_H: f32 = 116.0;

pub fn render_shell(
    ui: &mut Ui,
    screen: Screen,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
) {
    let app_theme = theme::resolve(model.theme_mode);
    let root_w = (screen.width - OUTER_PAD * 2.0).max(0.0);
    let root_h = (screen.height - OUTER_PAD * 2.0).max(0.0);

    draw_background(ui, screen.width, screen.height, app_theme);

    ui.row("control-center.root")
        .x(OUTER_PAD)
        .y(OUTER_PAD)
        .size(root_w, root_h)
        .gap(SHELL_GAP)
        .content(|ui| {
            draw_sidebar(ui, SIDEBAR_W, root_h, state_store, model, app_theme);
            let content_w = (root_w - SIDEBAR_W - SHELL_GAP).max(0.0);
            draw_workspace(
                ui,
                content_w,
                root_h,
                state_store,
                model,
                runtime,
                app_theme,
            );
        });
}

fn draw_background(ui: &mut Ui, width: f32, height: f32, app_theme: AppTheme) {
    ui.rect("control-center.clear")
        .size(width, height)
        .gradient(app_theme.background_top, app_theme.background_bottom)
        .build();

    ui.rect("control-center.glow.a")
        .x(width - 390.0)
        .y(80.0)
        .size(290.0, 290.0)
        .color(theme::alpha(app_theme.tokens.primary, 0.10))
        .radius(145.0)
        .build();

    ui.rect("control-center.glow.b")
        .x(90.0)
        .y(height - 280.0)
        .size(250.0, 250.0)
        .color(theme::alpha(app_theme.success, 0.08))
        .radius(125.0)
        .build();
}

fn draw_sidebar(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    ui.stack("control-center.sidebar")
        .size(width, height)
        .content(|ui| {
            widgets::panel(ui, "control-center.sidebar.bg")
                .size(width, height)
                .color(app_theme.shell)
                .border(1.0, app_theme.shell_edge)
                .shadow(28.0, 0.0, 8.0, theme::alpha(Color::BLACK, 0.15))
                .radius(28.0)
                .build();

            ui.text("control-center.brand.kicker")
                .x(24.0)
                .y(24.0)
                .size(width - 48.0, 18.0)
                .text(locale::app_kicker(model.locale))
                .font_size(12.0)
                .line_height(16.0)
                .color(app_theme.text_muted)
                .build();

            ui.text("control-center.brand.title")
                .x(24.0)
                .y(48.0)
                .size(width - 48.0, 38.0)
                .text(locale::app_title(model.locale))
                .font_size(30.0)
                .line_height(34.0)
                .color(app_theme.tokens.text)
                .build();

            ui.text("control-center.brand.project")
                .x(24.0)
                .y(90.0)
                .size(width - 48.0, 20.0)
                .text(&model.project_name)
                .font_size(14.0)
                .line_height(18.0)
                .color(app_theme.text_soft)
                .build();

            ui.column("control-center.nav")
                .x(18.0)
                .y(148.0)
                .size(width - 36.0, 210.0)
                .gap(12.0)
                .content(|ui| {
                    for page in Page::ALL {
                        nav_button(ui, page, width - 36.0, state_store, model, app_theme);
                    }
                });

            ui.stack("control-center.sidebar.status.wrap")
                .x(18.0)
                .y((height - 246.0).max(0.0))
                .size(width - 36.0, 228.0)
                .content(|ui| {
                    components::section_frame(
                        ui,
                        "control-center.sidebar.status",
                        width - 36.0,
                        228.0,
                        locale::session_title(model.locale),
                        "",
                        app_theme,
                        |ui, body_w, _| {
                            ui.column("control-center.sidebar.status.column")
                                .size(body_w, 150.0)
                                .gap(12.0)
                                .content(|ui| {
                                    components::badge(
                                        ui,
                                        "control-center.sidebar.status.state",
                                        128.0,
                                        locale::run_state_label(model.locale, model.run_state),
                                        theme::run_state_color(app_theme, model.run_state),
                                        app_theme,
                                    );

                                    ui.text("control-center.sidebar.status.summary")
                                        .size(body_w, 42.0)
                                        .text(locale::run_state_summary(
                                            model.locale,
                                            model.run_state,
                                        ))
                                        .font_size(14.0)
                                        .line_height(20.0)
                                        .wrap(true)
                                        .max_width(body_w)
                                        .color(app_theme.text_soft)
                                        .build();

                                    ui.row("control-center.sidebar.status.meta")
                                        .size(body_w, 34.0)
                                        .gap(10.0)
                                        .content(|ui| {
                                            components::badge(
                                                ui,
                                                "control-center.sidebar.status.active",
                                                (body_w - 10.0) * 0.5,
                                                &locale::active_tasks_short(
                                                    model.locale,
                                                    model.active_tasks(),
                                                ),
                                                app_theme.tokens.primary,
                                                app_theme,
                                            );
                                            components::badge(
                                                ui,
                                                "control-center.sidebar.status.urgent",
                                                (body_w - 10.0) * 0.5,
                                                &locale::urgent_tasks_short(
                                                    model.locale,
                                                    model.urgent_tasks(),
                                                ),
                                                app_theme.warning,
                                                app_theme,
                                            );
                                        });
                                });
                        },
                    );
                });
        });
}

fn nav_button(
    ui: &mut Ui,
    page: Page,
    width: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    let selected = model.page == page;
    let base = if selected {
        app_theme.tokens.primary
    } else {
        app_theme.panel_alt
    };
    let hover = if selected {
        theme::mix(base, Color::WHITE, 0.12)
    } else {
        app_theme.tokens.surface_hover
    };
    let pressed = if selected {
        theme::mix(base, Color::BLACK, 0.18)
    } else {
        app_theme.tokens.surface_active
    };
    let text_color = if selected || app_theme.tokens.dark {
        Color::new(0.96, 0.98, 1.0, 1.0)
    } else {
        app_theme.tokens.text
    };
    let page_state = state_store.clone();

    widgets::button(ui, format!("control-center.nav.{}", page.index()))
        .size(width, 58.0)
        .icon_codepoint(page.icon())
        .icon_size(16.0)
        .text(locale::page_label(model.locale, page))
        .font_size(16.0)
        .colors(base, hover, pressed)
        .text_color(text_color)
        .icon_color(text_color)
        .border(
            1.0,
            if selected {
                theme::alpha(app_theme.tokens.primary, 0.62)
            } else {
                app_theme.shell_edge
            },
        )
        .shadow(12.0, 0.0, 4.0, theme::alpha(Color::BLACK, 0.10))
        .radius(18.0)
        .on_click(move || actions::switch_page(&page_state, page))
        .build();
}

fn draw_workspace(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
    app_theme: AppTheme,
) {
    let body_h = (height - HEADER_H - 18.0).max(0.0);
    ui.column("control-center.workspace")
        .size(width, height)
        .gap(18.0)
        .content(|ui| {
            draw_header(ui, width, state_store, model, runtime, app_theme);
            pages::render(ui, width, body_h, state_store, model, runtime, app_theme);
        });
}

fn draw_header(
    ui: &mut Ui,
    width: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
    app_theme: AppTheme,
) {
    components::section_frame(
        ui,
        "control-center.header",
        width,
        HEADER_H + 30.0,
        locale::page_label(model.locale, model.page),
        locale::page_subtitle(model.locale, model.page),
        app_theme,
        |ui, body_w, _| {
            ui.row("control-center.header.meta")
                .size(body_w, 34.0)
                .gap(12.0)
                .align_items(Align::Center)
                .content(|ui| {
                    components::badge(
                        ui,
                        "control-center.header.meta.state",
                        128.0,
                        locale::run_state_label(model.locale, model.run_state),
                        theme::run_state_color(app_theme, model.run_state),
                        app_theme,
                    );
                    components::badge(
                        ui,
                        "control-center.header.meta.uptime",
                        146.0,
                        &locale::header_uptime(
                            model.locale,
                            &format_uptime(runtime.uptime_seconds),
                        ),
                        app_theme.tokens.primary,
                        app_theme,
                    );
                    components::badge(
                        ui,
                        "control-center.header.meta.frames",
                        138.0,
                        &locale::header_frames(model.locale, runtime.frame_count),
                        app_theme.success,
                        app_theme,
                    );
                    let add_state = state_store.clone();
                    widgets::button(ui, "control-center.header.action")
                        .size(150.0, 40.0)
                        .icon_codepoint(0xF067)
                        .text(locale::new_task_action(model.locale))
                        .font_size(15.0)
                        .primary_theme(app_theme.tokens)
                        .radius(16.0)
                        .on_click(move || actions::open_new_task_sheet(&add_state))
                        .build();
                });
        },
    );
}

fn format_uptime(seconds: f32) -> String {
    let total = seconds.max(0.0) as u64;
    let minutes = total / 60;
    let secs = total % 60;
    format!("{minutes:02}:{secs:02}")
}
