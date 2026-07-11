use super::common::*;

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
    type Data = TestHologramMaterial;

    fn interface() -> crate::render::resources::material::MaterialInterface {
        crate::render::resources::material::MaterialInterface::builder("test_hologram")
            .shader(crate::render::resources::material::MaterialShaderSet::wgsl(
                TEST_HOLOGRAM_SHADER,
            ))
            .vertex(crate::render::expert::resources::Mesh::vertex_layout_position_uv())
            .binding(
                crate::render::resources::material::MaterialBinding::uniform(
                    0,
                    std::num::NonZeroU64::new(32).expect("hologram uniform has non-zero size"),
                ),
            )
            .main_pass(crate::render::resources::material::MainPassMode::Transparent)
            .render_state(crate::render::MaterialRenderState::transparent())
            .build()
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut crate::render::resources::material::MaterialPrepareContext<'_>,
    ) -> Result<crate::render::resources::material::PreparedMaterial, MaterialError> {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct HologramUniform {
            tint: [f32; 4],
            params: [f32; 4],
        }

        let uniform = HologramUniform {
            tint: data.tint.to_array(),
            params: [data.intensity, data.stripe_scale, 0.0, 0.0],
        };
        ctx.bindings()
            .uniform(0, "test_hologram_material_uniform", &uniform)
            .build()
    }

    fn render_state(_data: &Self::Data) -> crate::render::MaterialRenderState {
        crate::render::MaterialRenderState::transparent()
    }
}

#[derive(Clone)]
struct TestScenePrepassMaterial {
    color: Color,
}

impl crate::render::Material for TestScenePrepassMaterial {
    type Data = TestScenePrepassMaterial;

    fn interface() -> crate::render::resources::material::MaterialInterface {
        crate::render::resources::material::MaterialInterface::builder("test_scene_prepass")
            .shader(crate::render::resources::material::MaterialShaderSet::wgsl(
                TEST_SCENE_PREPASS_SHADER,
            ))
            .vertex(crate::render::expert::resources::Mesh::vertex_layout_position_uv())
            .binding(
                crate::render::resources::material::MaterialBinding::uniform(
                    0,
                    std::num::NonZeroU64::new(16)
                        .expect("scene prepass material uniform has non-zero size"),
                ),
            )
            .render_state(crate::render::MaterialRenderState::opaque())
            .passes(crate::render::resources::material::MaterialPassSet {
                main: crate::render::resources::material::MainPassMode::Opaque,
                prepass: Some(
                    crate::render::resources::material::MaterialPrepassMode::SceneMaterial,
                ),
                shadow: crate::render::resources::material::ShadowPassMode::None,
            })
            .build()
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut crate::render::resources::material::MaterialPrepareContext<'_>,
    ) -> Result<crate::render::resources::material::PreparedMaterial, MaterialError> {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct MaterialUniform {
            color: [f32; 4],
        }

        ctx.bindings()
            .uniform(
                0,
                "test_scene_prepass_material_uniform",
                &MaterialUniform {
                    color: data.color.to_array(),
                },
            )
            .build()
    }

    fn render_state(_data: &Self::Data) -> crate::render::MaterialRenderState {
        crate::render::MaterialRenderState::opaque()
    }

    fn scene_prepass_shader_source(_data: &Self::Data) -> Option<crate::render::ShaderSource> {
        Some(crate::render::ShaderSource::wgsl(
            TEST_SCENE_PREPASS_GBUFFER_SHADER,
        ))
    }

    fn scene_prepass_vertex_layout(
        _data: &Self::Data,
    ) -> crate::render::expert::resources::VertexLayout {
        crate::render::expert::resources::Mesh::vertex_layout_position_uv()
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
    let mut renderer = RenderRuntime::from_asset(pipeline);
    renderer.register_material::<TestHologramMaterial>(&ctx);

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
    let material_handle = renderer.insert_material::<TestHologramMaterial>(TestHologramMaterial {
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
        WgpuMeshRenderer::new(mesh_handle, material_handle),
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
        .add_phase(crate::render::features::mesh::SceneNormalPrepass::default())
        .add_phase(crate::render::features::mesh::SceneMaterialPrepass::default())
        .add_phase(crate::render::expert::draw::OpaquePhase::new())
        .build();
    let mut renderer = RenderRuntime::from_asset(pipeline);
    renderer.register_material::<TestScenePrepassMaterial>(&ctx);

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
        renderer.insert_material::<TestScenePrepassMaterial>(TestScenePrepassMaterial {
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
        WgpuMeshRenderer::new(mesh_handle, material_handle),
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
