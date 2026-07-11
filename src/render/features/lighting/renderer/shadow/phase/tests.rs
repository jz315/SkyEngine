use super::*;
use crate::ecs::{EntityId, World};
use crate::gpu::GpuContext;
use crate::render::execution::{PhaseState, PreparedFrame, PreparedView};
use crate::render::graph::{RenderGraph, ResourceRef, TargetSize};
use crate::render::lighting::shadow::{
    append_directional_shadow_views, create_shadow_compare_sampler, sync_shadow_views,
    ShadowResourceKind,
};
use crate::render::phase::{DrawFunctionId, DrawSprite, MeshDrawData, PhaseItem, SpriteDrawData};
use crate::render::pipeline::RenderPhase;
use crate::render::resources::mesh::VertexAttribute;
use crate::render::view::{Projection, ProjectionViewUniformExt, ViewportRect};
use crate::render::{DirectionalLight, GpuScene, LightTable, MaterialHandle, Transform};

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for shadow phase tests");

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("shadow_phase_test_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("Failed to create test GPU device")
}

#[test]
fn opaque_shadow_vertex_layout_remains_position_only() {
    let layout = VertexLayout::new(
        20,
        [
            VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
            VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
        ],
    );

    let attributes = shadow_vertex_attributes(&layout, ShadowPipelineKind::Opaque)
        .expect("opaque shadow layout should require only position");

    assert_eq!(attributes.len(), 1);
    assert_eq!(attributes[0].shader_location, 0);
    assert_eq!(attributes[0].offset, 0);
    assert_eq!(attributes[0].format, wgpu::VertexFormat::Float32x3);
}

#[test]
fn alpha_test_shadow_vertex_layout_requires_uv0() {
    let layout = VertexLayout::new(
        12,
        [VertexAttribute::new(
            VertexSemantic::Position,
            wgpu::VertexFormat::Float32x3,
            0,
        )],
    );

    let error = shadow_vertex_attributes(&layout, ShadowPipelineKind::AlphaTest)
        .expect_err("alpha-test shadow layout should require uv0");

    assert_eq!(
        error,
        MaterialError::MissingVertexAttribute {
            semantic: VertexSemantic::UV0
        }
    );
}

#[test]
fn alpha_test_shadow_pipeline_compiles_with_standard_material_layout() {
    let (device, _queue) = create_test_device();
    let shadow_layout = ShadowPassBindingLayout::new(&device);
    let material_layout =
        <StandardMaterial as crate::render::resources::material::MaterialModel>::interface()
            .bindings
            .create_bind_group_layout(&device, "standard_material_shadow_test_bgl");
    let layout = VertexLayout::new(
        20,
        [
            VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
            VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
        ],
    );
    let mut phase = DirectionalShadowPhase::new();

    let pipeline = phase.pipeline_for(
        &device,
        shadow_layout.bind_group_layout(),
        Some(&material_layout),
        &layout,
        ShadowRasterBias::new(0, 0.0, 0.0),
        ShadowPipelineKind::AlphaTest,
    );

    assert!(pipeline.is_ok());
}

#[test]
fn transparent_shadow_pipeline_compiles_with_standard_material_layout() {
    let (device, _queue) = create_test_device();
    let shadow_layout = ShadowPassBindingLayout::new(&device);
    let material_layout =
        <StandardMaterial as crate::render::resources::material::MaterialModel>::interface()
            .bindings
            .create_bind_group_layout(&device, "standard_material_shadow_test_bgl");
    let layout = VertexLayout::new(
        20,
        [
            VertexAttribute::new(VertexSemantic::Position, wgpu::VertexFormat::Float32x3, 0),
            VertexAttribute::new(VertexSemantic::UV0, wgpu::VertexFormat::Float32x2, 12),
        ],
    );
    let mut phase = DirectionalShadowPhase::new();

    let pipeline = phase.pipeline_for(
        &device,
        shadow_layout.bind_group_layout(),
        Some(&material_layout),
        &layout,
        ShadowRasterBias::new(0, 0.0, 0.0),
        ShadowPipelineKind::Transparent,
    );

    assert!(pipeline.is_ok());
}

#[test]
fn transparent_shadow_blend_state_matches_wicked() {
    let blend = TRANSPARENT_SHADOW_BLEND_STATE;

    assert_eq!(blend.color.src_factor, wgpu::BlendFactor::Zero);
    assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::Src);
    assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
    assert_eq!(blend.alpha.src_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.alpha.dst_factor, wgpu::BlendFactor::One);
    assert_eq!(blend.alpha.operation, wgpu::BlendOperation::Max);
}

#[test]
fn shadow_depth_shaders_keep_raster_and_depth_projection_separate() {
    for source in [
        include_str!("../../../../../shaders/lighting/shadow_depth.wgsl"),
        include_str!("../../../../../shaders/lighting/shadow_depth_alpha_test.wgsl"),
        include_str!("../../../../../shaders/lighting/shadow_transparent.wgsl"),
    ] {
        assert!(source.contains("raster_view_proj: mat4x4<f32>"));
        assert!(source.contains("depth_view_proj: mat4x4<f32>"));
        assert!(source.contains("shadow_pass.raster_view_proj * world_position"));
        assert!(source.contains("shadow_pass.depth_view_proj * world_position"));
        assert!(
            !source.contains("clip_position.z = clamp"),
            "vertex-stage depth clamping bends shadow triangles and can stamp bands into the atlas"
        );
    }

    let opaque = include_str!("../../../../../shaders/lighting/shadow_depth.wgsl");
    let alpha_test = include_str!("../../../../../shaders/lighting/shadow_depth_alpha_test.wgsl");
    let transparent = include_str!("../../../../../shaders/lighting/shadow_transparent.wgsl");
    assert!(opaque.contains("@builtin(frag_depth)"));
    assert!(alpha_test.contains("@builtin(frag_depth)"));
    assert!(transparent.contains("@builtin(frag_depth)"));
}

#[test]
fn standard_mask_material_routes_to_alpha_test_shadow_caster() {
    let (device, _queue) = create_test_device();
    let mut draw_functions = DrawFunctionRegistry::new();
    let draw_mesh =
        draw_functions.register(crate::render::phase::DrawMesh::<StandardMaterial>::new());
    let mut material_registry = MaterialRegistry::new();
    material_registry
        .register_model::<StandardMaterial>(&device)
        .expect("standard material should register");
    let opaque = material_registry
        .insert_material::<StandardMaterial>(StandardMaterial::default())
        .expect("opaque material should insert");
    let mask = material_registry
        .insert_material::<StandardMaterial>(StandardMaterial::default().alpha_mask(0.35))
        .expect("mask material should insert");
    let opaque_item = PhaseItem::new(
        0,
        draw_mesh,
        EntityId::new(0, 0),
        0,
        MeshDrawData::new(crate::render::expert::resources::Mesh::QUAD, opaque, 0),
    );
    let mask_item = PhaseItem::new(
        0,
        draw_mesh,
        EntityId::new(1, 0),
        0,
        MeshDrawData::new(crate::render::expert::resources::Mesh::QUAD, mask, 0),
    );

    assert_eq!(
        shadow_caster_kind(&opaque_item, &draw_functions, &material_registry),
        ShadowCasterKind::Opaque
    );
    assert_eq!(
        shadow_caster_kind(&mask_item, &draw_functions, &material_registry),
        ShadowCasterKind::AlphaTest(mask.into())
    );
}

#[test]
fn standard_blend_material_routes_to_transparent_shadow_caster() {
    let (device, _queue) = create_test_device();
    let mut draw_functions = DrawFunctionRegistry::new();
    let draw_mesh =
        draw_functions.register(crate::render::phase::DrawMesh::<StandardMaterial>::new());
    let mut material_registry = MaterialRegistry::new();
    material_registry
        .register_model::<StandardMaterial>(&device)
        .expect("standard material should register");
    let blend = material_registry
        .insert_material::<StandardMaterial>(
            StandardMaterial::default().alpha_mode(crate::render::AlphaMode::Blend),
        )
        .expect("blend material should insert");
    let item = PhaseItem::new(
        0,
        draw_mesh,
        EntityId::new(0, 0),
        0,
        MeshDrawData::new(crate::render::expert::resources::Mesh::QUAD, blend, 0),
    );

    assert_eq!(
        transparent_shadow_material_handle(&item, &draw_functions, &material_registry),
        Some(blend.into())
    );
}

#[test]
fn transparent_shadow_candidates_ignore_sprite_payloads() {
    let mut draw_functions = DrawFunctionRegistry::new();
    let draw_sprite = draw_functions.register(DrawSprite::new());
    let material = MaterialHandle::new::<crate::render::SpriteMaterial>(0, 0);
    let item = PhaseItem::new(
        0,
        draw_sprite,
        EntityId::new(0, 0),
        0,
        SpriteDrawData::new(material, [16.0, 16.0], [1.0; 4], [0.0, 0.0, 1.0, 1.0]),
    );

    assert!(!has_transparent_mesh_shadow_candidates(
        std::slice::from_ref(&item),
        &draw_functions
    ));
}

#[test]
fn directional_shadow_phase_setup_imports_enabled_shadow_view() {
    let (device, queue) = create_test_device();
    let gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let projection = Projection::orthographic_fixed(16.0, 16.0);
    let main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([64, 64]),
        [64, 64],
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [64, 64]),
        true,
    );
    let mut world = World::new();
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
        .shadow_map_size(64)
        .shadow_bias(0.002)
        .radius(0.04)
        .shadow_depth_bias(5)
        .shadow_slope_bias(1.25)
        .shadow_normal_bias(0.03)
        .shadow_filter_radius(0.06),));

    let mut scene_views = vec![main_view];
    let shadow_setups = append_directional_shadow_views(&world, &mut scene_views);
    assert_eq!(scene_views.len(), 2);
    assert!(scene_views[1].is_shadow());

    let mut opaque_phases = vec![OpaquePhase::new(), OpaquePhase::new()];
    opaque_phases[1].add_item(PhaseItem::new(
        0,
        DrawFunctionId::from_raw(0),
        EntityId::new(0, 0),
        0,
        MeshDrawData::new(
            crate::render::expert::resources::Mesh::QUAD,
            MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
            0,
        ),
    ));
    let transparent_phases = vec![TransparentPhase::new(), TransparentPhase::new()];

    let mut gpu_scene = GpuScene::new(&gpu);
    gpu_scene.table_mut::<LightTable>().set_all(&gpu, &[]);
    gpu_scene.upload_all(gpu.queue());
    let scene_layout = ShadowSceneBindingLayout::new(gpu.device());
    let pass_layout = ShadowPassBindingLayout::new(gpu.device());
    let sampler = create_shadow_compare_sampler(gpu.device());
    let mut shadow_bindings = Vec::new();
    sync_shadow_views(
        &mut shadow_bindings,
        &gpu,
        &scene_views,
        &opaque_phases,
        &transparent_phases,
        &[IDENTITY_MATRIX],
        &shadow_setups,
        &scene_layout,
        &pass_layout,
        &sampler,
        gpu_scene.table::<LightTable>(),
        crate::render::RenderDebugView::DirectionalShadowCoverage,
    );
    assert_eq!(shadow_bindings.len(), 1);
    assert!(shadow_bindings[0].enabled());
    assert_eq!(shadow_bindings[0].radius(), 0.06);
    assert_eq!(
        shadow_bindings[0].raster_bias(),
        ShadowRasterBias::new(5, 1.25, 0.0)
    );
    assert_eq!(shadow_bindings[0].normal_bias(), 0.03);
    assert_eq!(shadow_bindings[0].debug_mode(), 1.0);

    let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
    frame.insert_payload(&scene_layout);
    let prepared_view = PreparedView::new(
        scene_views[1].order,
        scene_views[1].viewport,
        scene_views[1].target_size,
        scene_views[1].clear_surface,
    )
    .with_payload(&scene_views[1])
    .with_payload(&shadow_bindings[0]);
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(frame.surface_format(), frame.has_surface());
    let mut phase = DirectionalShadowPhase::new();

    assert!(phase.is_enabled(&frame, &prepared_view));
    {
        let mut setup = PhaseSetupContext::new(&mut graph, &mut state, &frame, &prepared_view);
        phase.setup(&mut setup);
    }

    let slot = state
        .texture_slot("directional_shadow_atlas_0")
        .expect("enabled shadow setup should publish the imported depth atlas");
    assert_eq!(slot.format(), DEFAULT_DEPTH_FORMAT);
    let transparent_slot = state
        .texture_slot("directional_transparent_shadow_atlas_0")
        .expect("enabled shadow setup should publish the imported transparent atlas");
    assert_eq!(transparent_slot.format(), TRANSPARENT_SHADOW_FORMAT);
    let graph_resources_key = SceneShadowGraphResources::blackboard_key(0);
    let graph_resources = graph
        .blackboard_ref()
        .get::<SceneShadowGraphResources>(&graph_resources_key)
        .expect("enabled shadow setup should publish graph handles for sampling passes");
    assert_eq!(graph_resources.directional_shadow_atlas(), slot.handle());
    assert_eq!(
        graph_resources.directional_transparent_shadow_atlas(),
        transparent_slot.handle()
    );
    let scene_shadows = state
        .scene_shadows()
        .expect("enabled shadow setup should publish scene shadow resources");
    assert_eq!(
        scene_shadows.kind(),
        ShadowResourceKind::DirectionalCascades
    );
    assert!(scene_shadows.enabled());
    assert!(scene_shadows.bind_group().is_some());
    assert_eq!(graph.pass_count(), 2);
    let passes = graph.compile().expect("shadow phase graph should compile");
    let depth_pass = passes
        .iter()
        .find(|pass| pass.color_outputs.is_empty())
        .expect("shadow phase should declare a depth-only pass");
    let transparent_pass = passes
        .iter()
        .find(|pass| !pass.color_outputs.is_empty())
        .expect("shadow phase should declare a transparent color pass");
    let depth = depth_pass
        .depth_stencil
        .expect("shadow pass should declare depth");
    assert!(
        depth.clear_depth.is_none(),
        "directional shadow cascades clear their own atlas rects while preserving the rest"
    );
    assert_eq!(transparent_pass.color_outputs.len(), 1);
    assert!(
        transparent_pass.depth_stencil.is_some(),
        "transparent shadow pass should depth-test against the directional atlas"
    );
}

#[test]
fn opaque_phase_reads_exact_shadow_graph_handles() {
    let (device, queue) = create_test_device();
    let gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let projection = Projection::orthographic_fixed(16.0, 16.0);
    let main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([64, 64]),
        [64, 64],
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [64, 64]),
        true,
    );
    let mut world = World::new();
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]).shadow_map_size(64),));

    let mut scene_views = vec![main_view];
    let shadow_setups = append_directional_shadow_views(&world, &mut scene_views);
    assert_eq!(scene_views.len(), 2);
    assert_eq!(scene_views[0].shadow_binding(), Some(0));
    assert!(scene_views[1].is_shadow());

    let mut opaque_phases = vec![OpaquePhase::new(), OpaquePhase::new()];
    opaque_phases[0].add_item(PhaseItem::new(
        0,
        DrawFunctionId::from_raw(0),
        EntityId::new(0, 0),
        0,
        MeshDrawData::new(
            crate::render::expert::resources::Mesh::QUAD,
            MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
            0,
        ),
    ));
    opaque_phases[1].add_item(PhaseItem::new(
        0,
        DrawFunctionId::from_raw(0),
        EntityId::new(1, 0),
        0,
        MeshDrawData::new(
            crate::render::expert::resources::Mesh::QUAD,
            MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
            0,
        ),
    ));
    let transparent_phases = vec![TransparentPhase::new(), TransparentPhase::new()];

    let mut gpu_scene = GpuScene::new(&gpu);
    gpu_scene.table_mut::<LightTable>().set_all(&gpu, &[]);
    gpu_scene.upload_all(gpu.queue());
    let scene_layout = ShadowSceneBindingLayout::new(gpu.device());
    let pass_layout = ShadowPassBindingLayout::new(gpu.device());
    let sampler = create_shadow_compare_sampler(gpu.device());
    let mut shadow_bindings = Vec::new();
    sync_shadow_views(
        &mut shadow_bindings,
        &gpu,
        &scene_views,
        &opaque_phases,
        &transparent_phases,
        &[IDENTITY_MATRIX],
        &shadow_setups,
        &scene_layout,
        &pass_layout,
        &sampler,
        gpu_scene.table::<LightTable>(),
        crate::render::RenderDebugView::None,
    );
    assert_eq!(shadow_bindings.len(), 1);
    assert!(shadow_bindings[0].enabled());

    let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
    frame.insert_payload(&scene_layout);
    let shadow_prepared_view = PreparedView::new(
        scene_views[1].order,
        scene_views[1].viewport,
        scene_views[1].target_size,
        scene_views[1].clear_surface,
    )
    .with_payload(&scene_views[1])
    .with_payload(&shadow_bindings[0]);
    let main_prepared_view = PreparedView::new(
        scene_views[0].order,
        scene_views[0].viewport,
        scene_views[0].target_size,
        scene_views[0].clear_surface,
    )
    .with_payload(&scene_views[0])
    .with_payload(&opaque_phases[0]);
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(frame.surface_format(), frame.has_surface());
    let mut shadow_phase = DirectionalShadowPhase::new();

    {
        let mut setup =
            PhaseSetupContext::new(&mut graph, &mut state, &frame, &shadow_prepared_view);
        shadow_phase.setup(&mut setup);
    }

    let graph_resources_key = SceneShadowGraphResources::blackboard_key(0);
    let graph_resources = graph
        .blackboard_ref()
        .get::<SceneShadowGraphResources>(&graph_resources_key)
        .cloned()
        .expect("shadow setup should publish graph handles before opaque setup");
    let scene_color = graph.create_texture(|builder| {
        builder
            .name("opaque_phase_shadow_dependency_color")
            .size(TargetSize::Exact(64, 64))
            .format(frame.surface_format())
            .persistent();
    });
    state.set_current_color(scene_color, frame.surface_format());

    let mut opaque_phase = OpaquePhase::new();
    {
        let mut setup = PhaseSetupContext::new(&mut graph, &mut state, &frame, &main_prepared_view);
        opaque_phase.setup(&mut setup);
    }

    let passes = graph
        .compile()
        .expect("shadow and opaque dependency graph should compile");
    let opaque_pass = passes
        .iter()
        .find(|pass| pass.name.as_ref() == "opaque_phase")
        .expect("opaque setup should declare a live opaque pass");
    assert!(
        opaque_pass.reads.contains(&ResourceRef::Texture(
            graph_resources.directional_shadow_atlas()
        )),
        "opaque pass must read the exact directional shadow atlas handle from shadow setup"
    );
    assert!(
        opaque_pass.reads.contains(&ResourceRef::Texture(
            graph_resources.directional_transparent_shadow_atlas()
        )),
        "opaque pass must read the exact transparent shadow atlas handle from shadow setup"
    );
}

#[test]
fn static_shadow_phase_setup_skips_clean_cascade_but_keeps_resource() {
    let (device, queue) = create_test_device();
    let gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 64.0);
    let main_view = SceneView::new(
        0,
        ViewportRect::from_surface_size([64, 64]),
        [64, 64],
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [64, 64]),
        false,
    );
    let mut world = World::new();
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
        .cascade_count(2)
        .cascade_distances([16.0, 48.0, 0.0, 0.0])
        .shadow_map_size(64)
        .static_shadows_when_unchanged(),));

    let mut scene_views = vec![main_view];
    let shadow_setups = append_directional_shadow_views(&world, &mut scene_views);
    assert_eq!(scene_views.len(), 3);

    let mut opaque_phases = (0..scene_views.len())
        .map(|_| OpaquePhase::new())
        .collect::<Vec<_>>();
    let transparent_phases = (0..scene_views.len())
        .map(|_| TransparentPhase::new())
        .collect::<Vec<_>>();
    for phase in opaque_phases.iter_mut().skip(1) {
        phase.add_item(PhaseItem::new(
            0,
            DrawFunctionId::from_raw(0),
            EntityId::new(0, 0),
            0,
            MeshDrawData::new(
                crate::render::expert::resources::Mesh::QUAD,
                MaterialHandle::new::<crate::render::StandardMaterial>(0, 0),
                0,
            ),
        ));
    }

    let mut gpu_scene = GpuScene::new(&gpu);
    gpu_scene.table_mut::<LightTable>().set_all(&gpu, &[]);
    gpu_scene.upload_all(gpu.queue());
    let scene_layout = ShadowSceneBindingLayout::new(gpu.device());
    let pass_layout = ShadowPassBindingLayout::new(gpu.device());
    let sampler = create_shadow_compare_sampler(gpu.device());
    let mut shadow_bindings = Vec::new();
    sync_shadow_views(
        &mut shadow_bindings,
        &gpu,
        &scene_views,
        &opaque_phases,
        &transparent_phases,
        &[IDENTITY_MATRIX],
        &shadow_setups,
        &scene_layout,
        &pass_layout,
        &sampler,
        gpu_scene.table::<LightTable>(),
        crate::render::RenderDebugView::None,
    );
    assert!(shadow_bindings[0].should_update_cascade(0));
    assert!(shadow_bindings[0].should_update_cascade(1));

    sync_shadow_views(
        &mut shadow_bindings,
        &gpu,
        &scene_views,
        &opaque_phases,
        &transparent_phases,
        &[IDENTITY_MATRIX],
        &shadow_setups,
        &scene_layout,
        &pass_layout,
        &sampler,
        gpu_scene.table::<LightTable>(),
        crate::render::RenderDebugView::None,
    );
    assert!(!shadow_bindings[0].should_update_cascade(0));
    assert!(!shadow_bindings[0].should_update_cascade(1));

    let mut frame = PreparedFrame::new(wgpu::TextureFormat::Bgra8Unorm, false);
    frame.insert_payload(&scene_layout);
    let prepared_view = PreparedView::new(
        scene_views[1].order,
        scene_views[1].viewport,
        scene_views[1].target_size,
        scene_views[1].clear_surface,
    )
    .with_payload(&scene_views[1])
    .with_payload(&shadow_bindings[0]);
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(frame.surface_format(), frame.has_surface());
    let mut phase = DirectionalShadowPhase::new();

    assert!(phase.is_enabled(&frame, &prepared_view));
    {
        let mut setup = PhaseSetupContext::new(&mut graph, &mut state, &frame, &prepared_view);
        phase.setup(&mut setup);
    }

    assert_eq!(graph.pass_count(), 0);
    assert!(
        state.texture_slot("directional_shadow_atlas_0").is_some(),
        "clean static cascades still publish the reusable imported atlas"
    );
    assert!(
        state
            .texture_slot("directional_transparent_shadow_atlas_0")
            .is_some(),
        "clean static cascades still publish the reusable transparent shadow atlas"
    );
    assert!(state
        .scene_shadows()
        .expect("clean static cascades should still publish shadow resources")
        .enabled());
}
