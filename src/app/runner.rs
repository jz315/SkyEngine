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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::config::{AppConfig, RedrawMode};
use crate::asset::{AssetServer, Handle, TextureAsset};
use crate::diagnostics::{
    write_diagnostic_events, DiagnosticConsole, DiagnosticCursor, Diagnostics,
};
use crate::ecs::{Time, World};
use crate::gpu::GpuContext;
use crate::input::raw::{Input, KeyCode, MouseButton};
use crate::render::backend::{create_scene_renderer, SceneRendererError};
use crate::render::{
    RenderAssets, RenderBackendKind, RenderPipelineAsset, RenderRuntime, RenderStats,
    SceneRenderer, SharedRenderAssetCache, TextureReadiness,
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

fn save_requested_screenshots(renderer: &mut dyn SceneRenderer, requests: &mut Vec<PathBuf>) {
    if requests.is_empty() {
        return;
    }

    let Some(gpu) = renderer.wgpu_mut() else {
        for path in requests.drain(..) {
            eprintln!(
                "[SkyEngine] Screenshot request ignored for {}: active renderer does not support wgpu surface readback",
                path.display()
            );
        }
        return;
    };

    for path in requests.drain(..) {
        match gpu.capture_surface_screenshot_png(&path) {
            Ok(()) => eprintln!("[SkyEngine] Screenshot saved: {}", path.display()),
            Err(error) => eprintln!(
                "[SkyEngine] Screenshot failed for {}: {error}",
                path.display()
            ),
        }
    }
}

fn default_screenshot_path() -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!("skyengine-screenshot-{timestamp}.png"))
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

    /// Mutably access the installed wgpu [`RenderRuntime`] and GPU together.
    pub fn with_render_runtime_mut<R>(
        &mut self,
        f: impl FnOnce(&mut RenderRuntime, &mut GpuContext) -> R,
    ) -> Option<R> {
        let (render_runtime, gpu) = self.renderer.wgpu_render_runtime_parts_mut()?;
        Some(f(render_runtime, gpu))
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
    screenshot_requests: &'a mut Vec<PathBuf>,
    #[cfg(feature = "egui")]
    egui: &'a mut Option<crate::app::egui_integration::EguiIntegration>,
}

/// Short-lived UI facade for backend-neutral UI operations.
#[cfg(feature = "ui-core")]
pub struct UiFrame<'ctx, 'frame> {
    ctx: &'ctx mut FrameContext<'frame>,
}

impl<'a> FrameContext<'a> {
    /// Manually advance the ECS schedule for this frame.
    ///
    /// This is useful when an app wants to run frame-local work before systems,
    /// such as updating UI interaction state before domain input systems drain
    /// semantic actions.
    pub fn tick(&mut self) {
        self.world.tick_with_delta(self.dt);
    }

    /// Execute the installed render pipeline.
    pub fn render(&mut self) {
        self.renderer.render_world(self.world);
    }

    /// Return the current CPU/GPU readiness state for a texture handle.
    ///
    /// This is a non-blocking query. If the CPU asset is unloaded but known to
    /// the asset server, the query may request CPU loading and report
    /// [`TextureReadiness::CpuLoading`].
    pub fn texture_readiness(&mut self, handle: Handle<TextureAsset>) -> TextureReadiness {
        let asset_server = self.world.get_resource::<AssetServer>().cloned();
        self.ensure_render_asset_cache();
        self.world
            .get_resource::<SharedRenderAssetCache>()
            .expect("render asset cache should be installed")
            .borrow_mut()
            .texture_readiness(asset_server.as_ref(), handle)
    }

    /// Ensure a CPU-ready texture is queued for GPU upload, without waiting for it.
    pub fn request_texture_gpu(&mut self, handle: Handle<TextureAsset>) -> TextureReadiness {
        let Some(asset_server) = self.world.get_resource::<AssetServer>().cloned() else {
            return TextureReadiness::MissingCpu;
        };
        self.ensure_render_asset_cache();
        let cache = self
            .world
            .get_resource::<SharedRenderAssetCache>()
            .expect("render asset cache should be installed");
        let gpu = self.renderer.wgpu_mut().expect(
            "FrameContext::request_texture_gpu is only available for the wgpu render backend",
        );
        cache
            .borrow_mut()
            .request_texture_gpu(gpu, &asset_server, handle)
    }

    /// Wait for the CPU-side texture asset to finish loading/decoding.
    pub fn wait_texture_cpu(
        &mut self,
        handle: Handle<TextureAsset>,
        timeout: Duration,
    ) -> TextureReadiness {
        let Some(asset_server) = self.world.get_resource::<AssetServer>().cloned() else {
            return TextureReadiness::MissingCpu;
        };
        let deadline = Instant::now().checked_add(timeout);
        loop {
            if let Err(error) = asset_server.update() {
                eprintln!(
                    "[SkyEngine] Asset update failed while waiting for texture CPU data: {error}"
                );
            }
            let readiness = self.cpu_texture_readiness(&asset_server, handle);
            if !matches!(readiness, TextureReadiness::CpuLoading) || timeout.is_zero() {
                return readiness;
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return readiness;
            }
            std::thread::yield_now();
        }
    }

    /// Wait for a texture to become resident on the GPU.
    ///
    /// This advances the shared GPU texture queue on the current render thread.
    /// Normal rendering never calls this; use it only for explicit blocking
    /// transitions such as loading screens.
    pub fn wait_texture_gpu(
        &mut self,
        handle: Handle<TextureAsset>,
        timeout: Duration,
    ) -> TextureReadiness {
        let Some(asset_server) = self.world.get_resource::<AssetServer>().cloned() else {
            return TextureReadiness::MissingCpu;
        };
        let deadline = Instant::now().checked_add(timeout);
        loop {
            if let Err(error) = asset_server.update() {
                eprintln!(
                    "[SkyEngine] Asset update failed while waiting for texture GPU data: {error}"
                );
            }
            match self.cpu_texture_readiness(&asset_server, handle) {
                TextureReadiness::CpuReady
                | TextureReadiness::GpuQueued
                | TextureReadiness::GpuReady => {}
                readiness @ (TextureReadiness::MissingCpu | TextureReadiness::Failed) => {
                    return readiness;
                }
                TextureReadiness::CpuLoading => {
                    if timeout.is_zero()
                        || deadline.is_some_and(|deadline| Instant::now() >= deadline)
                    {
                        return TextureReadiness::CpuLoading;
                    }
                    std::thread::yield_now();
                    continue;
                }
            }

            self.ensure_render_asset_cache();
            let cache = self
                .world
                .get_resource::<SharedRenderAssetCache>()
                .expect("render asset cache should be installed");
            let gpu = self.renderer.wgpu_mut().expect(
                "FrameContext::wait_texture_gpu is only available for the wgpu render backend",
            );
            let remaining = deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(timeout);
            let readiness =
                cache
                    .borrow_mut()
                    .wait_texture_gpu(gpu, &asset_server, handle, remaining);
            if matches!(
                readiness,
                TextureReadiness::GpuReady
                    | TextureReadiness::MissingCpu
                    | TextureReadiness::Failed
            ) || timeout.is_zero()
            {
                return readiness;
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return readiness;
            }
            std::thread::yield_now();
        }
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

    /// Save the fully rendered surface frame to a PNG before it is presented.
    ///
    /// Requests are drained by the app runner after `update()` and any built-in
    /// overlay rendering, but before presenting the frame. The wgpu backend
    /// currently supports PNG screenshots for RGBA8/BGRA8 presentation formats.
    pub fn request_screenshot(&mut self, path: impl Into<PathBuf>) {
        self.screenshot_requests.push(path.into());
    }

    /// Save the current frame to a timestamped PNG under the system temp directory.
    pub fn request_screenshot_temp(&mut self) -> PathBuf {
        let path = default_screenshot_path();
        self.request_screenshot(path.clone());
        path
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
        self.renderer.wgpu_render_runtime_mut()?.feature_mut::<T>()
    }

    /// Mutably access a render feature and the GPU at the same time.
    pub fn with_feature_mut<T: 'static, R>(
        &mut self,
        f: impl FnOnce(&mut T, &mut GpuContext) -> R,
    ) -> Option<R> {
        let (render_runtime, gpu) = self.renderer.wgpu_render_runtime_parts_mut()?;
        let feature = render_runtime.feature_mut::<T>()?;
        Some(f(feature, gpu))
    }

    /// Mutably access the installed [`RenderRuntime`] and the GPU together.
    ///
    /// This is the escape hatch for runtime mesh/material setup that depends on
    /// both renderer-owned registries and a live [`GpuContext`].
    pub fn with_render_runtime_mut<R>(
        &mut self,
        f: impl FnOnce(&mut RenderRuntime, &mut GpuContext) -> R,
    ) -> Option<R> {
        let (render_runtime, gpu) = self.renderer.wgpu_render_runtime_parts_mut()?;
        Some(f(render_runtime, gpu))
    }

    /// Backend-neutral UI facade.
    ///
    /// This is the preferred shape for new pluggable UI backend work.
    /// Existing `update_ui` / `render_ui` calls remain available for the
    /// retained ECS UI path.
    #[cfg(feature = "ui-core")]
    pub fn ui(&mut self) -> UiFrame<'_, 'a> {
        UiFrame { ctx: self }
    }

    /// Update native retained UI layout and interaction state.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui-legacy")]
    pub fn update_ui(&mut self) {
        crate::ui::update_ui(self.world, self.input, self.logical_surface_size());
    }

    /// Update all installed game UI backends.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui-core")]
    pub fn update_ui_backends(&mut self) {
        let [width, height] = self.surface_size();
        crate::ui::update_ui_backends(
            self.world,
            Some(self.window),
            self.input,
            self.logical_surface_size(),
            [width as f32, height as f32],
        );
    }

    /// Render native retained UI on top of the current surface frame.
    ///
    /// Call this after `ctx.render()` for the common scene + overlay order.
    /// Requires `--features ui`.
    #[cfg(feature = "ui-legacy")]
    pub fn render_ui(&mut self) {
        let gpu = self
            .renderer
            .wgpu_mut()
            .expect("FrameContext::render_ui is only available for the wgpu render backend");
        crate::ui::render_ui(self.world, gpu);
    }

    /// Render all installed game UI backend overlays on top of the current surface frame.
    ///
    /// Call this after `ctx.render()` for the common scene + overlay order.
    /// Requires `--features ui`.
    #[cfg(feature = "ui-core")]
    pub fn render_ui_overlays(&mut self) {
        let gpu = self.renderer.wgpu_mut().expect(
            "FrameContext::render_ui_overlays is only available for the wgpu render backend",
        );
        if let Err(error) = crate::ui::render_ui_overlays(self.world, gpu) {
            eprintln!("[SkyEngine] UI overlay rendering failed: {error}");
        }
    }

    /// Access native UI state if it has been installed.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui-legacy")]
    pub fn ui_state(&self) -> Option<&crate::ui::UiState> {
        self.world.get_resource::<crate::ui::UiState>()
    }

    /// Returns true when any installed game UI backend wants pointer input.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui-core")]
    pub fn ui_wants_pointer(&self) -> bool {
        #[cfg(feature = "ui-legacy")]
        {
            crate::ui::ui_wants_pointer(self.world)
                || self
                    .ui_state()
                    .is_some_and(crate::ui::UiState::wants_pointer)
        }
        #[cfg(not(feature = "ui-legacy"))]
        {
            crate::ui::ui_wants_pointer(self.world)
        }
    }

    /// Returns true when any installed game UI backend wants keyboard input.
    ///
    /// Requires `--features ui`.
    #[cfg(feature = "ui-core")]
    pub fn ui_wants_keyboard(&self) -> bool {
        crate::ui::ui_wants_keyboard(self.world)
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

    fn ensure_render_asset_cache(&mut self) {
        if !self.world.contains_resource::<SharedRenderAssetCache>() {
            self.world
                .insert_resource(SharedRenderAssetCache::default());
        }
    }

    fn cpu_texture_readiness(
        &mut self,
        asset_server: &AssetServer,
        handle: Handle<TextureAsset>,
    ) -> TextureReadiness {
        self.ensure_render_asset_cache();
        self.world
            .get_resource::<SharedRenderAssetCache>()
            .expect("render asset cache should be installed")
            .borrow_mut()
            .texture_readiness(Some(asset_server), handle)
    }
}

#[cfg(feature = "ui-core")]
impl<'ctx, 'frame> UiFrame<'ctx, 'frame> {
    /// Update all installed game UI backends.
    pub fn update(&mut self) {
        self.ctx.update_ui_backends();
    }

    /// Render all installed game UI backend overlays.
    pub fn render_overlays(&mut self) {
        self.ctx.render_ui_overlays();
    }

    /// Returns true when any installed game UI backend wants pointer input.
    pub fn wants_pointer(&self) -> bool {
        self.ctx.ui_wants_pointer()
    }

    /// Returns true when any installed game UI backend wants keyboard input.
    pub fn wants_keyboard(&self) -> bool {
        self.ctx.ui_wants_keyboard()
    }

    /// Mutably access a concrete UI backend for the duration of a closure.
    pub fn with_backend_mut<B, R>(&mut self, f: impl FnOnce(&mut B) -> R) -> Option<R>
    where
        B: crate::ui::UiBackend,
    {
        crate::ui::with_ui_backend_mut(self.ctx.world, f)
    }

    /// Run a closure against the installed yakui backend when the
    /// `yakui-ui` feature is enabled.
    #[cfg(feature = "yakui-ui")]
    pub fn yakui<R>(&mut self, f: impl FnOnce(&mut crate::ui::YakuiBackend) -> R) -> Option<R> {
        self.with_backend_mut::<crate::ui::YakuiBackend, R>(f)
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

            #[cfg(feature = "video")]
            if let Some(video_server) = world.get_resource::<crate::video::VideoServer>().cloned() {
                if let Err(error) = video_server.apply_commands() {
                    eprintln!("[SkyEngine] Video command application failed: {error}");
                }
                video_server.update(frame_dt);
                if let Err(error) = video_server.sync_world(world) {
                    eprintln!("[SkyEngine] Video world sync failed: {error}");
                }
            }

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
                                screenshot_requests: &mut screenshot_requests,
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

                        save_requested_screenshots(rt.renderer.as_mut(), &mut screenshot_requests);
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

        if !world.contains_resource::<SharedRenderAssetCache>() {
            world.insert_resource(SharedRenderAssetCache::default());
        }

        #[cfg(feature = "asset")]
        if !world.contains_resource::<crate::asset::AssetServer>() {
            let config = crate::asset::AssetConfig::default().with_background_loading(true);
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
                    let config = crate::asset::AssetConfig::default().with_background_loading(true);
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

        #[cfg(feature = "video")]
        {
            let asset_server =
                if let Some(server) = world.get_resource::<crate::asset::AssetServer>().cloned() {
                    server
                } else {
                    let config = crate::asset::AssetConfig::default().with_background_loading(true);
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

            if !world.contains_resource::<crate::video::VideoServer>() {
                let video_server = crate::video::VideoServer::new(asset_server);
                let video_commands = video_server.commands();
                world.insert_resource(video_server);
                if !world.contains_resource::<crate::video::VideoCommands>() {
                    world.insert_resource(video_commands);
                }
            } else if !world.contains_resource::<crate::video::VideoCommands>() {
                if let Some(video_server) =
                    world.get_resource::<crate::video::VideoServer>().cloned()
                {
                    world.insert_resource(video_server.commands());
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
        let scale_factor = rt.window.scale_factor() as f32;

        #[cfg(feature = "ui-core")]
        let ui_consumed = self.world.as_mut().is_some_and(|world| {
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
        assert_eq!(
            RenderPipelineAsset::renderling_3d().backend_kind(),
            crate::render::RenderBackendKind::Renderling
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
