use super::*;
use crate::render::live2d::model::Live2DModel;

pub(super) fn parse_curve(
    curve_index: usize,
    curve: &serde_json::Value,
    model: &mut Live2DModel,
    are_beziers_restricted: bool,
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
    let parsed_segments = parse_segments(&values, are_beziers_restricted)
        .map_err(|err| format!("motion curve {curve_index} invalid segments: {err}"))?;

    // Per-curve fade overrides (default -1.0 = use motion-level)
    let fade_in_time = curve
        .get("FadeInTime")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(-1.0);
    let fade_out_time = curve
        .get("FadeOutTime")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(-1.0);

    Ok(MotionCurve {
        target,
        parameter_index: if target == MotionCurveTarget::Parameter {
            Some(model.ensure_parameter_slot(&id))
        } else {
            None
        },
        part_opacity_parameter_index: if target == MotionCurveTarget::PartOpacity {
            Some(model.ensure_parameter_slot(&id))
        } else {
            None
        },
        id,
        segments: parsed_segments,
        fade_in_time,
        fade_out_time,
    })
}

/// Extract parameter indices for a named group (e.g. "EyeBlink", "LipSync")
/// from the `Groups` section of a `model3.json` file.
pub(super) fn extract_group_parameter_indices(
    json: &serde_json::Value,
    group_name: &str,
    model: &mut Live2DModel,
) -> Vec<usize> {
    json.get("Groups")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|group| {
            group.get("Target").and_then(|v| v.as_str()) == Some("Parameter")
                && group.get("Name").and_then(|v| v.as_str()) == Some(group_name)
        })
        .flat_map(|group| {
            group
                .get("Ids")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
        })
        .map(|id| model.ensure_parameter_slot(id))
        .collect()
}

pub(super) fn parse_segments(
    values: &[f32],
    are_beziers_restricted: bool,
) -> Result<Vec<MotionSegment>, String> {
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
                let segment = if are_beziers_restricted {
                    MotionSegment::BezierRestricted([current, p1, p2, p3])
                } else {
                    MotionSegment::Bezier([current, p1, p2, p3])
                };
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

pub(super) fn ensure_segment_capacity(
    values: &[f32],
    position: usize,
    needed: usize,
) -> Result<(), String> {
    if position + needed <= values.len() {
        Ok(())
    } else {
        Err("segment truncated".to_string())
    }
}

pub(super) fn linear_evaluate(points: [MotionPoint; 2], time: f32) -> f32 {
    let span = (points[1].time - points[0].time).max(f32::EPSILON);
    let t = ((time - points[0].time) / span).clamp(0.0, 1.0);
    points[0].value + (points[1].value - points[0].value) * t
}

pub(super) fn bezier_evaluate(points: [MotionPoint; 4], time: f32) -> f32 {
    let t = solve_bezier_parameter(points, time);
    cubic_bezier_scalar(
        points[0].value,
        points[1].value,
        points[2].value,
        points[3].value,
        t,
    )
}

pub(super) fn bezier_evaluate_restricted(points: [MotionPoint; 4], time: f32) -> f32 {
    let start_time = points[0].time;
    let end_time = points[3].time;
    if (end_time - start_time).abs() <= f32::EPSILON {
        return points[0].value;
    }

    let t = ((time - start_time) / (end_time - start_time)).clamp(0.0, 1.0);
    cubic_bezier_scalar(
        points[0].value,
        points[1].value,
        points[2].value,
        points[3].value,
        t,
    )
}

pub(super) fn solve_bezier_parameter(points: [MotionPoint; 4], time: f32) -> f32 {
    let start_time = points[0].time;
    let end_time = points[3].time;
    if (end_time - start_time).abs() <= f32::EPSILON {
        return 0.0;
    }

    let target_time = time.clamp(start_time, end_time);
    let mut low = 0.0f32;
    let mut high = 1.0f32;

    for _ in 0..20 {
        let mid = (low + high) * 0.5;
        let mid_time = cubic_bezier_scalar(
            points[0].time,
            points[1].time,
            points[2].time,
            points[3].time,
            mid,
        );
        if mid_time < target_time {
            low = mid;
        } else {
            high = mid;
        }
    }

    (low + high) * 0.5
}

fn cubic_bezier_scalar(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let one_minus_t = 1.0 - t;
    one_minus_t * one_minus_t * one_minus_t * p0
        + 3.0 * one_minus_t * one_minus_t * t * p1
        + 3.0 * one_minus_t * t * t * p2
        + t * t * t * p3
}
