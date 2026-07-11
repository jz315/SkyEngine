//! egui integration — wgpu + winit bridge.
//!
//! This module is only compiled when the `egui` feature is enabled.

/// Encapsulates all egui state: winit event translation, egui context,
/// and wgpu renderer.
pub(crate) struct EguiIntegration {
    winit_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    /// Output from the most recent `run()` call, consumed in `end_frame()`.
    pending_output: Option<egui::FullOutput>,
}

impl EguiIntegration {
    /// Create the integration after the GPU and window are ready.
    pub fn new(
        window: &winit::window::Window,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let egui_ctx = egui::Context::default();
        let winit_state = egui_winit::State::new(
            egui_ctx,
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None, // theme
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        let renderer = egui_wgpu::Renderer::new(
            device,
            surface_format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: false,
                ..Default::default()
            },
        );

        Self {
            winit_state,
            renderer,
            pending_output: None,
        }
    }

    /// Feed a winit event to egui. Returns `true` if egui consumed it
    /// (meaning the engine should NOT process it).
    pub fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        let response = self.winit_state.on_window_event(window, event);
        response.consumed
    }

    /// Run the user's egui UI closure. Call this during the frame callback.
    ///
    /// The closure receives egui's root [`egui::Ui`] for this frame.
    pub fn run(&mut self, window: &winit::window::Window, ui_fn: impl FnMut(&mut egui::Ui)) {
        let raw_input = self.winit_state.take_egui_input(window);
        let output = self.winit_state.egui_ctx().run_ui(raw_input, ui_fn);
        self.pending_output = Some(output);
    }

    /// Whether egui currently wants exclusive keyboard input (e.g. text box focused).
    #[allow(dead_code)]
    pub fn wants_keyboard(&self) -> bool {
        self.winit_state.egui_ctx().egui_wants_keyboard_input()
    }

    /// Whether egui currently wants exclusive pointer/mouse input.
    #[allow(dead_code)]
    pub fn wants_pointer(&self) -> bool {
        self.winit_state.egui_ctx().egui_wants_pointer_input()
    }

    /// Tessellate and render egui's output onto the given surface view.
    ///
    /// Must be called after `run()` and before `gpu.end_frame()`.
    pub fn end_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
        window: &winit::window::Window,
    ) {
        let output = match self.pending_output.take() {
            Some(o) => o,
            None => return, // ctx.egui() was not called this frame
        };

        // Handle platform output (cursor, clipboard, etc.)
        self.winit_state
            .handle_platform_output(window, output.platform_output);

        // Update textures
        for (id, delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }

        // Tessellate
        let pixels_per_point = output.pixels_per_point;
        let paint_jobs = self
            .winit_state
            .egui_ctx()
            .tessellate(output.shapes, pixels_per_point);

        // Update buffers
        let size = window.inner_size();
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [size.width, size.height],
            pixels_per_point,
        };

        let _ = self
            .renderer
            .update_buffers(device, queue, encoder, &paint_jobs, &screen);

        // Render egui onto the surface (load existing content, don't clear)
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_render_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: surface_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load, // preserve existing content
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &paint_jobs, &screen);
        }

        // Free textures
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}
