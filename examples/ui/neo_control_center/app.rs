use std::path::PathBuf;

use sky_engine::app::{AppState, FrameContext, SetupContext};
use sky_engine::render::{CameraMarker, MainCamera, Projection, RenderSettings, Transform};
use sky_engine::ui::neo::NeoState;

use crate::locale;
use crate::model::AppModel;
use crate::theme;
use crate::view::{self, RuntimeInfo};

#[derive(Debug)]
pub struct NeoControlCenter {
    state: NeoState<AppModel>,
    uptime_seconds: f32,
    frame_count: u64,
    screenshot: ScreenshotProbe,
}

impl Default for NeoControlCenter {
    fn default() -> Self {
        Self {
            state: NeoState::new(AppModel::default()),
            uptime_seconds: 0.0,
            frame_count: 0,
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl AppState for NeoControlCenter {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let shell = theme::resolve(self.state.read(|model| model.theme_mode));
        ctx.world.insert_resource(RenderSettings {
            clear_color: shell.background_bottom.into(),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(920.0),
            MainCamera,
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.uptime_seconds += ctx.dt();
        self.frame_count = self.frame_count.saturating_add(1);

        let snapshot = self.state.read(Clone::clone);
        let runtime = RuntimeInfo {
            uptime_seconds: self.uptime_seconds,
            frame_count: self.frame_count,
        };
        let ui_state = self.state.clone();
        let view_snapshot = snapshot.clone();

        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            view::render(ui, screen, &ui_state, &view_snapshot, runtime);
        });

        ctx.set_title(&locale::window_title(
            snapshot.locale,
            &snapshot.project_name,
            snapshot.page,
        ));
        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
    }
}

#[derive(Debug)]
struct ScreenshotProbe {
    path: Option<PathBuf>,
    frame: u32,
    frame_count: u32,
    taken: bool,
    exit_after: bool,
}

impl Default for ScreenshotProbe {
    fn default() -> Self {
        Self {
            path: std::env::var("SKY_NEO_SCREENSHOT_PATH")
                .ok()
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty()),
            frame: env_u32("SKY_NEO_SCREENSHOT_FRAME").unwrap_or(30),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_NEO_EXIT_AFTER_SCREENSHOT"),
        }
    }
}

impl ScreenshotProbe {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        if !self.taken && self.frame_count >= self.frame {
            if let Some(path) = self.path.as_ref() {
                ctx.request_screenshot(path);
                self.taken = true;
                if self.exit_after {
                    ctx.request_exit();
                }
            }
        }
        self.frame_count = self.frame_count.saturating_add(1);
    }
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}
