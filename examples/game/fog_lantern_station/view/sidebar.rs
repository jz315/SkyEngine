use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{State, Ui};

use crate::actions;
use crate::content;
use crate::model::{GameMode, GameSession, Location, MAX_NIGHT_MINUTES};
use crate::theme::AppTheme;
use crate::view::{commands, components};

pub fn draw(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let inner_w = (width - 36.0).max(0.0);
    let archive_h = if session.notice.is_some() {
        106.0
    } else {
        78.0
    };
    let top_h = (height - 36.0 - archive_h - 14.0).max(0.0);
    let locations_h = (top_h - 156.0).clamp(132.0, 292.0);

    ui.stack("left").size(width, height).content(|ui| {
        components::panel(ui, "left.bg", width, height, app_theme);
        ui.column("left.content")
            .x(18.0)
            .y(18.0)
            .size(inner_w, top_h)
            .gap(12.0)
            .clip()
            .z_index(1)
            .content(|ui| {
                ui.text("left.title")
                    .size(inner_w, 34.0)
                    .text("雾灯站")
                    .font_size(28.0)
                    .line_height(32.0)
                    .color(app_theme.text)
                    .build();

                components::body_text(
                    ui,
                    "left.pressure",
                    content::pressure_text(&session.state),
                    inner_w,
                    58.0,
                    app_theme.text_soft,
                    14.0,
                );
                widgets::progress(ui, "left.progress")
                    .size(inner_w, 8.0)
                    .value(session.state.elapsed_minutes() as f32 / MAX_NIGHT_MINUTES as f32)
                    .style(widgets::ProgressStyle {
                        track: app_theme.panel_strong,
                        fill: if session.state.final_train_due() {
                            app_theme.danger
                        } else {
                            app_theme.accent_warm
                        },
                    })
                    .build();

                components::section_label(ui, "left.location.label", "地点（不耗时）", inner_w);
                draw_location_buttons(ui, inner_w, locations_h, state, session, app_theme);
            });

        ui.column("left.archive")
            .x(18.0)
            .y((height - archive_h - 18.0).max(18.0))
            .size(inner_w, archive_h)
            .gap(8.0)
            .clip()
            .z_index(2)
            .content(|ui| {
                components::section_label(ui, "left.save.label", "存档", inner_w);
                draw_save_row(ui, inner_w, state, session, app_theme);

                if let Some(notice) = &session.notice {
                    components::body_text(
                        ui,
                        "left.notice",
                        notice,
                        inner_w,
                        26.0,
                        app_theme.accent,
                        12.0,
                    );
                }
            });
    });
}

fn draw_location_buttons(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let gap = 7.0;
    let button_h = ((height - gap * (Location::ALL.len() as f32 - 1.0))
        / Location::ALL.len() as f32)
        .clamp(28.0, 40.0);
    let list_h = button_h * Location::ALL.len() as f32 + gap * (Location::ALL.len() as f32 - 1.0);

    ui.column("left.locations")
        .size(width, list_h)
        .gap(gap)
        .content(|ui| {
            for location in Location::ALL {
                let selected = session.state.location == location;
                let can_move = !selected
                    && !session.state.final_train_due()
                    && session.state.ended.is_none()
                    && session.mode == GameMode::Playing;
                let label = if selected {
                    format!("{}  当前", location.title())
                } else {
                    location.title().to_string()
                };
                let move_state = state.clone();
                components::text_button(
                    ui,
                    format!("left.location.{}", location.id()),
                    width,
                    button_h,
                    label,
                    selected,
                    selected || can_move,
                    app_theme,
                    move || {
                        if can_move {
                            move_state.update(|session| {
                                actions::apply(session, crate::model::ActionId::Move(location))
                            });
                        }
                    },
                );
            }
        });
}

fn draw_save_row(
    ui: &mut Ui,
    width: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let gap = 8.0;
    let button_w = ((width - gap) * 0.5).max(88.0);

    ui.row("left.save.row")
        .size(width, 40.0)
        .gap(gap)
        .content(|ui| {
            let save_state = state.clone();
            components::text_button(
                ui,
                "left.save",
                button_w,
                38.0,
                "保存游戏",
                false,
                session.mode != GameMode::Title,
                app_theme,
                move || commands::save_current_state(&save_state),
            );

            let load_state = state.clone();
            components::text_button(
                ui,
                "left.load",
                button_w,
                38.0,
                "读取存档",
                false,
                true,
                app_theme,
                move || commands::load_into_state(&load_state),
            );
        });
}
