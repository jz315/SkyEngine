//! Manual `Scene2D` textured sprite demo.
//!
//! Demonstrates:
//! - Reusable `Scene2D`
//! - Procedural textures
//! - Mixed textured and untextured sprites
//! - High-level unlit rendering without ECS extraction
//!
//! ```bash
//! cargo run --example textured_demo --features app --release
//! ```

use sky_engine::app::{App, AppConfig};
use sky_engine::render::{
    Camera2D, Color, Renderer2D, Renderer2DConfig, Scene2D, Sprite2D, Texture, Transform2D,
};

const NUM_PARTICLES: usize = 3000;
const NUM_BLOCKS: usize = 200;
const NUM_STARS: usize = 500;

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    hue: f32,
    life: f32,
}

struct Block {
    x: f32,
    y: f32,
    size: f32,
    angle: f32,
    spin: f32,
}

struct Star {
    x: f32,
    y: f32,
    brightness: f32,
    twinkle_speed: f32,
}

fn main() {
    let mut rng = SimpleRng::new(123);

    let mut particles: Vec<Particle> = (0..NUM_PARTICLES)
        .map(|_| Particle {
            x: rng.range(-480.0, 480.0),
            y: rng.range(-320.0, 320.0),
            vx: rng.range(-80.0, 80.0),
            vy: rng.range(-80.0, 80.0),
            size: rng.range(6.0, 24.0),
            hue: rng.range(0.0, 360.0),
            life: rng.range(0.0, 1.0),
        })
        .collect();

    let mut blocks: Vec<Block> = (0..NUM_BLOCKS)
        .map(|_| Block {
            x: rng.range(-450.0, 450.0),
            y: rng.range(-300.0, 300.0),
            size: rng.range(16.0, 48.0),
            angle: rng.range(0.0, std::f32::consts::TAU),
            spin: rng.range(-1.5, 1.5),
        })
        .collect();

    let stars: Vec<Star> = (0..NUM_STARS)
        .map(|_| Star {
            x: rng.range(-480.0, 480.0),
            y: rng.range(-320.0, 320.0),
            brightness: rng.range(0.3, 1.0),
            twinkle_speed: rng.range(1.0, 5.0),
        })
        .collect();

    let mut renderer: Option<Renderer2D> = None;
    let mut circle_tex: Option<Texture> = None;
    let mut checker_tex: Option<Texture> = None;
    let mut scene = Scene2D::new();
    let mut camera = Camera2D::new(960.0, 640.0);
    let mut time = 0.0f32;

    App::run(
        AppConfig::new("SkyEngine — Scene2D Textured Demo", 960, 640),
        |_world, _gpu| {},
        move |ctx| {
            ctx.world.tick();
            let dt = ctx.world.time.delta;
            time += dt;

            if renderer.is_none() {
                renderer = Some(Renderer2D::new(ctx.gpu, Renderer2DConfig::unlit()));
                circle_tex = Some(Texture::circle(ctx.gpu, 64));
                checker_tex = Some(Texture::checkerboard(
                    ctx.gpu,
                    64,
                    8,
                    [200, 180, 255, 255],
                    [80, 60, 140, 255],
                ));
            }

            let renderer = renderer.as_mut().expect("renderer should exist");
            let circle = circle_tex.as_ref().expect("circle texture should exist");
            let checker = checker_tex.as_ref().expect("checker texture should exist");

            let [w, h] = ctx.gpu.surface_size();
            camera.set_viewport(w as f32, h as f32);

            for particle in &mut particles {
                particle.x += particle.vx * dt;
                particle.y += particle.vy * dt;
                particle.hue = (particle.hue + 40.0 * dt) % 360.0;
                particle.life = (particle.life + dt * 0.3) % 1.0;

                let hw = w as f32 * 0.5 + particle.size;
                let hh = h as f32 * 0.5 + particle.size;
                if particle.x > hw {
                    particle.x = -hw;
                }
                if particle.x < -hw {
                    particle.x = hw;
                }
                if particle.y > hh {
                    particle.y = -hh;
                }
                if particle.y < -hh {
                    particle.y = hh;
                }
            }

            for block in &mut blocks {
                block.angle += block.spin * dt;
            }

            scene.reset();
            scene.set_camera(camera);
            scene.settings_mut().clear_color = Color::new(0.01, 0.01, 0.03, 1.0);

            for star in &stars {
                let twinkle = (time * star.twinkle_speed).sin() * 0.5 + 0.5;
                let alpha = star.brightness * (0.3 + 0.7 * twinkle);
                scene.add_sprite(
                    Transform2D::from_xyz(star.x, star.y, 0.0),
                    Sprite2D::new(2.0, 2.0).color(Color::new(0.8, 0.85, 1.0, alpha)),
                );
            }

            for block in &blocks {
                let pulse = 1.0 + 0.15 * (time * 2.0 + block.angle).sin();
                let size = block.size * pulse;
                scene.add_sprite(
                    Transform2D::from_xyz(block.x, block.y, 1.0).with_rotation(block.angle),
                    Sprite2D::new(size, size)
                        .texture(checker.clone())
                        .color(Color::new(1.0, 1.0, 1.0, 0.7)),
                );
            }

            for particle in &particles {
                let color = Color::hsl(particle.hue, 0.9, 0.65);
                let alpha = 0.4 + 0.6 * (1.0 - particle.life);
                scene.add_sprite(
                    Transform2D::from_xyz(particle.x, particle.y, 2.0),
                    Sprite2D::new(particle.size, particle.size)
                        .texture(circle.clone())
                        .color(Color::new(color.r, color.g, color.b, alpha)),
                );
            }

            renderer.render_scene(ctx.gpu, &scene);
        },
    );
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
