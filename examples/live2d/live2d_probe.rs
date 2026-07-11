//! Live2D renderer performance probe.
//!
//! Headless benchmark for the Live2D rendering pipeline.  Measures CPU update,
//! frame preparation, mask pass, model pass, and GPU drain phases independently.
//! To keep the renderer API unchanged, the exclusive model-pass time is derived
//! from `full_execute_ms - isolated_mask_ms` for the same prepared frame.
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
use sky_engine::render::expert::live2d::{
    runtime::Live2DUpdateTimings, Live2DModelResource, Live2DRenderer, Live2DUserModel,
};
use sky_engine::render::expert::gpu::RenderTarget;

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
    update: Live2DUpdateTimings,
    prepare_ms: f64,
    mask_ms: f64,
    model_ms: f64,
}

#[derive(Default)]
struct StatsAccumulator {
    sample_count: usize,
    frame_ms_sync_sum: f64,
    frame_ms_sync_values: Vec<f64>,
    update_sum: Live2DUpdateTimings,
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
    avg_update: Live2DUpdateTimings,
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
        accumulate_update_timings(&mut self.update_sum, &timings.update);
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
            avg_update: scaled_update_timings(self.update_sum, inv),
            avg_prepare_ms: self.prepare_ms_sum * inv,
            avg_mask_ms: self.mask_ms_sum * inv,
            avg_model_ms: self.model_ms_sum * inv,
            avg_visible_drawables: self.visible_drawables_sum as f64 * inv,
            avg_mask_draws: self.mask_draws_sum as f64 * inv,
            avg_bind_groups: self.bind_groups_sum as f64 * inv,
        }
    }
}

fn accumulate_update_timings(total: &mut Live2DUpdateTimings, sample: &Live2DUpdateTimings) {
    total.load_parameters += sample.load_parameters;
    total.motion += sample.motion;
    total.save_parameters += sample.save_parameters;
    total.eye_blink += sample.eye_blink;
    total.expression += sample.expression;
    total.look += sample.look;
    total.breath += sample.breath;
    total.physics += sample.physics;
    total.lip_sync += sample.lip_sync;
    total.pose += sample.pose;
    total.model_update += sample.model_update;
    total.total += sample.total;
}

fn scaled_update_timings(sum: Live2DUpdateTimings, scale: f64) -> Live2DUpdateTimings {
    Live2DUpdateTimings {
        load_parameters: sum.load_parameters * scale,
        motion: sum.motion * scale,
        save_parameters: sum.save_parameters * scale,
        eye_blink: sum.eye_blink * scale,
        expression: sum.expression * scale,
        look: sum.look * scale,
        breath: sum.breath * scale,
        physics: sum.physics * scale,
        lip_sync: sum.lip_sync * scale,
        pose: sum.pose * scale,
        model_update: sum.model_update * scale,
        total: sum.total * scale,
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
            name: "render_repeat_x2",
            dt: 1.0 / 60.0,
            animate: true,
            instances: 2,
        },
        Scenario {
            name: "render_repeat_x4",
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
        let mut frame_mask_draws = 0usize;
        let mut frame_model_draws = 0usize;

        // ── Phase 1: CPU update (motion, physics, etc.) ─────────
        if scenario.animate {
            timings.update = user_model.update_profiled(scenario.dt);
        }

        // ── Phase 3a: Isolated mask diagnostics ─────────────────
        if has_masks {
            ctx.begin_frame()
                .expect("headless begin_frame should succeed");
            let mut diagnostic_prepared_frames = Vec::with_capacity(scenario.instances);
            for _inst in 0..scenario.instances {
                diagnostic_prepared_frames.push(renderer.prepare_frame_for_target(
                    ctx,
                    &target,
                    user_model.model(),
                    resource.textures(),
                    &mut clipping,
                ));
            }
            let t2 = Instant::now();
            for prepared in &diagnostic_prepared_frames {
                renderer.execute_prepared_mask_pass(ctx, prepared);
            }
            timings.mask_ms = t2.elapsed().as_secs_f64() * 1000.0;
            ctx.end_frame();
            let _ = ctx.device().poll(wgpu::PollType::wait_indefinitely());
        }

        // ── Phase 3b: Actual full render frame ──────────────────
        let frame_start = Instant::now();
        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        let mut prepared_frames = Vec::with_capacity(scenario.instances);
        for _inst in 0..scenario.instances {
            let t1 = Instant::now();
            let prepared = renderer.prepare_frame_for_target(
                ctx,
                &target,
                user_model.model(),
                resource.textures(),
                &mut clipping,
            );
            timings.prepare_ms += t1.elapsed().as_secs_f64() * 1000.0;
            frame_mask_draws += prepared.mask_draw_count();
            frame_model_draws += prepared.model_draw_count();
            prepared_frames.push(prepared);
        }
        let t3 = Instant::now();
        for prepared in &prepared_frames {
            renderer.execute_prepared_model_to_target(ctx, &target, prepared);
        }
        let full_execute_ms = t3.elapsed().as_secs_f64() * 1000.0;
        timings.model_ms = (full_execute_ms - timings.mask_ms).max(0.0);

        ctx.end_frame();
        let _ = ctx.device().poll(wgpu::PollType::wait_indefinitely());
        let frame_ms_sync = frame_start.elapsed().as_secs_f64() * 1000.0;

        // Count visible drawables & estimate bind groups
        let visible_drawables = (0..drawable_count)
            .filter(|&i| user_model.model().drawable_is_visible(i))
            .count()
            * scenario.instances;
        let mask_draws = frame_mask_draws;
        // One prepared draw maps closely to one bind-group selection in this path.
        let bind_groups = frame_model_draws + frame_mask_draws;

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
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("live2d_probe_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
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
    if results.iter().any(|result| result.scenario.instances > 1) {
        println!(
            "Note: render_repeat_xN reuses one updated model N times; runtime update timings remain single-instance."
        );
    }
    println!(
        "{:<20} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "scenario",
        "fps",
        "frame",
        "p95",
        "update",
        "load",
        "motion",
        "save",
        "phys",
        "mdl_upd",
        "prepare",
        "mask",
        "model",
        "vis_drw",
        "mask_d",
        "bgroups"
    );
    println!("{}", "-".repeat(150));
    for r in results {
        let fps = 1000.0 / r.avg_frame_ms_sync.max(f64::EPSILON);
        println!(
            "{:<20} {:>8.0} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>8.1} {:>8.1} {:>8.1}",
            r.scenario.name,
            fps,
            r.avg_frame_ms_sync,
            r.p95_frame_ms_sync,
            r.avg_update.total,
            r.avg_update.load_parameters,
            r.avg_update.motion,
            r.avg_update.save_parameters,
            r.avg_update.physics,
            r.avg_update.model_update,
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
        "scenario,fps_avg,frame_ms_avg,frame_ms_p95,update_ms_avg,load_parameters_ms_avg,motion_ms_avg,save_parameters_ms_avg,eye_blink_ms_avg,expression_ms_avg,look_ms_avg,breath_ms_avg,physics_ms_avg,lip_sync_ms_avg,pose_ms_avg,model_update_ms_avg,prepare_ms_avg,mask_ms_avg,model_ms_avg,drawables,textures,visible_drawables_avg,mask_draws_avg,bind_groups_avg,instances"
    );
    for r in results {
        let fps = 1000.0 / r.avg_frame_ms_sync.max(f64::EPSILON);
        println!(
            "{},{:.2},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{:.2},{:.2},{:.2},{}",
            r.scenario.name,
            fps,
            r.avg_frame_ms_sync,
            r.p95_frame_ms_sync,
            r.avg_update.total,
            r.avg_update.load_parameters,
            r.avg_update.motion,
            r.avg_update.save_parameters,
            r.avg_update.eye_blink,
            r.avg_update.expression,
            r.avg_update.look,
            r.avg_update.breath,
            r.avg_update.physics,
            r.avg_update.lip_sync,
            r.avg_update.pose,
            r.avg_update.model_update,
            r.avg_prepare_ms,
            r.avg_mask_ms,
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
