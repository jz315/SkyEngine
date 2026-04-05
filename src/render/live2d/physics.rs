//! Live2D physics (`physics3.json`) runtime support.

use crate::render::live2d::model::Live2DModel;

const AIR_RESISTANCE: f32 = 5.0;
const MAX_WEIGHT: f32 = 100.0;
const MOVEMENT_THRESHOLD: f32 = 0.001;
const MAX_DELTA_TIME: f32 = 5.0;

#[derive(Debug, Clone, Copy, Default)]
struct Vec2 {
    x: f32,
    y: f32,
}

impl Vec2 {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn normalize(&mut self) {
        let length = (self.x * self.x + self.y * self.y).sqrt();
        if length > f32::EPSILON {
            self.x /= length;
            self.y /= length;
        }
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl std::ops::MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl std::ops::DivAssign<f32> for Vec2 {
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

#[derive(Debug, Clone, Copy)]
struct Normalization {
    minimum: f32,
    maximum: f32,
    default: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    X,
    Y,
    Angle,
}

#[derive(Debug, Clone)]
struct Input {
    parameter_index: Option<usize>,
    weight: f32,
    reflect: bool,
    source: Source,
}

#[derive(Debug, Clone)]
struct Output {
    parameter_index: Option<usize>,
    vertex_index: usize,
    scale: f32,
    weight: f32,
    reflect: bool,
    source: Source,
    value_below_minimum: f32,
    value_exceeded_maximum: f32,
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    initial_position: Vec2,
    position: Vec2,
    last_position: Vec2,
    last_gravity: Vec2,
    velocity: Vec2,
    force: Vec2,
    mobility: f32,
    delay: f32,
    acceleration: f32,
    radius: f32,
}

#[derive(Debug, Clone)]
struct SubRig {
    normalization_position: Normalization,
    normalization_angle: Normalization,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    particles: Vec<Particle>,
}

/// Runtime physics state loaded from a `physics3.json` file.
#[derive(Debug, Clone)]
pub struct Live2DPhysics {
    gravity: Vec2,
    wind: Vec2,
    fps: f32,
    current_remain_time: f32,
    sub_rigs: Vec<SubRig>,
    current_rig_outputs: Vec<Vec<f32>>,
    previous_rig_outputs: Vec<Vec<f32>>,
    parameter_caches: Vec<f32>,
    parameter_input_caches: Vec<f32>,
}

impl Live2DPhysics {
    pub fn from_json_str(text: &str, model: &Live2DModel) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("physics JSON parse error: {e}"))?;
        let meta = json
            .get("Meta")
            .ok_or_else(|| "physics JSON missing Meta".to_string())?;
        let forces = meta
            .get("EffectiveForces")
            .ok_or_else(|| "physics JSON missing Meta.EffectiveForces".to_string())?;

        let gravity = parse_vec2(forces.get("Gravity"), "Meta.EffectiveForces.Gravity")?;
        let wind = parse_vec2(forces.get("Wind"), "Meta.EffectiveForces.Wind")?;
        let fps = meta
            .get("Fps")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(0.0);

        let settings = json
            .get("PhysicsSettings")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "physics JSON missing PhysicsSettings".to_string())?;

        let mut sub_rigs = Vec::with_capacity(settings.len());
        let mut current_rig_outputs = Vec::with_capacity(settings.len());
        let mut previous_rig_outputs = Vec::with_capacity(settings.len());

        for (setting_index, setting) in settings.iter().enumerate() {
            let normalization = setting
                .get("Normalization")
                .ok_or_else(|| format!("physics setting {setting_index} missing Normalization"))?;
            let inputs = setting
                .get("Input")
                .and_then(|v| v.as_array())
                .ok_or_else(|| format!("physics setting {setting_index} missing Input"))?
                .iter()
                .enumerate()
                .map(|(input_index, input)| parse_input(setting_index, input_index, input, model))
                .collect::<Result<Vec<_>, _>>()?;
            let outputs = setting
                .get("Output")
                .and_then(|v| v.as_array())
                .ok_or_else(|| format!("physics setting {setting_index} missing Output"))?
                .iter()
                .enumerate()
                .map(|(output_index, output)| {
                    parse_output(setting_index, output_index, output, model)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let particles = setting
                .get("Vertices")
                .and_then(|v| v.as_array())
                .ok_or_else(|| format!("physics setting {setting_index} missing Vertices"))?
                .iter()
                .enumerate()
                .map(|(vertex_index, vertex)| parse_particle(setting_index, vertex_index, vertex))
                .collect::<Result<Vec<_>, _>>()?;

            current_rig_outputs.push(vec![0.0; outputs.len()]);
            previous_rig_outputs.push(vec![0.0; outputs.len()]);
            sub_rigs.push(SubRig {
                normalization_position: parse_normalization(
                    normalization.get("Position"),
                    format!("physics setting {setting_index} normalization position"),
                )?,
                normalization_angle: parse_normalization(
                    normalization.get("Angle"),
                    format!("physics setting {setting_index} normalization angle"),
                )?,
                inputs,
                outputs,
                particles,
            });
        }

        let mut this = Self {
            gravity,
            wind,
            fps,
            current_remain_time: 0.0,
            sub_rigs,
            current_rig_outputs,
            previous_rig_outputs,
            parameter_caches: Vec::new(),
            parameter_input_caches: Vec::new(),
        };
        this.initialize();
        Ok(this)
    }

    pub fn evaluate(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        if delta_time_seconds <= 0.0 {
            return;
        }

        self.current_remain_time += delta_time_seconds;
        if self.current_remain_time > MAX_DELTA_TIME {
            self.current_remain_time = 0.0;
        }

        let parameter_count = model.parameter_count();
        if self.parameter_caches.len() < parameter_count {
            self.parameter_caches.resize(parameter_count, 0.0);
        }
        if self.parameter_input_caches.len() < parameter_count {
            self.parameter_input_caches.resize(parameter_count, 0.0);
            for index in 0..parameter_count {
                self.parameter_input_caches[index] = model.parameter_value(index);
            }
        }

        let physics_delta_time = if self.fps > 0.0 {
            1.0 / self.fps
        } else {
            delta_time_seconds
        };
        if physics_delta_time <= f32::EPSILON {
            return;
        }

        while self.current_remain_time >= physics_delta_time {
            for (current, previous) in self
                .current_rig_outputs
                .iter()
                .zip(self.previous_rig_outputs.iter_mut())
            {
                previous.copy_from_slice(current);
            }

            let input_weight = physics_delta_time / self.current_remain_time;
            for index in 0..parameter_count {
                let current = model.parameter_value(index);
                self.parameter_caches[index] = self.parameter_input_caches[index]
                    * (1.0 - input_weight)
                    + current * input_weight;
                self.parameter_input_caches[index] = self.parameter_caches[index];
            }

            for (setting_index, rig) in self.sub_rigs.iter_mut().enumerate() {
                let mut total_angle = 0.0f32;
                let mut total_translation = Vec2::default();

                for input in &rig.inputs {
                    let Some(parameter_index) = input.parameter_index else {
                        continue;
                    };
                    let weight = input.weight / MAX_WEIGHT;
                    let value = self.parameter_caches[parameter_index];
                    let minimum = model.parameter_minimum(parameter_index);
                    let maximum = model.parameter_maximum(parameter_index);
                    let default = model.parameter_defaults()[parameter_index];
                    let normalized = match input.source {
                        Source::X | Source::Y => normalize_parameter_value(
                            value,
                            minimum,
                            maximum,
                            default,
                            rig.normalization_position,
                            input.reflect,
                        ),
                        Source::Angle => normalize_parameter_value(
                            value,
                            minimum,
                            maximum,
                            default,
                            rig.normalization_angle,
                            input.reflect,
                        ),
                    } * weight;

                    match input.source {
                        Source::X => total_translation.x += normalized,
                        Source::Y => total_translation.y += normalized,
                        Source::Angle => total_angle += normalized,
                    }
                }

                let rad_angle = degrees_to_radian(-total_angle);
                total_translation.x =
                    total_translation.x * rad_angle.cos() - total_translation.y * rad_angle.sin();
                total_translation.y =
                    total_translation.x * rad_angle.sin() + total_translation.y * rad_angle.cos();

                update_particles(
                    &mut rig.particles,
                    total_translation,
                    total_angle,
                    self.wind,
                    MOVEMENT_THRESHOLD * rig.normalization_position.maximum,
                    physics_delta_time,
                );

                for (output_index, output) in rig.outputs.iter_mut().enumerate() {
                    let Some(parameter_index) = output.parameter_index else {
                        continue;
                    };
                    if output.vertex_index < 1 || output.vertex_index >= rig.particles.len() {
                        continue;
                    }

                    let translation = rig.particles[output.vertex_index].position
                        - rig.particles[output.vertex_index - 1].position;
                    let output_value = get_output_value(
                        output.source,
                        translation,
                        &rig.particles,
                        output.vertex_index,
                        output.reflect,
                        self.gravity,
                    );
                    self.current_rig_outputs[setting_index][output_index] = output_value;
                    self.parameter_caches[parameter_index] = update_output_parameter_value(
                        self.parameter_caches[parameter_index],
                        model.parameter_minimum(parameter_index),
                        model.parameter_maximum(parameter_index),
                        output_value,
                        output,
                    );
                }
            }

            self.current_remain_time -= physics_delta_time;
        }

        let alpha = self.current_remain_time / physics_delta_time;
        self.interpolate(model, alpha);
    }

    fn initialize(&mut self) {
        for rig in &mut self.sub_rigs {
            if let Some((first, rest)) = rig.particles.split_first_mut() {
                first.initial_position = Vec2::default();
                first.position = first.initial_position;
                first.last_position = first.initial_position;
                first.last_gravity = Vec2::new(0.0, 1.0);
                first.velocity = Vec2::default();
                first.force = Vec2::default();

                let mut previous_initial = first.initial_position;
                for particle in rest {
                    particle.initial_position = previous_initial + Vec2::new(0.0, particle.radius);
                    particle.position = particle.initial_position;
                    particle.last_position = particle.initial_position;
                    particle.last_gravity = Vec2::new(0.0, 1.0);
                    particle.velocity = Vec2::default();
                    particle.force = Vec2::default();
                    previous_initial = particle.initial_position;
                }
            }
        }
    }

    fn interpolate(&mut self, model: &mut Live2DModel, weight: f32) {
        for ((rig, current_outputs), previous_outputs) in self
            .sub_rigs
            .iter_mut()
            .zip(self.current_rig_outputs.iter())
            .zip(self.previous_rig_outputs.iter())
        {
            for (output_index, output) in rig.outputs.iter_mut().enumerate() {
                let Some(parameter_index) = output.parameter_index else {
                    continue;
                };
                let blended_output = previous_outputs[output_index] * (1.0 - weight)
                    + current_outputs[output_index] * weight;
                let value = update_output_parameter_value(
                    model.parameter_value(parameter_index),
                    model.parameter_minimum(parameter_index),
                    model.parameter_maximum(parameter_index),
                    blended_output,
                    output,
                );
                model.set_parameter_by_index(parameter_index, value);
            }
        }
    }
}

fn parse_input(
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

fn parse_output(
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

fn parse_particle(
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

fn parse_normalization(
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

fn parse_vec2(value: Option<&serde_json::Value>, label: impl AsRef<str>) -> Result<Vec2, String> {
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

fn read_f32(
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

fn parse_source(
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

fn update_particles(
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

        direction.x = radian.cos() * direction.x - direction.y * radian.sin();
        direction.y = radian.sin() * direction.x + direction.y * radian.cos();

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

fn normalize_parameter_value(
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
        result * -1.0
    }
}

fn get_output_value(
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

fn update_output_parameter_value(
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

fn sign(value: f32) -> i32 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

fn degrees_to_radian(degrees: f32) -> f32 {
    (degrees / 180.0) * std::f32::consts::PI
}

fn direction_to_radian(from: Vec2, to: Vec2) -> f32 {
    let mut ret = to.y.atan2(to.x) - from.y.atan2(from.x);
    while ret < -std::f32::consts::PI {
        ret += std::f32::consts::PI * 2.0;
    }
    while ret > std::f32::consts::PI {
        ret -= std::f32::consts::PI * 2.0;
    }
    ret
}

fn radian_to_direction(total_angle: f32) -> Vec2 {
    Vec2::new(total_angle.sin(), total_angle.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_value_respects_weight() {
        let mut output = Output {
            parameter_index: Some(0),
            vertex_index: 1,
            scale: 2.0,
            weight: 50.0,
            reflect: false,
            source: Source::Angle,
            value_below_minimum: f32::INFINITY,
            value_exceeded_maximum: f32::NEG_INFINITY,
        };
        let value = update_output_parameter_value(4.0, -10.0, 10.0, 6.0, &mut output);
        assert!((value - 7.0).abs() < 0.001);
    }
}
