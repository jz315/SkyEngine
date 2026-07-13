mod components;
mod overlays;
mod pages;
mod scroll_panel;
mod shell;

use sky_engine::ui::serein::{Screen, State, Ui};

use crate::model::AppModel;

#[derive(Debug, Clone, Copy)]
pub struct RuntimeInfo {
    pub uptime_seconds: f32,
    pub frame_count: u64,
}

pub fn render(
    ui: &mut Ui,
    screen: Screen,
    state_store: &State<AppModel>,
    model: &AppModel,
    runtime: RuntimeInfo,
) {
    shell::render_shell(ui, screen, state_store, model, runtime);
    overlays::render(ui, screen, state_store, model);
}
