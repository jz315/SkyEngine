use sky_engine::ui::neo::{State, Ui};

use crate::model::GameSession;
use crate::theme::AppTheme;
use crate::view::{action_panel, info_panel, sidebar, story};

const PAD: f32 = 22.0;
const GAP: f32 = 16.0;
const LEFT_W: f32 = 270.0;
const RIGHT_W: f32 = 360.0;

pub fn draw(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &State<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let root_w = (width - PAD * 2.0).max(360.0);
    let root_h = (height - PAD * 2.0).max(360.0);
    let left_w = LEFT_W.min(root_w * 0.24);
    let right_w = RIGHT_W.min(root_w * 0.32);
    let center_w = (root_w - left_w - right_w - GAP * 2.0).max(0.0);
    let action_h = (root_h * 0.56).clamp(330.0, 404.0);

    ui.row("game.root")
        .x(PAD)
        .y(PAD)
        .size(root_w, root_h)
        .gap(GAP)
        .clip()
        .z_index(10)
        .content(|ui| {
            sidebar::draw(ui, left_w, root_h, state, session, app_theme);
            story::draw(ui, center_w, root_h, state, session, app_theme);
            ui.column("right")
                .size(right_w, root_h)
                .gap(GAP)
                .content(|ui| {
                    action_panel::draw(ui, right_w, action_h, state, session, app_theme);
                    info_panel::draw(
                        ui,
                        right_w,
                        root_h - action_h - GAP,
                        state,
                        session,
                        app_theme,
                    );
                });
        });
}
