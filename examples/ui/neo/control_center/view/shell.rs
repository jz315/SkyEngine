use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::Color;
use sky_engine::ui::neo::{Align, Screen, Size, State, Ui};

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
const HEADER_H: f32 = 146.0;

pub fn render_shell(
    ui: &mut Ui,
    screen: Screen,
    state_store: &State<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
) {
    let app_theme = theme::resolve(model.theme_mode);
    let root_w = (screen.width - OUTER_PAD * 2.0).max(0.0);
    let root_h = (screen.height - OUTER_PAD * 2.0).max(0.0);
    let workspace_w = (root_w - SIDEBAR_W - SHELL_GAP).max(0.0);

    draw_background(ui, screen.width, screen.height, app_theme);

    ui.row("control-center.root")
        .size(screen.width, screen.height)
        .padding(OUTER_PAD)
        .gap(SHELL_GAP)
        .content(|ui| {
            draw_sidebar(ui, state_store, model, app_theme);
            draw_workspace(
                ui,
                workspace_w,
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
}

fn draw_sidebar(ui: &mut Ui, state_store: &State<AppModel>, model: &AppModel, app_theme: AppTheme) {
    ui.stack("control-center.sidebar")
        .size(SIDEBAR_W, Size::fill())
        .content(|ui| {
            widgets::panel(ui, "control-center.sidebar.bg")
                .fill()
                .color(app_theme.shell)
                .border(1.0, app_theme.shell_edge)
                .shadow(28.0, 0.0, 8.0, theme::alpha(Color::BLACK, 0.15))
                .radius(28.0)
                .build();

            ui.column("control-center.sidebar.content")
                .fill()
                .padding_each(18.0, 24.0, 18.0, 18.0)
                .gap(24.0)
                .content(|ui| {
                    ui.column("control-center.brand")
                        .size(Size::fill(), 96.0)
                        .gap(6.0)
                        .content(|ui| {
                            ui.text("control-center.brand.kicker")
                                .size(Size::fill(), 18.0)
                                .text(locale::app_kicker(model.locale))
                                .font_size(12.0)
                                .line_height(16.0)
                                .color(app_theme.text_muted)
                                .build();

                            ui.text("control-center.brand.title")
                                .size(Size::fill(), 38.0)
                                .text(locale::app_title(model.locale))
                                .font_size(30.0)
                                .line_height(34.0)
                                .color(app_theme.tokens.text)
                                .build();

                            ui.text("control-center.brand.project")
                                .size(Size::fill(), 20.0)
                                .text(&model.project_name)
                                .font_size(14.0)
                                .line_height(18.0)
                                .color(app_theme.text_soft)
                                .build();
                        });

                    let mut nav = widgets::nav_group(ui, "control-center.nav")
                        .size(Size::fill(), 210.0)
                        .theme(app_theme.tokens)
                        .signal(actions::page_signal(state_store));
                    for page in Page::ALL {
                        nav = nav.item_icon(
                            page.index(),
                            page.icon(),
                            locale::page_label(model.locale, page),
                        );
                    }
                    nav.build();

                    components::spacer(ui, "control-center.sidebar.flex");

                    components::section_frame(
                        ui,
                        "control-center.sidebar.status",
                        SIDEBAR_W - 36.0,
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

fn draw_workspace(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &State<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
    app_theme: AppTheme,
) {
    let body_h = (height - HEADER_H - 18.0).max(0.0);
    ui.column("control-center.workspace")
        .size(320.0, Size::fill())
        .grow(1.0)
        .min_width(420.0)
        .gap(18.0)
        .content(|ui| {
            draw_header(ui, state_store, model, runtime, app_theme);
            pages::render(ui, width, body_h, state_store, model, runtime, app_theme);
        });
}

fn draw_header(
    ui: &mut Ui,
    state_store: &State<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
    app_theme: AppTheme,
) {
    components::section_frame(
        ui,
        "control-center.header",
        Size::fill(),
        HEADER_H,
        locale::page_label(model.locale, model.page),
        locale::page_subtitle(model.locale, model.page),
        app_theme,
        |ui, _, _| {
            ui.row("control-center.header.meta")
                .size(Size::fill(), 34.0)
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
                    components::spacer(ui, "control-center.header.meta.flex");
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
