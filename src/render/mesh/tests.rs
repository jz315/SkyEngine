use super::*;
use crate::render::gpu::RenderTargetDescriptor;
use crate::render::resources::mesh::MeshIndexData;
use crate::render::view::Camera;

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for render tests");

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mesh_pass_test_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("Failed to create test GPU device")
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    color: [f32; 4],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

fn mesh_shader() -> &'static str {
    r#"
struct ViewUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: ViewUniform;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = view.view_proj * vec4<f32>(input.pos, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#
}

fn basic_pipeline_desc() -> MaterialPipelineDesc {
    MaterialPipelineDesc {
        label: "mesh_test".into(),
        shader_source: mesh_shader().into(),
        vs_entry: "vs_main",
        fs_entry: "fs_main",
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        material_slot: None,
        vertex_buffers: vec![wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        }],
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        color_write_mask: wgpu::ColorWrites::ALL,
    }
}

#[test]
fn mesh_pass_renders_indexed_mesh_to_target() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [32, 32],
    );
    let camera = Camera::new(32.0, 32.0);
    let mut mesh_pass = MeshPass::new(&ctx);
    let mut pipeline = mesh_pass
        .create_pipeline_cache(&ctx, basic_pipeline_desc(), None)
        .expect("mesh pipeline should build");
    let mesh = crate::render::resources::mesh::Mesh::from_vertices_indices(
        &ctx,
        &[
            Vertex {
                pos: [-8.0, -8.0],
                color: [1.0, 0.0, 0.0, 1.0],
            },
            Vertex {
                pos: [8.0, -8.0],
                color: [0.0, 1.0, 0.0, 1.0],
            },
            Vertex {
                pos: [0.0, 8.0],
                color: [0.0, 0.0, 1.0, 1.0],
            },
        ],
        MeshIndexData::U16(&[0, 1, 2]),
        "triangle",
    );
    let target = crate::render::gpu::RenderTarget::new(
        &ctx,
        32,
        32,
        wgpu::TextureFormat::Rgba8Unorm,
        "mesh",
    );
    let mut draws = [MeshDraw::new(&mesh, &mut pipeline)];

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    mesh_pass
        .render_to_target(&mut ctx, &target, &camera, Some(Color::BLACK), &mut draws)
        .expect("mesh pass should render");
    ctx.end_frame();
}

#[test]
fn mesh_pass_rejects_sample_count_mismatch() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [16, 16],
    );
    let camera = Camera::new(16.0, 16.0);
    let mut mesh_pass = MeshPass::new(&ctx);
    let mut pipeline = mesh_pass
        .create_pipeline_cache(&ctx, basic_pipeline_desc(), None)
        .expect("mesh pipeline should build");
    let mesh = crate::render::resources::mesh::Mesh::from_vertices(
        &ctx,
        &[Vertex {
            pos: [0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        }],
        "point",
    );
    let target = crate::render::gpu::RenderTarget::from_descriptor(
        &ctx,
        RenderTargetDescriptor::new(16, 16, wgpu::TextureFormat::Rgba8Unorm).sample_count(4),
    );
    let mut draws = [MeshDraw::new(&mesh, &mut pipeline)];

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    let err = mesh_pass
        .render_to_target(&mut ctx, &target, &camera, None, &mut draws)
        .expect_err("sample mismatch should fail");
    ctx.end_frame();

    assert!(matches!(
        err,
        MeshPassError::TargetSampleCountMismatch {
            target_samples: 4,
            ..
        }
    ));
}

#[test]
fn mesh_pass_rejects_depth_format_mismatch() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [16, 16],
    );
    let camera = Camera::new(16.0, 16.0);
    let mut mesh_pass = MeshPass::new(&ctx);
    let mut desc = basic_pipeline_desc();
    desc.depth_stencil = Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24Plus,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });
    let mut pipeline = mesh_pass
        .create_pipeline_cache(&ctx, desc, None)
        .expect("mesh pipeline should build");
    let mesh = crate::render::resources::mesh::Mesh::from_vertices(
        &ctx,
        &[Vertex {
            pos: [0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        }],
        "point",
    );
    let color = crate::render::gpu::RenderTarget::new(
        &ctx,
        16,
        16,
        wgpu::TextureFormat::Rgba8Unorm,
        "color",
    );
    let depth = crate::render::gpu::RenderTarget::new(
        &ctx,
        16,
        16,
        wgpu::TextureFormat::Depth32Float,
        "depth",
    );
    let mut draws = [MeshDraw::new(&mesh, &mut pipeline)];

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    let err = mesh_pass
        .render_to_target_with_depth(
            &mut ctx,
            &color,
            &depth,
            &camera,
            None,
            Some(1.0),
            &mut draws,
        )
        .expect_err("depth mismatch should fail");
    ctx.end_frame();

    assert!(matches!(
        err,
        MeshPassError::DepthFormatMismatch {
            pipeline_format: wgpu::TextureFormat::Depth24Plus,
            target_format: wgpu::TextureFormat::Depth32Float,
            ..
        }
    ));
}
