//! Textured sprite demo.
//!
//! Demonstrates:
//! - Procedurally generated textures (circles + checkerboard)
//! - Texture switching within a single frame
//! - Mixed textured and untextured sprites
//! - 10,000+ sprites in a few instanced draw calls
//!
//! ```bash
//! cargo run --example textured_demo --features app --release
//! ```

use sky_engine::app::{App, AppConfig};
use sky_engine::render::{Camera2D, Color, Sprite, SpriteBatch, Texture};

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

    let mut batch: Option<SpriteBatch> = None;
    let mut circle_tex: Option<Texture> = None;
    let mut checker_tex: Option<Texture> = None;
    let mut camera = Camera2D::new(960.0, 640.0);
    let mut time = 0.0f32;

    App::run(
        AppConfig::new("SkyEngine — Textured Sprites", 960, 640),
        |_world, _gpu| {
            eprintln!("[textured_demo] Press Escape to exit.");
        },
        move |ctx| {
            let dt = ctx.dt;
            time += dt;

            // Lazy init GPU resources
            if batch.is_none() {
                let b = SpriteBatch::new(ctx.gpu);
                circle_tex = Some(Texture::circle(ctx.gpu, 64));
                checker_tex = Some(Texture::checkerboard(
                    ctx.gpu,
                    64,
                    8,
                    [200, 180, 255, 255],
                    [80, 60, 140, 255],
                ));
                batch = Some(b);
            }
            let batch = batch.as_mut().unwrap();
            let circle = circle_tex.as_ref().unwrap();
            let checker = checker_tex.as_ref().unwrap();

            // Resize camera
            let [w, h] = ctx.gpu.surface_size();
            camera.set_viewport(w as f32, h as f32);

            // ── Update ──────────────────────────────────────────────────
            for p in particles.iter_mut() {
                p.x += p.vx * dt;
                p.y += p.vy * dt;
                p.hue = (p.hue + 40.0 * dt) % 360.0;
                p.life = (p.life + dt * 0.3) % 1.0;

                let hw = w as f32 * 0.5 + p.size;
                let hh = h as f32 * 0.5 + p.size;
                if p.x > hw {
                    p.x = -hw;
                }
                if p.x < -hw {
                    p.x = hw;
                }
                if p.y > hh {
                    p.y = -hh;
                }
                if p.y < -hh {
                    p.y = hh;
                }
            }

            for b in blocks.iter_mut() {
                b.angle += b.spin * dt;
            }

            // ── Draw ────────────────────────────────────────────────────
            batch.begin();

            // Layer 1: background stars (untextured tiny squares)
            for s in &stars {
                let t = (time * s.twinkle_speed).sin() * 0.5 + 0.5;
                let a = s.brightness * (0.3 + 0.7 * t);
                batch.draw(Sprite::new(s.x, s.y, 2.0, 2.0).color(Color::new(0.8, 0.85, 1.0, a)));
            }

            // Layer 2: checkerboard blocks (textured)
            batch.set_texture(checker);
            for b in &blocks {
                let pulse = 1.0 + 0.15 * (time * 2.0 + b.angle).sin();
                let sz = b.size * pulse;
                batch.draw(
                    Sprite::new(b.x, b.y, sz, sz)
                        .rotation(b.angle)
                        .color(Color::new(1.0, 1.0, 1.0, 0.7)),
                );
            }

            // Layer 3: circle particles (textured)
            batch.set_texture(circle);
            for p in &particles {
                let color = Color::hsl(p.hue, 0.9, 0.65);
                let alpha = 0.4 + 0.6 * (1.0 - p.life);
                batch.draw(
                    Sprite::new(p.x, p.y, p.size, p.size)
                        .color(Color::new(color.r, color.g, color.b, alpha)),
                );
            }

            // Draw all
            batch.draw_to_surface(ctx.gpu, &camera, Some([0.01, 0.01, 0.03, 1.0]));
        },
    );
}

// ── Minimal deterministic RNG ───────────────────────────────────────────────

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
