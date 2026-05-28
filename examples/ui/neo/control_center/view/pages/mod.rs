mod overview;
mod settings;
mod tasks;

use sky_engine::ui::neo::{State, Ui};

use crate::model::{AppModel, Page};
use crate::theme::AppTheme;
use crate::view::RuntimeInfo;

pub fn render(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state_store: &State<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
    app_theme: AppTheme,
) {
    match model.page {
        Page::Overview => {
            overview::render(ui, width, height, state_store, model, runtime, app_theme)
        }
        Page::Tasks => tasks::render(ui, width, height, state_store, model, app_theme),
        Page::Settings => settings::render(ui, width, height, state_store, model, app_theme),
    }
}
