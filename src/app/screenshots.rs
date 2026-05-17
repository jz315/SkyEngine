//! Screenshot request helpers for the app runner.

use std::path::PathBuf;

use crate::render::SceneRenderer;

pub(crate) fn save_requested(renderer: &mut dyn SceneRenderer, requests: &mut Vec<PathBuf>) {
    if requests.is_empty() {
        return;
    }

    let Some(gpu) = renderer.wgpu_mut() else {
        for path in requests.drain(..) {
            eprintln!(
                "[SkyEngine] Screenshot request ignored for {}: active renderer does not support wgpu surface readback",
                path.display()
            );
        }
        return;
    };

    for path in requests.drain(..) {
        match gpu.capture_surface_screenshot_png(&path) {
            Ok(()) => eprintln!("[SkyEngine] Screenshot saved: {}", path.display()),
            Err(error) => eprintln!(
                "[SkyEngine] Screenshot failed for {}: {error}",
                path.display()
            ),
        }
    }
}

pub(crate) fn default_path() -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!("skyengine-screenshot-{timestamp}.png"))
}
