//! Application runner — winit event loop integration.
//!
//! # Usage
//!
//! ```rust,no_run
//! use sky_engine::app::{App, AppConfig};
//! use sky_engine::ecs::World;
//!
//! let world = World::new();
//! App::new(AppConfig::new("Hello", 960, 640), world)
//!     .run(|ctx| {
//!         ctx.world.tick();
//!         ctx.render();
//!     });
//! ```

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::config::AppConfig;
use crate::app::input::{Input, KeyCode};
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::renderer2d::{Renderer2D, Renderer2DConfig};
use crate::render::scene2d::Scene2D;
use crate::render::RendererStats;

// ── FrameContext ────────────────────────────────────────────────────────────

/// Per-frame context passed to the user's frame callback.
///
/// Provides access to the ECS world, input state, and rendering facilities.
///
/// # Quick start
///
/// ```rust,no_run
/// # use sky_engine::app::FrameContext;
/// fn frame(ctx: &mut FrameContext) {
///     ctx.world.tick();      // advance ECS systems
///     ctx.render();          // draw Camera/Sprite/Light entities
/// }
/// ```
pub struct FrameContext<'a> {
    /// The ECS world.  Spawn entities, run queries, tick systems — all here.
    pub world: &'a mut World,

    /// Input state for this frame (keyboard + mouse).
    ///
    /// Also available as a world resource via
    /// `world.get_resource::<Input>()`.
    pub input: &'a Input,

    /// Frame delta time in seconds.
    pub dt: f32,

    // ── Internal ────────────────────────────────────────────────────────
    gpu: &'a mut GpuContext,
    renderer: &'a mut Renderer2D,
    window: &'a Window,
    exit_requested: &'a mut bool,
    #[cfg(feature = "egui")]
    egui: &'a mut crate::app::egui_integration::EguiIntegration,
}

impl<'a> FrameContext<'a> {
    /// Render the current frame from ECS components.
    ///
    /// Extracts camera, sprites, and lights from the [`World`] and draws
    /// them using the internal [`Renderer2D`].
    pub fn render(&mut self) {
        self.renderer.render_world(self.gpu, self.world);
    }

    /// Render a manually constructed [`Scene2D`].
    pub fn render_scene(&mut self, scene: &Scene2D) {
        self.renderer.render_scene(self.gpu, scene);
    }

    /// Current surface size in physical pixels `[width, height]`.
    #[inline]
    pub fn surface_size(&self) -> [u32; 2] {
        self.gpu.surface_size()
    }

    /// Rendering statistics from the most recent `render()` / `render_scene()`.
    #[inline]
    pub fn render_stats(&self) -> RendererStats {
        self.renderer.stats()
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

    /// Request the application to exit after this frame.
    pub fn request_exit(&mut self) {
        *self.exit_requested = true;
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
/// Create with [`App::new`], optionally chain [`.setup()`](App::setup),
/// then call [`.run()`](App::run) to enter the main loop.
///
/// # Examples
///
/// ```rust,no_run
/// use sky_engine::app::{App, AppConfig};
/// use sky_engine::ecs::World;
///
/// // Minimal — no setup needed
/// App::new(AppConfig::new("Demo", 960, 640), World::new())
///     .run(|ctx| { ctx.render(); });
///
/// // With GPU setup for texture creation
/// let world = World::new();
/// App::new(AppConfig::new("Demo", 960, 640), world)
///     .setup(|world, gpu| {
///         // create textures, spawn GPU-dependent entities
///     })
///     .run(|ctx| {
///         ctx.world.tick();
///         ctx.render();
///     });
/// ```
pub struct App {
    config: AppConfig,
    world: World,
    setup: Option<Box<dyn FnOnce(&mut World, &mut GpuContext)>>,
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
            setup: None,
        }
    }

    /// Register a one-time setup callback that runs after the GPU is ready.
    ///
    /// Use this to create GPU-dependent resources like textures.
    /// Called exactly once, before the first frame.
    pub fn setup(mut self, f: impl FnOnce(&mut World, &mut GpuContext) + 'static) -> Self {
        self.setup = Some(Box::new(f));
        self
    }

    /// Enter the main loop.
    ///
    /// The `frame` closure is called once per frame with a [`FrameContext`]
    /// that provides access to the ECS world, input, and rendering.
    ///
    /// This function does **not** return under normal operation.
    pub fn run(self, frame: impl FnMut(&mut FrameContext) + 'static) {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        let mut handler = RunnerHandler {
            config: self.config,
            world: Some(self.world),
            setup: self.setup,
            frame: Box::new(frame),
            state: None,
        };

        event_loop.run_app(&mut handler).expect("Event loop error");
    }
}

// ── Internal state ──────────────────────────────────────────────────────────

struct AppState {
    window: Arc<Window>,
    gpu: GpuContext,
    input: Input,
    renderer: Renderer2D,
    last_frame_time: Option<std::time::Instant>,
    #[cfg(feature = "egui")]
    egui: crate::app::egui_integration::EguiIntegration,
}

struct RunnerHandler {
    config: AppConfig,
    world: Option<World>,
    setup: Option<Box<dyn FnOnce(&mut World, &mut GpuContext)>>,
    frame: Box<dyn FnMut(&mut FrameContext)>,
    state: Option<AppState>,
}

impl ApplicationHandler for RunnerHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            // Surface restored; the next RedrawRequested will handle it.
            return;
        }

        // First resume — create window, GPU backend, and renderer.
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

        // Determine render path from World resource (default: lit_hdr).
        let world = self.world.as_mut().expect("world must be present");
        let render_config = world
            .get_resource::<Renderer2DConfig>()
            .copied()
            .unwrap_or(Renderer2DConfig::lit_hdr());
        let renderer = Renderer2D::new(&gpu, render_config);

        // Run one-time setup.
        if let Some(setup) = self.setup.take() {
            setup(world, &mut gpu);
        }

        // Ensure Input resource exists in World for systems.
        let input = Input::new();
        world.insert_resource(input.clone());

        #[cfg(feature = "egui")]
        let egui = crate::app::egui_integration::EguiIntegration::new(
            &window,
            gpu.device(),
            gpu.surface_format(),
        );

        self.state = Some(AppState {
            window,
            gpu,
            input,
            renderer,
            last_frame_time: None,
            #[cfg(feature = "egui")]
            egui,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                state.gpu.resize_surface(size.width, size.height);
                state.renderer.resize(&state.gpu, size.width, size.height);
                state.window.request_redraw();
            }

            // Let egui process events first; if consumed, skip engine Input.
            ref ev
                if cfg!(feature = "egui")
                    && matches!(
                        ev,
                        WindowEvent::KeyboardInput { .. }
                            | WindowEvent::CursorMoved { .. }
                            | WindowEvent::MouseInput { .. }
                            | WindowEvent::MouseWheel { .. }
                    ) =>
            {
                #[cfg(feature = "egui")]
                {
                    if state.egui.on_window_event(&state.window, ev) {
                        return; // egui consumed it
                    }
                }
                // Not consumed — fall through to engine input handling.
                match ev {
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                            let key = KeyCode::from_winit(code);
                            match event.state {
                                ElementState::Pressed => state.input.key_down(key),
                                ElementState::Released => state.input.key_up(key),
                            }
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        state
                            .input
                            .set_mouse_position(position.x as f32, position.y as f32);
                    }
                    WindowEvent::MouseInput {
                        state: btn_state,
                        button,
                        ..
                    } => {
                        let idx = match button {
                            winit::event::MouseButton::Left => 0,
                            winit::event::MouseButton::Right => 1,
                            winit::event::MouseButton::Middle => 2,
                            _ => return,
                        };
                        match btn_state {
                            ElementState::Pressed => state.input.mouse_button_down(idx),
                            ElementState::Released => state.input.mouse_button_up(idx),
                        }
                    }
                    _ => {}
                }
            }

            #[cfg(not(feature = "egui"))]
            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                    let key = KeyCode::from_winit(code);
                    match event.state {
                        ElementState::Pressed => state.input.key_down(key),
                        ElementState::Released => state.input.key_up(key),
                    }
                }
            }

            #[cfg(not(feature = "egui"))]
            WindowEvent::CursorMoved { position, .. } => {
                state
                    .input
                    .set_mouse_position(position.x as f32, position.y as f32);
            }

            #[cfg(not(feature = "egui"))]
            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                let idx = match button {
                    winit::event::MouseButton::Left => 0,
                    winit::event::MouseButton::Right => 1,
                    winit::event::MouseButton::Middle => 2,
                    _ => return,
                };
                match btn_state {
                    ElementState::Pressed => state.input.mouse_button_down(idx),
                    ElementState::Released => state.input.mouse_button_up(idx),
                }
            }

            WindowEvent::RedrawRequested => {
                // Compute dt from wall clock.
                let now = std::time::Instant::now();
                let dt = state
                    .last_frame_time
                    .map(|t| now.duration_since(t).as_secs_f32())
                    .unwrap_or(1.0 / 60.0);
                state.last_frame_time = Some(now);

                let world = self.world.as_mut().expect("world must exist");

                // Sync Input snapshot to World resource for Systems.
                world.insert_resource(state.input.clone());

                match state.gpu.begin_frame() {
                    Ok(()) => {
                        let mut exit_requested = false;
                        {
                            let ctx = &mut FrameContext {
                                world,
                                input: &state.input,
                                dt,
                                gpu: &mut state.gpu,
                                renderer: &mut state.renderer,
                                window: &state.window,
                                exit_requested: &mut exit_requested,
                                #[cfg(feature = "egui")]
                                egui: &mut state.egui,
                            };
                            (self.frame)(ctx);
                        }

                        // Render egui overlay (after user frame, before present).
                        #[cfg(feature = "egui")]
                        {
                            let surface_view = state.gpu.surface_view().clone();
                            state.egui.end_frame(
                                state.gpu.device(),
                                state.gpu.queue(),
                                state.gpu.encoder(),
                                &surface_view,
                                &state.window,
                            );
                        }

                        state.gpu.end_frame();

                        if exit_requested {
                            event_loop.exit();
                        }
                    }
                    Err(crate::gpu::GpuError::SurfaceLost) => {
                        state.renderer.surface_lost();
                        let [w, h] = state.gpu.surface_size();
                        state.renderer.resize(&state.gpu, w, h);
                    }
                    Err(e) => {
                        eprintln!("[SkyEngine] GPU error: {e}");
                    }
                }

                state.input.begin_frame();
                state.window.request_redraw();
            }

            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // World, GPU, and renderer are dropped automatically.
    }
}
