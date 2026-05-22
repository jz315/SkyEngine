use sky_engine::ecs::{With, World};
use sky_engine::input::{Input, InputActions};
use sky_engine::math::Vec2;
use sky_engine::render::{CameraMarker, MainCamera, Projection, Transform};

use crate::actions::{ACTION_CAMERA_PAN, ACTION_CAMERA_ZOOM};
use crate::app_bridge::FrameState;
use crate::geometry::{MAX_ZOOM, MIN_ZOOM, ORTHO_HEIGHT};

#[derive(Clone, Copy)]
pub struct BuilderCamera;

#[derive(Clone, Copy)]
pub struct BuilderCameraController {
    zoom: f32,
}

#[derive(Clone, Copy)]
pub struct CameraFrame {
    pub position: Vec2,
    pub zoom: f32,
}

pub fn spawn_camera(world: &mut World) {
    world.spawn((
        BuilderCamera,
        BuilderCameraController { zoom: ORTHO_HEIGHT },
        Transform::from_xyz(0.0, 0.0, 0.0),
        CameraMarker::new(),
        Projection::orthographic(ORTHO_HEIGHT),
        MainCamera,
    ));
}

pub fn update_camera(world: &mut World) {
    let Some(actions) = world.get_resource::<InputActions>() else {
        return;
    };
    let pan = actions.axis_value(ACTION_CAMERA_PAN);
    let zoom_input = actions.action_value(ACTION_CAMERA_ZOOM);
    let dt = world.time.delta;

    let mut query = world.query_filtered::<(
        &mut Transform,
        &mut Projection,
        &mut BuilderCameraController,
    ), With<BuilderCamera>>();
    query.for_each(world, |(transform, projection, controller)| {
        let pan_speed = controller.zoom * 0.75 * dt;
        transform.position[0] += pan[0] * pan_speed;
        transform.position[1] += pan[1] * pan_speed;

        if zoom_input.abs() > f32::EPSILON {
            let factor = if zoom_input > 0.0 { 0.90 } else { 1.10 };
            controller.zoom = (controller.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            *projection = Projection::orthographic(controller.zoom);
        }
    });
}

pub fn camera_frame(world: &World) -> Option<CameraFrame> {
    let mut query =
        world.query_filtered::<(&Transform, &BuilderCameraController), With<BuilderCamera>>();
    let mut frame = None;
    query.for_each(world, |(transform, controller)| {
        if frame.is_none() {
            frame = Some(CameraFrame {
                position: Vec2::new(transform.position[0], transform.position[1]),
                zoom: controller.zoom,
            });
        }
    });
    frame
}

pub fn mouse_world(input: &Input, frame: FrameState, camera: CameraFrame) -> Vec2 {
    let mouse = input.mouse_logical_position();
    let [width, height] = frame.logical_surface_size;
    let width = width.max(1.0);
    let height = height.max(1.0);
    let world_h = camera.zoom;
    let world_w = camera.zoom * width / height;
    Vec2::new(
        camera.position.x() + (mouse.x / width - 0.5) * world_w,
        camera.position.y() + (0.5 - mouse.y / height) * world_h,
    )
}
