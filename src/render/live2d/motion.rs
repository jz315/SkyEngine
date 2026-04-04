//! Minimal `motion3.json` runtime ported from the Cubism / SakuraEngine path.
//!
//! Scope for now:
//! - load motions from the `Idle` group
//! - evaluate `Parameter` and `PartOpacity` curves
//! - loop the active idle motion

use crate::render::live2d::model::Live2DModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MotionCurveTarget {
    Model,
    Parameter,
    PartOpacity,
}

#[derive(Debug, Clone, Copy)]
struct MotionPoint {
    time: f32,
    value: f32,
}

#[derive(Debug, Clone)]
enum MotionSegment {
    Linear([MotionPoint; 2]),
    Bezier([MotionPoint; 4]),
    Stepped([MotionPoint; 2]),
    InverseStepped([MotionPoint; 2]),
}

impl MotionSegment {
    fn end_time(&self) -> f32 {
        match self {
            Self::Linear(points) | Self::Stepped(points) | Self::InverseStepped(points) => {
                points[1].time
            }
            Self::Bezier(points) => points[3].time,
        }
    }

    fn value_at(&self, time: f32) -> f32 {
        match self {
            Self::Linear(points) => linear_evaluate(*points, time),
            Self::Bezier(points) => bezier_evaluate(*points, time),
            Self::Stepped(points) => points[0].value,
            Self::InverseStepped(points) => points[1].value,
        }
    }
}

#[derive(Debug, Clone)]
struct MotionCurve {
    target: MotionCurveTarget,
    parameter_index: Option<usize>,
    part_index: Option<usize>,
    segments: Vec<MotionSegment>,
}

impl MotionCurve {
    fn evaluate(&self, time: f32) -> Option<f32> {
        let first = self.segments.first()?;
        if time <= first.end_time() {
            return Some(first.value_at(time));
        }

        for segment in &self.segments {
            if time <= segment.end_time() {
                return Some(segment.value_at(time));
            }
        }

        self.segments
            .last()
            .map(|segment| segment.value_at(segment.end_time()))
    }
}

#[derive(Debug, Clone)]
struct Live2DMotion {
    duration: f32,
    looped: bool,
    curves: Vec<MotionCurve>,
}

impl Live2DMotion {
    fn from_json_str(text: &str, model: &Live2DModel) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("motion JSON parse error: {e}"))?;

        let meta = json
            .get("Meta")
            .ok_or_else(|| "motion JSON missing Meta".to_string())?;
        let duration = meta
            .get("Duration")
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
            .ok_or_else(|| "motion JSON missing Meta.Duration".to_string())?;
        let looped = meta
            .get("Loop")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let curves = json
            .get("Curves")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "motion JSON missing Curves".to_string())?;

        let mut parsed_curves = Vec::with_capacity(curves.len());
        for (curve_index, curve) in curves.iter().enumerate() {
            parsed_curves.push(parse_curve(curve_index, curve, model)?);
        }

        Ok(Self {
            duration,
            looped,
            curves: parsed_curves,
        })
    }

    fn apply(&self, model: &mut Live2DModel, time: f32) {
        let time = if self.looped && self.duration > f32::EPSILON {
            time.rem_euclid(self.duration)
        } else {
            time.min(self.duration)
        };

        for curve in &self.curves {
            let Some(value) = curve.evaluate(time) else {
                continue;
            };
            match curve.target {
                MotionCurveTarget::Parameter => {
                    if let Some(parameter_index) = curve.parameter_index {
                        model.set_parameter_by_index(parameter_index, value);
                    }
                }
                MotionCurveTarget::PartOpacity => {
                    if let Some(part_index) = curve.part_index {
                        model.set_part_opacity(part_index, value);
                    }
                }
                MotionCurveTarget::Model => {}
            }
        }
    }
}

/// Minimal idle-motion player.
#[derive(Debug, Clone, Default)]
pub struct Live2DMotionPlayer {
    idle_motions: Vec<Live2DMotion>,
    current_motion: usize,
    time_seconds: f32,
}

impl Live2DMotionPlayer {
    pub fn from_model_json(
        json: &serde_json::Value,
        base_dir: &std::path::Path,
        model: &Live2DModel,
    ) -> Result<Option<Self>, String> {
        let idle_entries = idle_entries_from_model_json(json);

        let Some(idle_entries) = idle_entries else {
            return Ok(None);
        };

        let mut idle_motions = Vec::new();
        for entry in idle_entries {
            let Some(file) = entry.get("File").and_then(|value| value.as_str()) else {
                continue;
            };
            let path = base_dir.join(file);
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read motion {}: {e}", path.display()))?;
            idle_motions.push(Live2DMotion::from_json_str(&text, model)?);
        }

        if idle_motions.is_empty() {
            return Ok(None);
        }

        Ok(Some(Self {
            idle_motions,
            current_motion: 0,
            time_seconds: 0.0,
        }))
    }

    pub fn update(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        if self.idle_motions.is_empty() {
            return;
        }

        self.time_seconds += delta_time_seconds.max(0.0);
        let motion = &self.idle_motions[self.current_motion];
        motion.apply(model, self.time_seconds);
    }

    pub fn motion_count(&self) -> usize {
        self.idle_motions.len()
    }
}

fn idle_entries_from_model_json(
    json: &serde_json::Value,
) -> Option<&Vec<serde_json::Value>> {
    json.get("FileReferences")
        .and_then(|value| value.get("Motions"))
        .and_then(|value| value.get("Idle"))
        .and_then(|value| value.as_array())
}

fn parse_curve(
    curve_index: usize,
    curve: &serde_json::Value,
    model: &Live2DModel,
) -> Result<MotionCurve, String> {
    let target = match curve.get("Target").and_then(|value| value.as_str()) {
        Some("Model") => MotionCurveTarget::Model,
        Some("Parameter") => MotionCurveTarget::Parameter,
        Some("PartOpacity") => MotionCurveTarget::PartOpacity,
        Some(other) => {
            return Err(format!(
                "unsupported motion target `{other}` at curve {curve_index}"
            ));
        }
        None => return Err(format!("motion curve {curve_index} missing Target")),
    };

    let id = curve
        .get("Id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("motion curve {curve_index} missing Id"))?
        .to_string();

    let segments = curve
        .get("Segments")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("motion curve {curve_index} missing Segments"))?;
    let values: Vec<f32> = segments
        .iter()
        .filter_map(|value| value.as_f64())
        .map(|value| value as f32)
        .collect();
    let parsed_segments = parse_segments(&values)
        .map_err(|err| format!("motion curve {curve_index} invalid segments: {err}"))?;

    Ok(MotionCurve {
        target,
        parameter_index: if target == MotionCurveTarget::Parameter {
            model.find_parameter(&id)
        } else {
            None
        },
        part_index: if target == MotionCurveTarget::PartOpacity {
            model.find_part(&id)
        } else {
            None
        },
        segments: parsed_segments,
    })
}

fn parse_segments(values: &[f32]) -> Result<Vec<MotionSegment>, String> {
    if values.len() < 2 {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let mut position = 0usize;
    let mut current = MotionPoint {
        time: values[position],
        value: values[position + 1],
    };
    position += 2;

    while position < values.len() {
        let segment_type = values[position] as i32;
        position += 1;

        let segment = match segment_type {
            0 => {
                ensure_segment_capacity(values, position, 2)?;
                let next = MotionPoint {
                    time: values[position],
                    value: values[position + 1],
                };
                position += 2;
                let segment = MotionSegment::Linear([current, next]);
                current = next;
                segment
            }
            1 => {
                ensure_segment_capacity(values, position, 6)?;
                let p1 = MotionPoint {
                    time: values[position],
                    value: values[position + 1],
                };
                let p2 = MotionPoint {
                    time: values[position + 2],
                    value: values[position + 3],
                };
                let p3 = MotionPoint {
                    time: values[position + 4],
                    value: values[position + 5],
                };
                position += 6;
                let segment = MotionSegment::Bezier([current, p1, p2, p3]);
                current = p3;
                segment
            }
            2 => {
                ensure_segment_capacity(values, position, 2)?;
                let next = MotionPoint {
                    time: values[position],
                    value: values[position + 1],
                };
                position += 2;
                let segment = MotionSegment::Stepped([current, next]);
                current = next;
                segment
            }
            3 => {
                ensure_segment_capacity(values, position, 2)?;
                let next = MotionPoint {
                    time: values[position],
                    value: values[position + 1],
                };
                position += 2;
                let segment = MotionSegment::InverseStepped([current, next]);
                current = next;
                segment
            }
            other => return Err(format!("unsupported segment type {other}")),
        };

        result.push(segment);
    }

    Ok(result)
}

fn ensure_segment_capacity(values: &[f32], position: usize, needed: usize) -> Result<(), String> {
    if position + needed <= values.len() {
        Ok(())
    } else {
        Err("segment truncated".to_string())
    }
}

fn linear_evaluate(points: [MotionPoint; 2], time: f32) -> f32 {
    let span = (points[1].time - points[0].time).max(f32::EPSILON);
    let t = ((time - points[0].time) / span).clamp(0.0, 1.0);
    points[0].value + (points[1].value - points[0].value) * t
}

fn bezier_evaluate(points: [MotionPoint; 4], time: f32) -> f32 {
    let span = (points[3].time - points[0].time).max(f32::EPSILON);
    let t = ((time - points[0].time) / span).clamp(0.0, 1.0);

    let p01 = lerp_point(points[0], points[1], t);
    let p12 = lerp_point(points[1], points[2], t);
    let p23 = lerp_point(points[2], points[3], t);
    let p012 = lerp_point(p01, p12, t);
    let p123 = lerp_point(p12, p23, t);
    lerp_point(p012, p123, t).value
}

fn lerp_point(a: MotionPoint, b: MotionPoint, t: f32) -> MotionPoint {
    MotionPoint {
        time: a.time + (b.time - a.time) * t,
        value: a.value + (b.value - a.value) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_linear_segments() {
        let segments = parse_segments(&[0.0, 1.0, 0.0, 1.0, 2.0]).unwrap();
        assert_eq!(segments.len(), 1);
        match &segments[0] {
            MotionSegment::Linear(points) => {
                assert_eq!(points[0].time, 0.0);
                assert_eq!(points[1].time, 1.0);
                assert_eq!(points[1].value, 2.0);
            }
            _ => panic!("expected linear"),
        }
    }

    #[test]
    fn evaluates_linear_segment() {
        let value = linear_evaluate(
            [
                MotionPoint {
                    time: 0.0,
                    value: 0.0,
                },
                MotionPoint {
                    time: 1.0,
                    value: 10.0,
                },
            ],
            0.25,
        );
        assert!((value - 2.5).abs() < 0.001);
    }

    #[test]
    fn finds_idle_motions_under_file_references() {
        let json = json!({
            "FileReferences": {
                "Motions": {
                    "Idle": [
                        { "File": "motions/a.motion3.json" },
                        { "File": "motions/b.motion3.json" }
                    ]
                }
            }
        });

        assert_eq!(idle_entries_from_model_json(&json).map(|value| value.len()), Some(2));
    }
}
