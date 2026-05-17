//! Yakui convenience helpers.

use crate::app::FrameContext;

mod backend;

pub use backend::{YakuiBackend, YakuiUiPlugin};

/// Run a yakui frame for the current app update.
pub fn run<R>(
    ctx: &mut FrameContext<'_>,
    f: impl FnOnce(&mut crate::ui::YakuiBackend) -> R,
) -> Option<R> {
    let mut ui = ctx.ui();
    ui.with_backend_mut::<crate::ui::YakuiBackend, R>(f)
}
