//! Lightweight Live2D runtime effects ported from SakuraEngine /
//! Cubism Framework behavior.

use crate::render::live2d::model::Live2DModel;

const TWO_PI: f32 = std::f32::consts::PI * 2.0;
const EYE_BLINK_RNG_SEED: u64 = 0x5eed_b17c_u64;
const LOOK_FRAME_RATE: f32 = 30.0;
const LOOK_EPSILON: f32 = 0.01;
const LOOK_FACE_PARAM_MAX_V: f32 = 40.0 / 10.0;
const LOOK_TIME_TO_MAX_SPEED: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EyeBlinkState {
    First,
    Interval,
    Closing,
    Closed,
    Opening,
}

/// Minimal eye-blink runtime copied from CubismEyeBlink behavior.
#[derive(Debug, Clone)]
pub struct Live2DEyeBlink {
    parameter_indices: Vec<usize>,
    state: EyeBlinkState,
    next_blinking_time: f32,
    state_start_time_seconds: f32,
    blinking_interval_seconds: f32,
    closing_seconds: f32,
    closed_seconds: f32,
    opening_seconds: f32,
    user_time_seconds: f32,
    rng_state: u64,
}

impl Live2DEyeBlink {
    pub fn from_model_json(json: &serde_json::Value, model: &mut Live2DModel) -> Option<Self> {
        let indices: Vec<usize> = json
            .get("Groups")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|group| {
                group.get("Target").and_then(|value| value.as_str()) == Some("Parameter")
                    && group.get("Name").and_then(|value| value.as_str()) == Some("EyeBlink")
            })
            .flat_map(|group| {
                group
                    .get("Ids")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str())
            })
            .map(|id| model.ensure_parameter_slot(id))
            .collect();

        if indices.is_empty() {
            return None;
        }

        Some(Self {
            parameter_indices: indices,
            state: EyeBlinkState::First,
            next_blinking_time: 0.0,
            state_start_time_seconds: 0.0,
            blinking_interval_seconds: 4.0,
            closing_seconds: 0.1,
            closed_seconds: 0.05,
            opening_seconds: 0.15,
            user_time_seconds: 0.0,
            rng_state: EYE_BLINK_RNG_SEED,
        })
    }

    pub(crate) fn reset_state(&mut self) {
        self.state = EyeBlinkState::First;
        self.next_blinking_time = 0.0;
        self.state_start_time_seconds = 0.0;
        self.user_time_seconds = 0.0;
        self.rng_state = EYE_BLINK_RNG_SEED;
    }

    pub fn update_parameters(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        self.user_time_seconds += delta_time_seconds.max(0.0);

        let parameter_value = match self.state {
            EyeBlinkState::Closing => {
                let mut t = (self.user_time_seconds - self.state_start_time_seconds)
                    / self.closing_seconds.max(f32::EPSILON);
                if t >= 1.0 {
                    t = 1.0;
                    self.state = EyeBlinkState::Closed;
                    self.state_start_time_seconds = self.user_time_seconds;
                }
                1.0 - t
            }
            EyeBlinkState::Closed => {
                let t = (self.user_time_seconds - self.state_start_time_seconds)
                    / self.closed_seconds.max(f32::EPSILON);
                if t >= 1.0 {
                    self.state = EyeBlinkState::Opening;
                    self.state_start_time_seconds = self.user_time_seconds;
                }
                0.0
            }
            EyeBlinkState::Opening => {
                let mut t = (self.user_time_seconds - self.state_start_time_seconds)
                    / self.opening_seconds.max(f32::EPSILON);
                if t >= 1.0 {
                    t = 1.0;
                    self.state = EyeBlinkState::Interval;
                    self.next_blinking_time = self.determine_next_blinking_timing();
                }
                t
            }
            EyeBlinkState::Interval => {
                if self.next_blinking_time < self.user_time_seconds {
                    self.state = EyeBlinkState::Closing;
                    self.state_start_time_seconds = self.user_time_seconds;
                }
                1.0
            }
            EyeBlinkState::First => {
                self.state = EyeBlinkState::Interval;
                self.next_blinking_time = self.determine_next_blinking_timing();
                1.0
            }
        };

        for &parameter_index in &self.parameter_indices {
            model.set_parameter_by_index(parameter_index, parameter_value);
        }
    }

    fn determine_next_blinking_timing(&mut self) -> f32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        let random = ((self.rng_state >> 32) as u32) as f32 / (u32::MAX as f32);
        self.user_time_seconds + (random * (2.0 * self.blinking_interval_seconds - 1.0))
    }
}

#[derive(Debug, Clone, Copy)]
struct LookParameter {
    parameter_index: usize,
    factor_x: f32,
    factor_y: f32,
    factor_xy: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct LookTargetPoint {
    face_target_x: f32,
    face_target_y: f32,
    face_x: f32,
    face_y: f32,
    face_vx: f32,
    face_vy: f32,
    last_time_seconds: f32,
    user_time_seconds: f32,
}

impl LookTargetPoint {
    fn set(&mut self, x: f32, y: f32) {
        self.face_target_x = x;
        self.face_target_y = y;
    }

    fn update(&mut self, delta_time_seconds: f32) {
        self.user_time_seconds += delta_time_seconds.max(0.0);

        let max_v = LOOK_FACE_PARAM_MAX_V / LOOK_FRAME_RATE;
        if self.last_time_seconds == 0.0 {
            self.last_time_seconds = self.user_time_seconds;
            return;
        }

        let delta_time_weight = (self.user_time_seconds - self.last_time_seconds) * LOOK_FRAME_RATE;
        self.last_time_seconds = self.user_time_seconds;

        let frame_to_max_speed = LOOK_TIME_TO_MAX_SPEED * LOOK_FRAME_RATE;
        let max_a = delta_time_weight * max_v / frame_to_max_speed;

        let dx = self.face_target_x - self.face_x;
        let dy = self.face_target_y - self.face_y;
        if dx.abs() <= LOOK_EPSILON && dy.abs() <= LOOK_EPSILON {
            return;
        }

        let distance = (dx * dx + dy * dy).sqrt();
        if distance <= f32::EPSILON {
            return;
        }

        let vx = max_v * dx / distance;
        let vy = max_v * dy / distance;

        let mut ax = vx - self.face_vx;
        let mut ay = vy - self.face_vy;
        let acceleration = (ax * ax + ay * ay).sqrt();
        if acceleration > max_a && acceleration > f32::EPSILON {
            let scale = max_a / acceleration;
            ax *= scale;
            ay *= scale;
        }

        self.face_vx += ax;
        self.face_vy += ay;

        let max_v_near_target = 0.5 * (((max_a * max_a) + 8.0 * max_a * distance).sqrt() - max_a);
        let cur_v = (self.face_vx * self.face_vx + self.face_vy * self.face_vy).sqrt();
        if cur_v > max_v_near_target && cur_v > f32::EPSILON {
            let scale = max_v_near_target / cur_v;
            self.face_vx *= scale;
            self.face_vy *= scale;
        }

        self.face_x += self.face_vx;
        self.face_y += self.face_vy;
    }

    fn x(self) -> f32 {
        self.face_x
    }

    fn y(self) -> f32 {
        self.face_y
    }
}

/// Lightweight drag/look runtime matching CubismTargetPoint + Full Demo mapping.
#[derive(Debug, Clone, Default)]
pub struct Live2DLook {
    target_point: LookTargetPoint,
    parameters: Vec<LookParameter>,
}

impl Live2DLook {
    pub fn from_model(model: &mut Live2DModel) -> Option<Self> {
        let definitions = [
            ("ParamAngleX", 30.0, 0.0, 0.0),
            ("ParamAngleY", 0.0, 30.0, 0.0),
            ("ParamAngleZ", 0.0, 0.0, -30.0),
            ("ParamBodyAngleX", 10.0, 0.0, 0.0),
            ("ParamEyeBallX", 1.0, 0.0, 0.0),
            ("ParamEyeBallY", 0.0, 1.0, 0.0),
        ];

        let parameters: Vec<LookParameter> = definitions
            .iter()
            .map(|(id, factor_x, factor_y, factor_xy)| LookParameter {
                parameter_index: model.ensure_parameter_slot(id),
                factor_x: *factor_x,
                factor_y: *factor_y,
                factor_xy: *factor_xy,
            })
            .collect();

        if parameters.is_empty() {
            return None;
        }

        Some(Self {
            target_point: LookTargetPoint::default(),
            parameters,
        })
    }

    pub fn set_target(&mut self, x: f32, y: f32) {
        self.target_point.set(x, y);
    }

    pub(crate) fn reset_state(&mut self) {
        self.target_point = LookTargetPoint::default();
    }

    pub fn update_parameters(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        self.target_point.update(delta_time_seconds);

        let drag_x = self.target_point.x();
        let drag_y = self.target_point.y();
        let drag_xy = drag_x * drag_y;

        for parameter in &self.parameters {
            model.add_parameter_by_index(
                parameter.parameter_index,
                parameter.factor_x * drag_x
                    + parameter.factor_y * drag_y
                    + parameter.factor_xy * drag_xy,
            );
        }
    }
}

/// Lightweight realtime lip-sync input.
#[derive(Debug, Clone, Default)]
pub struct Live2DLipSync {
    parameter_indices: Vec<usize>,
    value: f32,
    weight: f32,
}

impl Live2DLipSync {
    pub fn from_model_json(json: &serde_json::Value, model: &mut Live2DModel) -> Option<Self> {
        let parameter_indices = extract_group_parameter_indices(json, "LipSync", model);
        if parameter_indices.is_empty() {
            return None;
        }

        Some(Self {
            parameter_indices,
            value: 0.0,
            weight: 0.8,
        })
    }

    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }

    pub(crate) fn reset_state(&mut self) {
        self.value = 0.0;
    }

    pub fn update_parameters(&self, model: &mut Live2DModel) {
        for &parameter_index in &self.parameter_indices {
            model.add_parameter_weighted_by_index(parameter_index, self.value, self.weight);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BreathParameter {
    parameter_index: usize,
    offset: f32,
    peak: f32,
    cycle: f32,
    weight: f32,
}

/// Minimal breath runtime copied from SakuraEngine's default parameter setup.
#[derive(Debug, Clone, Default)]
pub struct Live2DBreath {
    current_time: f32,
    parameters: Vec<BreathParameter>,
}

impl Live2DBreath {
    pub fn from_model(model: &mut Live2DModel) -> Option<Self> {
        let definitions = [
            ("ParamAngleX", 0.0, 15.0, 6.5345, 0.5),
            ("ParamAngleY", 0.0, 8.0, 3.5345, 0.5),
            ("ParamAngleZ", 0.0, 10.0, 5.5345, 0.5),
            ("ParamBodyAngleX", 0.0, 4.0, 15.5345, 0.5),
            ("ParamBreath", 0.5, 0.5, 3.2345, 0.5),
        ];

        let parameters: Vec<BreathParameter> = definitions
            .iter()
            .map(|(id, offset, peak, cycle, weight)| BreathParameter {
                parameter_index: model.ensure_parameter_slot(id),
                offset: *offset,
                peak: *peak,
                cycle: *cycle,
                weight: *weight,
            })
            .collect();

        if parameters.is_empty() {
            return None;
        }

        Some(Self {
            current_time: 0.0,
            parameters,
        })
    }

    pub fn update_parameters(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        self.current_time += delta_time_seconds.max(0.0);
        let t = self.current_time * TWO_PI;

        for parameter in &self.parameters {
            let signal = parameter.offset + parameter.peak * (t / parameter.cycle).sin();
            model.add_parameter_weighted_by_index(
                parameter.parameter_index,
                signal,
                parameter.weight,
            );
        }
    }

    pub(crate) fn reset_state(&mut self) {
        self.current_time = 0.0;
    }
}

fn extract_group_parameter_indices(
    json: &serde_json::Value,
    group_name: &str,
    model: &mut Live2DModel,
) -> Vec<usize> {
    json.get("Groups")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|group| {
            group.get("Target").and_then(|value| value.as_str()) == Some("Parameter")
                && group.get("Name").and_then(|value| value.as_str()) == Some(group_name)
        })
        .flat_map(|group| {
            group
                .get("Ids")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str())
        })
        .map(|id| model.ensure_parameter_slot(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_blink_group_parser_ignores_non_eye_blink_entries() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
                "Groups": [
                    { "Target": "Parameter", "Name": "LipSync", "Ids": ["ParamMouthOpenY"] },
                    { "Target": "Parameter", "Name": "EyeBlink", "Ids": ["ParamEyeLOpen", "ParamEyeROpen"] }
                ]
            }"#,
        )
        .unwrap();

        let ids: Vec<&str> = json
            .get("Groups")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|group| {
                group.get("Target").and_then(|value| value.as_str()) == Some("Parameter")
                    && group.get("Name").and_then(|value| value.as_str()) == Some("EyeBlink")
            })
            .flat_map(|group| {
                group
                    .get("Ids")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str())
            })
            .collect();

        assert_eq!(ids, vec!["ParamEyeLOpen", "ParamEyeROpen"]);
    }

    #[test]
    fn lip_sync_parser_ignores_non_parameter_groups() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
                "Groups": [
                    { "Target": "PartOpacity", "Name": "LipSync", "Ids": ["PartA"] },
                    { "Target": "Parameter", "Name": "LipSync", "Ids": ["ParamMouthOpenY"] }
                ]
            }"#,
        )
        .unwrap();

        let ids: Vec<&str> = json
            .get("Groups")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|group| {
                group.get("Target").and_then(|value| value.as_str()) == Some("Parameter")
                    && group.get("Name").and_then(|value| value.as_str()) == Some("LipSync")
            })
            .flat_map(|group| {
                group
                    .get("Ids")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str())
            })
            .collect();

        assert_eq!(ids, vec!["ParamMouthOpenY"]);
    }

    #[test]
    fn look_target_point_waits_one_tick_before_moving() {
        let mut target = LookTargetPoint::default();
        target.set(1.0, 0.0);

        target.update(1.0 / 60.0);
        assert!((target.x() - 0.0).abs() < 0.0001);
        assert!((target.y() - 0.0).abs() < 0.0001);

        target.update(1.0 / 60.0);
        assert!(target.x() > 0.0);
    }

    #[test]
    fn eye_blink_reset_restores_initial_state() {
        let mut eye_blink = Live2DEyeBlink {
            parameter_indices: vec![1, 2],
            state: EyeBlinkState::Opening,
            next_blinking_time: 3.0,
            state_start_time_seconds: 2.0,
            blinking_interval_seconds: 4.0,
            closing_seconds: 0.1,
            closed_seconds: 0.05,
            opening_seconds: 0.15,
            user_time_seconds: 5.0,
            rng_state: 123,
        };

        eye_blink.reset_state();

        assert_eq!(eye_blink.state, EyeBlinkState::First);
        assert!(eye_blink.next_blinking_time.abs() < 0.0001);
        assert!(eye_blink.state_start_time_seconds.abs() < 0.0001);
        assert!(eye_blink.user_time_seconds.abs() < 0.0001);
        assert_eq!(eye_blink.rng_state, EYE_BLINK_RNG_SEED);
    }

    #[test]
    fn effect_resets_clear_transient_runtime_state() {
        let mut look = Live2DLook {
            target_point: LookTargetPoint {
                face_target_x: 1.0,
                face_target_y: 2.0,
                face_x: 3.0,
                face_y: 4.0,
                face_vx: 5.0,
                face_vy: 6.0,
                last_time_seconds: 7.0,
                user_time_seconds: 8.0,
            },
            parameters: Vec::new(),
        };
        let mut lip_sync = Live2DLipSync {
            parameter_indices: vec![1],
            value: 0.9,
            weight: 0.8,
        };
        let mut breath = Live2DBreath {
            current_time: 12.0,
            parameters: Vec::new(),
        };

        look.reset_state();
        lip_sync.reset_state();
        breath.reset_state();

        assert!(look.target_point.face_x.abs() < 0.0001);
        assert!(look.target_point.user_time_seconds.abs() < 0.0001);
        assert!(lip_sync.value.abs() < 0.0001);
        assert!(breath.current_time.abs() < 0.0001);
    }
}
