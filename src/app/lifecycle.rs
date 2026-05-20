//! winit lifecycle integration for the app runner.

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::config::{RedrawMode, RunnerOptions, WindowOptions, WindowSizeMode};
use crate::app::frame::FrameContext;
use crate::app::plugins::InputEnabled;
#[cfg(feature = "video")]
use crate::app::plugins::VideoEnabled;
use crate::app::runner::{AppState, SetupContext};
use crate::app::windows::{WindowRequest, WindowRuntime};
use crate::ecs::World;
use crate::input::raw::{Input, KeyCode};
use crate::logging::{self, LogOptions, LogStore};
use crate::render::backend::{create_scene_renderer, SceneRendererError};
use crate::render::{RenderBackendKind, RenderPipelineAsset, SceneRenderer};

struct RuntimeState {
    window: Arc<Window>,
    renderer: Box<dyn SceneRenderer>,
    input: Input,
    last_frame_time: Option<Instant>,
    occluded: bool,
    aux_windows: Vec<WindowRuntime>,
    #[cfg(feature = "egui")]
    egui: Option<crate::app::egui_integration::EguiIntegration>,
}

pub(in crate::app) struct RunnerHandler {
    window_options: Option<WindowOptions>,
    runner_options: RunnerOptions,
    world: Option<World>,
    pipeline: Option<RenderPipelineAsset>,
    asset_config: Option<crate::asset::AssetConfig>,
    input_enabled: bool,
    logs: Arc<LogStore>,
    #[cfg(feature = "audio")]
    audio_config: Option<crate::audio::AudioConfig>,
    #[cfg(feature = "video")]
    video_enabled: bool,
    app_state: Box<dyn AppState>,
    runtime: Option<RuntimeState>,
    pending_redraw: bool,
    kajiya_debug_frames: u64,
    did_shutdown: bool,
}

impl RunnerHandler {
    pub(in crate::app) fn new(mut world: World, app_state: Box<dyn AppState>) -> Self {
        let window_options = world.get_resource::<WindowOptions>().cloned();
        let runner_options = world
            .get_resource::<RunnerOptions>()
            .cloned()
            .unwrap_or_default();
        let asset_config = world.get_resource::<crate::asset::AssetConfig>().cloned();
        let input_enabled = world.contains_resource::<InputEnabled>();
        let log_options = world
            .get_resource::<LogOptions>()
            .copied()
            .unwrap_or_default();
        let logs = LogStore::shared(log_options.capacity);
        let _log_status = logging::try_install_logger(log_options);
        let pipeline = world.remove_resource::<RenderPipelineAsset>();
        #[cfg(feature = "audio")]
        let audio_config = world.get_resource::<crate::audio::AudioConfig>().cloned();
        #[cfg(feature = "video")]
        let video_enabled = world.contains_resource::<VideoEnabled>();
        Self {
            window_options,
            runner_options,
            world: Some(world),
            pipeline,
            asset_config,
            input_enabled,
            logs,
            #[cfg(feature = "audio")]
            audio_config,
            #[cfg(feature = "video")]
            video_enabled,
            app_state,
            runtime: None,
            pending_redraw: false,
            kajiya_debug_frames: 0,
            did_shutdown: false,
        }
    }

    fn request_redraw(&mut self) {
        self.pending_redraw = true;
    }

    fn can_draw(&self) -> bool {
        let Some(rt) = self.runtime.as_ref() else {
            return false;
        };
        if rt.occluded {
            return false;
        }
        let size = rt.window.inner_size();
        size.width > 0 && size.height > 0
    }

    fn shutdown_world(&mut self) {
        if self.did_shutdown {
            return;
        }

        if let Some(world) = self.world.as_mut() {
            self.app_state.shutdown(world);
            world.shutdown();
        }
        self.did_shutdown = true;
    }

    fn shutdown_and_exit(&mut self, event_loop: &ActiveEventLoop) {
        self.shutdown_world();
        event_loop.exit();
    }

    fn sync_input_resource(world: &mut World, input: Input) {
        if let Some(resource) = world.get_resource_mut::<Input>() {
            *resource = input;
        } else {
            world.insert_resource(input);
        }
        if let Some(interaction) = world.get_resource_mut::<crate::input::InteractionContext>() {
            interaction.begin_frame();
        } else {
            world.insert_resource(crate::input::InteractionContext::default());
        }
    }

    fn run_frame(&mut self, event_loop: &ActiveEventLoop) {
        if !self.can_draw() {
            return;
        }
        let trace_kajiya = self
            .runtime
            .as_ref()
            .is_some_and(|rt| rt.renderer.backend_kind() == RenderBackendKind::Kajiya);
        let trace_frame = self.kajiya_debug_frames;
        if trace_kajiya && should_trace_kajiya_runner_frame(trace_frame) {
            eprintln!(
                "[SkyEngine][App] run_frame begin frame={} pending_redraw={}",
                trace_frame, self.pending_redraw
            );
        }

        let now = Instant::now();
        let raw_dt = self
            .runtime
            .as_ref()
            .and_then(|rt| rt.last_frame_time)
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        let dt = raw_dt.min(self.runner_options.max_delta);

        let input_snapshot = self.runtime.as_ref().expect("runtime must exist").input;
        let auto_tick = self.runner_options.auto_tick;
        let exit_on_escape = self.runner_options.exit_on_escape;
        let mut request_redraw = false;
        let mut should_exit = false;
        let mut aux_window_requests = Vec::new();
        let logs = Arc::clone(&self.logs);

        {
            let (world_slot, runtime_slot, app_state, frame_rate_limit) = (
                &mut self.world,
                &mut self.runtime,
                &mut self.app_state,
                &mut self.runner_options.frame_rate_limit,
            );
            let world = world_slot.as_mut().expect("world must exist");
            let rt = runtime_slot.as_mut().expect("runtime must exist");
            rt.last_frame_time = Some(now);

            if self.input_enabled {
                Self::sync_input_resource(world, input_snapshot);
            }

            // Update action-based input system (if registered).
            if let Some(actions) = world.get_resource_mut::<crate::input::InputActions>() {
                actions.update(&input_snapshot);
            }

            crate::app::services::update_assets(world);

            if auto_tick {
                world.tick_with_frame_delta(dt, raw_dt);
            }
            let frame_dt = if auto_tick {
                world.time.frame_delta
            } else {
                dt
            };
            logs.set_frame(Some(world.time.frame_count));
            logging::set_logger_frame(Some(world.time.frame_count));

            crate::app::services::update_video(world, frame_dt);

            if exit_on_escape && input_snapshot.key_pressed(KeyCode::Escape) {
                rt.input.begin_frame();
                should_exit = true;
            } else {
                match rt.renderer.begin_frame() {
                    Ok(()) => {
                        let mut exit_requested = false;
                        let mut redraw_requested = false;
                        let mut screenshot_requests = Vec::new();

                        {
                            let ctx = &mut FrameContext {
                                world,
                                input: &input_snapshot,
                                dt: frame_dt,
                                renderer: rt.renderer.as_mut(),
                                window: &rt.window,
                                exit_requested: &mut exit_requested,
                                redraw_requested: &mut redraw_requested,
                                frame_rate_limit,
                                logs: logs.as_ref(),
                                aux_window_requests: &mut aux_window_requests,
                                screenshot_requests: &mut screenshot_requests,
                                #[cfg(feature = "egui")]
                                egui: &mut rt.egui,
                            };
                            app_state.update(ctx);
                        }

                        crate::app::services::update_audio_after_frame(world);

                        #[cfg(feature = "egui")]
                        {
                            if let (Some(egui), Some(gpu)) =
                                (rt.egui.as_mut(), rt.renderer.wgpu_mut())
                            {
                                let surface_view = gpu.surface_view().clone();
                                let device = gpu.device().clone();
                                let queue = gpu.queue().clone();
                                egui.end_frame(
                                    &device,
                                    &queue,
                                    gpu.encoder(),
                                    &surface_view,
                                    &rt.window,
                                );
                            }
                        }

                        crate::app::screenshots::save_requested(
                            rt.renderer.as_mut(),
                            &mut screenshot_requests,
                        );
                        rt.window.pre_present_notify();
                        rt.renderer.end_frame();
                        rt.input.begin_frame();

                        request_redraw = redraw_requested;
                        should_exit = exit_requested;
                    }
                    Err(SceneRendererError::Wgpu(crate::gpu::GpuError::SurfaceLost)) => {
                        rt.renderer.surface_lost();
                        let size = rt.window.inner_size();
                        rt.renderer.resize(size.width, size.height);
                        rt.last_frame_time = None;
                        request_redraw = true;
                    }
                    Err(SceneRendererError::Wgpu(crate::gpu::GpuError::Timeout)) => {
                        rt.last_frame_time = None;
                        request_redraw = true;
                    }
                    Err(SceneRendererError::Wgpu(crate::gpu::GpuError::OutOfMemory)) => {
                        should_exit = true;
                    }
                    Err(e) => {
                        eprintln!("[SkyEngine] Renderer error: {e}");
                        rt.last_frame_time = None;
                    }
                }
            }
        }

        logging::drain_logger(logs.as_ref());

        if request_redraw {
            self.request_redraw();
        }
        self.create_aux_windows(event_loop, aux_window_requests);
        self.prune_aux_windows();
        if trace_kajiya && should_trace_kajiya_runner_frame(trace_frame) {
            eprintln!(
                "[SkyEngine][App] run_frame end frame={} request_redraw={} should_exit={}",
                trace_frame, request_redraw, should_exit
            );
        }
        if trace_kajiya {
            self.kajiya_debug_frames = self.kajiya_debug_frames.wrapping_add(1);
        }
        if should_exit {
            self.shutdown_and_exit(event_loop);
        }
    }

    fn create_aux_windows(&mut self, event_loop: &ActiveEventLoop, requests: Vec<WindowRequest>) {
        if requests.is_empty() {
            return;
        }
        let vsync = self
            .window_options
            .as_ref()
            .map(|options| options.vsync)
            .unwrap_or(true);
        let Some(rt) = self.runtime.as_mut() else {
            return;
        };
        for request in requests {
            if let Some(aux_window) = WindowRuntime::open(event_loop, vsync, request) {
                aux_window.request_redraw();
                rt.aux_windows.push(aux_window);
            }
        }
        self.request_redraw();
    }

    fn prune_aux_windows(&mut self) {
        if let Some(rt) = self.runtime.as_mut() {
            rt.aux_windows.retain(|window| !window.close_requested());
        }
    }

    fn handle_aux_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> bool {
        let Some(rt) = self.runtime.as_mut() else {
            return false;
        };
        let Some(index) = rt
            .aux_windows
            .iter()
            .position(|window| window.id() == window_id)
        else {
            return false;
        };

        let only_aux_window = rt.aux_windows.len() == 1;
        let aux_window = &mut rt.aux_windows[index];
        aux_window.handle_event(event, self.runner_options.max_delta);
        let close_requested = aux_window.close_requested();
        if only_aux_window && close_requested && self.did_shutdown {
            event_loop.exit();
        }
        self.prune_aux_windows();
        true
    }

    fn is_aux_window(&self, window_id: WindowId) -> bool {
        self.runtime
            .as_ref()
            .is_some_and(|rt| rt.aux_windows.iter().any(|window| window.id() == window_id))
    }
}

impl ApplicationHandler for RunnerHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(rt) = self.runtime.as_mut() {
            rt.occluded = false;
            rt.last_frame_time = None;
            self.request_redraw();
            return;
        }

        // First resume — create window, GPU backend, and optional render pipeline.
        let Some(window_options) = self.window_options.clone() else {
            eprintln!(
                "[SkyEngine] Windowed App::run requires WindowPlugin. Install it with world.install(WindowPlugin::new(...))."
            );
            event_loop.exit();
            return;
        };
        let attrs = WindowAttributes::default()
            .with_title(&window_options.title)
            .with_inner_size(match window_options.size_mode {
                WindowSizeMode::Logical => winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                    window_options.width as f64,
                    window_options.height as f64,
                )),
                WindowSizeMode::Physical => winit::dpi::Size::Physical(
                    winit::dpi::PhysicalSize::new(window_options.width, window_options.height),
                ),
            })
            .with_resizable(window_options.resizable);

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        let pipeline = self.pipeline.take();
        let mut renderer =
            match create_scene_renderer(window.clone(), window_options.vsync, pipeline) {
                Ok(renderer) => renderer,
                Err(err) => {
                    eprintln!("[SkyEngine] Renderer initialization failed: {err}");
                    event_loop.exit();
                    return;
                }
            };

        let title = format!(
            "{} | {} ({})",
            window_options.title,
            renderer.adapter_name(),
            renderer.backend_name()
        );
        window.set_title(&title);

        let world = self.world.as_mut().expect("world must be present");

        let input = Input::new();
        if self.input_enabled {
            // Insert Input resource into the World (updated in-place each frame).
            world.insert_resource(input);
        }

        if let Some(asset_config) = self.asset_config.clone() {
            crate::app::services::install_assets(world, asset_config);
        }
        #[cfg(feature = "audio")]
        if let Some(audio_config) = self.audio_config.clone() {
            crate::app::services::install_audio(world, audio_config);
        }
        #[cfg(feature = "video")]
        if self.video_enabled {
            crate::app::services::install_video(world);
        }

        // Run one-time setup.
        {
            self.logs.set_frame(None);
            logging::set_logger_frame(None);
            let mut setup_ctx =
                SetupContext::new(world, renderer.as_mut(), &window, self.logs.as_ref());
            self.app_state.setup(&mut setup_ctx);
            logging::drain_logger(self.logs.as_ref());
        }

        #[cfg(feature = "egui")]
        let egui = renderer.wgpu().map(|gpu| {
            crate::app::egui_integration::EguiIntegration::new(
                &window,
                gpu.device(),
                gpu.surface_format(),
            )
        });

        self.runtime = Some(RuntimeState {
            window,
            renderer,
            input,
            last_frame_time: None,
            occluded: false,
            aux_windows: Vec::new(),
            #[cfg(feature = "egui")]
            egui,
        });
        if let Some(rt) = self.runtime.as_ref() {
            if rt.renderer.backend_kind() == RenderBackendKind::Kajiya && kajiya_trace_enabled() {
                eprintln!(
                    "[SkyEngine][App] resumed Kajiya backend surface={}x{}",
                    rt.renderer.surface_size()[0],
                    rt.renderer.surface_size()[1]
                );
            }
        }
        self.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.did_shutdown {
            event_loop.exit();
            return;
        }

        let should_request = match self.runner_options.redraw_mode {
            RedrawMode::Continuous => self.can_draw(),
            RedrawMode::Reactive => self.pending_redraw && self.can_draw(),
        };

        if should_request {
            if let Some(rt) = self.runtime.as_ref() {
                if let Some(next_frame) = crate::app::pacing::next_frame_deadline(
                    rt.last_frame_time,
                    self.runner_options.frame_rate_limit,
                    Instant::now(),
                ) {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame));
                    return;
                }
                if rt.renderer.backend_kind() == RenderBackendKind::Kajiya
                    && should_trace_kajiya_runner_frame(self.kajiya_debug_frames)
                {
                    eprintln!(
                        "[SkyEngine][App] request_redraw frame={} mode={:?}",
                        self.kajiya_debug_frames, self.runner_options.redraw_mode
                    );
                }
                rt.window.request_redraw();
            }
            self.pending_redraw = false;
        }

        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(rt) = self.runtime.as_mut() {
            rt.occluded = true;
            rt.last_frame_time = None;
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.is_aux_window(_window_id) {
            self.handle_aux_window_event(event_loop, _window_id, event);
            return;
        }

        let modal_focus_window = self.runtime.as_ref().and_then(|rt| {
            rt.aux_windows
                .iter()
                .rev()
                .find(|window| window.is_modal() && !window.close_requested())
                .map(|window| window.window().clone())
        });
        let aux_modal_open = modal_focus_window.is_some();

        let Some(rt) = self.runtime.as_mut() else {
            return;
        };

        #[cfg(feature = "egui")]
        let egui_consumed = !aux_modal_open
            && rt
                .egui
                .as_mut()
                .is_some_and(|egui| egui.on_window_event(&rt.window, &event));
        let scale_factor = rt.window.scale_factor() as f32;

        #[cfg(feature = "ui-core")]
        let ui_consumed = aux_modal_open
            || self.world.as_mut().is_some_and(|world| {
                crate::ui::handle_ui_event(world, Some(&rt.window), &event, scale_factor).consumed
            });

        match event {
            WindowEvent::CloseRequested => {
                self.shutdown_and_exit(event_loop);
            }

            WindowEvent::Resized(size) => {
                rt.renderer.resize(size.width, size.height);
                if size.width == 0 || size.height == 0 {
                    rt.last_frame_time = None;
                }
                self.app_state.on_resize(size.width, size.height);
                if size.width > 0 && size.height > 0 {
                    self.request_redraw();
                }
            }

            WindowEvent::Occluded(occluded) => {
                rt.occluded = occluded;
                if occluded {
                    rt.last_frame_time = None;
                } else {
                    self.request_redraw();
                }
            }

            WindowEvent::Focused(focused) => {
                if focused && aux_modal_open {
                    if let Some(window) = &modal_focus_window {
                        window.focus_window();
                    }
                } else if !focused {
                    rt.input.reset();
                }
            }

            WindowEvent::ScaleFactorChanged { .. } => {
                self.request_redraw();
            }

            WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => {
                let suppressed = {
                    #[cfg(feature = "egui")]
                    let egui_suppressed = egui_consumed;
                    #[cfg(not(feature = "egui"))]
                    let egui_suppressed = false;

                    #[cfg(feature = "ui-core")]
                    {
                        egui_suppressed || ui_consumed
                    }
                    #[cfg(not(feature = "ui-core"))]
                    {
                        egui_suppressed
                    }
                };

                crate::app::input::update_from_window_event(
                    &mut rt.input,
                    &event,
                    suppressed,
                    scale_factor,
                );
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                if rt.renderer.backend_kind() == RenderBackendKind::Kajiya
                    && should_trace_kajiya_runner_frame(self.kajiya_debug_frames)
                {
                    eprintln!(
                        "[SkyEngine][App] RedrawRequested frame={}",
                        self.kajiya_debug_frames
                    );
                }
                self.run_frame(event_loop);
            }

            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown_world();
    }
}

fn should_trace_kajiya_runner_frame(frame_index: u64) -> bool {
    kajiya_trace_enabled() && (frame_index < 8 || frame_index % 120 == 0)
}

fn kajiya_trace_enabled() -> bool {
    std::env::var_os("SKY_KAJIYA_TRACE").is_some_and(|value| {
        let value = value.to_string_lossy();
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}
