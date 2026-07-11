use super::*;
use crate::render::live2d::model::Live2DModel;

impl Live2DPhysics {
    pub fn options(&self) -> Live2DPhysicsOptions {
        Live2DPhysicsOptions {
            gravity: [self.gravity.x, self.gravity.y],
            wind: [self.wind.x, self.wind.y],
        }
    }

    pub fn set_options(&mut self, options: Live2DPhysicsOptions) {
        self.gravity = Vec2::new(options.gravity[0], options.gravity[1]);
        self.wind = Vec2::new(options.wind[0], options.wind[1]);
    }

    pub fn reset(&mut self) {
        self.set_options(Live2DPhysicsOptions::default());
        self.current_remain_time = 0.0;
        for outputs in &mut self.current_rig_outputs {
            outputs.fill(0.0);
        }
        for outputs in &mut self.previous_rig_outputs {
            outputs.fill(0.0);
        }
        self.parameter_caches.clear();
        self.parameter_input_caches.clear();
        self.initialize();
    }

    pub fn stabilize(&mut self, model: &mut Live2DModel) {
        self.current_remain_time = 0.0;
        self.ensure_parameter_cache_capacity(model);
        self.seed_parameter_caches_from_model(model);

        for (setting_index, rig) in self.sub_rigs.iter_mut().enumerate() {
            let mut total_angle = 0.0f32;
            let mut total_translation = Vec2::default();

            for input in &rig.inputs {
                let Some(parameter_index) = input.parameter_index else {
                    continue;
                };
                let weight = input.weight / MAX_WEIGHT;
                let value = model.parameter_value(parameter_index);
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
                self.parameter_caches[parameter_index] = value;
            }

            let rad_angle = degrees_to_radian(-total_angle);
            total_translation = rotate_vector(total_translation, rad_angle);

            update_particles_for_stabilization(
                &mut rig.particles,
                total_translation,
                total_angle,
                self.wind,
                MOVEMENT_THRESHOLD * rig.normalization_position.maximum,
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
                self.previous_rig_outputs[setting_index][output_index] = output_value;

                let value = update_output_parameter_value(
                    model.parameter_value(parameter_index),
                    model.parameter_minimum(parameter_index),
                    model.parameter_maximum(parameter_index),
                    output_value,
                    output,
                );
                model.set_parameter_clamped_no_repeat_by_index(parameter_index, value);
                self.parameter_caches[parameter_index] = model.parameter_value(parameter_index);
            }
        }
    }

    pub fn from_json_str(text: &str, model: &Live2DModel) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("physics JSON parse error: {e}"))?;
        let meta = json
            .get("Meta")
            .ok_or_else(|| "physics JSON missing Meta".to_string())?;
        let forces = meta
            .get("EffectiveForces")
            .ok_or_else(|| "physics JSON missing Meta.EffectiveForces".to_string())?;

        let _gravity = parse_vec2(forces.get("Gravity"), "Meta.EffectiveForces.Gravity")?;
        let _wind = parse_vec2(forces.get("Wind"), "Meta.EffectiveForces.Wind")?;
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
            gravity: Vec2::new(DEFAULT_GRAVITY[0], DEFAULT_GRAVITY[1]),
            wind: Vec2::new(DEFAULT_WIND[0], DEFAULT_WIND[1]),
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

        self.ensure_parameter_cache_capacity(model);
        let parameter_count = model.parameter_count();

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
                total_translation = rotate_vector(total_translation, rad_angle);

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
                model.set_parameter_clamped_no_repeat_by_index(parameter_index, value);
            }
        }
    }

    fn ensure_parameter_cache_capacity(&mut self, model: &Live2DModel) {
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
    }

    fn seed_parameter_caches_from_model(&mut self, model: &Live2DModel) {
        let parameter_count = model.parameter_count();
        for index in 0..parameter_count {
            let value = model.parameter_value(index);
            self.parameter_caches[index] = value;
            self.parameter_input_caches[index] = value;
        }
    }
}
