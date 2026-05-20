//! Per-frame app context types.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use winit::window::Window;

use crate::asset::{Assets, Handle, TextureAsset};
use crate::ecs::{Time, World};
use crate::gpu::GpuContext;
use crate::input::raw::Input;
use crate::logging::LogStore;
use crate::render::{
    RenderAssets, RenderRuntime, RenderStats, SceneRenderer, SharedRenderAssetCache,
    TextureReadiness,
};

/// Per-frame context passed to [`crate::app::AppState::update`].
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
    /// includes `World::time.time_scale`. With automatic ticking disabled it
    /// is the runner-sampled clamped delta; manual ticks update `world.time`.
    pub dt: f32,

    pub(crate) renderer: &'a mut dyn SceneRenderer,
    pub(crate) window: &'a Window,
    pub(crate) exit_requested: &'a mut bool,
    pub(crate) redraw_requested: &'a mut bool,
    pub(crate) frame_rate_limit: &'a mut Option<f64>,
    pub(crate) logs: &'a LogStore,
    pub(crate) aux_window_requests: &'a mut Vec<crate::app::windows::WindowRequest>,
    pub(crate) screenshot_requests: &'a mut Vec<PathBuf>,
    #[cfg(feature = "egui")]
    pub(crate) egui: &'a mut Option<crate::app::egui_integration::EguiIntegration>,
}

/// Short-lived UI facade for backend-neutral UI operations.
#[cfg(feature = "ui-core")]
pub struct UiFrame<'ctx, 'frame> {
    pub(crate) ctx: &'ctx mut FrameContext<'frame>,
}

/// Short-lived facade for opening auxiliary native app windows.
pub struct Windows<'ctx, 'frame> {
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
    /// the assets, the query may request CPU loading and report
    /// [`TextureReadiness::CpuLoading`].
    pub fn texture_readiness(&mut self, handle: Handle<TextureAsset>) -> TextureReadiness {
        let asset_server = self.world.get_resource::<Assets>().cloned();
        let (_, render_assets) = self.render_asset_cache_parts();
        render_assets
            .borrow_mut()
            .texture_readiness(asset_server.as_ref(), &handle)
    }

    /// Ensure a CPU-ready texture is queued for GPU upload, without waiting for it.
    pub fn request_texture_gpu(&mut self, handle: Handle<TextureAsset>) -> TextureReadiness {
        let Some(asset_server) = self.world.get_resource::<Assets>().cloned() else {
            return TextureReadiness::MissingCpu;
        };
        let (gpu, render_assets) = self.render_asset_cache_parts();
        render_assets
            .borrow_mut()
            .request_texture_gpu(gpu, &asset_server, &handle)
    }

    /// Wait for the CPU-side texture asset to finish loading/decoding.
    pub fn wait_texture_cpu(
        &mut self,
        handle: Handle<TextureAsset>,
        timeout: Duration,
    ) -> TextureReadiness {
        let Some(asset_server) = self.world.get_resource::<Assets>().cloned() else {
            return TextureReadiness::MissingCpu;
        };
        let deadline = Instant::now().checked_add(timeout);
        loop {
            if let Err(error) = asset_server.update() {
                eprintln!(
                    "[SkyEngine] Asset update failed while waiting for texture CPU data: {error}"
                );
            }
            let readiness = self.cpu_texture_readiness(&asset_server, &handle);
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
        let Some(asset_server) = self.world.get_resource::<Assets>().cloned() else {
            return TextureReadiness::MissingCpu;
        };
        let deadline = Instant::now().checked_add(timeout);
        loop {
            if let Err(error) = asset_server.update() {
                eprintln!(
                    "[SkyEngine] Asset update failed while waiting for texture GPU data: {error}"
                );
            }
            match self.cpu_texture_readiness(&asset_server, &handle) {
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

            let (gpu, render_assets) = self.render_asset_cache_parts();
            let remaining = deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(timeout);
            let readiness =
                render_assets
                    .borrow_mut()
                    .wait_texture_gpu(gpu, &asset_server, &handle, remaining);
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

    /// Recent logs captured by the app-owned logger.
    #[inline]
    pub fn logs(&self) -> &LogStore {
        crate::logging::drain_logger(self.logs);
        self.logs
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
        let path = crate::app::screenshots::default_path();
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
        let Some((gpu, render_assets)) = self.renderer.wgpu_overlay_parts_mut() else {
            panic!("FrameContext::render_ui is only available for the wgpu render backend");
        };
        crate::ui::render_ui(self.world, gpu, render_assets);
    }

    /// Render all installed game UI backend overlays on top of the current surface frame.
    ///
    /// Call this after `ctx.render()` for the common scene + overlay order.
    /// Requires `--features ui`.
    #[cfg(feature = "ui-core")]
    pub fn render_ui_overlays(&mut self) {
        let Some((gpu, render_assets)) = self.renderer.wgpu_overlay_parts_mut() else {
            panic!(
                "FrameContext::render_ui_overlays is only available for the wgpu render backend"
            );
        };
        if let Err(error) = crate::ui::render_ui_overlays(self.world, gpu, render_assets) {
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
    /// This is primarily useful in [`crate::app::config::RedrawMode::Reactive`] applications
    /// when a UI animation or custom pacing logic needs to keep driving
    /// future frames.
    pub fn request_redraw(&mut self) {
        *self.redraw_requested = true;
    }

    /// Access auxiliary native window operations.
    pub fn windows(&mut self) -> Windows<'_, 'a> {
        Windows { ctx: self }
    }

    /// Set the runner frame-rate cap for subsequent redraws.
    ///
    /// Values at or below zero disable the cap.
    pub fn set_frame_rate_limit(&mut self, fps: f64) {
        *self.frame_rate_limit = (fps > 0.0).then_some(fps);
    }

    pub(crate) fn queue_window(
        &mut self,
        config: crate::app::windows::WindowConfig,
        client: Box<dyn crate::app::windows::WindowClient>,
    ) {
        self.aux_window_requests
            .push(crate::app::windows::WindowRequest::new(config, client));
        *self.redraw_requested = true;
    }

    /// Run an egui UI overlay.
    ///
    /// The closure receives the raw [`egui::Context`] — write standard egui
    /// code directly. The UI is rendered on top of the current surface
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

    fn render_asset_cache_parts(&mut self) -> (&mut GpuContext, &SharedRenderAssetCache) {
        let Some((gpu, render_assets)) = self.renderer.wgpu_overlay_parts_mut() else {
            panic!("texture GPU readiness is only available for the wgpu render backend");
        };
        let Some(render_assets) = render_assets else {
            panic!("texture GPU readiness requires RenderPlugin::pipeline(...)");
        };
        (gpu, render_assets)
    }

    fn cpu_texture_readiness(
        &mut self,
        asset_server: &Assets,
        handle: &Handle<TextureAsset>,
    ) -> TextureReadiness {
        let (_, render_assets) = self.render_asset_cache_parts();
        render_assets
            .borrow_mut()
            .texture_readiness(Some(asset_server), handle)
    }
}

impl<'ctx, 'frame> Windows<'ctx, 'frame> {
    /// Open an auxiliary native window rendered by a custom client.
    pub fn open(
        &mut self,
        config: crate::app::windows::WindowConfig,
        client: impl crate::app::windows::WindowClient,
    ) {
        self.ctx.queue_window(config, Box::new(client));
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
}
