use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{Align, Size, State, Ui};

use crate::actions;
use crate::locale;
use crate::model::{AppModel, Locale, TaskItem};
use crate::theme::{self, AppTheme};
use crate::view::{components, scroll_panel};

pub fn render(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &State<AppModel>,
    model: &AppModel,
    app_theme: AppTheme,
) {
    let top_h = 144.0;
    let row_h = 92.0;
    let row_gap = 14.0;
    let list_content_h = (row_h + row_gap) * model.tasks.len() as f32;
    let list_h = (list_content_h + 108.0).max(220.0);
    let content_h = top_h + list_h + 18.0;

    scroll_panel::scroll_panel(
        ui,
        "tasks",
        width,
        height,
        content_h,
        model.tasks_scroll,
        actions::tasks_scroll_signal(state_store),
        |ui, body_w| {
            ui.column("tasks.page")
                .size(body_w, content_h)
                .gap(18.0)
                .content(|ui| {
                    components::section_frame(
                        ui,
                        "tasks.hero",
                        body_w,
                        top_h,
                        locale::task_queue_title(model.locale),
                        locale::task_queue_subtitle(model.locale),
                        app_theme,
                        |ui, section_w, _| {
                            ui.row("tasks.hero.row")
                                .size(section_w, 42.0)
                                .gap(12.0)
                                .align_items(Align::Center)
                                .content(|ui| {
                                    components::badge(
                                        ui,
                                        "tasks.hero.active",
                                        126.0,
                                        &locale::tasks_active_badge(
                                            model.locale,
                                            model.active_tasks(),
                                        ),
                                        app_theme.tokens.primary,
                                        app_theme,
                                    );
                                    components::badge(
                                        ui,
                                        "tasks.hero.done",
                                        138.0,
                                        &locale::tasks_done_badge(
                                            model.locale,
                                            model.completed_tasks(),
                                        ),
                                        app_theme.success,
                                        app_theme,
                                    );
                                    let add_state = state_store.clone();
                                    widgets::button(ui, "tasks.hero.add")
                                        .size(152.0, 42.0)
                                        .icon_codepoint(0xF067)
                                        .text(locale::create_task_action(model.locale))
                                        .font_size(15.0)
                                        .primary_theme(app_theme.tokens)
                                        .radius(16.0)
                                        .on_click(move || actions::open_new_task_sheet(&add_state))
                                        .build();
                                });
                        },
                    );

                    components::section_frame(
                        ui,
                        "tasks.list",
                        body_w,
                        list_h,
                        locale::live_queue_title(model.locale),
                        locale::live_queue_subtitle(model.locale),
                        app_theme,
                        |ui, section_w, body_h| {
                            ui.column("tasks.list.column")
                                .size(section_w, body_h.max(list_content_h))
                                .gap(row_gap)
                                .content(|ui| {
                                    for task in &model.tasks {
                                        task_card(
                                            ui,
                                            section_w,
                                            row_h,
                                            state_store,
                                            model.locale,
                                            task,
                                            app_theme,
                                        );
                                    }
                                });
                        },
                    );
                });
        },
    );
}

fn task_card(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &State<AppModel>,
    locale_id: Locale,
    task: &TaskItem,
    app_theme: AppTheme,
) {
    let task_id = task.id;
    let accent = theme::priority_color(app_theme, task.priority, task.urgent);
    let toggle_state = state_store.clone();

    ui.stack(format!("tasks.card.{task_id}"))
        .size(width, height)
        .content(|ui| {
            widgets::panel(ui, format!("tasks.card.{task_id}.bg"))
                .fill()
                .color(if task.done {
                    app_theme.panel_alt
                } else {
                    app_theme.panel
                })
                .border(1.0, app_theme.shell_edge)
                .shadow(
                    18.0,
                    0.0,
                    6.0,
                    theme::alpha(sky_engine::ui::neo::Color::BLACK, 0.10),
                )
                .radius(20.0)
                .build();

            ui.row(format!("tasks.card.{task_id}.content"))
                .fill()
                .padding(18.0)
                .gap(14.0)
                .align_items(Align::Center)
                .content(|ui| {
                    ui.rect(format!("tasks.card.{task_id}.accent"))
                        .size(10.0, Size::fill())
                        .color(accent)
                        .radius(5.0)
                        .build();

                    ui.column(format!("tasks.card.{task_id}.copy"))
                        .size(180.0, Size::fill())
                        .grow(1.0)
                        .justify_content(Align::Center)
                        .gap(6.0)
                        .content(|ui| {
                            ui.text(format!("tasks.card.{task_id}.title"))
                                .size(Size::fill(), 26.0)
                                .text(&task.title)
                                .font_size(22.0)
                                .line_height(26.0)
                                .color(app_theme.tokens.text)
                                .build();

                            ui.text(format!("tasks.card.{task_id}.meta"))
                                .size(Size::fill(), 18.0)
                                .text(locale::task_meta(locale_id, task.urgent))
                                .font_size(13.0)
                                .line_height(18.0)
                                .color(app_theme.text_muted)
                                .build();
                        });

                    ui.row(format!("tasks.card.{task_id}.badges"))
                        .size(176.0, 34.0)
                        .gap(10.0)
                        .align_items(Align::Center)
                        .content(|ui| {
                            components::badge(
                                ui,
                                &format!("tasks.card.{task_id}.priority"),
                                72.0,
                                locale::priority_label(locale_id, task.priority),
                                accent,
                                app_theme,
                            );
                            components::badge(
                                ui,
                                &format!("tasks.card.{task_id}.status"),
                                88.0,
                                locale::task_status_label(locale_id, task.done),
                                if task.done {
                                    app_theme.success
                                } else {
                                    app_theme.tokens.primary
                                },
                                app_theme,
                            );
                        });

                    widgets::button(ui, format!("tasks.card.{task_id}.action"))
                        .size(126.0, 46.0)
                        .icon_codepoint(if task.done { 0xF112 } else { 0xF00C })
                        .text(locale::task_action_label(locale_id, task.done))
                        .font_size(14.0)
                        .colors(
                            theme::alpha(accent, 0.14),
                            theme::alpha(accent, 0.20),
                            theme::alpha(accent, 0.28),
                        )
                        .text_color(accent)
                        .icon_color(accent)
                        .border(1.0, theme::alpha(accent, 0.32))
                        .radius(15.0)
                        .on_click(move || actions::toggle_task_done(&toggle_state, task_id))
                        .build();
                });
        });
}
