//! # Live2D Demo
//!
//! Render-feature integration example for loading and rendering Cubism models
//! on top of the default ECS 2D render path, with an egui control panel for
//! model switching and expression selection.
//!
//! ```bash
//! cargo run --example live2d_demo --features "live2d egui" --release -- [--no-ui] <model1.model3.json> [model2.model3.json ...]
//! cargo run --example live2d_demo --features "live2d egui" --release -- [--no-ui] <folder-with-models>
//! cargo run --example live2d_demo --features "live2d egui" --release -- --no-ui assets/Haru/Haru.model3.json
//! ```

use std::path::{Path, PathBuf};

use sky_engine::app::{egui, App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::{EntityId, World};
use sky_engine::input::KeyCode;
use sky_engine::render::expert::live2d::{Live2DLoadError, Live2DUserModel};
use sky_engine::render::{
    CameraMarker, Color, Live2DFeature, Live2DModelInstance, MainCamera, Projection,
    RenderPipelineAsset, RenderSettings, SortingLayer, SpriteFeature, SpriteRenderer, Transform,
};

const DEFAULT_MODEL_PATH: &str =
    "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.model3.json";

struct DemoOptions {
    model_paths: Vec<PathBuf>,
    ui_visible: bool,
}

/// One loaded model slot.
struct ModelSlot {
    entity: EntityId,
    /// Display name (filename stem).
    name: String,
    motion_groups: Vec<MotionGroupUi>,
    expression_names: Vec<String>,
}

impl ModelSlot {
    fn from_user_model(entity: EntityId, path: &Path, user_model: &Live2DUserModel) -> Self {
        Self {
            entity,
            name: model_display_name(path),
            motion_groups: MotionGroupUi::from_user_model(user_model),
            expression_names: expression_names_from_model(user_model),
        }
    }
}

struct MotionGroupUi {
    name: String,
    motions: Vec<MotionUi>,
}

impl MotionGroupUi {
    fn from_user_model(user_model: &Live2DUserModel) -> Vec<Self> {
        user_model
            .motion_player()
            .map(|player| {
                let mut groups = Vec::<Self>::new();
                for entry in player.motion_entries() {
                    if groups
                        .last()
                        .is_none_or(|group| group.name != entry.group_name)
                    {
                        groups.push(Self {
                            name: entry.group_name.to_string(),
                            motions: Vec::new(),
                        });
                    }
                    groups
                        .last_mut()
                        .expect("motion group should exist")
                        .motions
                        .push(MotionUi::new(entry.index_in_group, entry.motion_name));
                }
                groups
            })
            .unwrap_or_default()
    }
}

struct MotionUi {
    index_in_group: usize,
    name: String,
}

impl MotionUi {
    fn new(index_in_group: usize, name: &str) -> Self {
        Self {
            index_in_group,
            name: name.to_string(),
        }
    }
}

#[derive(Default)]
struct PendingUiActions {
    clicked_motion: Option<(String, usize, String)>,
    clicked_expression: Option<usize>,
}

#[derive(Default)]
struct FpsCounter {
    accum_seconds: f32,
    frames: u32,
    display: f32,
}

impl FpsCounter {
    fn update(&mut self, dt: f32) {
        self.accum_seconds += dt;
        self.frames += 1;
        if self.accum_seconds >= 0.5 {
            self.display = self.frames as f32 / self.accum_seconds;
            self.accum_seconds = 0.0;
            self.frames = 0;
        }
    }

    fn display(&self) -> f32 {
        self.display
    }
}

struct Live2DDemoApp {
    model_paths: Vec<PathBuf>,
    slots: Vec<ModelSlot>,
    active: usize,
    ui_visible: bool,
    fps: FpsCounter,
    last_titled_active: usize,
    last_titled_fps: f32,
    last_titled_ui_visible: bool,
    should_exit: bool,
}

impl Live2DDemoApp {
    fn new(model_paths: Vec<PathBuf>, ui_visible: bool) -> Self {
        Self {
            model_paths,
            slots: Vec::new(),
            active: 0,
            ui_visible,
            fps: FpsCounter::default(),
            last_titled_active: usize::MAX,
            last_titled_fps: -1.0,
            last_titled_ui_visible: !ui_visible,
            should_exit: false,
        }
    }

    fn active_slot(&self) -> &ModelSlot {
        &self.slots[self.active]
    }

    fn active_entity(&self) -> EntityId {
        self.active_slot().entity
    }

    fn load_models(&mut self, ctx: &mut FrameContext<'_>) {
        let model_paths = std::mem::take(&mut self.model_paths);

        for path in &model_paths {
            eprintln!("[Live2D] Loading model: {}", path.display());
            let entity = ctx.world.spawn((
                Transform::default().with_scale(180.0, 180.0),
                SortingLayer(1),
                Live2DModelInstance::new(path.clone()).visible(false),
            ));
            let instance = ctx
                .world
                .get::<Live2DModelInstance>(entity)
                .expect("spawned Live2DModelInstance should be queryable")
                .clone();
            let loaded = ctx
                .with_feature_mut::<Live2DFeature, _>(|feature, gpu| {
                    let index = feature.ensure_entity_loaded(entity, &instance, gpu)?;
                    let user_model = feature
                        .user_model_for_entity(entity)
                        .expect("loaded Live2D model should be queryable");
                    Ok::<_, Live2DLoadError>((
                        index,
                        ModelSlot::from_user_model(entity, path, user_model),
                    ))
                })
                .expect("live2d_demo requires an installed Live2DFeature");
            match loaded {
                Ok((index, slot)) => {
                    let drawable_count = ctx
                        .with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
                            feature
                                .model(index)
                                .map(|model| model.drawable_count())
                                .unwrap_or(0)
                        })
                        .unwrap_or(0);
                    eprintln!("[Live2D] Loaded: {drawable_count} drawables");
                    self.slots.push(slot);
                }
                Err(error) => eprintln!("[Live2D] {error}"),
            }
        }

        if self.slots.is_empty() {
            eprintln!("[Live2D] No models loaded, exiting.");
            self.should_exit = true;
            return;
        }

        self.apply_active_visibility(ctx);
        eprintln!(
            "[Live2D] Loaded {} model(s); UI: {} (press U to toggle)",
            self.slots.len(),
            if self.ui_visible { "on" } else { "off" }
        );
    }

    fn apply_active_visibility(&self, ctx: &mut FrameContext<'_>) {
        for (index, slot) in self.slots.iter().enumerate() {
            if let Some(instance) = ctx.world.get_mut::<Live2DModelInstance>(slot.entity) {
                instance.visible = index == self.active;
            }
        }
    }

    fn update_fps(&mut self, dt: f32) {
        self.fps.update(dt);
    }

    fn handle_global_input(&mut self, ctx: &FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::KeyU) {
            self.ui_visible = !self.ui_visible;
            eprintln!(
                "[Live2D] UI -> {}",
                if self.ui_visible { "on" } else { "off" }
            );
        }
    }

    fn draw_ui(&mut self, ctx: &mut FrameContext<'_>) -> PendingUiActions {
        let mut actions = PendingUiActions::default();
        if !self.ui_visible {
            return actions;
        }

        let fps_display = self.fps.display();
        let slots = &self.slots;
        let active = &mut self.active;

        ctx.egui(|egui_ctx| {
            egui::SidePanel::left("live2d_panel")
                .default_width(200.0)
                .resizable(true)
                .show(egui_ctx, |ui| {
                    ui.heading("🎭 Live2D");
                    ui.separator();

                    ui.label(format!("FPS: {fps_display:.0}"));
                    ui.label("U: toggle UI / pure render");
                    ui.separator();

                    ui.label(format!("Loaded: {}", slots.len()));
                    if slots.len() > 1 {
                        ui.small("Only the selected model is rendered.");
                        ui.add_space(4.0);
                        ui.label("Focus");
                        for (index, slot) in slots.iter().enumerate() {
                            ui.radio_value(active, index, &slot.name);
                        }
                        ui.separator();
                    }

                    let active_slot = &slots[*active];

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
                                                actions.clicked_motion = Some((
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

                    if !active_slot.expression_names.is_empty() {
                        ui.label("Expressions");
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for (index, name) in active_slot.expression_names.iter().enumerate()
                                {
                                    if ui.button(name).clicked() {
                                        actions.clicked_expression = Some(index);
                                    }
                                }
                            });
                    } else {
                        ui.weak("(no expressions)");
                    }
                });
        });

        actions
    }

    fn handle_pointer_interaction(&mut self, ctx: &mut FrameContext<'_>) {
        let surface_size = ctx.surface_size();
        let mouse_position = ctx.input.mouse_position();
        let in_bounds = ctx.input.mouse_in_window()
            && mouse_position[0] >= 0.0
            && mouse_position[1] >= 0.0
            && mouse_position[0] < surface_size[0] as f32
            && mouse_position[1] < surface_size[1] as f32;

        if in_bounds {
            let drag_x = mouse_position[0] / surface_size[0].max(1) as f32 * 2.0 - 1.0;
            let drag_y = 1.0 - mouse_position[1] / surface_size[1].max(1) as f32 * 2.0;
            let active = self.active_entity();
            let tapped = ctx
                .with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
                    let Some(slot) = feature.user_model_mut_for_entity(active) else {
                        return false;
                    };
                    let _ = slot.set_drag(drag_x, drag_y);
                    ctx.input.mouse_left_released()
                        && slot.handle_tap_screen(mouse_position, surface_size)
                })
                .unwrap_or(false);
            if tapped {
                eprintln!("[Live2D] Tap action fired");
            }
        } else {
            let active = self.active_entity();
            let _ = ctx.with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
                if let Some(slot) = feature.user_model_mut_for_entity(active) {
                    let _ = slot.clear_drag();
                }
            });
        }
    }

    fn update_active_model(&mut self, ctx: &mut FrameContext<'_>, dt: f32) {
        let active = self.active_entity();
        let _ = ctx.with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
            if let Some(slot) = feature.user_model_mut_for_entity(active) {
                slot.update(dt);
            }
        });
    }

    fn drain_runtime_events(&mut self, ctx: &mut FrameContext<'_>) {
        let active = self.active_entity();
        let _ = ctx.with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
            let Some(slot) = feature.user_model_mut_for_entity(active) else {
                return;
            };

            for event in slot.take_started_motions() {
                eprintln!(
                    "[Live2D] Motion started -> #{}, {}/{} ({:?})",
                    event.handle, event.group_name, event.motion_name, event.priority
                );
            }
            for event in slot.take_finished_motions() {
                if event.is_loop_cycle {
                    eprintln!(
                        "[Live2D] Motion loop completed -> #{}, {}/{} ({:?})",
                        event.handle, event.group_name, event.motion_name, event.priority
                    );
                } else {
                    eprintln!(
                        "[Live2D] Motion finished -> #{}, {}/{} ({:?})",
                        event.handle, event.group_name, event.motion_name, event.priority
                    );
                }
            }
            for sound_path in slot.take_started_motion_sounds() {
                eprintln!("[Live2D] Motion sound -> {}", sound_path);
            }
            for event in slot.take_motion_events() {
                eprintln!(
                    "[Live2D] Motion event -> {}/{} @ {:.3}s: {}",
                    event.group_name, event.motion_name, event.time_seconds, event.value
                );
            }
        });
    }

    fn render_active_model(&mut self, ctx: &mut FrameContext<'_>) {
        self.apply_active_visibility(ctx);
        if let Some(settings) = ctx.world.get_resource_mut::<RenderSettings>() {
            settings.clear_color = Color::new(0.12, 0.12, 0.18, 1.0);
            settings.bloom.enabled = false;
            settings.tonemap.enabled = false;
            settings.vignette.enabled = false;
        } else {
            ctx.world.insert_resource(RenderSettings {
                clear_color: Color::new(0.12, 0.12, 0.18, 1.0),
                bloom: sky_engine::render::BloomSettings {
                    enabled: false,
                    ..Default::default()
                },
                tonemap: sky_engine::render::ToneMapSettings {
                    enabled: false,
                    ..Default::default()
                },
                vignette: sky_engine::render::VignetteSettings {
                    enabled: false,
                    ..Default::default()
                },
                ..Default::default()
            });
        }
        ctx.render();
    }

    fn update_window_title(&mut self, ctx: &FrameContext<'_>) {
        let fps_display = self.fps.display();
        if self.active != self.last_titled_active
            || fps_display != self.last_titled_fps
            || self.ui_visible != self.last_titled_ui_visible
        {
            ctx.set_title(&format!(
                "SkyEngine — Live2D | {} model(s) | Focus: {} | {:.0} FPS | {}",
                self.slots.len(),
                self.active_slot().name,
                fps_display,
                if self.ui_visible { "UI" } else { "No UI" }
            ));
            self.last_titled_active = self.active;
            self.last_titled_fps = fps_display;
            self.last_titled_ui_visible = self.ui_visible;
        }
    }
}

impl AppState for Live2DDemoApp {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        if self.should_exit {
            ctx.request_exit();
            return;
        }

        if self.slots.is_empty() {
            self.load_models(ctx);
        }

        if self.slots.is_empty() {
            return;
        }

        self.update_fps(ctx.dt);
        self.handle_global_input(ctx);
        let actions = self.draw_ui(ctx);
        if let Some((group_name, motion_index, motion_name)) = actions.clicked_motion {
            let active = self.active_entity();
            let played = ctx
                .with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
                    feature
                        .user_model_mut_for_entity(active)
                        .is_some_and(|slot| slot.set_motion(&group_name, motion_index))
                })
                .unwrap_or(false);
            if played {
                eprintln!("[Live2D] Motion -> {group_name}/{motion_name}");
            }
        }
        if let Some(expression_index) = actions.clicked_expression {
            let expression_name = self.active_slot().expression_names[expression_index].clone();
            let active = self.active_entity();
            let played = ctx
                .with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
                    feature
                        .user_model_mut_for_entity(active)
                        .is_some_and(|slot| slot.set_expression(&expression_name))
                })
                .unwrap_or(false);
            if played {
                eprintln!("[Live2D] Expression -> {expression_name}");
            }
        }
        self.handle_pointer_interaction(ctx);
        self.update_active_model(ctx, ctx.dt);
        self.drain_runtime_events(ctx);
        self.render_active_model(ctx);
        self.update_window_title(ctx);
    }
}

fn print_usage() {
    eprintln!("Usage: live2d_demo [--no-ui] [model-or-folder] [more models/folders ...]");
    eprintln!("  --no-ui   start in pure render mode for FPS A/B");
    eprintln!("  --ui      force the control panel on at startup");
    eprintln!("  U         toggle the control panel while running");
    eprintln!("  You can pass one or more .model3.json files and/or folders.");
    eprintln!("  If no path is provided, the demo uses the hardcoded default model:");
    eprintln!("    {}", DEFAULT_MODEL_PATH);
    eprintln!("  Folders are scanned recursively for every *.model3.json.");
    eprintln!("  All matching models are loaded; only the selected model is rendered.");
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

    let model_paths = if inputs.is_empty() {
        let default_path = PathBuf::from(DEFAULT_MODEL_PATH);
        eprintln!(
            "[Live2D] No model path provided; using default: {}",
            default_path.display()
        );
        vec![default_path]
    } else {
        let model_paths = expand_model_inputs(inputs);
        if model_paths.is_empty() {
            print_usage();
            std::process::exit(1);
        }
        model_paths
    };

    DemoOptions {
        model_paths,
        ui_visible,
    }
}

fn expression_names_from_model(user_model: &Live2DUserModel) -> Vec<String> {
    user_model
        .expression_player()
        .map(|player| {
            player
                .expression_names()
                .map(|name| name.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn model_display_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Unknown")
        .trim_end_matches(".model3")
        .to_string()
}

fn main() {
    let DemoOptions {
        model_paths,
        ui_visible,
    } = parse_args();

    let config = AppConfig::new("SkyEngine — Live2D", 1280, 720)
        .with_vsync(false)
        .with_resizable(true);
    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: Color::new(0.12, 0.12, 0.18, 1.0),
        bloom: sky_engine::render::BloomSettings {
            enabled: false,
            ..Default::default()
        },
        tonemap: sky_engine::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        vignette: sky_engine::render::VignetteSettings {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    });
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic(1280.0, 720.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -5.0),
        SpriteRenderer::new(900.0, 540.0).color(Color::new(0.18, 0.2, 0.28, 1.0)),
    ));
    world.spawn((
        Transform::from_xyz(-220.0, 120.0, -4.0),
        SpriteRenderer::new(180.0, 180.0).color(Color::new(0.3, 0.2, 0.42, 0.35)),
    ));
    world.spawn((
        Transform::from_xyz(240.0, -80.0, -4.0),
        SpriteRenderer::new(240.0, 240.0).color(Color::new(0.16, 0.4, 0.46, 0.28)),
    ));

    let pipeline = RenderPipelineAsset::builder()
        .add_feature(SpriteFeature::unlit())
        .add_feature(Live2DFeature::new())
        .add_phase(sky_engine::render::expert::TransparentPhase::new())
        .build();

    App::new(config, world)
        .with_render_pipeline(pipeline)
        .run(Live2DDemoApp::new(model_paths, ui_visible));
}
