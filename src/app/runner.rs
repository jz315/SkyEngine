//! Application runner — winit event loop integration.
//!
//! # Usage
//!
//! ```rust,no_run
//! use sky_engine::app::{App, AppConfig, FrameContext};
//! use sky_engine::ecs::World;
//! use sky_engine::render::RenderPipelineAsset;
//!
//! struct Game;
//!
//! impl sky_engine::app::AppState for Game {
//!     fn update(&mut self, ctx: &mut FrameContext) {
//!         ctx.render();
//!     }
//! }
//!
//! App::new(AppConfig::new("Hello", 960, 640), World::new())
//!     .with_render_pipeline(RenderPipelineAsset::universal_2d())
//!     .run(Game);
//! ```

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::config::{AppConfig, RedrawMode};
use crate::app::input::{Input, KeyCode};
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::{RenderComposer, RenderPipelineAsset, RenderStats};

fn update_input_from_window_event(input: &mut Input, event: &WindowEvent, suppressed: bool) {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                let key = KeyCode::from_winit(code);
                match (event.state, suppressed) {
                    (ElementState::Pressed, false) => input.key_down(key),
                    (ElementState::Released, false) => input.key_up(key),
                    (ElementState::Pressed, true) => input.suppress_key_down(key),
                    (ElementState::Released, true) => input.suppress_key_up(key),
                }
            }
        }
        WindowEvent::CursorEntered { .. } => {
            if suppressed {
                input.set_cursor_in_window(false);
            } else {
                input.set_cursor_in_window(true);
            }
        }
        WindowEvent::CursorLeft { .. } => {
            input.set_cursor_in_window(false);
        }
        WindowEvent::CursorMoved { position, .. } => {
            if suppressed {
                input.set_mouse_position_suppressed(position.x as f32, position.y as f32);
            } else {
                input.set_mouse_position(position.x as f32, position.y as f32);
            }
        }
        WindowEvent::MouseInput {
            state: button_state,
            button,
            ..
        } => {
            let index = match button {
                winit::event::MouseButton::Left => 0,
                winit::event::MouseButton::Right => 1,
                winit::event::MouseButton::Middle => 2,
                _ => return,
            };
            match (button_state, suppressed) {
                (ElementState::Pressed, false) => input.mouse_button_down(index),
                (ElementState::Released, false) => input.mouse_button_up(index),
                (ElementState::Pressed, true) => {
                    input.set_cursor_in_window(false);
                    input.suppress_mouse_button_down(index);
                }
                (ElementState::Released, true) => {
                    input.set_cursor_in_window(false);
                    input.suppress_mouse_button_up(index);
                }
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            if suppressed {
                input.set_cursor_in_window(false);
                return;
            }
            let (dx, dy) = match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => (*x, *y),
                winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
            };
            input.add_scroll_delta(dx, dy);
        }
        _ => {}
    }
}

// ── AppState trait ──────────────────────────────────────────────────────────

/// Structured application lifecycle.
///
/// Implement this on your game/app struct for full lifecycle control.
/// The runner calls these methods at the appropriate times; you never
/// need to manage the event loop yourself.
///
/// # Example
///
/// ```rust,no_run
/// use sky_engine::app::{App, AppConfig, AppState, FrameContext};
/// use sky_engine::ecs::World;
/// use sky_engine::gpu::GpuContext;
/// use sky_engine::render::RenderPipelineAsset;
///
/// struct MyGame;
///
/// impl AppState for MyGame {
///     fn setup(&mut self, _world: &mut World, _gpu: &mut GpuContext) {
///         // Load textures, spawn initial entities, etc.
///     }
///
///     fn update(&mut self, ctx: &mut FrameContext) {
///         // Game logic goes here.  ECS systems have already ticked.
///         ctx.render();
///     }
/// }
///
/// App::new(AppConfig::new("My Game", 1280, 720), World::new())
///     .with_render_pipeline(RenderPipelineAsset::universal_2d())
///     .run(MyGame);
/// ```
pub trait AppState: 'static {
    /// Called once after the GPU is ready and `Input` resource exists.
    ///
    /// Use this for texture creation, asset loading, and initial entity
    /// spawns that depend on the GPU.
    fn setup(&mut self, _world: &mut World, _gpu: &mut GpuContext) {}

    /// Called every frame.
    ///
    /// When `AppConfig::auto_tick` is enabled (the default), the ECS
    /// schedule has already been advanced via `world.tick_with_delta(dt)`
    /// before this is called.
    fn update(&mut self, ctx: &mut FrameContext);

    /// Called when the window is resized.
    fn on_resize(&mut self, _width: u32, _height: u32) {}

    /// Called once before the application exits.
    fn shutdown(&mut self, _world: &mut World) {}
}

impl<F> AppState for F
where
    F: for<'a> FnMut(&mut FrameContext<'a>) + 'static,
{
    fn update(&mut self, ctx: &mut FrameContext) {
        self(ctx);
    }
}

// ── FrameContext ────────────────────────────────────────────────────────────

/// Per-frame context passed to [`AppState::update`].
///
/// Provides access to the ECS world, input state, and rendering facilities.
///
/// # Quick start
///
/// ```rust,no_run
/// # use sky_engine::app::FrameContext;
/// fn update(ctx: &mut FrameContext) {
///     // ECS systems already ticked (if auto_tick enabled)
///     ctx.render();  // draw using the installed render pipeline
/// }
/// ```
pub struct FrameContext<'a> {
    /// The ECS world.  Spawn entities, run queries — all here.
    pub world: &'a mut World,

    /// Input state for this frame (keyboard + mouse).
    pub input: &'a Input,

    /// Frame delta time in seconds (clamped by `AppConfig::max_delta`).
    pub dt: f32,

    // ── Internal ────────────────────────────────────────────────────────
    gpu: &'a mut GpuContext,
    renderer: Option<&'a mut RenderComposer>,
    window: &'a Window,
    exit_requested: &'a mut bool,
    redraw_requested: &'a mut bool,
    #[cfg(feature = "egui")]
    egui: &'a mut crate::app::egui_integration::EguiIntegration,
}

impl<'a> FrameContext<'a> {
    /// Execute the installed render pipeline.
    pub fn render(&mut self) {
        self.renderer
            .as_deref_mut()
            .expect("FrameContext::render requires App::with_render_pipeline(...)")
            .render_world(self.gpu, self.world);
    }

    /// Current surface size in physical pixels `[width, height]`.
    #[inline]
    pub fn surface_size(&self) -> [u32; 2] {
        self.gpu.surface_size()
    }

    /// Rendering statistics from the most recent `render()`.
    #[inline]
    pub fn render_stats(&self) -> RenderStats {
        self.renderer
            .as_deref()
            .map(RenderComposer::stats)
            .unwrap_or_default()
    }

    /// Update the window title bar.
    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    /// Direct access to the GPU backend.
    ///
    /// Use this for custom render passes, manual texture creation, or
    /// anything that requires the raw wgpu device and queue.
    #[inline]
    pub fn gpu(&mut self) -> &mut GpuContext {
        self.gpu
    }

    /// Mutably access a registered render domain by concrete type.
    pub fn domain_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.renderer.as_deref_mut()?.domain_mut::<T>()
    }

    /// Mutably access a render domain and the GPU at the same time.
    pub fn with_domain_mut<T: 'static, R>(
        &mut self,
        f: impl FnOnce(&mut T, &mut GpuContext) -> R,
    ) -> Option<R> {
        let renderer = self.renderer.as_deref_mut()?;
        let domain = renderer.domain_mut::<T>()?;
        Some(f(domain, self.gpu))
    }

    /// Request the application to exit after this frame.
    pub fn request_exit(&mut self) {
        *self.exit_requested = true;
    }

    /// Request another redraw after this frame completes.
    ///
    /// This is primarily useful in [`RedrawMode::Reactive`] applications
    /// when a UI animation or custom pacing logic needs to keep driving
    /// future frames.
    pub fn request_redraw(&mut self) {
        *self.redraw_requested = true;
    }

    /// Run an egui UI overlay.
    ///
    /// The closure receives the raw [`egui::Context`] — write standard egui
    /// code directly.  The UI is rendered on top of the current surface
    /// content at the end of the frame.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// ctx.egui(|egui_ctx| {
    ///     egui::Window::new("Debug").show(egui_ctx, |ui| {
    ///         ui.label("hello");
    ///     });
    /// });
    /// ```
    ///
    /// Requires `--features egui`.
    #[cfg(feature = "egui")]
    pub fn egui(&mut self, ui_fn: impl FnMut(&egui::Context)) {
        self.egui.run(self.window, ui_fn);
    }
}

// ── App builder ─────────────────────────────────────────────────────────────

/// Application builder.
///
/// Create with [`App::new`], then call [`.run()`](App::run) with your
/// [`AppState`] implementation to enter the main loop.
///
/// # Examples
///
/// ```rust,no_run
/// use sky_engine::app::{App, AppConfig, AppState, FrameContext};
/// use sky_engine::ecs::World;
/// use sky_engine::render::RenderPipelineAsset;
///
/// struct MyApp;
///
/// impl AppState for MyApp {
///     fn update(&mut self, ctx: &mut FrameContext) {
///         ctx.render();
///     }
/// }
///
/// App::new(AppConfig::new("Demo", 960, 640), World::new())
///     .with_render_pipeline(RenderPipelineAsset::universal_unlit())
///     .run(MyApp);
/// ```
pub struct App {
    config: AppConfig,
    world: World,
    renderer: Option<RenderComposer>,
}

impl App {
    /// Create a new application builder.
    ///
    /// The [`World`] is the centre of your application — spawn entities,
    /// register systems, and insert resources before calling `.run()`.
    pub fn new(config: AppConfig, world: World) -> Self {
        Self {
            config,
            world,
            renderer: None,
        }
    }

    pub fn with_render_pipeline(mut self, pipeline: RenderPipelineAsset) -> Self {
        self.renderer = Some(RenderComposer::from_asset(pipeline));
        self
    }

    /// Enter the main loop.
    ///
    /// The `state` receives structured lifecycle callbacks via [`AppState`].
    ///
    /// This function does **not** return under normal operation.
    pub fn run<S: AppState>(self, state: S) {
        let event_loop = EventLoop::new().expect("Failed to create event loop");

        let mut handler = RunnerHandler {
            config: self.config,
            world: Some(self.world),
            renderer: self.renderer,
            app_state: Box::new(state),
            runtime: None,
            pending_redraw: false,
            did_shutdown: false,
        };

        event_loop.run_app(&mut handler).expect("Event loop error");
    }
}

// ── Internal state ──────────────────────────────────────────────────────────

struct RuntimeState {
    window: Arc<Window>,
    gpu: GpuContext,
    renderer: Option<RenderComposer>,
    input: Input,
    last_frame_time: Option<Instant>,
    occluded: bool,
    #[cfg(feature = "egui")]
    egui: crate::app::egui_integration::EguiIntegration,
}

struct RunnerHandler {
    config: AppConfig,
    world: Option<World>,
    renderer: Option<RenderComposer>,
    app_state: Box<dyn AppState>,
    runtime: Option<RuntimeState>,
    pending_redraw: bool,
    did_shutdown: bool,
}

impl RunnerHandler {
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
    }

    fn run_frame(&mut self, event_loop: &ActiveEventLoop) {
        if !self.can_draw() {
            return;
        }

        let now = Instant::now();
        let raw_dt = self
            .runtime
            .as_ref()
            .and_then(|rt| rt.last_frame_time)
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        let dt = raw_dt.min(self.config.max_delta);

        let input_snapshot = self.runtime.as_ref().expect("runtime must exist").input;
        let auto_tick = self.config.auto_tick;
        let exit_on_escape = self.config.exit_on_escape;
        let mut request_redraw = false;
        let mut should_exit = false;

        {
            let (world_slot, runtime_slot, app_state) =
                (&mut self.world, &mut self.runtime, &mut self.app_state);
            let world = world_slot.as_mut().expect("world must exist");
            let rt = runtime_slot.as_mut().expect("runtime must exist");
            rt.last_frame_time = Some(now);

            Self::sync_input_resource(world, input_snapshot);

            #[cfg(feature = "asset")]
            if let Some(asset_server) = world.get_resource::<crate::asset::AssetServer>().cloned() {
                if let Err(error) = asset_server.update() {
                    eprintln!("[SkyEngine] Asset update failed: {error}");
                }
            }

            if auto_tick {
                world.tick_with_delta(dt);
            }

            if exit_on_escape && input_snapshot.key_pressed(KeyCode::Escape) {
                rt.input.begin_frame();
                should_exit = true;
            } else {
                match rt.gpu.begin_frame() {
                    Ok(()) => {
                        let mut exit_requested = false;
                        let mut redraw_requested = false;

                        {
                            let ctx = &mut FrameContext {
                                world,
                                input: &input_snapshot,
                                dt,
                                gpu: &mut rt.gpu,
                                renderer: rt.renderer.as_mut(),
                                window: &rt.window,
                                exit_requested: &mut exit_requested,
                                redraw_requested: &mut redraw_requested,
                                #[cfg(feature = "egui")]
                                egui: &mut rt.egui,
                            };
                            app_state.update(ctx);
                        }

                        #[cfg(feature = "audio")]
                        if let Some(audio_server) =
                            world.get_resource::<crate::audio::AudioServer>().cloned()
                        {
                            if let Err(error) = audio_server.apply_commands() {
                                eprintln!("[SkyEngine] Audio command application failed: {error}");
                            }

                            if let Err(error) = audio_server.sync_world(world) {
                                eprintln!("[SkyEngine] Audio world sync failed: {error}");
                            }
                            audio_server.update();
                        }

                        #[cfg(feature = "egui")]
                        {
                            let surface_view = rt.gpu.surface_view().clone();
                            let device = rt.gpu.device().clone();
                            let queue = rt.gpu.queue().clone();
                            rt.egui.end_frame(
                                &device,
                                &queue,
                                rt.gpu.encoder(),
                                &surface_view,
                                &rt.window,
                            );
                        }

                        rt.window.pre_present_notify();
                        rt.gpu.end_frame();
                        rt.input.begin_frame();

                        request_redraw = redraw_requested;
                        should_exit = exit_requested;
                    }
                    Err(crate::gpu::GpuError::SurfaceLost) => {
                        if let Some(renderer) = rt.renderer.as_mut() {
                            renderer.surface_lost();
                        }
                        let size = rt.window.inner_size();
                        rt.gpu.resize_surface(size.width, size.height);
                        if let Some(renderer) = rt.renderer.as_mut() {
                            renderer.resize(&rt.gpu, size.width, size.height);
                        }
                        rt.last_frame_time = None;
                        request_redraw = true;
                    }
                    Err(crate::gpu::GpuError::Timeout) => {
                        rt.last_frame_time = None;
                        request_redraw = true;
                    }
                    Err(crate::gpu::GpuError::OutOfMemory) => {
                        should_exit = true;
                    }
                    Err(e) => {
                        eprintln!("[SkyEngine] GPU error: {e}");
                        rt.last_frame_time = None;
                    }
                }
            }
        }

        if request_redraw {
            self.request_redraw();
        }
        if should_exit {
            self.shutdown_and_exit(event_loop);
        }
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
        let attrs = WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ))
            .with_resizable(self.config.resizable);

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        let mut gpu = match GpuContext::try_new(window.clone(), self.config.vsync) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("[SkyEngine] GPU initialization failed: {err}");
                event_loop.exit();
                return;
            }
        };

        let title = format!(
            "{} | {} ({})",
            self.config.title,
            gpu.adapter_name(),
            gpu.backend_name()
        );
        window.set_title(&title);

        let world = self.world.as_mut().expect("world must be present");
        let renderer = self.renderer.take();

        // Insert Input resource into the World (updated in-place each frame).
        let input = Input::new();
        world.insert_resource(input);

        #[cfg(feature = "asset")]
        if !world.contains_resource::<crate::asset::AssetServer>() {
            let config = crate::asset::AssetConfig::default();
            let asset_server = match crate::asset::AssetServer::new(config.clone()) {
                Ok(server) => server,
                Err(error) => {
                    eprintln!("[SkyEngine] Asset server initialization failed: {error}");
                    crate::asset::AssetServer::with_empty_manifest(config)
                }
            };
            world.insert_resource(asset_server);
        }

        #[cfg(feature = "audio")]
        {
            let asset_server =
                if let Some(server) = world.get_resource::<crate::asset::AssetServer>().cloned() {
                    server
                } else {
                    let config = crate::asset::AssetConfig::default();
                    let server = match crate::asset::AssetServer::new(config.clone()) {
                        Ok(server) => server,
                        Err(error) => {
                            eprintln!("[SkyEngine] Asset server initialization failed: {error}");
                            crate::asset::AssetServer::with_empty_manifest(config)
                        }
                    };
                    world.insert_resource(server.clone());
                    server
                };

            if !world.contains_resource::<crate::audio::AudioServer>() {
                let audio_server = crate::audio::AudioServer::new(
                    crate::audio::AudioConfig::default(),
                    asset_server,
                );
                let audio_commands = audio_server.commands();
                world.insert_resource(audio_server);
                if !world.contains_resource::<crate::audio::AudioCommands>() {
                    world.insert_resource(audio_commands);
                }
            } else if !world.contains_resource::<crate::audio::AudioCommands>() {
                if let Some(audio_server) =
                    world.get_resource::<crate::audio::AudioServer>().cloned()
                {
                    world.insert_resource(audio_server.commands());
                }
            }
        }

        // Run one-time setup.
        self.app_state.setup(world, &mut gpu);

        #[cfg(feature = "egui")]
        let egui = crate::app::egui_integration::EguiIntegration::new(
            &window,
            gpu.device(),
            gpu.surface_format(),
        );

        self.runtime = Some(RuntimeState {
            window,
            gpu,
            renderer,
            input,
            last_frame_time: None,
            occluded: false,
            #[cfg(feature = "egui")]
            egui,
        });
        self.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.did_shutdown {
            event_loop.exit();
            return;
        }

        let should_request = match self.config.redraw_mode {
            RedrawMode::Continuous => self.can_draw(),
            RedrawMode::Reactive => self.pending_redraw && self.can_draw(),
        };

        if should_request {
            if let Some(rt) = self.runtime.as_ref() {
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
        let Some(rt) = self.runtime.as_mut() else {
            return;
        };

        #[cfg(feature = "egui")]
        let egui_consumed = rt.egui.on_window_event(&rt.window, &event);

        match event {
            WindowEvent::CloseRequested => {
                self.shutdown_and_exit(event_loop);
            }

            WindowEvent::Resized(size) => {
                rt.gpu.resize_surface(size.width, size.height);
                if let Some(renderer) = rt.renderer.as_mut() {
                    renderer.resize(&rt.gpu, size.width, size.height);
                }
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

            WindowEvent::Focused(false) => {
                rt.input.reset();
            }

            WindowEvent::ScaleFactorChanged { .. } => {
                self.request_redraw();
            }

            WindowEvent::KeyboardInput { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => {
                #[cfg(feature = "egui")]
                let suppressed = egui_consumed;
                #[cfg(not(feature = "egui"))]
                let suppressed = false;

                update_input_from_window_event(&mut rt.input, &event, suppressed);
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                self.run_frame(event_loop);
            }

            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown_world();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_defaults_to_no_installed_pipeline() {
        let app = App::new(AppConfig::new("test", 64, 64), World::new());
        assert!(app.renderer.is_none());
    }

    #[test]
    fn with_render_pipeline_installs_the_supplied_pipeline() {
        let app = App::new(AppConfig::new("test", 64, 64), World::new())
            .with_render_pipeline(RenderPipelineAsset::universal_unlit());
        assert!(app.renderer.is_some());
    }

    #[test]
    fn with_render_pipeline_accepts_the_default_universal_2d_pipeline() {
        let app = App::new(AppConfig::new("test", 64, 64), World::new())
            .with_render_pipeline(RenderPipelineAsset::universal_2d());
        assert!(app.renderer.is_some());
    }
}
