use sky_engine::ui::neo::{State, Ui};

use crate::actions;
use crate::content;
use crate::model::{GameSession, Location};
use crate::theme::AppTheme;
use crate::view::{commands, components, scene};

pub fn draw(ui: &mut Ui, width: f32, height: f32, state: &State<GameSession>, app_theme: AppTheme) {
    let margin = if height < 700.0 { 24.0 } else { 34.0 };
    let available_h = (height - margin * 2.0).max(300.0);
    let card_w = (width - margin * 2.0).max(320.0).min(856.0);
    let card_h = available_h.min(520.0);
    let x = (width - card_w) * 0.5;
    let y = ((height - card_h) * 0.5).max(margin);
    let scene_h = (card_h * 0.40).clamp(160.0, 210.0);
    let copy_y = scene_h + 24.0;
    let copy_h = (card_h - copy_y - 30.0).max(0.0);

    ui.stack("title.stage")
        .x(x)
        .y(y)
        .size(card_w, card_h)
        .clip()
        .z_index(10)
        .content(|ui| {
            components::panel(ui, "title.panel", card_w, card_h, app_theme);
            scene::draw_station_scene(
                ui,
                "title.scene",
                card_w,
                scene_h,
                Location::WaitingHall,
                app_theme,
            );

            ui.column("title.copy")
                .x(34.0)
                .y(copy_y)
                .size(card_w - 68.0, copy_h)
                .gap(14.0)
                .clip()
                .z_index(2)
                .content(|ui| {
                    ui.text("title.kicker")
                        .size(card_w - 68.0, 18.0)
                        .text("FOG LANTERN STATION")
                        .font_size(12.0)
                        .line_height(16.0)
                        .color(app_theme.accent)
                        .build();
                    ui.text("title.name")
                        .size(card_w - 68.0, 52.0)
                        .text("雾灯站")
                        .font_size(42.0)
                        .line_height(48.0)
                        .color(app_theme.text)
                        .build();
                    components::body_text(
                        ui,
                        "title.body",
                        content::title_copy(),
                        card_w - 68.0,
                        76.0,
                        app_theme.text_soft,
                        17.0,
                    );
                    ui.row("title.actions")
                        .size(card_w - 68.0, 46.0)
                        .gap(12.0)
                        .content(|ui| {
                            let start_w = ((card_w - 68.0) * 0.34).clamp(150.0, 220.0);
                            let load_w = ((card_w - 68.0) * 0.28).clamp(128.0, 190.0);
                            let start_state = state.clone();
                            components::text_button(
                                ui,
                                "title.start",
                                start_w,
                                44.0,
                                "开始新游戏",
                                true,
                                true,
                                app_theme,
                                move || start_state.update(actions::start),
                            );

                            let load_state = state.clone();
                            components::text_button(
                                ui,
                                "title.load",
                                load_w,
                                44.0,
                                "读取存档",
                                false,
                                true,
                                app_theme,
                                move || commands::load_into_state(&load_state),
                            );
                        });
                });
        });
}
