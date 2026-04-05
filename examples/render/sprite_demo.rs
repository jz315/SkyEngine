//! GPU-accelerated ECS sprite demo.
//!
//! Spawns thousands of coloured sprites as ECS entities and renders them
//! through the high-level [`Renderer2D`].
//!
//! ```bash
//! cargo run --example sprite_demo --features app --release
//! ```

use sky_engine::app::{App, AppConfig};
use sky_engine::render::{
    Camera2D, Color, PrimaryCamera2D, Renderer2D, Renderer2DConfig, Sprite2D, Transform2D,
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

fn main() {
    let mut rng = SimpleRng::new(42);
    let mut renderer: Option<Renderer2D> = None;

    App::run(
        AppConfig::new("SkyEngine — ECS Sprite Demo", 960, 640),
        move |world, _gpu| {
            world.spawn((Camera2D::new(960.0, 640.0), PrimaryCamera2D));

            for _ in 0..NUM_SPRITES {
                let size = rng.range(4.0, 20.0);
                let hue = rng.range(0.0, 360.0);
                world.spawn((
                    Transform2D::new(rng.range(-480.0, 480.0), rng.range(-320.0, 320.0)),
                    Sprite2D::new(size, size).color(Color::hsl(hue, 0.8, 0.6)),
                    Velocity {
                        x: rng.range(-60.0, 60.0),
                        y: rng.range(-60.0, 60.0),
                    },
                    Spin(rng.range(-3.0, 3.0)),
                    Hue(hue),
                ));
            }
        },
        move |ctx| {
            if renderer.is_none() {
                renderer = Some(Renderer2D::new(ctx.gpu, Renderer2DConfig::unlit()));
            }
            let renderer = renderer.as_mut().expect("renderer should exist");
            ctx.world.tick();
            let dt = ctx.world.time.delta;

            let [w, h] = ctx.gpu.surface_size();
            let mut query =
                ctx.world
                    .query::<(&mut Transform2D, &mut Sprite2D, &Velocity, &Spin, &mut Hue)>();
            query.for_each(ctx.world, |(transform, sprite, velocity, spin, hue)| {
                transform.x += velocity.x * dt;
                transform.y += velocity.y * dt;
                transform.rotation += spin.0 * dt;
                hue.0 = (hue.0 + 20.0 * dt) % 360.0;
                sprite.color = Color::hsl(hue.0, 0.8, 0.6);

                let hw = w as f32 * 0.5 + sprite.width;
                let hh = h as f32 * 0.5 + sprite.height;
                if transform.x > hw {
                    transform.x = -hw;
                }
                if transform.x < -hw {
                    transform.x = hw;
                }
                if transform.y > hh {
                    transform.y = -hh;
                }
                if transform.y < -hh {
                    transform.y = hh;
                }
            });

            renderer.render_world(ctx.gpu, ctx.world);
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
