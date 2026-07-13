use std::path::PathBuf;

use sky_engine::app::{AppState, FrameContext, SetupContext};
use sky_engine::render::{CameraMarker, MainCamera, Projection, RenderSettings, Transform};
use sky_engine::ui::serein::State;

use crate::locale;
use crate::model::{AppModel, Page};
use crate::theme;
use crate::view::{self, RuntimeInfo};

#[derive(Debug)]
pub struct SereinControlCenter {
    state: State<AppModel>,
    uptime_seconds: f32,
    frame_count: u64,
    screenshot: ScreenshotProbe,
}

impl Default for SereinControlCenter {
    fn default() -> Self {
        let mut model = AppModel::default();
        apply_screenshot_overrides(&mut model);
        Self {
            state: State::new(model),
            uptime_seconds: 0.0,
            frame_count: 0,
            screenshot: ScreenshotProbe::default(),
        }
    }
}

fn apply_screenshot_overrides(model: &mut AppModel) {
    if let Ok(page) = std::env::var("SKY_SEREIN_CONTROL_CENTER_PAGE") {
        model.page = match page.to_ascii_lowercase().as_str() {
            "tasks" | "task" => Page::Tasks,
            "settings" | "setting" => Page::Settings,
            _ => Page::Overview,
        };
    }

    if env_flag("SKY_SEREIN_CONTROL_CENTER_QUALITY_OPEN") {
        model.page = Page::Settings;
        model.quality_preset_open = true;
    }

    if let Ok(overlay) = std::env::var("SKY_SEREIN_CONTROL_CENTER_OVERLAY") {
        match overlay.to_ascii_lowercase().as_str() {
            "new-task" | "task-sheet" => {
                model.page = Page::Tasks;
                model.new_task_sheet_open = true;
                model.draft.title = "Check screenshot spacing".to_string();
            }
            "ship" | "ship-dialog" => {
                model.ship_dialog_open = true;
            }
            "toast" => {
                model.toast.visible = true;
                let (title, message) = locale::build_queued_toast(model.locale);
                model.toast.title = title.to_string();
                model.toast.message = message.to_string();
            }
            _ => {}
        }
    }
}

impl AppState for SereinControlCenter {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let shell = theme::resolve(self.state.read(|model| model.theme_mode));
        ctx.world.insert_resource(RenderSettings {
            clear_color: sky_engine::ui::serein::to_render_color(shell.background_bottom),
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

        let runtime = RuntimeInfo {
            uptime_seconds: self.uptime_seconds,
            frame_count: self.frame_count,
        };
        let ui_state = self.state.clone();

        sky_engine::ui::serein::compose(ctx, move |ui, screen| {
            let snapshot = ui_state.read(Clone::clone);
            view::render(ui, screen, &ui_state, &snapshot, runtime);
        });

        let snapshot = self.state.read(Clone::clone);
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
            path: std::env::var("SKY_SEREIN_SCREENSHOT_PATH")
                .ok()
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty()),
            frame: env_u32("SKY_SEREIN_SCREENSHOT_FRAME").unwrap_or(30),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_SEREIN_EXIT_AFTER_SCREENSHOT"),
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
