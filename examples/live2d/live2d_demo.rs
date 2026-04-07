//! # Live2D Demo
//!
//! Focused integration example for loading and rendering Cubism models
//! with an egui control panel for model switching and expression selection.
//!
//! ```bash
//! cargo run --example live2d_demo --features "live2d egui" --release -- [--no-ui] <model1.model3.json> [model2.model3.json ...]
//! cargo run --example live2d_demo --features "live2d egui" --release -- [--no-ui] <folder-with-models>
//! cargo run --example live2d_demo --features "live2d egui" --release -- --no-ui assets/Haru/Haru.model3.json
//! ```

use std::path::{Path, PathBuf};

use sky_engine::app::{egui, App, AppConfig, KeyCode};
use sky_engine::ecs::World;
use sky_engine::render::expert::live2d::render::clipping::ClippingManager;
use sky_engine::render::expert::live2d::{Live2DModelResource, Live2DRenderer, Live2DUserModel};

struct DemoOptions {
    model_paths: Vec<PathBuf>,
    ui_visible: bool,
}

/// One loaded model slot.
struct ModelSlot {
    /// Display name (filename stem).
    name: String,
    resource: Live2DModelResource,
    user_model: Live2DUserModel,
    clipping: Option<ClippingManager>,
    motion_groups: Vec<MotionGroupUi>,
    expression_names: Vec<String>,
}

struct MotionGroupUi {
    name: String,
    motions: Vec<MotionUi>,
}

struct MotionUi {
    index_in_group: usize,
    name: String,
}

fn print_usage() {
    eprintln!("Usage: live2d_demo [--no-ui] <model-or-folder> [more models/folders ...]");
    eprintln!("  --no-ui   start in pure render mode for FPS A/B");
    eprintln!("  --ui      force the control panel on at startup");
    eprintln!("  U         toggle the control panel while running");
    eprintln!("  You can pass one or more .model3.json files and/or folders.");
    eprintln!("  Folders are scanned recursively for every *.model3.json.");
    eprintln!("  All loaded models are rendered at the same time.");
    eprintln!(
        "  e.g. live2d_demo --no-ui CubismSdkForNative\\CubismSdkForNative-5-r.5\\Samples\\Resources"
    );
}

fn is_model_json(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".model3.json"))
}

fn collect_models_under_dir(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_models_under_dir(&path, out)?;
        } else if path.is_file() && is_model_json(&path) {
            out.push(path);
        }
    }

    Ok(())
}

fn expand_model_inputs(inputs: Vec<String>) -> Vec<PathBuf> {
    let mut model_paths = Vec::new();

    for input in inputs {
        let path = PathBuf::from(&input);
        if path.is_dir() {
            if let Err(err) = collect_models_under_dir(&path, &mut model_paths) {
                eprintln!(
                    "[Live2D] Failed to scan directory {}: {err}",
                    path.display()
                );
            }
        } else if path.is_file() {
            if is_model_json(&path) {
                model_paths.push(path);
            } else {
                eprintln!("[Live2D] Ignoring non-model file: {}", path.display());
            }
        } else {
            eprintln!("[Live2D] Path not found: {}", path.display());
        }
    }

    model_paths.sort();
    model_paths.dedup();
    model_paths
}

fn parse_args() -> DemoOptions {
    let mut inputs = Vec::new();
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
            _ => inputs.push(arg),
        }
    }

    let model_paths = expand_model_inputs(inputs);

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
                eprintln!("[Live2D] Loading model: {}", path.display());
                match Live2DModelResource::load(ctx.gpu(), path) {
                    Ok(resource) => {
                        let user_model = match resource.instantiate() {
                            Ok(user_model) => user_model,
                            Err(err) => {
                                eprintln!(
                                    "[Live2D] Failed to instantiate {}: {err}",
                                    path.display()
                                );
                                continue;
                            }
                        };
                        eprintln!(
                            "[Live2D] Loaded: {} drawables, {} textures",
                            user_model.model().drawable_count(),
                            resource.texture_count(),
                        );

                        let clip_mgr = ClippingManager::new(user_model.model());
                        let clipping = if clip_mgr.has_masks() {
                            Some(clip_mgr)
                        } else {
                            None
                        };

                        let expression_names: Vec<String> = user_model
                            .expression_player()
                            .map(|p| p.expression_names().map(|n| n.to_string()).collect())
                            .unwrap_or_default();
                        let motion_groups: Vec<MotionGroupUi> = user_model
                            .motion_player()
                            .map(|player| {
                                let mut groups = Vec::<MotionGroupUi>::new();
                                for entry in player.motion_entries() {
                                    if groups
                                        .last()
                                        .is_none_or(|group| group.name != entry.group_name)
                                    {
                                        groups.push(MotionGroupUi {
                                            name: entry.group_name.to_string(),
                                            motions: Vec::new(),
                                        });
                                    }
                                    groups.last_mut().expect("group should exist").motions.push(
                                        MotionUi {
                                            index_in_group: entry.index_in_group,
                                            name: entry.motion_name.to_string(),
                                        },
                                    );
                                }
                                groups
                            })
                            .unwrap_or_default();

                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Unknown")
                            .trim_end_matches(".model3")
                            .to_string();

                        slots.push(ModelSlot {
                            name,
                            resource,
                            user_model,
                            clipping,
                            motion_groups,
                            expression_names,
                        });
                    }
                    Err(e) => {
                        eprintln!("[Live2D] Failed to load {}: {e}", path.display());
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
                "[Live2D] Loaded {} model(s); UI: {} (press U to toggle)",
                slots.len(),
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

        // UI output: which motion / expression was clicked (if any)
        let mut clicked_motion: Option<(String, usize, String)> = None;
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

                        ui.label(format!("Loaded: {}", slots.len()));
                        if slots.len() > 1 {
                            ui.small("Only the selected model is rendered.");
                            ui.add_space(4.0);
                            ui.label("Focus");
                            for (i, slot) in slots.iter().enumerate() {
                                ui.radio_value(&mut active, i, &slot.name);
                            }
                            ui.separator();
                        }

                        let active_slot = &slots[active];

                        // Active model name
                        ui.strong(&active_slot.name);
                        ui.add_space(4.0);

                        if !active_slot.motion_groups.is_empty() {
                            ui.label("Motions");
                            egui::ScrollArea::vertical()
                                .max_height(180.0)
                                .show(ui, |ui| {
                                    for group in &active_slot.motion_groups {
                                        ui.collapsing(group.name.as_str(), |ui| {
                                            for motion in &group.motions {
                                                if ui.button(&motion.name).clicked() {
                                                    clicked_motion = Some((
                                                        group.name.clone(),
                                                        motion.index_in_group,
                                                        motion.name.clone(),
                                                    ));
                                                }
                                            }
                                        });
                                        ui.add_space(4.0);
                                    }
                                });
                            ui.separator();
                        } else {
                            ui.weak("(no motions)");
                            ui.add_space(6.0);
                        }

                        // Expression buttons
                        if !active_slot.expression_names.is_empty() {
                            ui.label("Expressions");
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
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

        // ── Apply motion change (deferred from UI) ─────────────────
        if let Some((group_name, motion_index, motion_name)) = clicked_motion {
            if slots[active]
                .user_model
                .set_motion(&group_name, motion_index)
            {
                eprintln!("[Live2D] Motion -> {group_name}/{motion_name}");
            }
        }

        // ── Apply expression change (deferred from UI) ─────────────
        if let Some(expr_index) = clicked_expression {
            let expr = slots[active].expression_names[expr_index].clone();
            if slots[active].user_model.set_expression(&expr) {
                eprintln!("[Live2D] Expression -> {expr}");
            }
        }

        // ── Pointer-driven drag / tap behavior ────────────────────
        let surface_size = ctx.surface_size();
        let mouse_position = ctx.input.mouse_position();
        let in_bounds = mouse_position[0] >= 0.0
            && mouse_position[1] >= 0.0
            && mouse_position[0] < surface_size[0] as f32
            && mouse_position[1] < surface_size[1] as f32;
        if in_bounds {
            let drag_x = mouse_position[0] / surface_size[0].max(1) as f32 * 2.0 - 1.0;
            let drag_y = 1.0 - mouse_position[1] / surface_size[1].max(1) as f32 * 2.0;
            let _ = slots[active].user_model.set_drag(drag_x, drag_y);

            if ctx.input.mouse_left_released()
                && slots[active]
                    .user_model
                    .handle_tap_screen(mouse_position, surface_size)
            {
                eprintln!("[Live2D] Tap action fired");
            }
        } else {
            let _ = slots[active].user_model.clear_drag();
        }

        // ── Update active model ────────────────────────────────────
        slots[active].user_model.update(dt);
        for event in slots[active].user_model.take_started_motions() {
            eprintln!(
                "[Live2D] Motion started -> #{}, {}/{} ({:?})",
                event.handle, event.group_name, event.motion_name, event.priority
            );
        }
        for event in slots[active].user_model.take_finished_motions() {
            eprintln!(
                "[Live2D] Motion finished -> #{}, {}/{} ({:?})",
                event.handle, event.group_name, event.motion_name, event.priority
            );
        }
        for sound_path in slots[active].user_model.take_started_motion_sounds() {
            eprintln!("[Live2D] Motion sound -> {}", sound_path);
        }
        for event in slots[active].user_model.take_motion_events() {
            eprintln!(
                "[Live2D] Motion event -> {}/{} @ {:.3}s: {}",
                event.group_name, event.motion_name, event.time_seconds, event.value
            );
        }

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

        let slot = &mut slots[active];
        if let Some(ref mut rend) = l2d_renderer {
            rend.draw_to_surface(
                gpu,
                slot.user_model.model(),
                slot.resource.textures(),
                &mut slot.clipping,
            );
        }

        // Title
        if active != last_titled_active
            || fps_display != last_titled_fps
            || ui_visible != last_titled_ui_visible
        {
            ctx.set_title(&format!(
                "SkyEngine — Live2D | {} model(s) | Focus: {} | {:.0} FPS | {}",
                slots.len(),
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
