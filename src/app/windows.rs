//! Auxiliary native window support for the app runner.

use std::sync::Arc;
use std::time::Instant;

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::input::raw::Input;
use crate::render::backend::{create_scene_renderer, SceneRendererError};
use crate::render::{SceneFrame, SceneRenderer};

/// Configuration for an auxiliary top-level application window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub modal: bool,
}

impl WindowConfig {
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            resizable: true,
            modal: false,
        }
    }

    #[inline]
    pub fn resizable(mut self, value: bool) -> Self {
        self.resizable = value;
        self
    }

    #[inline]
    pub fn modal(mut self, value: bool) -> Self {
        self.modal = value;
        self
    }
}

pub(crate) struct WindowRequest {
    pub(crate) config: WindowConfig,
    pub(crate) client: Box<dyn WindowClient>,
}

impl WindowRequest {
    pub(crate) fn new(config: WindowConfig, client: Box<dyn WindowClient>) -> Self {
        Self { config, client }
    }
}

/// Render/update client for an auxiliary native window.
pub trait WindowClient: 'static {
    /// Render this auxiliary window's content.
    ///
    /// The app runner has already acquired the surface frame before this is
    /// called, and will present it after this method returns.
    fn render(&mut self, ctx: WindowFrameContext<'_>);

    fn needs_redraw(&self) -> bool {
        false
    }
}

/// Per-frame context for an auxiliary window client.
pub struct WindowFrameContext<'a> {
    pub window: &'a Window,
    pub frame: &'a mut SceneFrame,
    /// Active renderer for this auxiliary window.
    ///
    /// The frame is already open; draw into it but do not call
    /// [`SceneRenderer::begin_frame`] or [`SceneRenderer::end_frame`] here.
    pub renderer: &'a mut dyn SceneRenderer,
    pub input: &'a Input,
    pub dt: f32,
}

pub(crate) struct WindowRuntime {
    config: WindowConfig,
    window: Arc<Window>,
    renderer: Box<dyn SceneRenderer>,
    input: Input,
    last_frame_time: Option<Instant>,
    occluded: bool,
    close_requested: bool,
    client: Box<dyn WindowClient>,
}

impl WindowRuntime {
    pub(crate) fn open(
        event_loop: &ActiveEventLoop,
        vsync: bool,
        request: WindowRequest,
    ) -> Option<Self> {
        let attrs = WindowAttributes::default()
            .with_title(&request.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                request.config.width.max(160) as f64,
                request.config.height.max(120) as f64,
            ))
            .with_resizable(request.config.resizable);
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("[SkyEngine] Failed to create auxiliary window: {error}");
                return None;
            }
        };
        let renderer = match create_scene_renderer(window.clone(), vsync, None) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("[SkyEngine] Failed to initialize auxiliary window renderer: {error}");
                return None;
            }
        };
        if request.config.modal {
            window.focus_window();
        }

        Some(Self {
            config: request.config,
            window,
            renderer,
            input: Input::new(),
            last_frame_time: None,
            occluded: false,
            close_requested: false,
            client: request.client,
        })
    }

    #[inline]
    pub(crate) fn id(&self) -> WindowId {
        self.window.id()
    }

    #[inline]
    pub(crate) fn is_modal(&self) -> bool {
        self.config.modal
    }

    #[inline]
    pub(crate) fn window(&self) -> &Arc<Window> {
        &self.window
    }

    #[inline]
    pub(crate) fn close_requested(&self) -> bool {
        self.close_requested
    }

    pub(crate) fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub(crate) fn handle_event(&mut self, event: WindowEvent, max_delta: f32) {
        let scale_factor = self.window.scale_factor() as f32;
        match event {
            WindowEvent::CloseRequested => {
                self.close_requested = true;
            }
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
                self.last_frame_time = None;
                self.window.request_redraw();
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                if !occluded {
                    self.window.request_redraw();
                }
            }
            WindowEvent::Focused(false) => {
                self.input.reset();
            }
            WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => {
                crate::app::input::update_from_window_event(
                    &mut self.input,
                    &event,
                    false,
                    scale_factor,
                );
                self.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.render(max_delta);
            }
            _ => {}
        }
    }

    fn render(&mut self, max_delta: f32) {
        if self.occluded || self.close_requested {
            return;
        }
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let now = Instant::now();
        let raw_dt = self
            .last_frame_time
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        let dt = raw_dt.min(max_delta);
        self.last_frame_time = Some(now);

        match self.renderer.begin_frame() {
            Ok(mut frame) => {
                self.client.render(WindowFrameContext {
                    window: &self.window,
                    frame: &mut frame,
                    renderer: self.renderer.as_mut(),
                    input: &self.input,
                    dt,
                });
                if !frame.pre_present_notified() {
                    self.window.pre_present_notify();
                    frame.mark_pre_present_notified();
                }
                self.renderer.end_frame(frame);
            }
            Err(SceneRendererError::Wgpu(crate::gpu::GpuError::SurfaceLost)) => {
                self.renderer.surface_lost();
                let size = self.window.inner_size();
                self.renderer.resize(size.width, size.height);
                self.last_frame_time = None;
                self.window.request_redraw();
            }
            Err(SceneRendererError::Wgpu(crate::gpu::GpuError::Timeout)) => {
                self.last_frame_time = None;
                self.window.request_redraw();
            }
            Err(SceneRendererError::Wgpu(crate::gpu::GpuError::Occluded)) => {
                self.last_frame_time = None;
            }
            Err(SceneRendererError::Wgpu(crate::gpu::GpuError::OutOfMemory)) => {
                self.close_requested = true;
            }
            Err(error) => {
                eprintln!("[SkyEngine] Auxiliary window render failed: {error}");
                self.last_frame_time = None;
            }
        }

        self.input.begin_frame();
        if !self.close_requested && self.client.needs_redraw() {
            self.window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_config_defaults_to_resizable_non_modal() {
        let config = WindowConfig::new("Inspector", 640, 420);
        assert_eq!(config.title, "Inspector");
        assert_eq!(config.width, 640);
        assert_eq!(config.height, 420);
        assert!(config.resizable);
        assert!(!config.modal);
    }

    #[test]
    fn window_config_builders_update_flags() {
        let config = WindowConfig::new("Tool", 320, 240)
            .resizable(false)
            .modal(true);
        assert!(!config.resizable);
        assert!(config.modal);
    }
}
