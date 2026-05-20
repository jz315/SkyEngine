//! Neo UI-backed galgame vertical slice using local example assets.
//!
//! ```bash
//! cargo run --example vn_ui_demo --features vn-ui
//! ```

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, RunnerPlugin, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, Color as RenderColor, MainCamera, Projection, RenderPipelineAsset,
    RenderSettings, SpriteFeature, Transform, TransparentPhase,
};
use sky_engine::ui::neo::{widgets, Color, HorizontalAlign, Screen, Ui};
use sky_engine::vn::{
    compose_vn_ui_with, VnAction, VnPlaybackState, VnPlugin, VnResource, VnSaveStore, VnStatus,
    VnUiComposeContext, VnUiMode, YarnProject, YarnScript,
};

const CORRIDOR_CG: &str = "vn/bg/corridor_day.png";
const ROOM_EDIT_CG: &str = "vn/cg/notebook_secret.png";
const STANDING_POSE: &str = "vn/characters/chen_anqi/neutral.png";
const QUICK_SLOT: &str = "quick";

struct VnUiDemo {
    start: Instant,
    frame: u32,
    auto_timer: f32,
    skip_timer: f32,
    profile_frames: Option<u32>,
    last_ready_textures: usize,
    notice_timer: f32,
    actions: DemoActionSink,
    screenshot: ScreenshotProbe,
    screenshot_state: Option<DemoScreenshotState>,
    screenshot_state_applied: bool,
}

impl VnUiDemo {
    fn new() -> Self {
        let profile_frames = std::env::var("SKY_VN_UI_DEMO_PROFILE_FRAMES")
            .ok()
            .and_then(|value| value.parse().ok());
        Self {
            start: Instant::now(),
            frame: 0,
            auto_timer: 0.0,
            skip_timer: 0.0,
            profile_frames,
            last_ready_textures: 0,
            notice_timer: 0.0,
            actions: DemoActionSink::default(),
            screenshot: ScreenshotProbe::default(),
            screenshot_state: DemoScreenshotState::from_env(),
            screenshot_state_applied: false,
        }
    }

    fn drive_playback(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx
            .world
            .get_resource::<VnResource>()
            .is_some_and(|vn| vn.ui().mode != VnUiMode::Reading)
        {
            return;
        }

        let playback = ctx
            .world
            .get_resource::<VnResource>()
            .map(|vn| vn.playback().clone())
            .unwrap_or_default();
        if !playback.auto_mode && !playback.skip_mode {
            self.auto_timer = 0.0;
            self.skip_timer = 0.0;
            return;
        }

        let status = ctx
            .world
            .get_resource::<VnResource>()
            .and_then(|vn| vn.runtime())
            .map(|runtime| {
                (
                    runtime.status().clone(),
                    runtime.dialogue().line_complete,
                    runtime.dialogue().visible_text().chars().count(),
                )
            });
        let Some((status, line_complete, visible_chars)) = status else {
            return;
        };
        if matches!(status, VnStatus::Choice | VnStatus::Ended) {
            self.auto_timer = 0.0;
            self.skip_timer = 0.0;
            return;
        }

        if playback.skip_mode {
            self.skip_timer += ctx.dt;
            if self.skip_timer >= 0.06 {
                self.skip_timer = 0.0;
                push_vn_action(ctx.world, VnAction::Advance);
            }
            return;
        }

        if playback.auto_mode {
            let target_delay = if matches!(status, VnStatus::Line) && !line_complete {
                0.25
            } else {
                (0.75 + visible_chars as f32 * 0.035).clamp(0.9, 2.6)
            };
            self.auto_timer += ctx.dt;
            if self.auto_timer >= target_delay {
                self.auto_timer = 0.0;
                push_vn_action(ctx.world, VnAction::Advance);
            }
        }
    }

    fn apply_demo_actions(&mut self, ctx: &mut FrameContext<'_>) {
        for action in self.actions.drain() {
            match action {
                DemoUiAction::Start => {
                    reset_runtime_to_start(ctx.world);
                    enter_reading_mode(ctx.world);
                }
                DemoUiAction::Continue => {
                    if load_quick_slot(ctx.world).is_ok() {
                        enter_reading_mode(ctx.world);
                    }
                }
                DemoUiAction::Quit => ctx.request_exit(),
                DemoUiAction::Save => {
                    if save_quick_slot(ctx.world).is_ok() {
                        self.notice_timer = 1.4;
                    }
                }
                DemoUiAction::Load => {
                    if load_quick_slot(ctx.world).is_ok() {
                        self.notice_timer = 1.4;
                    }
                }
            }
        }
    }

    fn profile(&mut self, ctx: &mut FrameContext<'_>) {
        let Some(profile_frames) = self.profile_frames else {
            return;
        };

        let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        let (ready, total) = ctx
            .world
            .get_resource::<VnResource>()
            .map(|vn| {
                (
                    vn.sprite_textures().ready_count(),
                    vn.sprite_textures().len(),
                )
            })
            .unwrap_or((0, 0));

        if self.frame == 1 {
            eprintln!("[SkyEngine][VN profile] first_frame_ms={elapsed_ms:.2}");
        }
        if ready != self.last_ready_textures {
            eprintln!(
                "[SkyEngine][VN profile] frame={} elapsed_ms={elapsed_ms:.2} textures_ready={ready}/{total}",
                self.frame
            );
            self.last_ready_textures = ready;
        }
        if self.frame >= profile_frames {
            eprintln!(
                "[SkyEngine][VN profile] done frames={} elapsed_ms={elapsed_ms:.2} textures_ready={ready}/{total}",
                self.frame
            );
            ctx.request_exit();
        }
    }

    fn apply_screenshot_state(&mut self, ctx: &mut FrameContext<'_>) {
        if self.screenshot_state_applied {
            return;
        }
        let Some(state) = self.screenshot_state else {
            return;
        };
        if !ctx
            .world
            .get_resource::<VnResource>()
            .is_some_and(|vn| vn.runtime().is_some())
        {
            return;
        }

        let applied = match state {
            DemoScreenshotState::Title => {
                if let Some(vn) = ctx.world.get_resource_mut::<VnResource>() {
                    vn.ui_mut().enter(VnUiMode::Title);
                }
                true
            }
            DemoScreenshotState::Reading => prepare_demo_line_state(ctx.world, VnUiMode::Reading),
            DemoScreenshotState::Choice => prepare_demo_choice_state(ctx.world),
            DemoScreenshotState::Notice => {
                if prepare_demo_line_state(ctx.world, VnUiMode::Reading) {
                    let _ = save_quick_slot(ctx.world);
                    self.notice_timer = 1.4;
                    true
                } else {
                    false
                }
            }
            DemoScreenshotState::Hidden => prepare_demo_line_state(ctx.world, VnUiMode::Hidden),
        };

        self.screenshot_state_applied = applied;
    }
}

impl AppState for VnUiDemo {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.frame = self.frame.wrapping_add(1);
        self.notice_timer = (self.notice_timer - ctx.dt).max(0.0);
        self.drive_playback(ctx);
        self.apply_screenshot_state(ctx);

        let snapshot = DemoUiSnapshot::from_world(ctx.world, self.notice_timer);
        let actions = self.actions.clone();
        compose_vn_ui_with(ctx, move |ui, screen, vn_ui| {
            draw_demo_overlays(ui, screen, vn_ui, &snapshot, &actions);
        });
        self.apply_demo_actions(ctx);

        ctx.tick();
        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        self.profile(ctx);
        ctx.request_redraw();
    }
}

#[derive(Clone, Debug)]
struct DemoUiSnapshot {
    mode: VnUiMode,
    playback: VnPlaybackState,
    has_save: bool,
    save_status: String,
    notice_visible: bool,
}

impl DemoUiSnapshot {
    fn from_world(world: &World, notice_timer: f32) -> Self {
        let mode = world
            .get_resource::<VnResource>()
            .map(|vn| vn.ui().mode.clone())
            .unwrap_or(VnUiMode::Reading);
        let playback = world
            .get_resource::<VnResource>()
            .map(|vn| vn.playback().clone())
            .unwrap_or_default();
        let has_save = world
            .get_resource::<VnResource>()
            .is_some_and(|vn| vn.saves().get(QUICK_SLOT).is_some());
        Self {
            mode,
            playback,
            has_save,
            save_status: save_status_text(world),
            notice_visible: notice_timer > 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct DemoActionSink {
    actions: Rc<RefCell<Vec<DemoUiAction>>>,
}

impl DemoActionSink {
    fn push(&self, action: DemoUiAction) {
        self.actions.borrow_mut().push(action);
    }

    fn drain(&self) -> Vec<DemoUiAction> {
        self.actions.borrow_mut().drain(..).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoUiAction {
    Start,
    Continue,
    Quit,
    Save,
    Load,
}

fn draw_demo_overlays(
    ui: &mut Ui,
    screen: Screen,
    vn_ui: &VnUiComposeContext,
    snapshot: &DemoUiSnapshot,
    demo_actions: &DemoActionSink,
) {
    if snapshot.mode == VnUiMode::Title {
        draw_title_menu(ui, screen, snapshot, demo_actions);
    }

    if matches!(snapshot.mode, VnUiMode::Reading | VnUiMode::Debug) {
        draw_demo_hud(ui, screen, vn_ui, snapshot, demo_actions);
    }
}

fn draw_title_menu(
    ui: &mut Ui,
    screen: Screen,
    snapshot: &DemoUiSnapshot,
    demo_actions: &DemoActionSink,
) {
    let title_x = (screen.width * 0.12).max(52.0);
    let title_y = (screen.height * 0.22).max(96.0);
    let button_w = 240.0_f32.min((screen.width - 80.0).max(180.0));
    let button_y = (screen.height * 0.52).max(260.0);

    ui.stack("vn.title")
        .position(0.0, 0.0)
        .size(screen.width, screen.height)
        .z(200)
        .content(|ui| {
            ui.rect("vn.title.backdrop")
                .fill()
                .color(Color::rgba8(6, 10, 18, 236))
                .on_click(|| {})
                .build();
            ui.text("vn.title.name")
                .position(title_x, title_y)
                .size((screen.width * 0.74).max(320.0), 62.0)
                .text("After School Promise")
                .font_size(42.0)
                .line_height(62.0)
                .color(Color::rgba8(245, 248, 252, 255))
                .build();
            ui.text("vn.title.subtitle")
                .position(title_x + 4.0, title_y + 68.0)
                .size((screen.width * 0.64).max(280.0), 30.0)
                .text("SkyEngine Galgame Demo")
                .font_size(21.0)
                .line_height(30.0)
                .color(Color::rgba8(178, 204, 226, 255))
                .build();
            ui.text("vn.title.status")
                .position(title_x + 4.0, title_y + 112.0)
                .size((screen.width * 0.64).max(280.0), 48.0)
                .text(snapshot.save_status.clone())
                .font_size(17.0)
                .line_height(24.0)
                .wrap(true)
                .color(Color::rgba8(210, 224, 238, 255))
                .build();

            demo_button(
                ui,
                "vn.title.start",
                [title_x, button_y],
                [button_w, 42.0],
                "Start",
                true,
                {
                    let actions = demo_actions.clone();
                    move || actions.push(DemoUiAction::Start)
                },
            );
            demo_button(
                ui,
                "vn.title.continue",
                [title_x, button_y + 54.0],
                [button_w, 42.0],
                if snapshot.has_save {
                    "Continue"
                } else {
                    "No Save"
                },
                snapshot.has_save,
                {
                    let actions = demo_actions.clone();
                    move || actions.push(DemoUiAction::Continue)
                },
            );
            demo_button(
                ui,
                "vn.title.quit",
                [title_x, button_y + 108.0],
                [button_w, 42.0],
                "Quit",
                true,
                {
                    let actions = demo_actions.clone();
                    move || actions.push(DemoUiAction::Quit)
                },
            );
        });
}

fn draw_demo_hud(
    ui: &mut Ui,
    screen: Screen,
    vn_ui: &VnUiComposeContext,
    snapshot: &DemoUiSnapshot,
    demo_actions: &DemoActionSink,
) {
    let width = 488.0_f32.min((screen.width - 32.0).max(300.0));
    let x = (screen.width - width - 18.0).max(16.0);
    let mode = if snapshot.playback.skip_mode {
        "SKIP"
    } else if snapshot.playback.auto_mode {
        "AUTO"
    } else {
        "READ"
    };

    ui.stack("vn.demo.hud")
        .position(x, 18.0)
        .size(width, 82.0)
        .z(140)
        .content(|ui| {
            ui.rect("vn.demo.hud.bg")
                .size(width, 54.0)
                .radius(9.0)
                .color(Color::rgba8(10, 14, 22, 208))
                .border(1.0, Color::rgba8(140, 180, 215, 72))
                .shadow(18.0, 0.0, 8.0, Color::rgba8(0, 0, 0, 88))
                .build();
            ui.text("vn.demo.hud.status")
                .position(16.0, 13.0)
                .size(160.0, 30.0)
                .text(format!("{mode}  Space"))
                .font_size(17.0)
                .line_height(30.0)
                .color(Color::rgba8(220, 232, 244, 255))
                .build();

            hud_vn_button(
                ui,
                vn_ui,
                "vn.demo.auto",
                "Auto",
                [190.0, 10.0],
                snapshot.playback.auto_mode,
                VnAction::Auto,
            );
            hud_vn_button(
                ui,
                vn_ui,
                "vn.demo.skip",
                "Skip",
                [262.0, 10.0],
                snapshot.playback.skip_mode,
                VnAction::Skip,
            );
            hud_demo_button(
                ui,
                demo_actions,
                "vn.demo.save",
                "Save",
                [334.0, 10.0],
                DemoUiAction::Save,
            );
            hud_demo_button(
                ui,
                demo_actions,
                "vn.demo.load",
                "Load",
                [406.0, 10.0],
                DemoUiAction::Load,
            );

            if snapshot.notice_visible {
                ui.text("vn.demo.notice")
                    .position(width - 106.0, 58.0)
                    .size(96.0, 22.0)
                    .text("Saved")
                    .font_size(15.0)
                    .line_height(22.0)
                    .horizontal_align(HorizontalAlign::Right)
                    .color(Color::rgba8(210, 232, 244, 255))
                    .build();
            }
        });
}

fn demo_button(
    ui: &mut Ui,
    id: &'static str,
    position: [f32; 2],
    size: [f32; 2],
    label: &'static str,
    enabled: bool,
    on_click: impl FnMut() + 'static,
) {
    let normal = if enabled {
        Color::rgba8(34, 48, 66, 236)
    } else {
        Color::rgba8(28, 32, 38, 196)
    };
    ui.stack(format!("{id}.slot"))
        .position(position[0], position[1])
        .size(size[0], size[1])
        .content(|ui| {
            widgets::button(ui, id)
                .size(size[0], size[1])
                .text(label)
                .font_size(16.0)
                .radius(8.0)
                .enabled(enabled)
                .colors(
                    normal,
                    Color::rgba8(66, 92, 122, 246),
                    Color::rgba8(20, 28, 40, 250),
                )
                .text_color(if enabled {
                    Color::rgba8(246, 249, 252, 255)
                } else {
                    Color::rgba8(150, 160, 172, 255)
                })
                .on_click(on_click)
                .build();
        });
}

fn hud_vn_button(
    ui: &mut Ui,
    vn_ui: &VnUiComposeContext,
    id: &'static str,
    label: &'static str,
    position: [f32; 2],
    active: bool,
    action: VnAction,
) {
    let sink = vn_ui.action_sink();
    hud_button(ui, id, label, position, active, move || sink.push(action));
}

fn hud_demo_button(
    ui: &mut Ui,
    demo_actions: &DemoActionSink,
    id: &'static str,
    label: &'static str,
    position: [f32; 2],
    action: DemoUiAction,
) {
    let actions = demo_actions.clone();
    hud_button(ui, id, label, position, false, move || actions.push(action));
}

fn hud_button(
    ui: &mut Ui,
    id: &'static str,
    label: &'static str,
    position: [f32; 2],
    active: bool,
    on_click: impl FnMut() + 'static,
) {
    ui.stack(format!("{id}.slot"))
        .position(position[0], position[1])
        .size(62.0, 34.0)
        .content(|ui| {
            widgets::button(ui, id)
                .size(62.0, 34.0)
                .text(label)
                .font_size(14.0)
                .radius(8.0)
                .colors(
                    if active {
                        Color::rgba8(90, 140, 190, 242)
                    } else {
                        Color::rgba8(34, 44, 60, 232)
                    },
                    Color::rgba8(64, 88, 116, 244),
                    Color::rgba8(22, 30, 42, 246),
                )
                .text_color(Color::rgba8(244, 248, 252, 255))
                .on_click(on_click)
                .build();
        });
}

fn push_vn_action(world: &mut World, action: VnAction) {
    if let Some(vn) = world.get_resource_mut::<VnResource>() {
        vn.push_action(action);
    }
}

fn reset_runtime_to_start(world: &mut World) {
    let Some(script) = world
        .get_resource::<VnResource>()
        .and_then(|vn| vn.runtime())
        .map(|runtime| runtime.script().clone())
    else {
        return;
    };
    match world
        .get_resource_mut::<VnResource>()
        .expect("VnPlugin should install VnResource")
        .load_script(script, "Start")
    {
        Ok(()) => {}
        Err(error) => eprintln!("[SkyEngine][VN demo] new game failed: {error}"),
    }
}

fn enter_reading_mode(world: &mut World) {
    if let Some(vn) = world.get_resource_mut::<VnResource>() {
        vn.ui_mut().enter(VnUiMode::Reading);
    }
}

fn save_quick_slot(world: &mut World) -> Result<(), String> {
    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return Err("VN resource is not installed".to_owned());
    };
    vn.save_slot(QUICK_SLOT)
        .map_err(|error| error.to_string())?;
    vn.saves()
        .save_to_path(demo_save_path())
        .map_err(|error| error.to_string())
}

fn load_quick_slot(world: &mut World) -> Result<(), String> {
    refresh_save_store_from_disk(world);
    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return Err("VN resource is not installed".to_owned());
    };
    vn.load_slot(QUICK_SLOT).map_err(|error| error.to_string())
}

fn refresh_save_store_from_disk(world: &mut World) {
    let path = demo_save_path();
    if !path.exists() {
        return;
    }
    match VnSaveStore::load_from_path(&path) {
        Ok(store) => {
            if let Some(vn) = world.get_resource_mut::<VnResource>() {
                *vn.saves_mut() = store;
            }
        }
        Err(error) => eprintln!("[SkyEngine][VN demo] save load failed: {error}"),
    }
}

fn save_status_text(world: &World) -> String {
    world
        .get_resource::<VnResource>()
        .and_then(|vn| vn.saves().get(QUICK_SLOT))
        .and_then(|save| save.preview_text.as_deref())
        .map(|preview| format!("Continue from: {preview}"))
        .unwrap_or_else(|| {
            "No save yet. Start a new game, then use Save in the top bar.".to_owned()
        })
}

fn demo_save_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("vn_ui_demo")
        .join("saves.toml")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoScreenshotState {
    Title,
    Reading,
    Choice,
    Notice,
    Hidden,
}

impl DemoScreenshotState {
    fn from_env() -> Option<Self> {
        let value = std::env::var("SKY_VN_UI_SCREENSHOT_STATE").ok()?;
        match value.trim().to_ascii_lowercase().as_str() {
            "title" => Some(Self::Title),
            "reading" => Some(Self::Reading),
            "choice" => Some(Self::Choice),
            "notice" | "save" | "load" => Some(Self::Notice),
            "hidden" => Some(Self::Hidden),
            _ => None,
        }
    }
}

fn prepare_demo_line_state(world: &mut World, mode: VnUiMode) -> bool {
    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return false;
    };
    vn.ui_mut().enter(VnUiMode::Reading);
    let prepared = {
        let Some(runtime) = vn.runtime_mut() else {
            return false;
        };

        let mut prepared = false;
        for _ in 0..64 {
            match runtime.status() {
                VnStatus::Line => {
                    runtime.dialogue_mut().complete_line();
                    prepared = true;
                    break;
                }
                VnStatus::Ready => {
                    if runtime.advance().is_err() {
                        break;
                    }
                }
                VnStatus::Choice => {
                    prepared = true;
                    break;
                }
                VnStatus::Waiting => runtime.complete_wait(),
                VnStatus::Ended => break,
            }
        }
        prepared
    };
    if prepared {
        vn.ui_mut().enter(mode);
    }
    prepared
}

fn prepare_demo_choice_state(world: &mut World) -> bool {
    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return false;
    };
    vn.ui_mut().enter(VnUiMode::Reading);
    let Some(runtime) = vn.runtime_mut() else {
        return false;
    };

    for _ in 0..96 {
        match runtime.status() {
            VnStatus::Choice => return true,
            VnStatus::Line => {
                runtime.dialogue_mut().complete_line();
                if runtime.advance().is_err() {
                    return false;
                }
            }
            VnStatus::Ready => {
                if runtime.advance().is_err() {
                    return false;
                }
            }
            VnStatus::Waiting => runtime.complete_wait(),
            VnStatus::Ended => return false,
        }
    }
    false
}

#[derive(Debug)]
struct ScreenshotProbe {
    path: Option<String>,
    frame: u32,
    frame_count: u32,
    taken: bool,
    exit_after: bool,
}

impl Default for ScreenshotProbe {
    fn default() -> Self {
        Self {
            path: std::env::var("SKY_VN_UI_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_VN_UI_SCREENSHOT_FRAME").unwrap_or(60),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_VN_UI_EXIT_AFTER_SCREENSHOT"),
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

fn main() {
    let project = demo_project();
    let initial_window_size = project.manifest.resolution;

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: RenderColor::rgb(0.02, 0.024, 0.032),
        ..Default::default()
    });
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(
            initial_window_size[0] as f32,
            initial_window_size[1] as f32,
        ),
        MainCamera,
    ));
    world
        .install(VnPlugin::default())
        .expect("VN plugin should install");
    world
        .get_resource_mut::<VnResource>()
        .expect("VnPlugin should install VnResource")
        .set_asset_root(example_asset_root())
        .load_project(project)
        .expect("demo project should queue for loading");
    refresh_save_store_from_disk(&mut world);
    if let Some(vn) = world.get_resource_mut::<VnResource>() {
        vn.ui_mut().enter(VnUiMode::Title);
    }

    world
        .install(WindowPlugin::new(
            "SkyEngine Galgame - After School Promise",
            initial_window_size[0],
            initial_window_size[1],
        ))
        .unwrap();
    world
        .install(RunnerPlugin::game().with_auto_tick(false))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(VnUiDemo::new());
}

fn demo_project() -> YarnProject {
    let script = YarnScript::parse_str(&format!(
        r#"
title: Start
---
<<scene "{CORRIDOR_CG}" transition="fade" duration=0.8>>
<<play_bgm "audio/bgm/after_school.ogg" loop=true fade=1.2 volume=0.7>>
<<show alice "{STANDING_POSE}" at="right" layer=20 opacity=0.98>>
旁白: 四月最后一天的放学铃，像被雨洗过一样轻。 #line:start.narrator.0001
Alice: 找到了。你果然会选这条没人经过的路。 #line:start.alice.0001
我: 如果我说只是路过，你会相信吗？ #line:start.player.0001
Alice: 不信。你心虚的时候，会把书包带绕在手指上。 #line:start.alice.0002
-> 说出退社的事
    <<set $route = "honest">>
    <<jump HonestRoute>>
-> 提议先去看美术稿
    <<set $route = "art">>
    <<jump ArtRoute>>
===

title: HonestRoute
---
我: 其实我今天是来交退社申请的。 #line:honest.player.0001
<<move alice to="center">>
Alice: 你不用一个人把企划、程序、剧本和大家的期待全背起来。 #line:honest.alice.0001
-> 把申请书递给她
    <<set $ending = "leave">>
    Alice: 如果这是你认真想过的决定，我会替你好好收下。 #line:honest.leave.alice.0001
    <<jump Ending>>
-> 把申请书揉成一团
    <<set $ending = "stay">>
    Alice: 那就从最小的一幕开始。今晚只写两句台词，也算继续。 #line:honest.stay.alice.0001
    <<jump Ending>>
===

title: ArtRoute
---
我: 先去社办吧。我想看看新的美术稿。 #line:art.player.0001
<<cg "{ROOM_EDIT_CG}" layer=40>>
Alice: 这张可以当回忆 CG。角色站进去以后，故事就有了重量。 #line:art.alice.0001
-> 让她站到画面中央
    <<set $ending = "cg">>
    <<move alice to="center">>
    Alice: 如果要拍宣传截图，现在这个构图就很好。 #line:art.cg.alice.0001
    <<jump Ending>>
-> 关掉 CG 回到走廊
    <<set $ending = "corridor">>
    <<cg "{ROOM_EDIT_CG}" layer=0 opacity=0.0>>
    Alice: 那就回到最开始的地方，再选一次不逃跑的选项。 #line:art.corridor.alice.0001
    <<jump Ending>>
===

title: Ending
---
<<if $ending == "leave">>
旁白: 退社申请被她夹进文件夹。纸张合上的声音，比我想象中轻。 #line:end.leave.narrator.0001
<<elseif $ending == "stay">>
旁白: 被揉皱的纸团落进垃圾桶。故事没有突然变好，但它继续往下一行走。 #line:end.stay.narrator.0001
<<elseif $ending == "cg">>
旁白: 她站到画面中央。那一瞬间，我忽然明白所谓完成度，就是有人愿意相信它。 #line:end.cg.narrator.0001
<<else>>
旁白: 走廊的灯一盏盏亮起，我们把社办门锁好，像给今天的剧情打上句号。 #line:end.corridor.narrator.0001
<<endif>>
<<unlock_cg "corridor_promise">>
<<checkpoint "chapter_01_clear">>
旁白: Chapter 01 Clear。 #line:end.system.0001
===
"#
    ))
    .expect("demo script should parse");

    YarnProject::new("Start", script).expect("demo project should build")
}

fn example_asset_root() -> impl AsRef<Path> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sky_engine::vn::VnRuntimeEvent;

    #[test]
    fn demo_quick_save_restores_runtime_state() {
        let project = demo_project();
        let mut world = World::new();
        VnPlugin::default()
            .without_systems()
            .install(&mut world)
            .expect("plugin should install");
        world
            .get_resource_mut::<VnResource>()
            .unwrap()
            .set_asset_root(example_asset_root())
            .load_project(project)
            .unwrap();
        sky_engine::vn::vn_load_system(&mut world);

        {
            let runtime = world
                .get_resource_mut::<VnResource>()
                .unwrap()
                .runtime_mut()
                .unwrap();
            assert!(matches!(
                runtime.advance().unwrap(),
                VnRuntimeEvent::Command(_)
            ));
            assert!(matches!(
                runtime.advance().unwrap(),
                VnRuntimeEvent::Command(_)
            ));
            assert!(matches!(
                runtime.advance().unwrap(),
                VnRuntimeEvent::Command(_)
            ));
            assert!(matches!(
                runtime.advance().unwrap(),
                VnRuntimeEvent::Line(_)
            ));
        }

        save_quick_slot(&mut world).unwrap();

        {
            let runtime = world
                .get_resource_mut::<VnResource>()
                .unwrap()
                .runtime_mut()
                .unwrap();
            runtime.dialogue_mut().complete_line();
            assert!(matches!(
                runtime.advance().unwrap(),
                VnRuntimeEvent::Line(_)
            ));
        }
        assert_ne!(
            world
                .get_resource::<VnResource>()
                .unwrap()
                .runtime()
                .unwrap()
                .dialogue()
                .current_line
                .as_ref()
                .map(|line| line.line_id.as_deref()),
            Some(Some("start.narrator.0001"))
        );

        load_quick_slot(&mut world).unwrap();

        assert_eq!(
            world
                .get_resource::<VnResource>()
                .unwrap()
                .runtime()
                .unwrap()
                .dialogue()
                .current_line
                .as_ref()
                .map(|line| line.line_id.as_deref()),
            Some(Some("start.narrator.0001"))
        );
    }
}
