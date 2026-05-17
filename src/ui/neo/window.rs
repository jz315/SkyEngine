use super::config::NeoWindowConfig;
use super::input_bridge::{pointer_from_input, scroll_from_input};
use super::{KeyboardEvent, NeoRenderer, NeoRuntime, Screen, Ui};

type NeoWindowCompose = Box<dyn FnMut(&mut Ui, Screen) + 'static>;

pub(crate) struct NeoAuxWindowClient {
    runtime: NeoRuntime,
    renderer: Option<NeoRenderer>,
    config: NeoWindowConfig,
    compose: NeoWindowCompose,
}

impl NeoAuxWindowClient {
    pub(crate) fn new(
        config: NeoWindowConfig,
        compose: impl FnMut(&mut Ui, Screen) + 'static,
    ) -> Self {
        Self {
            runtime: NeoRuntime::new(config.page_id.clone()),
            renderer: None,
            config,
            compose: Box::new(compose),
        }
    }
}

impl crate::app::windows::WindowClient for NeoAuxWindowClient {
    fn render(&mut self, ctx: crate::app::windows::WindowFrameContext<'_>) {
        let size = ctx.window.inner_size();
        let scale = ctx.window.scale_factor() as f32;
        let screen = Screen {
            width: size.width as f32 / scale.max(0.01),
            height: size.height as f32 / scale.max(0.01),
        };

        self.runtime.update_events_and_timers(
            pointer_from_input(ctx.input),
            scroll_from_input(ctx.input),
            KeyboardEvent::default(),
            ctx.dt,
        );
        self.runtime
            .compose(screen.width, screen.height, |ui, screen| {
                (self.compose)(ui, screen);
            });
        self.runtime.tick_animations(ctx.dt);

        if let Some(gpu) = ctx.renderer.wgpu_mut() {
            {
                let mut frame = gpu.frame();
                let _pass = frame.begin_surface_pass(
                    "sky_neo_aux_window_clear",
                    Some(self.config.clear_color.to_wgpu()),
                );
            }
            if self.renderer.is_none()
                || !self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.matches_surface(gpu.surface_format()))
            {
                self.renderer = Some(NeoRenderer::new(gpu));
            }
            let draw_list = self.runtime.draw_list();
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.render(gpu, &draw_list, screen);
            }
            self.runtime.mark_rendered();
        }
    }

    fn needs_redraw(&self) -> bool {
        self.runtime.needs_render()
    }
}
