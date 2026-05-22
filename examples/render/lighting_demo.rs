//! Modern ECS-first 2D lighting demo.
//!
//! ```bash
//! cargo run --example lighting_demo --features app --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::asset::{Assets, TextureAsset};
use sky_engine::ecs::{With, World};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, PointLight, Projection, RenderSettings, SpriteRenderer,
    Transform,
};

const NUM_ORBS: usize = 240;

#[derive(Clone, Copy)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Hue {
    base: f32,
    shift: f32,
}

#[derive(Clone, Copy)]
struct Pulse(f32);

#[derive(Clone, Copy)]
struct MouseLight;

struct LightingDemo {
    orbs_data: Vec<OrbData>,
    fps_smooth: f32,
    frame_count: u32,
}

struct OrbData {
    x: f32,
    y: f32,
    size: f32,
    hue: f32,
    vx: f32,
    vy: f32,
    shift: f32,
    pulse: f32,
    intensity: f32,
    temperature: f32,
    falloff: f32,
}

impl LightingDemo {
    fn new(rng: &mut SimpleRng) -> Self {
        let orbs_data: Vec<OrbData> = (0..NUM_ORBS)
            .map(|_| OrbData {
                x: rng.range(-480.0, 480.0),
                y: rng.range(-320.0, 320.0),
                size: rng.range(14.0, 42.0),
                hue: rng.range(0.0, 360.0),
                vx: rng.range(-32.0, 32.0),
                vy: rng.range(-32.0, 32.0),
                shift: rng.range(10.0, 80.0),
                pulse: rng.range(0.0, std::f32::consts::TAU),
                intensity: rng.range(0.8, 1.8),
                temperature: rng.range(2600.0, 9200.0),
                falloff: rng.range(1.3, 2.1),
            })
            .collect();
        Self {
            orbs_data,
            fps_smooth: 0.0,
            frame_count: 0,
        }
    }
}

impl AppState for LightingDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        let asset_server = world
            .get_resource::<Assets>()
            .expect("App should install Assets before setup")
            .clone();
        let orb_tex = asset_server.insert_runtime(TextureAsset::circle(96));
        for orb in &self.orbs_data {
            let tint = Color::hsl(orb.hue, 0.72, 0.55);
            world.spawn((
                Transform::from_xy(orb.x, orb.y),
                SpriteRenderer::new(orb.size, orb.size)
                    .texture(orb_tex.clone())
                    .color(Color::new(tint.r, tint.g, tint.b, 0.95)),
                PointLight::new(orb.size * 7.0)
                    .intensity(orb.intensity)
                    .temperature(orb.temperature)
                    .color(tint)
                    .falloff(orb.falloff),
                Velocity {
                    x: orb.vx,
                    y: orb.vy,
                },
                Hue {
                    base: orb.hue,
                    shift: orb.shift,
                },
                Pulse(orb.pulse),
            ));
        }
    }

    fn update(&mut self, ctx: &mut FrameContext) {
        let dt = ctx.dt;
        let logical_view_size = ctx.logical_view_size();

        let mut camera_query = ctx
            .world
            .query_filtered::<(&Transform, &Projection), With<MainCamera>>();
        let mut camera_transform = None;
        let mut projection = None;
        camera_query.for_each(&mut *ctx.world, |(transform, camera_projection)| {
            if camera_transform.is_none() {
                camera_transform = Some(*transform);
                projection = Some(*camera_projection);
            }
        });
        let mouse = ctx.input.mouse_logical_position();
        let projection =
            projection.unwrap_or_else(|| Projection::orthographic(logical_view_size.height));
        let camera_transform = camera_transform.unwrap_or_default();
        let mouse_world =
            projection.screen_to_world_logical(camera_transform, logical_view_size, mouse);

        let mut orbs = ctx.world.query::<(
            &mut Transform,
            &mut SpriteRenderer,
            &mut PointLight,
            &Velocity,
            &Hue,
            &mut Pulse,
        )>();
        orbs.for_each(
            &mut *ctx.world,
            |(transform, sprite, light, velocity, hue, pulse)| {
                transform.position[0] += velocity.x * dt;
                transform.position[1] += velocity.y * dt;
                pulse.0 += dt * 0.8;

                let half_w = logical_view_size.width * 0.5 + sprite.width;
                let half_h = logical_view_size.height * 0.5 + sprite.height;
                if transform.position[0] > half_w {
                    transform.position[0] = -half_w;
                }
                if transform.position[0] < -half_w {
                    transform.position[0] = half_w;
                }
                if transform.position[1] > half_h {
                    transform.position[1] = -half_h;
                }
                if transform.position[1] < -half_h {
                    transform.position[1] = half_h;
                }

                let pulse_scale = 1.0 + 0.18 * pulse.0.sin();
                transform.scale[0] = pulse_scale;
                transform.scale[1] = pulse_scale;

                let shifted_hue = (hue.base + hue.shift * dt + pulse.0 * 4.0) % 360.0;
                let lightness = 0.46 + 0.14 * (pulse.0 * 0.7).cos();
                let tint = Color::hsl(shifted_hue, 0.72, lightness);
                sprite.color = Color::new(tint.r, tint.g, tint.b, 0.95);
                light.color = tint;
            },
        );

        let mut mouse_light = ctx
            .world
            .query::<(&mut Transform, &mut PointLight, &MouseLight)>();
        mouse_light.for_each(&mut *ctx.world, |(transform, light, _)| {
            transform.position[0] = mouse_world[0];
            transform.position[1] = mouse_world[1];
            light.intensity = 1.8 + 0.25 * (dt * 60.0).sin().abs();
        });

        ctx.render();

        let fps_instant = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        self.fps_smooth = if self.fps_smooth == 0.0 {
            fps_instant
        } else {
            self.fps_smooth * 0.95 + fps_instant * 0.05
        };
        self.frame_count += 1;
        if self.frame_count % 30 == 0 {
            let stats = ctx.render_stats();
            ctx.set_title(&format!(
                "SkyEngine — ECS Lighting Demo | {:.0} FPS | {} sprites | {} lights",
                self.fps_smooth, stats.sprite_count, stats.light_count
            ));
        }
    }
}

fn main() {
    let mut rng = SimpleRng::new(1337);

    let mut world = World::new();
    world.insert_resource(RenderSettings::default());
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic(720.0),
        MainCamera,
    ));

    // Mouse light entity.
    world.spawn((
        Transform::from_xy(0.0, 0.0),
        PointLight::new(150.0)
            .intensity(1.8)
            .temperature(5000.0)
            .color(Color::rgb(1.0, 0.95, 0.85))
            .falloff(1.4),
        MouseLight,
    ));

    world
        .install(WindowPlugin::new(
            "SkyEngine — ECS Lighting Demo",
            1280,
            720,
        ))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::forward_2d()).unwrap();

    App::new(world).run(LightingDemo::new(&mut rng));
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}
