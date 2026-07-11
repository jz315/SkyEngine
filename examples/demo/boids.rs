//! # Spirit Wisps — Ori-style Boids
//!
//! 2000 spirit wisps flocking through a dark void, each leaving a fading
//! comet trail of light. Full HDR pipeline with aggressive bloom.
//!
//!   SpriteBatch → LightPass → Composite → Bloom → ToneMap
//!
//! Interactive:
//! - Mouse = predator (spirits flee)
//! - Left click = attractor (spirits gather)
//! - Space = panic scatter
//!
//! ```sh
//! cargo run --example boids --features app --release
//! ```

use std::f32::consts::TAU;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::{EntityId, ExclusiveSystem, PreparedQuery, Update, World};
use sky_engine::gpu::GpuContext;
use sky_engine::input::KeyCode;
use sky_engine::math::Transform;
use sky_engine::render::expert::{
    Bloom, BloomGraph, CompositePass, Light2D, LightPass, PassHandle, RenderGraph, SpriteBatch,
    TargetSize, TextureHandle, ToneMap,
};
use sky_engine::render::{Camera, Color, Sprite, Texture};

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
const BOID_SIZE: f32 = 4.5;

// ─── Trail config ───────────────────────────────────────────────────────────

const TRAIL_LENGTH: usize = 14;
const TRAIL_SAMPLE_INTERVAL: f32 = 0.018;

// ─── Dust config ────────────────────────────────────────────────────────────

const NUM_DUST: usize = 400;

// ─── Color palette ──────────────────────────────────────────────────────────

const BG_COLOR: Color = Color::new(0.01, 0.02, 0.05, 1.0);
const TRAIL_COLD: Color = Color::new(0.10, 0.35, 0.95, 1.0);
const TRAIL_WARM: Color = Color::new(0.55, 1.00, 0.85, 1.0);
const SPIRIT_CORE: Color = Color::new(0.78, 0.96, 1.00, 1.0);
const SPIRIT_GLOW: Color = Color::new(0.24, 0.92, 0.82, 1.0);
const AMBIENT_COLOR: Color = Color::new(0.07, 0.08, 0.15, 1.0);

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

// ─── ECS Resources ──────────────────────────────────────────────────────────

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

// ─── Trail history ──────────────────────────────────────────────────────────

struct TrailHistory {
    positions: Vec<[f32; 2]>,
    heads: Vec<u8>,
    counts: Vec<u8>,
    timer: f32,
}

impl TrailHistory {
    fn new(num_boids: usize) -> Self {
        Self {
            positions: vec![[0.0; 2]; num_boids * TRAIL_LENGTH],
            heads: vec![0; num_boids],
            counts: vec![0; num_boids],
            timer: 0.0,
        }
    }

    fn sample(&mut self, boid_positions: &[Pos], dt: f32) {
        self.timer += dt;
        if self.timer < TRAIL_SAMPLE_INTERVAL {
            return;
        }
        self.timer -= TRAIL_SAMPLE_INTERVAL;
        let n = boid_positions.len().min(self.heads.len());
        for i in 0..n {
            let head = self.heads[i] as usize;
            let base = i * TRAIL_LENGTH;
            self.positions[base + head] = [boid_positions[i].x, boid_positions[i].y];
            self.heads[i] = ((head + 1) % TRAIL_LENGTH) as u8;
            if (self.counts[i] as usize) < TRAIL_LENGTH {
                self.counts[i] += 1;
            }
        }
    }

    #[inline]
    fn iter_trail(&self, boid_index: usize) -> TrailIter<'_> {
        let count = self.counts[boid_index] as usize;
        let head = self.heads[boid_index] as usize;
        let base = boid_index * TRAIL_LENGTH;
        let start = (head + TRAIL_LENGTH - count) % TRAIL_LENGTH;
        TrailIter {
            positions: &self.positions[base..base + TRAIL_LENGTH],
            start,
            count,
            current: 0,
        }
    }
}

struct TrailIter<'a> {
    positions: &'a [[f32; 2]],
    start: usize,
    count: usize,
    current: usize,
}

impl<'a> Iterator for TrailIter<'a> {
    type Item = ([f32; 2], f32);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.count {
            return None;
        }
        let slot = (self.start + self.current) % TRAIL_LENGTH;
        let age_frac = if self.count > 1 {
            self.current as f32 / (self.count - 1) as f32
        } else {
            1.0
        };
        self.current += 1;
        Some((self.positions[slot], age_frac))
    }
}

// ─── Dust motes ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct DustMote {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    brightness: f32,
    twinkle_phase: f32,
    twinkle_speed: f32,
}

// ─── Render graph handles ───────────────────────────────────────────────────

#[derive(Clone)]
struct GraphHandles {
    scene_rt: TextureHandle,
    normal_rt: TextureHandle,
    light_rt: TextureHandle,
    hdr_rt: TextureHandle,
    bloom_rt: TextureHandle,
    scene_pass: PassHandle,
    normal_pass: PassHandle,
    lighting_pass: PassHandle,
    composite_pass: PassHandle,
    bloom_graph: BloomGraph,
    tonemap_pass: PassHandle,
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
    let l = length_sq(x, y);
    if l > 1.0e-6 {
        let inv = l.sqrt().recip();
        (x * inv, y * inv)
    } else {
        (0.0, 0.0)
    }
}

#[inline(always)]
fn limit_vector(x: f32, y: f32, max: f32) -> (f32, f32) {
    let l = length_sq(x, y);
    if l > max * max {
        let s = max / l.sqrt();
        (x * s, y * s)
    } else {
        (x, y)
    }
}

#[inline(always)]
fn steer_towards(cx: f32, cy: f32, dx: f32, dy: f32, mf: f32) -> (f32, f32) {
    limit_vector(dx - cx, dy - cy, mf)
}

#[inline]
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
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

impl ExclusiveSystem for AttractorDecaySystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;
        let (click, mouse_valid, mouse_x, mouse_y) = {
            let i = world.get_resource::<InputState>().unwrap();
            (i.click, i.mouse_valid, i.mouse_x, i.mouse_y)
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
        let cached = &mut self.cached_items;
        self.query
            .for_each_with_entity(&mut *world, |entity, (pos, att)| {
                att.life -= dt;
                if att.life <= 0.0 {
                    dead.push(entity);
                } else {
                    cached.push(AttractorPoint {
                        x: pos.x,
                        y: pos.y,
                        life: att.life,
                    });
                }
            });
        for &e in &self.dead {
            world.despawn(e);
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
impl ExclusiveSystem for SnapshotSystem {
    fn run(&mut self, world: &mut World) {
        let snap = world.get_resource_mut::<BoidSnapshot>().unwrap();
        let mut positions = std::mem::take(&mut snap.positions);
        let mut velocities = std::mem::take(&mut snap.velocities);
        let steering = std::mem::take(&mut snap.steering);
        let cell_heads = std::mem::take(&mut snap.cell_heads);
        let next = std::mem::take(&mut snap.next);
        positions.clear();
        velocities.clear();
        self.query.for_each_chunk(&mut *world, |(cp, cv, _)| {
            positions.extend_from_slice(cp);
            velocities.extend_from_slice(cv);
        });
        let snap = world.get_resource_mut::<BoidSnapshot>().unwrap();
        snap.positions = positions;
        snap.velocities = velocities;
        snap.steering = steering;
        snap.cell_heads = cell_heads;
        snap.next = next;
        snap.rebuild_grid();
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
impl ExclusiveSystem for BoidStepSystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;
        let (mouse_x, mouse_y, mouse_valid, panic_mode) = {
            let i = world.get_resource::<InputState>().unwrap();
            (i.mouse_x, i.mouse_y, i.mouse_valid, i.panic)
        };
        self.attractors.clear();
        self.attractors
            .extend_from_slice(&world.get_resource::<AttractorCache>().unwrap().items);

        let snap = world.get_resource_mut::<BoidSnapshot>().unwrap();
        let mut positions = std::mem::take(&mut snap.positions);
        let mut velocities = std::mem::take(&mut snap.velocities);
        let mut steering = std::mem::take(&mut snap.steering);
        let cell_heads = std::mem::take(&mut snap.cell_heads);
        let next = std::mem::take(&mut snap.next);

        let vr2 = VISUAL_RANGE * VISUAL_RANGE;
        let sr2 = SEPARATION_RANGE * SEPARATION_RANGE;

        for index in 0..positions.len() {
            let pos = positions[index];
            let vel = velocities[index];
            let mut nc = 0.0f32;
            let (mut avx, mut avy) = (0.0f32, 0.0f32);
            let (mut cx, mut cy) = (0.0f32, 0.0f32);
            let (mut sx, mut sy) = (0.0f32, 0.0f32);
            let col = (pos.x / CELL_SIZE) as i32;
            let row = (pos.y / CELL_SIZE) as i32;
            for dr in -1..=1 {
                for dc in -1..=1 {
                    let nr = row + dr;
                    let nc2 = col + dc;
                    if nr < 0 || nr >= GRID_ROWS as i32 || nc2 < 0 || nc2 >= GRID_COLS as i32 {
                        continue;
                    }
                    let cell = nr as usize * GRID_COLS + nc2 as usize;
                    let mut h = cell_heads[cell];
                    while h >= 0 {
                        let o = h as usize;
                        h = next[o];
                        if o == index {
                            continue;
                        }
                        let dx = positions[o].x - pos.x;
                        let dy = positions[o].y - pos.y;
                        let d2 = length_sq(dx, dy);
                        if d2 <= 1.0e-4 || d2 > vr2 {
                            continue;
                        }
                        nc += 1.0;
                        avx += velocities[o].x;
                        avy += velocities[o].y;
                        cx += positions[o].x;
                        cy += positions[o].y;
                        if d2 < sr2 {
                            let d = d2.sqrt();
                            let f = 1.0 - d / SEPARATION_RANGE;
                            sx -= dx / d * f;
                            sy -= dy / d * f;
                        }
                    }
                }
            }
            let (mut ax, mut ay) = (0.0f32, 0.0f32);
            let pref = if panic_mode {
                MAX_SPEED
            } else {
                CRUISE_SPEED.max(length(vel.x, vel.y))
            };
            if nc > 0.0 {
                let inv = nc.recip();
                let (adx, ady) = normalize_or_zero(avx * inv, avy * inv);
                let (s1x, s1y) = steer_towards(vel.x, vel.y, adx * pref, ady * pref, MAX_STEERING);
                ax += s1x * ALIGNMENT_WEIGHT;
                ay += s1y * ALIGNMENT_WEIGHT;
                let (cdx, cdy) = normalize_or_zero(cx * inv - pos.x, cy * inv - pos.y);
                let (s2x, s2y) = steer_towards(
                    vel.x,
                    vel.y,
                    cdx * CRUISE_SPEED,
                    cdy * CRUISE_SPEED,
                    MAX_STEERING,
                );
                ax += s2x * COHESION_WEIGHT;
                ay += s2y * COHESION_WEIGHT;
                let (sdx, sdy) = normalize_or_zero(sx, sy);
                ax += sdx * SEPARATION_WEIGHT;
                ay += sdy * SEPARATION_WEIGHT;
            }
            if mouse_valid {
                let (fx, fy) = (pos.x - mouse_x, pos.y - mouse_y);
                let d2 = length_sq(fx, fy);
                if d2 > 1.0e-4 && d2 < PREDATOR_RANGE * PREDATOR_RANGE {
                    let strength = 1.0 - d2.sqrt() / PREDATOR_RANGE;
                    let (dx, dy) = normalize_or_zero(fx, fy);
                    ax += dx * PREDATOR_WEIGHT * strength;
                    ay += dy * PREDATOR_WEIGHT * strength;
                }
            }
            for att in &self.attractors {
                let (tx, ty) = (att.x - pos.x, att.y - pos.y);
                let d2 = length_sq(tx, ty);
                if d2 <= 1.0e-4 || d2 > ATTRACTOR_RANGE * ATTRACTOR_RANGE {
                    continue;
                }
                let strength =
                    (1.0 - d2.sqrt() / ATTRACTOR_RANGE) * (att.life / ATTRACTOR_LIFETIME);
                let (dx, dy) = normalize_or_zero(tx, ty);
                ax += dx * ATTRACTOR_WEIGHT * strength;
                ay += dy * ATTRACTOR_WEIGHT * strength;
            }
            if panic_mode {
                let a = (index as f32 * 2.399_963_1) % TAU;
                ax += a.cos() * PANIC_WEIGHT;
                ay += a.sin() * PANIC_WEIGHT;
            }
            if pos.x < WALL_MARGIN {
                ax += (1.0 - pos.x / WALL_MARGIN) * WALL_WEIGHT;
            } else if pos.x > W - WALL_MARGIN {
                ax -= (1.0 - (W - pos.x) / WALL_MARGIN) * WALL_WEIGHT;
            }
            if pos.y < WALL_MARGIN {
                ay += (1.0 - pos.y / WALL_MARGIN) * WALL_WEIGHT;
            } else if pos.y > H - WALL_MARGIN {
                ay -= (1.0 - (H - pos.y) / WALL_MARGIN) * WALL_WEIGHT;
            }
            let (ax, ay) = limit_vector(ax, ay, MAX_STEERING);
            steering[index].x = ax;
            steering[index].y = ay;
        }

        let drag = (1.0 - DRAG * dt).max(0.0);
        let max_speed = if panic_mode {
            MAX_SPEED * 1.35
        } else {
            MAX_SPEED
        };
        let mut index = 0usize;
        self.query.for_each(&mut *world, |(pos, vel, _)| {
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
            pos.x = (pos.x + vel.x * dt).clamp(0.0, W - 1.0);
            pos.y = (pos.y + vel.y * dt).clamp(0.0, H - 1.0);
            if (pos.x <= 0.0 && vel.x < 0.0) || (pos.x >= W - 1.0 && vel.x > 0.0) {
                vel.x *= -0.25;
            }
            if (pos.y <= 0.0 && vel.y < 0.0) || (pos.y >= H - 1.0 && vel.y > 0.0) {
                vel.y *= -0.25;
            }
            positions[index] = *pos;
            velocities[index] = *vel;
            index += 1;
        });

        let snap = world.get_resource_mut::<BoidSnapshot>().unwrap();
        snap.positions = positions;
        snap.velocities = velocities;
        snap.steering = steering;
        snap.cell_heads = cell_heads;
        snap.next = next;
    }
}

// ─── Render state (owned by the lifecycle struct, not Rc'd) ─────────────────

struct RenderState {
    camera: Camera,
    scene_batch: SpriteBatch,
    normal_batch: SpriteBatch,
    circle_tex: Texture,
    soft_glow_tex: Texture,
    normal_tex: Texture,
    dot_tex: Texture,
    light_pass: LightPass,
    composite_pass: CompositePass,
    bloom: Bloom,
    tonemap: ToneMap,
}

impl RenderState {
    fn new(gpu: &GpuContext) -> Self {
        let hdr = wgpu::TextureFormat::Rgba16Float;
        let mut bloom = Bloom::new(gpu, hdr);
        bloom.intensity = 0.85;
        bloom.spread = 1.8;
        let mut tonemap = ToneMap::new(gpu, gpu.surface_format());
        tonemap.exposure = 2.2;
        tonemap.gamma = 2.2;
        Self {
            camera: Camera::new(W, H),
            scene_batch: SpriteBatch::new(gpu),
            normal_batch: SpriteBatch::new(gpu),
            circle_tex: Texture::circle(gpu, 32),
            soft_glow_tex: make_soft_glow(gpu, 64),
            normal_tex: Texture::circle_normal(gpu, 32),
            dot_tex: Texture::circle(gpu, 8),
            light_pass: LightPass::new(gpu, hdr),
            composite_pass: CompositePass::new(gpu, hdr),
            bloom,
            tonemap,
        }
    }
    fn resize(&mut self, _gpu: &GpuContext, _w: u32, _h: u32) {}
}

fn make_soft_glow(gpu: &GpuContext, size: u32) -> Texture {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 * 0.5;
    let radius = center - 0.5;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let t = ((dx * dx + dy * dy).sqrt() / radius).min(1.0);
            let v = 1.0 - t * t;
            let v = (v * v * 255.0) as u8;
            let idx = ((y * size + x) * 4) as usize;
            data[idx] = v;
            data[idx + 1] = v;
            data[idx + 2] = v;
            data[idx + 3] = v;
        }
    }
    Texture::from_rgba8(gpu, size, size, &data)
}

// ── The application lifecycle ──────────────────────────────────────────────

struct SpiritWispsApp {
    render: Option<RenderState>,
    graph: Option<RenderGraph>,
    handles: Option<GraphHandles>,
    trails: TrailHistory,
    dust: Vec<DustMote>,
    lights: Vec<Light2D>,
    fps_smooth: f32,
    frame_count: u32,
    last_size: [u32; 2],
}

impl SpiritWispsApp {
    fn new(rng: &mut SimpleRng) -> Self {
        let dust: Vec<DustMote> = (0..NUM_DUST)
            .map(|_| DustMote {
                x: rng.range(0.0, W),
                y: rng.range(0.0, H),
                vx: rng.range(-5.0, 5.0),
                vy: rng.range(-3.0, 3.0),
                size: rng.range(1.0, 3.5),
                brightness: rng.range(0.15, 0.6),
                twinkle_phase: rng.range(0.0, TAU),
                twinkle_speed: rng.range(0.8, 3.5),
            })
            .collect();

        Self {
            render: None,
            graph: None,
            handles: None,
            trails: TrailHistory::new(NUM_BOIDS),
            dust,
            lights: Vec::with_capacity(NUM_BOIDS + 16),
            fps_smooth: 0.0,
            frame_count: 0,
            last_size: [0; 2],
        }
    }

    fn init_graph(&mut self, gpu: &GpuContext) {
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
            s.write_color_cleared(0, scene_rt, BG_COLOR.to_array());
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
        let bloom_graph = Bloom::setup_graph(
            &mut graph,
            hdr_rt,
            bloom_rt,
            TargetSize::Surface,
            wgpu::TextureFormat::Rgba16Float,
            "bloom",
        );
        let tonemap_pass = graph.add_render_pass("tonemap", |s| {
            s.read(bloom_rt);
            s.write_surface();
        });

        self.handles = Some(GraphHandles {
            scene_rt,
            normal_rt,
            light_rt,
            hdr_rt,
            bloom_rt,
            scene_pass,
            normal_pass,
            lighting_pass,
            composite_pass,
            bloom_graph,
            tonemap_pass,
        });
        self.graph = Some(graph);
        self.render = Some(RenderState::new(gpu));
    }
}

impl AppState for SpiritWispsApp {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let gpu = ctx.gpu();
        self.init_graph(gpu);
    }

    fn on_resize(&mut self, _width: u32, _height: u32) {
        if let Some(g) = &mut self.graph {
            g.destroy_physical_resources();
        }
    }

    fn update(&mut self, ctx: &mut FrameContext) {
        let physical_size = ctx.physical_surface_size();
        let [win_w, win_h] = physical_size.to_array();

        // Handle resize for render state
        let size = [win_w, win_h];
        if size != self.last_size && self.last_size != [0, 0] {
            if let Some(r) = &mut self.render {
                r.resize(ctx.gpu(), win_w, win_h);
            }
        }
        self.last_size = size;

        let logical_size = ctx.logical_view_size();
        let mouse = ctx.input.mouse_logical_position();
        let mouse_sim_x = (mouse.x / logical_size.width.max(1.0)) * W;
        let mouse_sim_y = (mouse.y / logical_size.height.max(1.0)) * H;
        let mouse_valid = logical_size.contains(mouse);
        let mouse_render_y = H - mouse_sim_y;

        // ── Update ECS input resource ───────────────────────────────
        {
            let input = ctx.world.get_resource_mut::<InputState>().unwrap();
            input.mouse_x = mouse_sim_x;
            input.mouse_y = mouse_sim_y;
            input.mouse_valid = mouse_valid;
            input.click = ctx.input.mouse_left();
            input.panic = ctx.input.key_held(KeyCode::Space);
        }
        let dt = ctx.dt.min(0.05);

        // ── Read ECS data ───────────────────────────────────────────
        let snapshot = ctx.world.get_resource::<BoidSnapshot>().unwrap();
        let attractors = ctx.world.get_resource::<AttractorCache>().unwrap();

        self.trails.sample(&snapshot.positions, dt);

        // ── Update dust ─────────────────────────────────────────────
        for mote in &mut self.dust {
            mote.x += mote.vx * dt;
            mote.y += mote.vy * dt;
            mote.twinkle_phase += mote.twinkle_speed * dt;
            if mote.x < 0.0 {
                mote.x += W;
            }
            if mote.x > W {
                mote.x -= W;
            }
            if mote.y < 0.0 {
                mote.y += H;
            }
            if mote.y > H {
                mote.y -= H;
            }
        }

        // ── Build sprites & lights ──────────────────────────────────
        let rs = self.render.as_mut().unwrap();
        let h = self.handles.as_ref().unwrap();
        rs.camera.transform = Transform::from_xyz(W * 0.5, H * 0.5, 0.0);

        self.lights.clear();

        // Dust background
        rs.scene_batch.set_texture(&rs.dot_tex);
        for mote in &self.dust {
            let twinkle = mote.brightness * (0.5 + 0.5 * mote.twinkle_phase.sin());
            let ry = H - mote.y;
            rs.scene_batch.draw(
                Sprite::new(mote.x, ry, mote.size, mote.size).color(Color::new(
                    twinkle * 0.7,
                    twinkle * 0.75,
                    twinkle,
                    0.8,
                )),
            );
        }

        // Trail segments
        rs.scene_batch.set_texture(&rs.soft_glow_tex);
        for i in 0..snapshot.positions.len() {
            let speed = length(snapshot.velocities[i].x, snapshot.velocities[i].y);
            let glow = (speed / MAX_SPEED).clamp(0.2, 1.0);
            for (tpos, age) in self.trails.iter_trail(i) {
                let fade_sq = age * age;
                let sz = BOID_SIZE * (0.5 + age * 1.5) * glow;
                let alpha = fade_sq * 0.35 * glow;
                if alpha < 0.01 {
                    continue;
                }
                let c = lerp_color(TRAIL_COLD, TRAIL_WARM, age);
                let m = 0.8 + glow;
                rs.scene_batch
                    .draw(Sprite::new(tpos[0], H - tpos[1], sz, sz).color(Color::new(
                        c.r * m,
                        c.g * m,
                        c.b * m,
                        alpha,
                    )));
            }
        }

        // Boid bodies
        rs.scene_batch.set_texture(&rs.circle_tex);
        rs.normal_batch.set_texture(&rs.normal_tex);
        for (pos, vel) in snapshot.positions.iter().zip(snapshot.velocities.iter()) {
            let speed = (vel.x * vel.x + vel.y * vel.y).sqrt();
            let ry = H - pos.y;
            let angle = (-vel.y).atan2(vel.x);
            let body_w = BOID_SIZE * 2.0;
            let body_h = BOID_SIZE * 1.2;
            let glow_factor = (speed / MAX_SPEED).clamp(0.2, 1.0);

            // Core
            rs.scene_batch.draw(
                Sprite::new(pos.x, ry, body_w, body_h)
                    .rotation(angle)
                    .color(Color::new(
                        SPIRIT_CORE.r * (1.0 + glow_factor),
                        SPIRIT_CORE.g * (1.0 + glow_factor * 0.8),
                        SPIRIT_CORE.b * (0.8 + glow_factor * 0.5),
                        0.95,
                    )),
            );
            rs.normal_batch.draw(
                Sprite::new(pos.x, ry, body_w, body_h)
                    .rotation(angle)
                    .color(Color::WHITE),
            );

            // Glow halo
            let halo_size = BOID_SIZE * 3.5 * (0.8 + glow_factor * 0.4);
            rs.scene_batch.draw(
                Sprite::new(pos.x, ry, halo_size, halo_size).color(Color::new(
                    SPIRIT_GLOW.r * glow_factor,
                    SPIRIT_GLOW.g * glow_factor * 0.7,
                    SPIRIT_GLOW.b * glow_factor * 0.3,
                    0.12 * glow_factor,
                )),
            );

            // Light
            self.lights.push(
                Light2D::new(pos.x, ry, 35.0 + speed * 0.15)
                    .intensity(0.2 + glow_factor * 0.35)
                    .falloff(2.0)
                    .color(Color::rgb(
                        SPIRIT_GLOW.r * 0.8,
                        SPIRIT_GLOW.g * 0.6,
                        SPIRIT_GLOW.b * 0.3,
                    )),
            );
        }

        // Attractor visuals
        rs.scene_batch.set_texture(&rs.dot_tex);
        for attractor in &attractors.items {
            let alpha = (attractor.life / ATTRACTOR_LIFETIME).clamp(0.0, 1.0);
            let pulse = (attractor.life * 5.0).sin() * 0.25 + 0.75;
            let size = ATTRACTOR_RANGE * 0.25 * alpha;
            let ay = H - attractor.y;
            rs.scene_batch
                .draw(Sprite::new(attractor.x, ay, size, size).color(Color::new(
                    0.4 * pulse,
                    1.0 * pulse,
                    0.6 * pulse,
                    alpha * 0.5,
                )));
            self.lights.push(
                Light2D::new(attractor.x, ay, ATTRACTOR_RANGE * 0.5 * alpha)
                    .intensity(1.0 * alpha * pulse)
                    .falloff(1.8)
                    .color(Color::rgb(0.3, 1.0, 0.5)),
            );
        }

        // Predator ring
        if mouse_valid {
            let ring_size = PREDATOR_RANGE * 0.5;
            rs.scene_batch.draw(
                Sprite::new(mouse_sim_x, mouse_render_y, ring_size, ring_size)
                    .color(Color::new(1.0, 0.3, 0.2, 0.10)),
            );
            self.lights.push(
                Light2D::new(mouse_sim_x, mouse_render_y, PREDATOR_RANGE)
                    .intensity(1.2)
                    .falloff(1.6)
                    .color(Color::rgb(1.0, 0.35, 0.2)),
            );
        }

        // Ambient lights
        self.lights.push(
            Light2D::new(W * 0.5, H * 0.5, 1500.0)
                .intensity(0.30)
                .falloff(3.5)
                .color(AMBIENT_COLOR),
        );
        for &(cx, cy) in &[(0.0, 0.0), (W, 0.0), (0.0, H), (W, H)] {
            self.lights.push(
                Light2D::new(cx, cy, 700.0)
                    .intensity(0.18)
                    .falloff(2.8)
                    .color(Color::rgb(0.08, 0.06, 0.15)),
            );
        }

        let camera = rs.camera;
        let lights = &self.lights;
        let num_boid_sprites = snapshot.positions.len() * 3
            + self
                .trails
                .counts
                .iter()
                .map(|c| *c as usize)
                .sum::<usize>()
            + self.dust.len();
        let num_lights = self.lights.len();

        // ── Execute render graph ────────────────────────────────────
        let graph = self.graph.as_mut().unwrap();
        let result = graph.try_execute(ctx.gpu(), |pass, gpu, textures| {
            let rs = self.render.as_mut().unwrap();
            if pass.handle == h.scene_pass {
                let target = textures.render_target(h.scene_rt).expect("scene_rt");
                rs.scene_batch
                    .flush_to_target(gpu, &camera, target, Some(BG_COLOR));
            } else if pass.handle == h.normal_pass {
                let target = textures.render_target(h.normal_rt).expect("normal_rt");
                rs.normal_batch.flush_to_target(
                    gpu,
                    &camera,
                    target,
                    Some(Color::new(0.5, 0.5, 1.0, 1.0)),
                );
            } else if pass.handle == h.lighting_pass {
                let normal = textures.render_target(h.normal_rt).expect("normal_rt");
                let output = textures.render_target(h.light_rt).expect("light_rt");
                rs.light_pass.render(
                    gpu,
                    lights,
                    Some(normal),
                    output,
                    &camera,
                    [0.04, 0.03, 0.08, 1.0],
                );
            } else if pass.handle == h.composite_pass {
                let scene = textures.render_target(h.scene_rt).expect("scene_rt");
                let lightmap = textures.render_target(h.light_rt).expect("light_rt");
                let output = textures.render_target(h.hdr_rt).expect("hdr_rt");
                rs.composite_pass
                    .render_to_target(gpu, scene, lightmap, output);
            } else if rs
                .bloom
                .execute_graph_pass(gpu, &h.bloom_graph, pass, textures)?
            {
            } else if pass.handle == h.tonemap_pass {
                let input = textures.render_target(h.bloom_rt).expect("bloom_rt");
                rs.tonemap.apply_to_surface(gpu, input);
            }
            Ok(())
        });
        if let Err(err) = result {
            eprintln!("[spirit_wisps] render error: {err}");
        }

        // ── FPS display ─────────────────────────────────────────
        let fps_instant = if dt > 0.0 { 1.0 / dt } else { 0.0 };

        self.fps_smooth = if self.fps_smooth == 0.0 {
            fps_instant
        } else {
            self.fps_smooth * 0.95 + fps_instant * 0.05
        };
        self.frame_count += 1;
        if self.frame_count % 30 == 0 {
            ctx.set_title(&format!(
                "SkyEngine — Spirit Wisps | {:.0} FPS | {} sprites | {} lights",
                self.fps_smooth, num_boid_sprites, num_lights,
            ));
        }
    }
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    let mut rng = SimpleRng::new(42);

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
        .stage(Update)
        .add_exclusive(AttractorDecaySystem::new())
        .add_exclusive(SnapshotSystem::new())
        .add_exclusive(BoidStepSystem::new());

    eprintln!(
        "[spirit_wisps] {} wisps | Mouse=predator  Click=attractor  Space=scatter",
        NUM_BOIDS
    );

    world
        .install(WindowPlugin::new("SkyEngine — Spirit Wisps", 1280, 720))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();

    App::new(world).run(SpiritWispsApp::new(&mut rng));
}

// ─── PRNG ───────────────────────────────────────────────────────────────────

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
