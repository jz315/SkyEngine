use std::any::Any;

use crate::ecs::World;
use crate::plugin::{Plugin, PluginResult};

use super::super::{
    ensure_ui_host, UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError,
    UiEventContext, UiEventResponse, UiRenderContext,
};
use yakui_core::geometry::Rect;
use yakui_core::geometry::Vec2;

/// Plugin that installs the experimental yakui backend into [`UiHost`](crate::ui::UiHost).
#[derive(Clone, Copy, Debug, Default)]
pub struct YakuiUiPlugin;

impl Plugin for YakuiUiPlugin {
    fn name(&self) -> &'static str {
        "yakui-ui"
    }

    fn install(self, world: &mut World) -> PluginResult {
        install_yakui_backend_plugin(world);
        Ok(())
    }
}

/// Install the experimental yakui backend if it is not already present.
fn install_yakui_backend_plugin(world: &mut World) {
    ensure_ui_host(world);
    super::super::try_with_ui_host_mut(world, |host, _world| {
        if !host.contains(YakuiBackend::ID) {
            host.register(YakuiBackend::new());
        }
    });
}

/// Pluggable yakui UI backend.
pub struct YakuiBackend {
    state: yakui_core::Yakui,
    renderer: Option<yakui_wgpu::YakuiWgpu>,
    winit: Option<yakui_winit::YakuiWinit>,
    capture: UiCaptureState,
    pointer_captured: bool,
}

impl YakuiBackend {
    pub const ID: UiBackendId = UiBackendId::new("yakui");

    pub fn new() -> Self {
        Self {
            state: yakui_core::Yakui::new(),
            renderer: None,
            winit: None,
            capture: UiCaptureState::default(),
            pointer_captured: false,
        }
    }

    /// Run a yakui widget-building closure for the current frame.
    pub fn run(&mut self, ui: impl FnOnce()) {
        self.state.start();
        ui();
        self.state.finish();
        self.refresh_capture();
    }

    pub fn state(&self) -> &yakui_core::Yakui {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut yakui_core::Yakui {
        &mut self.state
    }

    fn refresh_capture(&mut self) {
        let keyboard = self.state.text_input_enabled();
        self.capture = UiCaptureState::new(self.pointer_captured, keyboard);
    }
}

impl Default for YakuiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBackend for YakuiBackend {
    fn id(&self) -> UiBackendId {
        Self::ID
    }

    fn name(&self) -> &'static str {
        "yakui"
    }

    fn handle_event(&mut self, ctx: UiEventContext<'_>) -> UiEventResponse {
        if let Some(window) = ctx.window {
            if self.winit.is_none() {
                self.winit = Some(yakui_winit::YakuiWinit::new(window));
            }
            let consumed = self
                .winit
                .as_mut()
                .expect("yakui winit adapter should exist")
                .handle_window_event(&mut self.state, ctx.event);
            self.pointer_captured = consumed || self.pointer_captured;
            self.refresh_capture();
            return if consumed {
                UiEventResponse::consumed()
            } else {
                UiEventResponse::ignored()
            };
        }
        UiEventResponse::ignored()
    }

    fn begin_frame(&mut self, ctx: UiBeginFrameContext<'_>) {
        let size = Vec2::new(
            ctx.physical_surface_size[0].max(1.0),
            ctx.physical_surface_size[1].max(1.0),
        );
        self.state.set_surface_size(size);
        self.state.set_scale_factor(ctx.scale_factor);
        self.state
            .set_unscaled_viewport(Rect::from_pos_size(Vec2::ZERO, size));
        self.pointer_captured = false;
        self.refresh_capture();
    }

    fn render_overlay(&mut self, ctx: UiRenderContext<'_>) -> Result<(), UiError> {
        if !ctx.gpu.has_surface() || !ctx.gpu.has_active_frame() {
            return Ok(());
        }
        if self.renderer.is_none() {
            self.renderer = Some(yakui_wgpu::YakuiWgpu::new(
                ctx.gpu.device(),
                ctx.gpu.queue(),
            ));
        }

        let surface_view = ctx.gpu.surface_view().clone();
        let device = ctx.gpu.device().clone();
        let queue = ctx.gpu.queue().clone();
        let format = ctx.gpu.surface_format();
        let surface = yakui_wgpu::SurfaceInfo {
            format,
            sample_count: 1,
            color_attachment: &surface_view,
            resolve_target: None,
        };
        let encoder = ctx.gpu.encoder();
        self.renderer
            .as_mut()
            .expect("yakui renderer should exist")
            .paint_with_encoder(&mut self.state, &device, &queue, encoder, surface);
        self.refresh_capture();
        Ok(())
    }

    fn capture(&self) -> UiCaptureState {
        self.capture
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
