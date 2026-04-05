//! GPU-accelerated sprite demo.
//!
//! Spawns thousands of coloured, rotating sprites rendered in a single
//! instanced draw call via the SpriteBatch.
//!
//! ```bash
//! cargo run --example sprite_demo --features app --release
//! ```

use sky_engine::app::{App, AppConfig};
use sky_engine::render::{Camera2D, Color, Sprite, SpriteBatch};

const NUM_SPRITES: usize = 5000;

struct SpriteData {
    x: f32,
    y: f32,
    size: f32,
    angle: f32,
    speed: f32,
    drift_x: f32,
    drift_y: f32,
    hue: f32,
}

fn main() {
    // Pre-generate sprite data
    let mut sprites: Vec<SpriteData> = Vec::with_capacity(NUM_SPRITES);
    let mut rng = SimpleRng::new(42);

    for _ in 0..NUM_SPRITES {
        sprites.push(SpriteData {
            x: rng.range(-480.0, 480.0),
            y: rng.range(-320.0, 320.0),
            size: rng.range(4.0, 20.0),
            angle: rng.range(0.0, std::f32::consts::TAU),
            speed: rng.range(-3.0, 3.0),
            drift_x: rng.range(-60.0, 60.0),
            drift_y: rng.range(-60.0, 60.0),
            hue: rng.range(0.0, 360.0),
        });
    }

    let mut batch: Option<SpriteBatch> = None;
    let mut camera = Camera2D::new(960.0, 640.0);
    App::run(
        AppConfig::new("SkyEngine — Sprite Demo", 960, 640),
        // Setup (GPU resources created lazily in frame)
        |_world, _gpu| {
            eprintln!("[sprite_demo] Rendering {NUM_SPRITES} sprites. Press Escape to exit.");
        },
        // Frame
        move |ctx| {
            let dt = ctx.dt;

            // Lazily create SpriteBatch on first frame (needs GPU)
            if batch.is_none() {
                batch = Some(SpriteBatch::new(ctx.gpu));
            }
            let batch = batch.as_mut().unwrap();

            // Update camera viewport on resize
            let [w, h] = ctx.gpu.surface_size();
            camera.set_viewport(w as f32, h as f32);

            // Update sprites
            for s in sprites.iter_mut() {
                s.angle += s.speed * dt;
                s.x += s.drift_x * dt;
                s.y += s.drift_y * dt;

                // Wrap around screen edges
                let hw = w as f32 * 0.5 + s.size;
                let hh = h as f32 * 0.5 + s.size;
                if s.x > hw {
                    s.x = -hw;
                }
                if s.x < -hw {
                    s.x = hw;
                }
                if s.y > hh {
                    s.y = -hh;
                }
                if s.y < -hh {
                    s.y = hh;
                }

                // Slowly shift hue over time
                s.hue = (s.hue + 20.0 * dt) % 360.0;
            }

            // Build batch
            for s in sprites.iter() {
                let color = Color::hsl(s.hue, 0.8, 0.6);
                batch.draw(
                    Sprite::new(s.x, s.y, s.size, s.size)
                        .rotation(s.angle)
                        .color(color),
                );
            }

            // Draw (clear + render)
            batch.flush_to_surface(ctx.gpu, &camera, Some(Color::rgb(0.02, 0.02, 0.06)));
        },
    );
}

// ── Minimal deterministic RNG (no rand dependency needed for this example) ──

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
