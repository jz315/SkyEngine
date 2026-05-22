//! Application runner — winit event loop integration.
//!
//! # Usage
//!
//! ```rust,no_run
//! use sky_engine::app::{App, FrameContext, InputPlugin, RenderPlugin, WindowPlugin};
//! use sky_engine::ecs::World;
//!
//! struct Game;
//!
//! impl sky_engine::app::AppState for Game {
//!     fn update(&mut self, ctx: &mut FrameContext) {
//!         ctx.render();
//!     }
//! }
//!
//! let mut world = World::new();
//! world.install(WindowPlugin::new("Hello", 960, 640)).unwrap();
//! world.install(InputPlugin).unwrap();
//! world.install(RenderPlugin::forward_2d()).unwrap();
//! App::new(world).run(Game);
//! ```

use winit::event_loop::EventLoop;
use winit::window::Window;

use crate::app::frame::FrameContext;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::logging::LogStore;
use crate::math::{LogicalSize, PhysicalSize};
use crate::render::{RenderAssets, RenderBackendKind, RenderRuntime, SceneRenderer};

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
/// use sky_engine::app::{App, AppState, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin};
/// use sky_engine::ecs::World;
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
/// let mut world = World::new();
/// world.install(WindowPlugin::new("My Game", 1280, 720)).unwrap();
/// world.install(InputPlugin).unwrap();
/// world.install(RenderPlugin::forward_2d()).unwrap();
/// App::new(world).run(MyGame);
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
    /// When the installed [`RunnerPlugin`](crate::app::RunnerPlugin) enables
    /// automatic ticking (the default), the ECS
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
    logs: &'a LogStore,
}

impl<'a> SetupContext<'a> {
    pub(in crate::app) fn new(
        world: &'a mut World,
        renderer: &'a mut dyn SceneRenderer,
        window: &'a Window,
        logs: &'a LogStore,
    ) -> Self {
        Self {
            world,
            renderer,
            window,
            logs,
        }
    }

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

    /// Current GPU surface size in physical pixels.
    #[inline]
    pub fn physical_surface_size(&self) -> PhysicalSize {
        PhysicalSize::from_array(self.renderer.surface_size())
    }

    /// Window scale factor used to convert physical pixels to logical pixels.
    #[inline]
    pub fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    /// Current window view size in logical pixels.
    #[inline]
    pub fn logical_view_size(&self) -> LogicalSize {
        self.physical_surface_size().to_logical(self.scale_factor())
    }

    /// Recent logs captured by the app-owned logger.
    #[inline]
    pub fn logs(&self) -> &LogStore {
        crate::logging::drain_logger(self.logs);
        self.logs
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

// ── App builder ─────────────────────────────────────────────────────────────

/// Application builder.
///
/// Create with [`App::new`], then call [`.run()`](App::run) with your
/// [`AppState`] implementation to enter the main loop.
///
/// # Examples
///
/// ```rust,no_run
/// use sky_engine::app::{App, AppState, FrameContext, InputPlugin, RenderPlugin, WindowPlugin};
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
/// let mut world = World::new();
/// world.install(WindowPlugin::new("Demo", 960, 640)).unwrap();
/// world.install(InputPlugin).unwrap();
/// world.install(RenderPlugin::pipeline(
///     RenderPipelineAsset::builder()
///         .add_feature(SpriteFeature::unlit())
///         .add_phase(TransparentPhase::new())
///         .build(),
/// )).unwrap();
/// App::new(world).run(MyApp);
/// ```
pub struct App {
    world: World,
}

impl App {
    /// Create a new application builder.
    ///
    /// The [`World`] is the centre of your application — spawn entities,
    /// register systems, and insert resources before calling `.run()`.
    pub fn new(world: World) -> Self {
        Self { world }
    }

    /// Enter the main loop.
    ///
    /// The `state` receives structured lifecycle callbacks via [`AppState`].
    ///
    /// This function does **not** return under normal operation.
    pub fn run<S: AppState>(self, state: S) {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        let mut handler = crate::app::lifecycle::RunnerHandler::new(self.world, Box::new(state));

        event_loop.run_app(&mut handler).expect("Event loop error");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::RenderPipelineAsset;

    #[test]
    fn app_defaults_to_no_installed_pipeline() {
        let app = App::new(World::new());
        assert!(!app.world.contains_resource::<RenderPipelineAsset>());
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
}
