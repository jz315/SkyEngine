//! Modern ECS-first 2D lighting demo.
//!
//! ```bash
//! cargo run --example lighting_demo --features app --release
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::{With, World};
use sky_engine::gpu::GpuContext;
use sky_engine::render::{
    Camera2D, Color, PointLight2D, PrimaryCamera2D, RenderSettings2D, Renderer2DConfig, Sprite2D,
    Texture, Transform2D,
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
    fn setup(&mut self, world: &mut World, gpu: &mut GpuContext) {
        let orb_tex = Texture::circle(gpu, 96);
        for orb in &self.orbs_data {
            let tint = Color::hsl(orb.hue, 0.72, 0.55);
            world.spawn((
                Transform2D::new(orb.x, orb.y),
                Sprite2D::new(orb.size, orb.size)
                    .texture(orb_tex.clone())
                    .color(Color::new(tint.r, tint.g, tint.b, 0.95)),
                PointLight2D::new(orb.size * 7.0)
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
        let [w, h] = ctx.surface_size();

        let mut camera_query = ctx
            .world
            .query_filtered::<&Camera2D, With<PrimaryCamera2D>>();
        let mut camera = None;
        camera_query.for_each(ctx.world, |cam| {
            if camera.is_none() {
                camera = Some(*cam);
            }
        });
        let mut camera = camera.unwrap_or_else(|| Camera2D::new(w as f32, h as f32));
        camera.set_viewport(w as f32, h as f32);
        let mouse = ctx.input.mouse_position();
        let mouse_world = camera.screen_to_world(mouse[0], mouse[1]);

        let mut orbs = ctx.world.query::<(
            &mut Transform2D,
            &mut Sprite2D,
            &mut PointLight2D,
            &Velocity,
            &Hue,
            &mut Pulse,
        )>();
        orbs.for_each(
            ctx.world,
            |(transform, sprite, light, velocity, hue, pulse)| {
                transform.x += velocity.x * dt;
                transform.y += velocity.y * dt;
                pulse.0 += dt * 0.8;

                let half_w = w as f32 * 0.5 + sprite.width;
                let half_h = h as f32 * 0.5 + sprite.height;
                if transform.x > half_w {
                    transform.x = -half_w;
                }
                if transform.x < -half_w {
                    transform.x = half_w;
                }
                if transform.y > half_h {
                    transform.y = -half_h;
                }
                if transform.y < -half_h {
                    transform.y = half_h;
                }

                let pulse_scale = 1.0 + 0.18 * pulse.0.sin();
                transform.scale_x = pulse_scale;
                transform.scale_y = pulse_scale;

                let shifted_hue = (hue.base + hue.shift * dt + pulse.0 * 4.0) % 360.0;
                let lightness = 0.46 + 0.14 * (pulse.0 * 0.7).cos();
                let tint = Color::hsl(shifted_hue, 0.72, lightness);
                sprite.color = Color::new(tint.r, tint.g, tint.b, 0.95);
                light.color = tint;
            },
        );

        let mut mouse_light = ctx
            .world
            .query::<(&mut Transform2D, &mut PointLight2D, &MouseLight)>();
        mouse_light.for_each(ctx.world, |(transform, light, _)| {
            transform.x = mouse_world[0];
            transform.y = mouse_world[1];
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
    world.insert_resource(Renderer2DConfig::lit_hdr());
    world.insert_resource(RenderSettings2D::default());
    world.spawn((Camera2D::new(1280.0, 720.0), PrimaryCamera2D));

    // Mouse light entity.
    world.spawn((
        Transform2D::new(0.0, 0.0),
        PointLight2D::new(150.0)
            .intensity(1.8)
            .temperature(5000.0)
            .color(Color::rgb(1.0, 0.95, 0.85))
            .falloff(1.4),
        MouseLight,
    ));

    App::new(
        AppConfig::new("SkyEngine — ECS Lighting Demo", 1280, 720),
        world,
    )
    .run(LightingDemo::new(&mut rng));
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
