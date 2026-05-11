use crate::render::view::SceneView;
use std::sync::OnceLock;

pub(crate) fn render_debug_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SKY_RENDER_DEBUG_LOG")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

#[inline]
pub(crate) fn should_log_scene_view(scene_view: &SceneView) -> bool {
    render_debug_log_enabled() && scene_view.temporal.frame_index % 120 == 0
}
