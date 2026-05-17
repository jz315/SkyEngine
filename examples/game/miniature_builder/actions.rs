use sky_engine::ecs::World;
use sky_engine::input::action::{BindingKind, InputBinding};
use sky_engine::input::{
    ActionKind, ActionMap, InputActions, InputSource, KeyCode, MouseAxisKind, MouseButton,
};

pub const ACTION_CAMERA_PAN: &str = "camera.pan";
pub const ACTION_CAMERA_ZOOM: &str = "camera.zoom";
pub const ACTION_TOOL_NEXT: &str = "tool.next";
pub const ACTION_TOOL_ROTATE_CCW: &str = "tool.rotate_ccw";
pub const ACTION_TOOL_ROTATE_CW: &str = "tool.rotate_cw";
pub const ACTION_BOARD_PLACE: &str = "board.place";
pub const ACTION_BOARD_REMOVE: &str = "board.remove";
pub const ACTION_BOARD_UNDO: &str = "board.undo";
pub const ACTION_BOARD_RESET: &str = "board.reset";
pub const ACTION_APP_EXIT: &str = "app.exit";
pub const ACTION_TOOL_SLOTS: [&str; 9] = [
    "tool.slot_1",
    "tool.slot_2",
    "tool.slot_3",
    "tool.slot_4",
    "tool.slot_5",
    "tool.slot_6",
    "tool.slot_7",
    "tool.slot_8",
    "tool.slot_9",
];

pub fn install_actions(world: &mut World) {
    let mut builder = ActionMap::new("builder");
    builder.add_raw(
        ACTION_CAMERA_PAN,
        ActionKind::Axis2D,
        vec![
            BindingKind::Axis2D {
                up: InputBinding::new(InputSource::Key(KeyCode::KeyW)),
                down: InputBinding::with_scale(InputSource::Key(KeyCode::KeyS), -1.0),
                left: InputBinding::with_scale(InputSource::Key(KeyCode::KeyA), -1.0),
                right: InputBinding::new(InputSource::Key(KeyCode::KeyD)),
            },
            BindingKind::Axis2D {
                up: InputBinding::new(InputSource::Key(KeyCode::ArrowUp)),
                down: InputBinding::with_scale(InputSource::Key(KeyCode::ArrowDown), -1.0),
                left: InputBinding::with_scale(InputSource::Key(KeyCode::ArrowLeft), -1.0),
                right: InputBinding::new(InputSource::Key(KeyCode::ArrowRight)),
            },
        ],
    );
    builder.add_raw(
        ACTION_CAMERA_ZOOM,
        ActionKind::Axis1D,
        vec![BindingKind::Simple(InputBinding::with_scale(
            InputSource::MouseAxis(MouseAxisKind::ScrollY),
            1.0,
        ))],
    );
    builder.add_button(
        ACTION_TOOL_NEXT,
        [
            InputSource::Key(KeyCode::Tab),
            InputSource::Key(KeyCode::Space),
        ],
    );
    builder.add_button(ACTION_TOOL_ROTATE_CCW, [InputSource::Key(KeyCode::KeyQ)]);
    builder.add_button(ACTION_TOOL_ROTATE_CW, [InputSource::Key(KeyCode::KeyE)]);
    builder.add_button(ACTION_BOARD_PLACE, [InputSource::Mouse(MouseButton::Left)]);
    builder.add_button(
        ACTION_BOARD_REMOVE,
        [InputSource::Mouse(MouseButton::Right)],
    );
    builder.add_button(ACTION_BOARD_UNDO, [InputSource::Key(KeyCode::KeyZ)]);
    builder.add_button(ACTION_BOARD_RESET, [InputSource::Key(KeyCode::KeyR)]);
    builder.add_button(ACTION_APP_EXIT, [InputSource::Key(KeyCode::Escape)]);

    for (action, key) in ACTION_TOOL_SLOTS.iter().zip(
        [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ]
        .iter()
        .copied(),
    ) {
        builder.add_button(*action, [InputSource::Key(key)]);
    }

    let mut actions = InputActions::new();
    actions.add_map(builder);
    world.insert_resource(actions);
}
