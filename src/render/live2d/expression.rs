//! Minimal `exp3.json` expression runtime ported from Cubism / SakuraEngine.
//!
//! Scope for now:
//! - load expressions from `FileReferences.Expressions`
//! - evaluate a single active expression
//! - apply Add / Multiply / Overwrite blends with Cubism-style weighting

use std::path::Path;

use crate::render::live2d::model::Live2DModel;

const DEFAULT_FADE_TIME_SECONDS: f32 = 1.0;

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
    #[allow(dead_code)]
    fade_out_seconds: f32,
    parameters: Vec<ExpressionParameter>,
}

impl Live2DExpression {
    fn from_json_str(name: String, text: &str, model: &Live2DModel) -> Result<Self, String> {
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
                parameter_index: model.find_parameter(id),
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
struct ActiveExpression {
    expression_index: usize,
    time_seconds: f32,
}

/// Minimal expression player with one active expression at a time.
#[derive(Debug, Clone, Default)]
pub struct Live2DExpressionPlayer {
    expressions: Vec<Live2DExpression>,
    active: Option<ActiveExpression>,
}

impl Live2DExpressionPlayer {
    pub fn from_model_json(
        json: &serde_json::Value,
        base_dir: &Path,
        model: &Live2DModel,
    ) -> Result<Option<Self>, String> {
        let Some(expression_entries) = expression_entries_from_model_json(json) else {
            return Ok(None);
        };

        let mut expressions = Vec::with_capacity(expression_entries.len());
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

            expressions.push(Live2DExpression::from_json_str(
                name.to_string(),
                &text,
                model,
            )?);
        }

        if expressions.is_empty() {
            return Ok(None);
        }

        Ok(Some(Self {
            expressions,
            active: None,
        }))
    }

    pub fn update(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        let Some(active) = self.active.as_mut() else {
            return;
        };

        active.time_seconds += delta_time_seconds.max(0.0);
        let expression = &self.expressions[active.expression_index];
        let weight = if expression.fade_in_seconds <= f32::EPSILON {
            1.0
        } else {
            easing_sine(active.time_seconds / expression.fade_in_seconds)
        };

        for parameter in &expression.parameters {
            let Some(parameter_index) = parameter.parameter_index else {
                continue;
            };

            match parameter.blend {
                ExpressionBlend::Add => {
                    model.add_parameter_weighted_by_index(parameter_index, parameter.value, weight);
                }
                ExpressionBlend::Multiply => {
                    model.multiply_parameter_weighted_by_index(
                        parameter_index,
                        parameter.value,
                        weight,
                    );
                }
                ExpressionBlend::Overwrite => {
                    model.set_parameter_weighted_by_index(parameter_index, parameter.value, weight);
                }
            }
        }
    }

    pub fn set_expression(&mut self, name: &str) -> bool {
        if let Some(index) = self
            .expressions
            .iter()
            .position(|expression| expression.name == name)
        {
            self.active = Some(ActiveExpression {
                expression_index: index,
                time_seconds: 0.0,
            });
            true
        } else {
            false
        }
    }

    pub fn expression_count(&self) -> usize {
        self.expressions.len()
    }

    pub fn expression_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.expressions
            .iter()
            .map(|expression| expression.name.as_str())
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
}
