//! Application runner — winit event loop integration.
//!
//! # Lifecycle model
//!
//! The full lifecycle is driven by the [`AppLifecycle`] trait:
//!
//! ```text
//!   setup → resume → resize → frame* → suspend → … → resume → resize → frame* → shutdown
//!                                       ↑ surface_lost → (reconfigure) → resume
//! ```
//!
//! Simpler apps can use [`App::run`] or [`App::run_with_lifecycle`] which
//! internally adapt plain closures into the trait.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::config::AppConfig;
use crate::app::input::{Input, KeyCode};
use crate::ecs::World;
use crate::gpu::GpuContext;

/// Frame context passed to the user's update callback each frame.
pub struct FrameContext<'a> {
    /// The ECS world.
    pub world: &'a mut World,
    /// The GPU backend.
    pub gpu: &'a mut GpuContext,
    /// Input state for this frame.
    pub input: &'a Input,
    /// The application window.  Use `window.set_title()` to update the title
    /// bar (e.g. for FPS display).
    pub window: &'a winit::window::Window,
}

// ── AppLifecycle trait ──────────────────────────────────────────────────────

/// Complete lifecycle hooks for a SkyEngine application.
///
/// Implement this trait to get fine-grained control over the application's
/// response to window, surface, and GPU events.
///
/// All methods except [`setup`] and [`frame`] have default no-op
/// implementations, so simple apps only need to implement those two.
pub trait AppLifecycle: 'static {
    /// Called **once** after the GPU backend and ECS world are initialised.
    fn setup(&mut self, world: &mut World, gpu: &mut GpuContext);

    /// Called each time the GPU surface becomes available.
    ///
    /// On desktop this fires once right after `setup`.  On mobile platforms
    /// it fires every time the app returns from a suspended state.
    ///
    /// Use this to (re)create render graphs, pipelines, or other resources
    /// that depend on a valid surface.
    fn resume(&mut self, _world: &mut World, _gpu: &mut GpuContext) {}

    /// Called when the window/surface size changes.
    fn resize(
        &mut self,
        _world: &mut World,
        _gpu: &mut GpuContext,
        _old_size: [u32; 2],
        _new_size: [u32; 2],
    ) {
    }

    /// Called when the window is about to lose its surface.
    ///
    /// Release GPU resources that are surface-dependent (render targets,
    /// swapchain bind groups, etc.).
    fn suspend(&mut self, _world: &mut World, _gpu: &mut GpuContext) {}

    /// Called every frame with a [`FrameContext`].
    fn frame(&mut self, ctx: FrameContext<'_>);

    /// Called when the GPU surface is lost and has been reconfigured.
    ///
    /// The runner automatically reconfigures the surface; this callback is
    /// an opportunity to invalidate bind-group caches and similar state
    /// that captured the old surface's image handles.
    fn surface_lost(&mut self, _world: &mut World, _gpu: &mut GpuContext) {}

    /// Called exactly once before the process exits.
    fn shutdown(&mut self, _world: &mut World, _gpu: &mut GpuContext) {}
}

// ── Public entry points ─────────────────────────────────────────────────────

/// The main application entry point.
///
/// Provides three levels of API:
///
/// 1. [`run`] / [`run_with_world`] — simplest, setup + frame closure
/// 2. [`run_with_lifecycle`] / [`run_with_lifecycle_and_world`] — adds resize
///    and shutdown closures
/// 3. [`run_lifecycle`] / [`run_lifecycle_with_world`] — full [`AppLifecycle`]
///    trait
///
/// The `_with_world` variants accept a pre-populated [`World`] so that ECS
/// entities, resources, and systems can be set up *before* the event loop
/// starts.  This eliminates the need to wrap a separate `World` in
/// `Rc<RefCell<>>` when using the closure-based APIs.
pub struct App;

impl App {
    // ── Minimal closure API ─────────────────────────────────────────────

    /// Create the window, initialise the GPU backend and a **fresh** ECS
    /// world, call `setup`, then enter the main loop.
    pub fn run<S, F>(config: AppConfig, setup: S, frame: F)
    where
        S: FnOnce(&mut World, &mut GpuContext) + 'static,
        F: FnMut(FrameContext<'_>) + 'static,
    {
        Self::run_with_lifecycle(
            config,
            setup,
            frame,
            |_world, _gpu, _old_size, _new_size| {},
            |_world, _gpu| {},
        );
    }

    /// Like [`run`], but uses a **pre-populated** [`World`].
    ///
    /// The world is available in every lifecycle callback (`setup`, `frame`,
    /// `resize`, `shutdown`) via the `&mut World` / [`FrameContext::world`]
    /// parameter.
    pub fn run_with_world<S, F>(config: AppConfig, world: World, setup: S, frame: F)
    where
        S: FnOnce(&mut World, &mut GpuContext) + 'static,
        F: FnMut(FrameContext<'_>) + 'static,
    {
        Self::run_with_lifecycle_and_world(
            config,
            world,
            setup,
            frame,
            |_world, _gpu, _old_size, _new_size| {},
            |_world, _gpu| {},
        );
    }

    // ── Full closure API ────────────────────────────────────────────────

    /// Like [`run`], but also exposes resize and shutdown lifecycle hooks.
    pub fn run_with_lifecycle<S, F, R, T>(
        config: AppConfig,
        setup: S,
        frame: F,
        resize: R,
        shutdown: T,
    ) where
        S: FnOnce(&mut World, &mut GpuContext) + 'static,
        F: FnMut(FrameContext<'_>) + 'static,
        R: FnMut(&mut World, &mut GpuContext, [u32; 2], [u32; 2]) + 'static,
        T: FnOnce(&mut World, &mut GpuContext) + 'static,
    {
        Self::run_lifecycle(
            config,
            Self::make_closure_lifecycle(setup, frame, resize, shutdown),
        );
    }

    /// Like [`run_with_lifecycle`], but uses a **pre-populated** [`World`].
    pub fn run_with_lifecycle_and_world<S, F, R, T>(
        config: AppConfig,
        world: World,
        setup: S,
        frame: F,
        resize: R,
        shutdown: T,
    ) where
        S: FnOnce(&mut World, &mut GpuContext) + 'static,
        F: FnMut(FrameContext<'_>) + 'static,
        R: FnMut(&mut World, &mut GpuContext, [u32; 2], [u32; 2]) + 'static,
        T: FnOnce(&mut World, &mut GpuContext) + 'static,
    {
        Self::run_lifecycle_with_world(
            config,
            world,
            Self::make_closure_lifecycle(setup, frame, resize, shutdown),
        );
    }

    // ── Trait-based API ─────────────────────────────────────────────────

    /// Run with a full [`AppLifecycle`] implementation and a **fresh** world.
    ///
    /// The internal world is created when the GPU surface becomes available
    /// and passed to [`AppLifecycle::setup`].
    pub fn run_lifecycle(config: AppConfig, lifecycle: impl AppLifecycle) {
        Self::run_lifecycle_inner(config, None, lifecycle);
    }

    /// Run with a full [`AppLifecycle`] implementation and a
    /// **pre-populated** [`World`].
    ///
    /// The provided world is passed to [`AppLifecycle::setup`] and reused
    /// for all subsequent lifecycle callbacks.  This is the recommended
    /// entry point for applications that need to register ECS systems,
    /// resources, or entities before the event loop starts.
    pub fn run_lifecycle_with_world(config: AppConfig, world: World, lifecycle: impl AppLifecycle) {
        Self::run_lifecycle_inner(config, Some(world), lifecycle);
    }

    // ── Internal ────────────────────────────────────────────────────────

    fn run_lifecycle_inner(
        config: AppConfig,
        initial_world: Option<World>,
        lifecycle: impl AppLifecycle,
    ) {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        let mut handler = LifecycleHandler {
            config,
            lifecycle: Box::new(lifecycle),
            initial_world,
            state: None,
            setup_done: false,
            shutdown_done: false,
        };

        event_loop.run_app(&mut handler).expect("Event loop error");
    }

    fn make_closure_lifecycle<S, F, R, T>(
        setup: S,
        frame: F,
        resize: R,
        shutdown: T,
    ) -> impl AppLifecycle
    where
        S: FnOnce(&mut World, &mut GpuContext) + 'static,
        F: FnMut(FrameContext<'_>) + 'static,
        R: FnMut(&mut World, &mut GpuContext, [u32; 2], [u32; 2]) + 'static,
        T: FnOnce(&mut World, &mut GpuContext) + 'static,
    {
        struct ClosureLifecycle<S, F, R, T> {
            setup_fn: Option<S>,
            frame_fn: F,
            resize_fn: R,
            shutdown_fn: Option<T>,
        }

        impl<S, F, R, T> AppLifecycle for ClosureLifecycle<S, F, R, T>
        where
            S: FnOnce(&mut World, &mut GpuContext) + 'static,
            F: FnMut(FrameContext<'_>) + 'static,
            R: FnMut(&mut World, &mut GpuContext, [u32; 2], [u32; 2]) + 'static,
            T: FnOnce(&mut World, &mut GpuContext) + 'static,
        {
            fn setup(&mut self, world: &mut World, gpu: &mut GpuContext) {
                if let Some(f) = self.setup_fn.take() {
                    f(world, gpu);
                }
            }
            fn frame(&mut self, ctx: FrameContext<'_>) {
                (self.frame_fn)(ctx);
            }
            fn resize(
                &mut self,
                world: &mut World,
                gpu: &mut GpuContext,
                old_size: [u32; 2],
                new_size: [u32; 2],
            ) {
                (self.resize_fn)(world, gpu, old_size, new_size);
            }
            fn shutdown(&mut self, world: &mut World, gpu: &mut GpuContext) {
                if let Some(f) = self.shutdown_fn.take() {
                    f(world, gpu);
                }
            }
        }

        ClosureLifecycle {
            setup_fn: Some(setup),
            frame_fn: frame,
            resize_fn: resize,
            shutdown_fn: Some(shutdown),
        }
    }
}

// ── Internal state ──────────────────────────────────────────────────────────

/// Internal state created once the window is available.
struct AppState {
    window: Arc<Window>,
    gpu: GpuContext,
    world: World,
    input: Input,
}

struct LifecycleHandler {
    config: AppConfig,
    lifecycle: Box<dyn AppLifecycle>,
    initial_world: Option<World>,
    state: Option<AppState>,
    setup_done: bool,
    shutdown_done: bool,
}

impl ApplicationHandler for LifecycleHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            // Surface restored — fire resume + resize.
            let state = self.state.as_mut().unwrap();
            self.lifecycle.resume(&mut state.world, &mut state.gpu);
            let size = state.gpu.surface_size();
            self.lifecycle
                .resize(&mut state.world, &mut state.gpu, size, size);
            return;
        }

        // First resume — create window, backend, and world.
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

        let mut world = self.initial_world.take().unwrap_or_else(World::new);
        let input = Input::new();

        // One-time setup
        if !self.setup_done {
            self.lifecycle.setup(&mut world, &mut gpu);
            self.setup_done = true;
        }

        // Initial resume
        self.lifecycle.resume(&mut world, &mut gpu);

        self.state = Some(AppState {
            window,
            gpu,
            world,
            input,
        });
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            self.lifecycle.suspend(&mut state.world, &mut state.gpu);
        }
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
                let old_size = state.gpu.surface_size();
                state.gpu.resize_surface(size.width, size.height);
                let new_size = state.gpu.surface_size();
                self.lifecycle
                    .resize(&mut state.world, &mut state.gpu, old_size, new_size);
                state.window.request_redraw();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                    let key = KeyCode::from_winit(code);
                    match event.state {
                        ElementState::Pressed => state.input.key_down(key),
                        ElementState::Released => state.input.key_up(key),
                    }
                }
                if event.state == ElementState::Pressed {
                    if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) =
                        event.physical_key
                    {
                        event_loop.exit();
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

            WindowEvent::RedrawRequested => {
                match state.gpu.begin_frame() {
                    Ok(()) => {
                        self.lifecycle.frame(FrameContext {
                            world: &mut state.world,
                            gpu: &mut state.gpu,
                            input: &state.input,
                            window: &state.window,
                        });
                        state.gpu.end_frame();
                    }
                    Err(crate::gpu::GpuError::SurfaceLost) => {
                        // Surface was reconfigured by the backend.
                        // Notify the lifecycle so it can invalidate caches.
                        self.lifecycle
                            .surface_lost(&mut state.world, &mut state.gpu);
                        // Fire resume + resize so resources can be rebuilt.
                        self.lifecycle.resume(&mut state.world, &mut state.gpu);
                        let size = state.gpu.surface_size();
                        self.lifecycle
                            .resize(&mut state.world, &mut state.gpu, size, size);
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
        if self.shutdown_done {
            return;
        }
        self.shutdown_done = true;
        if let Some(mut state) = self.state.take() {
            self.lifecycle.shutdown(&mut state.world, &mut state.gpu);
        }
    }
}
