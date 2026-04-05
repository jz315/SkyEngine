//! # Boids Flocking Simulation
//!
//! Classic boids algorithm implemented with Sky Engine's system scheduling API,
//! rendered through the full GPU-accelerated HDR pipeline:
//!
//!   SpriteBatch → LightPass → Composite → Bloom → Vignette → ToneMap
//!
//! Interactive features:
//! - Mouse acts as a predator, boids flee from cursor
//! - Left click places an attractor that fades over time
//! - Space triggers a panic scatter
//! - Color shifts from blue to red with speed
//! - Each boid emits a tiny point light — the flock glows!
//!
//! ```sh
//! cargo run --example boids_classic --features app --release
//! ```

use std::cell::RefCell;
use std::f32::consts::TAU;
use std::rc::Rc;

use sky_engine::app::{App, AppConfig, KeyCode};
use sky_engine::ecs::{EntityId, PreparedQuery, System, World};
use sky_engine::gpu::GpuContext;
use sky_engine::render::{Camera2D, Color, Sprite, Texture};
use sky_engine::render::expert::{
    Bloom, CompositePass, Light2D, LightPass, RenderGraph, SpriteBatch, TargetSize, ToneMap,
};

// ─── Configuration ──────────────────────────────────────────────────────────

const W: f32 = 1280.0;
const H: f32 = 720.0;
const NUM_BOIDS: usize = 2000;

const MAX_SPEED: f32 = 220.0;
const MIN_SPEED: f32 = 40.0;
const CRUISE_SPEED: f32 = 150.0;
const VISUAL_RANGE: f32 = 55.0;
const SEPARATION_RANGE: f32 = 18.0;
const PREDATOR_RANGE: f32 = 120.0;
const ATTRACTOR_RANGE: f32 = 200.0;
const WALL_MARGIN: f32 = 80.0;

const ALIGNMENT_WEIGHT: f32 = 0.65;
const COHESION_WEIGHT: f32 = 0.45;
const SEPARATION_WEIGHT: f32 = 210.0;
const PREDATOR_WEIGHT: f32 = 540.0;
const ATTRACTOR_WEIGHT: f32 = 120.0;
const PANIC_WEIGHT: f32 = 320.0;
const WALL_WEIGHT: f32 = 280.0;
const MAX_STEERING: f32 = 260.0;
const DRAG: f32 = 0.18;

const ATTRACTOR_LIFETIME: f32 = 5.0;
const BOID_SIZE: f32 = 5.5;

// ─── ECS Components ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
struct Pos {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Default)]
struct Vel {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Boid;

#[derive(Clone, Copy)]
struct Attractor {
    life: f32,
}

// ─── Resources ──────────────────────────────────────────────────────────────

struct InputState {
    mouse_x: f32,
    mouse_y: f32,
    mouse_valid: bool,
    click: bool,
    panic: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            mouse_x: -1000.0,
            mouse_y: -1000.0,
            mouse_valid: false,
            click: false,
            panic: false,
        }
    }
}

const CELL_SIZE: f32 = VISUAL_RANGE;
const GRID_COLS: usize = (W / CELL_SIZE) as usize + 1;
const GRID_ROWS: usize = (H / CELL_SIZE) as usize + 1;
const GRID_CELLS: usize = GRID_COLS * GRID_ROWS;

#[derive(Clone, Copy, Default)]
struct AttractorPoint {
    x: f32,
    y: f32,
    life: f32,
}

struct AttractorCache {
    items: Vec<AttractorPoint>,
}

impl Default for AttractorCache {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

struct BoidSnapshot {
    positions: Vec<Pos>,
    velocities: Vec<Vel>,
    steering: Vec<Vel>,
    cell_heads: Vec<i32>,
    next: Vec<i32>,
}

impl Default for BoidSnapshot {
    fn default() -> Self {
        Self {
            positions: Vec::with_capacity(NUM_BOIDS),
            velocities: Vec::with_capacity(NUM_BOIDS),
            steering: Vec::with_capacity(NUM_BOIDS),
            cell_heads: vec![-1; GRID_CELLS],
            next: Vec::with_capacity(NUM_BOIDS),
        }
    }
}

impl BoidSnapshot {
    #[inline(always)]
    fn cell_index(x: f32, y: f32) -> usize {
        let col = ((x / CELL_SIZE) as usize).min(GRID_COLS - 1);
        let row = ((y / CELL_SIZE) as usize).min(GRID_ROWS - 1);
        row * GRID_COLS + col
    }

    fn rebuild_grid(&mut self) {
        if self.cell_heads.len() != GRID_CELLS {
            self.cell_heads.resize(GRID_CELLS, -1);
        }
        self.cell_heads.fill(-1);

        self.next.clear();
        self.next.resize(self.positions.len(), -1);

        self.steering.clear();
        self.steering
            .resize(self.positions.len(), Vel { x: 0.0, y: 0.0 });

        for (index, pos) in self.positions.iter().enumerate().rev() {
            let cell = Self::cell_index(pos.x, pos.y);
            self.next[index] = self.cell_heads[cell];
            self.cell_heads[cell] = index as i32;
        }
    }
}

// ─── Math helpers ───────────────────────────────────────────────────────────

#[inline(always)]
fn length_sq(x: f32, y: f32) -> f32 {
    x * x + y * y
}

#[inline(always)]
fn length(x: f32, y: f32) -> f32 {
    length_sq(x, y).sqrt()
}

#[inline(always)]
fn normalize_or_zero(x: f32, y: f32) -> (f32, f32) {
    let len_sq = length_sq(x, y);
    if len_sq > 1.0e-6 {
        let inv_len = len_sq.sqrt().recip();
        (x * inv_len, y * inv_len)
    } else {
        (0.0, 0.0)
    }
}

#[inline(always)]
fn limit_vector(x: f32, y: f32, max_len: f32) -> (f32, f32) {
    let len_sq = length_sq(x, y);
    if len_sq > max_len * max_len {
        let scale = max_len / len_sq.sqrt();
        (x * scale, y * scale)
    } else {
        (x, y)
    }
}

#[inline(always)]
fn steer_towards(
    current_x: f32,
    current_y: f32,
    desired_x: f32,
    desired_y: f32,
    max_force: f32,
) -> (f32, f32) {
    limit_vector(desired_x - current_x, desired_y - current_y, max_force)
}

// ─── Systems ────────────────────────────────────────────────────────────────

struct AttractorDecaySystem {
    query: PreparedQuery<(&'static Pos, &'static mut Attractor)>,
    dead: Vec<EntityId>,
    cached_items: Vec<AttractorPoint>,
}

impl AttractorDecaySystem {
    fn new() -> Self {
        Self {
            query: PreparedQuery::new(),
            dead: Vec::new(),
            cached_items: Vec::new(),
        }
    }
}

impl System for AttractorDecaySystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;
        let (click, mouse_valid, mouse_x, mouse_y) = {
            let input = world.get_resource::<InputState>().unwrap();
            (input.click, input.mouse_valid, input.mouse_x, input.mouse_y)
        };

        if click && mouse_valid {
            world.spawn((
                Pos {
                    x: mouse_x,
                    y: mouse_y,
                },
                Attractor {
                    life: ATTRACTOR_LIFETIME,
                },
            ));
        }

        self.dead.clear();
        self.cached_items.clear();

        let dead = &mut self.dead;
        let cached_items = &mut self.cached_items;
        self.query
            .for_each_with_entity(world, |entity, (pos, attractor)| {
                attractor.life -= dt;
                if attractor.life <= 0.0 {
                    dead.push(entity);
                } else {
                    cached_items.push(AttractorPoint {
                        x: pos.x,
                        y: pos.y,
                        life: attractor.life,
                    });
                }
            });

        for &entity in &self.dead {
            world.despawn(entity);
        }

        let cache = world.get_resource_mut::<AttractorCache>().unwrap();
        cache.items.clear();
        cache.items.extend_from_slice(&self.cached_items);
    }
}

struct SnapshotSystem {
    query: PreparedQuery<(&'static Pos, &'static Vel, &'static Boid)>,
}

impl SnapshotSystem {
    fn new() -> Self {
        Self {
            query: PreparedQuery::new(),
        }
    }
}

impl System for SnapshotSystem {
    fn run(&mut self, world: &mut World) {
        let snapshot = world.get_resource_mut::<BoidSnapshot>().unwrap();
        let mut positions = std::mem::take(&mut snapshot.positions);
        let mut velocities = std::mem::take(&mut snapshot.velocities);
        let steering = std::mem::take(&mut snapshot.steering);
        let cell_heads = std::mem::take(&mut snapshot.cell_heads);
        let next = std::mem::take(&mut snapshot.next);

        positions.clear();
        velocities.clear();

        self.query
            .for_each_chunk(world, |(chunk_positions, chunk_velocities, _)| {
                positions.extend_from_slice(chunk_positions);
                velocities.extend_from_slice(chunk_velocities);
            });

        let snapshot = world.get_resource_mut::<BoidSnapshot>().unwrap();
        snapshot.positions = positions;
        snapshot.velocities = velocities;
        snapshot.steering = steering;
        snapshot.cell_heads = cell_heads;
        snapshot.next = next;
        snapshot.rebuild_grid();
    }
}

struct BoidStepSystem {
    query: PreparedQuery<(&'static mut Pos, &'static mut Vel, &'static Boid)>,
    attractors: Vec<AttractorPoint>,
}

impl BoidStepSystem {
    fn new() -> Self {
        Self {
            query: PreparedQuery::new(),
            attractors: Vec::new(),
        }
    }
}

impl System for BoidStepSystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;
        let (mouse_x, mouse_y, mouse_valid, panic_mode) = {
            let input = world.get_resource::<InputState>().unwrap();
            (input.mouse_x, input.mouse_y, input.mouse_valid, input.panic)
        };

        self.attractors.clear();
        self.attractors
            .extend_from_slice(&world.get_resource::<AttractorCache>().unwrap().items);

        let snapshot = world.get_resource_mut::<BoidSnapshot>().unwrap();
        let mut positions = std::mem::take(&mut snapshot.positions);
        let mut velocities = std::mem::take(&mut snapshot.velocities);
        let mut steering = std::mem::take(&mut snapshot.steering);
        let cell_heads = std::mem::take(&mut snapshot.cell_heads);
        let next = std::mem::take(&mut snapshot.next);

        let visual_range_sq = VISUAL_RANGE * VISUAL_RANGE;
        let separation_range_sq = SEPARATION_RANGE * SEPARATION_RANGE;
        let width = W;
        let height = H;

        for index in 0..positions.len() {
            let pos = positions[index];
            let vel = velocities[index];

            let mut neighbour_count = 0.0f32;
            let mut avg_vel_x = 0.0f32;
            let mut avg_vel_y = 0.0f32;
            let mut center_x = 0.0f32;
            let mut center_y = 0.0f32;
            let mut separation_x = 0.0f32;
            let mut separation_y = 0.0f32;

            let col = (pos.x / CELL_SIZE) as i32;
            let row = (pos.y / CELL_SIZE) as i32;

            for dr in -1..=1 {
                for dc in -1..=1 {
                    let next_row = row + dr;
                    let next_col = col + dc;
                    if next_row < 0
                        || next_row >= GRID_ROWS as i32
                        || next_col < 0
                        || next_col >= GRID_COLS as i32
                    {
                        continue;
                    }

                    let cell = next_row as usize * GRID_COLS + next_col as usize;
                    let mut head = cell_heads[cell];
                    while head >= 0 {
                        let other = head as usize;
                        head = next[other];

                        if other == index {
                            continue;
                        }

                        let dx = positions[other].x - pos.x;
                        let dy = positions[other].y - pos.y;
                        let dist_sq = length_sq(dx, dy);
                        if dist_sq <= 1.0e-4 || dist_sq > visual_range_sq {
                            continue;
                        }

                        neighbour_count += 1.0;
                        avg_vel_x += velocities[other].x;
                        avg_vel_y += velocities[other].y;
                        center_x += positions[other].x;
                        center_y += positions[other].y;

                        if dist_sq < separation_range_sq {
                            let dist = dist_sq.sqrt();
                            let falloff = 1.0 - dist / SEPARATION_RANGE;
                            separation_x -= dx / dist * falloff;
                            separation_y -= dy / dist * falloff;
                        }
                    }
                }
            }

            let mut accel_x = 0.0f32;
            let mut accel_y = 0.0f32;
            let preferred_speed = if panic_mode {
                MAX_SPEED
            } else {
                CRUISE_SPEED.max(length(vel.x, vel.y))
            };

            if neighbour_count > 0.0 {
                let inv_neighbours = neighbour_count.recip();

                let (align_dir_x, align_dir_y) =
                    normalize_or_zero(avg_vel_x * inv_neighbours, avg_vel_y * inv_neighbours);
                let (align_x, align_y) = steer_towards(
                    vel.x,
                    vel.y,
                    align_dir_x * preferred_speed,
                    align_dir_y * preferred_speed,
                    MAX_STEERING,
                );
                accel_x += align_x * ALIGNMENT_WEIGHT;
                accel_y += align_y * ALIGNMENT_WEIGHT;

                let center_dx = center_x * inv_neighbours - pos.x;
                let center_dy = center_y * inv_neighbours - pos.y;
                let (cohesion_dir_x, cohesion_dir_y) = normalize_or_zero(center_dx, center_dy);
                let (cohesion_x, cohesion_y) = steer_towards(
                    vel.x,
                    vel.y,
                    cohesion_dir_x * CRUISE_SPEED,
                    cohesion_dir_y * CRUISE_SPEED,
                    MAX_STEERING,
                );
                accel_x += cohesion_x * COHESION_WEIGHT;
                accel_y += cohesion_y * COHESION_WEIGHT;

                let (sep_dir_x, sep_dir_y) = normalize_or_zero(separation_x, separation_y);
                accel_x += sep_dir_x * SEPARATION_WEIGHT;
                accel_y += sep_dir_y * SEPARATION_WEIGHT;
            }

            if mouse_valid {
                let away_x = pos.x - mouse_x;
                let away_y = pos.y - mouse_y;
                let dist_sq = length_sq(away_x, away_y);
                if dist_sq > 1.0e-4 && dist_sq < PREDATOR_RANGE * PREDATOR_RANGE {
                    let dist = dist_sq.sqrt();
                    let strength = 1.0 - dist / PREDATOR_RANGE;
                    let (dir_x, dir_y) = normalize_or_zero(away_x, away_y);
                    accel_x += dir_x * PREDATOR_WEIGHT * strength;
                    accel_y += dir_y * PREDATOR_WEIGHT * strength;
                }
            }

            for attractor in &self.attractors {
                let to_x = attractor.x - pos.x;
                let to_y = attractor.y - pos.y;
                let dist_sq = length_sq(to_x, to_y);
                if dist_sq <= 1.0e-4 || dist_sq > ATTRACTOR_RANGE * ATTRACTOR_RANGE {
                    continue;
                }

                let dist = dist_sq.sqrt();
                let strength =
                    (1.0 - dist / ATTRACTOR_RANGE) * (attractor.life / ATTRACTOR_LIFETIME);
                let (dir_x, dir_y) = normalize_or_zero(to_x, to_y);
                accel_x += dir_x * ATTRACTOR_WEIGHT * strength;
                accel_y += dir_y * ATTRACTOR_WEIGHT * strength;
            }

            if panic_mode {
                let scatter_angle = (index as f32 * 2.399_963_1) % TAU;
                accel_x += scatter_angle.cos() * PANIC_WEIGHT;
                accel_y += scatter_angle.sin() * PANIC_WEIGHT;
            }

            if pos.x < WALL_MARGIN {
                accel_x += (1.0 - pos.x / WALL_MARGIN) * WALL_WEIGHT;
            } else if pos.x > width - WALL_MARGIN {
                accel_x -= (1.0 - (width - pos.x) / WALL_MARGIN) * WALL_WEIGHT;
            }

            if pos.y < WALL_MARGIN {
                accel_y += (1.0 - pos.y / WALL_MARGIN) * WALL_WEIGHT;
            } else if pos.y > height - WALL_MARGIN {
                accel_y -= (1.0 - (height - pos.y) / WALL_MARGIN) * WALL_WEIGHT;
            }

            let (accel_x, accel_y) = limit_vector(accel_x, accel_y, MAX_STEERING);
            steering[index].x = accel_x;
            steering[index].y = accel_y;
        }

        let drag = (1.0 - DRAG * dt).max(0.0);
        let max_speed = if panic_mode {
            MAX_SPEED * 1.35
        } else {
            MAX_SPEED
        };

        let mut index = 0usize;
        self.query.for_each(world, |(pos, vel, _)| {
            vel.x += steering[index].x * dt;
            vel.y += steering[index].y * dt;
            vel.x *= drag;
            vel.y *= drag;

            let speed = length(vel.x, vel.y);
            if speed > max_speed {
                vel.x = vel.x / speed * max_speed;
                vel.y = vel.y / speed * max_speed;
            } else if speed < MIN_SPEED && speed > 1.0e-4 {
                vel.x = vel.x / speed * MIN_SPEED;
                vel.y = vel.y / speed * MIN_SPEED;
            }

            pos.x = (pos.x + vel.x * dt).clamp(0.0, width - 1.0);
            pos.y = (pos.y + vel.y * dt).clamp(0.0, height - 1.0);

            if (pos.x <= 0.0 && vel.x < 0.0) || (pos.x >= width - 1.0 && vel.x > 0.0) {
                vel.x *= -0.25;
            }
            if (pos.y <= 0.0 && vel.y < 0.0) || (pos.y >= height - 1.0 && vel.y > 0.0) {
                vel.y *= -0.25;
            }

            positions[index] = *pos;
            velocities[index] = *vel;
            index += 1;
        });

        let snapshot = world.get_resource_mut::<BoidSnapshot>().unwrap();
        snapshot.positions = positions;
        snapshot.velocities = velocities;
        snapshot.steering = steering;
        snapshot.cell_heads = cell_heads;
        snapshot.next = next;
    }
}

// ─── Rendering helpers ──────────────────────────────────────────────────────

/// Map boid speed to a hue: slow=blue (240°), fast=red (0°).
fn speed_color(speed: f32) -> Color {
    let t = ((speed - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)).clamp(0.0, 1.0);
    let hue = 240.0 * (1.0 - t);
    Color::hsl(hue, 0.85, 0.55)
}

// ─── Render state ───────────────────────────────────────────────────────────

struct RenderState {
    camera: Camera2D,
    scene_batch: SpriteBatch,
    normal_batch: SpriteBatch,
    circle_tex: Texture,
    normal_tex: Texture,
    dot_tex: Texture,
    light_pass: LightPass,
    composite_pass: CompositePass,
    bloom: Bloom,
    tonemap: ToneMap,
}

impl RenderState {
    fn new(gpu: &GpuContext) -> Self {
        let [sw, sh] = gpu.surface_size();
        let hdr = wgpu::TextureFormat::Rgba16Float;

        let mut bloom = Bloom::new(gpu, sw, sh, hdr);
        bloom.threshold = 0.4;
        bloom.intensity = 0.55;
        bloom.radius = 1.2;

        let mut tonemap = ToneMap::new(gpu, gpu.surface_format());
        tonemap.exposure = 1.6;
        tonemap.gamma = 2.2;

        Self {
            camera: Camera2D::new(W, H),
            scene_batch: SpriteBatch::new(gpu),
            normal_batch: SpriteBatch::new(gpu),
            circle_tex: Texture::circle(gpu, 32),
            normal_tex: Texture::circle_normal(gpu, 32),
            dot_tex: Texture::circle(gpu, 8),
            light_pass: LightPass::new(gpu, hdr),
            composite_pass: CompositePass::new(gpu, hdr),
            bloom,
            tonemap,
        }
    }

    fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        self.bloom
            .resize(gpu, width, height, wgpu::TextureFormat::Rgba16Float);
    }
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    let mut rng = SimpleRng::new(42);

    // ── ECS setup ───────────────────────────────────────────────────────
    let mut world = World::new();

    world.insert_resource(InputState::default());
    world.insert_resource(BoidSnapshot::default());
    world.insert_resource(AttractorCache::default());

    for _ in 0..NUM_BOIDS {
        let angle = rng.range(0.0, TAU);
        let speed = rng.range(MIN_SPEED, MAX_SPEED);
        world.spawn((
            Pos {
                x: rng.range(0.0, W),
                y: rng.range(0.0, H),
            },
            Vel {
                x: angle.cos() * speed,
                y: angle.sin() * speed,
            },
            Boid,
        ));
    }

    world
        .group("simulation")
        .add(AttractorDecaySystem::new())
        .add(SnapshotSystem::new())
        .add(BoidStepSystem::new());

    // ── Render graph (handles declared once, reused every frame) ─────────

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
    let bloom_rt = graph.create_texture(|b| {
        b.name("bloom_rt")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });

    let scene_pass = graph.add_render_pass("scene_batch", |s| {
        s.write_color_cleared(0, scene_rt, [0.02, 0.03, 0.06, 1.0]);
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
    let bloom_pass = graph.add_render_pass("bloom", |s| {
        s.read(hdr_rt);
        s.write(bloom_rt);
    });
    let tonemap_pass = graph.add_render_pass("tonemap", |s| {
        s.read(bloom_rt);
        s.write_surface();
    });

    // ── Shared state via Rc<RefCell<>> ──────────────────────────────────

    let graph = Rc::new(RefCell::new(graph));
    let render_state = Rc::new(RefCell::new(None::<RenderState>));
    let world = Rc::new(RefCell::new(world));

    let frame_graph = Rc::clone(&graph);
    let resize_graph = Rc::clone(&graph);
    let shutdown_graph = Rc::clone(&graph);
    let frame_state = Rc::clone(&render_state);
    let resize_state = Rc::clone(&render_state);
    let frame_world = Rc::clone(&world);

    let fps_state = Rc::new(RefCell::new((0.0f32, 0u32))); // (fps_smooth, frame_count)
    let frame_fps = Rc::clone(&fps_state);

    let mut config = AppConfig::new("SkyEngine — Boids Classic", 1280, 720);
    config.vsync = false;
    App::run_with_lifecycle(
        config,
        // ── setup ───────────────────────────────────────────────────────
        |_world, _gpu| {
            eprintln!(
                "[boids] {} boids | Mouse=predator  Click=attractor  Space=scatter  Escape=quit",
                NUM_BOIDS
            );
        },
        // ── frame ───────────────────────────────────────────────────────
        move |ctx| {
            // Lazy-init render state on first frame
            let mut state_ref = frame_state.borrow_mut();
            if state_ref.is_none() {
                *state_ref = Some(RenderState::new(ctx.gpu));
            }

            let [win_w, win_h] = ctx.gpu.surface_size();
            let mouse = ctx.input.mouse_position();

            // Map mouse screen coords → simulation coords (0..W, 0..H top-left origin).
            // Screen Y is top-down (winit), sim Y is also top-down — direct mapping.
            let mouse_sim_x = (mouse[0] / win_w as f32) * W;
            let mouse_sim_y = (mouse[1] / win_h as f32) * H;
            let mouse_valid = mouse[0] >= 0.0 && mouse[0] < win_w as f32;

            // For rendering we flip Y because Camera2D uses +Y up.
            // render_y = H - sim_y
            let mouse_render_y = H - mouse_sim_y;

            // ── ECS: update input resource & tick simulation ─────────
            {
                let mut world = frame_world.borrow_mut();
                {
                    let input = world.get_resource_mut::<InputState>().unwrap();
                    input.mouse_x = mouse_sim_x;
                    input.mouse_y = mouse_sim_y;
                    input.mouse_valid = mouse_valid;
                    input.click = ctx.input.mouse_left();
                    input.panic = ctx.input.key_held(KeyCode::Space);
                }
                world.tick();
            }
            let dt = frame_world.borrow().time.delta.min(0.05);

            // ── Collect snapshot for rendering ──────────────────────
            let world = frame_world.borrow();

            let snapshot = world.get_resource::<BoidSnapshot>().unwrap();
            let attractors = world.get_resource::<AttractorCache>().unwrap();

            let rs = state_ref.as_mut().unwrap();

            // Camera centred so that world coords [0,W]×[0,H] fill the viewport.
            // +Y up, so sprites use render_y = H - sim_y.
            rs.camera.position = [W * 0.5, H * 0.5];

            // ── Build sprites ───────────────────────────────────────
            // Boid bodies (textured circles)
            rs.scene_batch.set_texture(&rs.circle_tex);
            rs.normal_batch.set_texture(&rs.normal_tex);

            let mut lights = Vec::with_capacity(snapshot.positions.len() + 8);

            for (pos, vel) in snapshot.positions.iter().zip(snapshot.velocities.iter()) {
                let speed = length(vel.x, vel.y);
                let color = speed_color(speed);

                // Flip Y for rendering (+Y up camera vs +Y down sim)
                let ry = H - pos.y;
                // Also flip vel.y so the rotation angle points correctly
                let angle = (-vel.y).atan2(vel.x);
                let body_w = BOID_SIZE * 2.2;
                let body_h = BOID_SIZE * 1.4;

                // Main body sprite (elongated in direction of travel)
                rs.scene_batch.draw(
                    Sprite::new(pos.x, ry, body_w, body_h)
                        .rotation(angle)
                        .color(Color::new(
                            color.r * 1.5,
                            color.g * 1.5,
                            color.b * 1.5,
                            0.95,
                        )),
                );
                rs.normal_batch.draw(
                    Sprite::new(pos.x, ry, body_w, body_h)
                        .rotation(angle)
                        .color(Color::WHITE),
                );

                // Bright tip (leading edge glow)
                let tip_x = pos.x + angle.cos() * BOID_SIZE * 0.8;
                let tip_y = ry + angle.sin() * BOID_SIZE * 0.8;
                let tip_size = BOID_SIZE * 0.6;
                let glow_factor = (speed / MAX_SPEED).clamp(0.3, 1.0);
                rs.scene_batch
                    .draw(
                        Sprite::new(tip_x, tip_y, tip_size, tip_size).color(Color::new(
                            color.r * 3.0 * glow_factor,
                            color.g * 3.0 * glow_factor,
                            color.b * 3.0 * glow_factor,
                            0.8,
                        )),
                    );

                // Per-boid point light
                lights.push(
                    Light2D::new(pos.x, ry, 40.0 + speed * 0.12)
                        .intensity(0.15 + glow_factor * 0.25)
                        .falloff(2.0)
                        .color(color),
                );
            }

            // Attractor visuals
            rs.scene_batch.set_texture(&rs.dot_tex);
            for attractor in &attractors.items {
                let alpha = (attractor.life / ATTRACTOR_LIFETIME).clamp(0.0, 1.0);
                let pulse = (attractor.life * 5.0).sin() * 0.25 + 0.75;
                let size = ATTRACTOR_RANGE * 0.3 * alpha;
                let ay = H - attractor.y;
                rs.scene_batch
                    .draw(Sprite::new(attractor.x, ay, size, size).color(Color::new(
                        0.4 * pulse,
                        1.0 * pulse,
                        0.6 * pulse,
                        alpha * 0.6,
                    )));
                // Attractor light
                lights.push(
                    Light2D::new(attractor.x, ay, ATTRACTOR_RANGE * 0.6 * alpha)
                        .intensity(1.2 * alpha * pulse)
                        .falloff(1.8)
                        .color(Color::rgb(0.3, 1.0, 0.5)),
                );
            }

            // Predator (mouse) ring
            if mouse_valid {
                let ring_size = PREDATOR_RANGE * 0.6;
                rs.scene_batch.draw(
                    Sprite::new(mouse_sim_x, mouse_render_y, ring_size, ring_size)
                        .color(Color::new(1.0, 0.3, 0.2, 0.15)),
                );
                lights.push(
                    Light2D::new(mouse_sim_x, mouse_render_y, PREDATOR_RANGE)
                        .intensity(1.5)
                        .falloff(1.6)
                        .color(Color::rgb(1.0, 0.35, 0.2)),
                );
            }

            // Ambient fill lights (4 corners + center) so the scene is never pitch black
            lights.push(
                Light2D::new(W * 0.5, H * 0.5, 1200.0)
                    .intensity(0.35)
                    .falloff(3.0)
                    .color(Color::rgb(0.15, 0.12, 0.25)),
            );
            lights.push(
                Light2D::new(0.0, 0.0, 600.0)
                    .intensity(0.25)
                    .falloff(2.5)
                    .color(Color::rgb(0.1, 0.15, 0.3)),
            );
            lights.push(
                Light2D::new(W, 0.0, 600.0)
                    .intensity(0.25)
                    .falloff(2.5)
                    .color(Color::rgb(0.1, 0.15, 0.3)),
            );
            lights.push(
                Light2D::new(0.0, H, 600.0)
                    .intensity(0.25)
                    .falloff(2.5)
                    .color(Color::rgb(0.1, 0.15, 0.3)),
            );
            lights.push(
                Light2D::new(W, H, 600.0)
                    .intensity(0.25)
                    .falloff(2.5)
                    .color(Color::rgb(0.1, 0.15, 0.3)),
            );

            let camera = rs.camera;

            // ── Execute render graph ────────────────────────────────
            let mut graph = frame_graph.borrow_mut();
            let result = graph.try_execute(ctx.gpu, |pass, gpu, textures| {
                let rs = state_ref.as_mut().unwrap();

                if pass.handle == scene_pass {
                    let target = textures.render_target(scene_rt).expect("scene_rt");
                    rs.scene_batch.flush_to_target(
                        gpu,
                        &camera,
                        target,
                        Some(Color::new(0.02, 0.03, 0.06, 1.0)),
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
                        [0.12, 0.10, 0.18, 1.0],
                    );
                } else if pass.handle == composite_pass_h {
                    let scene = textures.render_target(scene_rt).expect("scene_rt");
                    let lightmap = textures.render_target(light_rt).expect("light_rt");
                    let output = textures.render_target(hdr_rt).expect("hdr_rt");
                    rs.composite_pass
                        .render_to_target(gpu, scene, lightmap, output);
                } else if pass.handle == bloom_pass {
                    let input = textures.render_target(hdr_rt).expect("hdr_rt");
                    let output = textures.render_target(bloom_rt).expect("bloom_rt");
                    rs.bloom.apply(gpu, input, output);
                } else if pass.handle == tonemap_pass {
                    let input = textures.render_target(bloom_rt).expect("bloom_rt");
                    rs.tonemap.apply_to_surface(gpu, input);
                }
                Ok(())
            });

            if let Err(err) = result {
                eprintln!("[boids] render graph error: {err}");
            }

            // ── FPS display ─────────────────────────────────────────
            let mut fps = frame_fps.borrow_mut();
            let fps_instant = if dt > 0.0 { 1.0 / dt } else { 0.0 };
            fps.0 = if fps.0 == 0.0 {
                fps_instant
            } else {
                fps.0 * 0.95 + fps_instant * 0.05
            };
            fps.1 += 1;
            if fps.1 % 30 == 0 {
                ctx.window.set_title(&format!(
                    "SkyEngine — Boids Classic | {:.0} FPS | {} boids | {} lights",
                    fps.0,
                    snapshot.positions.len(),
                    lights.len(),
                ));
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

// ─── Deterministic PRNG ─────────────────────────────────────────────────────

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
