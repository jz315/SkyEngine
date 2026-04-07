mod build;
mod control;
mod interaction;
mod update_api;

use crate::render::live2d::asset::{Live2DHitArea, Live2DLoadError, Live2DModelResource};
use crate::render::live2d::model::Live2DModel;
use crate::render::live2d::runtime::{
    Live2DBreath, Live2DExpressionPlayer, Live2DEyeBlink, Live2DLipSync, Live2DLook,
    Live2DMotionPlayer, Live2DPhysics, Live2DPose, Live2DUpdateScheduler, MotionFinishedEvent,
    MotionFiredEvent, MotionHandle, MotionPriority, MotionStartedEvent,
};

/// Official-framework-style runtime owner for one mutable Live2D model instance.
///
/// `Live2DModelResource` holds immutable loaded assets; `Live2DUserModel`
/// owns the per-instance model state, runtime controllers, interactions, and
/// frame-to-frame update flow.
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
}
