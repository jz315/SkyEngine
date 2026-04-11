use super::*;
use std::time::Instant;

impl Live2DUserModel {
    pub fn reset_to_default_parameters(&mut self) {
        let defaults: Vec<f32> = self.model.parameter_defaults().to_vec();
        let params = self.model.parameter_values_mut();
        params.copy_from_slice(&defaults);
        self.model.set_model_opacity(1.0);
        self.model.set_model_color([1.0, 1.0, 1.0, 1.0]);
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

    pub fn update_profiled(&mut self, dt: f32) -> Live2DUpdateTimings {
        let total_start = Instant::now();
        let mut timings = Live2DUpdateTimings::default();

        let load_start = Instant::now();
        self.model.load_parameters();
        timings.load_parameters = elapsed_ms(load_start);

        if self.motion_player.is_some() {
            let motion_start = Instant::now();
            let motion_updated = self.motion_player.as_mut().is_some_and(|motion_player| {
                if motion_player.is_finished() {
                    let _ = motion_player.start_idle_motion_if_finished();
                    false
                } else {
                    motion_player.update(&mut self.model, dt)
                }
            });
            timings.motion = elapsed_ms(motion_start);

            let save_start = Instant::now();
            self.model.save_parameters();
            timings.save_parameters = elapsed_ms(save_start);

            Live2DUpdateScheduler::run_profiled(self, motion_updated, dt, &mut timings);
        } else {
            let save_start = Instant::now();
            self.model.save_parameters();
            timings.save_parameters = elapsed_ms(save_start);

            Live2DUpdateScheduler::run_profiled(self, false, dt, &mut timings);
        }

        let model_update_start = Instant::now();
        self.model.update();
        timings.model_update = elapsed_ms(model_update_start);
        timings.total = elapsed_ms(total_start);

        timings
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use serde_json::Value;

    fn sample_model_json_path() -> PathBuf {
        PathBuf::from(
            "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.model3.json",
        )
    }

    fn build_sample_user_model() -> Live2DUserModel {
        let model_json_path = sample_model_json_path();
        let base_dir = model_json_path
            .parent()
            .expect("sample model3 should have a parent directory");
        let json_text =
            std::fs::read_to_string(&model_json_path).expect("sample model3 should exist");
        let json: Value = serde_json::from_str(&json_text).expect("sample model3 should parse");
        let moc_bytes = std::fs::read(
            base_dir.join(
                json.pointer("/FileReferences/Moc")
                    .and_then(|value| value.as_str())
                    .expect("sample model3 should reference a moc file"),
            ),
        )
        .expect("sample moc3 should exist");

        let mut model = Live2DModel::from_moc3_bytes(&moc_bytes).expect("sample moc3 should load");
        let motion_player = Live2DMotionPlayer::from_model_json(&json, base_dir, &mut model)
            .expect("sample motion data should load");
        let eye_blink = Live2DEyeBlink::from_model_json(&json, &mut model);
        let expression_player =
            Live2DExpressionPlayer::from_model_json(&json, base_dir, &mut model)
                .expect("sample expression data should load");
        let look = Live2DLook::from_model(&mut model);
        let breath = Live2DBreath::from_model(&mut model);
        let physics = load_optional_text(&json, base_dir, "/FileReferences/Physics").map(|text| {
            Live2DPhysics::from_json_str(&text, &model).expect("sample physics data should load")
        });
        let pose = load_optional_text(&json, base_dir, "/FileReferences/Pose")
            .map(|text| Live2DPose::from_json_str(&text).expect("sample pose data should load"));
        let lip_sync = Live2DLipSync::from_model_json(&json, &mut model);

        Live2DUserModel {
            model,
            motion_player,
            eye_blink,
            expression_player,
            look,
            breath,
            physics,
            lip_sync,
            pose,
            hit_areas: Vec::new(),
        }
    }

    fn load_optional_text(json: &Value, base_dir: &Path, pointer: &str) -> Option<String> {
        json.pointer(pointer)
            .and_then(|value| value.as_str())
            .map(|path| std::fs::read_to_string(base_dir.join(path)).expect("sample file exists"))
    }

    fn prime_runtime(user_model: &mut Live2DUserModel) {
        if user_model.motion_player().is_some() {
            assert!(user_model.set_motion_by_index(0));
        }
        if let Some(expression_name) = user_model
            .expression_player()
            .and_then(|player| player.expression_names().next().map(str::to_string))
        {
            assert!(user_model.set_expression(&expression_name));
        }
        let _ = user_model.set_drag(0.35, -0.2);
        let _ = user_model.set_lip_sync(0.4);
    }

    fn assert_user_models_match(lhs: &Live2DUserModel, rhs: &Live2DUserModel) {
        assert_eq!(lhs.model.parameter_count(), rhs.model.parameter_count());
        for index in 0..lhs.model.parameter_count() {
            assert!(
                (lhs.model.parameter_value(index) - rhs.model.parameter_value(index)).abs()
                    < 0.0001,
                "parameter {} diverged",
                lhs.model.parameter_id(index)
            );
        }

        assert_eq!(lhs.model.part_count(), rhs.model.part_count());
        for index in 0..lhs.model.part_count() {
            assert!(
                (lhs.model.part_opacity(index) - rhs.model.part_opacity(index)).abs() < 0.0001,
                "part opacity {index} diverged"
            );
        }

        assert_eq!(lhs.model.drawable_count(), rhs.model.drawable_count());
        assert_eq!(
            lhs.model.sorted_drawable_indices(),
            rhs.model.sorted_drawable_indices()
        );
        for index in 0..lhs.model.drawable_count() {
            assert_eq!(
                lhs.model.drawable_is_visible(index),
                rhs.model.drawable_is_visible(index),
                "drawable visibility {index} diverged"
            );
            assert!(
                (lhs.model.drawable_opacity(index) - rhs.model.drawable_opacity(index)).abs()
                    < 0.0001,
                "drawable opacity {index} diverged"
            );
        }

        assert!(
            (lhs.model.model_opacity() - rhs.model.model_opacity()).abs() < 0.0001,
            "model opacity diverged"
        );
        assert_eq!(
            lhs.motion_player()
                .map(Live2DMotionPlayer::current_priority),
            rhs.motion_player()
                .map(Live2DMotionPlayer::current_priority),
            "motion priority diverged"
        );
    }

    #[test]
    fn update_profiled_matches_update_semantics() {
        let mut baseline = build_sample_user_model();
        let mut profiled = build_sample_user_model();

        prime_runtime(&mut baseline);
        prime_runtime(&mut profiled);

        for dt in [1.0 / 60.0, 1.0 / 60.0, 1.0 / 30.0] {
            baseline.update(dt);
            let timings = profiled.update_profiled(dt);

            assert!(timings.total.is_finite());
            assert!(timings.physics.is_finite());
            assert!(timings.model_update.is_finite());

            assert_user_models_match(&baseline, &profiled);
            assert_eq!(
                baseline.take_started_motions(),
                profiled.take_started_motions()
            );
            assert_eq!(
                baseline.take_finished_motions(),
                profiled.take_finished_motions()
            );
            assert_eq!(baseline.take_motion_events(), profiled.take_motion_events());
            assert_eq!(
                baseline.take_started_motion_sounds(),
                profiled.take_started_motion_sounds()
            );
        }
    }
}
