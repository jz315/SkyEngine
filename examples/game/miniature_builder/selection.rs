use sky_engine::ecs::World;
use sky_engine::input::{Input, InputActions};
use sky_engine::math::Vec2;

use crate::actions::{
    ACTION_TOOL_NEXT, ACTION_TOOL_ROTATE_CCW, ACTION_TOOL_ROTATE_CW, ACTION_TOOL_SLOTS,
};
use crate::camera::{camera_frame, mouse_world};
use crate::geometry::cell_at_world;
use crate::hud::HudState;
use crate::model::{BLUEPRINTS, ORIENTATIONS};

#[derive(Clone, Copy, Debug)]
pub struct BuildSelection {
    pub selected: usize,
    pub orientation: usize,
}

impl Default for BuildSelection {
    fn default() -> Self {
        Self {
            selected: 0,
            orientation: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HoverState {
    pub cell: Option<(usize, usize)>,
    pub world_pos: Option<Vec2>,
}

impl Default for HoverState {
    fn default() -> Self {
        Self {
            cell: None,
            world_pos: None,
        }
    }
}

pub fn update_selection(world: &mut World) {
    let Some(actions) = world.get_resource::<InputActions>() else {
        return;
    };
    let mut next_selected = None;
    for (index, action) in ACTION_TOOL_SLOTS.iter().enumerate() {
        if actions.action_pressed(action) && index < BLUEPRINTS.len() {
            next_selected = Some(index);
        }
    }
    let cycle_next = actions.action_pressed(ACTION_TOOL_NEXT);
    let rotate_ccw = actions.action_pressed(ACTION_TOOL_ROTATE_CCW);
    let rotate_cw = actions.action_pressed(ACTION_TOOL_ROTATE_CW);

    let Some(selection) = world.get_resource_mut::<BuildSelection>() else {
        return;
    };
    let mut message = None;

    if let Some(index) = next_selected {
        if selection.selected != index {
            selection.selected = index;
            message = Some(format!("selected {}", BLUEPRINTS[index].name));
        }
    } else if cycle_next {
        selection.selected = (selection.selected + 1) % BLUEPRINTS.len();
        message = Some(format!("selected {}", BLUEPRINTS[selection.selected].name));
    }

    if rotate_ccw {
        selection.orientation =
            (selection.orientation + ORIENTATIONS.len() - 1) % ORIENTATIONS.len();
    }
    if rotate_cw {
        selection.orientation = (selection.orientation + 1) % ORIENTATIONS.len();
    }

    if let Some(message) = message {
        if let Some(hud) = world.get_resource_mut::<HudState>() {
            hud.message = message;
        }
    }
}

pub fn update_hover(world: &mut World) {
    let Some(input) = world.get_resource::<Input>().copied() else {
        return;
    };
    let Some(frame) = world
        .get_resource::<crate::app_bridge::FrameState>()
        .copied()
    else {
        return;
    };

    let hover = if !input.mouse_in_window() {
        HoverState::default()
    } else if let Some(camera) = camera_frame(world) {
        let world_pos = mouse_world(&input, frame, camera);
        HoverState {
            cell: cell_at_world(world_pos),
            world_pos: Some(world_pos),
        }
    } else {
        HoverState::default()
    };

    world.insert_resource(hover);
}
