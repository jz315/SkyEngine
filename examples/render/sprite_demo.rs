//! GPU-accelerated ECS sprite demo.
//!
//! Spawns thousands of coloured sprites as ECS entities and renders them
//! through the default programmable scene pipeline installed via `RenderPlugin`.
//!
//! ```bash
//! cargo run --example sprite_demo --features app --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, SpriteFeature,
    SpriteRenderer, Transform, TransparentPhase,
};

const NUM_SPRITES: usize = 5000;

#[derive(Clone, Copy)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Spin(f32);

#[derive(Clone, Copy)]
struct Hue(f32);

struct SpriteDemo {
    fps_smooth: f32,
    frame_count: u32,
}

impl AppState for SpriteDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        let dt = ctx.dt;
        let [w, h] = ctx.surface_size();

        let mut query = ctx.world.query::<(
            &mut Transform,
            &mut SpriteRenderer,
            &Velocity,
            &Spin,
            &mut Hue,
        )>();
        query.for_each(ctx.world, |(transform, sprite, velocity, spin, hue)| {
            transform.position[0] += velocity.x * dt;
            transform.position[1] += velocity.y * dt;
            transform.rotate_z(spin.0 * dt);
            hue.0 = (hue.0 + 20.0 * dt) % 360.0;
            sprite.color = Color::hsl(hue.0, 0.8, 0.6);

            let hw = w as f32 * 0.5 + sprite.width;
            let hh = h as f32 * 0.5 + sprite.height;
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
                "SkyEngine — ECS Sprite Demo | {:.0} FPS | {} sprites",
                self.fps_smooth, stats.sprite_count
            ));
        }
    }
}

fn main() {
    let mut rng = SimpleRng::new(42);

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic(640.0),
        MainCamera,
    ));

    for _ in 0..NUM_SPRITES {
        let size = rng.range(4.0, 20.0);
        let hue = rng.range(0.0, 360.0);
        world.spawn((
            Transform::from_xy(rng.range(-480.0, 480.0), rng.range(-320.0, 320.0)),
            SpriteRenderer::new(size, size).color(Color::hsl(hue, 0.8, 0.6)),
            Velocity {
                x: rng.range(-60.0, 60.0),
                y: rng.range(-60.0, 60.0),
            },
            Spin(rng.range(-3.0, 3.0)),
            Hue(hue),
        ));
    }

    world
        .install(WindowPlugin::new("SkyEngine — ECS Sprite Demo", 960, 640))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(SpriteDemo {
        fps_smooth: 0.0,
        frame_count: 0,
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
