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

#[path = "live2d_demo/args.rs"]
mod args;
#[path = "live2d_demo/benchmark.rs"]
mod benchmark;
#[path = "live2d_demo/model.rs"]
mod model;
#[path = "live2d_demo/ui.rs"]
mod ui;

use std::path::PathBuf;

use args::{parse_args, DemoOptions};
use benchmark::{BenchmarkConfig, BenchmarkState};
use model::ModelSlot;
use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, WindowPlugin,
};
use sky_engine::ecs::{EntityId, With, World};
use sky_engine::input::KeyCode;
use sky_engine::math::{LogicalPoint, LogicalSize, PhysicalSize, Vec3};
use sky_engine::render::expert::live2d::Live2DLoadError;
use sky_engine::render::{
    CameraMarker, Color, Live2DAnimator, Live2DCommands, Live2DFeature, Live2DLookTarget,
    Live2DModelInstance, Live2DModelPoint, MainCamera, Projection, RenderPipelineAsset,
    RenderSettings, SortingLayer, SpriteRenderer, Transform,
};

const DEMO_CAMERA_HEIGHT: f32 = 720.0;
const DEMO_MODEL_HEIGHT: f32 = 360.0;

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
    active_dirty: bool,
    last_titled_active: usize,
    last_titled_fps_rounded: i32,
    last_titled_ui_visible: bool,
    benchmark: Option<BenchmarkState>,
    pointer_drag_active: bool,
    debug_look: bool,
    frame_index: u64,
    should_exit: bool,
}

impl Live2DDemoApp {
    fn new(
        model_paths: Vec<PathBuf>,
        ui_visible: bool,
        benchmark: Option<BenchmarkConfig>,
        debug_look: bool,
    ) -> Self {
        Self {
            model_paths,
            slots: Vec::new(),
            active: 0,
            ui_visible,
            fps: FpsCounter::default(),
            active_dirty: true,
            last_titled_active: usize::MAX,
            last_titled_fps_rounded: -1,
            last_titled_ui_visible: !ui_visible,
            benchmark: benchmark.map(BenchmarkState::new),
            pointer_drag_active: false,
            debug_look,
            frame_index: 0,
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
                Transform::default(),
                SortingLayer(1),
                Live2DModelInstance::new(path.clone())
                    .with_height(DEMO_MODEL_HEIGHT)
                    .visible(false),
                Live2DAnimator::default(),
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

        self.active_dirty = true;
        self.sync_active_model_state(ctx);
        eprintln!(
            "[Live2D] Loaded {} model(s); UI: {} (press U to toggle)",
            self.slots.len(),
            if self.ui_visible { "on" } else { "off" }
        );
    }

    fn sync_active_model_state(&mut self, ctx: &mut FrameContext<'_>) {
        if !self.active_dirty {
            return;
        }

        for (index, slot) in self.slots.iter().enumerate() {
            if let Some(instance) = ctx.world.get_mut::<Live2DModelInstance>(slot.entity) {
                instance.visible = index == self.active;
            }
        }

        let active = self.active;
        let _ = ctx.with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
            feature.set_active_only(active);
        });

        self.active_dirty = false;
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

    fn draw_ui(&mut self, ctx: &mut FrameContext<'_>) -> ui::PendingUiActions {
        let mut actions = ui::PendingUiActions::default();
        if !self.ui_visible {
            return actions;
        }

        let fps_display = self.fps.display();
        let slots = &self.slots;
        let active = &mut self.active;
        let benchmark = self.benchmark.as_ref().map(BenchmarkState::config);

        ctx.egui(|root_ui| {
            actions = ui::draw_live2d_panel(root_ui, slots, active, fps_display, benchmark);
        });

        actions
    }

    fn handle_pointer_interaction(&mut self, ctx: &mut FrameContext<'_>) {
        if self.ui_visible && ctx.egui_wants_pointer() {
            if self.debug_look {
                let mouse_position = ctx.input.mouse_logical_position();
                let physical_surface_size = ctx.physical_surface_size();
                let logical_view_size = ctx.logical_view_size();
                eprintln!(
                    "[Live2D][look][frame={}] pointer captured by egui; drag not sent. mouse_logical=({:.2}, {:.2}) logical_view=({:.2}, {:.2}) physical_surface=({}, {})",
                    self.frame_index,
                    mouse_position.x,
                    mouse_position.y,
                    logical_view_size.width,
                    logical_view_size.height,
                    physical_surface_size.width,
                    physical_surface_size.height
                );
            }
            if self.pointer_drag_active {
                let active = self.active_entity();
                Live2DCommands::resource(ctx.world).clear_look_target(active);
                self.pointer_drag_active = false;
            }
            return;
        }

        let physical_surface_size = ctx.physical_surface_size();
        let logical_view_size = ctx.logical_view_size();
        let mouse_position = ctx.input.mouse_logical_position();
        let in_bounds = ctx.input.mouse_in_window() && logical_view_size.contains(mouse_position);

        if in_bounds {
            let active = self.active_entity();
            let model_transform = ctx
                .world
                .get::<Transform>(active)
                .copied()
                .unwrap_or_default();
            let model_height = ctx
                .world
                .get::<Live2DModelInstance>(active)
                .and_then(|instance| instance.height)
                .unwrap_or(DEMO_MODEL_HEIGHT);
            let (camera_transform, camera_projection) = first_main_camera(ctx.world)
                .unwrap_or_else(|| {
                    (
                        Transform::default(),
                        Projection::orthographic(logical_view_size.height.max(1.0)),
                    )
                });
            let trace = pointer_to_live2d_model_view(
                mouse_position,
                logical_view_size,
                physical_surface_size,
                camera_transform,
                camera_projection,
                model_transform,
                model_height,
            );
            if self.debug_look {
                print_look_coordinate_trace(self.frame_index, &trace);
            }
            let tapped = ctx.input.mouse_left_released();
            let commands = Live2DCommands::resource(ctx.world);
            commands.set_look_target(active, trace.look_target);
            if tapped {
                commands.tap_screen(active, mouse_position, logical_view_size);
            }
            self.pointer_drag_active = true;
            if tapped {
                eprintln!("[Live2D] Tap action requested");
            }
        } else if self.pointer_drag_active {
            let active = self.active_entity();
            Live2DCommands::resource(ctx.world).clear_look_target(active);
            self.pointer_drag_active = false;
        } else if self.debug_look {
            eprintln!(
                "[Live2D][look][frame={}] mouse out of window: mouse_logical=({:.2}, {:.2}) logical_view=({:.2}, {:.2}) physical_surface=({}, {})",
                self.frame_index,
                mouse_position.x,
                mouse_position.y,
                logical_view_size.width,
                logical_view_size.height,
                physical_surface_size.width,
                physical_surface_size.height
            );
        }
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
        self.sync_active_model_state(ctx);
        ctx.render();
    }

    fn debug_runtime_look(&mut self, ctx: &mut FrameContext<'_>) {
        if !self.debug_look {
            return;
        }

        let active = self.active_entity();
        let frame_index = self.frame_index;
        let _ = ctx.with_feature_mut::<Live2DFeature, _>(|feature, _gpu| {
            let Some(user_model) = feature.user_model_for_entity(active) else {
                eprintln!(
                    "[Live2D][look][frame={frame_index}] runtime: active entity has no user model"
                );
                return;
            };

            if let Some(state) = user_model.look_debug_state() {
                eprintln!(
                    "[Live2D][look][frame={frame_index}] runtime target=({:.4}, {:.4}) current=({:.4}, {:.4}) velocity=({:.4}, {:.4}) time={:.4}/{:.4}",
                    state.target[0],
                    state.target[1],
                    state.current[0],
                    state.current[1],
                    state.velocity[0],
                    state.velocity[1],
                    state.last_time_seconds,
                    state.user_time_seconds
                );
            } else {
                eprintln!("[Live2D][look][frame={frame_index}] runtime: model has no look controller");
            }

            let model = user_model.model();
            for id in [
                "ParamAngleX",
                "ParamAngleY",
                "ParamAngleZ",
                "ParamBodyAngleX",
                "ParamEyeBallX",
                "ParamEyeBallY",
            ] {
                if let Some(index) = model.find_parameter(id) {
                    if index < model.parameter_count() {
                        eprintln!(
                            "[Live2D][look][frame={frame_index}] param {id}: value={:.4} default={:.4} min={:.4} max={:.4}",
                            model.parameter_value(index),
                            model.parameter_defaults()[index],
                            model.parameter_minimum(index),
                            model.parameter_maximum(index)
                        );
                    } else {
                        eprintln!(
                            "[Live2D][look][frame={frame_index}] param {id}: virtual value={:.4}",
                            model.parameter_value(index)
                        );
                    }
                } else {
                    eprintln!("[Live2D][look][frame={frame_index}] param {id}: missing");
                }
            }
        });
    }

    fn update_window_title(&mut self, ctx: &FrameContext<'_>) {
        if self.benchmark.is_some() {
            return;
        }

        let fps_rounded = self.fps.display().round() as i32;
        if self.active != self.last_titled_active
            || fps_rounded != self.last_titled_fps_rounded
            || self.ui_visible != self.last_titled_ui_visible
        {
            ctx.set_title(&format!(
                "SkyEngine — Live2D | {} model(s) | Focus: {} | {:.0} FPS | {}",
                self.slots.len(),
                self.active_slot().name,
                fps_rounded,
                if self.ui_visible { "UI" } else { "No UI" }
            ));
            self.last_titled_active = self.active;
            self.last_titled_fps_rounded = fps_rounded;
            self.last_titled_ui_visible = self.ui_visible;
        }
    }
}

impl AppState for Live2DDemoApp {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.frame_index = self.frame_index.wrapping_add(1);

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

        let dt = ctx.time().frame_delta;
        self.update_fps(dt);
        self.handle_global_input(ctx);
        let previous_active = self.active;
        let actions = self.draw_ui(ctx);
        if previous_active != self.active {
            self.active_dirty = true;
        }
        if actions.hide_panel {
            self.ui_visible = false;
        }
        if actions.clear_look_target {
            let active = self.active_entity();
            Live2DCommands::resource(ctx.world).clear_look_target(active);
            self.pointer_drag_active = false;
        }
        if let Some((group_index, motion_index)) = actions.clicked_motion {
            let active_slot = self.active_slot();
            let group = &active_slot.motion_groups[group_index];
            let motion = &group.motions[motion_index];
            let active = self.active_entity();
            Live2DCommands::resource(ctx.world).play_motion(
                active,
                group.name.clone(),
                motion.index_in_group,
            );
            eprintln!(
                "[Live2D] Motion requested -> {}/{}",
                group.name, motion.name
            );
        }
        if let Some(expression_index) = actions.clicked_expression {
            let expression_name = self.active_slot().expression_names[expression_index].clone();
            let active = self.active_entity();
            Live2DCommands::resource(ctx.world).set_expression(active, expression_name.clone());
            eprintln!("[Live2D] Expression requested -> {expression_name}");
        }
        self.handle_pointer_interaction(ctx);
        self.render_active_model(ctx);
        self.debug_runtime_look(ctx);
        self.drain_runtime_events(ctx);
        self.update_window_title(ctx);
        if let Some(result) = self
            .benchmark
            .as_mut()
            .and_then(|benchmark| benchmark.record_frame(dt))
        {
            eprintln!(
                "[Live2D][bench] sample_frames={} avg_fps={:.2} avg_frame_ms={:.4}",
                result.sample_frames,
                result.fps(),
                result.frame_ms()
            );
            ctx.request_exit();
        }
    }
}

fn first_main_camera(world: &mut World) -> Option<(Transform, Projection)> {
    let query = world
        .query::<(&Transform, &Projection)>()
        .filter::<With<MainCamera>>();
    let mut result = None;
    query.for_each(|(transform, projection)| {
        if result.is_none() {
            result = Some((*transform, *projection));
        }
    });
    result
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LookCoordinateTrace {
    mouse_logical: LogicalPoint,
    logical_view_size: LogicalSize,
    physical_surface_size: PhysicalSize,
    camera_transform: Transform,
    camera_projection: Projection,
    world: [f32; 2],
    model_transform: Transform,
    model_local: Live2DModelPoint,
    model_height: f32,
    half_model_height: f32,
    look_target: Live2DLookTarget,
}

fn pointer_to_live2d_model_view(
    pointer: LogicalPoint,
    logical_view_size: LogicalSize,
    physical_surface_size: PhysicalSize,
    camera_transform: Transform,
    camera_projection: Projection,
    model_transform: Transform,
    model_height: f32,
) -> LookCoordinateTrace {
    let logical_view_size = LogicalSize::new(
        logical_view_size.width.max(1.0),
        logical_view_size.height.max(1.0),
    );
    let world =
        camera_projection.screen_to_world_logical(camera_transform, logical_view_size, pointer);
    let model_local = model_transform
        .to_matrix4()
        .inverse()
        .transform_point3(Vec3::new(world.x(), world.y(), 0.0));
    let half_model_height = (model_height.abs() * 0.5).max(f32::EPSILON);
    let model_local = Live2DModelPoint::new(model_local.x(), model_local.y());

    LookCoordinateTrace {
        mouse_logical: pointer,
        logical_view_size,
        physical_surface_size,
        camera_transform,
        camera_projection,
        world: [world.x(), world.y()],
        model_transform,
        model_local,
        model_height,
        half_model_height,
        look_target: Live2DLookTarget::from_model_point(model_local, model_height),
    }
}

fn print_look_coordinate_trace(frame_index: u64, trace: &LookCoordinateTrace) {
    eprintln!(
        "[Live2D][look][frame={frame_index}] mouse_logical=({:.2}, {:.2}) logical_view=({:.2}, {:.2}) physical_surface=({}, {})",
        trace.mouse_logical.x,
        trace.mouse_logical.y,
        trace.logical_view_size.width,
        trace.logical_view_size.height,
        trace.physical_surface_size.width,
        trace.physical_surface_size.height
    );
    eprintln!(
        "[Live2D][look][frame={frame_index}] camera pos=({:.2}, {:.2}, {:.2}) scale=({:.2}, {:.2}, {:.2}) projection={:?}",
        trace.camera_transform.position[0],
        trace.camera_transform.position[1],
        trace.camera_transform.position[2],
        trace.camera_transform.scale[0],
        trace.camera_transform.scale[1],
        trace.camera_transform.scale[2],
        trace.camera_projection
    );
    eprintln!(
        "[Live2D][look][frame={frame_index}] screen_to_world=({:.4}, {:.4})",
        trace.world[0], trace.world[1]
    );
    eprintln!(
        "[Live2D][look][frame={frame_index}] model pos=({:.2}, {:.2}, {:.2}) scale=({:.2}, {:.2}, {:.2}) height={:.4} half_height={:.4}",
        trace.model_transform.position[0],
        trace.model_transform.position[1],
        trace.model_transform.position[2],
        trace.model_transform.scale[0],
        trace.model_transform.scale[1],
        trace.model_transform.scale[2],
        trace.model_height,
        trace.half_model_height
    );
    eprintln!(
        "[Live2D][look][frame={frame_index}] world_to_model_local=({:.4}, {:.4})",
        trace.model_local.x, trace.model_local.y
    );
    eprintln!(
        "[Live2D][look][frame={frame_index}] look_target=({:.4}, {:.4})  // x>0 means mouse is on model's local right",
        trace.look_target.x, trace.look_target.y
    );
}

fn main() {
    let DemoOptions {
        model_paths,
        ui_visible,
        benchmark,
        debug_look,
    } = parse_args();

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
        Projection::orthographic(DEMO_CAMERA_HEIGHT),
        MainCamera,
    ));
    if ui_visible && benchmark.is_none() {
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
    }

    world
        .install(
            WindowPlugin::new("SkyEngine — Live2D", 1280, 720)
                .with_vsync(false)
                .with_resizable(true),
        )
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(RenderPipelineAsset::live2d_2d()))
        .unwrap();

    App::new(world).run(Live2DDemoApp::new(
        model_paths,
        ui_visible,
        benchmark,
        debug_look,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: [f32; 2], expected: [f32; 2]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 0.0001);
        }
    }

    #[test]
    fn pointer_to_live2d_model_view_uses_camera_and_model_height() {
        let camera_transform = Transform::default();
        let camera_projection = Projection::orthographic(DEMO_CAMERA_HEIGHT);
        let model_transform = Transform::default();

        assert_close(
            pointer_to_live2d_model_view(
                LogicalPoint::new(640.0, 360.0),
                LogicalSize::new(1280.0, 720.0),
                PhysicalSize::new(1280, 720),
                camera_transform,
                camera_projection,
                model_transform,
                DEMO_MODEL_HEIGHT,
            )
            .look_target
            .to_array(),
            [0.0, 0.0],
        );
        assert_close(
            pointer_to_live2d_model_view(
                LogicalPoint::new(640.0, 180.0),
                LogicalSize::new(1280.0, 720.0),
                PhysicalSize::new(1280, 720),
                camera_transform,
                camera_projection,
                model_transform,
                DEMO_MODEL_HEIGHT,
            )
            .look_target
            .to_array(),
            [0.0, 1.0],
        );
    }

    #[test]
    fn pointer_to_live2d_model_view_uses_logical_input_space_on_hidpi_surfaces() {
        let trace = pointer_to_live2d_model_view(
            LogicalPoint::new(1024.0, 360.0),
            LogicalSize::new(1280.0, 720.0),
            PhysicalSize::new(3840, 2160),
            Transform::default(),
            Projection::orthographic(DEMO_CAMERA_HEIGHT),
            Transform::default(),
            DEMO_MODEL_HEIGHT,
        );

        assert_close(trace.world, [384.0, 0.0]);
        assert_close(trace.look_target.to_array(), [384.0 / 180.0, 0.0]);
    }

    #[test]
    fn pointer_to_live2d_model_view_tracks_transformed_model_origin() {
        let camera_transform = Transform::default();
        let camera_projection = Projection::orthographic(DEMO_CAMERA_HEIGHT);
        let model_transform = Transform::from_xy(100.0, -50.0);

        assert_close(
            pointer_to_live2d_model_view(
                LogicalPoint::new(740.0, 230.0),
                LogicalSize::new(1280.0, 720.0),
                PhysicalSize::new(1280, 720),
                camera_transform,
                camera_projection,
                model_transform,
                DEMO_MODEL_HEIGHT,
            )
            .look_target
            .to_array(),
            [0.0, 1.0],
        );
    }
}
