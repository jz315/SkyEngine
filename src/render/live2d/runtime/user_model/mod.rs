mod build;
mod control;
mod interaction;
mod update_api;

use crate::math::{LogicalPoint, LogicalSize};
use crate::render::component::{Live2DLookTarget, Live2DModelPoint};
use crate::render::live2d::asset::{Live2DHitArea, Live2DLoadError, Live2DModelResource};
use crate::render::live2d::model::Live2DModel;
use crate::render::live2d::runtime::{
    Live2DBreath, Live2DExpressionPlayer, Live2DEyeBlink, Live2DLipSync, Live2DLook,
    Live2DLookDebugState, Live2DMotionPlayer, Live2DPhysics, Live2DPhysicsOptions, Live2DPose,
    Live2DUpdateScheduler, MotionFinishedEvent, MotionFiredEvent, MotionHandle, MotionPriority,
    MotionStartedEvent,
};

/// Official-framework-style runtime owner for one mutable Live2D model instance.
///
/// `Live2DModelResource` holds immutable loaded assets; `Live2DUserModel`
/// owns the per-instance model state, runtime controllers, interactions, and
/// frame-to-frame update flow.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Live2DUpdateTimings {
    /// Time spent in `model.load_parameters()`, in milliseconds.
    pub load_parameters: f64,
    /// Time spent in `motion_player.update(...)`, in milliseconds.
    pub motion: f64,
    /// Time spent in `model.save_parameters()`, in milliseconds.
    pub save_parameters: f64,
    /// Time spent in the EyeBlink stage, in milliseconds.
    pub eye_blink: f64,
    /// Time spent in the Expression stage, in milliseconds.
    pub expression: f64,
    /// Time spent in the Look stage, in milliseconds.
    pub look: f64,
    /// Time spent in the Breath stage, in milliseconds.
    pub breath: f64,
    /// Time spent in the Physics stage, in milliseconds.
    pub physics: f64,
    /// Time spent in the LipSync stage, in milliseconds.
    pub lip_sync: f64,
    /// Time spent in the Pose stage, in milliseconds.
    pub pose: f64,
    /// Time spent in `model.update()`, in milliseconds.
    pub model_update: f64,
    /// End-to-end `Live2DUserModel::update_profiled()` time, in milliseconds.
    pub total: f64,
}

pub struct Live2DUserModel {
    pub(crate) model: Live2DModel,
    pub(crate) motion_player: Option<Live2DMotionPlayer>,
    pub(crate) eye_blink: Option<Live2DEyeBlink>,
    pub(crate) expression_player: Option<Live2DExpressionPlayer>,
    pub(crate) look: Option<Live2DLook>,
    pub(crate) breath: Option<Live2DBreath>,
    pub(crate) physics: Option<Live2DPhysics>,
    pub(crate) lip_sync: Option<Live2DLipSync>,
    pub(crate) pose: Option<Live2DPose>,
    pub(crate) hit_areas: Vec<Live2DHitArea>,
}

impl Live2DUserModel {
    pub fn model(&self) -> &Live2DModel {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut Live2DModel {
        &mut self.model
    }

    pub fn motion_player(&self) -> Option<&Live2DMotionPlayer> {
        self.motion_player.as_ref()
    }

    pub fn motion_player_mut(&mut self) -> Option<&mut Live2DMotionPlayer> {
        self.motion_player.as_mut()
    }

    pub fn expression_player(&self) -> Option<&Live2DExpressionPlayer> {
        self.expression_player.as_ref()
    }

    pub fn expression_player_mut(&mut self) -> Option<&mut Live2DExpressionPlayer> {
        self.expression_player.as_mut()
    }

    pub fn look_debug_state(&self) -> Option<Live2DLookDebugState> {
        self.look.as_ref().map(Live2DLook::debug_state)
    }
}
