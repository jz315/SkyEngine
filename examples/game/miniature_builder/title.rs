use sky_engine::ecs::World;

use crate::app_bridge::AppRequests;
use crate::board::{BoardState, TARGET_SCORE};
use crate::geometry::cell_index;
use crate::hud::HudState;
use crate::model::{BLUEPRINTS, ORIENTATIONS};
use crate::selection::{BuildSelection, HoverState};

#[derive(Default)]
pub struct TitleState {
    frame: u32,
}

pub fn update_window_title(world: &mut World) {
    let Some(mut state) = world.remove_resource::<TitleState>() else {
        return;
    };
    state.frame = state.frame.wrapping_add(1);
    if state.frame % 10 == 0 {
        if let (Some(board), Some(selection), Some(hover_state), Some(hud)) = (
            world.get_resource::<BoardState>(),
            world.get_resource::<BuildSelection>(),
            world.get_resource::<HoverState>(),
            world.get_resource::<HudState>(),
        ) {
            let selected = BLUEPRINTS[selection.selected];
            let hover = hover_state
                .cell
                .map(|(row, col)| board.cells[cell_index(row, col)].zone.name())
                .unwrap_or("off board");
            let title = format!(
                "Miniature Builder | score {}/{} | coins {} | {} {} {} | [{}] {} {} | {}",
                board.score,
                TARGET_SCORE,
                board.budget,
                board.counts.farm,
                board.counts.dungeon,
                board.counts.library,
                selection.selected + 1,
                selected.name,
                ORIENTATIONS[selection.orientation],
                format_args!(
                    "{} | hover {hover} | {}",
                    selected.family.name(),
                    hud.message
                )
            );
            world.get_resource_mut::<AppRequests>().unwrap().title = Some(title);
        }
    }
    world.insert_resource(state);
}
