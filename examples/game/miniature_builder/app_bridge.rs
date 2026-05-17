use sky_engine::app::FrameContext;
use sky_engine::ecs::World;
use sky_engine::input::InputActions;

#[derive(Clone, Copy, Debug)]
pub struct FrameState {
    pub logical_surface_size: [f32; 2],
}

impl Default for FrameState {
    fn default() -> Self {
        Self {
            logical_surface_size: [1.0, 1.0],
        }
    }
}

#[derive(Default, Debug)]
pub struct AppRequests {
    pub exit: bool,
    pub skip_render: bool,
    pub title: Option<String>,
    pub screenshot: Option<String>,
}

pub fn install_app_bridge(world: &mut World) {
    world.insert_resource(FrameState::default());
    world.insert_resource(AppRequests::default());
}

pub fn sync_frame_state(ctx: &mut FrameContext<'_>) {
    ctx.world.insert_resource(FrameState {
        logical_surface_size: ctx.logical_surface_size(),
    });
}

pub fn request_exit_from_actions(world: &mut World) {
    let Some(actions) = world.get_resource::<InputActions>() else {
        return;
    };
    if actions.action_pressed(crate::actions::ACTION_APP_EXIT) {
        let requests = world.get_resource_mut::<AppRequests>().unwrap();
        requests.exit = true;
        requests.skip_render = true;
    }
}

pub fn apply_app_requests(ctx: &mut FrameContext<'_>) -> bool {
    let Some(mut requests) = ctx.world.remove_resource::<AppRequests>() else {
        return false;
    };

    if let Some(title) = requests.title.take() {
        ctx.set_title(&title);
    }
    if let Some(path) = requests.screenshot.take() {
        ctx.request_screenshot(path);
    }
    let exit = requests.exit;
    if exit {
        ctx.request_exit();
    }
    let skip_render = requests.skip_render;

    ctx.world.insert_resource(AppRequests::default());
    skip_render
}
