//! Lightweight Live2D runtime effects ported from SakuraEngine /
//! Cubism Framework behavior.

use crate::render::live2d::model::Live2DModel;

const TWO_PI: f32 = std::f32::consts::PI * 2.0;

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
    pub fn from_model_json(json: &serde_json::Value, model: &Live2DModel) -> Option<Self> {
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
            .filter_map(|id| model.find_parameter(id))
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
            rng_state: 0x5eed_b17c_u64,
        })
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
struct BreathParameter {
    parameter_index: usize,
    base_value: f32,
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
    pub fn from_model(model: &Live2DModel) -> Option<Self> {
        let definitions = [
            ("ParamAngleX", 0.0, 15.0, 6.5345, 0.5),
            ("ParamAngleY", 0.0, 8.0, 3.5345, 0.5),
            ("ParamAngleZ", 0.0, 10.0, 5.5345, 0.5),
            ("ParamBodyAngleX", 0.0, 4.0, 15.5345, 0.5),
            ("ParamBreath", 0.5, 0.5, 3.2345, 0.5),
        ];

        let defaults = model.parameter_defaults();
        let parameters: Vec<BreathParameter> = definitions
            .iter()
            .filter_map(|(id, offset, peak, cycle, weight)| {
                model
                    .find_parameter(id)
                    .map(|parameter_index| BreathParameter {
                        parameter_index,
                        base_value: defaults[parameter_index],
                        offset: *offset,
                        peak: *peak,
                        cycle: *cycle,
                        weight: *weight,
                    })
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
            model.set_parameter_by_index(
                parameter.parameter_index,
                parameter.base_value + signal * parameter.weight,
            );
        }
    }
}

#[cfg(test)]
mod tests {
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
}
