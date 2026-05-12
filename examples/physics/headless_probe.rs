//! Headless physics probe for the `physics_arcade_demo`-scale workload.
//!
//! ```bash
//! cargo run --example physics_headless_probe --features physics --release
//! cargo run --example physics_headless_probe --features physics --release -- --scenario catchup
//! cargo run --example physics_headless_probe --features physics --release -- --scenario arcade --csv
//! ```

use std::env;
use std::process;
use std::time::Instant;

use sky_engine::ecs::{EntityId, World};
use sky_engine::math::{Transform, Vec2};
use sky_engine::physics::{
    Collider2D, PhysicsConfig2D, PhysicsEvent2D, PhysicsEvents, PhysicsPlugin, PhysicsWorld2D,
    RigidBody2D, Velocity2D,
};
use sky_engine::plugin::Plugin;

const ARENA_W: f32 = 820.0;
const ARENA_H: f32 = 560.0;
const DEFAULT_TOYS: usize = 180;
const DEFAULT_FRAMES: usize = 3000;
const DEFAULT_WARMUP: usize = 300;
const FIXED_DT: f32 = 1.0 / 90.0;
const FRAME_DT: f32 = 1.0 / 60.0;
const CATCHUP_DT: f32 = 0.1;
const CATCHUP_PERIOD: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Steady,
    Arcade,
    Catchup,
}

impl Scenario {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "steady" => Some(Self::Steady),
            "arcade" => Some(Self::Arcade),
            "catchup" => Some(Self::Catchup),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Steady => "steady",
            Self::Arcade => "arcade",
            Self::Catchup => "catchup",
        }
    }

    fn dt_for_frame(self, frame: usize) -> f32 {
        if self == Self::Catchup && frame > 0 && frame % CATCHUP_PERIOD == 0 {
            CATCHUP_DT
        } else {
            FRAME_DT
        }
    }
}

struct Config {
    scenario: Scenario,
    frames: usize,
    warmup: usize,
    toys: usize,
    csv: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scenario: Scenario::Steady,
            frames: DEFAULT_FRAMES,
            warmup: DEFAULT_WARMUP,
            toys: DEFAULT_TOYS,
            csv: false,
        }
    }
}

impl Config {
    fn from_args() -> Result<Option<Self>, String> {
        let mut config = Self::default();
        let mut args = env::args().skip(1).peekable();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "--csv" => config.csv = true,
                "--scenario" => {
                    let value = next_value(&mut args, "--scenario")?;
                    config.scenario = Scenario::parse(&value).ok_or_else(|| {
                        format!("unknown scenario `{value}`; expected steady, arcade, or catchup")
                    })?;
                }
                "--frames" => {
                    config.frames = parse_usize(&next_value(&mut args, "--frames")?, "--frames")?;
                }
                "--warmup" => {
                    config.warmup = parse_usize(&next_value(&mut args, "--warmup")?, "--warmup")?;
                }
                "--toys" => {
                    config.toys = parse_usize(&next_value(&mut args, "--toys")?, "--toys")?;
                }
                _ if arg.starts_with("--scenario=") => {
                    let value = arg.trim_start_matches("--scenario=");
                    config.scenario = Scenario::parse(value).ok_or_else(|| {
                        format!("unknown scenario `{value}`; expected steady, arcade, or catchup")
                    })?;
                }
                _ if arg.starts_with("--frames=") => {
                    config.frames = parse_usize(arg.trim_start_matches("--frames="), "--frames")?;
                }
                _ if arg.starts_with("--warmup=") => {
                    config.warmup = parse_usize(arg.trim_start_matches("--warmup="), "--warmup")?;
                }
                _ if arg.starts_with("--toys=") => {
                    config.toys = parse_usize(arg.trim_start_matches("--toys="), "--toys")?;
                }
                _ => return Err(format!("unknown argument `{arg}`")),
            }
        }

        if config.frames == 0 {
            return Err("--frames must be greater than zero".to_string());
        }

        Ok(Some(config))
    }
}

fn next_value(
    args: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    flag: &str,
) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{flag} expects a non-negative integer, got `{value}`"))
}

#[derive(Clone, Copy)]
struct PhysicsToy;

#[derive(Clone, Copy)]
struct PlayerPaddle;

#[derive(Clone, Copy)]
struct MixerArm {
    speed: f32,
}

struct ProbeWorld {
    world: World,
    spawner: ToySpawner,
}

impl ProbeWorld {
    fn new(config: &Config) -> Self {
        let mut world = World::new();
        PhysicsPlugin::new(PhysicsConfig2D {
            gravity: Vec2::new(0.0, -900.0),
            fixed_dt: FIXED_DT,
            pixels_per_meter: 64.0,
        })
        .install(&mut world)
        .unwrap();
        spawn_scene(&mut world);

        let mut spawner = ToySpawner::new(0x5EED_2026);
        spawner.spawn_to_target(&mut world, config.toys, config.scenario == Scenario::Arcade);

        Self { world, spawner }
    }

    fn post_frame(&mut self, scenario: Scenario, dt: f32, target_toys: usize) -> usize {
        match scenario {
            Scenario::Steady | Scenario::Catchup => drain_events(&mut self.world),
            Scenario::Arcade => {
                self.spawner.clear_initial_velocities(&mut self.world);
                let events = drain_events(&mut self.world);
                self.spawner.prune(&mut self.world, target_toys);
                self.spawner
                    .spawn_to_target(&mut self.world, target_toys, true);
                animate_mixers(&mut self.world, dt);
                events
            }
        }
    }
}

struct ToySpawner {
    rng: SimpleRng,
    toys: Vec<EntityId>,
    pending_velocity_clear: Vec<EntityId>,
}

impl ToySpawner {
    fn new(seed: u64) -> Self {
        Self {
            rng: SimpleRng::new(seed),
            toys: Vec::new(),
            pending_velocity_clear: Vec::new(),
        }
    }

    fn spawn_to_target(&mut self, world: &mut World, target: usize, with_velocity: bool) {
        while self.toys.len() < target {
            let slot = self.toys.len();
            let center = if slot % 2 == 0 {
                [-170.0, 205.0]
            } else {
                [140.0, 240.0]
            };
            let entity = self.spawn_toy(world, center, slot, with_velocity);
            self.toys.push(entity);
            if with_velocity {
                self.pending_velocity_clear.push(entity);
            }
        }
    }

    fn spawn_toy(
        &mut self,
        world: &mut World,
        center: [f32; 2],
        slot: usize,
        with_velocity: bool,
    ) -> EntityId {
        let x = center[0] + self.rng.range(-74.0, 74.0);
        let y = center[1] + self.rng.range(-12.0, 68.0) + (slot % 24) as f32 * 1.5;
        let vx = self.rng.range(-210.0, 210.0);
        let vy = self.rng.range(90.0, 360.0);

        if self.rng.chance(0.46) {
            let radius = self.rng.range(13.0, 25.0);
            if with_velocity {
                world.spawn((
                    Transform::from_xy(x, y),
                    RigidBody2D::dynamic(),
                    Collider2D::circle(radius).friction(0.52).restitution(0.7),
                    Velocity2D::new(vx, vy).with_angular(self.rng.range(-5.0, 5.0)),
                    PhysicsToy,
                ))
            } else {
                world.spawn((
                    Transform::from_xy(x, y),
                    RigidBody2D::dynamic(),
                    Collider2D::circle(radius).friction(0.52).restitution(0.7),
                    PhysicsToy,
                ))
            }
        } else {
            let size = [self.rng.range(18.0, 42.0), self.rng.range(16.0, 36.0)];
            let transform = Transform::from_xy(x, y).with_rotation(self.rng.range(-0.7, 0.7));
            let collider = Collider2D::rectangle(size[0], size[1])
                .friction(0.72)
                .restitution(0.28);
            if with_velocity {
                world.spawn((
                    transform,
                    RigidBody2D::dynamic(),
                    collider,
                    Velocity2D::new(vx, vy).with_angular(self.rng.range(-6.0, 6.0)),
                    PhysicsToy,
                ))
            } else {
                world.spawn((transform, RigidBody2D::dynamic(), collider, PhysicsToy))
            }
        }
    }

    fn clear_initial_velocities(&mut self, world: &mut World) {
        for entity in self.pending_velocity_clear.drain(..) {
            if world.contains(entity) {
                let _ = world.remove::<Velocity2D>(entity);
            }
        }
    }

    fn prune(&mut self, world: &mut World, target: usize) {
        let mut remove = Vec::new();
        for &entity in &self.toys {
            let should_remove = if let Some(transform) = world.get::<Transform>(entity) {
                transform.position[1] < -520.0
                    || transform.position[1] > 520.0
                    || transform.position[0].abs() > 720.0
            } else {
                true
            };
            if should_remove {
                remove.push(entity);
            }
        }

        for entity in remove {
            let _ = world.despawn(entity);
        }

        while self.toys.len() > target {
            let entity = self.toys.remove(0);
            let _ = world.despawn(entity);
        }

        self.toys.retain(|entity| world.contains(*entity));
        self.pending_velocity_clear
            .retain(|entity| world.contains(*entity));
    }

    fn count(&self) -> usize {
        self.toys.len()
    }
}

#[derive(Clone, Copy, Default)]
struct PhysicsCounts {
    bodies: usize,
    colliders: usize,
}

#[derive(Clone, Copy)]
struct FrameSample {
    frame: usize,
    dt: f32,
    expected_substeps: usize,
    tick_ms: f64,
    post_ms: f64,
    bodies: usize,
    colliders: usize,
    events: usize,
    toys: usize,
}

impl FrameSample {
    fn total_ms(self) -> f64 {
        self.tick_ms + self.post_ms
    }

    fn tick_ms_per_substep(self) -> Option<f64> {
        (self.expected_substeps > 0).then(|| self.tick_ms / self.expected_substeps as f64)
    }
}

struct FixedStepCounter {
    accumulator: f32,
    fixed_dt: f32,
}

impl FixedStepCounter {
    fn new(fixed_dt: f32) -> Self {
        Self {
            accumulator: 0.0,
            fixed_dt,
        }
    }

    fn count_for_delta(&mut self, dt: f32) -> usize {
        self.accumulator += dt.max(0.0);
        let mut count = 0;
        while self.accumulator >= self.fixed_dt {
            count += 1;
            self.accumulator -= self.fixed_dt;
        }
        count
    }
}

#[derive(Clone, Copy)]
struct Summary {
    mean: f64,
    p50: f64,
    p90: f64,
    p95: f64,
    p99: f64,
    max: f64,
}

impl Summary {
    fn from_values(values: &[f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }

        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;

        Some(Self {
            mean,
            p50: percentile(&sorted, 0.50),
            p90: percentile(&sorted, 0.90),
            p95: percentile(&sorted, 0.95),
            p99: percentile(&sorted, 0.99),
            max: *sorted.last().unwrap(),
        })
    }
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[index]
}

fn main() {
    let config = match Config::from_args() {
        Ok(Some(config)) => config,
        Ok(None) => {
            print_help();
            return;
        }
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("try `--help` for usage");
            process::exit(2);
        }
    };

    let mut probe = ProbeWorld::new(&config);
    let mut step_counter = FixedStepCounter::new(FIXED_DT);
    let mut samples = Vec::with_capacity(config.frames);
    let total_frames = config.warmup + config.frames;

    if config.csv {
        println!(
            "frame,dt,expected_substeps,tick_ms,post_ms,total_ms,tick_ms_per_substep,bodies,colliders,events,toys"
        );
    }

    for run_frame in 0..total_frames {
        let dt = config.scenario.dt_for_frame(run_frame);
        let expected_substeps = step_counter.count_for_delta(dt);

        let tick_start = Instant::now();
        probe.world.tick_with_delta(dt);
        let tick_ms = elapsed_ms(tick_start);

        let counts = physics_counts(&probe.world);

        let post_start = Instant::now();
        let events = probe.post_frame(config.scenario, dt, config.toys);
        let post_ms = elapsed_ms(post_start);

        if run_frame < config.warmup {
            continue;
        }

        let sample = FrameSample {
            frame: run_frame - config.warmup,
            dt,
            expected_substeps,
            tick_ms,
            post_ms,
            bodies: counts.bodies,
            colliders: counts.colliders,
            events,
            toys: probe.spawner.count(),
        };

        if config.csv {
            print_csv_sample(sample);
        }
        samples.push(sample);
    }

    if config.csv {
        eprintln!();
        print_summary(&config, &samples, true);
    } else {
        print_summary(&config, &samples, false);
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn print_help() {
    println!(
        "\
Headless Physics Probe

USAGE:
    cargo run --example physics_headless_probe --features physics --release -- [OPTIONS]

OPTIONS:
    --scenario steady|arcade|catchup   Workload to run [default: steady]
    --frames N                         Measured frames after warmup [default: 3000]
    --warmup N                         Warmup frames excluded from stats [default: 300]
    --toys N                           Dynamic toy count target [default: 180]
    --csv                              Print per-frame CSV rows
    -h, --help                         Print this help
"
    );
}

fn print_csv_sample(sample: FrameSample) {
    let per_substep = sample.tick_ms_per_substep().unwrap_or(0.0);
    println!(
        "{},{:.6},{},{:.6},{:.6},{:.6},{:.6},{},{},{},{}",
        sample.frame,
        sample.dt,
        sample.expected_substeps,
        sample.tick_ms,
        sample.post_ms,
        sample.total_ms(),
        per_substep,
        sample.bodies,
        sample.colliders,
        sample.events,
        sample.toys
    );
}

fn print_summary(config: &Config, samples: &[FrameSample], stderr: bool) {
    let tick_values = samples
        .iter()
        .map(|sample| sample.tick_ms)
        .collect::<Vec<_>>();
    let post_values = samples
        .iter()
        .map(|sample| sample.post_ms)
        .collect::<Vec<_>>();
    let total_values = samples
        .iter()
        .map(|sample| sample.total_ms())
        .collect::<Vec<_>>();
    let substep_values = samples
        .iter()
        .filter_map(|sample| sample.tick_ms_per_substep())
        .collect::<Vec<_>>();

    let total_substeps = samples
        .iter()
        .map(|sample| sample.expected_substeps)
        .sum::<usize>();
    let max_substeps = samples
        .iter()
        .map(|sample| sample.expected_substeps)
        .max()
        .unwrap_or(0);
    let catchup_frames = samples
        .iter()
        .filter(|sample| sample.expected_substeps > 2)
        .count();
    let total_events = samples.iter().map(|sample| sample.events).sum::<usize>();
    let max_events = samples
        .iter()
        .map(|sample| sample.events)
        .max()
        .unwrap_or(0);
    let last = samples.last().copied();

    let mut out = String::new();
    out.push_str("Headless Physics Probe\n");
    out.push_str(&format!("scenario: {}\n", config.scenario.name()));
    out.push_str(&format!(
        "measured_frames: {} warmup_frames: {} target_toys: {}\n",
        config.frames, config.warmup, config.toys
    ));
    out.push_str(&format!(
        "fixed_dt: {:.6}s nominal_frame_dt: {:.6}s\n",
        FIXED_DT, FRAME_DT
    ));
    out.push_str(&format!(
        "substeps: total {} max/frame {} catchup_like_frames {}\n",
        total_substeps, max_substeps, catchup_frames
    ));
    if let Some(last) = last {
        out.push_str(&format!(
            "last_load: bodies {} colliders {} toys {}\n",
            last.bodies, last.colliders, last.toys
        ));
    }
    out.push_str(&format!(
        "events: total {} max/frame {}\n",
        total_events, max_events
    ));

    append_summary(&mut out, "tick_ms", Summary::from_values(&tick_values));
    append_summary(&mut out, "post_ms", Summary::from_values(&post_values));
    append_summary(&mut out, "total_ms", Summary::from_values(&total_values));
    append_summary(
        &mut out,
        "tick_ms_per_substep",
        Summary::from_values(&substep_values),
    );

    if stderr {
        eprint!("{out}");
    } else {
        print!("{out}");
    }
}

fn append_summary(output: &mut String, label: &str, summary: Option<Summary>) {
    if let Some(summary) = summary {
        output.push_str(&format!(
            "{label}: mean {:.3} p50 {:.3} p90 {:.3} p95 {:.3} p99 {:.3} max {:.3}\n",
            summary.mean, summary.p50, summary.p90, summary.p95, summary.p99, summary.max
        ));
    } else {
        output.push_str(&format!("{label}: n/a\n"));
    }
}

fn physics_counts(world: &World) -> PhysicsCounts {
    world
        .get_resource::<PhysicsWorld2D>()
        .map(|physics| PhysicsCounts {
            bodies: physics.body_count(),
            colliders: physics.collider_count(),
        })
        .unwrap_or_default()
}

fn drain_events(world: &mut World) -> usize {
    let Some(events) = world.get_resource_mut::<PhysicsEvents>() else {
        return 0;
    };

    events
        .drain()
        .map(|event| match event {
            PhysicsEvent2D::ContactStarted { .. }
            | PhysicsEvent2D::ContactStopped { .. }
            | PhysicsEvent2D::TriggerEntered { .. }
            | PhysicsEvent2D::TriggerExited { .. } => 1usize,
        })
        .sum()
}

fn spawn_scene(world: &mut World) {
    spawn_static_bar(world, 0.0, -ARENA_H * 0.5, ARENA_W, 32.0, 0.0);
    spawn_static_bar(world, -ARENA_W * 0.5, 0.0, 32.0, ARENA_H, 0.0);
    spawn_static_bar(world, ARENA_W * 0.5, 0.0, 32.0, ARENA_H, 0.0);
    spawn_static_bar(world, -165.0, -105.0, 210.0, 18.0, 0.36);
    spawn_static_bar(world, 170.0, 18.0, 235.0, 18.0, -0.42);
    spawn_static_bar(world, -255.0, 118.0, 160.0, 16.0, -0.24);
    spawn_sensor(world);
    spawn_mixer(world);
    spawn_paddle(world);
}

fn spawn_static_bar(
    world: &mut World,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation: f32,
) -> EntityId {
    world.spawn((
        Transform::from_xy(x, y).with_rotation(rotation),
        RigidBody2D::static_body(),
        Collider2D::rectangle(width, height)
            .friction(0.82)
            .restitution(0.35),
    ))
}

fn spawn_sensor(world: &mut World) -> EntityId {
    world.spawn((
        Transform::from_xy(285.0, 116.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(150.0, 170.0).sensor(true),
    ))
}

fn spawn_mixer(world: &mut World) -> EntityId {
    world.spawn((
        Transform::from_xy(0.0, 54.0),
        RigidBody2D::kinematic().lock_rotation(),
        Collider2D::rectangle(230.0, 14.0)
            .friction(0.2)
            .restitution(0.45),
        MixerArm { speed: 1.8 },
    ))
}

fn spawn_paddle(world: &mut World) -> EntityId {
    world.spawn((
        Transform::from_xy(0.0, -210.0),
        RigidBody2D::dynamic().lock_rotation(),
        Collider2D::rectangle(92.0, 24.0)
            .friction(0.0)
            .restitution(0.35),
        Velocity2D::default(),
        PlayerPaddle,
    ))
}

fn animate_mixers(world: &mut World, dt: f32) {
    let mut mixers = world.query::<(&mut Transform, &MixerArm)>();
    mixers.for_each(world, |(transform, mixer)| {
        transform.rotate_z(mixer.speed * dt);
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

    fn chance(&mut self, probability: f32) -> bool {
        self.next_f32() < probability
    }
}
