//! ✦ Neon Galaxy — a spectacular SkyEngine lighting showcase ✦
//!
//! Multi-layered spiral galaxies with orbiting neon lights, pulsing stars,
//! animated dust lanes, normal-mapped spheres, and the full HDR post-processing
//! pipeline (bloom → vignette → ACES tonemap).
//!
//! Move the mouse to control a bright torch light. Press Escape to exit.
//!
//! ```bash
//! cargo run --example neon_galaxy --features app --release
//! ```

use sky_engine::app::{App, AssetPlugin, InputPlugin, WindowPlugin};
use sky_engine::ecs::World;
use sky_engine::gpu::GpuContext;
use sky_engine::render::expert::{
    Bloom, CompositePass, Light2D, LightPass, RenderGraph, SpriteBatch, TargetSize, ToneMap,
    Vignette,
};
use sky_engine::render::{Camera, Color, Sprite, Texture};

// ═══════════════════════════════════════════════════════════════════════════
//  Constants
// ═══════════════════════════════════════════════════════════════════════════

const NUM_STARS: usize = 2000;
const NUM_DUST: usize = 600;
const NUM_CORE_PARTICLES: usize = 40;
const GALAXY_ARMS: usize = 5;
const GALAXY_SPREAD: f32 = 0.30;

// ═══════════════════════════════════════════════════════════════════════════
//  Particle types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
struct Star {
    /// Distance from galaxy centre
    arm_dist: f32,
    /// Which spiral arm (0..GALAXY_ARMS)
    arm_idx: usize,
    /// Offset angle from the arm spine
    angle_offset: f32,
    /// Perpendicular scatter
    scatter: f32,
    /// Base size
    size: f32,
    /// Hue (0..360)
    hue: f32,
    /// Hue drift speed
    hue_shift: f32,
    /// Pulse phase
    pulse_phase: f32,
    /// Luminosity (0..1)
    luminosity: f32,
    /// Orbital speed multiplier
    orbit_speed: f32,
}

#[derive(Clone)]
struct DustMote {
    arm_dist: f32,
    arm_idx: usize,
    angle_offset: f32,
    scatter: f32,
    size: f32,
    alpha: f32,
    orbit_speed: f32,
}

#[derive(Clone)]
struct CoreParticle {
    angle: f32,
    dist: f32,
    size: f32,
    hue: f32,
    speed: f32,
    pulse_phase: f32,
}

// ═══════════════════════════════════════════════════════════════════════════
//  Render state
// ═══════════════════════════════════════════════════════════════════════════

struct RenderState {
    camera: Camera,
    scene_batch: SpriteBatch,
    normal_batch: SpriteBatch,
    circle_tex: Texture,
    soft_circle_tex: Texture,
    normal_tex: Texture,
    light_pass: LightPass,
    composite_pass: CompositePass,
    vignette: Vignette,
    bloom: Bloom,
    tonemap: ToneMap,
}

impl RenderState {
    fn new(gpu: &GpuContext) -> Self {
        let hdr = wgpu::TextureFormat::Rgba16Float;

        let mut vignette = Vignette::new(gpu, hdr);
        vignette.intensity = 0.18;
        vignette.smoothness = 0.50;

        let mut bloom = Bloom::new(gpu, hdr);
        bloom.intensity = 0.25;
        bloom.spread = 0.70;

        let mut tonemap = ToneMap::new(gpu, gpu.surface_format());
        tonemap.exposure = 1.15;
        tonemap.gamma = 2.2;

        Self {
            camera: Camera::new(1280.0, 720.0),
            scene_batch: SpriteBatch::new(gpu),
            normal_batch: SpriteBatch::new(gpu),
            circle_tex: Texture::circle(gpu, 32),
            soft_circle_tex: soft_glow_texture(gpu, 64),
            normal_tex: Texture::circle_normal(gpu, 32),
            light_pass: LightPass::new(gpu, hdr),
            composite_pass: CompositePass::new(gpu, hdr),
            vignette,
            bloom,
            tonemap,
        }
    }

    fn resize(&mut self, _gpu: &GpuContext, _width: u32, _height: u32) {}
}

// ═══════════════════════════════════════════════════════════════════════════
//  Procedural texture: soft radial glow
// ═══════════════════════════════════════════════════════════════════════════

fn soft_glow_texture(ctx: &GpuContext, size: u32) -> Texture {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 * 0.5;
    let radius = center - 1.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let dist = (dx * dx + dy * dy).sqrt() / radius;
            // Smooth Gaussian-ish falloff
            let alpha = (1.0 - dist * dist).max(0.0).powi(3);
            let a = (alpha * 255.0).min(255.0) as u8;
            let i = ((y * size + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            data[i + 3] = a;
        }
    }
    Texture::from_rgba8(ctx, size, size, &data)
}

// ═══════════════════════════════════════════════════════════════════════════
//  Galaxy maths
// ═══════════════════════════════════════════════════════════════════════════

/// Compute world position of a particle on a spiral arm at time `t`.
fn spiral_position(
    arm_dist: f32,
    arm_idx: usize,
    angle_offset: f32,
    scatter: f32,
    time: f32,
    orbit_speed: f32,
) -> [f32; 2] {
    let arm_angle = (arm_idx as f32 / GALAXY_ARMS as f32) * std::f32::consts::TAU;
    let wind = arm_dist * 4.0; // tighter winding for visible spiral arms
    let theta = arm_angle + wind + angle_offset + time * orbit_speed;

    let r = arm_dist * 420.0; // scale to world units
    let x = theta.cos() * r + scatter * (theta * 3.7).sin() * 25.0;
    let y = theta.sin() * r + scatter * (theta * 5.1).cos() * 25.0;
    [x, y]
}

// ═══════════════════════════════════════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════════════════════════════════════

fn main() {
    let mut rng = SimpleRng::new(42);

    // ── Generate galaxy particles ───────────────────────────────────────
    let stars: Vec<Star> = (0..NUM_STARS)
        .map(|_| {
            let arm_dist = rng.range(0.02, 1.0).powf(0.5); // balanced spread
            Star {
                arm_dist,
                arm_idx: (rng.next_u64() as usize) % GALAXY_ARMS,
                angle_offset: rng.range(-GALAXY_SPREAD, GALAXY_SPREAD),
                scatter: rng.range(-1.0, 1.0),
                size: rng.range(1.5, 7.0) * (1.0 - arm_dist * 0.25),
                hue: rng.range(0.0, 360.0),
                hue_shift: rng.range(5.0, 60.0),
                pulse_phase: rng.range(0.0, std::f32::consts::TAU),
                luminosity: rng.range(0.3, 1.0),
                orbit_speed: rng.range(0.08, 0.28) / (arm_dist + 0.2),
            }
        })
        .collect();

    let dust: Vec<DustMote> = (0..NUM_DUST)
        .map(|_| {
            let arm_dist = rng.range(0.08, 0.95).powf(0.4);
            DustMote {
                arm_dist,
                arm_idx: (rng.next_u64() as usize) % GALAXY_ARMS,
                angle_offset: rng.range(-GALAXY_SPREAD * 1.5, GALAXY_SPREAD * 1.5),
                scatter: rng.range(-2.0, 2.0),
                size: rng.range(20.0, 55.0),
                alpha: rng.range(0.015, 0.05),
                orbit_speed: rng.range(0.06, 0.18) / (arm_dist + 0.2),
            }
        })
        .collect();

    let core_particles: Vec<CoreParticle> = (0..NUM_CORE_PARTICLES)
        .map(|_| CoreParticle {
            angle: rng.range(0.0, std::f32::consts::TAU),
            dist: rng.range(3.0, 45.0),
            size: rng.range(3.0, 12.0),
            hue: rng.range(10.0, 60.0), // warm core spectrum
            speed: rng.range(0.3, 1.2),
            pulse_phase: rng.range(0.0, std::f32::consts::TAU),
        })
        .collect();

    let mut graph = RenderGraph::new();
    let mut render_state: Option<RenderState> = None;
    let mut sim_time = 0.0f32;
    let mut last_size = [0u32; 2];

    let hdr = wgpu::TextureFormat::Rgba16Float;

    let scene_rt = graph.create_texture(|b| {
        b.name("scene_rt").size(TargetSize::Surface).format(hdr);
    });
    let normal_rt = graph.create_texture(|b| {
        b.name("normal_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba8Unorm);
    });
    let light_rt = graph.create_texture(|b| {
        b.name("light_rt").size(TargetSize::Surface).format(hdr);
    });
    let hdr_rt = graph.create_texture(|b| {
        b.name("hdr_rt").size(TargetSize::Surface).format(hdr);
    });
    let graded_rt = graph.create_texture(|b| {
        b.name("graded_rt").size(TargetSize::Surface).format(hdr);
    });
    let bloom_rt = graph.create_texture(|b| {
        b.name("bloom_rt").size(TargetSize::Surface).format(hdr);
    });

    let scene_pass = graph.add_render_pass("scene_batch", |s| {
        s.write_color_cleared(0, scene_rt, [0.005, 0.005, 0.012, 1.0]);
    });
    let normal_pass = graph.add_render_pass("normal_batch", |s| {
        s.write_color_cleared(0, normal_rt, [0.5, 0.5, 1.0, 1.0]);
    });
    let lighting_pass = graph.add_render_pass("lighting", |s| {
        s.read(normal_rt);
        s.write(light_rt);
    });
    let composite_pass = graph.add_render_pass("composite", |s| {
        s.read(scene_rt);
        s.read(light_rt);
        s.write(hdr_rt);
    });
    let vignette_pass = graph.add_render_pass("vignette", |s| {
        s.read(hdr_rt);
        s.write(graded_rt);
    });
    let bloom_graph = Bloom::setup_graph(
        &mut graph,
        graded_rt,
        bloom_rt,
        TargetSize::Surface,
        hdr,
        "bloom",
    );
    let tonemap_pass = graph.add_render_pass("tonemap", |s| {
        s.read(bloom_rt);
        s.write_surface();
    });

    let mut world = World::new();

    eprintln!("[neon_galaxy] Move the mouse to control the torch light.");

    world
        .install(WindowPlugin::new(
            "SkyEngine — \u{2726} Neon Galaxy \u{2726}",
            1280,
            720,
        ))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();

    App::new(world).run(move |ctx: &mut sky_engine::app::FrameContext| {
        ctx.world.tick();
        sim_time += ctx.dt;
        let time = sim_time;

        if render_state.is_none() {
            render_state = Some(RenderState::new(ctx.gpu()));
        }

        // Handle resize
        let size = ctx.surface_size();
        if size != last_size && last_size != [0, 0] {
            graph.destroy_physical_resources();
            if let Some(rs) = render_state.as_mut() {
                rs.resize(ctx.gpu(), size[0], size[1]);
            }
        }
        last_size = size;

        let [w, h] = size;
        let mouse = ctx.input.mouse_position();
        let mouse_world = {
            let rs = render_state.as_mut().unwrap();
            rs.camera.set_viewport(w as f32, h as f32);
            rs.camera.screen_to_world(mouse[0], mouse[1])
        };

        // ── Build lights ────────────────────────────────────────────
        let mut lights = Vec::with_capacity(10);
        let orbit_configs: &[(f32, f32, f32, [f32; 3], f32)] = &[
            (300.0, 0.30, 1.1, [1.0, 0.4, 0.85], 3200.0),
            (400.0, -0.22, 0.9, [0.3, 0.9, 0.6], 7500.0),
            (240.0, 0.50, 1.0, [0.4, 0.5, 1.0], 12000.0),
            (480.0, 0.15, 0.7, [1.0, 0.85, 0.3], 2200.0),
            (360.0, -0.38, 0.85, [0.9, 0.25, 0.5], 4500.0),
            (200.0, 0.60, 0.8, [0.2, 0.85, 0.9], 9500.0),
        ];
        for (i, (orbit_r, speed, intensity, tint, temperature)) in orbit_configs.iter().enumerate()
        {
            let angle = time * speed + (i as f32) * 1.047;
            let r = orbit_r + 40.0 * (time * 0.25 + i as f32).sin();
            let x = angle.cos() * r;
            let y = angle.sin() * r;
            let pulse = 1.0 + 0.15 * (time * 1.5 + i as f32 * 0.7).sin();
            lights.push(
                Light2D::new(x, y, 450.0 + 60.0 * pulse)
                    .temperature(*temperature)
                    .intensity(intensity * pulse)
                    .falloff(2.5)
                    .color(Color::rgb(tint[0], tint[1], tint[2])),
            );
        }
        let core_pulse = 1.0 + 0.08 * (time * 0.6).sin();
        lights.push(
            Light2D::new(0.0, 0.0, 400.0)
                .temperature(4500.0)
                .intensity(1.5 * core_pulse)
                .falloff(2.0)
                .color(Color::rgb(1.0, 0.93, 0.78)),
        );
        lights.push(
            Light2D::new(mouse_world[0], mouse_world[1], 220.0)
                .temperature(5500.0)
                .intensity(2.0)
                .falloff(1.8)
                .color(Color::rgb(1.0, 0.97, 0.92)),
        );

        // ── Draw scene sprites ──────────────────────────────────────
        let rs = render_state.as_mut().unwrap();
        let circle_tex = rs.circle_tex.clone();
        let soft_tex = rs.soft_circle_tex.clone();
        let normal_tex = rs.normal_tex.clone();

        // Dust lanes
        rs.scene_batch.set_texture(&soft_tex);
        for mote in &dust {
            let pos = spiral_position(
                mote.arm_dist,
                mote.arm_idx,
                mote.angle_offset,
                mote.scatter,
                time,
                mote.orbit_speed,
            );
            let drift = 1.0 + 0.3 * (time * 0.2 + mote.arm_dist * 10.0).sin();
            let s = mote.size * drift;
            let hue = (mote.arm_idx as f32 * 75.0 + mote.arm_dist * 120.0 + time * 4.0) % 360.0;
            let tint = Color::hsl(hue, 0.5, 0.35);
            rs.scene_batch
                .draw(Sprite::new(pos[0], pos[1], s, s).color(Color::new(
                    tint.r,
                    tint.g,
                    tint.b,
                    mote.alpha * drift,
                )));
        }

        // Core particles
        for cp in &core_particles {
            let a = cp.angle + time * cp.speed;
            let pulse = 1.0 + 0.25 * (time * 1.2 + cp.pulse_phase).sin();
            let r = cp.dist * pulse;
            let x = a.cos() * r;
            let y = a.sin() * r;
            let s = cp.size * pulse;
            let tint = Color::hsl(cp.hue + time * 8.0, 0.65, 0.55);
            rs.scene_batch
                .draw(Sprite::new(x, y, s, s).color(Color::new(tint.r, tint.g, tint.b, 0.4)));
        }

        // Stars
        rs.scene_batch.set_texture(&circle_tex);
        for star in &stars {
            let pos = spiral_position(
                star.arm_dist,
                star.arm_idx,
                star.angle_offset,
                star.scatter,
                time,
                star.orbit_speed,
            );
            let pulse = 1.0 + 0.22 * (time * 1.8 + star.pulse_phase).sin();
            let s = star.size * pulse;
            let hue = (star.hue + star.hue_shift * time) % 360.0;
            let lightness = 0.4 + 0.2 * star.luminosity;
            let saturation = 0.65 + 0.2 * (time * 0.5 + star.pulse_phase).cos();
            let tint = Color::hsl(hue, saturation, lightness);
            rs.scene_batch
                .draw(Sprite::new(pos[0], pos[1], s, s).color(Color::new(
                    tint.r,
                    tint.g,
                    tint.b,
                    0.92 * star.luminosity,
                )));
        }

        // Normal map pass
        rs.normal_batch.set_texture(&normal_tex);
        for star in &stars {
            let pos = spiral_position(
                star.arm_dist,
                star.arm_idx,
                star.angle_offset,
                star.scatter,
                time,
                star.orbit_speed,
            );
            let pulse = 1.0 + 0.22 * (time * 1.8 + star.pulse_phase).sin();
            let s = star.size * pulse;
            rs.normal_batch
                .draw(Sprite::new(pos[0], pos[1], s, s).color(Color::WHITE));
        }

        let camera = rs.camera;

        // ── Execute render graph ────────────────────────────────────
        let execute_result = graph.try_execute(ctx.gpu(), |pass, gpu, textures| {
            let rs = render_state.as_mut().unwrap();
            if pass.handle == scene_pass {
                let target = textures.render_target(scene_rt).expect("scene_rt");
                rs.scene_batch.flush_to_target(
                    gpu,
                    &camera,
                    target,
                    Some(Color::new(0.005, 0.005, 0.012, 1.0)),
                );
            } else if pass.handle == normal_pass {
                let target = textures.render_target(normal_rt).expect("normal_rt");
                rs.normal_batch.flush_to_target(
                    gpu,
                    &camera,
                    target,
                    Some(Color::new(0.5, 0.5, 1.0, 1.0)),
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
                    [0.04, 0.035, 0.055, 1.0],
                );
            } else if pass.handle == composite_pass {
                let scene = textures.render_target(scene_rt).expect("scene_rt");
                let lightmap = textures.render_target(light_rt).expect("light_rt");
                let output = textures.render_target(hdr_rt).expect("hdr_rt");
                rs.composite_pass
                    .render_to_target(gpu, scene, lightmap, output);
            } else if pass.handle == vignette_pass {
                let input = textures.render_target(hdr_rt).expect("hdr_rt");
                let output = textures.render_target(graded_rt).expect("graded_rt");
                rs.vignette.apply_to_target(gpu, input, output);
            } else if rs
                .bloom
                .execute_graph_pass(gpu, &bloom_graph, pass, textures)?
            {
            } else if pass.handle == tonemap_pass {
                let input = textures.render_target(bloom_rt).expect("bloom_rt");
                rs.tonemap.apply_to_surface(gpu, input);
            }
            Ok(())
        });

        if let Err(err) = execute_result {
            eprintln!("[neon_galaxy] render graph error: {err}");
        }
    });
}

// ═══════════════════════════════════════════════════════════════════════════
//  Minimal PRNG (same as lighting_demo)
// ═══════════════════════════════════════════════════════════════════════════

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
