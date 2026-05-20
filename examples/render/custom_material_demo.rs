//! Custom `Material` demo.
//!
//! Shows the public user-extension path for a custom mesh material:
//! define a new `Material`, register it with the pipeline builder, then
//! create material instances and meshes through `FrameContext::with_render_runtime_mut(...)`.
//!
//! ```bash
//! cargo run --example custom_material_demo --features app --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::expert::{Mesh, MeshDescriptor, MeshIndexData};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Material, MaterialBinding, MaterialError, MaterialHandle,
    MaterialInterface, MaterialPrepareContext, MaterialRenderState, MaterialShaderSet,
    PreparedMaterial, Projection, RenderPipelineAsset, Transform, TransparentPhase,
    WgpuMeshRenderer,
};

const HOLOGRAM_SHADER: &str = r#"
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
    let scan = 0.55 + 0.45 * sin(input.world_position.y * material.params.y + input.uv.x * 14.0);
    let edge = pow(1.0 - abs(input.uv.y * 2.0 - 1.0), 2.0);
    let shimmer = 0.7 + 0.3 * cos(input.world_position.x * 5.0 + input.uv.y * 9.0);
    let glow = material.params.x * (0.25 + scan * 0.55 + edge * shimmer);
    let alpha = material.tint.a * (0.18 + scan * 0.52 + edge * 0.3);
    return vec4<f32>(material.tint.rgb * glow, alpha);
}
"#;

#[derive(Clone)]
struct HologramMaterial {
    tint: Color,
    intensity: f32,
    stripe_scale: f32,
}

impl HologramMaterial {
    fn new(tint: Color) -> Self {
        Self {
            tint,
            intensity: 1.35,
            stripe_scale: 14.0,
        }
    }
}

impl Material for HologramMaterial {
    type Data = HologramMaterial;

    fn interface() -> MaterialInterface {
        MaterialInterface::builder("hologram")
            .shader(MaterialShaderSet::wgsl(HOLOGRAM_SHADER))
            .vertex(Mesh::vertex_layout_position_uv())
            .binding(MaterialBinding::uniform(
                0,
                std::num::NonZeroU64::new(32).expect("hologram uniform has non-zero size"),
            ))
            .main_pass(sky_engine::render::MainPassMode::Transparent)
            .render_state(MaterialRenderState::transparent())
            .build()
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError> {
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
            .uniform(0, "custom_material_demo_uniform", &uniform)
            .build()
    }

    fn render_state(_data: &Self::Data) -> MaterialRenderState {
        MaterialRenderState::transparent()
    }
}

#[derive(Clone, Copy)]
struct Spin {
    speed: f32,
}

#[derive(Clone, Copy)]
struct Bob {
    amplitude: f32,
    phase: f32,
}

struct CustomMaterialDemo {
    initialized: bool,
    time: f32,
    fps_smooth: f32,
    frame_count: u32,
}

impl AppState for CustomMaterialDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        self.time += ctx.dt;

        if !self.initialized {
            let (mesh_handle, cyan, gold) = ctx
                .with_render_runtime_mut(|renderer, gpu| {
                    #[repr(C)]
                    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
                    struct Vertex {
                        position: [f32; 3],
                        uv: [f32; 2],
                    }

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
                        gpu,
                        MeshDescriptor::new(
                            bytemuck::cast_slice(&vertices),
                            vertices.len() as u32,
                            Mesh::vertex_layout_position_uv(),
                            "custom_material_demo_mesh",
                        )
                        .with_indices(MeshIndexData::U16(&indices)),
                    ));
                    let cyan = renderer.insert_material::<HologramMaterial>(HologramMaterial::new(
                        Color::new(0.20, 0.90, 1.00, 0.72),
                    ));
                    let gold = renderer.insert_material::<HologramMaterial>(HologramMaterial::new(
                        Color::new(1.00, 0.78, 0.22, 0.68),
                    ));
                    (mesh_handle, cyan, gold)
                })
                .expect("custom_material_demo requires RenderPlugin::pipeline(...)");

            spawn_hologram(
                ctx.world,
                mesh_handle,
                cyan.into(),
                -1.4,
                0.0,
                -0.8,
                0.8,
                0.0,
            );
            spawn_hologram(
                ctx.world,
                mesh_handle,
                gold.into(),
                1.5,
                0.4,
                0.6,
                -0.65,
                1.7,
            );
            spawn_hologram(
                ctx.world,
                mesh_handle,
                cyan.into(),
                0.0,
                -1.2,
                1.2,
                0.55,
                3.2,
            );
            self.initialized = true;
        }

        let time = self.time;
        let mut query = ctx.world.query::<(&mut Transform, &Spin, &Bob)>();
        query.for_each(ctx.world, |(transform, spin, bob)| {
            transform.rotate_z(spin.speed * ctx.dt);
            transform.position[1] = bob.amplitude * (time + bob.phase).sin();
        });

        ctx.render();

        let fps_instant = if ctx.dt > 0.0 { 1.0 / ctx.dt } else { 0.0 };
        self.fps_smooth = if self.fps_smooth == 0.0 {
            fps_instant
        } else {
            self.fps_smooth * 0.92 + fps_instant * 0.08
        };
        self.frame_count += 1;
        if self.frame_count % 30 == 0 {
            let stats = ctx.render_stats();
            ctx.set_title(&format!(
                "SkyEngine — Custom Material Demo | {:.0} FPS | {} draws",
                self.fps_smooth, stats.draw_calls
            ));
        }
    }
}

fn spawn_hologram(
    world: &mut World,
    mesh: sky_engine::render::expert::MeshHandle,
    material: MaterialHandle,
    x: f32,
    y: f32,
    z: f32,
    spin: f32,
    phase: f32,
) {
    world.spawn((
        Transform::from_xyz(x, y, z)
            .with_scale3(1.8, 2.4, 1.0)
            .with_euler_angles(0.55, 0.0, 0.2),
        WgpuMeshRenderer::new(mesh, material),
        Spin { speed: spin },
        Bob {
            amplitude: 0.35,
            phase,
        },
    ));
}

fn main() {
    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 6.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));

    let pipeline = RenderPipelineAsset::builder()
        .register_material::<HologramMaterial>()
        .add_phase(TransparentPhase::new())
        .build();

    world
        .install(WindowPlugin::new(
            "SkyEngine — Custom Material Demo",
            960,
            640,
        ))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::pipeline(pipeline)).unwrap();

    App::new(world).run(CustomMaterialDemo {
        initialized: false,
        time: 0.0,
        fps_smooth: 0.0,
        frame_count: 0,
    });
}
