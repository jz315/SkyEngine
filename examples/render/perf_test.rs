//! Sprite rendering performance benchmark.
//!
//! Renders increasing numbers of sprites and reports FPS + frame time.
//! Starts at 10K, doubles every 5 seconds up to 200K.
//!
//! ```bash
//! cargo run --example perf_test --features app --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::expert::draw::SpriteBatch;
use sky_engine::render::{Camera, Color, Sprite, Texture};

struct PerfTest {
    batch: Option<SpriteBatch>,
    circle_tex: Option<Texture>,
    camera: Camera,
    positions: Vec<(f32, f32, f32, f32, f32)>,
    time: f32,
    frame_count: u64,
    fps_timer: f32,
    level_idx: usize,
    level_timer: f32,
    current_count: u32,
    levels: [u32; 5],
}

impl PerfTest {
    fn new() -> Self {
        let levels = [10_000u32, 20_000, 50_000, 100_000, 200_000];
        let max_count = *levels.last().unwrap() as usize;
        let mut rng = SimpleRng::new(777);
        let positions: Vec<(f32, f32, f32, f32, f32)> = (0..max_count)
            .map(|_| {
                (
                    rng.range(-640.0, 640.0),
                    rng.range(-360.0, 360.0),
                    rng.range(3.0, 10.0),
                    rng.range(0.0, 360.0),
                    rng.range(-2.0, 2.0),
                )
            })
            .collect();

        eprintln!("╔══════════════════════════════════════════════════╗");
        eprintln!("║     SkyEngine Sprite Renderer — Perf Test        ║");
        eprintln!("╠══════════════════════════════════════════════════╣");
        eprintln!("║  Sprites │  FPS   │ Frame (ms) │ Draw Calls      ║");
        eprintln!("╠══════════════════════════════════════════════════╣");

        Self {
            batch: None,
            circle_tex: None,
            camera: Camera::new(1280.0, 720.0),
            positions,
            time: 0.0,
            frame_count: 0,
            fps_timer: 0.0,
            level_idx: 0,
            level_timer: 0.0,
            current_count: levels[0],
            levels,
        }
    }
}

impl AppState for PerfTest {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let gpu = ctx.gpu();
        self.batch = Some(SpriteBatch::new(gpu));
        self.circle_tex = Some(Texture::circle(gpu, 32));
    }

    fn update(&mut self, ctx: &mut FrameContext) {
        let dt = ctx.dt;
        self.time += dt;
        self.frame_count += 1;
        self.fps_timer += dt;
        self.level_timer += dt;

        let batch = self.batch.as_mut().unwrap();
        let circle = self.circle_tex.as_ref().unwrap();

        // Resize camera
        let view_size = ctx.logical_view_size();
        self.camera.set_viewport(view_size.width, view_size.height);

        // FPS reporting (every second)
        if self.fps_timer >= 1.0 {
            let fps = self.frame_count as f32 / self.fps_timer;
            let frame_ms = self.fps_timer * 1000.0 / self.frame_count as f32;
            eprintln!(
                "║  {:>6}  │ {:>5.0}  │   {:>6.2}   │      1          ║",
                self.current_count, fps, frame_ms
            );
            self.fps_timer = 0.0;
            self.frame_count = 0;
        }

        // Level progression (every 5 seconds)
        if self.level_timer >= 5.0 && self.level_idx + 1 < self.levels.len() {
            self.level_idx += 1;
            self.current_count = self.levels[self.level_idx];
            self.level_timer = 0.0;
            self.fps_timer = 0.0;
            self.frame_count = 0;
            eprintln!("╠──────────────────────────────────────────────────╣");
        }

        // Draw
        batch.set_texture(circle);
        let n = self.current_count as usize;
        for i in 0..n {
            let (x, y, size, hue, spin) = self.positions[i];
            let angle = self.time * spin;
            let h = (hue + self.time * 30.0) % 360.0;
            let color = Color::hsl(h, 0.8, 0.6);
            batch.draw(Sprite::new(x, y, size, size).rotation(angle).color(color));
        }
        batch.flush_to_surface(
            ctx.gpu(),
            &self.camera,
            Some(Color::new(0.02, 0.02, 0.05, 1.0)),
        );
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine — Perf Test", 1280, 720).with_vsync(false))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();

    App::new(world).run(PerfTest::new());
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
