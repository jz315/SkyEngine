use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::Color;
use sky_engine::ui::neo::{Align, Screen, Size, State, Ui};

use crate::actions;
use crate::locale;
use crate::model::AppModel;
use crate::theme::{self, AppTheme};

pub fn render(ui: &mut Ui, screen: Screen, state_store: &State<AppModel>, model: &AppModel) {
    let app_theme = theme::resolve(model.theme_mode);
    render_new_task_sheet(ui, screen, state_store, model, app_theme);
    render_ship_dialog(ui, screen, state_store, model, app_theme);
    render_toast(ui, screen, state_store, model, app_theme);
}

fn render_new_task_sheet(
    ui: &mut Ui,
    screen: Screen,
    state_store: &State<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    if !model.new_task_sheet_open {
        return;
    }

    let width = (screen.width - 64.0).min(520.0);
    let height = 338.0;
    let close_state = state_store.clone();
    let submit_state = state_store.clone();

    ui.stack("overlays.task-sheet")
        .size(screen.width, screen.height)
        .z_index(950)
        .content(|ui| {
            ui.rect("overlays.task-sheet.backdrop")
                .size(screen.width, screen.height)
                .color(theme::alpha(
                    Color::BLACK,
                    if app_theme.tokens.dark { 0.46 } else { 0.20 },
                ))
                .on_click(move || actions::close_new_task_sheet(&close_state))
                .build();

            ui.stack("overlays.task-sheet.center")
                .fill()
                .padding(24.0)
                .align(Align::Center, Align::Center)
                .content(|ui| {
                    ui.stack("overlays.task-sheet.panel")
                        .size(width, height)
                        .content(|ui| {
                            widgets::panel(ui, "overlays.task-sheet.panel.bg")
                                .fill()
                                .color(app_theme.panel)
                                .border(1.0, app_theme.shell_edge)
                                .shadow(30.0, 0.0, 10.0, theme::alpha(Color::BLACK, 0.18))
                                .radius(26.0)
                                .build();

                            ui.column("overlays.task-sheet.layout")
                                .fill()
                                .padding(24.0)
                                .gap(14.0)
                                .content(|ui| {
                                    ui.text("overlays.task-sheet.title")
                                        .size(Size::fill(), 32.0)
                                        .text(locale::task_sheet_title(model.locale))
                                        .font_size(28.0)
                                        .line_height(32.0)
                                        .color(app_theme.tokens.text)
                                        .build();

                                    ui.text("overlays.task-sheet.subtitle")
                                        .size(Size::fill(), 22.0)
                                        .text(locale::task_sheet_subtitle(model.locale))
                                        .font_size(14.0)
                                        .line_height(20.0)
                                        .color(app_theme.text_muted)
                                        .build();

                                    ui.column("overlays.task-sheet.form")
                                        .size(Size::fill(), 128.0)
                                        .gap(14.0)
                                        .content(|ui| {
                                            widgets::input(ui, "overlays.task-sheet.form.title")
                                                .size(Size::fill(), 42.0)
                                                .text_signal(actions::draft_title_signal(
                                                    state_store,
                                                ))
                                                .placeholder(locale::task_title_placeholder(
                                                    model.locale,
                                                ))
                                                .theme(app_theme.tokens)
                                                .build();

                                            widgets::segmented(
                                                ui,
                                                "overlays.task-sheet.form.priority",
                                            )
                                            .size(width - 48.0, 38.0)
                                            .items(locale::priority_items(model.locale))
                                            .signal(actions::draft_priority_signal(state_store))
                                            .theme(app_theme.tokens)
                                            .build();

                                            widgets::switch(ui, "overlays.task-sheet.form.urgent")
                                                .size(width - 48.0, 34.0)
                                                .signal(actions::draft_urgent_signal(state_store))
                                                .text(locale::mark_urgent_label(model.locale))
                                                .theme(app_theme.tokens)
                                                .build();
                                        });

                                    ui.row("overlays.task-sheet.actions")
                                        .size(Size::fill(), 48.0)
                                        .gap(14.0)
                                        .align_items(Align::Center)
                                        .content(|ui| {
                                            let cancel_state = state_store.clone();
                                            widgets::button(
                                                ui,
                                                "overlays.task-sheet.actions.cancel",
                                            )
                                            .size(120.0, 46.0)
                                            .grow(1.0)
                                            .text(locale::cancel_action(model.locale))
                                            .font_size(15.0)
                                            .secondary_theme(app_theme.tokens)
                                            .radius(16.0)
                                            .on_click(move || {
                                                actions::close_new_task_sheet(&cancel_state)
                                            })
                                            .build();

                                            widgets::button(
                                                ui,
                                                "overlays.task-sheet.actions.create",
                                            )
                                            .size(120.0, 46.0)
                                            .grow(1.0)
                                            .icon_codepoint(0xF067)
                                            .text(locale::create_action(model.locale))
                                            .font_size(15.0)
                                            .primary_theme(app_theme.tokens)
                                            .radius(16.0)
                                            .on_click(move || {
                                                actions::submit_new_task(&submit_state)
                                            })
                                            .build();
                                        });
                                });
                        });
                });
        });
}

fn render_ship_dialog(
    ui: &mut Ui,
    screen: Screen,
    state_store: &State<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    widgets::dialog(ui, "overlays.ship-dialog")
        .screen(screen.width, screen.height)
        .size(448.0, 230.0)
        .title(locale::ship_dialog_title(model.locale))
        .message(locale::ship_dialog_message(model.locale))
        .primary_text(locale::ship_dialog_primary(model.locale))
        .secondary_text(locale::ship_dialog_secondary(model.locale))
        .theme(app_theme.tokens)
        .open(model.ship_dialog_open)
        .on_primary({
            let ship_state = state_store.clone();
            move || actions::confirm_ship(&ship_state)
        })
        .on_close({
            let close_state = state_store.clone();
            move || actions::close_ship_dialog(&close_state)
        })
        .build();
}

fn render_toast(
    ui: &mut Ui,
    screen: Screen,
    state_store: &State<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    widgets::toast(ui, "overlays.toast")
        .screen(screen.width, screen.height)
        .title(model.toast.title.clone())
        .message(model.toast.message.clone())
        .theme(app_theme.tokens)
        .visible_signal(actions::toast_visible_signal(state_store))
        .duration(2.8)
        .build();
}
