//! Modern 2D lighting demo.
//!
//! ```bash
//! cargo run --example lighting_demo --features app --release
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use sky_engine::app::{App, AppConfig};
use sky_engine::gpu::GpuContext;
use sky_engine::render::{
    Bloom, Camera2D, Color, CompositePass, Light2D, LightPass, RenderGraph, Sprite, SpriteBatch,
    TargetSize, Texture, ToneMap, Vignette,
};

const NUM_ORBS: usize = 720;

#[derive(Clone)]
struct Orb {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    hue: f32,
    hue_shift: f32,
    pulse: f32,
}

struct RenderState {
    camera: Camera2D,
    scene_batch: SpriteBatch,
    normal_batch: SpriteBatch,
    circle_tex: Texture,
    normal_tex: Texture,
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
        vignette.intensity = 0.35;
        vignette.smoothness = 0.28;

        let mut bloom = Bloom::new(gpu, sw, sh, hdr);
        bloom.threshold = 0.55;
        bloom.intensity = 0.45;
        bloom.radius = 1.15;

        let mut tonemap = ToneMap::new(gpu, gpu.surface_format());
        tonemap.exposure = 1.35;
        tonemap.gamma = 2.2;

        Self {
            camera: Camera2D::new(1280.0, 720.0),
            scene_batch: SpriteBatch::new(gpu),
            normal_batch: SpriteBatch::new(gpu),
            circle_tex: Texture::circle(gpu, 96),
            normal_tex: Texture::circle_normal(gpu, 96),
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

fn main() {
    let mut rng = SimpleRng::new(1337);
    let sprites: Vec<Orb> = (0..NUM_ORBS)
        .map(|_| Orb {
            x: rng.range(-480.0, 480.0),
            y: rng.range(-320.0, 320.0),
            vx: rng.range(-32.0, 32.0),
            vy: rng.range(-32.0, 32.0),
            size: rng.range(14.0, 42.0),
            hue: rng.range(0.0, 360.0),
            hue_shift: rng.range(10.0, 80.0),
            pulse: rng.range(0.0, std::f32::consts::TAU),
        })
        .collect();

    let mut graph = RenderGraph::new();
    let render_state = Rc::new(RefCell::new(None::<RenderState>));
    let sim_time = Rc::new(RefCell::new(0.0f32));
    let orbs = Rc::new(RefCell::new(sprites));

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
        s.write_color_cleared(0, scene_rt, [0.015, 0.016, 0.02, 1.0]);
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
    let bloom_pass = graph.add_render_pass("bloom", |s| {
        s.read(graded_rt);
        s.write(bloom_rt);
    });
    let tonemap_pass = graph.add_render_pass("tonemap", |s| {
        s.read(bloom_rt);
        s.write_surface();
    });

    let graph = Rc::new(RefCell::new(graph));
    let frame_graph = Rc::clone(&graph);
    let resize_graph = Rc::clone(&graph);
    let shutdown_graph = Rc::clone(&graph);
    let frame_state = Rc::clone(&render_state);
    let resize_state = Rc::clone(&render_state);
    let frame_time = Rc::clone(&sim_time);
    let frame_orbs = Rc::clone(&orbs);

    App::run_with_lifecycle(
        AppConfig::new("SkyEngine — Lighting Demo", 1280, 720),
        |_world, _gpu| {
            eprintln!(
                "[lighting_demo] Move the mouse to drag the key light. Press Escape to exit."
            );
        },
        move |ctx| {
            *frame_time.borrow_mut() += ctx.dt;

            let mut state_ref = frame_state.borrow_mut();
            if state_ref.is_none() {
                *state_ref = Some(RenderState::new(ctx.gpu));
            }

            let [w, h] = ctx.gpu.surface_size();
            let mouse = ctx.input.mouse_position();
            let mouse_world = {
                let rs = state_ref.as_mut().expect("render state should exist");
                rs.camera.set_viewport(w as f32, h as f32);
                rs.camera.screen_to_world(mouse[0], mouse[1])
            };

            let mut orbs_ref = frame_orbs.borrow_mut();
            let time = *frame_time.borrow();
            for orb in &mut *orbs_ref {
                orb.x += orb.vx * ctx.dt;
                orb.y += orb.vy * ctx.dt;
                orb.hue = (orb.hue + orb.hue_shift * ctx.dt) % 360.0;
                orb.pulse += ctx.dt * 0.8;

                let half_w = w as f32 * 0.5 + orb.size;
                let half_h = h as f32 * 0.5 + orb.size;
                if orb.x > half_w {
                    orb.x = -half_w;
                }
                if orb.x < -half_w {
                    orb.x = half_w;
                }
                if orb.y > half_h {
                    orb.y = -half_h;
                }
                if orb.y < -half_h {
                    orb.y = half_h;
                }
            }

            let lights = vec![
                Light2D::new(-260.0, -50.0, 240.0)
                    .temperature(2600.0)
                    .intensity(2.1)
                    .falloff(1.7)
                    .color(Color::rgb(1.0, 0.78, 0.45)),
                Light2D::new(260.0, 180.0, 320.0)
                    .temperature(9200.0)
                    .intensity(1.2)
                    .falloff(2.0)
                    .color(Color::rgb(0.65, 0.8, 1.0)),
                Light2D::new(220.0, -170.0, 200.0)
                    .temperature(3400.0)
                    .intensity(1.55)
                    .falloff(1.9)
                    .color(Color::rgb(1.0, 0.92, 0.75)),
                Light2D::new(-70.0, 180.0, 160.0)
                    .temperature(5600.0)
                    .intensity(0.9)
                    .falloff(1.4)
                    .color(Color::rgb(0.9, 0.3, 0.8)),
                Light2D::new(mouse_world[0], mouse_world[1], 150.0)
                    .temperature(5000.0)
                    .intensity(1.8)
                    .falloff(1.4)
                    .color(Color::rgb(1.0, 0.95, 0.85)),
            ];

            let camera = {
                let rs = state_ref.as_mut().expect("render state should exist");

                rs.scene_batch.begin();
                rs.scene_batch.set_texture(&rs.circle_tex);
                for orb in &*orbs_ref {
                    let pulse = 1.0 + 0.18 * (time * 1.8 + orb.pulse).sin();
                    let lightness = 0.46 + 0.14 * (time * 0.7 + orb.pulse).cos();
                    let tint = Color::hsl(orb.hue, 0.72, lightness);
                    rs.scene_batch.draw(
                        Sprite::new(orb.x, orb.y, orb.size * pulse, orb.size * pulse)
                            .color(Color::new(tint.r, tint.g, tint.b, 0.95)),
                    );
                }

                rs.normal_batch.begin();
                rs.normal_batch.set_texture(&rs.normal_tex);
                for orb in &*orbs_ref {
                    let pulse = 1.0 + 0.18 * (time * 1.8 + orb.pulse).sin();
                    rs.normal_batch.draw(
                        Sprite::new(orb.x, orb.y, orb.size * pulse, orb.size * pulse)
                            .color(Color::WHITE),
                    );
                }

                rs.camera
            };

            let mut graph = frame_graph.borrow_mut();
            let execute_result = graph.try_execute(ctx.gpu, |pass, gpu, textures| {
                let rs = state_ref.as_mut().expect("render state should exist");

                if pass.handle == scene_pass {
                    let target = textures
                        .render_target(scene_rt)
                        .expect("scene_rt should resolve to a render target");
                    rs.scene_batch.draw_to_target(
                        gpu,
                        &camera,
                        target,
                        Some([0.015, 0.016, 0.02, 1.0]),
                    );
                } else if pass.handle == normal_pass {
                    let target = textures
                        .render_target(normal_rt)
                        .expect("normal_rt should resolve to a render target");
                    rs.normal_batch.draw_to_target(
                        gpu,
                        &camera,
                        target,
                        Some([0.5, 0.5, 1.0, 1.0]),
                    );
                } else if pass.handle == lighting_pass {
                    let normal_target = textures
                        .render_target(normal_rt)
                        .expect("normal_rt should resolve to a render target");
                    let output = textures
                        .render_target(light_rt)
                        .expect("light_rt should resolve to a render target");
                    rs.light_pass.render(
                        gpu,
                        &lights,
                        Some(normal_target),
                        output,
                        &camera,
                        [0.07, 0.075, 0.09, 1.0],
                    );
                } else if pass.handle == composite_pass {
                    let scene = textures
                        .render_target(scene_rt)
                        .expect("scene_rt should resolve to a render target");
                    let lightmap = textures
                        .render_target(light_rt)
                        .expect("light_rt should resolve to a render target");
                    let output = textures
                        .render_target(hdr_rt)
                        .expect("hdr_rt should resolve to a render target");
                    rs.composite_pass
                        .render_to_target(gpu, scene, lightmap, output);
                } else if pass.handle == vignette_pass {
                    let input = textures
                        .render_target(hdr_rt)
                        .expect("hdr_rt should resolve to a render target");
                    let output = textures
                        .render_target(graded_rt)
                        .expect("graded_rt should resolve to a render target");
                    rs.vignette.apply_to_target(gpu, input, output);
                } else if pass.handle == bloom_pass {
                    let input = textures
                        .render_target(graded_rt)
                        .expect("graded_rt should resolve to a render target");
                    let output = textures
                        .render_target(bloom_rt)
                        .expect("bloom_rt should resolve to a render target");
                    rs.bloom.apply(gpu, input, output);
                } else if pass.handle == tonemap_pass {
                    let input = textures
                        .render_target(bloom_rt)
                        .expect("bloom_rt should resolve to a render target");
                    rs.tonemap.apply_to_surface(gpu, input);
                }
                Ok(())
            });

            if let Err(err) = execute_result {
                eprintln!("[lighting_demo] render graph error: {err}");
            }
        },
        move |_world, gpu, _old_size, new_size| {
            resize_graph.borrow_mut().destroy_physical_resources();
            if let Some(state) = resize_state.borrow_mut().as_mut() {
                state.resize(gpu, new_size[0], new_size[1]);
            }
        },
        move |_world, _gpu| {
            shutdown_graph.borrow_mut().destroy_physical_resources();
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
