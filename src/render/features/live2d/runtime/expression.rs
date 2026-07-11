//! Minimal `exp3.json` expression runtime ported from Cubism / SakuraEngine.
//!
//! Scope for now:
//! - load expressions from `FileReferences.Expressions`
//! - queue and fade expressions with CubismExpressionMotionManager-style layering
//! - apply Add / Multiply / Overwrite blends through per-parameter aggregation

use std::path::Path;

use crate::render::live2d::model::Live2DModel;

const DEFAULT_FADE_TIME_SECONDS: f32 = 1.0;
const DEFAULT_ADDITIVE_VALUE: f32 = 0.0;
const DEFAULT_MULTIPLY_VALUE: f32 = 1.0;
const EXPRESSION_RNG_SEED: u64 = 0xE17E_55A1_CAFE_BABE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpressionBlend {
    Add,
    Multiply,
    Overwrite,
}

impl ExpressionBlend {
    fn from_json_value(value: Option<&str>) -> Self {
        match value {
            None | Some("Add") => Self::Add,
            Some("Multiply") => Self::Multiply,
            Some("Overwrite") => Self::Overwrite,
            Some(_) => Self::Add,
        }
    }
}

#[derive(Debug, Clone)]
struct ExpressionParameter {
    parameter_index: Option<usize>,
    value: f32,
    blend: ExpressionBlend,
}

#[derive(Debug, Clone)]
struct Live2DExpression {
    name: String,
    fade_in_seconds: f32,
    fade_out_seconds: f32,
    parameters: Vec<ExpressionParameter>,
}

impl Live2DExpression {
    fn from_json_str(name: String, text: &str, model: &mut Live2DModel) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("expression JSON parse error: {e}"))?;

        let fade_in_seconds = json
            .get("FadeInTime")
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
            .unwrap_or(DEFAULT_FADE_TIME_SECONDS);
        let fade_out_seconds = json
            .get("FadeOutTime")
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
            .unwrap_or(DEFAULT_FADE_TIME_SECONDS);

        let parameters = json
            .get("Parameters")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "expression JSON missing Parameters".to_string())?;

        let mut parsed_parameters = Vec::with_capacity(parameters.len());
        for (parameter_index, parameter) in parameters.iter().enumerate() {
            let id = parameter
                .get("Id")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("expression parameter {parameter_index} missing Id"))?;
            let value = parameter
                .get("Value")
                .and_then(|value| value.as_f64())
                .map(|value| value as f32)
                .ok_or_else(|| format!("expression parameter {parameter_index} missing Value"))?;
            let blend = ExpressionBlend::from_json_value(
                parameter.get("Blend").and_then(|value| value.as_str()),
            );

            parsed_parameters.push(ExpressionParameter {
                parameter_index: Some(model.ensure_parameter_slot(id)),
                value,
                blend,
            });
        }

        Ok(Self {
            name,
            fade_in_seconds,
            fade_out_seconds,
            parameters: parsed_parameters,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct QueuedExpression {
    expression_index: usize,
    started: bool,
    triggered_fade_out: bool,
    fade_in_start_time_seconds: f32,
    end_time_seconds: f32,
    fade_out_seconds: f32,
    state_weight: f32,
}

impl QueuedExpression {
    fn new(expression_index: usize, fade_out_seconds: f32) -> Self {
        Self {
            expression_index,
            started: false,
            triggered_fade_out: false,
            fade_in_start_time_seconds: 0.0,
            end_time_seconds: -1.0,
            fade_out_seconds,
            state_weight: 0.0,
        }
    }

    fn ensure_started(&mut self, user_time_seconds: f32) {
        if self.started {
            return;
        }
        self.started = true;
        self.fade_in_start_time_seconds = user_time_seconds;
    }

    fn queue_fade_out(&mut self) {
        self.triggered_fade_out = true;
    }

    fn apply_triggered_fade_out(&mut self, user_time_seconds: f32) {
        if !self.triggered_fade_out {
            return;
        }

        let new_end_time_seconds = user_time_seconds + self.fade_out_seconds;
        if self.end_time_seconds < 0.0 || new_end_time_seconds < self.end_time_seconds {
            self.end_time_seconds = new_end_time_seconds;
        }
    }

    fn fade_in_weight(self, expression: &Live2DExpression, user_time_seconds: f32) -> f32 {
        expression_fade_in_weight(
            expression,
            (user_time_seconds - self.fade_in_start_time_seconds).max(0.0),
        )
    }

    fn fade_weight(self, expression: &Live2DExpression, user_time_seconds: f32) -> f32 {
        let fade_in = self.fade_in_weight(expression, user_time_seconds);
        let fade_out = if self.fade_out_seconds <= f32::EPSILON || self.end_time_seconds < 0.0 {
            1.0
        } else {
            easing_sine((self.end_time_seconds - user_time_seconds) / self.fade_out_seconds)
        };
        (fade_in * fade_out).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct ExpressionParameterValue {
    parameter_index: usize,
    additive_value: f32,
    multiply_value: f32,
    overwrite_value: f32,
}

/// Expression player that follows CubismExpressionMotionManager-style blending.
#[derive(Debug, Clone, Default)]
pub struct Live2DExpressionPlayer {
    expressions: Vec<Live2DExpression>,
    parameter_indices: Vec<usize>,
    queue: Vec<QueuedExpression>,
    rng_state: u64,
    user_time_seconds: f32,
}

impl Live2DExpressionPlayer {
    pub fn from_model_json(
        json: &serde_json::Value,
        base_dir: &Path,
        model: &mut Live2DModel,
    ) -> Result<Option<Self>, String> {
        let Some(expression_entries) = expression_entries_from_model_json(json) else {
            return Ok(None);
        };

        let mut expressions = Vec::with_capacity(expression_entries.len());
        let mut parameter_indices = Vec::new();
        for (index, entry) in expression_entries.iter().enumerate() {
            let name = entry
                .get("Name")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("expression entry {index} missing Name"))?;
            let file = entry
                .get("File")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("expression entry {index} missing File"))?;
            let path = base_dir.join(file);
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read expression {}: {e}", path.display()))?;

            let expression = Live2DExpression::from_json_str(name.to_string(), &text, model)?;
            parameter_indices.extend(
                expression
                    .parameters
                    .iter()
                    .filter_map(|parameter| parameter.parameter_index),
            );
            expressions.push(expression);
        }

        if expressions.is_empty() {
            return Ok(None);
        }

        parameter_indices.sort_unstable();
        parameter_indices.dedup();

        Ok(Some(Self {
            expressions,
            parameter_indices,
            queue: Vec::new(),
            rng_state: EXPRESSION_RNG_SEED,
            user_time_seconds: 0.0,
        }))
    }

    pub(crate) fn fresh_clone(&self) -> Self {
        Self {
            expressions: self.expressions.clone(),
            parameter_indices: self.parameter_indices.clone(),
            queue: Vec::new(),
            rng_state: EXPRESSION_RNG_SEED,
            user_time_seconds: 0.0,
        }
    }

    pub(crate) fn reset_state(&mut self) {
        self.queue.clear();
        self.rng_state = EXPRESSION_RNG_SEED;
        self.user_time_seconds = 0.0;
    }

    pub fn update(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        if self.queue.is_empty() {
            return;
        }

        self.user_time_seconds += delta_time_seconds.max(0.0);

        let mut parameter_values: Vec<ExpressionParameterValue> = self
            .parameter_indices
            .iter()
            .map(|&parameter_index| ExpressionParameterValue {
                parameter_index,
                additive_value: DEFAULT_ADDITIVE_VALUE,
                multiply_value: DEFAULT_MULTIPLY_VALUE,
                overwrite_value: model.parameter_value(parameter_index),
            })
            .collect();
        let current_values: Vec<f32> = parameter_values
            .iter()
            .map(|parameter| parameter.overwrite_value)
            .collect();

        let mut expression_weight = 0.0;
        for (queue_index, entry) in self.queue.iter_mut().enumerate() {
            let expression = &self.expressions[entry.expression_index];
            entry.ensure_started(self.user_time_seconds);
            let fade_weight = entry.fade_weight(expression, self.user_time_seconds);
            entry.state_weight = fade_weight;

            accumulate_expression_parameter_values(
                &mut parameter_values,
                &current_values,
                expression,
                queue_index,
                fade_weight,
            );
            expression_weight += entry.fade_in_weight(expression, self.user_time_seconds);
            entry.apply_triggered_fade_out(self.user_time_seconds);
        }

        prune_expression_queue(&mut self.queue);

        let expression_weight = expression_weight.clamp(0.0, 1.0);
        for parameter in parameter_values {
            model.set_parameter_weighted_by_index(
                parameter.parameter_index,
                (parameter.overwrite_value + parameter.additive_value) * parameter.multiply_value,
                expression_weight,
            );
        }
    }

    pub fn set_expression(&mut self, name: &str) -> bool {
        if let Some(index) = self
            .expressions
            .iter()
            .position(|expression| expression.name == name)
        {
            for queued in &mut self.queue {
                queued.queue_fade_out();
            }
            self.queue.push(QueuedExpression::new(
                index,
                self.expressions[index].fade_out_seconds,
            ));
            true
        } else {
            false
        }
    }

    pub fn set_random_expression(&mut self) -> bool {
        if self.expressions.is_empty() {
            return false;
        }
        let index = self.next_random_u32() as usize % self.expressions.len();
        let name = self.expressions[index].name.clone();
        self.set_expression(&name)
    }

    pub fn expression_count(&self) -> usize {
        self.expressions.len()
    }

    pub fn expression_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.expressions
            .iter()
            .map(|expression| expression.name.as_str())
    }

    fn next_random_u32(&mut self) -> u32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        (self.rng_state >> 32) as u32
    }
}

fn accumulate_expression_parameter_values(
    parameter_values: &mut [ExpressionParameterValue],
    current_values: &[f32],
    expression: &Live2DExpression,
    expression_index: usize,
    fade_weight: f32,
) {
    for (slot_index, parameter_value) in parameter_values.iter_mut().enumerate() {
        let current_parameter_value = current_values[slot_index];
        let expression_parameter = expression
            .parameters
            .iter()
            .find(|parameter| parameter.parameter_index == Some(parameter_value.parameter_index));

        let (next_additive, next_multiply, next_overwrite) = match expression_parameter {
            Some(parameter) => match parameter.blend {
                ExpressionBlend::Add => (
                    parameter.value,
                    DEFAULT_MULTIPLY_VALUE,
                    current_parameter_value,
                ),
                ExpressionBlend::Multiply => (
                    DEFAULT_ADDITIVE_VALUE,
                    parameter.value,
                    current_parameter_value,
                ),
                ExpressionBlend::Overwrite => (
                    DEFAULT_ADDITIVE_VALUE,
                    DEFAULT_MULTIPLY_VALUE,
                    parameter.value,
                ),
            },
            None => (
                DEFAULT_ADDITIVE_VALUE,
                DEFAULT_MULTIPLY_VALUE,
                current_parameter_value,
            ),
        };

        if expression_index == 0 {
            parameter_value.additive_value = next_additive;
            parameter_value.multiply_value = next_multiply;
            parameter_value.overwrite_value = next_overwrite;
        } else {
            parameter_value.additive_value =
                calculate_value(parameter_value.additive_value, next_additive, fade_weight);
            parameter_value.multiply_value =
                calculate_value(parameter_value.multiply_value, next_multiply, fade_weight);
            parameter_value.overwrite_value =
                calculate_value(parameter_value.overwrite_value, next_overwrite, fade_weight);
        }
    }
}

fn prune_expression_queue(queue: &mut Vec<QueuedExpression>) {
    if queue.len() <= 1 {
        return;
    }

    let latest_fade_weight = queue.last().map(|entry| entry.state_weight).unwrap_or(0.0);
    if latest_fade_weight >= 1.0 {
        let latest = *queue.last().expect("queue is not empty");
        queue.clear();
        queue.push(latest);
    }
}

fn calculate_value(source: f32, destination: f32, fade_weight: f32) -> f32 {
    source * (1.0 - fade_weight) + destination * fade_weight
}

fn expression_fade_in_weight(expression: &Live2DExpression, time_seconds: f32) -> f32 {
    if expression.fade_in_seconds <= f32::EPSILON {
        1.0
    } else {
        easing_sine(time_seconds / expression.fade_in_seconds)
    }
}

fn expression_entries_from_model_json(json: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    json.get("FileReferences")
        .and_then(|value| value.get("Expressions"))
        .and_then(|value| value.as_array())
}

fn easing_sine(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    0.5 - 0.5 * (value * std::f32::consts::PI).cos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_expression_entries_under_file_references() {
        let json = json!({
            "FileReferences": {
                "Expressions": [
                    { "Name": "F01", "File": "expressions/F01.exp3.json" },
                    { "Name": "F02", "File": "expressions/F02.exp3.json" }
                ]
            }
        });

        assert_eq!(
            expression_entries_from_model_json(&json).map(|entries| entries.len()),
            Some(2)
        );
    }

    #[test]
    fn easing_sine_clamps_to_zero_and_one() {
        assert!((easing_sine(-1.0) - 0.0).abs() < 0.0001);
        assert!((easing_sine(2.0) - 1.0).abs() < 0.0001);
        assert!((easing_sine(0.5) - 0.5).abs() < 0.0001);
    }

    #[test]
    fn unknown_blend_falls_back_to_add() {
        assert_eq!(
            ExpressionBlend::from_json_value(Some("Nope")),
            ExpressionBlend::Add
        );
        assert_eq!(
            ExpressionBlend::from_json_value(Some("Multiply")),
            ExpressionBlend::Multiply
        );
        assert_eq!(
            ExpressionBlend::from_json_value(Some("Overwrite")),
            ExpressionBlend::Overwrite
        );
    }

    #[test]
    fn expression_fade_in_weight_clamps_to_one() {
        let expression = Live2DExpression {
            name: "test".into(),
            fade_in_seconds: 1.0,
            fade_out_seconds: 1.0,
            parameters: Vec::new(),
        };
        assert!((expression_fade_in_weight(&expression, 3.0) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn first_expression_uses_target_values_before_global_weight() {
        let expression = Live2DExpression {
            name: "overwrite".into(),
            fade_in_seconds: 1.0,
            fade_out_seconds: 1.0,
            parameters: vec![ExpressionParameter {
                parameter_index: Some(0),
                value: 1.0,
                blend: ExpressionBlend::Overwrite,
            }],
        };
        let mut parameter_values = vec![ExpressionParameterValue {
            parameter_index: 0,
            additive_value: DEFAULT_ADDITIVE_VALUE,
            multiply_value: DEFAULT_MULTIPLY_VALUE,
            overwrite_value: 0.25,
        }];

        accumulate_expression_parameter_values(&mut parameter_values, &[0.25], &expression, 0, 0.5);

        assert!((parameter_values[0].overwrite_value - 1.0).abs() < 0.0001);
        assert!((parameter_values[0].additive_value - 0.0).abs() < 0.0001);
        assert!((parameter_values[0].multiply_value - 1.0).abs() < 0.0001);
    }

    #[test]
    fn later_expression_blends_from_previous_layer() {
        let expression = Live2DExpression {
            name: "add".into(),
            fade_in_seconds: 1.0,
            fade_out_seconds: 1.0,
            parameters: vec![ExpressionParameter {
                parameter_index: Some(0),
                value: 0.5,
                blend: ExpressionBlend::Add,
            }],
        };
        let mut parameter_values = vec![ExpressionParameterValue {
            parameter_index: 0,
            additive_value: 0.0,
            multiply_value: 1.0,
            overwrite_value: 1.0,
        }];

        accumulate_expression_parameter_values(&mut parameter_values, &[0.25], &expression, 1, 0.5);

        assert!((parameter_values[0].additive_value - 0.25).abs() < 0.0001);
        assert!((parameter_values[0].multiply_value - 1.0).abs() < 0.0001);
        assert!((parameter_values[0].overwrite_value - 0.625).abs() < 0.0001);
    }

    #[test]
    fn prune_queue_keeps_only_latest_when_fully_faded_in() {
        let mut queue = vec![
            QueuedExpression {
                expression_index: 0,
                started: true,
                triggered_fade_out: true,
                fade_in_start_time_seconds: 0.0,
                end_time_seconds: 0.5,
                fade_out_seconds: 1.0,
                state_weight: 0.3,
            },
            QueuedExpression {
                expression_index: 1,
                started: true,
                triggered_fade_out: false,
                fade_in_start_time_seconds: 0.0,
                end_time_seconds: -1.0,
                fade_out_seconds: 1.0,
                state_weight: 1.0,
            },
        ];

        prune_expression_queue(&mut queue);

        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].expression_index, 1);
    }

    #[test]
    fn queued_expression_first_update_starts_from_zero_weight() {
        let expression = Live2DExpression {
            name: "test".into(),
            fade_in_seconds: 1.0,
            fade_out_seconds: 1.0,
            parameters: Vec::new(),
        };
        let mut queued = QueuedExpression::new(0, 1.0);

        queued.ensure_started(0.5);

        assert!(queued.started);
        assert!((queued.fade_in_start_time_seconds - 0.5).abs() < 0.0001);
        assert!((queued.fade_weight(&expression, 0.5) - 0.0).abs() < 0.0001);
        assert!(queued.fade_weight(&expression, 1.0) > 0.0);
    }

    #[test]
    fn reset_state_clears_expression_queue_and_time() {
        let mut player = Live2DExpressionPlayer {
            expressions: Vec::new(),
            parameter_indices: vec![0],
            queue: vec![QueuedExpression::new(0, 1.0)],
            rng_state: 7,
            user_time_seconds: 3.0,
        };

        player.reset_state();

        assert!(player.queue.is_empty());
        assert!(player.user_time_seconds.abs() < 0.0001);
        assert_eq!(player.rng_state, EXPRESSION_RNG_SEED);
    }
}
