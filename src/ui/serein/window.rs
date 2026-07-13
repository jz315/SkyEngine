use super::config::SereinWindowConfig;
use super::input_bridge::{pointer_from_input, scroll_from_input};
use super::renderer::SereinRenderer;
use super::{FrameInput, KeyboardEvent, Runtime, Screen, Ui};
use crate::asset::Assets;
use crate::render::SharedRenderAssetCache;

type SereinWindowCompose = Box<dyn FnMut(&mut Ui, Screen) + 'static>;

pub(crate) struct SereinAuxWindowClient {
    runtime: Runtime,
    renderer: Option<SereinRenderer>,
    asset_server: Option<Assets>,
    render_assets: SharedRenderAssetCache,
    config: SereinWindowConfig,
    compose: SereinWindowCompose,
}

impl SereinAuxWindowClient {
    pub(crate) fn new(
        config: SereinWindowConfig,
        asset_server: Option<Assets>,
        compose: impl FnMut(&mut Ui, Screen) + 'static,
    ) -> Self {
        Self {
            runtime: Runtime::new(config.page_id.clone()),
            renderer: None,
            asset_server,
            render_assets: SharedRenderAssetCache::default(),
            config,
            compose: Box::new(compose),
        }
    }
}

impl crate::app::windows::WindowClient for SereinAuxWindowClient {
    fn render(&mut self, ctx: crate::app::windows::WindowFrameContext<'_>) {
        let size = ctx.window.inner_size();
        let scale = ctx.window.scale_factor() as f32;
        let screen = Screen {
            width: size.width as f32 / scale.max(0.01),
            height: size.height as f32 / scale.max(0.01),
        };

        self.runtime.frame(
            FrameInput::new(screen, ctx.dt)
                .pointer(pointer_from_input(ctx.input))
                .scroll(scroll_from_input(ctx.input))
                .keyboard(KeyboardEvent::default()),
            |ui, screen| {
                (self.compose)(ui, screen);
            },
        );

        if let Some(gpu) = ctx.renderer.wgpu_mut() {
            {
                let mut frame = gpu.frame();
                let _pass = frame.begin_surface_pass(
                    "sky_serein_aux_window_clear",
                    Some(self.config.clear_color.to_wgpu()),
                );
            }
            if self.renderer.is_none()
                || !self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.matches_surface(gpu.surface_format()))
            {
                self.renderer = Some(SereinRenderer::new(gpu));
            }
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.render(
                    gpu,
                    &mut self.runtime,
                    self.asset_server.as_ref(),
                    Some(&self.render_assets),
                );
            }
            self.runtime.mark_rendered();
        }
    }

    fn needs_redraw(&self) -> bool {
        self.runtime.needs_render()
    }
}
