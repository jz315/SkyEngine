mod action_panel;
mod background;
mod commands;
mod components;
mod info_panel;
mod scene;
mod shell;
mod sidebar;
mod story;
mod title;

use sky_engine::ui::serein::{Screen, State, Ui};

use crate::model::{GameMode, GameSession};
use crate::theme;

pub fn render(ui: &mut Ui, screen: Screen, state: &State<GameSession>, session: &GameSession) {
    let app_theme = theme::station_theme();
    background::draw(ui, screen.width, screen.height, app_theme);

    match session.mode {
        GameMode::Title => title::draw(ui, screen.width, screen.height, state, app_theme),
        GameMode::Playing | GameMode::Ending => {
            shell::draw(ui, screen.width, screen.height, state, session, app_theme)
        }
    }
}
