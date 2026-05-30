use super::*;
use crate::render::execution::TextureFormat;
use crate::render::graph::{PassFlags, PassHandle, PassType, ResourceRef};
use crate::render::pipeline::GraphPass;
use rustc_hash::FxHashMap;
use std::borrow::Cow;

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for render tests");

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("compute_context_test_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("Failed to create test GPU device")
}

fn completed_view_state(
    view: &PreparedView<'_>,
    scene_shadows: Option<SceneShadowResources>,
) -> CompletedViewState {
    CompletedViewState::new(
        0,
        view,
        Default::default(),
        Default::default(),
        scene_shadows,
    )
}

#[test]
fn compute_context_resource_helpers_resolve_declared_resources() {
    let (device, queue) = create_test_device();
    let mut gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [4, 4]);
    let read_texture = TextureHandle(0, 7);
    let write_texture = TextureHandle(1, 7);
    let read_subresource = TextureSubresource::new(TextureHandle(2, 7), 1, 1, 3, 1);
    let write_subresource = TextureSubresource::new(TextureHandle(3, 7), 2, 1, 4, 1);
    let pass = CompiledPass {
        handle: PassHandle(0, 7),
        index: 0,
        name: Cow::Borrowed("compute"),
        pass_type: PassType::Compute,
        reads: vec![
            ResourceRef::Texture(read_texture),
            ResourceRef::TextureSubresource(read_subresource),
        ],
        writes: vec![
            ResourceRef::Texture(write_texture),
            ResourceRef::TextureSubresource(write_subresource),
        ],
        color_outputs: Vec::new(),
        depth_stencil: None,
        copy_ops: Vec::new(),
        flags: PassFlags::empty(),
        dep_level: 0,
    };
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [4, 4], false);
    let view_state = completed_view_state(&view, None);
    let execution = ViewExecutionContext {
        frame: &frame,
        view: &view,
        view_state: &view_state,
        view_index: 0,
    };
    let textures = [];
    let buffers = [];
    let texture_descs = [];
    let buffer_descs = [];
    let alias_redirects = FxHashMap::default();
    let blackboard = Blackboard::new();
    let resources = PhysicalResources {
        handle_token: 7,
        textures: &textures,
        buffers: &buffers,
        texture_descs: &texture_descs,
        buffer_descs: &buffer_descs,
        alias_redirects: &alias_redirects,
        blackboard: &blackboard,
        view_stats: None,
    };

    let ctx = ComputePassExecuteContext::new(&mut gpu, &pass, &resources, &execution);

    assert_eq!(ctx.read_texture(0), read_texture);
    assert_eq!(ctx.write_texture(0), write_texture);
    assert_eq!(ctx.read_subresource(0), read_subresource);
    assert_eq!(ctx.write_subresource(0), write_subresource);
}

#[test]
fn custom_pass_can_require_scene_depth() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [8, 8], false);
    let depth = graph.create_texture(|builder| {
        builder
            .name("test_scene_depth")
            .size(crate::render::graph::TargetSize::Exact(8, 8))
            .format(TextureFormat::Depth32Float);
    });
    let _ = state.set_scene_texture(SceneTexture::Depth, depth, TextureFormat::Depth32Float);

    let ctx = PhaseSetupContext::new(&mut graph, &mut state, &frame, &view);

    assert_eq!(
        ctx.require_scene_texture(SceneTexture::Depth).handle(),
        depth
    );
    assert_eq!(ctx.optional_scene_texture(SceneTexture::Normal), None);
}

#[test]
fn custom_pass_can_publish_indirect_diffuse() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [16, 12], false);
    let mut ctx = ComputePassSetupContext::new(&mut graph, &mut state, &frame, &view);

    let indirect =
        ctx.ensure_scene_texture(SceneTexture::IndirectDiffuse, TextureFormat::Rgba16Float);
    let reused =
        ctx.ensure_scene_texture(SceneTexture::IndirectDiffuse, TextureFormat::Rgba16Float);

    assert_eq!(indirect, reused);
    assert_eq!(
        ctx.require_scene_texture(SceneTexture::IndirectDiffuse),
        indirect
    );
    assert_eq!(
        ctx.state()
            .texture_slot(SceneTexture::IndirectDiffuse.debug_name()),
        None
    );
}

#[test]
fn custom_pass_can_publish_disabled_scene_shadows() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [8, 8], false);
    let mut ctx = ComputePassSetupContext::new(&mut graph, &mut state, &frame, &view);

    assert!(ctx.optional_scene_shadows().is_none());
    let previous = ctx.publish_scene_shadows(SceneShadowResources::disabled(
        crate::render::lighting::ShadowResourceKind::DirectionalCascades,
    ));

    assert!(previous.is_none());
    let shadows = ctx.require_scene_shadows();
    assert_eq!(
        shadows.kind(),
        crate::render::lighting::ShadowResourceKind::DirectionalCascades
    );
    assert!(!shadows.enabled());
    assert!(shadows.bind_group().is_none());
    assert!(shadows.bind_group_layout().is_none());
    assert!(state.scene_shadows().is_some());
}

#[test]
fn custom_shadow_resources_flow_to_execute_context() {
    let (device, queue) = create_test_device();
    let mut gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
    let layout = gpu
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("custom_shadow_bgl"),
            entries: &[],
        });
    let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("custom_shadow_bg"),
        layout: &layout,
        entries: &[],
    });
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [8, 8], false);
    {
        let mut setup = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);
        let previous = setup.publish_scene_shadows(SceneShadowResources::from_bind_group(
            crate::render::lighting::ShadowResourceKind::DirectionalCascades,
            &layout,
            &bind_group,
        ));
        assert!(previous.is_none());
    }
    let view_state = completed_view_state(&view, state.scene_shadows().cloned());
    let pass = CompiledPass {
        handle: PassHandle(0, 7),
        index: 0,
        name: Cow::Borrowed("custom_shadow_consumer"),
        pass_type: PassType::Render,
        reads: Vec::new(),
        writes: Vec::new(),
        color_outputs: Vec::new(),
        depth_stencil: None,
        copy_ops: Vec::new(),
        flags: PassFlags::empty(),
        dep_level: 0,
    };
    let execution = ViewExecutionContext {
        frame: &frame,
        view: &view,
        view_state: &view_state,
        view_index: 0,
    };
    let textures = [];
    let buffers = [];
    let texture_descs = [];
    let buffer_descs = [];
    let alias_redirects = FxHashMap::default();
    let resources = PhysicalResources {
        handle_token: 7,
        textures: &textures,
        buffers: &buffers,
        texture_descs: &texture_descs,
        buffer_descs: &buffer_descs,
        alias_redirects: &alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let consumer = GraphPassExecuteContext::new(&mut gpu, &pass, &resources, &execution);

    let shadows = consumer.require_scene_shadows();
    assert_eq!(
        shadows.kind(),
        crate::render::lighting::ShadowResourceKind::DirectionalCascades
    );
    assert!(shadows.enabled());
    assert_eq!(shadows.bind_group_layout(), Some(&layout));
    assert_eq!(shadows.bind_group(), Some(&bind_group));
}

#[test]
fn scene_texture_requests_do_not_use_generic_slot_map() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [4, 4], false);
    let light = graph.create_texture(|builder| {
        builder
            .name("test_scene_light")
            .size(crate::render::graph::TargetSize::Exact(4, 4))
            .format(TextureFormat::Rgba16Float);
    });
    let mut ctx = PostFxPassSetupContext::new(&mut graph, &mut state, &frame, &view);

    let slot = ctx.set_scene_texture(SceneTexture::Light, light, TextureFormat::Rgba16Float);

    assert_eq!(ctx.optional_scene_texture(SceneTexture::Light), Some(slot));
    assert_eq!(
        ctx.state().texture_slot(SceneTexture::Light.debug_name()),
        None
    );
}

#[test]
fn setup_context_can_request_history_texture() {
    let (device, queue) = create_test_device();
    let gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
    let history_store = HistoryTextureStore::new();
    history_store.begin_frame(&gpu);
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let mut frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let _ = frame.insert_payload(&history_store);
    let view = PreparedView::new(0, Default::default(), [32, 24], false).with_history_key(5);
    let mut ctx = PhaseSetupContext::new(&mut graph, &mut state, &frame, &view);

    let history = ctx
        .history_texture("taa_color")
        .format(TextureFormat::Rgba16Float)
        .half_res()
        .ping_pong()
        .get();

    assert_eq!(history.size(), [16, 12]);
    assert_eq!(history.format(), TextureFormat::Rgba16Float);
    assert!(history.reset());
    assert!(history.read().is_none());
    assert!(graph.get_texture("history_5_taa_color_write").is_some());
}

struct DepthPublishingGraphPass;

impl GraphPass for DepthPublishingGraphPass {
    fn name(&self) -> &'static str {
        "depth_publishing_graph_pass"
    }

    fn setup(&mut self, ctx: &mut GraphPassSetupContext<'_, '_>) {
        let depth = ctx.require_scene_texture(SceneTexture::Depth);
        let target_size = ctx.view().target_size();
        let target = ctx.graph().create_texture(|builder| {
            builder
                .name("custom_indirect_diffuse")
                .size(TargetSize::Exact(target_size[0], target_size[1]))
                .format(TextureFormat::Rgba16Float)
                .persistent();
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(depth.handle());
            setup.write_color(0, target);
        });
        let _ = ctx.set_scene_texture(
            SceneTexture::IndirectDiffuse,
            target,
            TextureFormat::Rgba16Float,
        );
    }
}

#[test]
fn custom_graph_pass_can_read_scene_depth_and_write_texture() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [8, 8], false);
    let depth = graph.create_texture(|builder| {
        builder
            .name("scene_depth_for_graph_pass")
            .size(TargetSize::Exact(8, 8))
            .format(TextureFormat::Depth32Float);
    });
    graph.add_render_pass("scene_depth_seed_for_graph_pass", |setup| {
        setup.set_depth_stencil(depth);
    });
    let _ = state.set_scene_texture(SceneTexture::Depth, depth, TextureFormat::Depth32Float);

    let mut pass = DepthPublishingGraphPass;
    let mut ctx = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);
    pass.setup(&mut ctx);

    let indirect = state
        .scene_texture(SceneTexture::IndirectDiffuse)
        .expect("graph pass should publish indirect diffuse");
    assert_eq!(indirect.format(), TextureFormat::Rgba16Float);
    let compiled = graph.compile().expect("graph pass setup should compile");
    let compiled_pass = compiled
        .iter()
        .find(|compiled_pass| compiled_pass.name == pass.name())
        .expect("graph pass should declare a render-graph pass");
    assert!(compiled_pass.reads.contains(&ResourceRef::Texture(depth)));
    assert!(compiled_pass
        .writes
        .contains(&ResourceRef::Texture(indirect.handle())));
}

#[test]
fn graph_pass_setup_can_create_texture_from_spec() {
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [64, 32], false);
    let mut ctx = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);

    let slot = ctx.create_texture(
        TextureSpec::rgba16f("graph_pass_spec_target")
            .half_res()
            .storage()
            .mips(2),
    );

    assert_eq!(slot.format(), TextureFormat::Rgba16Float);
    assert_eq!(
        ctx.graph().get_texture("graph_pass_spec_target"),
        Some(slot.handle())
    );
}

#[test]
fn graph_pass_contexts_share_blackboard_values() {
    let (device, queue) = create_test_device();
    let mut gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [8, 8], false);
    let published = {
        let mut setup = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);
        let slot = setup.create_texture(TextureSpec::rgba16f("blackboard_target").persistent());
        setup.blackboard_set("custom_target", slot.handle());
        slot.handle()
    };
    let pass = CompiledPass {
        handle: PassHandle(0, 7),
        index: 0,
        name: Cow::Borrowed("blackboard_execute"),
        pass_type: PassType::Render,
        reads: Vec::new(),
        writes: Vec::new(),
        color_outputs: Vec::new(),
        depth_stencil: None,
        copy_ops: Vec::new(),
        flags: PassFlags::empty(),
        dep_level: 0,
    };
    let view_state = completed_view_state(&view, state.scene_shadows().cloned());
    let execution = ViewExecutionContext {
        frame: &frame,
        view: &view,
        view_state: &view_state,
        view_index: 0,
    };
    let textures = [];
    let buffers = [];
    let texture_descs = [];
    let buffer_descs = [];
    let alias_redirects = FxHashMap::default();
    let resources = PhysicalResources {
        handle_token: 7,
        textures: &textures,
        buffers: &buffers,
        texture_descs: &texture_descs,
        buffer_descs: &buffer_descs,
        alias_redirects: &alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let ctx = GraphPassExecuteContext::new(&mut gpu, &pass, &resources, &execution);

    assert_eq!(
        ctx.blackboard_get::<TextureHandle>("custom_target"),
        Some(&published)
    );
}

#[test]
fn postfx_and_finalize_contexts_share_blackboard_values() {
    let (device, queue) = create_test_device();
    let mut gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let view = PreparedView::new(0, Default::default(), [8, 8], false);

    {
        let mut setup = PostFxPassSetupContext::new(&mut graph, &mut state, &frame, &view);
        setup.blackboard_set("postfx_value", 41u32);
        *setup
            .blackboard_get_mut::<u32>("postfx_value")
            .expect("postfx setup should retrieve blackboard value") += 1;
    }

    let pass = CompiledPass {
        handle: PassHandle(0, 7),
        index: 0,
        name: Cow::Borrowed("blackboard_postfx_execute"),
        pass_type: PassType::Render,
        reads: Vec::new(),
        writes: Vec::new(),
        color_outputs: Vec::new(),
        depth_stencil: None,
        copy_ops: Vec::new(),
        flags: PassFlags::empty(),
        dep_level: 0,
    };
    let view_state = completed_view_state(&view, state.scene_shadows().cloned());
    let execution = ViewExecutionContext {
        frame: &frame,
        view: &view,
        view_state: &view_state,
        view_index: 0,
    };
    let textures = [];
    let buffers = [];
    let texture_descs = [];
    let buffer_descs = [];
    let alias_redirects = FxHashMap::default();
    let resources = PhysicalResources {
        handle_token: 7,
        textures: &textures,
        buffers: &buffers,
        texture_descs: &texture_descs,
        buffer_descs: &buffer_descs,
        alias_redirects: &alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let postfx_execute = PostFxPassExecuteContext::new(&mut gpu, &pass, &resources, &execution);
    assert_eq!(
        postfx_execute.blackboard_get::<u32>("postfx_value"),
        Some(&42)
    );

    let mut frame_slots = state.into_slots();
    let mut scene_gbuffer = Default::default();
    let completed_views = [];
    {
        let mut finalize_state = FinalizePhaseState::new(
            TextureFormat::Bgra8Unorm,
            false,
            &mut frame_slots,
            &mut scene_gbuffer,
            &completed_views,
        );
        let mut setup = RenderPassSetupContext::new(&mut graph, &mut finalize_state, &frame);
        setup.blackboard_set("finalize_value", 7u32);
    }
    let finalize_execution = FinalizeExecutionContext {
        frame: &frame,
        completed_views: &completed_views,
    };
    let resources = PhysicalResources {
        handle_token: 7,
        textures: &textures,
        buffers: &buffers,
        texture_descs: &texture_descs,
        buffer_descs: &buffer_descs,
        alias_redirects: &alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let finalize_execute =
        RenderPassExecuteContext::new(&mut gpu, &pass, &resources, &finalize_execution);
    assert_eq!(
        finalize_execute.blackboard_get::<u32>("finalize_value"),
        Some(&7)
    );
}

#[test]
fn graph_pass_setup_exposes_scene_lighting_resources() {
    let (device, queue) = create_test_device();
    let gpu = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [8, 8]);
    let mut gpu_scene = GpuScene::new(&gpu);
    gpu_scene.table_mut::<LightTable>().set_all(
        &gpu,
        &[crate::render::GpuLight {
            pos_radius: [1.0, 2.0, 3.0, 4.0],
            color: [0.8, 0.7, 0.6, 1.0],
            falloff: [1.0, 0.0, 0.0, 0.0],
            dir_shadow: [0.0, 0.0, 0.0, -1.0],
        }],
    );
    let settings = RenderSettings {
        ambient_color: crate::render::Color::new(0.2, 0.3, 0.4, 1.0),
        ..Default::default()
    };
    let mut graph = RenderGraph::new();
    let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
    let mut frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
    let _ = frame.insert_payload(&settings);
    let _ = frame.insert_payload(&gpu_scene);
    let view = PreparedView::new(0, Default::default(), [8, 8], false);
    let ctx = GraphPassSetupContext::new(&mut graph, &mut state, &frame, &view);

    let lighting = ctx
        .scene_lighting()
        .expect("GpuScene payload should expose scene lighting");

    assert_eq!(lighting.ambient_color(), [0.2, 0.3, 0.4, 1.0]);
    assert_eq!(lighting.light_count(), 1);
    assert_eq!(lighting.point_light_count(), 1);
    assert_eq!(lighting.directional_light_count(), 0);
    assert!(lighting.scene_bind_group().is_none());
    assert!(std::ptr::eq(
        lighting.light_bind_group_layout(),
        gpu_scene.table::<LightTable>().bind_group_layout()
    ));
}
