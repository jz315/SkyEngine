//! 🪼 Cosmic Jellyfish — SkyEngine rendering showcase
//!
//! Hundreds of glowing ECS-driven jellyfish drift through deep space,
//! each emitting its own point light.  The full HDR pipeline is active:
//!
//!   SpriteBatch → LightPass → Composite → Bloom → Vignette → ToneMap
//!
//! ```bash
//! cargo run --example cosmic_jellyfish --features app --release
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use sky_engine::app::{App, AppConfig};
use sky_engine::ecs::World;
use sky_engine::gpu::GpuContext;
use sky_engine::render::{
    Bloom, Camera2D, Color, CompositePass, Light2D, LightPass, RenderGraph, Sprite, SpriteBatch,
    TargetSize, Texture, ToneMap, Vignette,
};

// ── Configuration ───────────────────────────────────────────────────────────

const NUM_JELLYFISH: usize = 350;
const NUM_STARS: usize = 1200;
const TENTACLE_SEGMENTS: usize = 6;

// ── ECS components ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Drift {
    vx: f32,
    vy: f32,
    wobble_phase: f32,
    wobble_freq: f32,
    wobble_amp: f32,
}

#[derive(Clone, Copy)]
struct JellyfishData {
    size: f32,
    hue: f32,
    hue_drift: f32,
    pulse_phase: f32,
    pulse_speed: f32,
    light_radius: f32,
    light_intensity: f32,
}

#[derive(Clone, Copy)]
struct Star {
    x: f32,
    y: f32,
    brightness: f32,
    twinkle_phase: f32,
    twinkle_speed: f32,
    size: f32,
}

// ── Render state ────────────────────────────────────────────────────────────

struct RenderState {
    camera: Camera2D,
    scene_batch: SpriteBatch,
    normal_batch: SpriteBatch,
    circle_tex: Texture,
    normal_tex: Texture,
    dot_tex: Texture,
    light_pass: LightPass,
    composite_pass: CompositePass,
    vignette: Vignette,
    bloom: Bloom,
    tonemap: ToneMap,
}

impl RenderState {
    fn new(gpu: &GpuContext) -> Self {
        let [sw, sh] = gpu.surface_size();
        let hdr = wgpu::TextureFormat::Rgba16Float;

        let mut vignette = Vignette::new(gpu, hdr);
        vignette.intensity = 0.25;
        vignette.smoothness = 0.45;

        let mut bloom = Bloom::new(gpu, sw, sh, hdr);
        bloom.threshold = 0.32;
        bloom.intensity = 0.7;
        bloom.radius = 1.4;

        let mut tonemap = ToneMap::new(gpu, gpu.surface_format());
        tonemap.exposure = 1.8;
        tonemap.gamma = 2.2;

        Self {
            camera: Camera2D::new(1280.0, 720.0),
            scene_batch: SpriteBatch::new(gpu),
            normal_batch: SpriteBatch::new(gpu),
            circle_tex: Texture::circle(gpu, 64),
            normal_tex: Texture::circle_normal(gpu, 64),
            dot_tex: Texture::circle(gpu, 16),
            light_pass: LightPass::new(gpu, hdr),
            composite_pass: CompositePass::new(gpu, hdr),
            vignette,
            bloom,
            tonemap,
        }
    }

    fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        self.bloom
            .resize(gpu, width, height, wgpu::TextureFormat::Rgba16Float);
    }
}

// ── Main ────────────────────────────────────────────────────────────────────

fn main() {
    let mut rng = SimpleRng::new(42);

    // Spawn jellyfish
    let mut world = World::new();
    for _ in 0..NUM_JELLYFISH {
        let size = rng.range(18.0, 55.0);
        world.spawn((
            Position {
                x: rng.range(-1500.0, 1500.0),
                y: rng.range(-600.0, 600.0),
            },
            Drift {
                vx: rng.range(-18.0, 18.0),
                vy: rng.range(-28.0, -5.0),
                wobble_phase: rng.range(0.0, std::f32::consts::TAU),
                wobble_freq: rng.range(0.4, 1.2),
                wobble_amp: rng.range(8.0, 30.0),
            },
            JellyfishData {
                size,
                hue: rng.range(0.0, 360.0),
                hue_drift: rng.range(5.0, 25.0),
                pulse_phase: rng.range(0.0, std::f32::consts::TAU),
                pulse_speed: rng.range(0.6, 1.8),
                light_radius: size * rng.range(3.0, 5.5),
                light_intensity: rng.range(0.7, 1.6),
            },
        ));
    }

    // Stars
    let stars: Vec<Star> = (0..NUM_STARS)
        .map(|_| Star {
            x: rng.range(-2000.0, 2000.0),
            y: rng.range(-1200.0, 1200.0),
            brightness: rng.range(0.3, 1.2),
            twinkle_phase: rng.range(0.0, std::f32::consts::TAU),
            twinkle_speed: rng.range(0.5, 3.0),
            size: rng.range(1.5, 4.5),
        })
        .collect();

    // ── Build render graph ──────────────────────────────────────────────

    let mut graph = RenderGraph::new();

    let scene_rt = graph.create_texture(|b| {
        b.name("scene_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let normal_rt = graph.create_texture(|b| {
        b.name("normal_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba8Unorm);
    });
    let light_rt = graph.create_texture(|b| {
        b.name("light_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let hdr_rt = graph.create_texture(|b| {
        b.name("hdr_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let graded_rt = graph.create_texture(|b| {
        b.name("graded_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let bloom_rt = graph.create_texture(|b| {
        b.name("bloom_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });

    let scene_pass = graph.add_render_pass("scene_batch", |s| {
        s.write_color_cleared(0, scene_rt, [0.005, 0.003, 0.012, 1.0]);
    });
    let normal_pass = graph.add_render_pass("normal_batch", |s| {
        s.write_color_cleared(0, normal_rt, [0.5, 0.5, 1.0, 1.0]);
    });
    let lighting_pass = graph.add_render_pass("lighting", |s| {
        s.read(normal_rt);
        s.write(light_rt);
    });
    let composite_pass_h = graph.add_render_pass("composite", |s| {
        s.read(scene_rt);
        s.read(light_rt);
        s.write(hdr_rt);
    });
    let vignette_pass = graph.add_render_pass("vignette", |s| {
        s.read(hdr_rt);
        s.write(graded_rt);
    });
    let bloom_pass = graph.add_render_pass("bloom", |s| {
        s.read(graded_rt);
        s.write(bloom_rt);
    });
    let tonemap_pass = graph.add_render_pass("tonemap", |s| {
        s.read(bloom_rt);
        s.write_surface();
    });

    // ── Shared state via Rc<RefCell<>> ──────────────────────────────────

    let graph = Rc::new(RefCell::new(graph));
    let render_state = Rc::new(RefCell::new(None::<RenderState>));
    let sim_time = Rc::new(RefCell::new(0.0f32));
    let world = Rc::new(RefCell::new(world));
    let stars = Rc::new(RefCell::new(stars));

    let frame_graph = Rc::clone(&graph);
    let resize_graph = Rc::clone(&graph);
    let shutdown_graph = Rc::clone(&graph);
    let frame_state = Rc::clone(&render_state);
    let resize_state = Rc::clone(&render_state);
    let frame_time = Rc::clone(&sim_time);
    let frame_world = Rc::clone(&world);
    let frame_stars = Rc::clone(&stars);

    App::run_with_lifecycle(
        AppConfig::new("SkyEngine 🪼 Cosmic Jellyfish", 1280, 720),
        // ── setup ───────────────────────────────────────────────────────
        |_world, _gpu| {
            eprintln!(
                "[cosmic_jellyfish] Move mouse to steer the spotlight. Press Escape to exit."
            );
        },
        // ── frame ───────────────────────────────────────────────────────
        move |ctx| {
            let dt = ctx.dt.min(0.05);
            *frame_time.borrow_mut() += dt;
            let time = *frame_time.borrow();

            let mut state_ref = frame_state.borrow_mut();
            if state_ref.is_none() {
                *state_ref = Some(RenderState::new(ctx.gpu));
            }

            let [w, h] = ctx.gpu.surface_size();
            let mouse = ctx.input.mouse_position();
            let aspect = w as f32 / h as f32;
            let camera_h = 1080.0;
            let camera_w = camera_h * aspect;

            let mouse_world = {
                let rs = state_ref.as_mut().unwrap();
                rs.camera.set_viewport(camera_w, camera_h);
                // We need to pass raw screen coordinates and true window size to screen_to_world
                // Wait, screen_to_world uses the camera's viewport which is now logical!
                // So we must scale the mouse input or pass the physical window size.
                // Camera2D assumes screen_x and screen_y are in [0, viewport_width].
                // We must map physical mouse to [0, camera_w].
                let scaled_mouse_x = (mouse[0] / w as f32) * camera_w;
                let scaled_mouse_y = (mouse[1] / h as f32) * camera_h;
                rs.camera.screen_to_world(scaled_mouse_x, scaled_mouse_y)
            };

            // ── ECS: simulate jellyfish ─────────────────────────────────
            // Wrap around a bounds slightly larger than the camera view
            let half_w = camera_w * 0.55;
            let half_h = camera_h * 0.55;
            {
                let world = frame_world.borrow_mut();
                let mut q = world.query::<(&mut Position, &mut Drift, &mut JellyfishData)>();
                q.for_each(&world, |(pos, drift, jelly)| {
                    drift.wobble_phase += drift.wobble_freq * dt;
                    jelly.pulse_phase += jelly.pulse_speed * dt;
                    jelly.hue = (jelly.hue + jelly.hue_drift * dt) % 360.0;

                    pos.x += drift.vx * dt + drift.wobble_amp * (drift.wobble_phase).sin() * dt;
                    pos.y += drift.vy * dt;

                    // Wrap around
                    if pos.y < -half_h {
                        pos.y = half_h;
                        pos.x = (pos.x + 200.0) % (half_w * 2.0) - half_w;
                    }
                    if pos.y > half_h {
                        pos.y = -half_h;
                    }
                    if pos.x < -half_w {
                        pos.x = half_w;
                    }
                    if pos.x > half_w {
                        pos.x = -half_w;
                    }
                });
            }

            // Twinkle stars
            {
                let mut stars = frame_stars.borrow_mut();
                for star in stars.iter_mut() {
                    star.twinkle_phase += star.twinkle_speed * dt;
                }
            }

            // ── Collect scene data ──────────────────────────────────────
            struct JellyVisual {
                x: f32,
                y: f32,
                size: f32,
                pulse: f32,
                hue: f32,
                light_radius: f32,
                light_intensity: f32,
                wobble_phase: f32,
            }

            let mut visuals = Vec::with_capacity(NUM_JELLYFISH);
            {
                let world = frame_world.borrow();
                let mut q = world.query::<(&Position, &Drift, &JellyfishData)>();
                q.for_each(&world, |(pos, drift, jelly)| {
                    let pulse = 0.85 + 0.2 * jelly.pulse_phase.sin();
                    visuals.push(JellyVisual {
                        x: pos.x,
                        y: pos.y,
                        size: jelly.size,
                        pulse,
                        hue: jelly.hue,
                        light_radius: jelly.light_radius,
                        light_intensity: jelly.light_intensity
                            * (0.7 + 0.3 * jelly.pulse_phase.sin()),
                        wobble_phase: drift.wobble_phase,
                    });
                });
            }

            // ── Build sprites & lights ──────────────────────────────────
            let rs = state_ref.as_mut().unwrap();

            rs.scene_batch.begin();
            rs.normal_batch.begin();

            // Stars background
            {
                let stars = frame_stars.borrow();
                rs.scene_batch.set_texture(&rs.dot_tex);
                for star in stars.iter() {
                    let twinkle = star.brightness * (0.5 + 0.5 * star.twinkle_phase.sin());
                    rs.scene_batch.draw(
                        Sprite::new(star.x, star.y, star.size, star.size).color(Color::new(
                            twinkle,
                            twinkle * 0.95,
                            twinkle * 1.05,
                            0.9,
                        )),
                    );
                }
            }

            // Jellyfish bodies
            rs.scene_batch.set_texture(&rs.circle_tex);
            rs.normal_batch.set_texture(&rs.normal_tex);

            let mut lights = Vec::with_capacity(visuals.len() + 1);

            for jv in &visuals {
                let body_size = jv.size * jv.pulse;
                let saturation = 0.75;
                let lightness = 0.55 + 0.1 * jv.pulse;
                let body_color = Color::hsl(jv.hue, saturation, lightness);

                // Bell (body)
                rs.scene_batch
                    .draw(
                        Sprite::new(jv.x, jv.y, body_size, body_size * 0.7).color(Color::new(
                            body_color.r * 1.3,
                            body_color.g * 1.3,
                            body_color.b * 1.3,
                            0.85,
                        )),
                    );
                rs.normal_batch
                    .draw(Sprite::new(jv.x, jv.y, body_size, body_size * 0.7).color(Color::WHITE));

                // Inner glow (brighter core)
                let glow_size = body_size * 0.55;
                rs.scene_batch.draw(
                    Sprite::new(jv.x, jv.y - body_size * 0.05, glow_size, glow_size * 0.5).color(
                        Color::new(
                            body_color.r * 2.0,
                            body_color.g * 2.0,
                            body_color.b * 2.0,
                            0.6,
                        ),
                    ),
                );

                // Tentacles (procedural chain of circles going downward)
                let tentacle_hue = (jv.hue + 30.0) % 360.0;
                let tentacle_color = Color::hsl(tentacle_hue, 0.6, 0.45);
                for seg in 0..TENTACLE_SEGMENTS {
                    let t = seg as f32 / TENTACLE_SEGMENTS as f32;
                    let seg_size = body_size * 0.2 * (1.0 - t * 0.7);
                    let sway = (jv.wobble_phase + seg as f32 * 0.8).sin() * (8.0 + t * 15.0);
                    let ty = jv.y - body_size * 0.35 - seg as f32 * body_size * 0.18;
                    let tx = jv.x + sway;
                    let alpha = 0.6 * (1.0 - t * 0.5);

                    rs.scene_batch
                        .draw(Sprite::new(tx, ty, seg_size, seg_size).color(Color::new(
                            tentacle_color.r,
                            tentacle_color.g,
                            tentacle_color.b,
                            alpha,
                        )));
                }

                // Light
                lights.push(
                    Light2D::new(jv.x, jv.y, jv.light_radius)
                        .intensity(jv.light_intensity)
                        .falloff(1.8)
                        .color(Color::hsl(jv.hue, 0.8, 0.65)),
                );
            }

            // Mouse spotlight — cool white
            let mouse_pulse = 1.0 + 0.15 * (time * 3.0).sin();
            lights.push(
                Light2D::new(mouse_world[0], mouse_world[1], 200.0 * mouse_pulse)
                    .intensity(2.2)
                    .temperature(7500.0)
                    .falloff(1.4)
                    .color(Color::rgb(0.9, 0.92, 1.0)),
            );

            // Global fill light — covers the entire viewport so no edge is ever black
            lights.push(
                Light2D::new(0.0, 0.0, 2000.0)
                    .intensity(0.5)
                    .falloff(3.5)
                    .color(Color::rgb(0.2, 0.15, 0.3)),
            );

            // Four corner ambient lights cycling hue — full coverage
            lights.push(
                Light2D::new(-500.0, 300.0, 1000.0)
                    .intensity(0.45)
                    .falloff(2.0)
                    .color(Color::hsl((time * 8.0) % 360.0, 0.5, 0.5)),
            );
            lights.push(
                Light2D::new(500.0, -300.0, 1000.0)
                    .intensity(0.45)
                    .falloff(2.0)
                    .color(Color::hsl((time * 8.0 + 180.0) % 360.0, 0.5, 0.5)),
            );
            lights.push(
                Light2D::new(500.0, 300.0, 1000.0)
                    .intensity(0.35)
                    .falloff(2.2)
                    .color(Color::hsl((time * 8.0 + 90.0) % 360.0, 0.4, 0.45)),
            );
            lights.push(
                Light2D::new(-500.0, -300.0, 1000.0)
                    .intensity(0.35)
                    .falloff(2.2)
                    .color(Color::hsl((time * 8.0 + 270.0) % 360.0, 0.4, 0.45)),
            );

            let camera = rs.camera;

            // ── Execute render graph ────────────────────────────────────
            let mut graph = frame_graph.borrow_mut();
            let result = graph.try_execute(ctx.gpu, |pass, gpu, textures| {
                let rs = state_ref.as_mut().unwrap();

                if pass.handle == scene_pass {
                    let target = textures.render_target(scene_rt).expect("scene_rt");
                    rs.scene_batch.draw_to_target(
                        gpu,
                        &camera,
                        target,
                        Some([0.005, 0.003, 0.012, 1.0]),
                    );
                } else if pass.handle == normal_pass {
                    let target = textures.render_target(normal_rt).expect("normal_rt");
                    rs.normal_batch.draw_to_target(
                        gpu,
                        &camera,
                        target,
                        Some([0.5, 0.5, 1.0, 1.0]),
                    );
                } else if pass.handle == lighting_pass {
                    let normal_target = textures.render_target(normal_rt).expect("normal_rt");
                    let output = textures.render_target(light_rt).expect("light_rt");
                    rs.light_pass.render(
                        gpu,
                        &lights,
                        Some(normal_target),
                        output,
                        &camera,
                        [0.12, 0.09, 0.16, 1.0],
                    );
                } else if pass.handle == composite_pass_h {
                    let scene = textures.render_target(scene_rt).expect("scene_rt");
                    let lightmap = textures.render_target(light_rt).expect("light_rt");
                    let output = textures.render_target(hdr_rt).expect("hdr_rt");
                    rs.composite_pass
                        .render_to_target(gpu, scene, lightmap, output);
                } else if pass.handle == vignette_pass {
                    let input = textures.render_target(hdr_rt).expect("hdr_rt");
                    let output = textures.render_target(graded_rt).expect("graded_rt");
                    rs.vignette.apply_to_target(gpu, input, output);
                } else if pass.handle == bloom_pass {
                    let input = textures.render_target(graded_rt).expect("graded_rt");
                    let output = textures.render_target(bloom_rt).expect("bloom_rt");
                    rs.bloom.apply(gpu, input, output);
                } else if pass.handle == tonemap_pass {
                    let input = textures.render_target(bloom_rt).expect("bloom_rt");
                    rs.tonemap.apply_to_surface(gpu, input);
                }
                Ok(())
            });

            if let Err(err) = result {
                eprintln!("[cosmic_jellyfish] render graph error: {err}");
            }
        },
        // ── resize ──────────────────────────────────────────────────────
        move |_world, gpu, _old_size, new_size| {
            resize_graph.borrow_mut().destroy_physical_resources();
            if let Some(state) = resize_state.borrow_mut().as_mut() {
                state.resize(gpu, new_size[0], new_size[1]);
            }
        },
        // ── shutdown ────────────────────────────────────────────────────
        move |_world, _gpu| {
            shutdown_graph.borrow_mut().destroy_physical_resources();
        },
    );
}

// ── Deterministic PRNG ──────────────────────────────────────────────────────

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
