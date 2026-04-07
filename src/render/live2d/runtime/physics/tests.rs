use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model() -> crate::render::live2d::model::Live2DModel {
        let moc_bytes = std::fs::read(
            "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.moc3",
        )
        .expect("sample moc3 should exist");
        crate::render::live2d::model::Live2DModel::from_moc3_bytes(&moc_bytes)
            .expect("sample moc3 should load")
    }

    #[test]
    fn rotate_vector_uses_original_components() {
        let rotated = rotate_vector(Vec2::new(1.0, 0.0), std::f32::consts::FRAC_PI_2);
        assert!(rotated.x.abs() < 0.001);
        assert!((rotated.y - 1.0).abs() < 0.001);
    }

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

    #[test]
    fn stabilization_syncs_rig_outputs_and_parameter_value() {
        let mut model = sample_model();
        let parameter_index = model
            .find_parameter("ParamAngleX")
            .expect("sample model should have ParamAngleX");

        let mut physics = Live2DPhysics {
            gravity: Vec2::new(0.0, 1.0),
            wind: Vec2::default(),
            fps: 30.0,
            current_remain_time: 1.0,
            sub_rigs: vec![SubRig {
                normalization_position: Normalization {
                    minimum: -10.0,
                    maximum: 10.0,
                    default: 0.0,
                },
                normalization_angle: Normalization {
                    minimum: -10.0,
                    maximum: 10.0,
                    default: 0.0,
                },
                inputs: Vec::new(),
                outputs: vec![Output {
                    parameter_index: Some(parameter_index),
                    vertex_index: 1,
                    scale: 1.0,
                    weight: 100.0,
                    reflect: false,
                    source: Source::Y,
                    value_below_minimum: f32::INFINITY,
                    value_exceeded_maximum: f32::NEG_INFINITY,
                }],
                particles: vec![
                    Particle {
                        initial_position: Vec2::default(),
                        position: Vec2::default(),
                        last_position: Vec2::default(),
                        last_gravity: Vec2::new(0.0, 1.0),
                        velocity: Vec2::default(),
                        force: Vec2::default(),
                        mobility: 0.0,
                        delay: 0.0,
                        acceleration: 0.0,
                        radius: 0.0,
                    },
                    Particle {
                        initial_position: Vec2::new(3.0, 3.0),
                        position: Vec2::new(3.0, 3.0),
                        last_position: Vec2::new(3.0, 3.0),
                        last_gravity: Vec2::new(0.0, 1.0),
                        velocity: Vec2::new(2.0, 2.0),
                        force: Vec2::default(),
                        mobility: 0.0,
                        delay: 0.0,
                        acceleration: 1.0,
                        radius: 1.0,
                    },
                ],
            }],
            current_rig_outputs: vec![vec![5.0]],
            previous_rig_outputs: vec![vec![7.0]],
            parameter_caches: Vec::new(),
            parameter_input_caches: Vec::new(),
        };

        physics.reset();
        physics.stabilize(&mut model);

        assert!(physics.current_remain_time.abs() < 0.0001);
        assert!((physics.current_rig_outputs[0][0] - 1.0).abs() < 0.0001);
        assert!((physics.previous_rig_outputs[0][0] - 1.0).abs() < 0.0001);
        assert!((model.parameter_value(parameter_index) - 1.0).abs() < 0.0001);
        assert!((physics.sub_rigs[0].particles[1].position.y - 1.0).abs() < 0.0001);
    }
}
