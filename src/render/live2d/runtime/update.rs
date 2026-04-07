use crate::render::live2d::runtime::user_model::Live2DUserModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Live2DUpdateOrder {
    EyeBlink = 200,
    Expression = 300,
    Look = 400,
    Breath = 500,
    Physics = 600,
    LipSync = 700,
    Pose = 800,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Live2DUpdateStage {
    EyeBlink,
    Expression,
    Look,
    Breath,
    Physics,
    LipSync,
    Pose,
}

impl Live2DUpdateStage {
    const fn order(self) -> Live2DUpdateOrder {
        match self {
            Self::EyeBlink => Live2DUpdateOrder::EyeBlink,
            Self::Expression => Live2DUpdateOrder::Expression,
            Self::Look => Live2DUpdateOrder::Look,
            Self::Breath => Live2DUpdateOrder::Breath,
            Self::Physics => Live2DUpdateOrder::Physics,
            Self::LipSync => Live2DUpdateOrder::LipSync,
            Self::Pose => Live2DUpdateOrder::Pose,
        }
    }
}

pub(crate) struct Live2DUpdateScheduler;

impl Live2DUpdateScheduler {
    const STAGES: [Live2DUpdateStage; 7] = [
        Live2DUpdateStage::EyeBlink,
        Live2DUpdateStage::Expression,
        Live2DUpdateStage::Look,
        Live2DUpdateStage::Breath,
        Live2DUpdateStage::Physics,
        Live2DUpdateStage::LipSync,
        Live2DUpdateStage::Pose,
    ];

    pub(crate) fn run(user_model: &mut Live2DUserModel, motion_updated: bool, dt: f32) {
        debug_assert!(Self::is_sorted());

        for stage in Self::STAGES {
            match stage {
                Live2DUpdateStage::EyeBlink => {
                    if !motion_updated {
                        if let Some(eye_blink) = user_model.eye_blink.as_mut() {
                            eye_blink.update_parameters(&mut user_model.model, dt);
                        }
                    }
                }
                Live2DUpdateStage::Expression => {
                    if let Some(expression_player) = user_model.expression_player.as_mut() {
                        expression_player.update(&mut user_model.model, dt);
                    }
                }
                Live2DUpdateStage::Look => {
                    if let Some(look) = user_model.look.as_mut() {
                        look.update_parameters(&mut user_model.model, dt);
                    }
                }
                Live2DUpdateStage::Breath => {
                    if let Some(breath) = user_model.breath.as_mut() {
                        breath.update_parameters(&mut user_model.model, dt);
                    }
                }
                Live2DUpdateStage::Physics => {
                    if let Some(physics) = user_model.physics.as_mut() {
                        physics.evaluate(&mut user_model.model, dt);
                    }
                }
                Live2DUpdateStage::LipSync => {
                    if let Some(lip_sync) = user_model.lip_sync.as_ref() {
                        lip_sync.update_parameters(&mut user_model.model);
                    }
                }
                Live2DUpdateStage::Pose => {
                    if let Some(pose) = user_model.pose.as_mut() {
                        pose.update_parameters(&mut user_model.model, dt);
                    }
                }
            }
        }
    }

    fn is_sorted() -> bool {
        let stages = Self::STAGES;
        let mut index = 1usize;
        while index < stages.len() {
            if !(stages[index - 1].order() <= stages[index].order()) {
                return false;
            }
            index += 1;
        }
        true
    }
}
