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
//!     .with_render_pipeline(RenderPipelineAsset::forward_2d())
//!     .run(Game);
//! ```

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::config::{AppConfig, RedrawMode};
use crate::diagnostics::{
    write_diagnostic_events, DiagnosticConsole, DiagnosticCursor, Diagnostics,
};
use crate::ecs::{Time, World};
use crate::gpu::GpuContext;
use crate::input::raw::{Input, KeyCode, MouseButton};
use crate::render::backend::{create_scene_renderer, SceneRendererError};
use crate::render::{
    RenderAssets, RenderBackendKind, RenderComposer, RenderPipelineAsset, RenderStats,
    SceneRenderer,
};

fn update_input_from_window_event(
    input: &mut Input,
    event: &WindowEvent,
    suppressed: bool,
    scale_factor: f32,
) {
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
            let [x, y] = physical_cursor_to_logical(*position, scale_factor);
            if suppressed {
                input.set_mouse_position_suppressed(x, y);
            } else {
                input.set_mouse_position(x, y);
            }
        }
        WindowEvent::MouseInput {
            state: button_state,
            button,
            ..
        } => {
            let Some(mb) = MouseButton::from_winit(*button) else {
                return;
            };
            let index = mb.index();
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

fn physical_cursor_to_logical(
    position: winit::dpi::PhysicalPosition<f64>,
    scale_factor: f32,
) -> [f32; 2] {
    let scale = scale_factor.max(0.0001) as f64;
    [(position.x / scale) as f32, (position.y / scale) as f32]
}

fn write_new_diagnostics<W: std::io::Write>(
    world: &World,
    cursor: &mut DiagnosticCursor,
    console: DiagnosticConsole,
    writer: &mut W,
) -> std::io::Result<usize> {
    let Some(diagnostics) = world.get_resource::<Diagnostics>() else {
        return Ok(0);
    };
    let events = diagnostics.events_since(cursor);
    write_diagnostic_events(writer, events.iter(), console)
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
/// use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
/// use sky_engine::ecs::World;
/// use sky_engine::render::{RenderPipelineAsset, SpriteFeature, TransparentPhase};
///
/// struct MyGame;
///
/// impl AppState for MyGame {
///     fn setup(&mut self, ctx: &mut SetupContext) {
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
///     .with_render_pipeline(RenderPipelineAsset::forward_2d())
///     .run(MyGame);
/// ```
pub trait AppState: 'static {
    /// Called once after the window and render backend are ready.
    ///
    /// Use this for backend-neutral asset creation, asset loading, and initial
    /// entity spawns.  Backend-specific GPU setup is still available through
    /// [`SetupContext::gpu`] for wgpu-only applications.
    fn setup(&mut self, _ctx: &mut SetupContext<'_>) {}

    /// Called every frame.
    ///
    /// When `AppConfig::auto_tick` is enabled (the default), the ECS
    /// schedule has already been advanced from the runner-sampled frame delta
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

// ── SetupContext ────────────────────────────────────────────────────────────

/// One-time application setup context.
///
/// This is intentionally backend-neutral: prefer [`render_assets_mut`](Self::render_assets_mut)
/// for meshes, textures, and materials.  The raw wgpu accessors exist only for
/// applications that explicitly choose the wgpu backend.
pub struct SetupContext<'a> {
    /// The ECS world. Spawn initial entities and install resources here.
    pub world: &'a mut World,

    renderer: &'a mut dyn SceneRenderer,
    window: &'a Window,
}

impl<'a> SetupContext<'a> {
    /// Backend requested by the active render pipeline.
    #[inline]
    pub fn backend_kind(&self) -> RenderBackendKind {
        self.renderer.backend_kind()
    }

    /// Create or resolve backend-neutral render assets.
    #[inline]
    pub fn render_assets_mut(&mut self) -> RenderAssets<'_> {
        RenderAssets::new(self.world)
    }

    /// Current surface size in physical pixels `[width, height]`.
    #[inline]
    pub fn surface_size(&self) -> [u32; 2] {
        self.renderer.surface_size()
    }

    /// Window scale factor used to convert physical pixels to logical pixels.
    #[inline]
    pub fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    /// Direct access to the wgpu backend when the active renderer is wgpu.
    #[inline]
    pub fn wgpu(&mut self) -> Option<&mut GpuContext> {
        self.renderer.wgpu_mut()
    }

    /// Direct access to the wgpu backend.
    ///
    /// Panics with a clear message when the active renderer is not wgpu.
    #[inline]
    pub fn gpu(&mut self) -> &mut GpuContext {
        self.wgpu()
            .expect("SetupContext::gpu is only available for the wgpu render backend")
    }

    /// Mutably access the installed wgpu [`RenderComposer`] and GPU together.
    pub fn with_renderer_mut<R>(
        &mut self,
        f: impl FnOnce(&mut RenderComposer, &mut GpuContext) -> R,
    ) -> Option<R> {
        let (renderer, gpu) = self.renderer.wgpu_parts_mut()?;
        Some(f(renderer, gpu))
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

    /// Frame delta time in seconds.
    ///
    /// With automatic ticking enabled this is `world.time.frame_delta`, so it
    /// includes `World::time.time_scale`.  With automatic ticking disabled it
    /// is the runner-sampled clamped delta; manual ticks update `world.time`.
    pub dt: f32,

    // ── Internal ────────────────────────────────────────────────────────
    renderer: &'a mut dyn SceneRenderer,
    window: &'a Window,
    exit_requested: &'a mut bool,
    redraw_requested: &'a mut bool,
    #[cfg(feature = "egui")]
    egui: &'a mut Option<crate::app::egui_integration::EguiIntegration>,
}

impl<'a> FrameContext<'a> {
    /// Execute the installed render pipeline.
    pub fn render(&mut self) {
        self.renderer.render_world(self.world);
    }

    /// Current surface size in physical pixels `[width, height]`.
    #[inline]
    pub fn surface_size(&self) -> [u32; 2] {
        self.renderer.surface_size()
    }

    /// Window scale factor used to convert physical pixels to logical pixels.
    #[inline]
    pub fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    /// Current surface size in logical pixels `[width, height]`.
    #[inline]
    pub fn logical_surface_size(&self) -> [f32; 2] {
        let scale = self.scale_factor().max(0.0001);
        let [width, height] = self.renderer.surface_size();
        [width as f32 / scale, height as f32 / scale]
    }

    /// Built-in ECS timing state for the current world.
    #[inline]
    pub fn time(&self) -> &Time {
        &self.world.time
    }

    /// Frame delta in seconds.
    #[inline]
    pub fn dt(&self) -> f32 {
        self.dt
    }

    /// Rendering statistics from the most recent `render()`.
    #[inline]
    pub fn render_stats(&self) -> RenderStats {
        self.renderer.stats()
    }

    /// Create or resolve backend-neutral render assets.
    #[inline]
    pub fn render_assets_mut(&mut self) -> RenderAssets<'_> {
        RenderAssets::new(self.world)
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
        self.renderer
            .wgpu_mut()
            .expect("FrameContext::gpu is only available for the wgpu render backend")
    }

    /// Mutably access a registered render feature by concrete type.
    pub fn feature_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.renderer.wgpu_composer_mut()?.feature_mut::<T>()
    }

    /// Mutably access a render feature and the GPU at the same time.
    pub fn with_feature_mut<T: 'static, R>(
        &mut self,
        f: impl FnOnce(&mut T, &mut GpuContext) -> R,
    ) -> Option<R> {
        let (renderer, gpu) = self.renderer.wgpu_parts_mut()?;
        let feature = renderer.feature_mut::<T>()?;
        Some(f(feature, gpu))
    }

    /// Mutably access the installed [`RenderComposer`] and the GPU together.
    ///
    /// This is the escape hatch for runtime mesh/material setup that depends on
    /// both renderer-owned registries and a live [`GpuContext`].
    pub fn with_renderer_mut<R>(
        &mut self,
        f: impl FnOnce(&mut RenderComposer, &mut GpuContext) -> R,
    ) -> Option<R> {
        let (renderer, gpu) = self.renderer.wgpu_parts_mut()?;
        Some(f(renderer, gpu))
    }

    /// Update native retained UI layout and interaction state.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui")]
    pub fn update_ui(&mut self) {
        crate::ui::update_ui(self.world, self.input, self.logical_surface_size());
    }

    /// Render native retained UI on top of the current surface frame.
    ///
    /// Call this after `ctx.render()` for the common scene + overlay order.
    /// Requires `--features ui`.
    #[cfg(feature = "ui")]
    pub fn render_ui(&mut self) {
        let gpu = self
            .renderer
            .wgpu_mut()
            .expect("FrameContext::render_ui is only available for the wgpu render backend");
        crate::ui::render_ui(self.world, gpu);
    }

    /// Access native UI state if it has been installed.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui")]
    pub fn ui_state(&self) -> Option<&crate::ui::UiState> {
        self.world.get_resource::<crate::ui::UiState>()
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
        self.egui
            .as_mut()
            .expect("FrameContext::egui is only available for the wgpu render backend")
            .run(self.window, ui_fn);
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
/// use sky_engine::render::{RenderPipelineAsset, SpriteFeature, TransparentPhase};
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
///     .with_render_pipeline(
///         RenderPipelineAsset::builder()
///             .add_feature(SpriteFeature::unlit())
///             .add_phase(TransparentPhase::new())
///             .build(),
///     )
///     .run(MyApp);
/// ```
pub struct App {
    config: AppConfig,
    world: World,
    pipeline: Option<RenderPipelineAsset>,
}

impl App {
    /// Create a new application builder.
    ///
    /// The [`World`] is the centre of your application — spawn entities,
    /// register systems, and insert resources before calling `.run()`.
    pub fn new(config: AppConfig, world: World) -> Self {
        #[cfg(feature = "ui")]
        let world = {
            let mut world = world;
            world.insert_resource(config.ui.clone());
            world
        };

        Self {
            config,
            world,
            pipeline: None,
        }
    }

    pub fn with_render_pipeline(mut self, pipeline: RenderPipelineAsset) -> Self {
        self.pipeline = Some(pipeline);
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
            pipeline: self.pipeline,
            app_state: Box::new(state),
            runtime: None,
            pending_redraw: false,
            kajiya_debug_frames: 0,
            did_shutdown: false,
        };

        event_loop.run_app(&mut handler).expect("Event loop error");
    }
}

// ── Internal state ──────────────────────────────────────────────────────────

struct RuntimeState {
    window: Arc<Window>,
    renderer: Box<dyn SceneRenderer>,
    input: Input,
    diagnostic_cursor: DiagnosticCursor,
    last_frame_time: Option<Instant>,
    occluded: bool,
    #[cfg(feature = "egui")]
    egui: Option<crate::app::egui_integration::EguiIntegration>,
}

struct RunnerHandler {
    config: AppConfig,
    world: Option<World>,
    pipeline: Option<RenderPipelineAsset>,
    app_state: Box<dyn AppState>,
    runtime: Option<RuntimeState>,
    pending_redraw: bool,
    kajiya_debug_frames: u64,
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

            // Update action-based input system (if registered).
            if let Some(actions) = world.get_resource_mut::<crate::input::InputActions>() {
                actions.update(&input_snapshot);
            }

            #[cfg(feature = "asset")]
            if let Some(asset_server) = world.get_resource::<crate::asset::AssetServer>().cloned() {
                if let Err(error) = asset_server.update() {
                    eprintln!("[SkyEngine] Asset update failed: {error}");
                }
            }

            if auto_tick {
                world.tick_with_frame_delta(dt, raw_dt);
            }
            let frame_dt = if auto_tick {
                world.time.frame_delta
            } else {
                dt
            };

            if exit_on_escape && input_snapshot.key_pressed(KeyCode::Escape) {
                rt.input.begin_frame();
                should_exit = true;
            } else {
                match rt.renderer.begin_frame() {
                    Ok(()) => {
                        let mut exit_requested = false;
                        let mut redraw_requested = false;

                        {
                            let ctx = &mut FrameContext {
                                world,
                                input: &input_snapshot,
                                dt: frame_dt,
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

                        rt.window.pre_present_notify();
                        rt.renderer.end_frame();
                        rt.input.begin_frame();
                        if let Err(error) = write_new_diagnostics(
                            world,
                            &mut rt.diagnostic_cursor,
                            self.config.diagnostic_console,
                            &mut std::io::stderr(),
                        ) {
                            eprintln!("[SkyEngine] Diagnostic console write failed: {error}");
                        }

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

        if request_redraw {
            self.request_redraw();
        }
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

        let pipeline = self.pipeline.take();
        let mut renderer = match create_scene_renderer(window.clone(), self.config.vsync, pipeline)
        {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("[SkyEngine] Renderer initialization failed: {err}");
                event_loop.exit();
                return;
            }
        };

        let title = format!(
            "{} | {} ({})",
            self.config.title,
            renderer.adapter_name(),
            renderer.backend_name()
        );
        window.set_title(&title);

        let world = self.world.as_mut().expect("world must be present");

        // Insert Input resource into the World (updated in-place each frame).
        let input = Input::new();
        world.insert_resource(input);

        if !world.contains_resource::<crate::diagnostics::Diagnostics>() {
            world.insert_resource(crate::diagnostics::Diagnostics::default());
        }

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
        {
            let mut setup_ctx = SetupContext {
                world,
                renderer: renderer.as_mut(),
                window: &window,
            };
            self.app_state.setup(&mut setup_ctx);
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
            diagnostic_cursor: DiagnosticCursor::new(),
            last_frame_time: None,
            occluded: false,
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

        let should_request = match self.config.redraw_mode {
            RedrawMode::Continuous => self.can_draw(),
            RedrawMode::Reactive => self.pending_redraw && self.can_draw(),
        };

        if should_request {
            if let Some(rt) = self.runtime.as_ref() {
                if rt.renderer.backend_kind() == RenderBackendKind::Kajiya
                    && should_trace_kajiya_runner_frame(self.kajiya_debug_frames)
                {
                    eprintln!(
                        "[SkyEngine][App] request_redraw frame={} mode={:?}",
                        self.kajiya_debug_frames, self.config.redraw_mode
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
        let Some(rt) = self.runtime.as_mut() else {
            return;
        };

        #[cfg(feature = "egui")]
        let egui_consumed = rt
            .egui
            .as_mut()
            .is_some_and(|egui| egui.on_window_event(&rt.window, &event));

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

                let scale_factor = rt.window.scale_factor() as f32;
                update_input_from_window_event(&mut rt.input, &event, suppressed, scale_factor);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticEvent, DiagnosticSubsystem};

    #[test]
    fn app_defaults_to_no_installed_pipeline() {
        let app = App::new(AppConfig::new("test", 64, 64), World::new());
        assert!(app.pipeline.is_none());
    }

    #[test]
    fn with_render_pipeline_installs_the_supplied_pipeline() {
        let app = App::new(AppConfig::new("test", 64, 64), World::new()).with_render_pipeline(
            RenderPipelineAsset::builder()
                .add_feature(crate::render::SpriteFeature::unlit())
                .add_phase(crate::render::TransparentPhase::new())
                .build(),
        );
        assert!(app.pipeline.is_some());
    }

    #[test]
    fn with_render_pipeline_accepts_the_default_forward_2d_pipeline() {
        let app = App::new(AppConfig::new("test", 64, 64), World::new())
            .with_render_pipeline(RenderPipelineAsset::forward_2d());
        assert!(app.pipeline.is_some());
    }

    #[cfg(feature = "ui")]
    #[test]
    fn app_new_installs_ui_config_from_app_config() {
        let app = App::new(
            AppConfig::new("test", 64, 64).with_ui_config(crate::ui::UiConfig {
                load_system_fonts: false,
            }),
            World::new(),
        );

        assert!(
            !app.world
                .get_resource::<crate::ui::UiConfig>()
                .unwrap()
                .load_system_fonts
        );
    }

    #[test]
    fn render_pipeline_assets_advertise_backend_kind() {
        assert_eq!(
            RenderPipelineAsset::forward_2d().backend_kind(),
            crate::render::RenderBackendKind::Wgpu
        );
        assert_eq!(
            RenderPipelineAsset::forward_3d().backend_kind(),
            crate::render::RenderBackendKind::Wgpu
        );
        assert_eq!(
            RenderPipelineAsset::kajiya_3d().backend_kind(),
            crate::render::RenderBackendKind::Kajiya
        );
    }

    #[test]
    fn cursor_position_is_converted_to_logical_pixels() {
        let logical =
            physical_cursor_to_logical(winit::dpi::PhysicalPosition::new(300.0, 150.0), 1.5);
        assert_eq!(logical, [200.0, 100.0]);
    }

    #[test]
    fn diagnostic_console_bridge_writes_new_filtered_events() {
        let mut world = World::new();
        world.insert_resource(Diagnostics::default());
        let diagnostics = world
            .get_resource::<Diagnostics>()
            .expect("diagnostics should exist");
        diagnostics.report(
            DiagnosticEvent::info("engine.note", DiagnosticSubsystem::engine(), "note")
                .with_title("Note"),
        );
        diagnostics.report(
            DiagnosticEvent::warning("render.warning", DiagnosticSubsystem::render(), "warning")
                .with_title("Warning"),
        );
        diagnostics.report(
            DiagnosticEvent::error("asset.error", DiagnosticSubsystem::asset(), "error")
                .with_title("Error"),
        );

        let mut cursor = DiagnosticCursor::new();
        let mut output = Vec::new();
        let written = write_new_diagnostics(
            &world,
            &mut cursor,
            DiagnosticConsole::WarningsAndErrors,
            &mut output,
        )
        .expect("diagnostics should write to memory");

        let text = String::from_utf8(output).expect("diagnostics should be UTF-8");
        assert_eq!(written, 2);
        assert!(!text.contains("[SkyEngine][engine][info]"));
        assert!(text.contains("[SkyEngine][render][warning] Warning"));
        assert!(text.contains("[SkyEngine][asset][error] Error"));

        let mut second = Vec::new();
        let written = write_new_diagnostics(
            &world,
            &mut cursor,
            DiagnosticConsole::WarningsAndErrors,
            &mut second,
        )
        .expect("diagnostics should write to memory");
        assert_eq!(written, 0);
        assert!(second.is_empty());
    }
}
