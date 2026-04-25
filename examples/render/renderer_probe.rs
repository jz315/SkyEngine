//! RenderComposer performance probe for the default universal scene pipeline.
//!
//! Runs controlled headless scenarios and reports synchronized frame timings
//! plus lightweight internal phase timings.
//!
//! ```bash
//! cargo run --example renderer_probe --features app --release
//! cargo run --example renderer_probe --features app --release -- --csv
//! cargo run --example renderer_probe --features app --release -- --frames 180 --warmup 60
//! ```

use std::time::Instant;

use sky_engine::ecs::{EntityId, World};
use sky_engine::gpu::GpuContext;
use sky_engine::render::{
    BloomSettings, CameraMarker, CameraViewport, Color, MainCamera, PointLight, Projection,
    RenderComposer, RenderPipelineAsset, RenderSettings, RenderStats, SpriteFeature,
    SpriteRenderer, ToneMapSettings, Transform, TransparentPhase, ViewportRect, VignetteSettings,
};

const DEFAULT_SURFACE_SIZE: [u32; 2] = [1280, 720];

struct ProbeConfig {
    warmup_frames: usize,
    sample_frames: usize,
    csv: bool,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            warmup_frames: 30,
            sample_frames: 90,
            csv: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Scenario {
    name: &'static str,
    path: ProbeRenderPath,
    sprites: usize,
    lights: usize,
    views: usize,
    sprite_dirty_ratio: f32,
    light_dirty_ratio: f32,
    postfx: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbeRenderPath {
    Unlit,
    LitHdr,
}

struct ProbeScene {
    world: World,
    sprite_entities: Vec<EntityId>,
    light_entities: Vec<EntityId>,
}

#[derive(Default)]
struct StatsAccumulator {
    sample_count: usize,
    frame_ms_sync_sum: f64,
    frame_ms_sync_values: Vec<f64>,
    renderer_frame_ms_sum: f64,
    extract_ms_sum: f64,
    resize_ms_sum: f64,
    prepare_ms_sum: f64,
    upload_ms_sum: f64,
    execute_ms_sum: f64,
    dirty_sprite_slots_sum: usize,
    dirty_light_slots_sum: usize,
    visible_sprite_count_sum: usize,
    visible_light_count_sum: usize,
    draw_calls_sum: usize,
    passes_sum: usize,
}

struct ScenarioResult {
    scenario: Scenario,
    avg_frame_ms_sync: f64,
    p95_frame_ms_sync: f64,
    avg_renderer_frame_ms: f64,
    avg_extract_ms: f64,
    avg_resize_ms: f64,
    avg_prepare_ms: f64,
    avg_upload_ms: f64,
    avg_execute_ms: f64,
    avg_dirty_sprite_slots: f64,
    avg_dirty_light_slots: f64,
    avg_visible_sprite_count: f64,
    avg_visible_light_count: f64,
    avg_draw_calls: f64,
    avg_passes: f64,
}

impl StatsAccumulator {
    fn record(&mut self, frame_ms_sync: f64, stats: RenderStats) {
        self.sample_count += 1;
        self.frame_ms_sync_sum += frame_ms_sync;
        self.frame_ms_sync_values.push(frame_ms_sync);
        self.renderer_frame_ms_sum += stats.timings.frame_ms;
        self.extract_ms_sum += stats.timings.extract_ms;
        self.resize_ms_sum += stats.timings.resize_ms;
        self.prepare_ms_sum += stats.timings.prepare_ms;
        self.upload_ms_sum += stats.timings.upload_ms;
        self.execute_ms_sum += stats.timings.execute_ms;
        self.dirty_sprite_slots_sum += stats.dirty_sprite_slots;
        self.dirty_light_slots_sum += stats.dirty_light_slots;
        self.visible_sprite_count_sum += stats.sprite_count;
        self.visible_light_count_sum += stats.light_count;
        self.draw_calls_sum += stats.draw_calls;
        self.passes_sum += stats.passes;
    }

    fn finish(mut self, scenario: Scenario) -> ScenarioResult {
        debug_assert!(self.sample_count > 0);
        self.frame_ms_sync_values
            .sort_by(|lhs, rhs| lhs.total_cmp(rhs));
        let p95_index = ((self.frame_ms_sync_values.len() as f64) * 0.95).floor() as usize;
        let p95_index = p95_index.min(self.frame_ms_sync_values.len() - 1);
        let inv = 1.0 / self.sample_count as f64;
        ScenarioResult {
            scenario,
            avg_frame_ms_sync: self.frame_ms_sync_sum * inv,
            p95_frame_ms_sync: self.frame_ms_sync_values[p95_index],
            avg_renderer_frame_ms: self.renderer_frame_ms_sum * inv,
            avg_extract_ms: self.extract_ms_sum * inv,
            avg_resize_ms: self.resize_ms_sum * inv,
            avg_prepare_ms: self.prepare_ms_sum * inv,
            avg_upload_ms: self.upload_ms_sum * inv,
            avg_execute_ms: self.execute_ms_sum * inv,
            avg_dirty_sprite_slots: self.dirty_sprite_slots_sum as f64 * inv,
            avg_dirty_light_slots: self.dirty_light_slots_sum as f64 * inv,
            avg_visible_sprite_count: self.visible_sprite_count_sum as f64 * inv,
            avg_visible_light_count: self.visible_light_count_sum as f64 * inv,
            avg_draw_calls: self.draw_calls_sum as f64 * inv,
            avg_passes: self.passes_sum as f64 * inv,
        }
    }
}

fn main() {
    let config = parse_args(std::env::args().skip(1));
    let (device, queue) = create_probe_device();
    let mut ctx = GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        DEFAULT_SURFACE_SIZE,
    );

    let scenarios = [
        Scenario {
            name: "unlit_static_0",
            path: ProbeRenderPath::Unlit,
            sprites: 10_000,
            lights: 0,
            views: 1,
            sprite_dirty_ratio: 0.0,
            light_dirty_ratio: 0.0,
            postfx: false,
        },
        Scenario {
            name: "unlit_sparse_1",
            path: ProbeRenderPath::Unlit,
            sprites: 10_000,
            lights: 0,
            views: 1,
            sprite_dirty_ratio: 0.01,
            light_dirty_ratio: 0.0,
            postfx: false,
        },
        Scenario {
            name: "unlit_sparse_5",
            path: ProbeRenderPath::Unlit,
            sprites: 10_000,
            lights: 0,
            views: 1,
            sprite_dirty_ratio: 0.05,
            light_dirty_ratio: 0.0,
            postfx: false,
        },
        Scenario {
            name: "unlit_full_100",
            path: ProbeRenderPath::Unlit,
            sprites: 10_000,
            lights: 0,
            views: 1,
            sprite_dirty_ratio: 1.0,
            light_dirty_ratio: 0.0,
            postfx: false,
        },
        Scenario {
            name: "unlit_multiview_static",
            path: ProbeRenderPath::Unlit,
            sprites: 10_000,
            lights: 0,
            views: 2,
            sprite_dirty_ratio: 0.0,
            light_dirty_ratio: 0.0,
            postfx: false,
        },
        Scenario {
            name: "lit_static_postfx",
            path: ProbeRenderPath::LitHdr,
            sprites: 4_000,
            lights: 256,
            views: 1,
            sprite_dirty_ratio: 0.0,
            light_dirty_ratio: 0.0,
            postfx: true,
        },
        Scenario {
            name: "lit_sparse_5_postfx",
            path: ProbeRenderPath::LitHdr,
            sprites: 4_000,
            lights: 256,
            views: 1,
            sprite_dirty_ratio: 0.05,
            light_dirty_ratio: 0.05,
            postfx: true,
        },
        Scenario {
            name: "lit_static_no_postfx",
            path: ProbeRenderPath::LitHdr,
            sprites: 4_000,
            lights: 256,
            views: 1,
            sprite_dirty_ratio: 0.0,
            light_dirty_ratio: 0.0,
            postfx: false,
        },
    ];

    let mut results = Vec::with_capacity(scenarios.len());
    for scenario in scenarios {
        let result = run_scenario(&mut ctx, scenario, &config);
        results.push(result);
    }

    if config.csv {
        print_csv(&results);
    } else {
        print_table(&results, &config);
    }
}

fn run_scenario(ctx: &mut GpuContext, scenario: Scenario, config: &ProbeConfig) -> ScenarioResult {
    let mut renderer = match scenario.path {
        ProbeRenderPath::Unlit => RenderComposer::from_asset(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ),
        ProbeRenderPath::LitHdr => RenderComposer::from_asset(RenderPipelineAsset::forward_2d()),
    };
    let mut scene = build_scene(ctx, scenario);
    let mut accum = StatsAccumulator {
        frame_ms_sync_values: Vec::with_capacity(config.sample_frames),
        ..Default::default()
    };
    let total_frames = config.warmup_frames + config.sample_frames;

    for frame_index in 0..total_frames {
        apply_dirty(
            frame_index,
            scenario,
            &mut scene.world,
            &scene.sprite_entities,
            &scene.light_entities,
        );

        let frame_start = Instant::now();
        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        renderer.render_world(ctx, &scene.world);
        ctx.end_frame();
        let _ = ctx.device().poll(wgpu::MaintainBase::Wait);
        let frame_ms_sync = frame_start.elapsed().as_secs_f64() * 1000.0;

        if frame_index >= config.warmup_frames {
            accum.record(frame_ms_sync, renderer.stats());
        }
    }

    accum.finish(scenario)
}

fn build_scene(ctx: &GpuContext, scenario: Scenario) -> ProbeScene {
    let mut rng = SimpleRng::new(seed_for_scenario(scenario.name));
    let mut world = World::new();

    if scenario.path == ProbeRenderPath::LitHdr {
        world.insert_resource(if scenario.postfx {
            RenderSettings::default()
        } else {
            RenderSettings {
                bloom: BloomSettings {
                    enabled: false,
                    ..Default::default()
                },
                tonemap: ToneMapSettings {
                    enabled: false,
                    ..Default::default()
                },
                vignette: VignetteSettings {
                    enabled: false,
                    ..Default::default()
                },
                ..RenderSettings::default()
            }
        });
    }

    spawn_views(&mut world, scenario.views);

    let mut sprite_entities = Vec::with_capacity(scenario.sprites);
    for index in 0..scenario.sprites {
        let size = rng.range(6.0, 22.0);
        let hue = rng.range(0.0, 360.0);
        let x = rng.range(-620.0, 620.0);
        let y = rng.range(-340.0, 340.0);
        let entity = world.spawn((
            Transform::from_xyz(x, y, (index % 32) as f32 * 0.01),
            SpriteRenderer::new(size, size).color(Color::hsl(hue, 0.8, 0.6)),
        ));
        sprite_entities.push(entity);
    }

    let mut light_entities = Vec::with_capacity(scenario.lights);
    if scenario.lights > 0 {
        for _ in 0..scenario.lights {
            let hue = rng.range(0.0, 360.0);
            let entity = world.spawn((
                Transform::from_xy(rng.range(-620.0, 620.0), rng.range(-340.0, 340.0)),
                PointLight::new(rng.range(36.0, 120.0))
                    .intensity(rng.range(0.7, 1.6))
                    .color(Color::hsl(hue, 0.7, 0.55))
                    .temperature(rng.range(2800.0, 8500.0))
                    .falloff(rng.range(1.3, 2.0)),
            ));
            light_entities.push(entity);
        }
    }

    let _ = ctx;
    ProbeScene {
        world,
        sprite_entities,
        light_entities,
    }
}

fn spawn_views(world: &mut World, views: usize) {
    match views {
        0 | 1 => {
            world.spawn((
                Transform::default(),
                CameraMarker::new(),
                Projection::orthographic(DEFAULT_SURFACE_SIZE[1] as f32),
                MainCamera,
            ));
        }
        2 => {
            let half_width = DEFAULT_SURFACE_SIZE[0] / 2;
            world.spawn((
                Transform::default(),
                CameraMarker::new(),
                Projection::orthographic(DEFAULT_SURFACE_SIZE[1] as f32),
                CameraViewport::new(ViewportRect::new(0, 0, half_width, DEFAULT_SURFACE_SIZE[1])),
                MainCamera,
            ));
            world.spawn((
                Transform::default(),
                CameraMarker::new(),
                Projection::orthographic(DEFAULT_SURFACE_SIZE[1] as f32),
                CameraViewport::new(ViewportRect::new(
                    half_width,
                    0,
                    DEFAULT_SURFACE_SIZE[0] - half_width,
                    DEFAULT_SURFACE_SIZE[1],
                ))
                .order(1),
            ));
        }
        _ => panic!("unsupported probe view count: {views}"),
    }
}

fn apply_dirty(
    frame_index: usize,
    scenario: Scenario,
    world: &mut World,
    sprite_entities: &[EntityId],
    light_entities: &[EntityId],
) {
    let sprite_dirty_count = dirty_count(sprite_entities.len(), scenario.sprite_dirty_ratio);
    for offset in 0..sprite_dirty_count {
        let entity = sprite_entities[(frame_index * 997 + offset) % sprite_entities.len()];
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.position[0] += 1.5 + (offset % 7) as f32 * 0.1;
            transform.position[1] += 0.5 + (offset % 5) as f32 * 0.07;
            transform.rotate_z(0.01);
            if transform.position[0] > 640.0 {
                transform.position[0] = -640.0;
            }
            if transform.position[1] > 360.0 {
                transform.position[1] = -360.0;
            }
        }
    }

    let light_dirty_count = dirty_count(light_entities.len(), scenario.light_dirty_ratio);
    for offset in 0..light_dirty_count {
        let entity = light_entities[(frame_index * 131 + offset) % light_entities.len()];
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.position[0] += 1.2;
            transform.position[1] -= 0.8;
            if transform.position[0] > 640.0 {
                transform.position[0] = -640.0;
            }
            if transform.position[1] < -360.0 {
                transform.position[1] = 360.0;
            }
        }
        if let Some(light) = world.get_mut::<PointLight>(entity) {
            light.radius = 36.0 + ((frame_index + offset) % 96) as f32;
        }
    }
}

fn dirty_count(total: usize, ratio: f32) -> usize {
    if total == 0 || ratio <= 0.0 {
        return 0;
    }
    ((total as f32 * ratio).round() as usize).clamp(1, total)
}

fn create_probe_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for renderer_probe");

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("renderer_probe_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("Failed to create probe GPU device")
}

fn parse_args(args: impl Iterator<Item = String>) -> ProbeConfig {
    let mut config = ProbeConfig::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--csv" => config.csv = true,
            "--frames" => {
                let value = args.next().expect("--frames requires a numeric value");
                config.sample_frames = value
                    .parse::<usize>()
                    .expect("--frames must be a positive integer");
            }
            "--warmup" => {
                let value = args.next().expect("--warmup requires a numeric value");
                config.warmup_frames = value
                    .parse::<usize>()
                    .expect("--warmup must be a positive integer");
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => panic!("unrecognized argument: {other}"),
        }
    }
    config
}

fn print_help() {
    eprintln!("renderer_probe");
    eprintln!("  --frames <N>   sample frame count per scenario (default: 90)");
    eprintln!("  --warmup <N>   warmup frame count per scenario (default: 30)");
    eprintln!("  --csv          print CSV instead of a table");
}

fn print_table(results: &[ScenarioResult], config: &ProbeConfig) {
    println!(
        "RenderComposer probe | headless | warmup={} | sample={}",
        config.warmup_frames, config.sample_frames
    );
    println!(
        "{:<22} {:>8} {:>8} {:>9} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>7}",
        "scenario",
        "fps",
        "frame",
        "p95",
        "render",
        "extract",
        "resize",
        "prepare",
        "upload",
        "execute",
        "d_spr",
        "d_lit",
        "vis_spr",
        "vis_lit",
        "draws"
    );
    for result in results {
        let fps = 1000.0 / result.avg_frame_ms_sync.max(f64::EPSILON);
        println!(
            "{:<22} {:>8.0} {:>8.2} {:>9.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.1} {:>8.1} {:>8.0} {:>8.0} {:>7.1}",
            result.scenario.name,
            fps,
            result.avg_frame_ms_sync,
            result.p95_frame_ms_sync,
            result.avg_renderer_frame_ms,
            result.avg_extract_ms,
            result.avg_resize_ms,
            result.avg_prepare_ms,
            result.avg_upload_ms,
            result.avg_execute_ms,
            result.avg_dirty_sprite_slots,
            result.avg_dirty_light_slots,
            result.avg_visible_sprite_count,
            result.avg_visible_light_count,
            result.avg_draw_calls,
        );
    }
}

fn print_csv(results: &[ScenarioResult]) {
    println!(
        "scenario,fps_avg,frame_ms_avg,frame_ms_p95,renderer_ms_avg,extract_ms_avg,resize_ms_avg,prepare_ms_avg,upload_ms_avg,execute_ms_avg,dirty_sprite_slots_avg,dirty_light_slots_avg,visible_sprites_avg,visible_lights_avg,draw_calls_avg,passes_avg"
    );
    for result in results {
        let fps = 1000.0 / result.avg_frame_ms_sync.max(f64::EPSILON);
        println!(
            "{},{:.2},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
            result.scenario.name,
            fps,
            result.avg_frame_ms_sync,
            result.p95_frame_ms_sync,
            result.avg_renderer_frame_ms,
            result.avg_extract_ms,
            result.avg_resize_ms,
            result.avg_prepare_ms,
            result.avg_upload_ms,
            result.avg_execute_ms,
            result.avg_dirty_sprite_slots,
            result.avg_dirty_light_slots,
            result.avg_visible_sprite_count,
            result.avg_visible_light_count,
            result.avg_draw_calls,
            result.avg_passes,
        );
    }
}

fn seed_for_scenario(name: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
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
