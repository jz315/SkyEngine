//! winit lifecycle integration for the app runner.

use std::sync::{Arc, OnceLock};
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
use crate::render::{RenderBackendKind, RenderPipelineAsset, SceneFrameClearReason, SceneRenderer};

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
    app_profile_frames: u64,
    startup_start: Instant,
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
            app_profile_frames: 0,
            startup_start: Instant::now(),
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
        let mut app_profile = AppFrameProfile::new(self.app_profile_frames, now);
        let mut app_profile_samples = AppFrameProfileSamples::default();
        #[cfg(feature = "profile")]
        let _profile_frame = sky_profile::profile_frame!(self.app_profile_frames);
        #[cfg(feature = "profile")]
        let _profile_scope = sky_profile::profile_scope!("app", "App::run_frame");
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
        #[cfg(feature = "profile")]
        let mut profile_render_stats = None;
        let logs = Arc::clone(&self.logs);
        app_profile_samples.frame_setup_ms = app_profile.mark();

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
            app_profile_samples.input_sync_ms = app_profile.mark();

            // Update action-based input system (if registered).
            if let Some(actions) = world.get_resource_mut::<crate::input::InputActions>() {
                actions.update(&input_snapshot);
            }
            app_profile_samples.actions_ms = app_profile.mark();

            crate::app::services::update_assets(world);
            app_profile_samples.assets_ms = app_profile.mark();

            let schedule_error = auto_tick
                .then(|| world.tick_with_frame_delta(dt, raw_dt).err())
                .flatten();
            app_profile_samples.tick_ms = app_profile.mark();
            let frame_dt = if auto_tick {
                world.time.frame_delta
            } else {
                dt
            };
            logs.set_frame(Some(world.time.frame_count));
            logging::set_logger_frame(Some(world.time.frame_count));

            if schedule_error.is_none() {
                crate::app::services::update_video(world, frame_dt);
            }
            app_profile_samples.video_ms = app_profile.mark();

            if let Some(error) = schedule_error {
                eprintln!("[SkyEngine] ECS schedule tick failed: {error}");
                rt.input.begin_frame();
                app_profile_samples.input_reset_ms = app_profile.mark();
                should_exit = true;
            } else if exit_on_escape && input_snapshot.key_pressed(KeyCode::Escape) {
                rt.input.begin_frame();
                app_profile_samples.input_reset_ms = app_profile.mark();
                should_exit = true;
            } else {
                let begin_frame_result = rt.renderer.begin_frame();
                app_profile_samples.begin_frame_ms = app_profile.mark();
                match begin_frame_result {
                    Ok(mut renderer_frame) => {
                        let mut exit_requested = false;
                        let mut redraw_requested = false;
                        let mut screenshot_requests = Vec::new();

                        {
                            let ctx = &mut FrameContext {
                                world,
                                input: &input_snapshot,
                                dt: frame_dt,
                                frame: &mut renderer_frame,
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
                        app_profile_samples.update_ms = app_profile.mark();

                        crate::app::services::update_audio_after_frame(world);
                        app_profile_samples.audio_ms = app_profile.mark();

                        if !renderer_frame.is_presentable() {
                            rt.renderer.clear_frame(
                                &mut renderer_frame,
                                world,
                                SceneFrameClearReason::NoRenderCall,
                            );
                        }

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
                        app_profile_samples.egui_ms = app_profile.mark();

                        crate::app::screenshots::save_requested(
                            rt.renderer.as_mut(),
                            &renderer_frame,
                            &mut screenshot_requests,
                        );
                        app_profile_samples.screenshot_ms = app_profile.mark();
                        if !renderer_frame.pre_present_notified() {
                            rt.window.pre_present_notify();
                            renderer_frame.mark_pre_present_notified();
                        }
                        app_profile_samples.pre_present_ms = app_profile.mark();
                        rt.renderer.end_frame(renderer_frame);
                        app_profile_samples.end_frame_ms = app_profile.mark();
                        let render_stats = rt.renderer.stats();
                        crate::app::render_diagnostics::publish_render_diagnostics(
                            world,
                            render_stats,
                        );
                        #[cfg(feature = "profile")]
                        {
                            profile_render_stats = Some(render_stats);
                        }
                        rt.input.begin_frame();
                        app_profile_samples.input_reset_ms = app_profile.mark();

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
                    Err(SceneRendererError::Wgpu(crate::gpu::GpuError::Occluded)) => {
                        rt.last_frame_time = None;
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
        app_profile_samples.log_drain_ms = app_profile.mark();

        if request_redraw {
            self.request_redraw();
        }
        app_profile_samples.redraw_request_ms = app_profile.mark();
        self.create_aux_windows(event_loop, aux_window_requests);
        app_profile_samples.aux_windows_ms = app_profile.mark();
        self.prune_aux_windows();
        app_profile_samples.prune_windows_ms = app_profile.mark();
        if trace_kajiya && should_trace_kajiya_runner_frame(trace_frame) {
            eprintln!(
                "[SkyEngine][App] run_frame end frame={} request_redraw={} should_exit={}",
                trace_frame, request_redraw, should_exit
            );
        }
        if trace_kajiya {
            self.kajiya_debug_frames = self.kajiya_debug_frames.wrapping_add(1);
        }
        app_profile_samples.shutdown_ms = app_profile.mark();
        app_profile.print(&app_profile_samples, request_redraw, should_exit);
        #[cfg(feature = "profile")]
        record_app_profile_frame(
            self.app_profile_frames,
            app_profile.start,
            &app_profile_samples,
            profile_render_stats,
            request_redraw,
            should_exit,
        );
        self.app_profile_frames = self.app_profile_frames.wrapping_add(1);
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
        let mut startup_profile = AppStartupProfile::new(self.startup_start);
        let mut startup_samples = AppStartupProfileSamples::default();
        #[cfg(feature = "profile")]
        let _startup_scope = sky_profile::profile_scope!("app", "App::startup");
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
        startup_samples.window_ms = startup_profile.mark();

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
        startup_samples.renderer_ms = startup_profile.mark();

        let title = format!(
            "{} | {} ({})",
            window_options.title,
            renderer.adapter_name(),
            renderer.backend_name()
        );
        window.set_title(&title);
        startup_samples.title_ms = startup_profile.mark();

        let world = self.world.as_mut().expect("world must be present");

        let input = Input::new();
        if self.input_enabled {
            // Insert Input resource into the World (updated in-place each frame).
            world.insert_resource(input);
        }
        startup_samples.input_ms = startup_profile.mark();

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
        startup_samples.services_ms = startup_profile.mark();

        // Run one-time setup.
        {
            self.logs.set_frame(None);
            logging::set_logger_frame(None);
            let mut setup_ctx =
                SetupContext::new(world, renderer.as_mut(), &window, self.logs.as_ref());
            self.app_state.setup(&mut setup_ctx);
            logging::drain_logger(self.logs.as_ref());
        }
        startup_samples.setup_ms = startup_profile.mark();

        #[cfg(feature = "egui")]
        let egui = renderer.wgpu().map(|gpu| {
            crate::app::egui_integration::EguiIntegration::new(
                &window,
                gpu.device(),
                gpu.surface_format(),
            )
        });
        #[cfg(feature = "egui")]
        {
            startup_samples.egui_ms = startup_profile.mark();
        }

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
        startup_samples.runtime_ms = startup_profile.mark();
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
        startup_samples.request_redraw_ms = startup_profile.mark();
        startup_profile.print(&startup_samples);
        #[cfg(feature = "profile")]
        record_app_startup_profile(startup_profile.start, &startup_samples);
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
        #[cfg(feature = "profile")]
        sky_profile::flush();
    }
}

fn should_trace_kajiya_runner_frame(frame_index: u64) -> bool {
    kajiya_trace_enabled() && (frame_index < 8 || frame_index.is_multiple_of(120))
}

#[cfg(feature = "profile")]
fn record_app_profile_frame(
    frame: u64,
    start: Instant,
    samples: &AppFrameProfileSamples,
    render_stats: Option<crate::render::RenderStats>,
    request_redraw: bool,
    should_exit: bool,
) {
    let segments = [
        ("frame_setup", samples.frame_setup_ms),
        ("input_sync", samples.input_sync_ms),
        ("actions", samples.actions_ms),
        ("assets", samples.assets_ms),
        ("tick", samples.tick_ms),
        ("video", samples.video_ms),
        ("surface_begin", samples.begin_frame_ms),
        ("update", samples.update_ms),
        ("audio", samples.audio_ms),
        ("egui", samples.egui_ms),
        ("screenshot", samples.screenshot_ms),
        ("pre_present", samples.pre_present_ms),
        ("end_submit_present", samples.end_frame_ms),
        ("input_reset", samples.input_reset_ms),
        ("log_drain", samples.log_drain_ms),
        ("redraw_request", samples.redraw_request_ms),
        ("aux_windows", samples.aux_windows_ms),
        ("prune_windows", samples.prune_windows_ms),
        ("shutdown", samples.shutdown_ms),
    ];
    record_profile_segments(Some(frame), start, "app", &segments);

    let mut profile_frame = sky_profile::ProfileFrame::new(sky_profile::run_id(), frame);
    profile_frame.cpu_frame_ms = start.elapsed().as_secs_f32() * 1000.0;
    if let Some(render_stats) = render_stats {
        profile_frame.draw_calls = render_stats.draw_calls;
        profile_frame.passes = render_stats.passes;
        profile_frame.uploaded_render_assets = render_stats.uploaded_render_assets;
        profile_frame.uploaded_render_asset_bytes = render_stats.uploaded_render_asset_bytes;
        profile_frame.gpu_frame_ms = None;
        profile_frame = profile_frame
            .with_metadata("render_execute_ms", render_stats.timings.execute_ms)
            .with_metadata("render_upload_ms", render_stats.timings.upload_ms)
            .with_metadata("render_frame_ms", render_stats.timings.frame_ms);
    }
    profile_frame = profile_frame
        .with_metadata("request_redraw", request_redraw)
        .with_metadata("should_exit", should_exit);
    sky_profile::record_frame(profile_frame);
}

#[cfg(feature = "profile")]
fn record_app_startup_profile(start: Instant, samples: &AppStartupProfileSamples) {
    let segments = [
        ("window", samples.window_ms),
        ("renderer", samples.renderer_ms),
        ("title", samples.title_ms),
        ("input", samples.input_ms),
        ("services", samples.services_ms),
        ("setup", samples.setup_ms),
        ("egui", app_startup_profile_egui_ms(samples)),
        ("runtime", samples.runtime_ms),
        ("request_redraw", samples.request_redraw_ms),
    ];
    record_profile_segments(None, start, "app_startup", &segments);
    sky_profile::flush();
}

#[cfg(feature = "profile")]
fn record_profile_segments(
    frame: Option<u64>,
    start: Instant,
    category: &'static str,
    segments: &[(&'static str, f32)],
) {
    if !sky_profile::enabled() {
        return;
    }
    let run_id = sky_profile::run_id();
    let mut cursor_ns = sky_profile::elapsed_ns_since_start(start);
    for &(name, elapsed_ms) in segments {
        let duration_ns = profile_ms_to_ns(elapsed_ms);
        sky_profile::record_event(sky_profile::ProfileEvent::new(
            run_id.clone(),
            frame,
            category,
            name,
            cursor_ns,
            duration_ns,
        ));
        cursor_ns = cursor_ns.saturating_add(duration_ns);
    }
}

#[cfg(feature = "profile")]
fn profile_ms_to_ns(ms: f32) -> u64 {
    if !ms.is_finite() || ms <= 0.0 {
        return 0;
    }
    (f64::from(ms) * 1_000_000.0).round().min(u64::MAX as f64) as u64
}

#[derive(Debug, Default)]
struct AppFrameProfileSamples {
    frame_setup_ms: f32,
    input_sync_ms: f32,
    actions_ms: f32,
    assets_ms: f32,
    tick_ms: f32,
    video_ms: f32,
    begin_frame_ms: f32,
    update_ms: f32,
    audio_ms: f32,
    egui_ms: f32,
    screenshot_ms: f32,
    pre_present_ms: f32,
    end_frame_ms: f32,
    input_reset_ms: f32,
    log_drain_ms: f32,
    redraw_request_ms: f32,
    aux_windows_ms: f32,
    prune_windows_ms: f32,
    shutdown_ms: f32,
}

struct AppFrameProfile {
    enabled: bool,
    frame: u64,
    start: Instant,
    last: Instant,
}

impl AppFrameProfile {
    fn new(frame: u64, start: Instant) -> Self {
        Self {
            enabled: app_profile_enabled(),
            frame,
            start,
            last: start,
        }
    }

    fn mark(&mut self) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let now = Instant::now();
        let elapsed_ms = now.duration_since(self.last).as_secs_f32() * 1000.0;
        self.last = now;
        elapsed_ms
    }

    fn print(&self, samples: &AppFrameProfileSamples, request_redraw: bool, should_exit: bool) {
        if !self.enabled || !should_trace_app_profile_frame(self.frame) {
            return;
        }
        let total_ms = self.start.elapsed().as_secs_f32() * 1000.0;
        eprintln!(
            concat!(
                "[SkyEngine][AppProfile] frame={} total={:.3}ms ",
                "setup={:.3} input={:.3} actions={:.3} assets={:.3} tick={:.3} video={:.3} ",
                "surface_begin={:.3} update={:.3} audio={:.3} egui={:.3} screenshot={:.3} ",
                "pre_present={:.3} end_submit_present={:.3} input_reset={:.3} log={:.3} ",
                "redraw={:.3} aux={:.3} prune={:.3} shutdown={:.3} request_redraw={} should_exit={}"
            ),
            self.frame,
            total_ms,
            samples.frame_setup_ms,
            samples.input_sync_ms,
            samples.actions_ms,
            samples.assets_ms,
            samples.tick_ms,
            samples.video_ms,
            samples.begin_frame_ms,
            samples.update_ms,
            samples.audio_ms,
            samples.egui_ms,
            samples.screenshot_ms,
            samples.pre_present_ms,
            samples.end_frame_ms,
            samples.input_reset_ms,
            samples.log_drain_ms,
            samples.redraw_request_ms,
            samples.aux_windows_ms,
            samples.prune_windows_ms,
            samples.shutdown_ms,
            request_redraw,
            should_exit
        );
    }
}

#[derive(Debug, Default)]
struct AppStartupProfileSamples {
    window_ms: f32,
    renderer_ms: f32,
    title_ms: f32,
    input_ms: f32,
    services_ms: f32,
    setup_ms: f32,
    #[cfg(feature = "egui")]
    egui_ms: f32,
    runtime_ms: f32,
    request_redraw_ms: f32,
}

struct AppStartupProfile {
    enabled: bool,
    start: Instant,
    last: Instant,
}

impl AppStartupProfile {
    fn new(start: Instant) -> Self {
        Self {
            enabled: app_startup_profile_enabled(),
            start,
            last: start,
        }
    }

    fn mark(&mut self) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let now = Instant::now();
        let elapsed_ms = now.duration_since(self.last).as_secs_f32() * 1000.0;
        self.last = now;
        elapsed_ms
    }

    fn print(&self, samples: &AppStartupProfileSamples) {
        if !self.enabled {
            return;
        }
        let total_ms = self.start.elapsed().as_secs_f32() * 1000.0;
        eprintln!(
            concat!(
                "[SkyEngine][StartupProfile] total={:.3}ms ",
                "window={:.3} renderer={:.3} title={:.3} input={:.3} services={:.3} setup={:.3} ",
                "egui={:.3} runtime={:.3} request_redraw={:.3}"
            ),
            total_ms,
            samples.window_ms,
            samples.renderer_ms,
            samples.title_ms,
            samples.input_ms,
            samples.services_ms,
            samples.setup_ms,
            app_startup_profile_egui_ms(samples),
            samples.runtime_ms,
            samples.request_redraw_ms,
        );
    }
}

#[cfg(feature = "egui")]
fn app_startup_profile_egui_ms(samples: &AppStartupProfileSamples) -> f32 {
    samples.egui_ms
}

#[cfg(not(feature = "egui"))]
fn app_startup_profile_egui_ms(_samples: &AppStartupProfileSamples) -> f32 {
    0.0
}

fn should_trace_app_profile_frame(frame_index: u64) -> bool {
    frame_index < 8 || frame_index.is_multiple_of(120)
}

fn app_startup_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("SKY_APP_STARTUP_PROFILE")
            .or_else(|| std::env::var_os("SKY_APP_PROFILE"))
            .or_else(|| std::env::var_os("SKY_PROFILE"))
            .is_some_and(env_flag_enabled)
    })
}

fn app_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("SKY_APP_PROFILE")
            .or_else(|| std::env::var_os("SKY_PROFILE"))
            .is_some_and(env_flag_enabled)
    })
}

fn kajiya_trace_enabled() -> bool {
    std::env::var_os("SKY_KAJIYA_TRACE").is_some_and(env_flag_enabled)
}

fn env_flag_enabled(value: std::ffi::OsString) -> bool {
    let value = value.to_string_lossy();
    !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
}
