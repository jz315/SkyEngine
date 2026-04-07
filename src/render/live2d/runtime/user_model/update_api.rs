use super::*;

impl Live2DUserModel {
    pub fn reset_to_default_parameters(&mut self) {
        let defaults: Vec<f32> = self.model.parameter_defaults().to_vec();
        let params = self.model.parameter_values_mut();
        params.copy_from_slice(&defaults);
        self.model.set_model_opacity(1.0);
        if let Some(motion_player) = self.motion_player.as_mut() {
            motion_player.reset_state();
        }
        if let Some(expression_player) = self.expression_player.as_mut() {
            expression_player.reset_state();
        }
        if let Some(eye_blink) = self.eye_blink.as_mut() {
            eye_blink.reset_state();
        }
        if let Some(look) = self.look.as_mut() {
            look.reset_state();
        }
        if let Some(breath) = self.breath.as_mut() {
            breath.reset_state();
        }
        if let Some(lip_sync) = self.lip_sync.as_mut() {
            lip_sync.reset_state();
        }
        if let Some(pose) = self.pose.as_mut() {
            pose.reset_state(&mut self.model);
        }
        if let Some(physics) = self.physics.as_mut() {
            physics.reset();
            physics.stabilize(&mut self.model);
        }
        self.model.update();
        self.model.save_parameters();
    }

    pub fn update(&mut self, dt: f32) {
        self.model.load_parameters();
        let motion_updated = self.motion_player.as_mut().is_some_and(|motion_player| {
            if motion_player.is_finished() {
                let _ = motion_player.start_idle_motion_if_finished();
                false
            } else {
                motion_player.update(&mut self.model, dt)
            }
        });
        self.model.save_parameters();
        Live2DUpdateScheduler::run(self, motion_updated, dt);
        self.model.update();
    }
}
