use super::*;

fn sample_model() -> Live2DModel {
    let moc_bytes = std::fs::read(
        "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.moc3",
    )
    .expect("sample moc3 should exist");
    Live2DModel::from_moc3_bytes(&moc_bytes).expect("sample moc3 should load")
}

#[test]
fn layout_transform_matches_cubism_width_then_bottom() {
    let layout = Live2DLayout {
        width: Some(2.0),
        bottom: Some(-1.0),
        ..Default::default()
    };
    let (transform, has_size_override) = build_layout_transform(4.0, 8.0, layout);
    assert!(has_size_override);
    assert!((transform.scale_x - 0.5).abs() < 0.001);
    assert!((transform.scale_y - 0.5).abs() < 0.001);
    assert!((transform.translate_y - (-5.0)).abs() < 0.001);
}

#[test]
fn portrait_fit_switches_default_model_to_width_fit() {
    let default = default_render_transform(3.0, 4.0);
    let fitted = fit_render_transform_for_view(default, 3.0, false, 720.0, 1280.0);
    assert!((fitted.scale_x - (2.0 / 3.0)).abs() < 0.001);
    assert!((fitted.scale_y - (2.0 / 3.0)).abs() < 0.001);
}

#[test]
fn model_opacity_is_tracked_separately_from_drawables() {
    let mut model = sample_model();
    assert!(model.drawable_count() > 0);

    let base_opacity = model.drawable_opacity(0);
    model.set_model_opacity(0.25);

    assert!((model.model_opacity() - 0.25).abs() < 0.0001);
    assert!((model.drawable_opacity(0) - base_opacity).abs() < 0.0001);
}

#[test]
fn hit_test_is_disabled_while_model_is_translucent() {
    let mut model = sample_model();
    let drawable_index = (0..model.drawable_count())
        .find(|&index| {
            model.drawable_is_visible(index)
                && model.drawable_opacity(index) > 0.0
                && model.drawable_bounds(index).is_some()
        })
        .expect("sample model should have a visible drawable");

    let (min, max) = model
        .drawable_bounds(drawable_index)
        .expect("visible drawable should have bounds");
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];

    assert!(model.hit_test_drawable(drawable_index, center));

    model.set_model_opacity(0.5);
    assert!(!model.hit_test_drawable(drawable_index, center));
}

#[test]
fn extra_virtual_parameter_slots_rebuild_in_stable_order() {
    let mut template_model = sample_model();
    let slot_a = template_model.ensure_parameter_slot("ParamMissingA");
    let slot_b = template_model.ensure_parameter_slot("ParamMissingB");
    let extra_ids = template_model.extra_virtual_parameter_ids().to_vec();

    assert!(slot_b > slot_a);
    assert_eq!(
        extra_ids,
        vec!["ParamMissingA".to_string(), "ParamMissingB".to_string()]
    );

    let mut instance_model = sample_model();
    instance_model.register_virtual_parameter_ids(&extra_ids);

    assert_eq!(instance_model.find_parameter("ParamMissingA"), Some(slot_a));
    assert_eq!(instance_model.find_parameter("ParamMissingB"), Some(slot_b));
}
