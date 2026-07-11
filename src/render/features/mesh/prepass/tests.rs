use super::{
    material::SCENE_MATERIAL_VELOCITY_CLEAR_PASS,
    normal::{NormalPrepassInstance, IDENTITY_MATRIX, SCENE_VELOCITY_CLEAR, SCENE_VELOCITY_FORMAT},
    SceneMaterialPrepass, SceneNormalPrepass,
};
use crate::ecs::EntityId;
use crate::render::execution::{
    PhaseSetupContext, PhaseState, PreparedFrame, PreparedView, SceneTexture, TextureFormat,
};
use crate::render::graph::{LoadOp, RenderGraph, ResourceRef, TargetSize};
use crate::render::phase::{DrawFunctionId, MeshDrawData, OpaquePhase, PhaseItem};
use crate::render::view::{Projection, ProjectionViewUniformExt, ViewportRect};
use crate::render::RenderPhase;
use crate::render::SceneView;

fn test_scene_view() -> SceneView {
    let target_size = [64, 64];
    let projection = Projection::orthographic_fixed(64.0, 64.0);
    let transform = crate::render::Transform::default();
    let view_uniform = projection.view_uniform(transform, target_size);
    SceneView::new(
        0,
        ViewportRect::new(0, 0, target_size[0], target_size[1]),
        target_size,
        false,
        u32::MAX,
        transform,
        projection,
        view_uniform,
        true,
    )
}

fn test_opaque_phase() -> OpaquePhase {
    let mut phase = OpaquePhase::new();
    phase.add_item(PhaseItem::new(
        0,
        DrawFunctionId::from_raw(0),
        EntityId::new(1, 0),
        0,
        MeshDrawData::default(),
    ));
    phase
}

fn setup_normal_prepass(graph: &mut RenderGraph, state: &mut PhaseState) {
    let scene_view = test_scene_view();
    let opaque = test_opaque_phase();
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let mut view = PreparedView::new(0, scene_view.viewport, scene_view.target_size, false);
    let _ = view.insert_payload(&scene_view);
    let _ = view.insert_payload(&opaque);
    let mut pass = SceneNormalPrepass::default();
    let mut ctx = PhaseSetupContext::new(graph, state, &frame, &view);
    pass.setup(&mut ctx);
}

fn setup_material_prepass(graph: &mut RenderGraph, state: &mut PhaseState) {
    let scene_view = test_scene_view();
    let opaque = test_opaque_phase();
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let mut view = PreparedView::new(0, scene_view.viewport, scene_view.target_size, false);
    let _ = view.insert_payload(&scene_view);
    let _ = view.insert_payload(&opaque);
    let mut pass = SceneMaterialPrepass::default();
    let mut ctx = PhaseSetupContext::new(graph, state, &frame, &view);
    pass.setup(&mut ctx);
}

fn keep_velocity_alive(graph: &mut RenderGraph, velocity: crate::render::execution::TextureSlot) {
    let sink = graph.create_texture(|builder| {
        builder
            .name("velocity_test_sink")
            .size(TargetSize::Exact(64, 64))
            .format(velocity.format())
            .persistent();
    });
    graph.add_render_pass("velocity_test_sink", |setup| {
        setup.read(velocity.handle());
        setup.write_color(0, sink);
    });
}

#[test]
fn modern_3d_material_prepass_publishes_all_gbuffer_slots() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);

    setup_material_prepass(&mut graph, &mut state);

    for texture in [
        SceneTexture::Depth,
        SceneTexture::Normal,
        SceneTexture::Velocity,
        SceneTexture::Albedo,
        SceneTexture::Material,
        SceneTexture::Emissive,
    ] {
        let slot = state
            .scene_texture(texture)
            .unwrap_or_else(|| panic!("material prepass should publish {}", texture.label()));
        assert_eq!(slot.format(), texture.modern_3d_format());
    }

    let material = state
        .scene_material()
        .expect("material prepass should publish material target");
    let sink = graph.create_texture(|builder| {
        builder
            .name("material_contract_sink")
            .size(TargetSize::Exact(64, 64))
            .format(material.format())
            .persistent();
    });
    graph.add_render_pass("material_contract_sink", |setup| {
        setup.read(material.handle());
        setup.write_color(0, sink);
    });
    let compiled = graph.compile().expect("material graph should compile");
    let material_pass = compiled
        .iter()
        .find(|pass| pass.name == "scene_material_prepass")
        .expect("material prepass should stay alive");
    assert_eq!(
        material_pass.color_outputs[1].load,
        LoadOp::Clear([1.0, 0.0, 1.0, 0.0])
    );
}

#[test]
fn scene_material_prepass_writes_velocity() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);

    setup_material_prepass(&mut graph, &mut state);

    let velocity = state
        .scene_velocity()
        .expect("material prepass should publish scene velocity");
    assert_eq!(velocity.format(), SCENE_VELOCITY_FORMAT);
    keep_velocity_alive(&mut graph, velocity);
    let compiled = graph.compile().expect("velocity graph should compile");
    let clear_pass = compiled
        .iter()
        .find(|pass| pass.name == SCENE_MATERIAL_VELOCITY_CLEAR_PASS)
        .expect("material prepass should clear velocity when no normal prepass wrote it");
    assert_eq!(
        clear_pass.color_outputs[0].target,
        ResourceRef::Texture(velocity.handle())
    );
    assert_eq!(
        clear_pass.color_outputs[0].load,
        LoadOp::Clear(SCENE_VELOCITY_CLEAR)
    );
}

#[test]
fn scene_material_prepass_preserves_existing_velocity() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);

    setup_normal_prepass(&mut graph, &mut state);
    let velocity = state
        .scene_velocity()
        .expect("normal prepass should publish scene velocity");
    setup_material_prepass(&mut graph, &mut state);

    assert_eq!(
        state
            .scene_velocity()
            .expect("material prepass should keep scene velocity")
            .handle(),
        velocity.handle()
    );
    keep_velocity_alive(&mut graph, velocity);
    let compiled = graph.compile().expect("velocity graph should compile");
    assert!(
        compiled
            .iter()
            .all(|pass| pass.name != SCENE_MATERIAL_VELOCITY_CLEAR_PASS),
        "material prepass should not overwrite motion vectors from normal prepass"
    );
}

#[test]
fn static_mesh_velocity_is_zero() {
    let instance = NormalPrepassInstance::from_models(IDENTITY_MATRIX, IDENTITY_MATRIX);
    let current = clip_uv(transform_instance_position(
        [
            instance.model_col0,
            instance.model_col1,
            instance.model_col2,
            instance.model_col3,
        ],
        [0.0, 0.0, 0.0],
    ));
    let previous = clip_uv(transform_instance_position(
        [
            instance.prev_model_col0,
            instance.prev_model_col1,
            instance.prev_model_col2,
            instance.prev_model_col3,
        ],
        [0.0, 0.0, 0.0],
    ));

    assert_eq!(
        [previous[0] - current[0], previous[1] - current[1]],
        [0.0, 0.0]
    );
    assert_eq!(SCENE_VELOCITY_CLEAR, [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn moving_mesh_velocity_is_nonzero() {
    let current_model = translated_model(1.0, 0.0, 0.0);
    let instance = NormalPrepassInstance::from_models(current_model, IDENTITY_MATRIX);
    let current = clip_uv(transform_instance_position(
        [
            instance.model_col0,
            instance.model_col1,
            instance.model_col2,
            instance.model_col3,
        ],
        [0.0, 0.0, 0.0],
    ));
    let previous = clip_uv(transform_instance_position(
        [
            instance.prev_model_col0,
            instance.prev_model_col1,
            instance.prev_model_col2,
            instance.prev_model_col3,
        ],
        [0.0, 0.0, 0.0],
    ));
    let velocity = [previous[0] - current[0], previous[1] - current[1]];

    assert!(velocity[0].abs() > 0.0 || velocity[1].abs() > 0.0);
}

fn translated_model(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        x, y, z, 1.0,
    ]
}

fn transform_instance_position(cols: [[f32; 4]; 4], position: [f32; 3]) -> [f32; 4] {
    let [x, y, z] = position;
    [
        cols[0][0] * x + cols[1][0] * y + cols[2][0] * z + cols[3][0],
        cols[0][1] * x + cols[1][1] * y + cols[2][1] * z + cols[3][1],
        cols[0][2] * x + cols[1][2] * y + cols[2][2] * z + cols[3][2],
        cols[0][3] * x + cols[1][3] * y + cols[2][3] * z + cols[3][3],
    ]
}

fn clip_uv(clip: [f32; 4]) -> [f32; 2] {
    let inv_w = clip[3].recip();
    let ndc = [clip[0] * inv_w, clip[1] * inv_w];
    [ndc[0] * 0.5 + 0.5, ndc[1] * -0.5 + 0.5]
}
