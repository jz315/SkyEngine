use super::*;
use crate::render::live2d::model::Live2DModel;

pub(super) fn parse_input(
    setting_index: usize,
    input_index: usize,
    input: &serde_json::Value,
    model: &Live2DModel,
) -> Result<Input, String> {
    let source = input.get("Source").ok_or_else(|| {
        format!("physics setting {setting_index} input {input_index} missing Source")
    })?;
    let target = source
        .get("Target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!("physics setting {setting_index} input {input_index} missing Source.Target")
        })?;
    if target != "Parameter" {
        return Err(format!(
            "physics setting {setting_index} input {input_index} unsupported Source.Target `{target}`"
        ));
    }
    let id = source.get("Id").and_then(|v| v.as_str()).ok_or_else(|| {
        format!("physics setting {setting_index} input {input_index} missing Source.Id")
    })?;
    Ok(Input {
        parameter_index: model.find_parameter(id),
        weight: read_f32(input, "Weight", setting_index, input_index, "input")?,
        reflect: input
            .get("Reflect")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        source: parse_source(
            input.get("Type").and_then(|v| v.as_str()),
            setting_index,
            input_index,
            "input",
        )?,
    })
}

pub(super) fn parse_output(
    setting_index: usize,
    output_index: usize,
    output: &serde_json::Value,
    model: &Live2DModel,
) -> Result<Output, String> {
    let destination = output.get("Destination").ok_or_else(|| {
        format!("physics setting {setting_index} output {output_index} missing Destination")
    })?;
    let target = destination
        .get("Target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!(
                "physics setting {setting_index} output {output_index} missing Destination.Target"
            )
        })?;
    if target != "Parameter" {
        return Err(format!(
            "physics setting {setting_index} output {output_index} unsupported Destination.Target `{target}`"
        ));
    }
    let id = destination
        .get("Id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!("physics setting {setting_index} output {output_index} missing Destination.Id")
        })?;

    Ok(Output {
        parameter_index: model.find_parameter(id),
        vertex_index: output
            .get("VertexIndex")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                format!("physics setting {setting_index} output {output_index} missing VertexIndex")
            })? as usize,
        scale: read_f32(output, "Scale", setting_index, output_index, "output")?,
        weight: read_f32(output, "Weight", setting_index, output_index, "output")?,
        reflect: output
            .get("Reflect")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        source: parse_source(
            output.get("Type").and_then(|v| v.as_str()),
            setting_index,
            output_index,
            "output",
        )?,
        value_below_minimum: f32::INFINITY,
        value_exceeded_maximum: f32::NEG_INFINITY,
    })
}

pub(super) fn parse_particle(
    setting_index: usize,
    vertex_index: usize,
    vertex: &serde_json::Value,
) -> Result<Particle, String> {
    let position = parse_vec2(
        vertex.get("Position"),
        format!("physics setting {setting_index} vertex {vertex_index} position"),
    )?;
    Ok(Particle {
        initial_position: position,
        position,
        last_position: position,
        last_gravity: Vec2::new(0.0, 1.0),
        velocity: Vec2::default(),
        force: Vec2::default(),
        mobility: read_f32(vertex, "Mobility", setting_index, vertex_index, "vertex")?,
        delay: read_f32(vertex, "Delay", setting_index, vertex_index, "vertex")?,
        acceleration: read_f32(
            vertex,
            "Acceleration",
            setting_index,
            vertex_index,
            "vertex",
        )?,
        radius: read_f32(vertex, "Radius", setting_index, vertex_index, "vertex")?,
    })
}

pub(super) fn parse_normalization(
    value: Option<&serde_json::Value>,
    label: impl AsRef<str>,
) -> Result<Normalization, String> {
    let value = value.ok_or_else(|| format!("{} missing", label.as_ref()))?;
    Ok(Normalization {
        minimum: value
            .get("Minimum")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .ok_or_else(|| format!("{} missing Minimum", label.as_ref()))?,
        maximum: value
            .get("Maximum")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .ok_or_else(|| format!("{} missing Maximum", label.as_ref()))?,
        default: value
            .get("Default")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .ok_or_else(|| format!("{} missing Default", label.as_ref()))?,
    })
}

pub(super) fn parse_vec2(
    value: Option<&serde_json::Value>,
    label: impl AsRef<str>,
) -> Result<Vec2, String> {
    let value = value.ok_or_else(|| format!("{} missing", label.as_ref()))?;
    Ok(Vec2::new(
        value
            .get("X")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .ok_or_else(|| format!("{} missing X", label.as_ref()))?,
        value
            .get("Y")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .ok_or_else(|| format!("{} missing Y", label.as_ref()))?,
    ))
}

pub(super) fn read_f32(
    value: &serde_json::Value,
    field: &str,
    setting_index: usize,
    item_index: usize,
    kind: &str,
) -> Result<f32, String> {
    value
        .get(field)
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .ok_or_else(|| {
            format!("physics setting {setting_index} {kind} {item_index} missing {field}")
        })
}

pub(super) fn parse_source(
    value: Option<&str>,
    setting_index: usize,
    item_index: usize,
    kind: &str,
) -> Result<Source, String> {
    match value {
        Some("X") => Ok(Source::X),
        Some("Y") => Ok(Source::Y),
        Some("Angle") => Ok(Source::Angle),
        Some(other) => Err(format!(
            "physics setting {setting_index} {kind} {item_index} unsupported Type `{other}`"
        )),
        None => Err(format!(
            "physics setting {setting_index} {kind} {item_index} missing Type"
        )),
    }
}

pub(super) fn update_particles(
    strand: &mut [Particle],
    total_translation: Vec2,
    total_angle: f32,
    wind_direction: Vec2,
    threshold_value: f32,
    delta_time_seconds: f32,
) {
    if strand.is_empty() {
        return;
    }

    strand[0].position = total_translation;
    let total_radian = degrees_to_radian(total_angle);
    let mut current_gravity = radian_to_direction(total_radian);
    current_gravity.normalize();

    for index in 1..strand.len() {
        strand[index].force = current_gravity * strand[index].acceleration + wind_direction;
        strand[index].last_position = strand[index].position;

        let delay = strand[index].delay * delta_time_seconds * 30.0;
        let mut direction = strand[index].position - strand[index - 1].position;
        let radian =
            direction_to_radian(strand[index].last_gravity, current_gravity) / AIR_RESISTANCE;

        direction = rotate_vector(direction, radian);

        strand[index].position = strand[index - 1].position + direction;
        strand[index].position +=
            strand[index].velocity * delay + strand[index].force * delay * delay;

        let mut new_direction = strand[index].position - strand[index - 1].position;
        new_direction.normalize();
        strand[index].position = strand[index - 1].position + new_direction * strand[index].radius;

        if strand[index].position.x.abs() < threshold_value {
            strand[index].position.x = 0.0;
        }

        if delay != 0.0 {
            strand[index].velocity = strand[index].position - strand[index].last_position;
            strand[index].velocity /= delay;
            strand[index].velocity *= strand[index].mobility;
        }

        strand[index].force = Vec2::default();
        strand[index].last_gravity = current_gravity;
    }
}

pub(super) fn update_particles_for_stabilization(
    strand: &mut [Particle],
    total_translation: Vec2,
    total_angle: f32,
    wind_direction: Vec2,
    threshold_value: f32,
) {
    if strand.is_empty() {
        return;
    }

    strand[0].position = total_translation;
    let total_radian = degrees_to_radian(total_angle);
    let mut current_gravity = radian_to_direction(total_radian);
    current_gravity.normalize();

    for index in 1..strand.len() {
        strand[index].force = current_gravity * strand[index].acceleration + wind_direction;
        strand[index].last_position = strand[index].position;
        strand[index].velocity = Vec2::default();

        let mut force = strand[index].force;
        force.normalize();
        force *= strand[index].radius;
        strand[index].position = strand[index - 1].position + force;

        if strand[index].position.x.abs() < threshold_value {
            strand[index].position.x = 0.0;
        }

        strand[index].force = Vec2::default();
        strand[index].last_gravity = current_gravity;
    }
}

pub(super) fn normalize_parameter_value(
    value: f32,
    parameter_minimum: f32,
    parameter_maximum: f32,
    _parameter_default: f32,
    normalization: Normalization,
    is_inverted: bool,
) -> f32 {
    let value = value.clamp(
        parameter_minimum.min(parameter_maximum),
        parameter_minimum.max(parameter_maximum),
    );
    let min_value = parameter_minimum.min(parameter_maximum);
    let max_value = parameter_minimum.max(parameter_maximum);
    let middle_value = min_value + (max_value - min_value).abs() / 2.0;
    let param_value = value - middle_value;
    let result = match sign(param_value) {
        1 => {
            let n_length = normalization.maximum.max(normalization.minimum) - normalization.default;
            let p_length = max_value - middle_value;
            if p_length != 0.0 {
                param_value * (n_length / p_length) + normalization.default
            } else {
                normalization.default
            }
        }
        -1 => {
            let n_length = normalization.minimum.min(normalization.maximum) - normalization.default;
            let p_length = min_value - middle_value;
            if p_length != 0.0 {
                param_value * (n_length / p_length) + normalization.default
            } else {
                normalization.default
            }
        }
        _ => normalization.default,
    };
    if is_inverted {
        result
    } else {
        -result
    }
}

pub(super) fn get_output_value(
    source: Source,
    translation: Vec2,
    particles: &[Particle],
    particle_index: usize,
    is_inverted: bool,
    mut parent_gravity: Vec2,
) -> f32 {
    let mut output_value = match source {
        Source::X => translation.x,
        Source::Y => translation.y,
        Source::Angle => {
            if particle_index >= 2 {
                parent_gravity =
                    particles[particle_index - 1].position - particles[particle_index - 2].position;
            } else {
                parent_gravity.x *= -1.0;
                parent_gravity.y *= -1.0;
            }
            direction_to_radian(parent_gravity, translation)
        }
    };
    if is_inverted {
        output_value *= -1.0;
    }
    output_value
}

pub(super) fn update_output_parameter_value(
    current_parameter_value: f32,
    parameter_minimum: f32,
    parameter_maximum: f32,
    translation: f32,
    output: &mut Output,
) -> f32 {
    let mut value = translation * output.scale;
    if value < parameter_minimum {
        if value < output.value_below_minimum {
            output.value_below_minimum = value;
        }
        value = parameter_minimum;
    } else if value > parameter_maximum {
        if value > output.value_exceeded_maximum {
            output.value_exceeded_maximum = value;
        }
        value = parameter_maximum;
    }
    let weight = output.weight / MAX_WEIGHT;
    if weight >= 1.0 {
        value
    } else {
        current_parameter_value * (1.0 - weight) + value * weight
    }
}

pub(super) fn sign(value: f32) -> i32 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

pub(super) fn degrees_to_radian(degrees: f32) -> f32 {
    (degrees / 180.0) * std::f32::consts::PI
}

pub(super) fn rotate_vector(vector: Vec2, radian: f32) -> Vec2 {
    let sin = radian.sin();
    let cos = radian.cos();
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

pub(super) fn direction_to_radian(from: Vec2, to: Vec2) -> f32 {
    let mut ret = to.y.atan2(to.x) - from.y.atan2(from.x);
    while ret < -std::f32::consts::PI {
        ret += std::f32::consts::PI * 2.0;
    }
    while ret > std::f32::consts::PI {
        ret -= std::f32::consts::PI * 2.0;
    }
    ret
}

pub(super) fn radian_to_direction(total_angle: f32) -> Vec2 {
    Vec2::new(total_angle.sin(), total_angle.cos())
}
