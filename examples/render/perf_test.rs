//! Sprite rendering performance benchmark.
//!
//! Renders increasing numbers of sprites and reports FPS + frame time.
//! Starts at 10K, doubles every 5 seconds up to 200K.
//!
//! ```bash
//! cargo run --example perf_test --features app --release
//! ```

use sky_engine::app::{App, AppConfig};
use sky_engine::ecs::World;
use sky_engine::render::{Camera2D, Color, Renderer2DConfig, Sprite, Texture};
use sky_engine::render::expert::SpriteBatch;

fn main() {
    let mut batch: Option<SpriteBatch> = None;
    let mut circle_tex: Option<Texture> = None;
    let mut camera = Camera2D::new(1280.0, 720.0);

    let mut time = 0.0f32;
    let mut frame_count = 0u64;
    let mut fps_timer = 0.0f32;
    // Sprite counts to test
    let levels = [10_000u32, 20_000, 50_000, 100_000, 200_000];
    let mut level_idx = 0usize;
    let mut level_timer = 0.0f32;
    let mut current_count = levels[0];

    // Pre-generate positions (reused across levels)
    let max_count = *levels.last().unwrap() as usize;
    let mut rng = SimpleRng::new(777);
    let positions: Vec<(f32, f32, f32, f32, f32)> = (0..max_count)
        .map(|_| {
            (
                rng.range(-640.0, 640.0), // x
                rng.range(-360.0, 360.0), // y
                rng.range(3.0, 10.0),     // size
                rng.range(0.0, 360.0),    // hue
                rng.range(-2.0, 2.0),     // rotation speed
            )
        })
        .collect();

    eprintln!("╔══════════════════════════════════════════════════╗");
    eprintln!("║     SkyEngine Sprite Renderer — Perf Test        ║");
    eprintln!("╠══════════════════════════════════════════════════╣");
    eprintln!("║  Sprites │  FPS   │ Frame (ms) │ Draw Calls      ║");
    eprintln!("╠══════════════════════════════════════════════════╣");

    let mut world = World::new();
    world.insert_resource(Renderer2DConfig::unlit());

    App::new(
        {
            let mut cfg = AppConfig::new("SkyEngine — Perf Test", 1280, 720);
            cfg.vsync = false;
            cfg
        },
        world,
    )
    .run(move |ctx| {
        ctx.world.tick();
        let dt = ctx.dt;
        time += dt;
        frame_count += 1;
        fps_timer += dt;
        level_timer += dt;

        // Lazy init
        if batch.is_none() {
            let gpu = ctx.gpu();
            let b = SpriteBatch::new(gpu);
            circle_tex = Some(Texture::circle(gpu, 32));
            batch = Some(b);
        }
        let batch = batch.as_mut().unwrap();
        let circle = circle_tex.as_ref().unwrap();

        // Resize camera
        let [w, h] = ctx.surface_size();
        camera.set_viewport(w as f32, h as f32);

        // FPS reporting (every second)
        if fps_timer >= 1.0 {
            let fps = frame_count as f32 / fps_timer;
            let frame_ms = fps_timer * 1000.0 / frame_count as f32;
            eprintln!(
                "║  {:>6}  │ {:>5.0}  │   {:>6.2}   │      1          ║",
                current_count, fps, frame_ms
            );
            fps_timer = 0.0;
            frame_count = 0;
        }

        // Level progression (every 5 seconds)
        if level_timer >= 5.0 && level_idx + 1 < levels.len() {
            level_idx += 1;
            current_count = levels[level_idx];
            level_timer = 0.0;
            fps_timer = 0.0;
            frame_count = 0;
            eprintln!("╠──────────────────────────────────────────────────╣");
        }

        // Draw
        batch.set_texture(circle);
        let n = current_count as usize;
        for i in 0..n {
            let (x, y, size, hue, spin) = positions[i];
            let angle = time * spin;
            let h = (hue + time * 30.0) % 360.0;
            let color = Color::hsl(h, 0.8, 0.6);
            batch.draw(Sprite::new(x, y, size, size).rotation(angle).color(color));
        }
        batch.flush_to_surface(ctx.gpu(), &camera, Some(Color::new(0.02, 0.02, 0.05, 1.0)));
    });
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
