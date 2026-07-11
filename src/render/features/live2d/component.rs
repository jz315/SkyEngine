#[cfg(feature = "live2d")]
use std::cell::RefCell;
#[cfg(feature = "live2d")]
use std::path::PathBuf;

#[cfg(feature = "live2d")]
use crate::ecs::{EntityId, World};
#[cfg(feature = "live2d")]
use crate::math::{LogicalPoint, LogicalSize, Projection, Transform, Vec3};

#[cfg(feature = "live2d")]
/// A point in Cubism model coordinates, used for hit testing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Live2DModelPoint {
    pub x: f32,
    pub y: f32,
}

#[cfg(feature = "live2d")]
impl Live2DModelPoint {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn from_array(value: [f32; 2]) -> Self {
        Self {
            x: value[0],
            y: value[1],
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

#[cfg(feature = "live2d")]
impl From<[f32; 2]> for Live2DModelPoint {
    #[inline]
    fn from(value: [f32; 2]) -> Self {
        Self::from_array(value)
    }
}

#[cfg(feature = "live2d")]
impl From<Live2DModelPoint> for [f32; 2] {
    #[inline]
    fn from(value: Live2DModelPoint) -> Self {
        value.to_array()
    }
}

#[cfg(feature = "live2d")]
/// Normalized Cubism look target.
///
/// This is the value fed into Cubism's look/drag controller. Positive `x`
/// means the target is on the model's local right; positive `y` means local up.
/// It is derived from model-local coordinates divided by half the authored
/// model height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Live2DLookTarget {
    pub x: f32,
    pub y: f32,
}

#[cfg(feature = "live2d")]
impl Live2DLookTarget {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn neutral() -> Self {
        Self::new(0.0, 0.0)
    }

    #[inline]
    pub const fn from_array(value: [f32; 2]) -> Self {
        Self {
            x: value[0],
            y: value[1],
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }

    #[inline]
    pub fn from_model_point(point: Live2DModelPoint, model_height: f32) -> Self {
        let half_model_height = (model_height.abs() * 0.5).max(f32::EPSILON);
        Self::new(point.x / half_model_height, point.y / half_model_height)
    }

    #[inline]
    pub fn from_logical_screen(
        pointer: LogicalPoint,
        logical_view_size: LogicalSize,
        camera_transform: Transform,
        camera_projection: Projection,
        model_transform: Transform,
        model_height: f32,
    ) -> Self {
        let world =
            camera_projection.screen_to_world_logical(camera_transform, logical_view_size, pointer);
        let local = model_transform
            .to_matrix4()
            .inverse()
            .transform_point3(Vec3::new(world.x(), world.y(), 0.0));
        Self::from_model_point(Live2DModelPoint::new(local.x(), local.y()), model_height)
    }
}

#[cfg(feature = "live2d")]
impl From<[f32; 2]> for Live2DLookTarget {
    #[inline]
    fn from(value: [f32; 2]) -> Self {
        Self::from_array(value)
    }
}

#[cfg(feature = "live2d")]
impl From<Live2DLookTarget> for [f32; 2] {
    #[inline]
    fn from(value: Live2DLookTarget) -> Self {
        value.to_array()
    }
}

#[cfg(feature = "live2d")]
#[derive(Clone, Debug, PartialEq)]
pub struct Live2DModelInstance {
    pub model_path: PathBuf,
    pub visible: bool,
    pub height: Option<f32>,
}

#[cfg(feature = "live2d")]
impl Live2DModelInstance {
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: path.into(),
            visible: true,
            height: None,
        }
    }

    #[inline]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub fn with_height(mut self, height: f32) -> Self {
        self.height = Some(height.max(f32::EPSILON));
        self
    }
}

#[cfg(feature = "live2d")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Live2DAnimator {
    pub enabled: bool,
    pub speed: f32,
    pub update_when_hidden: bool,
}

#[cfg(feature = "live2d")]
impl Live2DAnimator {
    #[inline]
    pub const fn new() -> Self {
        Self {
            enabled: true,
            speed: 1.0,
            update_when_hidden: false,
        }
    }

    #[inline]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            speed: 1.0,
            update_when_hidden: false,
        }
    }

    #[inline]
    pub const fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    #[inline]
    pub const fn update_when_hidden(mut self, update_when_hidden: bool) -> Self {
        self.update_when_hidden = update_when_hidden;
        self
    }
}

#[cfg(feature = "live2d")]
impl Default for Live2DAnimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "live2d")]
#[derive(Clone, Debug, PartialEq)]
pub enum Live2DCommand {
    PlayMotion {
        entity: EntityId,
        group_name: String,
        index_in_group: usize,
    },
    PlayMotionByIndex {
        entity: EntityId,
        index: usize,
    },
    SetExpression {
        entity: EntityId,
        name: String,
    },
    SetLookTarget {
        entity: EntityId,
        target: Live2DLookTarget,
    },
    ClearLookTarget {
        entity: EntityId,
    },
    TapScreen {
        entity: EntityId,
        screen_position: LogicalPoint,
        view_size: LogicalSize,
    },
    TapModel {
        entity: EntityId,
        point: Live2DModelPoint,
    },
}

#[cfg(feature = "live2d")]
#[derive(Default)]
pub struct Live2DCommands {
    queue: RefCell<Vec<Live2DCommand>>,
}

#[cfg(feature = "live2d")]
impl Live2DCommands {
    pub fn resource(world: &mut World) -> &Self {
        if !world.contains_resource::<Self>() {
            world.insert_resource(Self::default());
        }
        world
            .get_resource::<Self>()
            .expect("Live2DCommands resource should exist")
    }

    #[inline]
    pub fn push(&self, command: Live2DCommand) {
        self.queue.borrow_mut().push(command);
    }

    #[inline]
    pub fn play_motion(
        &self,
        entity: EntityId,
        group_name: impl Into<String>,
        index_in_group: usize,
    ) {
        self.push(Live2DCommand::PlayMotion {
            entity,
            group_name: group_name.into(),
            index_in_group,
        });
    }

    #[inline]
    pub fn play_motion_by_index(&self, entity: EntityId, index: usize) {
        self.push(Live2DCommand::PlayMotionByIndex { entity, index });
    }

    #[inline]
    pub fn set_expression(&self, entity: EntityId, name: impl Into<String>) {
        self.push(Live2DCommand::SetExpression {
            entity,
            name: name.into(),
        });
    }

    #[inline]
    pub fn set_look_target(&self, entity: EntityId, target: Live2DLookTarget) {
        self.push(Live2DCommand::SetLookTarget { entity, target });
    }

    #[inline]
    pub fn clear_look_target(&self, entity: EntityId) {
        self.push(Live2DCommand::ClearLookTarget { entity });
    }

    #[inline]
    pub fn tap_screen(
        &self,
        entity: EntityId,
        screen_position: LogicalPoint,
        view_size: LogicalSize,
    ) {
        self.push(Live2DCommand::TapScreen {
            entity,
            screen_position,
            view_size,
        });
    }

    #[inline]
    pub fn tap_model(&self, entity: EntityId, point: Live2DModelPoint) {
        self.push(Live2DCommand::TapModel { entity, point });
    }

    #[inline]
    pub fn clear(&self) {
        self.queue.borrow_mut().clear();
    }

    pub(crate) fn drain(&self) -> Vec<Live2DCommand> {
        self.queue.borrow_mut().drain(..).collect()
    }
}

#[cfg(all(test, feature = "live2d"))]
mod tests {
    use super::*;

    #[test]
    fn command_resource_queues_and_drains_commands() {
        let mut world = World::new();
        let entity = world.spawn((Live2DModelInstance::new("model.model3.json"),));

        let commands = Live2DCommands::resource(&mut world);
        commands.play_motion(entity, "Idle", 0);
        commands.set_expression(entity, "smile");

        let queued = world
            .get_resource::<Live2DCommands>()
            .expect("commands resource should be inserted")
            .drain();

        assert_eq!(queued.len(), 2);
        assert!(matches!(
            &queued[0],
            Live2DCommand::PlayMotion {
                group_name,
                index_in_group: 0,
                ..
            } if group_name == "Idle"
        ));
        assert!(matches!(
            &queued[1],
            Live2DCommand::SetExpression { name, .. } if name == "smile"
        ));
    }

    #[test]
    fn model_instance_height_is_world_space_authoring_size() {
        let instance = Live2DModelInstance::new("model.model3.json").with_height(360.0);
        assert_eq!(instance.height, Some(360.0));
    }

    #[test]
    fn look_target_is_derived_from_model_height() {
        let target = Live2DLookTarget::from_model_point(Live2DModelPoint::new(90.0, -45.0), 180.0);
        assert_eq!(target, Live2DLookTarget::new(1.0, -0.5));
    }
}
