//! Live2D renderer performance probe.
//!
//! Headless benchmark for the Live2D rendering pipeline.  Measures CPU update,
//! frame preparation, mask pass, model pass, and GPU drain phases independently.
//!
//! ```bash
//! $env:LIVE2D_CUBISM_SDK_NATIVE_DIR='C:\Coding\SkyEngine\CubismSdkForNative'
//! cargo run --example live2d_probe --features live2d --release -- <model.model3.json>
//! cargo run --example live2d_probe --features live2d --release -- --csv <model.model3.json>
//! cargo run --example live2d_probe --features live2d --release -- --frames 240 --warmup 60 <model.model3.json>
//! ```

use std::time::Instant;

use sky_engine::gpu::GpuContext;
use sky_engine::render::expert::live2d::render::clipping::ClippingManager;
use sky_engine::render::expert::live2d::{Live2DModelResource, Live2DRenderer, Live2DUserModel};
use sky_engine::render::expert::RenderTarget;

const DEFAULT_SURFACE_SIZE: [u32; 2] = [1280, 720];
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct ProbeConfig {
    warmup_frames: usize,
    sample_frames: usize,
    csv: bool,
    model_path: String,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            warmup_frames: 30,
            sample_frames: 120,
            csv: false,
            model_path: String::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct Scenario {
    name: &'static str,
    /// Simulated delta-time per frame (seconds).
    dt: f32,
    /// Whether to run `user_model.update(dt)` each frame (vs. static model).
    animate: bool,
    /// Number of model instances to render (1 = single model, N = measure scaling).
    instances: usize,
}

#[derive(Default, Clone)]
struct FrameTimings {
    update_ms: f64,
    prepare_ms: f64,
    mask_ms: f64,
    model_ms: f64,
}

#[derive(Default)]
struct StatsAccumulator {
    sample_count: usize,
    frame_ms_sync_sum: f64,
    frame_ms_sync_values: Vec<f64>,
    update_ms_sum: f64,
    prepare_ms_sum: f64,
    mask_ms_sum: f64,
    model_ms_sum: f64,
    drawables_sum: usize,
    visible_drawables_sum: usize,
    mask_draws_sum: usize,
    bind_groups_sum: usize,
}

struct ScenarioResult {
    scenario: Scenario,
    drawable_count: usize,
    texture_count: usize,
    avg_frame_ms_sync: f64,
    p95_frame_ms_sync: f64,
    avg_update_ms: f64,
    avg_prepare_ms: f64,
    avg_mask_ms: f64,
    avg_model_ms: f64,
    avg_visible_drawables: f64,
    avg_mask_draws: f64,
    avg_bind_groups: f64,
}

impl StatsAccumulator {
    fn record(
        &mut self,
        frame_ms_sync: f64,
        timings: &FrameTimings,
        visible_drawables: usize,
        mask_draws: usize,
        bind_groups: usize,
        drawable_count: usize,
    ) {
        self.sample_count += 1;
        self.frame_ms_sync_sum += frame_ms_sync;
        self.frame_ms_sync_values.push(frame_ms_sync);
        self.update_ms_sum += timings.update_ms;
        self.prepare_ms_sum += timings.prepare_ms;
        self.mask_ms_sum += timings.mask_ms;
        self.model_ms_sum += timings.model_ms;
        self.drawables_sum += drawable_count;
        self.visible_drawables_sum += visible_drawables;
        self.mask_draws_sum += mask_draws;
        self.bind_groups_sum += bind_groups;
    }

    fn finish(
        mut self,
        scenario: Scenario,
        drawable_count: usize,
        texture_count: usize,
    ) -> ScenarioResult {
        assert!(self.sample_count > 0);
        self.frame_ms_sync_values.sort_by(|a, b| a.total_cmp(b));
        let p95_index = ((self.frame_ms_sync_values.len() as f64) * 0.95).floor() as usize;
        let p95_index = p95_index.min(self.frame_ms_sync_values.len() - 1);
        let inv = 1.0 / self.sample_count as f64;
        ScenarioResult {
            scenario,
            drawable_count,
            texture_count,
            avg_frame_ms_sync: self.frame_ms_sync_sum * inv,
            p95_frame_ms_sync: self.frame_ms_sync_values[p95_index],
            avg_update_ms: self.update_ms_sum * inv,
            avg_prepare_ms: self.prepare_ms_sum * inv,
            avg_mask_ms: self.mask_ms_sum * inv,
            avg_model_ms: self.model_ms_sum * inv,
            avg_visible_drawables: self.visible_drawables_sum as f64 * inv,
            avg_mask_draws: self.mask_draws_sum as f64 * inv,
            avg_bind_groups: self.bind_groups_sum as f64 * inv,
        }
    }
}

fn main() {
    let config = parse_args(std::env::args().skip(1));
    let (device, queue) = create_probe_device();
    let mut ctx = GpuContext::new_headless(device, queue, TARGET_FORMAT, DEFAULT_SURFACE_SIZE);

    // Load model
    eprintln!("[live2d_probe] Loading model: {}", config.model_path);
    let resource =
        Live2DModelResource::load(&ctx, &config.model_path).expect("Failed to load Live2D model");
    let mut user_model = resource
        .instantiate()
        .expect("Failed to instantiate Live2D runtime");
    let has_masks = {
        let mgr = ClippingManager::new(user_model.model());
        mgr.has_masks()
    };
    let drawable_count = user_model.model().drawable_count();
    let texture_count = resource.texture_count();
    eprintln!(
        "[live2d_probe] Loaded: {} drawables, {} textures, masks={}",
        drawable_count, texture_count, has_masks,
    );

    let scenarios = [
        Scenario {
            name: "static_model",
            dt: 0.0,
            animate: false,
            instances: 1,
        },
        Scenario {
            name: "animated_60fps",
            dt: 1.0 / 60.0,
            animate: true,
            instances: 1,
        },
        Scenario {
            name: "animated_144fps",
            dt: 1.0 / 144.0,
            animate: true,
            instances: 1,
        },
        Scenario {
            name: "animated_x2",
            dt: 1.0 / 60.0,
            animate: true,
            instances: 2,
        },
        Scenario {
            name: "animated_x4",
            dt: 1.0 / 60.0,
            animate: true,
            instances: 4,
        },
    ];

    let mut results = Vec::with_capacity(scenarios.len());
    for scenario in scenarios {
        let result = run_scenario(
            &mut ctx,
            &resource,
            &mut user_model,
            has_masks,
            scenario,
            &config,
        );
        results.push(result);
    }

    if config.csv {
        print_csv(&results);
    } else {
        print_table(&results, &config);
    }
}

fn run_scenario(
    ctx: &mut GpuContext,
    resource: &Live2DModelResource,
    user_model: &mut Live2DUserModel,
    has_masks: bool,
    scenario: Scenario,
    config: &ProbeConfig,
) -> ScenarioResult {
    let mut renderer = Live2DRenderer::new(ctx);
    let target = RenderTarget::new(
        ctx,
        DEFAULT_SURFACE_SIZE[0],
        DEFAULT_SURFACE_SIZE[1],
        TARGET_FORMAT,
        "live2d_probe_target",
    );
    let drawable_count = user_model.model().drawable_count();
    let texture_count = resource.texture_count();

    // Reset model to default state
    user_model.reset_to_default_parameters();

    let mut clipping = if has_masks {
        Some(ClippingManager::new(user_model.model()))
    } else {
        None
    };
    let mut accum = StatsAccumulator {
        frame_ms_sync_values: Vec::with_capacity(config.sample_frames),
        ..Default::default()
    };
    let total_frames = config.warmup_frames + config.sample_frames;

    for frame_index in 0..total_frames {
        let mut timings = FrameTimings::default();

        // ── Phase 1: CPU update (motion, physics, etc.) ─────────
        let t0 = Instant::now();
        if scenario.animate {
            user_model.update(scenario.dt);
        }
        timings.update_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // ── Begin GPU frame ─────────────────────────────────────
        let frame_start = Instant::now();
        ctx.begin_frame()
            .expect("headless begin_frame should succeed");

        // Render N instances
        for _inst in 0..scenario.instances {
            // ── Phase 2: Prepare (clipping, uniform push, uploads, bind groups)
            let t1 = Instant::now();
            let prepared = renderer.prepare_frame_for_target(
                ctx,
                &target,
                user_model.model(),
                resource.textures(),
                &mut clipping,
            );
            timings.prepare_ms += t1.elapsed().as_secs_f64() * 1000.0;

            // ── Phase 3: Render pass (includes any inline mask work) ─
            let t3 = Instant::now();
            renderer.execute_prepared_model_to_target(ctx, &target, &prepared);
            timings.model_ms += t3.elapsed().as_secs_f64() * 1000.0;
        }

        ctx.end_frame();
        let _ = ctx.device().poll(wgpu::MaintainBase::Wait);
        let frame_ms_sync = frame_start.elapsed().as_secs_f64() * 1000.0;

        // Count visible drawables & estimate bind groups
        let visible_drawables = (0..drawable_count)
            .filter(|&i| user_model.model().drawable_is_visible(i))
            .count()
            * scenario.instances;
        let mask_draws = clipping
            .as_ref()
            .map(|c| {
                c.contexts
                    .iter()
                    .flat_map(|ctx_entry| &ctx_entry.mask_drawable_indices)
                    .filter(|&&idx| user_model.model().drawable_is_visible(idx))
                    .count()
            })
            .unwrap_or(0)
            * scenario.instances;
        // Each visible drawable creates 1 bind group, each mask draw creates 1 bind group
        let bind_groups = (visible_drawables + mask_draws) * scenario.instances;

        if frame_index >= config.warmup_frames {
            accum.record(
                frame_ms_sync,
                &timings,
                visible_drawables,
                mask_draws,
                bind_groups,
                drawable_count * scenario.instances,
            );
        }
    }

    accum.finish(scenario, drawable_count, texture_count)
}

fn create_probe_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for live2d_probe");

    let info = adapter.get_info();
    eprintln!(
        "[live2d_probe] GPU: {} | Backend: {:?}",
        info.name, info.backend
    );

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("live2d_probe_device"),
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
    let args: Vec<String> = args.collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--csv" => {
                config.csv = true;
                i += 1;
            }
            "--frames" => {
                config.sample_frames = args[i + 1]
                    .parse::<usize>()
                    .expect("--frames requires a positive integer");
                i += 2;
            }
            "--warmup" => {
                config.warmup_frames = args[i + 1]
                    .parse::<usize>()
                    .expect("--warmup requires a positive integer");
                i += 2;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ if args[i].starts_with('-') => {
                panic!("unrecognized argument: {}", args[i]);
            }
            _ => {
                config.model_path = args[i].clone();
                i += 1;
            }
        }
    }
    if config.model_path.is_empty() {
        eprintln!("Error: no model path specified.");
        print_help();
        std::process::exit(1);
    }
    config
}

fn print_help() {
    eprintln!("live2d_probe — Live2D rendering performance benchmark");
    eprintln!("  --frames <N>   sample frame count per scenario (default: 120)");
    eprintln!("  --warmup <N>   warmup frame count per scenario (default: 30)");
    eprintln!("  --csv          print CSV instead of a table");
    eprintln!("  <model.model3.json>  path to the Live2D model file");
}

fn print_table(results: &[ScenarioResult], config: &ProbeConfig) {
    if let Some(first) = results.first() {
        println!(
            "Live2D probe | headless {}x{} | drawables={} textures={} | warmup={} sample={}",
            DEFAULT_SURFACE_SIZE[0],
            DEFAULT_SURFACE_SIZE[1],
            first.drawable_count,
            first.texture_count,
            config.warmup_frames,
            config.sample_frames,
        );
    }
    println!(
        "{:<20} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "scenario",
        "fps",
        "frame",
        "p95",
        "update",
        "prepare",
        "mask",
        "model",
        "vis_drw",
        "mask_d",
        "bgroups"
    );
    println!("{}", "-".repeat(110));
    for r in results {
        let fps = 1000.0 / r.avg_frame_ms_sync.max(f64::EPSILON);
        println!(
            "{:<20} {:>8.0} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.1} {:>8.1} {:>8.1}",
            r.scenario.name,
            fps,
            r.avg_frame_ms_sync,
            r.p95_frame_ms_sync,
            r.avg_update_ms,
            r.avg_prepare_ms,
            r.avg_mask_ms,
            r.avg_model_ms,
            r.avg_visible_drawables,
            r.avg_mask_draws,
            r.avg_bind_groups,
        );
    }
}

fn print_csv(results: &[ScenarioResult]) {
    println!(
        "scenario,fps_avg,frame_ms_avg,frame_ms_p95,update_ms_avg,prepare_ms_avg,model_ms_avg,drawables,textures,visible_drawables_avg,mask_draws_avg,bind_groups_avg,instances"
    );
    for r in results {
        let fps = 1000.0 / r.avg_frame_ms_sync.max(f64::EPSILON);
        println!(
            "{},{:.2},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{:.2},{:.2},{:.2},{}",
            r.scenario.name,
            fps,
            r.avg_frame_ms_sync,
            r.p95_frame_ms_sync,
            r.avg_update_ms,
            r.avg_prepare_ms,
            r.avg_model_ms,
            r.drawable_count,
            r.texture_count,
            r.avg_visible_drawables,
            r.avg_mask_draws,
            r.avg_bind_groups,
            r.scenario.instances,
        );
    }
}
