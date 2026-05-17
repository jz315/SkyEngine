use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{NeoState, Ui};

use crate::actions;
use crate::locale;
use crate::model::{AppModel, Locale};
use crate::theme::AppTheme;
use crate::view::{components, scroll_panel};

pub fn render(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    let top_h = 216.0;
    let middle_h = 232.0;
    let bottom_h = 210.0;
    let gap = 18.0;
    let content_h = top_h + middle_h + bottom_h + gap * 2.0;

    scroll_panel::scroll_panel(
        ui,
        "settings",
        width,
        height,
        content_h,
        model.settings_scroll,
        actions::bind_settings_scroll(state_store),
        |ui, body_w| {
            let half_w = ((body_w - gap) * 0.5).max(220.0);

            ui.column("settings.page")
                .size(body_w, content_h)
                .gap(gap)
                .content(|ui| {
                    ui.row("settings.top")
                        .size(body_w, top_h)
                        .gap(gap)
                        .content(|ui| {
                            workspace_section(
                                ui,
                                half_w,
                                top_h,
                                state_store,
                                model.locale,
                                app_theme,
                            );
                            sound_section(ui, half_w, top_h, state_store, model, app_theme);
                        });

                    preference_section(ui, body_w, middle_h, state_store, model, app_theme);
                    appearance_section(ui, body_w, bottom_h, state_store, model, app_theme);
                });
        },
    );
}

fn workspace_section(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<AppModel>,
    locale_id: Locale,
    app_theme: AppTheme,
) {
    components::section_frame(
        ui,
        "settings.workspace",
        width,
        height,
        locale::workspace_title(locale_id),
        locale::workspace_subtitle(locale_id),
        app_theme,
        |ui, body_w, _| {
            ui.column("settings.workspace.column")
                .size(body_w, 190.0)
                .gap(12.0)
                .content(|ui| {
                    widgets::input(ui, "settings.workspace.name")
                        .size(body_w, 42.0)
                        .text_bind(actions::bind_project_name(state_store))
                        .placeholder(locale::project_name_placeholder(locale_id))
                        .theme(app_theme.tokens)
                        .build();

                    ui.text("settings.workspace.language.label")
                        .size(body_w, 18.0)
                        .text(locale::language_title(locale_id))
                        .font_size(13.0)
                        .line_height(18.0)
                        .color(app_theme.text_muted)
                        .build();

                    widgets::segmented(ui, "settings.workspace.language")
                        .size(body_w, 38.0)
                        .items(locale::language_items(locale_id))
                        .selected_bind(actions::bind_locale(state_store))
                        .theme(app_theme.tokens)
                        .build();

                    ui.text("settings.workspace.quality.label")
                        .size(body_w, 18.0)
                        .text(locale::quality_preset_title(locale_id))
                        .font_size(13.0)
                        .line_height(18.0)
                        .color(app_theme.text_muted)
                        .build();

                    widgets::dropdown(ui, "settings.workspace.quality")
                        .size(body_w, 42.0)
                        .items(locale::quality_preset_items(locale_id))
                        .selected_bind(actions::bind_quality_preset(state_store))
                        .open_bind(actions::bind_quality_preset_open(state_store))
                        .theme(app_theme.tokens)
                        .build();
                });
        },
    );
}

fn sound_section(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    components::section_frame(
        ui,
        "settings.sound",
        width,
        height,
        locale::sound_scale_title(model.locale),
        locale::sound_scale_subtitle(model.locale),
        app_theme,
        |ui, body_w, _| {
            ui.column("settings.sound.column")
                .size(body_w, 150.0)
                .gap(12.0)
                .content(|ui| {
                    ui.text("settings.sound.scale")
                        .size(body_w, 18.0)
                        .text(&locale::ui_scale_text(
                            model.locale,
                            (model.ui_scale * 100.0).round() as u32,
                        ))
                        .font_size(13.0)
                        .line_height(18.0)
                        .color(app_theme.text_muted)
                        .build();
                    widgets::slider(ui, "settings.sound.scale.slider")
                        .size(body_w, 32.0)
                        .value_bind(actions::bind_ui_scale(state_store))
                        .theme(app_theme.tokens)
                        .build();

                    ui.text("settings.sound.volume")
                        .size(body_w, 18.0)
                        .text(&locale::volume_text(
                            model.locale,
                            (model.volume * 100.0).round() as u32,
                        ))
                        .font_size(13.0)
                        .line_height(18.0)
                        .color(app_theme.text_muted)
                        .build();
                    widgets::slider(ui, "settings.sound.volume.slider")
                        .size(body_w, 32.0)
                        .value_bind(actions::bind_volume(state_store))
                        .theme(app_theme.tokens)
                        .build();
                });
        },
    );
}

fn preference_section(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    components::section_frame(
        ui,
        "settings.preferences",
        width,
        height,
        locale::preferences_title(model.locale),
        locale::preferences_settings_subtitle(model.locale),
        app_theme,
        |ui, body_w, _| {
            widgets::switch(ui, "settings.preferences.notifications")
                .size(body_w, 34.0)
                .checked_bind(actions::bind_notifications(state_store))
                .text(locale::notifications_label(model.locale))
                .theme(app_theme.tokens)
                .build();

            ui.stack("settings.preferences.autosave.wrap")
                .y(46.0)
                .size(body_w, 34.0)
                .content(|ui| {
                    widgets::checkbox(ui, "settings.preferences.autosave")
                        .size(body_w, 30.0)
                        .checked_bind(actions::bind_auto_save(state_store))
                        .text(locale::auto_save_label(model.locale))
                        .theme(app_theme.tokens)
                        .build();
                });

            ui.text("settings.preferences.meta")
                .y(94.0)
                .size(body_w, 38.0)
                .text(locale::preferences_settings_meta(
                    model.locale,
                    model.notifications_enabled,
                    model.auto_save,
                ))
                .font_size(14.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(body_w)
                .color(app_theme.text_soft)
                .build();
        },
    );
}

fn appearance_section(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &NeoState<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    components::section_frame(
        ui,
        "settings.appearance",
        width,
        height,
        locale::appearance_title(model.locale),
        locale::appearance_subtitle(model.locale),
        app_theme,
        |ui, body_w, _| {
            widgets::segmented(ui, "settings.appearance.theme")
                .size(body_w, 38.0)
                .items(locale::theme_mode_items(model.locale))
                .selected_bind(actions::bind_theme_mode(state_store))
                .theme(app_theme.tokens)
                .build();

            ui.stack("settings.appearance.focus.wrap")
                .y(56.0)
                .size(body_w, 38.0)
                .content(|ui| {
                    widgets::segmented(ui, "settings.appearance.focus")
                        .size(body_w, 38.0)
                        .items(locale::focus_mode_items(model.locale))
                        .selected_bind(actions::bind_focus_mode(state_store))
                        .theme(app_theme.tokens)
                        .build();
                });

            ui.text("settings.appearance.caption")
                .y(112.0)
                .size(body_w, 40.0)
                .text(locale::appearance_caption(model.locale))
                .font_size(14.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(body_w)
                .color(app_theme.text_soft)
                .build();
        },
    );
}
