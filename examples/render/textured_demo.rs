//! ECS-driven textured sprite demo.
//!
//! Demonstrates:
//! - Textured sprites driven entirely from ECS
//! - Procedural textures
//! - Mixed textured and untextured sprite rendering
//! - High-level unlit rendering through `ctx.render()`
//!
//! ```bash
//! cargo run --example textured_demo --features app --release
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::{EntityId, World};
use sky_engine::gpu::GpuContext;
use sky_engine::render::{
    Camera, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteRenderer,
    Texture, Transform,
};

const NUM_PARTICLES: usize = 3000;
const NUM_BLOCKS: usize = 200;
const NUM_STARS: usize = 500;

struct Particle {
    entity: EntityId,
    vx: f32,
    vy: f32,
    hue: f32,
    life: f32,
}

struct Block {
    entity: EntityId,
    size: f32,
    angle: f32,
    spin: f32,
}

struct Star {
    entity: EntityId,
    brightness: f32,
    twinkle_speed: f32,
}

struct TexturedDemo {
    particles: Vec<Particle>,
    blocks: Vec<Block>,
    stars: Vec<Star>,
    camera: Option<EntityId>,
    time: f32,
}

impl TexturedDemo {
    fn new() -> Self {
        Self {
            particles: Vec::with_capacity(NUM_PARTICLES),
            blocks: Vec::with_capacity(NUM_BLOCKS),
            stars: Vec::with_capacity(NUM_STARS),
            camera: None,
            time: 0.0,
        }
    }
}

impl AppState for TexturedDemo {
    fn setup(&mut self, world: &mut World, gpu: &mut GpuContext) {
        let mut rng = SimpleRng::new(123);
        let circle = Texture::circle(gpu, 64);
        let checker = Texture::checkerboard(gpu, 64, 8, [200, 180, 255, 255], [80, 60, 140, 255]);

        self.camera = Some(world.spawn((
            Transform::default(),
            Camera::new(),
            Projection::orthographic(960.0, 640.0),
            MainCamera,
        )));

        for _ in 0..NUM_STARS {
            let x = rng.range(-480.0, 480.0);
            let y = rng.range(-320.0, 320.0);
            let brightness = rng.range(0.3, 1.0);
            let twinkle_speed = rng.range(1.0, 5.0);
            let entity = world.spawn((
                Transform::from_xyz(x, y, 0.0),
                SpriteRenderer::new(2.0, 2.0),
            ));
            self.stars.push(Star {
                entity,
                brightness,
                twinkle_speed,
            });
        }

        for _ in 0..NUM_BLOCKS {
            let x = rng.range(-450.0, 450.0);
            let y = rng.range(-300.0, 300.0);
            let size = rng.range(16.0, 48.0);
            let angle = rng.range(0.0, std::f32::consts::TAU);
            let spin = rng.range(-1.5, 1.5);
            let entity = world.spawn((
                Transform::from_xyz(x, y, 1.0).with_rotation(angle),
                SpriteRenderer::new(size, size)
                    .texture(checker.clone())
                    .color(Color::new(1.0, 1.0, 1.0, 0.7)),
            ));
            self.blocks.push(Block {
                entity,
                size,
                angle,
                spin,
            });
        }

        for _ in 0..NUM_PARTICLES {
            let x = rng.range(-480.0, 480.0);
            let y = rng.range(-320.0, 320.0);
            let vx = rng.range(-80.0, 80.0);
            let vy = rng.range(-80.0, 80.0);
            let size = rng.range(6.0, 24.0);
            let hue = rng.range(0.0, 360.0);
            let life = rng.range(0.0, 1.0);
            let color = Color::hsl(hue, 0.9, 0.65);
            let alpha = 0.4 + 0.6 * (1.0 - life);
            let entity = world.spawn((
                Transform::from_xyz(x, y, 2.0),
                SpriteRenderer::new(size, size)
                    .texture(circle.clone())
                    .color(Color::new(color.r, color.g, color.b, alpha)),
            ));
            self.particles.push(Particle {
                entity,
                vx,
                vy,
                hue,
                life,
            });
        }
    }

    fn update(&mut self, ctx: &mut FrameContext) {
        let dt = ctx.dt;
        self.time += dt;

        let [w, h] = ctx.surface_size();
        if let Some(camera) = self.camera {
            if let Some(projection) = ctx.world.get_mut::<Projection>(camera) {
                *projection = Projection::orthographic(w as f32, h as f32);
            }
        }

        for star in &self.stars {
            let twinkle = (self.time * star.twinkle_speed).sin() * 0.5 + 0.5;
            let alpha = star.brightness * (0.3 + 0.7 * twinkle);
            if let Some(sprite) = ctx.world.get_mut::<SpriteRenderer>(star.entity) {
                sprite.color = Color::new(0.8, 0.85, 1.0, alpha);
            }
        }

        for block in &mut self.blocks {
            block.angle += block.spin * dt;
            let pulse = 1.0 + 0.15 * (self.time * 2.0 + block.angle).sin();
            if let Some(transform) = ctx.world.get_mut::<Transform>(block.entity) {
                transform.set_rotation_z(block.angle);
            }
            if let Some(sprite) = ctx.world.get_mut::<SpriteRenderer>(block.entity) {
                sprite.width = block.size * pulse;
                sprite.height = block.size * pulse;
            }
        }

        for particle in &mut self.particles {
            let Some(transform) = ctx.world.get_mut::<Transform>(particle.entity) else {
                continue;
            };

            transform.position[0] += particle.vx * dt;
            transform.position[1] += particle.vy * dt;
            particle.hue = (particle.hue + 40.0 * dt) % 360.0;
            particle.life = (particle.life + dt * 0.3) % 1.0;

            let hw = w as f32 * 0.5 + 24.0;
            let hh = h as f32 * 0.5 + 24.0;
            if transform.position[0] > hw {
                transform.position[0] = -hw;
            }
            if transform.position[0] < -hw {
                transform.position[0] = hw;
            }
            if transform.position[1] > hh {
                transform.position[1] = -hh;
            }
            if transform.position[1] < -hh {
                transform.position[1] = hh;
            }

            if let Some(sprite) = ctx.world.get_mut::<SpriteRenderer>(particle.entity) {
                let color = Color::hsl(particle.hue, 0.9, 0.65);
                let alpha = 0.4 + 0.6 * (1.0 - particle.life);
                sprite.color = Color::new(color.r, color.g, color.b, alpha);
            }
        }

        if let Some(settings) = ctx.world.get_resource_mut::<RenderSettings>() {
            settings.clear_color = Color::new(0.01, 0.01, 0.03, 1.0);
        } else {
            ctx.world.insert_resource(RenderSettings {
                clear_color: Color::new(0.01, 0.01, 0.03, 1.0),
                ..Default::default()
            });
        }

        ctx.render();
    }
}

fn main() {
    let world = World::new();

    App::new(
        AppConfig::new("SkyEngine — ECS Textured Demo", 960, 640),
        world,
    )
    .with_render_pipeline(RenderPipelineAsset::universal_unlit())
    .run(TexturedDemo::new());
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
