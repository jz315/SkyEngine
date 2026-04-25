use crate::asset::{AssetConfig, AssetId, AssetServer, Handle, TextureAsset};
use crate::diagnostics::{
    DiagnosticSeverity, DiagnosticSubsystem, Diagnostics, EngineDiagnosticKind,
};
#[cfg(feature = "live2d")]
use crate::ecs::EntityId;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::expert::{Mesh, MeshDescriptor, MeshIndexData, RenderGraphError, TargetSize};
use crate::render::pipeline::{
    ComputePass, PostFxPass, RenderPass, RenderPhase, RenderPhaseExecuteContext,
    RenderPhaseSetupContext,
};
use crate::render::view::Projection;
use crate::render::{
    CameraMarker, CameraViewport, Color, ComputePassExecuteContext, ComputePassSetupContext,
    DirectionalLight, MainCamera, MeshRenderer, PostFxPassExecuteContext, PostFxPassSetupContext,
    RenderComposer, RenderPassExecuteContext, RenderPassSetupContext, RenderPipelineAsset,
    RenderPipelineBuilder, RenderSettings, StandardMaterial, Transform, UnlitMaterial,
    ViewportRect,
};
#[cfg(feature = "live2d")]
use crate::render::{OrderInLayer, RenderQueueSort, SceneView, SortingLayer};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[cfg(feature = "live2d")]
use crate::render::live2d::{
    live2d_instance_visible_in_view, sort_live2d_scene_instances, Live2DSceneInstance,
};

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for composer tests");

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("composer_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("Failed to create test GPU device")
}

#[test]
fn builder_unlit_pipeline_renders_default_sprite_scene() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert!(stats.passes >= 1);
    assert!(stats.step_count >= 1);
}

#[test]
fn sprite_texture_asset_handle_uploads_into_render_cache() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );
    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let texture = asset_server.insert_runtime(TextureAsset::checkerboard(
        2,
        1,
        [255, 255, 255, 255],
        [32, 32, 32, 255],
    ));
    let mut world = World::new();
    world.insert_resource(asset_server.clone());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(texture),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert!(renderer.runtime.render_assets.contains_texture(texture));
    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 1);
    assert_eq!(stats.uploaded_render_assets, 1);
    assert_eq!(stats.missing_render_assets, 0);
    assert_eq!(stats.failed_render_assets, 0);

    asset_server.unload(&texture);
    asset_server
        .update()
        .expect("runtime asset unload should update");
    ctx.begin_frame()
        .expect("second headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert!(!renderer.runtime.render_assets.contains_texture(texture));
    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 0);
    assert_eq!(stats.missing_render_assets, 1);
}

#[test]
fn missing_sprite_texture_asset_is_reported_in_render_stats() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );
    let missing = Handle::<TextureAsset>::new(AssetId::new());
    let mut world = World::new();
    world.insert_resource(Diagnostics::default());
    world.insert_resource(AssetServer::with_empty_manifest(AssetConfig::default()));
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        crate::render::SpriteRenderer::new(8.0, 8.0).texture(missing),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.resident_render_assets, 0);
    assert_eq!(stats.uploaded_render_assets, 0);
    assert_eq!(stats.loading_render_assets, 0);
    assert_eq!(stats.missing_render_assets, 1);
    assert_eq!(stats.failed_render_assets, 0);

    let diagnostics = world
        .get_resource::<Diagnostics>()
        .expect("diagnostics should exist")
        .entries();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id.as_str(), "render.asset.texture.missing");
    assert_eq!(diagnostics[0].subsystem, DiagnosticSubsystem::render());
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostics[0].title, "Texture asset is missing");
    assert_eq!(
        diagnostics[0].help.as_deref(),
        Some(
            "Check that the texture is registered in the asset manifest or inserted as a runtime \
             asset before rendering."
        )
    );
    let missing_id = missing.id().to_string();
    assert_eq!(diagnostics[0].field("asset_id"), Some(missing_id.as_str()));
}

#[test]
fn forward_3d_descriptor_includes_directional_shadow_phase() {
    let descriptor = RenderPipelineAsset::forward_3d().descriptor();
    assert!(descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Phase("directional_shadow")
    )));
    assert!(descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Phase("scene_material_prepass")
    )));
}

#[test]
fn forward_3d_enables_shadow_view_for_perspective_directional_light() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_quad",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -3.0),
        MeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.shadows.views.len(), 1);
    assert!(renderer.shadows.views[0].enabled());
    assert!(renderer.shadows.views[0].caster_count() > 0);
}

#[test]
fn forward_3d_enables_shadow_view_for_orthographic_directional_light() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_quad_ortho",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 8.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(12.0, 12.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(4.0, 4.0),
        MeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.shadows.views.len(), 1);
    assert!(renderer.shadows.views[0].enabled());
    assert!(renderer.shadows.views[0].caster_count() > 0);
}

const TEST_HOLOGRAM_SHADER: &str = r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct HologramUniform {
    tint: vec4<f32>,
    params: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: HologramUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    let world_position = model * vec4<f32>(input.position, 1.0);
    output.clip_position = camera.view_proj * world_position;
    output.world_position = world_position.xyz;
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let scan = 0.55 + 0.45 * sin(input.world_position.y * material.params.y + input.uv.x * 12.0);
    let edge = pow(1.0 - abs(input.uv.y * 2.0 - 1.0), 2.0);
    let glow = material.params.x * (0.35 + scan * 0.65 + edge * 0.8);
    let alpha = material.tint.a * (0.25 + scan * 0.55 + edge * 0.2);
    return vec4<f32>(material.tint.rgb * glow, alpha);
}
"#;

const TEST_SCENE_PREPASS_SHADER: &str = r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct MaterialUniform {
    color: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    output.clip_position = camera.view_proj * (model * vec4<f32>(input.position, 1.0));
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(material.color.rgb * vec3<f32>(0.6 + input.uv.x * 0.4), material.color.a);
}
"#;

const TEST_SCENE_PREPASS_GBUFFER_SHADER: &str = r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: ViewUniform;

struct MaterialUniform {
    color: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) model_col0: vec4<f32>,
    @location(9) model_col1: vec4<f32>,
    @location(10) model_col2: vec4<f32>,
    @location(11) model_col3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct FragmentOutput {
    @location(0) albedo: vec4<f32>,
    @location(1) material: vec4<f32>,
    @location(2) emissive: vec4<f32>,
    @location(3) encoded_normal: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_col0,
        input.model_col1,
        input.model_col2,
        input.model_col3,
    );
    output.clip_position = camera.view_proj * (model * vec4<f32>(input.position, 1.0));
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    let tint = vec3<f32>(input.uv.x, input.uv.y, 1.0 - input.uv.x * 0.5);
    var output: FragmentOutput;
    output.albedo = vec4<f32>(material.color.rgb * tint, material.color.a);
    output.material = vec4<f32>(0.15, 0.75, 0.25, material.color.a);
    output.emissive = vec4<f32>(material.color.rgb * 0.05, material.color.a);
    output.encoded_normal = vec4<f32>(0.5, 0.5, 1.0, 1.0);
    return output;
}
"#;

#[derive(Clone)]
struct TestHologramMaterial {
    tint: Color,
    intensity: f32,
    stripe_scale: f32,
}

impl crate::render::Material for TestHologramMaterial {
    fn shader_source(&self) -> crate::render::ShaderSource {
        crate::render::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(TEST_HOLOGRAM_SHADER))
    }

    fn vertex_layout(&self) -> crate::render::expert::VertexLayout {
        crate::render::expert::Mesh::vertex_layout_position_uv()
    }

    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test_hologram_material_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn create_bind_group(&self, ctx: &crate::render::MaterialBindContext<'_>) -> wgpu::BindGroup {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct HologramUniform {
            tint: [f32; 4],
            params: [f32; 4],
        }

        let uniform = HologramUniform {
            tint: self.tint.to_array(),
            params: [self.intensity, self.stripe_scale, 0.0, 0.0],
        };
        let buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("test_hologram_material_uniform"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("test_hologram_material_bg"),
            layout: ctx.layout(),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    fn render_state(&self) -> crate::render::MaterialRenderState {
        crate::render::MaterialRenderState::transparent()
    }
}

#[derive(Clone)]
struct TestScenePrepassMaterial {
    color: Color,
}

impl crate::render::Material for TestScenePrepassMaterial {
    fn shader_source(&self) -> crate::render::ShaderSource {
        crate::render::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(TEST_SCENE_PREPASS_SHADER))
    }

    fn vertex_layout(&self) -> crate::render::expert::VertexLayout {
        crate::render::expert::Mesh::vertex_layout_position_uv()
    }

    fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test_scene_prepass_material_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn create_bind_group(&self, ctx: &crate::render::MaterialBindContext<'_>) -> wgpu::BindGroup {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct MaterialUniform {
            color: [f32; 4],
        }

        let buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("test_scene_prepass_material_uniform"),
                contents: bytemuck::bytes_of(&MaterialUniform {
                    color: self.color.to_array(),
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("test_scene_prepass_material_bg"),
            layout: ctx.layout(),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    fn render_state(&self) -> crate::render::MaterialRenderState {
        crate::render::MaterialRenderState::opaque()
    }

    fn scene_prepass_shader_source(&self) -> Option<crate::render::ShaderSource> {
        Some(crate::render::ShaderSource::Wgsl(
            std::borrow::Cow::Borrowed(TEST_SCENE_PREPASS_GBUFFER_SHADER),
        ))
    }

    fn scene_prepass_vertex_layout(&self) -> crate::render::expert::VertexLayout {
        crate::render::expert::Mesh::vertex_layout_position_uv()
    }
}

#[test]
fn custom_material_registration_renders_mesh_without_engine_changes() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let pipeline = RenderPipelineBuilder::new()
        .register_material::<TestHologramMaterial>()
        .add_phase(crate::render::TransparentPhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);

    let vertices = [
        Vertex {
            position: [-0.9, -1.1, 0.3],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.1, -0.7, -0.2],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.7, 1.0, 0.1],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 0.6, -0.3],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_uv(),
            "custom_material_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle =
        renderer
            .materials_mut::<TestHologramMaterial>()
            .insert(TestHologramMaterial {
                tint: Color::new(0.2, 0.9, 1.0, 0.72),
                intensity: 1.35,
                stripe_scale: 14.0,
            });

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 6.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0).with_euler_angles(0.35, 0.0, 0.2),
        MeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert_eq!(stats.draw_calls, 1);
    assert!(stats.passes >= 1);
}

#[test]
fn custom_material_scene_prepass_runs_in_opaque_3d_pipeline() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let pipeline = RenderPipelineBuilder::new()
        .register_material::<TestScenePrepassMaterial>()
        .add_phase(crate::render::SceneNormalPrepass::default())
        .add_phase(crate::render::SceneMaterialPrepass::default())
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);

    let vertices = [
        Vertex {
            position: [-0.8, -0.8, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.8, -0.8, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.8, 0.8, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.8, 0.8, 0.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_uv(),
            "custom_scene_prepass_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle =
        renderer
            .materials_mut::<TestScenePrepassMaterial>()
            .insert(TestScenePrepassMaterial {
                color: Color::new(0.9, 0.4, 0.2, 1.0),
            });

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 4.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        MeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert!(stats.passes >= 3);
    assert_eq!(stats.draw_calls, 1);
}

#[test]
fn forward_3d_global_illumination_executes_with_scene_material_inputs() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "forward_3d_global_illumination_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial {
            albedo: Color::new(0.82, 0.48, 0.26, 1.0),
            emissive: Color::new(0.08, 0.03, 0.01, 1.0),
            ..StandardMaterial::default()
        });

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        global_illumination: crate::render::GlobalIlluminationSettings {
            enabled: true,
            intensity: 0.48,
            detail_strength: 0.2,
            probe_volume: crate::render::ProbeVolumeGiSettings {
                counts: [6, 4, 6],
                spacing: 3.0,
                ..Default::default()
            },
            ..crate::render::GlobalIlluminationSettings::default()
        },
        bloom: crate::render::BloomSettings {
            enabled: false,
            ..Default::default()
        },
        tonemap: crate::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        vignette: crate::render::VignetteSettings {
            enabled: false,
            ..Default::default()
        },
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 4.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        MeshRenderer::new(mesh_handle, material_handle),
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 2.0),
        DirectionalLight::new([0.0, 0.0, -1.0])
            .intensity(0.9)
            .color(Color::new(1.0, 0.96, 0.9, 1.0)),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 2);
    assert!(stats.passes >= 5);
    assert!(stats.draw_calls >= 1);
}

#[derive(Clone)]
struct CountingComputePass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl ComputePass for CountingComputePass {
    fn name(&self) -> &'static str {
        "counting_compute"
    }

    fn setup(&mut self, ctx: &mut ComputePassSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom compute");
        ctx.graph().add_compute_pass(self.name(), |setup| {
            setup.readwrite(current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingPostFxPass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl PostFxPass for CountingPostFxPass {
    fn name(&self) -> &'static str {
        "counting_postfx"
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom postfx");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(current.handle());
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingFinalizePass {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPass for CountingFinalizePass {
    fn name(&self) -> &'static str {
        "counting_finalize"
    }

    fn setup(&mut self, ctx: &mut RenderPassSetupContext<'_, '_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let first_view = ctx
            .state()
            .completed_views()
            .first()
            .expect("finalize pass should see at least one prepared view");
        let input = first_view
            .slots()
            .current_color()
            .expect("view should expose a current color slot");
        let size = first_view.target_size();
        let sink = ctx.graph().create_texture(|builder| {
            builder
                .name("counting_finalize_sink")
                .size(TargetSize::Exact(size[0], size[1]))
                .format(input.format())
                .persistent();
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, sink);
        });
        let _ = ctx.state().set_current_color(sink, input.format());
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert_eq!(ctx.completed_views().len(), 1);
        assert_eq!(ctx.pass().name.as_ref(), self.name());
        Ok(())
    }
}

#[derive(Clone)]
struct CountingPhase {
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPhase for CountingPhase {
    fn name(&self) -> &'static str {
        "counting_phase"
    }

    fn setup(&mut self, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before custom phase");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        assert!(ctx.scene_view().is_some());

        let (
            gpu,
            pass,
            resources,
            _execution,
            _draw_functions,
            _material_registry,
            _mesh_registry,
            _fallback,
        ) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[derive(Clone)]
struct ViewFilteredPhase {
    enabled_order: i32,
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl RenderPhase for ViewFilteredPhase {
    fn name(&self) -> &'static str {
        "view_filtered_phase"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        view.order() == self.enabled_order
    }

    fn setup(&mut self, ctx: &mut RenderPhaseSetupContext<'_, '_>) {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before filtered phase");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut RenderPhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        let (gpu, pass, resources, _execution, _, _, _, _) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[derive(Clone)]
struct ViewFilteredPostFxPass {
    enabled_order: i32,
    setup_calls: Arc<AtomicUsize>,
    execute_calls: Arc<AtomicUsize>,
}

impl PostFxPass for ViewFilteredPostFxPass {
    fn name(&self) -> &'static str {
        "view_filtered_postfx"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        view.order() == self.enabled_order
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        let current = ctx
            .state()
            .current_color()
            .expect("scene color should exist before filtered postfx");
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(current.handle());
            setup.write_color_loaded(0, current.handle());
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        assert_eq!(ctx.view().order(), self.enabled_order);
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        let (gpu, pass, resources, _execution) = ctx.split();
        let output_handle =
            crate::render::execution::pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let _render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        Ok(())
    }
}

#[test]
fn custom_pipeline_steps_receive_setup_and_execute_contexts() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let compute_setup = Arc::new(AtomicUsize::new(0));
    let compute_execute = Arc::new(AtomicUsize::new(0));
    let postfx_setup = Arc::new(AtomicUsize::new(0));
    let postfx_execute = Arc::new(AtomicUsize::new(0));
    let finalize_setup = Arc::new(AtomicUsize::new(0));
    let finalize_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_compute(CountingComputePass {
            setup_calls: compute_setup.clone(),
            execute_calls: compute_execute.clone(),
        })
        .add_postfx(CountingPostFxPass {
            setup_calls: postfx_setup.clone(),
            execute_calls: postfx_execute.clone(),
        })
        .add_pass(CountingFinalizePass {
            setup_calls: finalize_setup.clone(),
            execute_calls: finalize_execute.clone(),
        })
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    let world = World::new();

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(compute_setup.load(Ordering::Relaxed), 1);
    assert_eq!(compute_execute.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_setup.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_execute.load(Ordering::Relaxed), 1);
    assert_eq!(finalize_setup.load(Ordering::Relaxed), 1);
    assert_eq!(finalize_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn custom_phase_steps_receive_setup_and_execute_contexts() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let phase_setup = Arc::new(AtomicUsize::new(0));
    let phase_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_phase(CountingPhase {
            setup_calls: phase_setup.clone(),
            execute_calls: phase_execute.clone(),
        })
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    let world = World::new();

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(phase_setup.load(Ordering::Relaxed), 1);
    assert_eq!(phase_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn view_specific_phase_and_postfx_skip_disabled_views() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let phase_setup = Arc::new(AtomicUsize::new(0));
    let phase_execute = Arc::new(AtomicUsize::new(0));
    let postfx_setup = Arc::new(AtomicUsize::new(0));
    let postfx_execute = Arc::new(AtomicUsize::new(0));

    let pipeline = RenderPipelineBuilder::new()
        .add_phase(ViewFilteredPhase {
            enabled_order: 1,
            setup_calls: phase_setup.clone(),
            execute_calls: phase_execute.clone(),
        })
        .add_postfx(ViewFilteredPostFxPass {
            enabled_order: 1,
            setup_calls: postfx_setup.clone(),
            execute_calls: postfx_execute.clone(),
        })
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(0, 0, 32, 32)).order(10),
    ));
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        CameraViewport::new(ViewportRect::new(32, 0, 32, 32)).order(20),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().view_count, 2);
    assert_eq!(phase_setup.load(Ordering::Relaxed), 1);
    assert_eq!(phase_execute.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_setup.load(Ordering::Relaxed), 1);
    assert_eq!(postfx_execute.load(Ordering::Relaxed), 1);
}

#[test]
fn register_material_automatically_wires_mesh_draw_and_extract() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let pipeline = RenderPipelineBuilder::new()
        .register_material::<UnlitMaterial>()
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let material_handle = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        MeshRenderer::new(mesh_handle, material_handle),
    ));

    assert_eq!(renderer.plan.extractors.len(), 1);

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().view_count, 1);
    assert!(renderer.stats().passes >= 2);
}

#[test]
fn transparent_sprite_phase_reports_single_draw_call_for_same_texture_batch() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(
        RenderPipelineAsset::builder()
            .add_feature(crate::render::SpriteFeature::unlit())
            .add_phase(crate::render::TransparentPhase::new())
            .build(),
    );

    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let white = asset_server.insert_runtime(TextureAsset::white_pixel());

    let mut world = World::new();
    world.insert_resource(asset_server);
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-8.0, 0.0, 0.0),
        crate::render::SpriteRenderer::new(16.0, 16.0)
            .color(Color::RED)
            .texture(white.clone()),
    ));
    world.spawn((
        Transform::from_xyz(8.0, 0.0, 0.0),
        crate::render::SpriteRenderer::new(16.0, 16.0)
            .color(Color::GREEN)
            .texture(white.clone()),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().draw_calls, 1);
}

#[test]
fn opaque_mesh_phase_batches_same_mesh_and_material_instances() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let pipeline = RenderPipelineBuilder::new()
        .register_material::<UnlitMaterial>()
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let material_handle = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        MeshRenderer::new(mesh_handle, material_handle),
    ));
    world.spawn((
        Transform::from_xyz(8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        MeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().draw_calls, 1);
}

#[test]
fn opaque_mesh_phase_keeps_separate_draws_for_different_material_instances() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);

    let pipeline = RenderPipelineBuilder::new()
        .register_material::<UnlitMaterial>()
        .add_phase(crate::render::expert::OpaquePhase::new())
        .build();
    let mut renderer = RenderComposer::from_asset(pipeline);
    renderer.register_material::<UnlitMaterial>(&ctx);

    let mesh_handle = renderer.insert_mesh(crate::render::expert::Mesh::builtin_quad(&ctx));
    let green = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.3, 0.8, 0.4, 1.0)));
    let orange = renderer
        .materials_mut::<UnlitMaterial>()
        .insert(UnlitMaterial::default().color(Color::new(0.9, 0.5, 0.2, 1.0)));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(64.0, 64.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        MeshRenderer::new(mesh_handle, green),
    ));
    world.spawn((
        Transform::from_xyz(8.0, 0.0, 0.2).with_scale(16.0, 16.0),
        MeshRenderer::new(mesh_handle, orange),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.stats().draw_calls, 2);
}

#[test]
fn collect_world_views_uses_camera_projection_viewport_and_layer_mask() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(12.0, -4.0, 8.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
        CameraViewport::new(ViewportRect::new(100, 50, 400, 300))
            .order(7)
            .layer_mask(0b0011),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        CameraMarker::new().enabled(false),
        Projection::orthographic_fixed(64.0, 64.0),
        CameraViewport::new(ViewportRect::new(0, 0, 64, 64)).order(99),
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);
    assert_eq!(views.len(), 1);

    let view = views[0];
    assert_eq!(view.order, 7);
    assert_eq!(view.viewport, ViewportRect::new(100, 50, 400, 300));
    assert_eq!(view.target_size, [400, 300]);
    assert_eq!(view.layer_mask, 0b0011);
    assert!(view.is_planar_2d);
}

#[test]
fn collect_world_views_prefers_explicit_viewports_over_implicit_main_camera() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(10.0, 20.0, 30.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-4.0, 6.0, 8.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(160.0, 90.0),
        CameraViewport::new(ViewportRect::new(50, 40, 320, 200)).order(5),
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].order, 5);
    assert_eq!(views[0].viewport, ViewportRect::new(50, 40, 320, 200));
    assert_eq!(views[0].view_uniform.camera, [-4.0, 6.0, 8.0, 1.0]);
}

#[test]
fn collect_world_views_uses_main_camera_for_implicit_view_selection() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(1.0, 2.0, 3.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
    ));
    world.spawn((
        Transform::from_xyz(11.0, 12.0, 13.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(640.0, 360.0),
        MainCamera,
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].viewport, ViewportRect::new(0, 0, 800, 600));
    assert_eq!(views[0].target_size, [800, 600]);
    assert_eq!(views[0].view_uniform.camera, [11.0, 12.0, 13.0, 1.0]);
}

#[test]
fn collect_world_views_reports_missing_projection_diagnostic() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.insert_resource(Diagnostics::default());
    let camera = world.spawn((Transform::default(), CameraMarker::new(), MainCamera));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);

    assert_eq!(views.len(), 1);
    let diagnostics = world
        .get_resource::<Diagnostics>()
        .expect("diagnostics should exist")
        .entries();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].id.as_str(),
        EngineDiagnosticKind::CAMERA_MISSING_PROJECTION
    );
    assert_eq!(diagnostics[0].subsystem, DiagnosticSubsystem::render());
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostics[0].entity, Some(camera));
    assert_eq!(diagnostics[0].title, "Camera is missing a Projection");
    assert_eq!(
        diagnostics[0].help.as_deref(),
        Some(
            "Add Projection::orthographic(height) for stable world-unit sizing, or \
             Projection::orthographic_fixed(width, height) for a fixed logical view."
        )
    );
}

#[test]
fn projection_view_uniforms_stay_finite_for_orthographic_and_perspective() {
    for projection in [
        Projection::orthographic_fixed(1280.0, 720.0),
        Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0),
    ] {
        let uniform = projection.view_uniform(Transform::from_xyz(3.0, 4.0, 5.0), [1280, 720]);
        assert!(uniform.view_proj.iter().all(|value| value.is_finite()));
        assert!(uniform.camera.iter().all(|value| value.is_finite()));
        assert!(uniform.viewport.iter().all(|value| value.is_finite()));
    }
}

#[test]
fn perspective_screen_to_world_intersects_the_world_z_plane() {
    let projection = Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0);
    let transform = Transform::from_xyz(10.0, 20.0, 10.0);

    let center =
        projection.screen_to_world(transform, [800.0, 600.0].into(), [400.0, 300.0].into());
    assert!((center[0] - 10.0).abs() <= 0.001);
    assert!((center[1] - 20.0).abs() <= 0.001);

    let top_left = projection.screen_to_world(transform, [800.0, 600.0].into(), [0.0, 0.0].into());
    assert!(top_left[0] < transform.x());
    assert!(top_left[1] > transform.y());
    assert!(top_left.to_array().iter().all(|value| value.is_finite()));
}

#[test]
fn orthographic_screen_to_world_respects_camera_rotation() {
    let projection = Projection::orthographic_fixed(100.0, 50.0);
    let transform = Transform::from_xy(10.0, 20.0).with_rotation(std::f32::consts::FRAC_PI_2);

    let center = projection.screen_to_world(transform, [200.0, 100.0].into(), [100.0, 50.0].into());
    assert!((center[0] - 10.0).abs() <= 0.001);
    assert!((center[1] - 20.0).abs() <= 0.001);

    let right_edge =
        projection.screen_to_world(transform, [200.0, 100.0].into(), [200.0, 50.0].into());
    assert!((right_edge[0] - 10.0).abs() <= 0.001);
    assert!((right_edge[1] - 70.0).abs() <= 0.001);
}

#[test]
fn perspective_view_extraction_keeps_camera_depth_and_disables_2d_culling() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [1280, 720];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(3.0, 4.0, 12.0),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 500.0),
        CameraViewport::new(ViewportRect::new(0, 0, 640, 360)).order(2),
        MainCamera,
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);
    assert_eq!(views.len(), 1);
    let view = views[0];
    assert_eq!(view.order, 2);
    assert_eq!(view.target_size, [640, 360]);
    assert_eq!(view.view_uniform.camera, [3.0, 4.0, 12.0, 1.0]);
    assert!(!view.is_planar_2d);
    assert!(view
        .view_uniform
        .view_proj
        .iter()
        .all(|value| value.is_finite()));
}

#[test]
fn tilted_orthographic_view_disables_2d_culling() {
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.runtime.surface_size = [800, 600];

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 10.0).with_euler_angles(0.35, 0.0, 0.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(320.0, 180.0),
        MainCamera,
    ));

    let resolved = renderer.resolve_scene_transforms(&world);
    let views = renderer.collect_world_views(&world, &resolved);
    assert_eq!(views.len(), 1);
    let view = views[0];
    assert!(!view.is_planar_2d);
    assert!(view
        .view_uniform
        .view_proj
        .iter()
        .all(|value| value.is_finite()));
}

#[cfg(feature = "live2d")]
#[test]
fn live2d_scene_sort_and_layer_visibility_follow_queue_policy() {
    let base = vec![
        Live2DSceneInstance {
            entity: EntityId::new(3, 0),
            model_index: 0,
            transform: Transform::from_xyz(0.0, 0.0, 0.8),
            layer_mask: 0b0001,
            sorting_layer: SortingLayer(1),
            order_in_layer: OrderInLayer(0),
        },
        Live2DSceneInstance {
            entity: EntityId::new(1, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 0.2),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(0),
            order_in_layer: OrderInLayer(5),
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 2,
            transform: Transform::from_xyz(0.0, 0.0, 0.1),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(1),
            order_in_layer: OrderInLayer(0),
        },
    ];

    let mut transparent = base.clone();
    sort_live2d_scene_instances(&mut transparent, RenderQueueSort::TransparentScene, None);
    assert_eq!(
        transparent
            .iter()
            .map(|item| (
                item.entity.index(),
                item.sorting_layer.0,
                item.order_in_layer.0
            ))
            .collect::<Vec<_>>(),
        vec![(1, 0, 5), (2, 1, 0), (3, 1, 0)]
    );

    let mut opaque = base.clone();
    sort_live2d_scene_instances(&mut opaque, RenderQueueSort::OpaqueDepthFrontToBack, None);
    assert_eq!(
        opaque
            .iter()
            .map(|item| (item.entity.index(), item.transform.z()))
            .collect::<Vec<_>>(),
        vec![(2, 0.1), (1, 0.2), (3, 0.8)]
    );

    let projection = Projection::orthographic_fixed(64.0, 64.0);
    let view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        0b0010,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [64, 64]),
        true,
    );
    assert!(!live2d_instance_visible_in_view(&base[0], &view));
    assert!(live2d_instance_visible_in_view(&base[1], &view));
    assert!(live2d_instance_visible_in_view(&base[2], &view));
}

#[cfg(feature = "live2d")]
#[test]
fn live2d_perspective_sort_uses_view_relative_depth() {
    let mut instances = vec![
        Live2DSceneInstance {
            entity: EntityId::new(1, 0),
            model_index: 0,
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            layer_mask: u32::MAX,
            sorting_layer: SortingLayer(0),
            order_in_layer: OrderInLayer(0),
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 5.0),
            layer_mask: u32::MAX,
            sorting_layer: SortingLayer(0),
            order_in_layer: OrderInLayer(0),
        },
    ];
    let projection = Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0);
    let perspective_view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        u32::MAX,
        Transform::from_xyz(0.0, 0.0, 10.0),
        projection,
        projection.view_uniform(Transform::from_xyz(0.0, 0.0, 10.0), [64, 64]),
        false,
    );

    sort_live2d_scene_instances(
        &mut instances,
        RenderQueueSort::TransparentScene,
        Some(&perspective_view),
    );
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.entity.index())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    sort_live2d_scene_instances(
        &mut instances,
        RenderQueueSort::OpaqueDepthFrontToBack,
        Some(&perspective_view),
    );
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.entity.index())
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
}
