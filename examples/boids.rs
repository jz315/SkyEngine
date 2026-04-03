//! # Boids Flocking Simulation
//!
//! Classic boids algorithm implemented with Sky Engine's system scheduling API.
//! The simulation keeps an ECS-driven update loop, but the hot path uses cached
//! snapshot buffers so the flock stays responsive at higher boid counts.
//!
//! Interactive features:
//! - Mouse acts as a predator, boids flee from cursor
//! - Left click places an attractor that fades over time
//! - Space triggers a panic scatter
//! - Color shifts from blue to red with speed
//!
//! ```sh
//! cargo run --example boids --features demo --release
//! ```

use minifb::{Key, MouseButton, MouseMode, Window, WindowOptions};
use rand::Rng;
use sky_engine::ecs::{EntityId, PreparedQuery, System, World};
use std::f32::consts::TAU;

const W: usize = 1024;
const H: usize = 768;
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
const BACKGROUND_COLOR: u32 = 0x05080C;
const BOID_SIZE: f32 = 5.5;
const TRAIL_LENGTH: f32 = 7.0;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

struct Input {
    mouse_x: f32,
    mouse_y: f32,
    mouse_valid: bool,
    click: bool,
    panic: bool,
}

impl Default for Input {
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
const GRID_COLS: usize = (W as f32 / CELL_SIZE) as usize + 1;
const GRID_ROWS: usize = (H as f32 / CELL_SIZE) as usize + 1;
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

        // Linked-list buckets avoid per-cell Vec churn in the neighbor pass.
        for (index, pos) in self.positions.iter().enumerate().rev() {
            let cell = Self::cell_index(pos.x, pos.y);
            self.next[index] = self.cell_heads[cell];
            self.cell_heads[cell] = index as i32;
        }
    }
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

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
            let input = world.get_resource::<Input>().unwrap();
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
            let input = world.get_resource::<Input>().unwrap();
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
        let width = W as f32;
        let height = H as f32;

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

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn speed_color(speed: f32) -> u32 {
    let t = ((speed - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)).clamp(0.0, 1.0);
    let hue = 240.0 * (1.0 - t);
    hsv_to_u32(hue, 0.85, 1.0)
}

fn hsv_to_u32(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (((r + m) * 255.0) as u32) << 16 | (((g + m) * 255.0) as u32) << 8 | ((b + m) * 255.0) as u32
}

fn scale_color(color: u32, factor: f32) -> u32 {
    let factor = factor.clamp(0.0, 1.0);
    let r = (((color >> 16) & 0xFF) as f32 * factor) as u32;
    let g = (((color >> 8) & 0xFF) as f32 * factor) as u32;
    let b = ((color & 0xFF) as f32 * factor) as u32;
    (r << 16) | (g << 8) | b
}

fn plot(buf: &mut [u32], x: i32, y: i32, color: u32) {
    if x >= 0 && x < W as i32 && y >= 0 && y < H as i32 {
        buf[y as usize * W + x as usize] = color;
    }
}

fn draw_line(buf: &mut [u32], x0: f32, y0: f32, x1: f32, y1: f32, color: u32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()).max(1.0) as usize;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        plot(buf, (x0 + dx * t) as i32, (y0 + dy * t) as i32, color);
    }
}

fn draw_boid(buf: &mut [u32], x: f32, y: f32, vx: f32, vy: f32, color: u32) {
    let angle = vy.atan2(vx);
    let speed = length(vx, vy);
    let trail = TRAIL_LENGTH * (speed / MAX_SPEED).clamp(0.35, 1.0);
    let trail_color = scale_color(color, 0.35);

    draw_line(
        buf,
        x,
        y,
        x - angle.cos() * trail,
        y - angle.sin() * trail,
        trail_color,
    );

    let tip_x = x + angle.cos() * BOID_SIZE;
    let tip_y = y + angle.sin() * BOID_SIZE;
    let left_x = x + (angle + 2.55).cos() * BOID_SIZE * 0.65;
    let left_y = y + (angle + 2.55).sin() * BOID_SIZE * 0.65;
    let right_x = x + (angle - 2.55).cos() * BOID_SIZE * 0.65;
    let right_y = y + (angle - 2.55).sin() * BOID_SIZE * 0.65;

    draw_line(buf, tip_x, tip_y, left_x, left_y, color);
    draw_line(buf, tip_x, tip_y, right_x, right_y, color);
    draw_line(
        buf,
        left_x,
        left_y,
        right_x,
        right_y,
        scale_color(color, 0.75),
    );
    plot(buf, tip_x as i32, tip_y as i32, 0xF6F8FF);
}

fn draw_ring(buf: &mut [u32], cx: f32, cy: f32, radius: f32, color: u32) {
    let segments = (radius * 1.5).max(16.0) as usize;
    for i in 0..segments {
        let angle = TAU * i as f32 / segments as f32;
        plot(
            buf,
            (cx + angle.cos() * radius) as i32,
            (cy + angle.sin() * radius) as i32,
            color,
        );
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let mut window = Window::new(
        "SkyEngine - Boids",
        W,
        H,
        WindowOptions {
            resize: false,
            ..Default::default()
        },
    )
    .unwrap();
    window.set_target_fps(0);

    let mut world = World::new();
    let mut buf = vec![0u32; W * H];
    let mut rng = rand::thread_rng();

    world.insert_resource(Input::default());
    world.insert_resource(BoidSnapshot::default());
    world.insert_resource(AttractorCache::default());

    for _ in 0..NUM_BOIDS {
        let angle = rng.gen_range(0.0..TAU);
        let speed = rng.gen_range(MIN_SPEED..MAX_SPEED);
        world.spawn((
            Pos {
                x: rng.gen_range(0.0..W as f32),
                y: rng.gen_range(0.0..H as f32),
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

    let mut last = std::time::Instant::now();
    let mut fps_timer = std::time::Instant::now();
    let mut fps_count = 0u32;
    let mut display_fps = 0.0f64;
    let mut display_sim_ms = 0.0f64;
    let mut display_draw_ms = 0.0f64;
    let mut display_present_ms = 0.0f64;
    let mut accum_sim = 0.0f64;
    let mut accum_draw = 0.0f64;
    let mut accum_present = 0.0f64;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = std::time::Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;

        let mouse = window
            .get_mouse_pos(MouseMode::Clamp)
            .unwrap_or((-1000.0, -1000.0));

        {
            let input = world.get_resource_mut::<Input>().unwrap();
            input.mouse_x = mouse.0;
            input.mouse_y = mouse.1;
            input.mouse_valid = mouse.0 >= 0.0 && mouse.0 < W as f32;
            input.click = window.get_mouse_down(MouseButton::Left);
            input.panic = window.is_key_down(Key::Space);
        }

        let sim_start = std::time::Instant::now();
        world.tick_with_delta(dt);
        let sim_ms = sim_start.elapsed().as_secs_f64() * 1000.0;

        let draw_start = std::time::Instant::now();
        buf.fill(BACKGROUND_COLOR);

        {
            let attractors = world.get_resource::<AttractorCache>().unwrap();
            for attractor in &attractors.items {
                let alpha = (attractor.life / ATTRACTOR_LIFETIME).clamp(0.0, 1.0);
                let pulse = (attractor.life * 5.0).sin() * 0.25 + 0.75;
                let outer = scale_color(0x66FF99, alpha * pulse);
                let inner = scale_color(0xD8FFAA, alpha * 0.65);
                draw_ring(
                    &mut buf,
                    attractor.x,
                    attractor.y,
                    ATTRACTOR_RANGE * 0.28,
                    outer,
                );
                draw_ring(
                    &mut buf,
                    attractor.x,
                    attractor.y,
                    ATTRACTOR_RANGE * 0.14,
                    inner,
                );
            }
        }

        if mouse.0 >= 0.0 && mouse.0 < W as f32 {
            draw_ring(&mut buf, mouse.0, mouse.1, PREDATOR_RANGE * 0.5, 0x7A2F22);
            draw_ring(&mut buf, mouse.0, mouse.1, PREDATOR_RANGE * 0.25, 0xC44C33);
        }

        {
            let snapshot = world.get_resource::<BoidSnapshot>().unwrap();
            for (pos, vel) in snapshot.positions.iter().zip(snapshot.velocities.iter()) {
                draw_boid(
                    &mut buf,
                    pos.x,
                    pos.y,
                    vel.x,
                    vel.y,
                    speed_color(length(vel.x, vel.y)),
                );
            }
        }
        let draw_ms = draw_start.elapsed().as_secs_f64() * 1000.0;

        fps_count += 1;
        accum_sim += sim_ms;
        accum_draw += draw_ms;

        if fps_timer.elapsed().as_secs_f64() >= 0.5 {
            let elapsed = fps_timer.elapsed().as_secs_f64();
            display_fps = fps_count as f64 / elapsed;
            let frames = fps_count.max(1) as f64;
            display_sim_ms = accum_sim / frames;
            display_draw_ms = accum_draw / frames;
            display_present_ms = accum_present / frames;
            fps_count = 0;
            accum_sim = 0.0;
            accum_draw = 0.0;
            accum_present = 0.0;
            fps_timer = std::time::Instant::now();
        }

        window.set_title(&format!(
            "SkyEngine Boids | {} boids | {:.0} FPS | sim {:.2} ms draw {:.2} ms present {:.2} ms | Mouse=predator Click=attractor Space=scatter",
            NUM_BOIDS, display_fps, display_sim_ms, display_draw_ms, display_present_ms
        ));

        let present_start = std::time::Instant::now();
        window.update_with_buffer(&buf, W, H).unwrap();
        let present_ms = present_start.elapsed().as_secs_f64() * 1000.0;
        accum_present += present_ms;
    }
}
