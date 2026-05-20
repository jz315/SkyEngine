//! Renderling-backed 3D scene demo.
//!
//! ```bash
//! cargo run --example renderling_3d --features app,renderling-renderer --release
//! ```

use std::f32::consts::{FRAC_PI_2, TAU};

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::{EntityId, World};
use sky_engine::math::{Quat, Vec3};
use sky_engine::render::{
    CameraMarker, Color, DirectionalLight, MeshAsset, MeshAssetDescriptor, MeshIndexData,
    MeshRenderer, MeshVertexLayout, PointLight, Projection, RenderPipelineAsset,
    StandardMaterialAsset, Transform,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

#[derive(Default)]
struct RenderlingDemo {
    hero: Option<EntityId>,
    orbiters: Vec<EntityId>,
    light_a: Option<EntityId>,
    light_b: Option<EntityId>,
    glow_a: Option<EntityId>,
    glow_b: Option<EntityId>,
    camera: Option<EntityId>,
    time: f32,
}

impl AppState for RenderlingDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let mut render_assets = ctx.render_assets_mut();
        let cube = render_assets.insert_mesh(cube_mesh());
        let octahedron = render_assets.insert_mesh(octahedron_mesh());
        let plane = render_assets.insert_mesh(plane_mesh());

        let floor = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.18, 0.20, 0.22))
                .roughness(0.92),
        );
        let hero_material = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.92, 0.33, 0.16))
                .roughness(0.38)
                .metallic(0.18),
        );
        let blue = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.22, 0.56, 0.95))
                .roughness(0.5),
        );
        let teal = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.10, 0.78, 0.66))
                .roughness(0.44)
                .metallic(0.08),
        );
        let stone = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.53, 0.54, 0.50))
                .roughness(0.84),
        );
        let warm_glow = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(1.0, 0.58, 0.20))
                .emissive(Color::rgb(1.0, 0.42, 0.10))
                .roughness(0.2),
        );
        let cool_glow = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.36, 0.76, 1.0))
                .emissive(Color::rgb(0.10, 0.48, 1.0))
                .roughness(0.2),
        );
        drop(render_assets);

        ctx.world.spawn((
            Transform::from_xyz(0.0, -1.05, 0.0).with_scale3(12.0, 1.0, 12.0),
            MeshRenderer::new(plane, floor).casts_shadows(false),
        ));

        self.hero = Some(ctx.world.spawn((
            Transform::from_xyz(0.0, 0.35, 0.0).with_scale3(1.25, 1.25, 1.25),
            MeshRenderer::new(octahedron, hero_material),
        )));

        for (index, [x, z, height]) in [
            [-4.0, -2.6, 1.4],
            [4.0, -2.8, 1.9],
            [-3.7, 2.9, 1.1],
            [3.8, 2.6, 1.55],
        ]
        .into_iter()
        .enumerate()
        {
            ctx.world.spawn((
                Transform::from_xyz(x, -1.05 + height * 0.5, z)
                    .with_scale3(0.55, height, 0.55)
                    .with_rotation_quat(Quat::from_rotation_y(index as f32 * 0.45)),
                MeshRenderer::new(cube, stone),
            ));
        }

        for index in 0..10 {
            let angle = index as f32 / 10.0 * TAU;
            let radius = 3.0;
            let y = -0.25 + (index % 2) as f32 * 0.35;
            let material = if index % 2 == 0 { blue } else { teal };
            let entity = ctx.world.spawn((
                Transform::from_xyz(angle.cos() * radius, y, angle.sin() * radius)
                    .with_scale3(0.42, 0.42, 0.42)
                    .with_rotation_quat(Quat::from_rotation_y(angle + FRAC_PI_2)),
                MeshRenderer::new(cube, material),
            ));
            self.orbiters.push(entity);
        }

        self.light_a = Some(
            ctx.world.spawn((
                Transform::from_xyz(-2.8, 1.6, 1.8),
                PointLight::new(8.0)
                    .intensity(18.0)
                    .color(Color::rgb(1.0, 0.52, 0.24)),
            )),
        );
        self.light_b = Some(
            ctx.world.spawn((
                Transform::from_xyz(2.8, 1.2, -1.8),
                PointLight::new(7.0)
                    .intensity(14.0)
                    .color(Color::rgb(0.24, 0.58, 1.0)),
            )),
        );
        self.glow_a = Some(ctx.world.spawn((
            Transform::from_xyz(-2.8, 1.6, 1.8).with_scale3(0.18, 0.18, 0.18),
            MeshRenderer::new(cube, warm_glow).casts_shadows(false),
        )));
        self.glow_b = Some(ctx.world.spawn((
            Transform::from_xyz(2.8, 1.2, -1.8).with_scale3(0.18, 0.18, 0.18),
            MeshRenderer::new(cube, cool_glow).casts_shadows(false),
        )));

        self.camera = Some(ctx.world.spawn((
            Transform::from_xyz(0.0, 2.7, 7.2).with_euler_angles(-0.34, 0.0, 0.0),
            CameraMarker::new(),
            Projection::perspective(58.0f32.to_radians(), 0.1, 100.0),
        )));

        ctx.world.spawn((
            Transform::default(),
            DirectionalLight::new([0.35, -1.0, -0.42])
                .intensity(2.6)
                .color(Color::rgb(1.0, 0.95, 0.86)),
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.time += ctx.dt();

        if let Some(hero) = self.hero {
            if let Some(transform) = ctx.world.get_mut::<Transform>(hero) {
                transform.rotation = (Quat::from_rotation_y(self.time * 0.75)
                    * Quat::from_rotation_x((self.time * 0.55).sin() * 0.22))
                .normalized();
                transform.position = Vec3::new(0.0, 0.35 + (self.time * 1.4).sin() * 0.12, 0.0);
            }
        }

        for (index, entity) in self.orbiters.iter().copied().enumerate() {
            if let Some(transform) = ctx.world.get_mut::<Transform>(entity) {
                let base = index as f32 / self.orbiters.len() as f32 * TAU;
                let angle = base + self.time * 0.28;
                let radius = 3.0 + (self.time * 0.8 + index as f32).sin() * 0.18;
                transform.position = Vec3::new(
                    angle.cos() * radius,
                    -0.25 + (index % 2) as f32 * 0.35,
                    angle.sin() * radius,
                );
                transform.rotation = (Quat::from_rotation_y(angle + FRAC_PI_2)
                    * Quat::from_rotation_x(self.time * 0.9 + index as f32))
                .normalized();
            }
        }

        let light_a = [
            -2.8 + (self.time * 0.9).sin() * 0.55,
            1.65 + (self.time * 1.2).cos() * 0.25,
            1.8,
        ];
        let light_b = [
            2.8,
            1.25 + (self.time * 1.1).sin() * 0.22,
            -1.8 + (self.time * 0.75).cos() * 0.55,
        ];
        self.set_position(ctx.world, self.light_a, light_a);
        self.set_position(ctx.world, self.glow_a, light_a);
        self.set_position(ctx.world, self.light_b, light_b);
        self.set_position(ctx.world, self.glow_b, light_b);

        if let Some(camera) = self.camera {
            if let Some(transform) = ctx.world.get_mut::<Transform>(camera) {
                let orbit = self.time * 0.12;
                transform.position = Vec3::new(orbit.sin() * 1.2, 2.7, 7.2 + orbit.cos() * 0.45);
                transform.rotation = Quat::from_euler_angles(-0.34, orbit.sin() * 0.15, 0.0);
            }
        }

        ctx.render();
    }
}

impl RenderlingDemo {
    fn set_position(&self, world: &mut World, entity: Option<EntityId>, position: [f32; 3]) {
        if let Some(entity) = entity {
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.position = Vec3::from_array(position);
            }
        }
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new(
            "SkyEngine Renderling Showcase",
            1280,
            720,
        ))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::renderling_3d()).unwrap();

    App::new(world).run(RenderlingDemo::default());
}

fn plane_mesh() -> MeshAsset {
    let vertices = [
        Vertex {
            position: [-0.5, 0.0, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.0, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.0, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.0, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 1.0],
        },
    ];
    MeshAsset::from_raw(
        MeshAssetDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            MeshVertexLayout::position_normal_uv(),
            "renderling_floor_plane",
        )
        .with_indices(MeshIndexData::u32([0, 2, 1, 0, 3, 2])),
    )
}

fn cube_mesh() -> MeshAsset {
    let faces = [
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, 0.5],
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [-0.5, -0.5, 0.5],
            ],
        ),
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (face_index, (normal, positions)) in faces.into_iter().enumerate() {
        let base = (face_index * 4) as u32;
        vertices.extend([
            Vertex {
                position: positions[0],
                normal,
                uv: [0.0, 0.0],
            },
            Vertex {
                position: positions[1],
                normal,
                uv: [1.0, 0.0],
            },
            Vertex {
                position: positions[2],
                normal,
                uv: [1.0, 1.0],
            },
            Vertex {
                position: positions[3],
                normal,
                uv: [0.0, 1.0],
            },
        ]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    MeshAsset::from_raw(
        MeshAssetDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            MeshVertexLayout::position_normal_uv(),
            "renderling_cube",
        )
        .with_indices(MeshIndexData::u32(indices)),
    )
}

fn octahedron_mesh() -> MeshAsset {
    let positions = [
        [0.0, 0.85, 0.0],
        [0.85, 0.0, 0.0],
        [0.0, 0.0, 0.85],
        [-0.85, 0.0, 0.0],
        [0.0, 0.0, -0.85],
        [0.0, -0.85, 0.0],
    ];
    let triangles = [
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 1],
        [5, 2, 1],
        [5, 3, 2],
        [5, 4, 3],
        [5, 1, 4],
    ];

    let mut vertices = Vec::with_capacity(triangles.len() * 3);
    for triangle in triangles {
        let a = positions[triangle[0]];
        let b = positions[triangle[1]];
        let c = positions[triangle[2]];
        let normal = triangle_normal(a, b, c);
        vertices.extend([
            Vertex {
                position: a,
                normal,
                uv: [0.5, 0.0],
            },
            Vertex {
                position: b,
                normal,
                uv: [1.0, 1.0],
            },
            Vertex {
                position: c,
                normal,
                uv: [0.0, 1.0],
            },
        ]);
    }

    MeshAsset::from_raw(MeshAssetDescriptor::new(
        bytemuck::cast_slice(&vertices),
        vertices.len() as u32,
        MeshVertexLayout::position_normal_uv(),
        "renderling_octahedron",
    ))
}

fn triangle_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
        .sqrt()
        .max(1e-6);
    [normal[0] / len, normal[1] / len, normal[2] / len]
}
