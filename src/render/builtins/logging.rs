use std::sync::OnceLock;

use crate::render::view::SceneView;

fn render_debug_log_enabled() -> bool {
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

pub(super) fn should_log_scene_view(scene_view: &SceneView) -> bool {
    render_debug_log_enabled() && scene_view.temporal.frame_index.is_multiple_of(120)
}
