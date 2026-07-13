use sky_engine::ui::serein::widgets;
use sky_engine::ui::serein::{Align, Size, State, Ui};

use crate::actions;
use crate::locale;
use crate::model::AppModel;
use crate::theme::{self, AppTheme};
use crate::view::RuntimeInfo;
use crate::view::{components, scroll_panel};

pub fn render(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &State<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
    app_theme: AppTheme,
) {
    let gap = 18.0;
    let stats_h = 128.0;
    let hero_h = 262.0;
    let signals_h = 188.0;
    let content_h = stats_h + hero_h + signals_h + gap * 2.0 + 20.0;
    scroll_panel::scroll_panel(
        ui,
        "overview",
        width,
        height,
        content_h,
        model.overview_scroll,
        actions::overview_scroll_signal(state_store),
        |ui, body_w| {
            let card_width = ((body_w - gap * 2.0) / 3.0).max(120.0);
            let hero_w = (body_w * 0.62).max(320.0);
            let side_w = (body_w - hero_w - gap).max(220.0);

            ui.column("overview.page")
                .size(body_w, content_h)
                .gap(gap)
                .content(|ui| {
                    ui.row("overview.stats")
                        .size(body_w, stats_h)
                        .gap(gap)
                        .content(|ui| {
                            components::stat_card(
                                ui,
                                "overview.stats.active",
                                card_width,
                                stats_h,
                                locale::active_tasks_title(model.locale),
                                &model.active_tasks().to_string(),
                                locale::active_tasks_meta(model.locale),
                                app_theme.tokens.primary,
                                app_theme,
                            );
                            components::stat_card(
                                ui,
                                "overview.stats.progress",
                                card_width,
                                stats_h,
                                locale::completion_title(model.locale),
                                &format!("{}%", model.completion_percent()),
                                locale::completion_meta(model.locale),
                                app_theme.success,
                                app_theme,
                            );
                            components::stat_card(
                                ui,
                                "overview.stats.mode",
                                card_width,
                                stats_h,
                                locale::focus_mode_title(model.locale),
                                locale::focus_mode_label(model.locale, model.focus_mode),
                                locale::focus_mode_meta(model.locale),
                                app_theme.warning,
                                app_theme,
                            );
                        });

                    ui.row("overview.hero")
                        .size(body_w, hero_h)
                        .gap(gap)
                        .content(|ui| {
                            draw_launchpad(ui, hero_w, state_store, model, runtime, app_theme);
                            draw_side_panels(ui, side_w, model, app_theme);
                        });

                    draw_signal_strip(ui, body_w, model, app_theme);
                });
        },
    );
}

fn draw_launchpad(
    ui: &mut Ui,
    width: f32,
    state_store: &State<AppModel>,
    model: &AppModel,
    _runtime: RuntimeInfo,
    app_theme: AppTheme,
) {
    components::section_frame(
        ui,
        "overview.launchpad",
        width,
        264.0,
        locale::launchpad_title(model.locale),
        locale::launchpad_subtitle(model.locale),
        app_theme,
        |ui, body_w, _| {
            ui.column("overview.launchpad.column")
                .size(body_w, 180.0)
                .gap(12.0)
                .content(|ui| {
                    ui.text("overview.launchpad.value")
                        .size(body_w, 42.0)
                        .text(locale::run_state_summary(model.locale, model.run_state))
                        .font_size(28.0)
                        .line_height(34.0)
                        .wrap(true)
                        .max_width(body_w)
                        .color(app_theme.tokens.text)
                        .build();

                    components::progress_bar(
                        ui,
                        "overview.launchpad.progress",
                        model.completion_ratio(),
                        app_theme,
                    );

                    ui.text("overview.launchpad.progress.label")
                        .size(body_w, 18.0)
                        .text(locale::completion_text(
                            model.locale,
                            model.completed_tasks(),
                            model.tasks.len(),
                        ))
                        .font_size(13.0)
                        .line_height(18.0)
                        .color(app_theme.text_muted)
                        .build();

                    ui.row("overview.launchpad.actions")
                        .size(body_w, 56.0)
                        .gap(14.0)
                        .content(|ui| {
                            let run_state = state_store.clone();
                            widgets::button(ui, "overview.launchpad.actions.run")
                                .size((body_w - 28.0) / 3.0, 50.0)
                                .icon_codepoint(0xF04B)
                                .text(locale::start_run_action(model.locale))
                                .font_size(15.0)
                                .primary_theme(app_theme.tokens)
                                .radius(16.0)
                                .on_click(move || actions::advance_run(&run_state))
                                .build();

                            let pause_state = state_store.clone();
                            widgets::button(ui, "overview.launchpad.actions.pause")
                                .size((body_w - 28.0) / 3.0, 50.0)
                                .icon_codepoint(0xF04C)
                                .text(locale::pause_action(model.locale))
                                .font_size(15.0)
                                .secondary_theme(app_theme.tokens)
                                .radius(16.0)
                                .on_click(move || actions::pause_run(&pause_state))
                                .build();

                            let ship_state = state_store.clone();
                            widgets::button(ui, "overview.launchpad.actions.ship")
                                .size((body_w - 28.0) / 3.0, 50.0)
                                .icon_codepoint(0xF0EE)
                                .text(locale::ship_build_action(model.locale))
                                .font_size(15.0)
                                .colors(
                                    theme::alpha(app_theme.warning, 0.18),
                                    theme::alpha(app_theme.warning, 0.24),
                                    theme::alpha(app_theme.warning, 0.34),
                                )
                                .text_color(app_theme.warning)
                                .icon_color(app_theme.warning)
                                .border(1.0, theme::alpha(app_theme.warning, 0.34))
                                .radius(16.0)
                                .on_click(move || actions::open_ship_dialog(&ship_state))
                                .build();
                        });
                });
        },
    );
}

fn draw_side_panels(ui: &mut Ui, width: f32, model: &AppModel, app_theme: AppTheme) {
    ui.column("overview.side")
        .size(width, 262.0)
        .gap(18.0)
        .content(|ui| {
            components::section_frame(
                ui,
                "overview.side.next",
                width,
                110.0,
                locale::next_up_title(model.locale),
                "",
                app_theme,
                |ui, body_w, _| {
                    let summary = model
                        .next_task()
                        .map(|task| task.title.as_str())
                        .unwrap_or(locale::next_up_empty(model.locale));
                    ui.text("overview.side.next.text")
                        .size(body_w, 36.0)
                        .text(summary)
                        .font_size(18.0)
                        .line_height(24.0)
                        .wrap(true)
                        .max_width(body_w)
                        .color(app_theme.tokens.text)
                        .build();
                },
            );

            components::section_frame(
                ui,
                "overview.side.notes",
                width,
                134.0,
                locale::atmosphere_title(model.locale),
                "",
                app_theme,
                |ui, body_w, _| {
                    ui.row("overview.side.notes.row")
                        .size(body_w, 34.0)
                        .gap(12.0)
                        .align_items(Align::Center)
                        .content(|ui| {
                            components::badge(
                                ui,
                                "overview.side.notes.theme",
                                132.0,
                                locale::theme_mode_label(model.locale, model.theme_mode),
                                app_theme.tokens.primary,
                                app_theme,
                            );
                            components::badge(
                                ui,
                                "overview.side.notes.quality",
                                118.0,
                                locale::quality_preset_label(model.locale, model.quality_preset),
                                app_theme.success,
                                app_theme,
                            );
                        });

                    ui.text("overview.side.notes.summary")
                        .size(Size::fill(), 18.0)
                        .text(locale::atmosphere_summary(model.locale))
                        .font_size(13.0)
                        .line_height(18.0)
                        .color(app_theme.text_muted)
                        .build();
                },
            );
        });
}

fn draw_signal_strip(ui: &mut Ui, width: f32, model: &AppModel, app_theme: AppTheme) {
    let gap = 18.0;
    let strip_w = ((width - gap) * 0.5).max(220.0);
    ui.row("overview.signals")
        .size(width, 162.0)
        .gap(gap)
        .content(|ui| {
            components::section_frame(
                ui,
                "overview.signals.queue",
                strip_w,
                162.0,
                locale::queue_texture_title(model.locale),
                locale::queue_texture_subtitle(model.locale),
                app_theme,
                |ui, body_w, _| {
                    let urgent = model.urgent_tasks();
                    ui.text("overview.signals.queue.value")
                        .size(body_w, 28.0)
                        .text(locale::urgent_items_text(model.locale, urgent))
                        .font_size(24.0)
                        .line_height(28.0)
                        .color(if urgent > 0 {
                            app_theme.warning
                        } else {
                            app_theme.success
                        })
                        .build();
                    ui.text("overview.signals.queue.meta")
                        .size(Size::fill(), 36.0)
                        .text(locale::queue_texture_meta(model.locale))
                        .font_size(14.0)
                        .line_height(20.0)
                        .wrap(true)
                        .max_width(body_w)
                        .color(app_theme.text_soft)
                        .build();
                },
            );

            components::section_frame(
                ui,
                "overview.signals.preferences",
                strip_w,
                162.0,
                locale::preferences_title(model.locale),
                locale::preferences_subtitle(model.locale),
                app_theme,
                |ui, body_w, _| {
                    ui.row("overview.signals.preferences.row")
                        .size(body_w, 34.0)
                        .gap(12.0)
                        .content(|ui| {
                            components::badge(
                                ui,
                                "overview.signals.preferences.focus",
                                132.0,
                                locale::focus_mode_label(model.locale, model.focus_mode),
                                app_theme.tokens.primary,
                                app_theme,
                            );
                            components::badge(
                                ui,
                                "overview.signals.preferences.notifications",
                                154.0,
                                locale::notifications_badge(
                                    model.locale,
                                    model.notifications_enabled,
                                ),
                                if model.notifications_enabled {
                                    app_theme.success
                                } else {
                                    app_theme.text_muted
                                },
                                app_theme,
                            );
                        });

                    ui.text("overview.signals.preferences.meta")
                        .size(Size::fill(), 40.0)
                        .text(locale::preferences_meta(model.locale))
                        .font_size(14.0)
                        .line_height(20.0)
                        .wrap(true)
                        .max_width(body_w)
                        .color(app_theme.text_soft)
                        .build();
                },
            );
        });
}
