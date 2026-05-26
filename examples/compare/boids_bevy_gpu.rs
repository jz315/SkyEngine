//! # Boids — Bevy Native GPU Version
//!
//! Same 2000-boid flocking simulation, rendered through Bevy's native 2D
//! pipeline with HDR camera, bloom, and tonemapping.
//!
//! ```sh
//! cargo run --example boids_bevy_gpu --features compare-bevy --release
//! ```

use std::f32::consts::TAU;

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use bevy_full as bevy;

const WINDOW_TITLE: &str = "Bevy GPU — Boids";
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;
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

const CELL_SIZE: f32 = VISUAL_RANGE;
const GRID_COLS: usize = (WIDTH / CELL_SIZE) as usize + 1;
const GRID_ROWS: usize = (HEIGHT / CELL_SIZE) as usize + 1;
const GRID_CELLS: usize = GRID_COLS * GRID_ROWS;

#[derive(Component, Clone, Copy, Default)]
struct Pos {
    x: f32,
    y: f32,
}

impl Pos {
    #[inline]
    fn vec2(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    #[inline]
    fn set(&mut self, value: Vec2) {
        self.x = value.x;
        self.y = value.y;
    }
}

#[derive(Component, Clone, Copy, Default)]
struct Vel {
    x: f32,
    y: f32,
}

impl Vel {
    #[inline]
    fn vec2(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    #[inline]
    fn set(&mut self, value: Vec2) {
        self.x = value.x;
        self.y = value.y;
    }
}

#[derive(Component)]
struct Boid;

#[derive(Component)]
struct BoidAttractor {
    life: f32,
}

#[derive(Clone, Copy)]
struct SimInput {
    mouse_sim: Vec2,
    mouse_valid: bool,
    panic: bool,
}

#[derive(Resource, Default)]
struct InputState {
    mouse_sim: Vec2,
    mouse_valid: bool,
    click: bool,
    panic: bool,
}

impl InputState {
    #[inline]
    fn sim_input(&self) -> SimInput {
        SimInput {
            mouse_sim: self.mouse_sim,
            mouse_valid: self.mouse_valid,
            panic: self.panic,
        }
    }
}

#[derive(Resource, Default)]
struct AttractorCache {
    items: Vec<(Vec2, f32)>,
}

#[derive(Resource)]
struct BoidSnapshot {
    positions: Vec<Vec2>,
    velocities: Vec<Vec2>,
    steering: Vec<Vec2>,
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
    #[inline]
    fn cell_index(pos: Vec2) -> usize {
        let col = ((pos.x / CELL_SIZE) as usize).min(GRID_COLS - 1);
        let row = ((pos.y / CELL_SIZE) as usize).min(GRID_ROWS - 1);
        row * GRID_COLS + col
    }

    fn rebuild_grid(&mut self) {
        self.cell_heads.fill(-1);
        self.next.clear();
        self.next.resize(self.positions.len(), -1);
        self.steering.clear();
        self.steering.resize(self.positions.len(), Vec2::ZERO);

        for (index, pos) in self.positions.iter().enumerate().rev() {
            let cell = Self::cell_index(*pos);
            self.next[index] = self.cell_heads[cell];
            self.cell_heads[cell] = index as i32;
        }
    }
}

#[derive(Resource, Default)]
struct FpsCounter {
    smooth: f32,
    frames: u32,
}

#[derive(Default)]
struct Neighborhood {
    count: f32,
    average_velocity: Vec2,
    center: Vec2,
    separation: Vec2,
}

#[inline(always)]
fn limit_vector(vector: Vec2, max_len: f32) -> Vec2 {
    let len_sq = vector.length_squared();
    if len_sq > max_len * max_len {
        vector * (max_len / len_sq.sqrt())
    } else {
        vector
    }
}

#[inline(always)]
fn steer_towards(current: Vec2, desired: Vec2, max_force: f32) -> Vec2 {
    limit_vector(desired - current, max_force)
}

fn speed_color(speed: f32) -> Color {
    let t = ((speed - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)).clamp(0.0, 1.0);
    let hue = 180.0 * (1.0 - t);
    Color::hsl(hue, 0.9, 0.5)
}

#[inline]
fn sim_to_render(pos: Vec2) -> Vec2 {
    Vec2::new(pos.x - WIDTH * 0.5, HEIGHT * 0.5 - pos.y)
}

fn gather_neighborhood(snapshot: &BoidSnapshot, index: usize) -> Neighborhood {
    let pos = snapshot.positions[index];
    let visual_range_sq = VISUAL_RANGE * VISUAL_RANGE;
    let separation_range_sq = SEPARATION_RANGE * SEPARATION_RANGE;

    let mut neighborhood = Neighborhood::default();
    let col = (pos.x / CELL_SIZE) as i32;
    let row = (pos.y / CELL_SIZE) as i32;

    for row_offset in -1..=1 {
        for col_offset in -1..=1 {
            let next_row = row + row_offset;
            let next_col = col + col_offset;
            if next_row < 0
                || next_row >= GRID_ROWS as i32
                || next_col < 0
                || next_col >= GRID_COLS as i32
            {
                continue;
            }

            let cell = next_row as usize * GRID_COLS + next_col as usize;
            let mut head = snapshot.cell_heads[cell];
            while head >= 0 {
                let other = head as usize;
                head = snapshot.next[other];

                if other == index {
                    continue;
                }

                let delta = snapshot.positions[other] - pos;
                let dist_sq = delta.length_squared();
                if dist_sq <= 1.0e-4 || dist_sq > visual_range_sq {
                    continue;
                }

                neighborhood.count += 1.0;
                neighborhood.average_velocity += snapshot.velocities[other];
                neighborhood.center += snapshot.positions[other];

                if dist_sq < separation_range_sq {
                    let dist = dist_sq.sqrt();
                    let falloff = 1.0 - dist / SEPARATION_RANGE;
                    neighborhood.separation -= delta / dist * falloff;
                }
            }
        }
    }

    neighborhood
}

fn apply_neighbor_forces(
    velocity: Vec2,
    position: Vec2,
    neighborhood: &Neighborhood,
    panic_mode: bool,
) -> Vec2 {
    if neighborhood.count <= 0.0 {
        return Vec2::ZERO;
    }

    let inv_count = neighborhood.count.recip();
    let preferred_speed = if panic_mode {
        MAX_SPEED
    } else {
        CRUISE_SPEED.max(velocity.length())
    };

    let align_dir = (neighborhood.average_velocity * inv_count).normalize_or_zero();
    let cohesion_dir = (neighborhood.center * inv_count - position).normalize_or_zero();
    let separation_dir = neighborhood.separation.normalize_or_zero();

    steer_towards(velocity, align_dir * preferred_speed, MAX_STEERING) * ALIGNMENT_WEIGHT
        + steer_towards(velocity, cohesion_dir * CRUISE_SPEED, MAX_STEERING) * COHESION_WEIGHT
        + separation_dir * SEPARATION_WEIGHT
}

fn apply_predator_force(position: Vec2, input: SimInput) -> Vec2 {
    if !input.mouse_valid {
        return Vec2::ZERO;
    }

    let away = position - input.mouse_sim;
    let dist_sq = away.length_squared();
    if dist_sq <= 1.0e-4 || dist_sq >= PREDATOR_RANGE * PREDATOR_RANGE {
        return Vec2::ZERO;
    }

    let strength = 1.0 - dist_sq.sqrt() / PREDATOR_RANGE;
    away.normalize_or_zero() * PREDATOR_WEIGHT * strength
}

fn apply_attractor_forces(position: Vec2, attractors: &[(Vec2, f32)]) -> Vec2 {
    let mut force = Vec2::ZERO;

    for &(attractor_pos, attractor_life) in attractors {
        let toward = attractor_pos - position;
        let dist_sq = toward.length_squared();
        if dist_sq <= 1.0e-4 || dist_sq >= ATTRACTOR_RANGE * ATTRACTOR_RANGE {
            continue;
        }

        let strength =
            (1.0 - dist_sq.sqrt() / ATTRACTOR_RANGE) * (attractor_life / ATTRACTOR_LIFETIME);
        force += toward.normalize_or_zero() * ATTRACTOR_WEIGHT * strength;
    }

    force
}

fn apply_panic_force(index: usize, panic_mode: bool) -> Vec2 {
    if !panic_mode {
        return Vec2::ZERO;
    }

    let angle = (index as f32 * 2.399_963_1) % TAU;
    Vec2::new(angle.cos(), angle.sin()) * PANIC_WEIGHT
}

fn apply_wall_force(position: Vec2) -> Vec2 {
    let mut force = Vec2::ZERO;

    if position.x < WALL_MARGIN {
        force.x += (1.0 - position.x / WALL_MARGIN) * WALL_WEIGHT;
    } else if position.x > WIDTH - WALL_MARGIN {
        force.x -= (1.0 - (WIDTH - position.x) / WALL_MARGIN) * WALL_WEIGHT;
    }

    if position.y < WALL_MARGIN {
        force.y += (1.0 - position.y / WALL_MARGIN) * WALL_WEIGHT;
    } else if position.y > HEIGHT - WALL_MARGIN {
        force.y -= (1.0 - (HEIGHT - position.y) / WALL_MARGIN) * WALL_WEIGHT;
    }

    force
}

fn compute_steering(
    snapshot: &BoidSnapshot,
    index: usize,
    input: SimInput,
    attractors: &[(Vec2, f32)],
) -> Vec2 {
    let position = snapshot.positions[index];
    let velocity = snapshot.velocities[index];
    let neighborhood = gather_neighborhood(snapshot, index);

    let accel = apply_neighbor_forces(velocity, position, &neighborhood, input.panic)
        + apply_predator_force(position, input)
        + apply_attractor_forces(position, attractors)
        + apply_panic_force(index, input.panic)
        + apply_wall_force(position);

    limit_vector(accel, MAX_STEERING)
}

fn integrate_boid(
    position: &mut Pos,
    velocity: &mut Vel,
    steering: Vec2,
    dt: f32,
    panic_mode: bool,
) {
    let drag = (1.0 - DRAG * dt).max(0.0);
    let max_speed = if panic_mode {
        MAX_SPEED * 1.35
    } else {
        MAX_SPEED
    };

    let mut next_velocity = velocity.vec2() + steering * dt;
    next_velocity *= drag;

    let speed = next_velocity.length();
    if speed > max_speed {
        next_velocity = next_velocity / speed * max_speed;
    } else if speed < MIN_SPEED && speed > 1.0e-4 {
        next_velocity = next_velocity / speed * MIN_SPEED;
    }

    let mut next_position = position.vec2() + next_velocity * dt;
    next_position.x = next_position.x.clamp(0.0, WIDTH - 1.0);
    next_position.y = next_position.y.clamp(0.0, HEIGHT - 1.0);

    if (next_position.x <= 0.0 && next_velocity.x < 0.0)
        || (next_position.x >= WIDTH - 1.0 && next_velocity.x > 0.0)
    {
        next_velocity.x *= -0.25;
    }
    if (next_position.y <= 0.0 && next_velocity.y < 0.0)
        || (next_position.y >= HEIGHT - 1.0 && next_velocity.y > 0.0)
    {
        next_velocity.y *= -0.25;
    }

    position.set(next_position);
    velocity.set(next_velocity);
}

fn cursor_to_sim(
    window: &Window,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<Vec2> {
    let cursor_pos = window.cursor_position()?;
    let world_pos = camera
        .viewport_to_world_2d(camera_transform, cursor_pos)
        .ok()?;
    Some(Vec2::new(
        world_pos.x + WIDTH * 0.5,
        HEIGHT * 0.5 - world_pos.y,
    ))
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.02, 0.03, 0.06)),
            ..default()
        },
        Tonemapping::TonyMcMapface,
        Bloom::default(),
    ));
}

fn setup_boids(mut commands: Commands) {
    let mut rng = SimpleRng::new(42);

    for _ in 0..NUM_BOIDS {
        let angle = rng.range(0.0, TAU);
        let speed = rng.range(MIN_SPEED, MAX_SPEED);
        let position = Vec2::new(rng.range(0.0, WIDTH), rng.range(0.0, HEIGHT));
        let render_pos = sim_to_render(position);

        commands.spawn((
            Pos {
                x: position.x,
                y: position.y,
            },
            Vel {
                x: angle.cos() * speed,
                y: angle.sin() * speed,
            },
            Boid,
            Sprite {
                color: Color::WHITE,
                custom_size: Some(Vec2::new(BOID_SIZE * 2.2, BOID_SIZE * 1.4)),
                ..default()
            },
            Transform::from_xyz(render_pos.x, render_pos.y, 0.0),
        ));
    }

    eprintln!(
        "[boids_bevy_gpu] {} boids | Mouse=predator  Click=attractor  Space=scatter  Escape=quit",
        NUM_BOIDS
    );
}

fn input_system(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut input: ResMut<InputState>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };

    if let Some(mouse_sim) = cursor_to_sim(window, camera, camera_transform) {
        input.mouse_sim = mouse_sim;
        input.mouse_valid = true;
    } else {
        input.mouse_valid = false;
    }

    input.click = mouse.just_pressed(MouseButton::Left);
    input.panic = keys.pressed(KeyCode::Space);
}

fn attractor_spawn_system(mut commands: Commands, input: Res<InputState>) {
    if !input.click || !input.mouse_valid {
        return;
    }

    commands.spawn((
        Pos {
            x: input.mouse_sim.x,
            y: input.mouse_sim.y,
        },
        BoidAttractor {
            life: ATTRACTOR_LIFETIME,
        },
    ));
}

fn attractor_decay_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &Pos, &mut BoidAttractor)>,
    mut cache: ResMut<AttractorCache>,
) {
    let dt = time.delta_secs();
    cache.items.clear();

    for (entity, position, mut attractor) in &mut query {
        attractor.life -= dt;
        if attractor.life <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            cache.items.push((position.vec2(), attractor.life));
        }
    }
}

fn snapshot_system(query: Query<(&Pos, &Vel), With<Boid>>, mut snapshot: ResMut<BoidSnapshot>) {
    snapshot.positions.clear();
    snapshot.velocities.clear();

    for (position, velocity) in &query {
        snapshot.positions.push(position.vec2());
        snapshot.velocities.push(velocity.vec2());
    }

    snapshot.rebuild_grid();
}

fn boid_step_system(
    time: Res<Time>,
    input: Res<InputState>,
    attractors: Res<AttractorCache>,
    mut snapshot: ResMut<BoidSnapshot>,
    mut query: Query<(&mut Pos, &mut Vel), With<Boid>>,
) {
    let dt = time.delta_secs().min(0.05);
    let sim_input = input.sim_input();

    for index in 0..snapshot.positions.len() {
        snapshot.steering[index] = compute_steering(&snapshot, index, sim_input, &attractors.items);
    }

    // This relies on the snapshot query and the mutable boid query seeing boids in the same order.
    let mut index = 0usize;
    for (mut position, mut velocity) in &mut query {
        if index >= snapshot.steering.len() {
            break;
        }

        integrate_boid(
            &mut position,
            &mut velocity,
            snapshot.steering[index],
            dt,
            sim_input.panic,
        );
        index += 1;
    }
}

fn render_sync_system(mut query: Query<(&Pos, &Vel, &mut Transform, &mut Sprite), With<Boid>>) {
    for (position, velocity, mut transform, mut sprite) in &mut query {
        let render_pos = sim_to_render(position.vec2());
        transform.translation.x = render_pos.x;
        transform.translation.y = render_pos.y;
        transform.rotation = Quat::from_rotation_z((-velocity.y).atan2(velocity.x));

        let speed = velocity.vec2().length();
        let glow = (speed / MAX_SPEED).clamp(0.3, 1.0);
        let base = speed_color(speed).to_srgba();
        let multiplier = 1.2 + glow;
        sprite.color = Color::srgb(
            base.red * multiplier,
            base.green * multiplier,
            base.blue * multiplier,
        );
    }
}

fn fps_system(
    time: Res<Time>,
    mut fps: ResMut<FpsCounter>,
    mut windows: Query<&mut Window>,
    boids: Query<&Boid>,
) {
    let dt = time.delta_secs();
    let instant = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    fps.smooth = if fps.smooth == 0.0 {
        instant
    } else {
        fps.smooth * 0.95 + instant * 0.05
    };
    fps.frames += 1;

    if fps.frames % 30 != 0 {
        return;
    }

    if let Ok(mut window) = windows.single_mut() {
        window.title = format!(
            "{WINDOW_TITLE} | {:.0} FPS | {} boids",
            fps.smooth,
            boids.iter().count()
        );
    }
}

fn exit_system(keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::Escape) {
        std::process::exit(0);
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: WINDOW_TITLE.into(),
                resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(InputState::default())
        .insert_resource(BoidSnapshot::default())
        .insert_resource(AttractorCache::default())
        .insert_resource(FpsCounter::default())
        .add_systems(Startup, (setup_camera, setup_boids))
        .add_systems(
            Update,
            (
                exit_system,
                input_system,
                attractor_spawn_system,
                attractor_decay_system,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                snapshot_system,
                boid_step_system,
                render_sync_system,
                fps_system,
            )
                .chain()
                .after(attractor_decay_system),
        )
        .run();
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
