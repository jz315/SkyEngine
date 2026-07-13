use super::backend::SereinUiBackend;
use super::config::{SereinUiConfig, SereinWindowConfig};
use super::plugin::install_serein_ui_backend;
use super::window::SereinAuxWindowClient;
use super::{Screen, SereinSkin, Ui};
use crate::asset::Assets;
use crate::ecs::World;

/// Queue a native child window driven by the serein UI runtime.
pub fn open_window(
    ctx: &mut crate::app::FrameContext<'_>,
    config: SereinWindowConfig,
    compose: impl FnMut(&mut Ui, Screen) + 'static,
) {
    let window_config =
        crate::app::windows::WindowConfig::new(config.title.clone(), config.width, config.height)
            .modal(config.modal);
    let asset_server = ctx.world.get_resource::<Assets>().cloned();
    ctx.windows().open(
        window_config,
        SereinAuxWindowClient::new(config, asset_server, compose),
    );
}

/// Register a named serein skin on the installed backend.
pub fn register_skin(world: &mut World, skin: SereinSkin) {
    if crate::ui::with_ui_backend_mut::<SereinUiBackend, _>(world, |_| ()).is_none() {
        install_serein_ui_backend(world, SereinUiConfig::default());
    }
    let _ = crate::ui::with_ui_backend_mut::<SereinUiBackend, _>(world, |backend| {
        backend.runtime_mut().register_skin(skin);
    });
}

/// Compose a Serein declarative UI for the current app frame.
pub fn compose<R>(
    ctx: &mut crate::app::FrameContext<'_>,
    f: impl FnOnce(&mut Ui, Screen) -> R,
) -> Option<R> {
    if crate::ui::with_ui_backend_mut::<SereinUiBackend, _>(ctx.world, |_| ()).is_none() {
        install_serein_ui_backend(ctx.world, SereinUiConfig::default());
    }
    let input = *ctx.input;
    let logical_surface_size = ctx.logical_view_size().to_array();
    let dt = ctx.dt;
    let output = {
        let mut ui = ctx.ui();
        ui.with_backend_mut::<SereinUiBackend, _>(|backend| {
            backend.begin_frame_snapshot(&input, logical_surface_size, dt);
            let mut output = None;
            backend.frame(|ui, screen| {
                output = Some(f(ui, screen));
            });
            output
        })
        .flatten()
    };
    let window = ctx.window;
    let _ = crate::ui::with_ui_backend_mut::<SereinUiBackend, _>(ctx.world, |backend| {
        backend.apply_platform_effects(Some(window));
    });
    output
}
