use sky_engine::ui::neo::{Signal, State, Ui};

use crate::content;
use crate::dialogue_system;
use crate::model::GameSession;
use crate::theme::AppTheme;
use crate::view::{components, scene};

const GAP: f32 = 16.0;

pub fn draw(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    ui.column("story")
        .size(width, height)
        .gap(GAP)
        .content(|ui| {
            scene::draw_station_scene(
                ui,
                "story.scene",
                width,
                248.0,
                session.state.location,
                app_theme,
            );
            draw_narrative(ui, width, height - 248.0 - GAP, state, session, app_theme);
        });
}

fn draw_narrative(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    ui.stack("story.narrative")
        .size(width, height)
        .content(|ui| {
            components::panel(ui, "story.narrative.bg", width, height, app_theme);
            let inner_w = width - 44.0;
            let viewport_h = (height - 44.0).max(160.0);
            let title = session
                .latest()
                .map(|event| event.title.as_str())
                .unwrap_or(session.state.location.title());
            let dialogue_summary = dialogue_system::active_thread_summary(&session.state);
            let body = session
                .latest()
                .map(|event| event.body.clone())
                .unwrap_or_else(|| content::location_description(&session.state));
            let body_h = wrapped_height(&body, inner_w, 18.0, 160.0);
            let summary_h = if dialogue_summary.is_some() {
                24.0
            } else {
                0.0
            };
            let gap_count = if dialogue_summary.is_some() { 3.0 } else { 2.0 };
            let content_h = 36.0 + summary_h + body_h + 30.0 + gap_count * 14.0;

            ui.stack("story.narrative.viewport")
                .x(22.0)
                .y(22.0)
                .size(inner_w, viewport_h)
                .content(|ui| {
                    ui.scroll_y("story.narrative.content")
                        .size(inner_w, viewport_h)
                        .content_height(content_h.max(viewport_h))
                        .gap(14.0)
                        .theme(app_theme.tokens)
                        .scrollbar_gap(10.0)
                        .offset_signal(story_scroll_signal(state))
                        .content(|ui| {
                            ui.text("story.narrative.title")
                                .size(inner_w, 36.0)
                                .text(title)
                                .font_size(28.0)
                                .line_height(34.0)
                                .color(app_theme.text)
                                .build();

                            if let Some(summary) = dialogue_summary.as_deref() {
                                ui.text("story.narrative.dialogue.thread")
                                    .size(inner_w, 24.0)
                                    .text(summary)
                                    .font_size(12.0)
                                    .line_height(16.0)
                                    .color(app_theme.accent_warm)
                                    .build();
                            }

                            components::body_text(
                                ui,
                                "story.narrative.body",
                                body,
                                inner_w,
                                body_h,
                                app_theme.text_soft,
                                18.0,
                            );

                            ui.row("story.tags")
                                .size(inner_w, 30.0)
                                .gap(8.0)
                                .content(|ui| {
                                    for (index, tag) in session
                                        .latest()
                                        .map(|event| event.tags.as_slice())
                                        .unwrap_or(&[])
                                        .iter()
                                        .take(4)
                                        .enumerate()
                                    {
                                        components::badge(
                                            ui,
                                            format!("story.tag.{index}"),
                                            126.0,
                                            tag,
                                            app_theme.accent_warm,
                                            app_theme,
                                        );
                                    }
                                });
                        });
                });
        });
}

fn story_scroll_signal(state: &State<GameSession>) -> Signal<GameSession, f32> {
    state.signal(
        "fog.story-scroll",
        |session| session.story_scroll,
        |session, value| session.story_scroll = value.max(0.0),
    )
}

fn wrapped_height(text: &str, width: f32, font_size: f32, min_height: f32) -> f32 {
    let line_height = font_size + 8.0;
    let chars_per_line = (width / (font_size * 0.92)).floor().max(10.0);
    let lines = text
        .split('\n')
        .map(|line| {
            (line.chars().count() as f32 / chars_per_line)
                .ceil()
                .max(1.0)
        })
        .sum::<f32>();
    (lines * line_height).max(min_height)
}
