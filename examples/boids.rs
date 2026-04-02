//! # Boids Flocking Simulation
//!
//! Classic boids algorithm implemented with Sky Engine's **System** scheduling API.
//! Each behaviour rule is a separate system, grouped and ticked automatically.
//!
//! Interactive features:
//! - **Mouse** acts as a predator — boids flee from cursor
//! - **Left click** places an attractor (food source, fades after 5s)
//! - **Space** triggers a panic scatter
//! - Colour shifts from blue (slow) → green → yellow → red (fast)
//! - Motion trails via framebuffer fade
//!
//! ```
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
const VISUAL_RANGE: f32 = 55.0;
const SEPARATION_RANGE: f32 = 18.0;
const PREDATOR_RANGE: f32 = 120.0;
const ATTRACTOR_RANGE: f32 = 200.0;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Pos { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Vel { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Boid;

#[derive(Clone, Copy)]
struct Attractor { life: f32 }

// ---------------------------------------------------------------------------
// Resources — shared state between systems
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
        Self { mouse_x: -1000.0, mouse_y: -1000.0, mouse_valid: false, click: false, panic: false }
    }
}

/// Spatial hash grid cell size — must be >= VISUAL_RANGE.
const CELL_SIZE: f32 = VISUAL_RANGE;
const GRID_COLS: usize = (W as f32 / CELL_SIZE) as usize + 1; // 19
const GRID_ROWS: usize = (H as f32 / CELL_SIZE) as usize + 1; // 14
const GRID_CELLS: usize = GRID_COLS * GRID_ROWS;

/// Snapshot of all boid positions/velocities + spatial hash grid.
struct BoidSnapshot {
    positions: Vec<(f32, f32)>,
    velocities: Vec<(f32, f32)>,
    /// Each cell stores indices into the positions/velocities arrays.
    grid: Vec<Vec<usize>>,
}

impl Default for BoidSnapshot {
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            velocities: Vec::new(),
            grid: vec![Vec::new(); GRID_CELLS],
        }
    }
}

impl BoidSnapshot {
    fn cell_index(x: f32, y: f32) -> usize {
        let col = ((x / CELL_SIZE) as usize).min(GRID_COLS - 1);
        let row = ((y / CELL_SIZE) as usize).min(GRID_ROWS - 1);
        row * GRID_COLS + col
    }

    fn rebuild_grid(&mut self) {
        for cell in self.grid.iter_mut() { cell.clear(); }
        for (i, &(x, y)) in self.positions.iter().enumerate() {
            let ci = Self::cell_index(x, y);
            self.grid[ci].push(i);
        }
    }
}

struct AttractorCache {
    positions: Vec<(f32, f32)>,
}
impl Default for AttractorCache {
    fn default() -> Self { Self { positions: Vec::new() } }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Collects all attractor positions and decays their lifetimes.
struct AttractorDecaySystem {
    q_read: PreparedQuery<(&'static Pos, &'static Attractor)>,
    q_decay: PreparedQuery<&'static mut Attractor>,
}

impl AttractorDecaySystem {
    fn new() -> Self {
        Self { q_read: PreparedQuery::new(), q_decay: PreparedQuery::new() }
    }
}

impl System for AttractorDecaySystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;

        // Spawn attractor on click
        let input = world.get_resource::<Input>().unwrap();
        if input.click && input.mouse_valid {
            let mx = input.mouse_x;
            let my = input.mouse_y;
            world.spawn((Pos { x: mx, y: my }, Attractor { life: 5.0 }));
        }

        // Collect positions into local vec
        let mut attr_positions: Vec<(f32, f32)> = Vec::new();
        self.q_read.for_each(world, |(p, _)| {
            attr_positions.push((p.x, p.y));
        });

        // Write to resource
        let cache = world.get_resource_mut::<AttractorCache>().unwrap();
        cache.positions = attr_positions;

        // Decay
        let mut dead: Vec<EntityId> = Vec::new();
        self.q_decay.for_each_with_entity(world, |e, attr| {
            attr.life -= dt;
            if attr.life <= 0.0 { dead.push(e); }
        });
        for e in dead { world.despawn(e); }
    }
}

/// Snapshots all boid positions/velocities for the neighbour pass.
struct SnapshotSystem {
    query: PreparedQuery<(&'static Pos, &'static Vel, &'static Boid)>,
}

impl SnapshotSystem {
    fn new() -> Self { Self { query: PreparedQuery::new() } }
}

impl System for SnapshotSystem {
    fn run(&mut self, world: &mut World) {
        let mut positions: Vec<(f32, f32)> = Vec::new();
        let mut velocities: Vec<(f32, f32)> = Vec::new();

        self.query.for_each(world, |(p, v, _)| {
            positions.push((p.x, p.y));
            velocities.push((v.x, v.y));
        });

        let snap = world.get_resource_mut::<BoidSnapshot>().unwrap();
        snap.positions = positions;
        snap.velocities = velocities;
        snap.rebuild_grid();
    }
}

/// Core boid rules: separation, alignment, cohesion, predator avoidance,
/// attractor pull, wall steering, and speed clamping.
struct BoidRulesSystem {
    query: PreparedQuery<(&'static Pos, &'static mut Vel, &'static Boid)>,
}

impl BoidRulesSystem {
    fn new() -> Self { Self { query: PreparedQuery::new() } }
}

impl System for BoidRulesSystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;
        let input = world.get_resource::<Input>().unwrap();
        let mouse = (input.mouse_x, input.mouse_y);
        let panic_mode = input.panic;

        // Take data out of resources (zero-alloc move, not clone)
        let snap = world.get_resource_mut::<BoidSnapshot>().unwrap();
        let positions = std::mem::take(&mut snap.positions);
        let velocities = std::mem::take(&mut snap.velocities);
        let grid = std::mem::take(&mut snap.grid);

        let cache = world.get_resource_mut::<AttractorCache>().unwrap();
        let attractors = std::mem::take(&mut cache.positions);

        let mut idx = 0usize;
        self.query.for_each(world, |(pos, vel, _)| {
            let mut sep_x = 0.0f32;
            let mut sep_y = 0.0f32;
            let mut align_x = 0.0f32;
            let mut align_y = 0.0f32;
            let mut coh_x = 0.0f32;
            let mut coh_y = 0.0f32;
            let mut neighbours = 0u32;

            // Spatial grid: only check 3×3 neighboring cells
            let col = (pos.x / CELL_SIZE) as i32;
            let row = (pos.y / CELL_SIZE) as i32;
            for dr in -1..=1i32 {
                for dc in -1..=1i32 {
                    let nr = row + dr;
                    let nc = col + dc;
                    if nr < 0 || nr >= GRID_ROWS as i32 || nc < 0 || nc >= GRID_COLS as i32 {
                        continue;
                    }
                    let cell_idx = nr as usize * GRID_COLS + nc as usize;
                    for &j in &grid[cell_idx] {
                        if j == idx { continue; }
                        let dx = positions[j].0 - pos.x;
                        let dy = positions[j].1 - pos.y;
                        let dist_sq = dx * dx + dy * dy;

                        if dist_sq < VISUAL_RANGE * VISUAL_RANGE {
                            let dist = dist_sq.sqrt().max(0.01);
                            align_x += velocities[j].0;
                            align_y += velocities[j].1;
                            coh_x += positions[j].0;
                            coh_y += positions[j].1;
                            neighbours += 1;

                            if dist_sq < SEPARATION_RANGE * SEPARATION_RANGE {
                                sep_x -= dx / dist;
                                sep_y -= dy / dist;
                            }
                        }
                    }
                }
            }

            if neighbours > 0 {
                let n = neighbours as f32;
                vel.x += (align_x / n - vel.x) * 0.05;
                vel.y += (align_y / n - vel.y) * 0.05;
                vel.x += (coh_x / n - pos.x) * 0.005;
                vel.y += (coh_y / n - pos.y) * 0.005;
                vel.x += sep_x * 2.0;
                vel.y += sep_y * 2.0;
            }

            // Predator avoidance
            let pdx = pos.x - mouse.0;
            let pdy = pos.y - mouse.1;
            let pdist_sq = pdx * pdx + pdy * pdy;
            if pdist_sq < PREDATOR_RANGE * PREDATOR_RANGE && pdist_sq > 0.01 {
                let pdist = pdist_sq.sqrt();
                let strength = (1.0 - pdist / PREDATOR_RANGE) * 600.0;
                vel.x += pdx / pdist * strength * dt;
                vel.y += pdy / pdist * strength * dt;
            }

            // Attractor pull
            for &(ax, ay) in &attractors {
                let adx = ax - pos.x;
                let ady = ay - pos.y;
                let adist_sq = adx * adx + ady * ady;
                if adist_sq < ATTRACTOR_RANGE * ATTRACTOR_RANGE && adist_sq > 1.0 {
                    let adist = adist_sq.sqrt();
                    vel.x += adx / adist * 80.0 * dt;
                    vel.y += ady / adist * 80.0 * dt;
                }
            }

            // Panic scatter
            if panic_mode {
                let scatter_angle = (idx as f32 * 2.399) % TAU;
                vel.x += scatter_angle.cos() * 500.0 * dt;
                vel.y += scatter_angle.sin() * 500.0 * dt;
            }

            // Wall steering
            let margin = 60.0;
            let turn = 150.0;
            if pos.x < margin       { vel.x += turn * dt; }
            if pos.x > W as f32 - margin { vel.x -= turn * dt; }
            if pos.y < margin       { vel.y += turn * dt; }
            if pos.y > H as f32 - margin { vel.y -= turn * dt; }

            // Clamp speed
            let speed = (vel.x * vel.x + vel.y * vel.y).sqrt();
            let target_max = if panic_mode { MAX_SPEED * 1.5 } else { MAX_SPEED };
            if speed > target_max {
                vel.x = vel.x / speed * target_max;
                vel.y = vel.y / speed * target_max;
            }
            if speed < MIN_SPEED && speed > 0.01 {
                vel.x = vel.x / speed * MIN_SPEED;
                vel.y = vel.y / speed * MIN_SPEED;
            }

            idx += 1;
        });

        // Put vecs back so allocations are reused next frame
        let snap = world.get_resource_mut::<BoidSnapshot>().unwrap();
        snap.positions = positions;
        snap.velocities = velocities;
        snap.grid = grid;
        let cache = world.get_resource_mut::<AttractorCache>().unwrap();
        cache.positions = attractors;
    }
}

/// Integrates position from velocity.
struct MoveSystem {
    query: PreparedQuery<(&'static mut Pos, &'static Vel, &'static Boid)>,
}

impl MoveSystem {
    fn new() -> Self { Self { query: PreparedQuery::new() } }
}

impl System for MoveSystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.time.delta;
        self.query.for_each(world, |(pos, vel, _)| {
            pos.x += vel.x * dt;
            pos.y += vel.y * dt;
            if !(pos.x >= 0.0) { pos.x = 0.0; }
            if !(pos.x <= W as f32 - 1.0) { pos.x = W as f32 - 1.0; }
            if !(pos.y >= 0.0) { pos.y = 0.0; }
            if !(pos.y <= H as f32 - 1.0) { pos.y = H as f32 - 1.0; }
        });
    }
}

// ---------------------------------------------------------------------------
// Rendering helpers (unchanged)
// ---------------------------------------------------------------------------

fn speed_color(speed: f32) -> u32 {
    let t = ((speed - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)).max(0.0).min(1.0);
    let hue = 240.0 * (1.0 - t);
    hsv_to_u32(hue, 0.85, 1.0)
}

fn hsv_to_u32(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x),
        3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x),
    };
    (((r+m)*255.0) as u32) << 16 | (((g+m)*255.0) as u32) << 8 | ((b+m)*255.0) as u32
}

fn plot(buf: &mut [u32], x: i32, y: i32, col: u32) {
    if x >= 0 && x < W as i32 && y >= 0 && y < H as i32 {
        buf[y as usize * W + x as usize] = col;
    }
}

fn draw_line(buf: &mut [u32], x0: f32, y0: f32, x1: f32, y1: f32, col: u32) {
    let dx = x1 - x0; let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()).max(1.0) as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        plot(buf, (x0+dx*t) as i32, (y0+dy*t) as i32, col);
    }
}

fn draw_boid(buf: &mut [u32], x: f32, y: f32, vx: f32, vy: f32, col: u32) {
    let a = vy.atan2(vx); let sz = 6.0;
    let (tx, ty) = (x + a.cos()*sz, y + a.sin()*sz);
    let (lx, ly) = (x + (a+2.5).cos()*sz*0.6, y + (a+2.5).sin()*sz*0.6);
    let (rx, ry) = (x + (a-2.5).cos()*sz*0.6, y + (a-2.5).sin()*sz*0.6);
    draw_line(buf, tx, ty, lx, ly, col);
    draw_line(buf, tx, ty, rx, ry, col);
    draw_line(buf, lx, ly, rx, ry, col);
}

fn draw_ring(buf: &mut [u32], cx: f32, cy: f32, r: f32, col: u32) {
    let segs = (r * 1.5).max(16.0) as usize;
    for i in 0..segs {
        let a = TAU * i as f32 / segs as f32;
        plot(buf, (cx + a.cos()*r) as i32, (cy + a.sin()*r) as i32, col);
    }
}

fn fade_buffer(buf: &mut [u32], factor: u32) {
    for pixel in buf.iter_mut() {
        let r = ((*pixel >> 16) & 0xFF).saturating_sub(factor);
        let g = ((*pixel >> 8) & 0xFF).saturating_sub(factor);
        let b = (*pixel & 0xFF).saturating_sub(factor);
        *pixel = (r << 16) | (g << 8) | b;
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let mut window = Window::new("SkyEngine — Boids", W, H,
        WindowOptions { resize: false, ..Default::default() }).unwrap();
    window.set_target_fps(0);

    let mut world = World::new();
    let mut buf = vec![0u32; W * H];
    let mut rng = rand::thread_rng();

    // Resources
    world.insert_resource(Input::default());
    world.insert_resource(BoidSnapshot::default());
    world.insert_resource(AttractorCache::default());

    // Spawn boids
    for _ in 0..NUM_BOIDS {
        let angle = rng.gen_range(0.0..TAU);
        let speed = rng.gen_range(MIN_SPEED..MAX_SPEED);
        world.spawn((
            Pos { x: rng.gen_range(0.0..W as f32), y: rng.gen_range(0.0..H as f32) },
            Vel { x: angle.cos() * speed, y: angle.sin() * speed },
            Boid,
        ));
    }

    // Schedule systems
    world.group("simulation")
        .add(AttractorDecaySystem::new())
        .add(SnapshotSystem::new())
        .add(BoidRulesSystem::new())
        .add(MoveSystem::new());

    let mut last = std::time::Instant::now();
    let mut fps_timer = std::time::Instant::now();
    let mut fps_count = 0u32;
    let mut display_fps = 0.0f64;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = std::time::Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;

        // Update input resource
        let mouse = window.get_mouse_pos(MouseMode::Clamp).unwrap_or((-1000.0, -1000.0));
        {
            let input = world.get_resource_mut::<Input>().unwrap();
            input.mouse_x = mouse.0;
            input.mouse_y = mouse.1;
            input.mouse_valid = mouse.0 >= 0.0 && mouse.0 < W as f32;
            input.click = window.get_mouse_down(MouseButton::Left);
            input.panic = window.is_key_down(Key::Space);
        }

        // Tick all systems
        world.tick_with_delta(dt);

        // --- Render ---
        fade_buffer(&mut buf, 18);

        // Attractors
        {
            let mut q = world.query::<(&Pos, &Attractor)>();
            q.for_each(&world, |(pos, attr)| {
                let alpha = (attr.life / 5.0).max(0.0).min(1.0);
                let pulse = (attr.life * 4.0).sin() * 0.3 + 0.7;
                let g = (200.0 * alpha * pulse) as u32;
                draw_ring(&mut buf, pos.x, pos.y, ATTRACTOR_RANGE * 0.3, (g << 8) | 0x44);
                draw_ring(&mut buf, pos.x, pos.y, ATTRACTOR_RANGE * 0.15, (g << 8) | 0x44);
            });
        }

        // Predator ring
        if mouse.0 >= 0.0 && mouse.0 < W as f32 {
            draw_ring(&mut buf, mouse.0, mouse.1, PREDATOR_RANGE * 0.5, 0x442222);
        }

        // Boids
        {
            let mut q = world.query::<(&Pos, &Vel, &Boid)>();
            q.for_each(&world, |(pos, vel, _)| {
                let speed = (vel.x * vel.x + vel.y * vel.y).sqrt();
                draw_boid(&mut buf, pos.x, pos.y, vel.x, vel.y, speed_color(speed));
            });
        }

        fps_count += 1;
        if fps_timer.elapsed().as_secs_f64() >= 0.5 {
            display_fps = fps_count as f64 / fps_timer.elapsed().as_secs_f64();
            fps_count = 0;
            fps_timer = std::time::Instant::now();
        }

        window.set_title(&format!(
            "SkyEngine Boids | {} boids | {:.0} FPS | Mouse=predator  Click=attractor  Space=scatter",
            NUM_BOIDS, display_fps));
        window.update_with_buffer(&buf, W, H).unwrap();
    }
}
