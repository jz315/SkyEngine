//! Forward 3D scene demo.
//!
//! Shows the public high-level 3D path:
//! - perspective camera
//! - `RenderPipelineAsset::modern_3d()`
//! - `StandardMaterial`
//! - `DirectionalLight` shadowing
//! - animated point lights and mesh instances
//!
//! Controls:
//! - `A` / `D` or left / right: orbit camera
//! - up / down: tilt camera
//! - `W` / `S` or mouse wheel: zoom
//! - `Space`: toggle auto orbit
//! - `0`-`4`: GI debug off / probes / irradiance / visibility / ray budget
//! - `F1`: lit scene
//! - `F2`-`F5`: raw directional shadow cascade 0-3
//! - `F6`: directional shadow cascade coverage overlay
//!
//! ```bash
//! cargo run --example three_d_demo --features app --release
//! ```

use glam::{Mat3 as GlamMat3, Quat as GlamQuat, Vec3 as GlamVec3};
use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::{With, World};
use sky_engine::input::KeyCode;
use sky_engine::math::{Quat, Vec3};
use sky_engine::render::expert::{BoundingSphere, Mesh, MeshDescriptor, MeshHandle, MeshIndexData};
use sky_engine::render::{
    BloomSettings, CameraMarker, Color, DdgiSettings, DdgiVolumeSettings, DirectionalLight,
    GiDebugMode, GlobalIlluminationMode, GlobalIlluminationSettings, MainCamera, MaterialHandle,
    PointLight, Projection, RenderDebugView, RenderPipelineAsset, RenderSettings, SharpenSettings,
    SpotLight, SsgiSettings, StandardMaterial, Texture, ToneMapSettings, Transform,
    WgpuMeshRenderer,
};

const GROUND_Y: f32 = -1.25;
const CAMERA_FOCUS: [f32; 3] = [0.0, 1.25, -1.8];
const CAMERA_PITCH_MIN: f32 = -0.85;
const CAMERA_PITCH_MAX: f32 = 0.08;
const CAMERA_DISTANCE_MIN: f32 = 7.0;
const CAMERA_DISTANCE_MAX: f32 = 20.0;

#[derive(Clone, Copy)]
struct ShowcaseBlock {
    base_position: [f32; 3],
    scale: [f32; 3],
    yaw_speed: f32,
    bob_amplitude: f32,
    bob_speed: f32,
    phase: f32,
    tilt: f32,
}

#[derive(Clone, Copy)]
struct OrbitLight {
    radius: f32,
    height: f32,
    speed: f32,
    phase: f32,
    vertical_amplitude: f32,
    base_intensity: f32,
}

struct ThreeDDemo {
    initialized: bool,
    time: f32,
    fps_smooth: f32,
    frame_count: u32,
    auto_orbit: bool,
    camera_yaw: f32,
    camera_pitch: f32,
    camera_distance: f32,
    gi_debug: GiDebugMode,
    shadow_debug: RenderDebugView,
}

impl Default for ThreeDDemo {
    fn default() -> Self {
        Self {
            initialized: false,
            time: 0.0,
            fps_smooth: 0.0,
            frame_count: 0,
            auto_orbit: false,
            camera_yaw: 0.0,
            camera_pitch: -0.18,
            camera_distance: 13.0,
            gi_debug: GiDebugMode::Off,
            shadow_debug: RenderDebugView::None,
        }
    }
}

impl AppState for ThreeDDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        self.time += ctx.dt;

        if !self.initialized {
            initialize_scene(ctx);
            self.initialized = true;
        }

        if ctx.input.key_pressed(KeyCode::Space) {
            self.auto_orbit = !self.auto_orbit;
        }
        if let Some(debug_mode) = gi_debug_mode_from_input(ctx) {
            self.gi_debug = debug_mode;
            if let Some(settings) = ctx.world.get_resource_mut::<RenderSettings>() {
                settings.global_illumination.debug = debug_mode;
            }
        }
        if let Some(debug_view) = shadow_debug_view_from_input(ctx) {
            self.shadow_debug = debug_view;
            if let Some(settings) = ctx.world.get_resource_mut::<RenderSettings>() {
                settings.debug_view = debug_view;
            }
        }

        let orbit_speed = 0.85;
        if self.auto_orbit {
            self.camera_yaw += ctx.dt * 0.28;
        }
        if ctx.input.key_held(KeyCode::KeyA) || ctx.input.key_held(KeyCode::ArrowLeft) {
            self.camera_yaw -= orbit_speed * ctx.dt;
            self.auto_orbit = false;
        }
        if ctx.input.key_held(KeyCode::KeyD) || ctx.input.key_held(KeyCode::ArrowRight) {
            self.camera_yaw += orbit_speed * ctx.dt;
            self.auto_orbit = false;
        }
        if ctx.input.key_held(KeyCode::ArrowUp) {
            self.camera_pitch += 0.75 * ctx.dt;
            self.auto_orbit = false;
        }
        if ctx.input.key_held(KeyCode::ArrowDown) {
            self.camera_pitch -= 0.75 * ctx.dt;
            self.auto_orbit = false;
        }
        if ctx.input.key_held(KeyCode::KeyW) {
            self.camera_distance -= 7.5 * ctx.dt;
            self.auto_orbit = false;
        }
        if ctx.input.key_held(KeyCode::KeyS) {
            self.camera_distance += 7.5 * ctx.dt;
            self.auto_orbit = false;
        }

        self.camera_distance -= ctx.input.scroll_delta()[1] * 0.45;
        self.camera_pitch = self.camera_pitch.clamp(CAMERA_PITCH_MIN, CAMERA_PITCH_MAX);
        self.camera_distance = self
            .camera_distance
            .clamp(CAMERA_DISTANCE_MIN, CAMERA_DISTANCE_MAX);

        update_camera(
            ctx,
            self.camera_yaw,
            self.camera_pitch,
            self.camera_distance,
        );
        animate_blocks(ctx, self.time);
        animate_lights(ctx, self.time);

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
            let orbit_mode = if self.auto_orbit { "auto" } else { "manual" };
            ctx.set_title(&format!(
                "SkyEngine — 3D Demo | {:.0} FPS | {} draws ({} shadow) | {} lights | CSM {}x {}x{} | camera {orbit_mode} | GI {} | shadows {}",
                self.fps_smooth,
                stats.draw_calls,
                stats.shadow_draw_calls,
                stats.light_count,
                stats.shadow_cascade_count,
                stats.shadow_atlas_width,
                stats.shadow_atlas_height,
                gi_debug_name(self.gi_debug),
                shadow_debug_name(self.shadow_debug)
            ));
        }
    }
}

fn gi_debug_mode_from_input(ctx: &FrameContext) -> Option<GiDebugMode> {
    if ctx.input.key_pressed(KeyCode::Digit0) {
        Some(GiDebugMode::Off)
    } else if ctx.input.key_pressed(KeyCode::Digit1) {
        Some(GiDebugMode::Probes)
    } else if ctx.input.key_pressed(KeyCode::Digit2) {
        Some(GiDebugMode::Irradiance)
    } else if ctx.input.key_pressed(KeyCode::Digit3) {
        Some(GiDebugMode::Visibility)
    } else if ctx.input.key_pressed(KeyCode::Digit4) {
        Some(GiDebugMode::RayBudget)
    } else {
        None
    }
}

fn shadow_debug_view_from_input(ctx: &FrameContext) -> Option<RenderDebugView> {
    if ctx.input.key_pressed(KeyCode::F1) {
        Some(RenderDebugView::None)
    } else if ctx.input.key_pressed(KeyCode::F2) {
        Some(RenderDebugView::DirectionalShadowCascade(0))
    } else if ctx.input.key_pressed(KeyCode::F3) {
        Some(RenderDebugView::DirectionalShadowCascade(1))
    } else if ctx.input.key_pressed(KeyCode::F4) {
        Some(RenderDebugView::DirectionalShadowCascade(2))
    } else if ctx.input.key_pressed(KeyCode::F5) {
        Some(RenderDebugView::DirectionalShadowCascade(3))
    } else if ctx.input.key_pressed(KeyCode::F6) {
        Some(RenderDebugView::DirectionalShadowCoverage)
    } else {
        None
    }
}

fn gi_debug_name(mode: GiDebugMode) -> &'static str {
    match mode {
        GiDebugMode::Off => "off",
        GiDebugMode::Probes => "probes",
        GiDebugMode::Irradiance => "irradiance",
        GiDebugMode::Visibility => "visibility",
        GiDebugMode::RayBudget => "ray budget",
    }
}

fn shadow_debug_name(view: RenderDebugView) -> &'static str {
    match view {
        RenderDebugView::None => "lit",
        RenderDebugView::DirectionalShadowMap => "shadow map",
        RenderDebugView::DirectionalShadowCascade(0) => "cascade 0",
        RenderDebugView::DirectionalShadowCascade(1) => "cascade 1",
        RenderDebugView::DirectionalShadowCascade(2) => "cascade 2",
        RenderDebugView::DirectionalShadowCascade(3) => "cascade 3",
        RenderDebugView::DirectionalShadowCascade(_) => "cascade",
        RenderDebugView::DirectionalShadowCoverage => "coverage",
        _ => "debug",
    }
}

fn initialize_scene(ctx: &mut FrameContext) {
    let assets = ctx
        .with_renderer_mut(|renderer, gpu| {
            let cube_mesh = renderer.insert_mesh(create_box_mesh(gpu, "three_d_demo_cube"));
            let plane_mesh = renderer.insert_mesh(create_ground_mesh(gpu, "three_d_demo_ground"));

            let ground_texture = create_soft_floor_texture(gpu, 256);
            let wall_texture = create_soft_wall_texture(gpu, 256);
            let floor_normal_texture =
                create_normal_texture(gpu, 256, "three_d_demo_floor_normal", sample_floor_height);
            let wall_normal_texture =
                create_normal_texture(gpu, 256, "three_d_demo_wall_normal", sample_wall_height);
            let block_normal_texture =
                create_normal_texture(gpu, 256, "three_d_demo_block_normal", sample_block_height);

            let floor = renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo: Color::rgb(0.46, 0.48, 0.52),
                    albedo_texture: Some(ground_texture),
                    normal_texture: Some(floor_normal_texture.clone()),
                    roughness: 0.94,
                    metallic: 0.0,
                    ..Default::default()
                });
            let plaster = renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo_texture: Some(wall_texture.clone()),
                    normal_texture: Some(wall_normal_texture.clone()),
                    albedo: Color::rgb(0.67, 0.68, 0.70),
                    roughness: 0.92,
                    metallic: 0.0,
                    ..Default::default()
                });
            let warm_wall = renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo_texture: Some(wall_texture.clone()),
                    normal_texture: Some(wall_normal_texture.clone()),
                    albedo: Color::rgb(0.56, 0.26, 0.20),
                    roughness: 0.90,
                    metallic: 0.0,
                    ..Default::default()
                });
            let cool_wall = renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo_texture: Some(wall_texture.clone()),
                    normal_texture: Some(wall_normal_texture.clone()),
                    albedo: Color::rgb(0.18, 0.32, 0.38),
                    roughness: 0.88,
                    metallic: 0.0,
                    ..Default::default()
                });
            let charcoal = renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo_texture: Some(wall_texture.clone()),
                    normal_texture: Some(wall_normal_texture.clone()),
                    albedo: Color::rgb(0.10, 0.11, 0.12),
                    roughness: 0.94,
                    metallic: 0.02,
                    ..Default::default()
                });
            let stone = renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo_texture: Some(wall_texture.clone()),
                    normal_texture: Some(block_normal_texture),
                    albedo: Color::rgb(0.49, 0.53, 0.58),
                    roughness: 0.84,
                    metallic: 0.02,
                    ..Default::default()
                });
            let bronze = renderer
                .materials_mut::<StandardMaterial>()
                .insert(StandardMaterial {
                    albedo: Color::rgb(0.68, 0.56, 0.34),
                    roughness: 0.28,
                    metallic: 0.66,
                    ..Default::default()
                });
            let teal_emissive =
                renderer
                    .materials_mut::<StandardMaterial>()
                    .insert(StandardMaterial {
                        albedo: Color::rgb(0.03, 0.08, 0.10),
                        emissive: Color::rgb(0.16, 0.82, 1.08),
                        roughness: 0.18,
                        metallic: 0.04,
                        ..Default::default()
                    });
            let amber_emissive =
                renderer
                    .materials_mut::<StandardMaterial>()
                    .insert(StandardMaterial {
                        albedo: Color::rgb(0.10, 0.07, 0.03),
                        emissive: Color::rgb(1.25, 0.72, 0.24),
                        roughness: 0.18,
                        metallic: 0.04,
                        ..Default::default()
                    });

            SceneAssets {
                cube_mesh,
                plane_mesh,
                floor,
                plaster,
                warm_wall,
                cool_wall,
                charcoal,
                stone,
                bronze,
                teal_emissive,
                amber_emissive,
            }
        })
        .expect("three_d_demo requires App::with_render_pipeline(...)");

    ctx.world.spawn((
        Transform::from_xyz(0.0, GROUND_Y, -0.8).with_scale3(14.5, 1.0, 20.0),
        WgpuMeshRenderer::new(assets.plane_mesh, assets.floor),
    ));

    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.charcoal,
        [0.0, GROUND_Y + 6.05, -0.8],
        [14.8, 0.18, 20.2],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.plaster,
        [0.0, GROUND_Y + 3.05, -10.2],
        [14.8, 6.3, 0.18],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.warm_wall,
        [-7.35, GROUND_Y + 3.05, -0.8],
        [0.18, 6.3, 20.2],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.cool_wall,
        [7.35, GROUND_Y + 3.05, -0.8],
        [0.18, 6.3, 20.2],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.charcoal,
        [-5.1, GROUND_Y + 3.0, 8.7],
        [0.75, 6.0, 2.7],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.charcoal,
        [5.1, GROUND_Y + 3.0, 8.7],
        [0.75, 6.0, 2.7],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.charcoal,
        [0.0, GROUND_Y + 5.05, 7.9],
        [3.8, 0.28, 4.3],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.bronze,
        [0.0, GROUND_Y + 1.55, -2.2],
        [1.9, 2.9, 1.9],
        [0.0, 0.28, 0.0],
    );

    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.stone,
        [-3.0, GROUND_Y + 0.72, -4.8],
        [1.6, 1.45, 1.6],
        [0.0, 0.30, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.stone,
        [2.9, GROUND_Y + 1.18, -5.4],
        [1.2, 2.35, 1.2],
        [0.0, -0.18, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.charcoal,
        [-4.0, GROUND_Y + 0.35, 1.3],
        [2.5, 0.7, 1.1],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.plaster,
        [3.9, GROUND_Y + 0.52, 1.9],
        [1.7, 1.05, 1.7],
        [0.0, 0.36, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.charcoal,
        [0.0, GROUND_Y + 4.1, -3.2],
        [6.0, 0.22, 0.38],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.teal_emissive,
        [-6.85, GROUND_Y + 2.8, -2.0],
        [0.09, 3.8, 8.8],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.amber_emissive,
        [6.85, GROUND_Y + 2.75, -1.0],
        [0.09, 3.6, 8.2],
        [0.0, 0.0, 0.0],
    );
    spawn_static_box(
        ctx.world,
        assets.cube_mesh,
        assets.amber_emissive,
        [0.0, GROUND_Y + 4.55, -9.65],
        [7.2, 0.10, 0.22],
        [0.0, 0.0, 0.0],
    );
    ctx.world.spawn((
        Transform::from_xyz(-4.1, GROUND_Y + 3.75, -5.4).with_scale3(0.35, 0.35, 0.35),
        WgpuMeshRenderer::new(assets.cube_mesh, assets.teal_emissive),
        PointLight::new(7.0)
            .intensity(0.62)
            .color(Color::rgb(0.28, 0.92, 1.0))
            .falloff(1.55),
    ));
    ctx.world.spawn((
        Transform::from_xyz(3.8, GROUND_Y + 3.45, -3.7).with_scale3(0.35, 0.35, 0.35),
        WgpuMeshRenderer::new(assets.cube_mesh, assets.amber_emissive),
        PointLight::new(7.0)
            .intensity(0.58)
            .color(Color::rgb(1.0, 0.76, 0.28))
            .falloff(1.45),
    ));
    ctx.world.spawn((
        Transform::from_xyz(0.0, GROUND_Y + 4.45, 2.6).with_scale3(0.28, 0.28, 0.28),
        WgpuMeshRenderer::new(assets.cube_mesh, assets.plaster),
        PointLight::new(9.0)
            .intensity(0.16)
            .color(Color::rgb(1.0, 0.97, 0.90))
            .falloff(1.10),
    ));
    ctx.world.spawn((
        Transform::from_xyz(-1.8, GROUND_Y + 6.2, -2.1).with_scale3(0.24, 0.24, 0.24),
        WgpuMeshRenderer::new(assets.cube_mesh, assets.teal_emissive),
        SpotLight::new(11.0)
            .intensity(1.15)
            .color(Color::rgb(0.62, 0.86, 1.0))
            .direction([0.28, -0.92, -0.26])
            .cone_angles(18.0_f32.to_radians(), 34.0_f32.to_radians())
            .falloff(1.8)
            .shadow_resolution(1024),
    ));

    ctx.world.spawn((DirectionalLight::new([0.58, -1.0, 0.34])
        .intensity(1.88)
        .color(Color::rgb(1.0, 0.96, 0.90))
        .cascade_count(4)
        .cascade_distances([7.5, 18.0, 38.0, 80.0])
        .cascade_blend(0.12)
        .shadow_resolution_per_cascade(1536)
        .shadow_bias(0.0008)
        .shadow_depth_bias(3)
        .shadow_slope_bias(1.8)
        .shadow_normal_bias(0.018)
        .shadow_filter_radius(0.035),));
}

fn update_camera(ctx: &mut FrameContext, yaw: f32, pitch: f32, distance: f32) {
    let cos_pitch = pitch.cos();
    let focus = GlamVec3::from_array(CAMERA_FOCUS);
    let position = focus
        + GlamVec3::new(
            yaw.sin() * distance * cos_pitch,
            -pitch.sin() * distance,
            yaw.cos() * distance * cos_pitch,
        );
    let forward = (focus - position).normalize_or_zero();
    let right = forward.cross(GlamVec3::Y).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    let basis = GlamMat3::from_cols(right, up, -forward);
    let rotation = Quat::from_xyzw_array(GlamQuat::from_mat3(&basis).to_array());

    let mut cameras = ctx
        .world
        .query_filtered::<&mut Transform, With<MainCamera>>();
    cameras.for_each(ctx.world, |transform| {
        *transform =
            Transform::from_xyz(position.x, position.y, position.z).with_rotation_quat(rotation);
    });
}

fn animate_blocks(ctx: &mut FrameContext, time: f32) {
    let mut blocks = ctx.world.query::<(&mut Transform, &ShowcaseBlock)>();
    blocks.for_each(ctx.world, |(transform, block)| {
        let bob = block.bob_amplitude * (time * block.bob_speed + block.phase).sin();
        let pitch = block.tilt * (time * (block.bob_speed * 0.75) + block.phase * 1.3).sin();
        let roll = block.tilt * 0.65 * (time * 0.55 + block.phase).cos();
        let yaw = time * block.yaw_speed + block.phase;
        transform.position = Vec3::new(
            block.base_position[0],
            block.base_position[1] + bob,
            block.base_position[2],
        );
        transform.scale = Vec3::new(block.scale[0], block.scale[1], block.scale[2]);
        transform.rotation = Quat::from_euler_angles(pitch, yaw, roll);
    });
}

fn animate_lights(ctx: &mut FrameContext, time: f32) {
    let mut lights = ctx
        .world
        .query::<(&mut Transform, &mut PointLight, &OrbitLight)>();
    lights.for_each(ctx.world, |(transform, light, orbit)| {
        let angle = time * orbit.speed + orbit.phase;
        transform.position = Vec3::new(
            angle.cos() * orbit.radius,
            orbit.height + orbit.vertical_amplitude * (angle * 1.6).sin(),
            angle.sin() * orbit.radius,
        );
        light.intensity = orbit.base_intensity * (0.84 + 0.16 * (angle * 2.3).cos().abs());
    });
}

#[allow(dead_code)]
fn spawn_showcase_block(
    world: &mut World,
    mesh: MeshHandle,
    material: MaterialHandle,
    block: ShowcaseBlock,
) {
    world.spawn((
        Transform::from_xyz(
            block.base_position[0],
            block.base_position[1],
            block.base_position[2],
        )
        .with_scale3(block.scale[0], block.scale[1], block.scale[2]),
        WgpuMeshRenderer::new(mesh, material),
        block,
    ));
}

fn spawn_static_box(
    world: &mut World,
    mesh: MeshHandle,
    material: MaterialHandle,
    position: [f32; 3],
    scale: [f32; 3],
    euler_angles: [f32; 3],
) {
    world.spawn((
        Transform::from_xyz(position[0], position[1], position[2])
            .with_scale3(scale[0], scale[1], scale[2])
            .with_euler_angles(euler_angles[0], euler_angles[1], euler_angles[2]),
        WgpuMeshRenderer::new(mesh, material),
    ));
}

#[allow(dead_code)]
fn spawn_orbit_light(
    world: &mut World,
    mesh: MeshHandle,
    material: MaterialHandle,
    light: PointLight,
    orbit: OrbitLight,
) {
    world.spawn((
        Transform::from_xyz(orbit.radius, orbit.height, 0.0).with_scale3(0.35, 0.35, 0.35),
        WgpuMeshRenderer::new(mesh, material),
        light,
        orbit,
    ));
}

fn create_soft_floor_texture(ctx: &sky_engine::gpu::GpuContext, size: u32) -> Texture {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let max = (size.saturating_sub(1)).max(1) as f32;

    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / max;
            let v = y as f32 / max;
            let px = u * 2.0 - 1.0;
            let py = v * 2.0 - 1.0;

            let radial = 1.0 - (px * px + py * py).min(1.0);
            let band_x = 0.5 + 0.5 * (u * std::f32::consts::TAU * 2.0).cos();
            let band_y = 0.5 + 0.5 * (v * std::f32::consts::TAU * 2.0).sin();
            let tile_x = ((u * 4.0).fract() - 0.5).abs();
            let tile_y = ((v * 4.0).fract() - 0.5).abs();
            let grout = ((tile_x.max(tile_y) - 0.43) / 0.07).clamp(0.0, 1.0);
            let shade = 0.70 + radial * 0.18 + band_x * 0.05 + band_y * 0.04 - grout * 0.10;

            let r = (shade * 175.0 + 14.0).clamp(0.0, 255.0) as u8;
            let g = (shade * 183.0 + 16.0).clamp(0.0, 255.0) as u8;
            let b = (shade * 194.0 + 18.0).clamp(0.0, 255.0) as u8;
            let idx = ((y * size + x) * 4) as usize;
            data[idx] = r;
            data[idx + 1] = g;
            data[idx + 2] = b;
            data[idx + 3] = 255;
        }
    }

    Texture::from_rgba8(ctx, size, size, &data)
}

fn create_soft_wall_texture(ctx: &sky_engine::gpu::GpuContext, size: u32) -> Texture {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let max = (size.saturating_sub(1)).max(1) as f32;

    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / max;
            let v = y as f32 / max;
            let wave_a = 0.5 + 0.5 * ((u * 5.0 + v * 1.7) * std::f32::consts::TAU).sin();
            let wave_b = 0.5 + 0.5 * ((v * 7.0 - u * 2.1) * std::f32::consts::TAU).cos();
            let pores = 0.5 + 0.5 * ((u * 31.0).sin() * (v * 27.0).cos());
            let seam = (((u * 3.0).fract() - 0.5).abs() / 0.5).powf(2.6);
            let shade = 0.86 + wave_a * 0.05 + wave_b * 0.04 + pores * 0.03 - seam * 0.05;

            let value = (shade * 255.0).clamp(0.0, 255.0) as u8;
            let idx = ((y * size + x) * 4) as usize;
            data[idx] = value;
            data[idx + 1] = value;
            data[idx + 2] = value;
            data[idx + 3] = 255;
        }
    }

    Texture::from_rgba8(ctx, size, size, &data)
}

fn sample_floor_height(u: f32, v: f32) -> f32 {
    let tile_x = ((u * 4.0).fract() - 0.5).abs();
    let tile_y = ((v * 4.0).fract() - 0.5).abs();
    let grout = ((tile_x.max(tile_y) - 0.42) / 0.08).clamp(0.0, 1.0);
    let ripple = 0.5 + 0.5 * ((u * 6.0 + v * 2.5) * std::f32::consts::TAU).sin();
    let chips = 0.5 + 0.5 * ((u * 34.0).sin() * (v * 30.0).cos());
    ripple * 0.12 + chips * 0.05 - grout * 0.22
}

fn sample_wall_height(u: f32, v: f32) -> f32 {
    let plaster = 0.5 + 0.5 * ((u * 4.0 + v * 1.6) * std::f32::consts::TAU).sin();
    let streak = 0.5 + 0.5 * ((v * 7.0 - u * 1.5) * std::f32::consts::TAU).cos();
    let pores = 0.5 + 0.5 * ((u * 23.0).sin() * (v * 19.0).cos());
    let seams = 1.0 - (((u * 3.0).fract() - 0.5).abs() / 0.5).clamp(0.0, 1.0);
    plaster * 0.10 + streak * 0.08 + pores * 0.03 + seams * 0.02
}

fn sample_block_height(u: f32, v: f32) -> f32 {
    let chisel = 0.5 + 0.5 * ((u * 10.0 + v * 2.8) * std::f32::consts::TAU).sin();
    let grain = 0.5 + 0.5 * ((u * 22.0).sin() * (v * 12.0).sin());
    let pits = 0.5 + 0.5 * ((u * 41.0).cos() * (v * 37.0).sin());
    chisel * 0.10 + grain * 0.05 + pits * 0.03
}

fn create_normal_texture(
    ctx: &sky_engine::gpu::GpuContext,
    size: u32,
    label: &'static str,
    sample_height: fn(f32, f32) -> f32,
) -> Texture {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let max = (size.saturating_sub(1)).max(1) as f32;
    let du = 1.0 / max;
    let dv = 1.0 / max;

    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / max;
            let v = y as f32 / max;
            let height_l = sample_height((u - du).clamp(0.0, 1.0), v);
            let height_r = sample_height((u + du).clamp(0.0, 1.0), v);
            let height_d = sample_height(u, (v - dv).clamp(0.0, 1.0));
            let height_u = sample_height(u, (v + dv).clamp(0.0, 1.0));
            let normal = GlamVec3::new(
                (height_l - height_r) * 2.6,
                (height_d - height_u) * 2.6,
                1.0,
            )
            .normalize();

            let idx = ((y * size + x) * 4) as usize;
            data[idx] = ((normal.x * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            data[idx + 1] = ((normal.y * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            data[idx + 2] = ((normal.z * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            data[idx + 3] = 255;
        }
    }

    Texture::from_rgba8_with_format(
        ctx,
        size,
        size,
        &data,
        wgpu::TextureFormat::Rgba8Unorm,
        label,
    )
}

fn create_ground_mesh(ctx: &sky_engine::gpu::GpuContext, label: &'static str) -> Mesh {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        tangent: [f32; 4],
        uv: [f32; 2],
    }

    let vertices = [
        Vertex {
            position: [-0.5, 0.0, -0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.0, -0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.0, 0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.0, 0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];

    Mesh::from_raw(
        ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_tangent_uv(),
            label,
        )
        .with_indices(MeshIndexData::U16(&indices))
        .with_bounding_sphere(BoundingSphere::new(
            [0.0, 0.0, 0.0],
            (0.5f32 * 0.5 + 0.5 * 0.5).sqrt(),
        )),
    )
}

fn create_box_mesh(ctx: &sky_engine::gpu::GpuContext, label: &'static str) -> Mesh {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        tangent: [f32; 4],
        uv: [f32; 2],
    }

    let vertices = [
        // Front (+Z)
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        // Back (-Z)
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            tangent: [-1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            tangent: [-1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            tangent: [-1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            tangent: [-1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        // Left (-X)
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [-1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 1.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 1.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 1.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [-1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 1.0, 1.0],
            uv: [0.0, 0.0],
        },
        // Right (+X)
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, -1.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, -1.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, -1.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            tangent: [0.0, 0.0, -1.0, 1.0],
            uv: [0.0, 0.0],
        },
        // Top (+Y)
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        // Bottom (-Y)
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];

    let indices: [u16; 36] = [
        0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17,
        18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
    ];

    Mesh::from_raw(
        ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_tangent_uv(),
            label,
        )
        .with_indices(MeshIndexData::U16(&indices))
        .with_bounding_sphere(BoundingSphere::new(
            [0.0, 0.0, 0.0],
            (0.5f32 * 0.5 + 0.5 * 0.5 + 0.5 * 0.5).sqrt(),
        )),
    )
}

struct SceneAssets {
    cube_mesh: MeshHandle,
    plane_mesh: MeshHandle,
    floor: MaterialHandle,
    plaster: MaterialHandle,
    warm_wall: MaterialHandle,
    cool_wall: MaterialHandle,
    charcoal: MaterialHandle,
    stone: MaterialHandle,
    bronze: MaterialHandle,
    teal_emissive: MaterialHandle,
    amber_emissive: MaterialHandle,
}

fn main() {
    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: Color::rgb(0.0014, 0.0018, 0.0024),
        ambient_color: Color::rgb(0.007, 0.009, 0.012),
        global_illumination: GlobalIlluminationSettings {
            enabled: true,
            mode: GlobalIlluminationMode::Ssgi,
            ssgi: SsgiSettings {
                intensity: 1.0,
                radius_pixels: 8.0,
                depth_rejection: 8.0,
                normal_power: 64.0,
            },
            ddgi: DdgiSettings {
                volume: DdgiVolumeSettings {
                    origin: [-12.0, -1.25, -14.0],
                    spacing: 1.85,
                    counts: [14, 8, 18],
                    scroll_with_main_camera: true,
                },
                rays_per_probe: 64,
                probes_per_frame: 128,
                hysteresis: 0.92,
                normal_bias: 0.08,
                view_bias: 0.20,
                max_ray_distance: 42.0,
                irradiance_resolution: 6,
                visibility_resolution: 6,
                bounces: 2,
            },
            debug: GiDebugMode::Off,
        },
        bloom: BloomSettings {
            enabled: true,
            threshold: 1.60,
            intensity: 0.06,
            radius: 0.40,
        },
        sharpen: SharpenSettings {
            enabled: true,
            strength: 0.32,
            clamp: 0.075,
        },
        tonemap: ToneMapSettings {
            enabled: true,
            exposure: 1.10,
            gamma: 2.2,
        },
        ..Default::default()
    });
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 80.0),
        MainCamera,
    ));

    App::new(AppConfig::new("SkyEngine — 3D Demo", 1280, 720), world)
        .with_render_pipeline(RenderPipelineAsset::modern_3d())
        .run(ThreeDDemo::default());
}
