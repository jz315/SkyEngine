use super::backend::NeoUiBackend;
use super::config::{NeoUiConfig, NeoWindowConfig};
use super::plugin::install_neo_ui_backend;
use super::window::NeoAuxWindowClient;
use super::{NeoSkin, Screen, Ui};
use crate::asset::Assets;
use crate::ecs::World;

/// Queue a native child window driven by the neo UI runtime.
pub fn open_window(
    ctx: &mut crate::app::FrameContext<'_>,
    config: NeoWindowConfig,
    compose: impl FnMut(&mut Ui, Screen) + 'static,
) {
    let window_config =
        crate::app::windows::WindowConfig::new(config.title.clone(), config.width, config.height)
            .modal(config.modal);
    let asset_server = ctx.world.get_resource::<Assets>().cloned();
    ctx.windows().open(
        window_config,
        NeoAuxWindowClient::new(config, asset_server, compose),
    );
}

/// Register a named neo skin on the installed backend.
pub fn register_skin(world: &mut World, skin: NeoSkin) {
    if crate::ui::with_ui_backend_mut::<NeoUiBackend, _>(world, |_| ()).is_none() {
        install_neo_ui_backend(world, NeoUiConfig::default());
    }
    let _ = crate::ui::with_ui_backend_mut::<NeoUiBackend, _>(world, |backend| {
        backend.runtime_mut().register_skin(skin);
    });
}

/// Compose an EUI-NEO-style declarative UI for the current app frame.
pub fn compose<R>(
    ctx: &mut crate::app::FrameContext<'_>,
    f: impl FnOnce(&mut Ui, Screen) -> R,
) -> Option<R> {
    if crate::ui::with_ui_backend_mut::<NeoUiBackend, _>(ctx.world, |_| ()).is_none() {
        install_neo_ui_backend(ctx.world, NeoUiConfig::default());
    }
    let input = *ctx.input;
    let logical_surface_size = ctx.logical_view_size().to_array();
    let dt = ctx.dt;
    let mut ui = ctx.ui();
    ui.with_backend_mut::<NeoUiBackend, _>(|backend| {
        backend.begin_frame_snapshot(&input, logical_surface_size, dt);
        let mut output = None;
        backend.compose(|ui, screen| {
            output = Some(f(ui, screen));
        });
        output
    })
    .flatten()
}
