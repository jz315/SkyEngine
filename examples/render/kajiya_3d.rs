//! Kajiya-backed 3D scene demo.
//!
//! ```bash
//! cargo run --example kajiya_3d --features app,kajiya-renderer --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::{EntityId, World};
use sky_engine::input::{KeyCode, MouseButton};
use sky_engine::math::{Quat, Vec3};
use sky_engine::render::{
    CameraMarker, Color, DirectionalLight, KajiyaRendererSettings, MeshAsset, MeshAssetDescriptor,
    MeshIndexData, MeshRenderer, MeshVertexLayout, Projection, RenderPipelineAsset,
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
struct KajiyaDemo {
    cube: Option<EntityId>,
    camera: Option<EntityId>,
    camera_yaw: f32,
    camera_pitch: f32,
    time: f32,
    fps_accum_seconds: f32,
    fps_accum_frames: u32,
}

impl AppState for KajiyaDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let mut render_assets = ctx.render_assets_mut();

        let cube_mesh = render_assets.insert_mesh(cube_mesh());
        let floor_mesh = render_assets.insert_mesh(floor_mesh());
        let cube_material = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.15, 0.72, 0.82))
                .roughness(0.42)
                .metallic(0.05),
        );
        let floor_material = render_assets.insert_standard_material(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.72, 0.70, 0.64))
                .roughness(0.9),
        );
        drop(render_assets);

        let cube = ctx.world.spawn((
            Transform::from_xyz(0.0, 0.7, 0.0).with_scale3(1.25, 1.25, 1.25),
            MeshRenderer::new(cube_mesh, cube_material),
        ));
        self.cube = Some(cube);

        ctx.world.spawn((
            Transform::from_xyz(0.0, -0.05, 0.0),
            MeshRenderer::new(floor_mesh, floor_material),
        ));

        let camera_pitch = -0.28;
        let camera = ctx.world.spawn((
            Transform::from_xyz(0.0, 2.2, 6.0).with_euler_angles(camera_pitch, 0.0, 0.0),
            CameraMarker::new(),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        ));
        self.camera = Some(camera);
        self.camera_pitch = camera_pitch;

        ctx.world.spawn((
            Transform::default(),
            DirectionalLight::new([0.35, -1.0, -0.25]).intensity(2.5),
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.time += ctx.dt();
        if let Some(cube) = self.cube {
            if let Some(transform) = ctx.world.get_mut::<Transform>(cube) {
                transform.rotation = (Quat::from_rotation_y(self.time * 0.55)
                    * Quat::from_rotation_x(0.18))
                .normalized();
                transform.position = Vec3::new(0.0, 0.75 + (self.time * 1.3).sin() * 0.08, 0.0);
            }
        }

        self.update_camera(ctx);
        ctx.render();
        self.update_frame_stats(ctx);
    }
}

impl KajiyaDemo {
    fn update_camera(&mut self, ctx: &mut FrameContext<'_>) {
        const LOOK_SPEED_KEYS: f32 = 1.6;
        const LOOK_SPEED_MOUSE: f32 = 0.003;
        const MOVE_SPEED: f32 = 4.0;
        const FAST_MULTIPLIER: f32 = 3.0;
        const PITCH_LIMIT: f32 = 1.45;

        let dt = ctx.dt();
        if ctx.input.key_held(KeyCode::ArrowLeft) {
            self.camera_yaw += LOOK_SPEED_KEYS * dt;
        }
        if ctx.input.key_held(KeyCode::ArrowRight) {
            self.camera_yaw -= LOOK_SPEED_KEYS * dt;
        }
        if ctx.input.key_held(KeyCode::ArrowUp) {
            self.camera_pitch += LOOK_SPEED_KEYS * dt;
        }
        if ctx.input.key_held(KeyCode::ArrowDown) {
            self.camera_pitch -= LOOK_SPEED_KEYS * dt;
        }
        if ctx.input.mouse_button_held(MouseButton::Right) && ctx.input.mouse_in_window() {
            let [dx, dy] = ctx.input.mouse_delta();
            self.camera_yaw -= dx * LOOK_SPEED_MOUSE;
            self.camera_pitch -= dy * LOOK_SPEED_MOUSE;
        }
        self.camera_pitch = self.camera_pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);

        let rotation = Quat::from_euler_angles(self.camera_pitch, self.camera_yaw, 0.0);
        let forward = rotation.rotate_vec3(Vec3::new(0.0, 0.0, -1.0));
        let right = rotation.rotate_vec3(Vec3::new(1.0, 0.0, 0.0));
        let up = Vec3::new(0.0, 1.0, 0.0);

        let mut movement = Vec3::ZERO;
        if ctx.input.key_held(KeyCode::KeyW) {
            movement += forward;
        }
        if ctx.input.key_held(KeyCode::KeyS) {
            movement -= forward;
        }
        if ctx.input.key_held(KeyCode::KeyD) {
            movement += right;
        }
        if ctx.input.key_held(KeyCode::KeyA) {
            movement -= right;
        }
        if ctx.input.key_held(KeyCode::KeyE) || ctx.input.key_held(KeyCode::Space) {
            movement += up;
        }
        if ctx.input.key_held(KeyCode::KeyQ) {
            movement -= up;
        }

        let speed =
            if ctx.input.key_held(KeyCode::ShiftLeft) || ctx.input.key_held(KeyCode::ShiftRight) {
                MOVE_SPEED * FAST_MULTIPLIER
            } else {
                MOVE_SPEED
            };

        if let Some(camera) = self.camera {
            if let Some(transform) = ctx.world.get_mut::<Transform>(camera) {
                transform.rotation = rotation;
                if movement.length_squared() > f32::EPSILON {
                    transform.position += movement.normalized() * speed * dt;
                }
            }
        }
    }

    fn update_frame_stats(&mut self, ctx: &mut FrameContext<'_>) {
        self.fps_accum_seconds += ctx.dt().max(0.0);
        self.fps_accum_frames += 1;
        if self.fps_accum_seconds < 0.5 {
            return;
        }

        let fps = self.fps_accum_frames as f32 / self.fps_accum_seconds.max(0.0001);
        let frame_ms = 1000.0 / fps.max(0.0001);
        let stats = ctx.render_stats();
        ctx.set_title(&format!(
            "SkyEngine Kajiya 3D | {:.0} FPS ({:.2} ms) | views {} meshes {} instances {} uploaded {} | WASD/QE + RMB",
            fps,
            frame_ms,
            stats.view_count,
            stats.resident_render_assets,
            stats.draw_calls,
            stats.uploaded_render_assets
        ));

        self.fps_accum_seconds = 0.0;
        self.fps_accum_frames = 0;
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine Kajiya 3D", 1280, 720))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::kajiya_3d()
                .with_kajiya_settings(KajiyaRendererSettings::viewer_720p()),
        ))
        .unwrap();

    App::new(world).run(KajiyaDemo::default());
}

fn cube_mesh() -> MeshAsset {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    push_face(
        &mut vertices,
        &mut indices,
        [
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ],
        [0.0, 0.0, 1.0],
    );
    push_face(
        &mut vertices,
        &mut indices,
        [
            [1.0, -1.0, -1.0],
            [-1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [1.0, 1.0, -1.0],
        ],
        [0.0, 0.0, -1.0],
    );
    push_face(
        &mut vertices,
        &mut indices,
        [
            [-1.0, -1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [-1.0, 1.0, -1.0],
        ],
        [-1.0, 0.0, 0.0],
    );
    push_face(
        &mut vertices,
        &mut indices,
        [
            [1.0, -1.0, 1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [1.0, 1.0, 1.0],
        ],
        [1.0, 0.0, 0.0],
    );
    push_face(
        &mut vertices,
        &mut indices,
        [
            [-1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
        ],
        [0.0, 1.0, 0.0],
    );
    push_face(
        &mut vertices,
        &mut indices,
        [
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, -1.0, 1.0],
            [-1.0, -1.0, 1.0],
        ],
        [0.0, -1.0, 0.0],
    );

    MeshAsset::from_raw(
        MeshAssetDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            MeshVertexLayout::position_normal_uv(),
            "kajiya_cube",
        )
        .with_indices(MeshIndexData::u32(indices)),
    )
}

fn floor_mesh() -> MeshAsset {
    let vertices = [
        Vertex {
            position: [-5.0, 0.0, -5.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [5.0, 0.0, -5.0],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [5.0, 0.0, 5.0],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-5.0, 0.0, 5.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 1.0],
        },
    ];
    MeshAsset::from_raw(
        MeshAssetDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            MeshVertexLayout::position_normal_uv(),
            "kajiya_floor",
        )
        .with_indices(MeshIndexData::u32([0, 2, 1, 0, 3, 2])),
    )
}

fn push_face(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    positions: [[f32; 3]; 4],
    normal: [f32; 3],
) {
    let base = vertices.len() as u32;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for (position, uv) in positions.into_iter().zip(uvs) {
        vertices.push(Vertex {
            position,
            normal,
            uv,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}
