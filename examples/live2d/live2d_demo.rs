//! # Live2D Demo
//!
//! Focused integration example for loading and rendering Cubism models
//! with an egui control panel for model switching and expression selection.
//!
//! ```bash
//! cargo run --example live2d_demo --features "live2d egui" --release -- [--no-ui] <model1.model3.json> [model2.model3.json ...]
//! cargo run --example live2d_demo --features "live2d egui" --release -- --no-ui assets/Haru/Haru.model3.json
//! ```

use sky_engine::app::{egui, App, AppConfig, KeyCode};
use sky_engine::ecs::World;
use sky_engine::render::expert::live2d::clipping::ClippingManager;
use sky_engine::render::expert::live2d::{Live2DModelResource, Live2DRenderer};

struct DemoOptions {
    model_paths: Vec<String>,
    ui_visible: bool,
}

/// One loaded model slot.
struct ModelSlot {
    /// Display name (filename stem).
    name: String,
    resource: Live2DModelResource,
    clipping: Option<ClippingManager>,
    expression_names: Vec<String>,
}

fn print_usage() {
    eprintln!("Usage: live2d_demo [--no-ui] <model1.model3.json> [model2 ...]");
    eprintln!("  --no-ui   start in pure render mode for FPS A/B");
    eprintln!("  --ui      force the control panel on at startup");
    eprintln!("  U         toggle the control panel while running");
    eprintln!("  e.g. live2d_demo --no-ui assets/Haru/Haru.model3.json");
}

fn parse_args() -> DemoOptions {
    let mut model_paths = Vec::new();
    let mut ui_visible = true;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--no-ui" => ui_visible = false,
            "--ui" => ui_visible = true,
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => {
                eprintln!("Unknown option: {arg}");
                print_usage();
                std::process::exit(1);
            }
            _ => model_paths.push(arg),
        }
    }

    if model_paths.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    DemoOptions {
        model_paths,
        ui_visible,
    }
}

fn main() {
    let DemoOptions {
        model_paths,
        mut ui_visible,
    } = parse_args();

    let config = AppConfig::new("SkyEngine — Live2D", 1280, 720)
        .with_vsync(false)
        .with_resizable(true);

    let mut slots: Vec<ModelSlot> = Vec::new();
    let mut active: usize = 0;
    let mut l2d_renderer: Option<Live2DRenderer> = None;
    let mut initialized = false;

    // FPS tracking
    let mut fps_accum = 0.0_f32;
    let mut fps_frames = 0u32;
    let mut fps_display = 0.0_f32;
    let mut last_titled_active = usize::MAX;
    let mut last_titled_fps = -1.0_f32;
    let mut last_titled_ui_visible = !ui_visible;

    let world = World::new();

    App::new(config, world).run(move |ctx: &mut sky_engine::app::FrameContext| {
        let dt = ctx.dt;

        // FPS counter
        fps_accum += dt;
        fps_frames += 1;
        if fps_accum >= 0.5 {
            fps_display = fps_frames as f32 / fps_accum;
            fps_accum = 0.0;
            fps_frames = 0;
        }

        // ── Lazy init ──────────────────────────────────────────────
        if !initialized {
            for path in &model_paths {
                eprintln!("[Live2D] Loading model: {path}");
                match Live2DModelResource::load(ctx.gpu(), path) {
                    Ok(res) => {
                        eprintln!(
                            "[Live2D] Loaded: {} drawables, {} textures",
                            res.model.drawable_count(),
                            res.textures.len(),
                        );

                        let clip_mgr = ClippingManager::new(&res.model);
                        let clipping = if clip_mgr.has_masks() {
                            Some(clip_mgr)
                        } else {
                            None
                        };

                        let expression_names: Vec<String> = res
                            .expression_player
                            .as_ref()
                            .map(|p| p.expression_names().map(|n| n.to_string()).collect())
                            .unwrap_or_default();

                        let name = std::path::Path::new(path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Unknown")
                            .to_string();

                        slots.push(ModelSlot {
                            name,
                            resource: res,
                            clipping,
                            expression_names,
                        });
                    }
                    Err(e) => {
                        eprintln!("[Live2D] Failed to load {path}: {e}");
                    }
                }
            }

            if slots.is_empty() {
                eprintln!("[Live2D] No models loaded, exiting.");
                ctx.request_exit();
                return;
            }

            l2d_renderer = Some(Live2DRenderer::new(ctx.gpu()));
            eprintln!(
                "[Live2D] UI: {} (press U to toggle)",
                if ui_visible { "on" } else { "off" }
            );
            initialized = true;
        }

        if slots.is_empty() {
            return;
        }

        if ctx.input.key_pressed(KeyCode::KeyU) {
            ui_visible = !ui_visible;
            eprintln!("[Live2D] UI -> {}", if ui_visible { "on" } else { "off" });
        }

        // UI output: which expression was clicked (if any)
        let mut clicked_expression: Option<usize> = None;

        // ── egui control panel ─────────────────────────────────────
        if ui_visible {
            ctx.egui(|egui_ctx| {
                egui::SidePanel::left("live2d_panel")
                    .default_width(200.0)
                    .resizable(true)
                    .show(egui_ctx, |ui| {
                        ui.heading("🎭 Live2D");
                        ui.separator();

                        // FPS
                        ui.label(format!("FPS: {fps_display:.0}"));
                        ui.label("U: toggle UI / pure render");
                        ui.separator();

                        // Model selector (only if multiple loaded)
                        if slots.len() > 1 {
                            ui.label("Model");
                            for (i, slot) in slots.iter().enumerate() {
                                ui.radio_value(&mut active, i, &slot.name);
                            }
                            ui.separator();
                        }

                        let active_slot = &slots[active];

                        // Active model name
                        ui.strong(&active_slot.name);
                        ui.add_space(4.0);

                        // Expression buttons
                        if !active_slot.expression_names.is_empty() {
                            ui.label("Expressions");
                            egui::ScrollArea::vertical()
                                .max_height(400.0)
                                .show(ui, |ui| {
                                    for (index, name) in
                                        active_slot.expression_names.iter().enumerate()
                                    {
                                        if ui.button(name).clicked() {
                                            clicked_expression = Some(index);
                                        }
                                    }
                                });
                        } else {
                            ui.weak("(no expressions)");
                        }
                    });
            });
        }

        // ── Apply expression change (deferred from UI) ─────────────
        if let Some(expr_index) = clicked_expression {
            let expr = slots[active].expression_names[expr_index].clone();
            if slots[active].resource.set_expression(&expr) {
                eprintln!("[Live2D] Expression -> {expr}");
            }
        }

        // ── Update & Render active model ───────────────────────────
        slots[active].resource.update(dt);

        let gpu = ctx.gpu();

        // Clear surface
        gpu.with_surface_pass(
            "live2d_clear",
            Some(wgpu::Color {
                r: 0.12,
                g: 0.12,
                b: 0.18,
                a: 1.0,
            }),
            |_| {},
        );

        // Draw model
        let slot = &mut slots[active];
        if let Some(ref mut rend) = l2d_renderer {
            rend.draw_to_surface(
                gpu,
                &slot.resource.model,
                &slot.resource.textures,
                &mut slot.clipping,
            );
        }

        // Title
        if active != last_titled_active
            || fps_display != last_titled_fps
            || ui_visible != last_titled_ui_visible
        {
            ctx.set_title(&format!(
                "SkyEngine — {} | {:.0} FPS | {}",
                slots[active].name,
                fps_display,
                if ui_visible { "UI" } else { "No UI" }
            ));
            last_titled_active = active;
            last_titled_fps = fps_display;
            last_titled_ui_visible = ui_visible;
        }
    });
}
