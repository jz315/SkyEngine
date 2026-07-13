use sky_engine::ui::serein::{HorizontalAlign, Signal, Size, State, Ui};

use crate::actions::{self, ActionDefinition};
use crate::content;
use crate::model::{GameMode, GameSession};
use crate::theme::{self, AppTheme};
use crate::view::components;

const ACTION_ROW_HEIGHT: f32 = 62.0;
const ACTION_ROW_GAP: f32 = 8.0;
const RECAP_ROW_HEIGHT: f32 = 78.0;
const RECAP_ROW_GAP: f32 = 8.0;

pub fn draw(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let actions = actions::scene_actions(&session.state);
    let list_h = (height - 78.0).max(120.0);
    let content_h = action_list_height(actions.len()).max(list_h);
    ui.stack("right.actions").size(width, height).content(|ui| {
        components::panel(ui, "right.actions.bg", width, height, app_theme);
        ui.column("right.actions.content")
            .x(18.0)
            .y(18.0)
            .size(width - 36.0, height - 36.0)
            .gap(10.0)
            .content(|ui| {
                ui.text("right.actions.title")
                    .size(width - 36.0, 28.0)
                    .text(if session.mode == GameMode::Ending {
                        "通关回顾"
                    } else if session.state.final_train_due() {
                        "最终选择"
                    } else {
                        "此刻可做"
                    })
                    .font_size(22.0)
                    .line_height(26.0)
                    .color(app_theme.text)
                    .build();
                if session.mode == GameMode::Ending {
                    draw_run_recap(ui, width - 36.0, list_h, state, session, app_theme);
                    return;
                }
                let max_scroll = (content_h - list_h).max(0.0);
                let scroll_offset = session.action_scroll.clamp(0.0, max_scroll);
                ui.scroll_y("right.actions.list")
                    .size(width - 36.0, list_h)
                    .content_height(content_h)
                    .gap(0.0)
                    .theme(app_theme.tokens)
                    .scrollbar_gap(10.0)
                    .offset_signal(action_scroll_signal(state))
                    .content(|ui| {
                        let mut list = components::VirtualList::new(
                            "right.actions.virtual",
                            scroll_offset,
                            list_h,
                        );
                        for (index, action) in actions.iter().enumerate() {
                            let row_h = action_row_height(index, actions.len());
                            list.row(
                                ui,
                                format!("right.actions.row.{index}"),
                                width - 56.0,
                                row_h,
                                |ui| {
                                    draw_action_button(
                                        ui,
                                        width - 56.0,
                                        state,
                                        action,
                                        index,
                                        session,
                                        app_theme,
                                    );
                                },
                            );
                        }
                        list.finish(ui, width - 56.0);
                    });
            });
    });
}

fn draw_run_recap(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let recap = content::run_recap(&session.state);
    let content_h = recap_list_height(recap.len()).max(height);
    let max_scroll = (content_h - height).max(0.0);
    let scroll_offset = session.action_scroll.clamp(0.0, max_scroll);
    ui.scroll_y("right.recap.list")
        .size(width, height)
        .content_height(content_h)
        .gap(0.0)
        .theme(app_theme.tokens)
        .scrollbar_gap(10.0)
        .offset_signal(action_scroll_signal(state))
        .content(|ui| {
            let mut list =
                components::VirtualList::new("right.recap.virtual", scroll_offset, height);
            for (index, line) in recap.iter().enumerate() {
                let row_h = recap_row_height(index, recap.len());
                list.row(
                    ui,
                    format!("right.recap.row.{index}"),
                    width - 20.0,
                    row_h,
                    |ui| {
                        draw_recap_card(ui, width - 20.0, index, line, app_theme);
                    },
                );
            }
            list.finish(ui, width - 20.0);
        });
}

fn draw_recap_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    line: &content::RunRecapLine,
    app_theme: AppTheme,
) {
    let height = 78.0;
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(line.progress.min(100)) / 100.0;
    let accent = if line.progress >= 100 {
        app_theme.accent_warm
    } else {
        app_theme.accent
    };

    ui.stack(format!("right.recap.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.recap.{index}.bg"))
                .size(width, height)
                .color(app_theme.panel_alt)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.recap.{index}.label"))
                .x(12.0)
                .y(8.0)
                .size(82.0, 18.0)
                .text(line.label)
                .font_size(12.0)
                .line_height(16.0)
                .color(app_theme.text_muted)
                .build();
            ui.text(format!("right.recap.{index}.value"))
                .x(92.0)
                .y(8.0)
                .size(width - 104.0, 18.0)
                .text(line.value.clone())
                .font_size(13.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.recap.{index}.detail"))
                .x(12.0)
                .y(30.0)
                .size(width - 24.0, 30.0)
                .text(line.detail.clone())
                .font_size(10.0)
                .line_height(14.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.recap.{index}.track"))
                .x(12.0)
                .y(height - 12.0)
                .size(bar_w, 5.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.recap.{index}.fill"))
                .x(12.0)
                .y(height - 12.0)
                .size(fill_w, 5.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_action_button(
    ui: &mut Ui,
    width: f32,
    state: &State<GameSession>,
    action: &ActionDefinition,
    index: usize,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let action_id = action.id;
    let enabled = action.enabled && session.mode == GameMode::Playing;
    let action_state = state.clone();
    ui.stack(format!("right.action.{index}"))
        .size(width, ACTION_ROW_HEIGHT)
        .content(|ui| {
            components::text_button(
                ui,
                format!("right.action.{index}.button"),
                Size::fill(),
                ACTION_ROW_HEIGHT,
                "",
                session.state.final_train_due() && enabled,
                enabled,
                app_theme,
                move || action_state.update(|session| actions::apply(session, action_id)),
            );
            ui.text(format!("right.action.{index}.label"))
                .x(14.0)
                .y(10.0)
                .size(width - 28.0, 18.0)
                .text(action.label.clone())
                .font_size(14.0)
                .line_height(18.0)
                .color(theme::alpha(
                    app_theme.text,
                    if enabled { 1.0 } else { 0.54 },
                ))
                .horizontal_align(HorizontalAlign::Center)
                .build();
            ui.text(format!("right.action.{index}.detail"))
                .x(14.0)
                .y(33.0)
                .size(width - 28.0, 24.0)
                .text(action.detail.clone())
                .font_size(10.0)
                .line_height(12.0)
                .wrap(true)
                .max_width(width - 28.0)
                .color(theme::alpha(
                    app_theme.text_muted,
                    if enabled { 0.92 } else { 0.50 },
                ))
                .horizontal_align(HorizontalAlign::Center)
                .build();
        });
}

fn action_scroll_signal(state: &State<GameSession>) -> Signal<GameSession, f32> {
    state.signal(
        "fog.action-scroll",
        |session| session.action_scroll,
        |session, value| session.action_scroll = value.max(0.0),
    )
}

fn action_list_height(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        count as f32 * ACTION_ROW_HEIGHT + count.saturating_sub(1) as f32 * ACTION_ROW_GAP
    }
}

fn action_row_height(index: usize, count: usize) -> f32 {
    ACTION_ROW_HEIGHT
        + if index + 1 == count {
            0.0
        } else {
            ACTION_ROW_GAP
        }
}

fn recap_list_height(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        count as f32 * RECAP_ROW_HEIGHT + count.saturating_sub(1) as f32 * RECAP_ROW_GAP
    }
}

fn recap_row_height(index: usize, count: usize) -> f32 {
    RECAP_ROW_HEIGHT
        + if index + 1 == count {
            0.0
        } else {
            RECAP_ROW_GAP
        }
}
