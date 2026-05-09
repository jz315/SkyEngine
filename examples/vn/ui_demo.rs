//! Native UI-backed galgame vertical slice using local example assets.
//!
//! ```bash
//! cargo run --example vn_ui_demo --features vn-ui
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::{EntityId, World};
use sky_engine::render::{
    Color, RenderPipelineAsset, RenderSettings, SpriteFeature, TransparentPhase,
};
use sky_engine::ui::{
    UiButton, UiEventKind, UiEvents, UiId, UiLength, UiNode, UiPanel, UiState, UiText,
};
use sky_engine::vn::{
    VnAction, VnPlaybackState, VnPlugin, VnResource, VnSaveStore, VnStatus, VnUiMode, YarnProject,
    YarnScript,
};

const CORRIDOR_CG: &str = "在校园走廊的长椅上坐着_4K_202604121803.png";
const ROOM_EDIT_CG: &str = "把图2_的_人换成图一的_202604112345.png";
const STANDING_POSE: &str = "图2人物站着半身照_202604142337.png";
const QUICK_SLOT: &str = "quick";

struct VnUiDemo {
    start: Instant,
    frame: u32,
    auto_timer: f32,
    skip_timer: f32,
    profile_frames: Option<u32>,
    last_ready_textures: usize,
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
        }
    }

    fn drive_playback(&mut self, ctx: &mut FrameContext) {
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

    fn sync_demo_hud(&mut self, ctx: &mut FrameContext) {
        sync_demo_hud(ctx.world);
    }

    fn sync_title_menu(&mut self, ctx: &mut FrameContext) {
        sync_title_menu(ctx.world);
    }

    fn apply_demo_hud_events(&mut self, ctx: &mut FrameContext) {
        apply_demo_hud_events(ctx.world);
    }

    fn apply_title_menu_events(&mut self, ctx: &mut FrameContext) {
        if apply_title_menu_events(ctx.world) == DemoTitleAction::Quit {
            ctx.request_exit();
        }
    }

    fn profile(&mut self, ctx: &mut FrameContext) {
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
}

impl AppState for VnUiDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        self.frame = self.frame.wrapping_add(1);
        self.drive_playback(ctx);
        self.sync_title_menu(ctx);
        self.sync_demo_hud(ctx);
        ctx.update_ui();
        self.apply_title_menu_events(ctx);
        self.apply_demo_hud_events(ctx);
        ctx.tick();
        ctx.render();
        ctx.render_ui();
        self.profile(ctx);
    }
}

#[derive(Clone, Debug, Default)]
struct DemoHudEntities {
    panel: Option<EntityId>,
    status: Option<EntityId>,
    auto: Option<EntityId>,
    skip: Option<EntityId>,
    save: Option<EntityId>,
    load: Option<EntityId>,
    backlog: Option<EntityId>,
    hide: Option<EntityId>,
    notice: Option<EntityId>,
    notice_timer: f32,
}

#[derive(Clone, Debug, Default)]
struct DemoTitleEntities {
    root: Option<EntityId>,
    title: Option<EntityId>,
    subtitle: Option<EntityId>,
    status: Option<EntityId>,
    start: Option<EntityId>,
    continue_button: Option<EntityId>,
    quit: Option<EntityId>,
}

fn sync_demo_hud(world: &mut World) {
    let surface = world
        .get_resource::<UiState>()
        .map(UiState::surface_size)
        .filter(|size| size[0] > 0.0 && size[1] > 0.0)
        .unwrap_or([1280.0, 720.0]);
    let width = 488.0_f32.min((surface[0] - 32.0).max(300.0));
    let x = (surface[0] - width - 18.0).max(16.0);
    let mode = world
        .get_resource::<VnResource>()
        .map(|vn| vn.ui().mode.clone())
        .unwrap_or(VnUiMode::Reading);
    let visible = matches!(mode, VnUiMode::Reading | VnUiMode::Debug);
    let playback = world
        .get_resource::<VnResource>()
        .map(|vn| vn.playback().clone())
        .unwrap_or_default();

    let mut entities = world
        .remove_resource::<DemoHudEntities>()
        .unwrap_or_default();
    let panel = ensure_hud_panel(world, &mut entities, x, width);
    ensure_hud_text(world, &mut entities, panel, &playback);
    ensure_hud_button(
        world,
        &mut entities.auto,
        panel,
        "vn.demo.auto",
        "Auto",
        [190.0, 10.0],
        playback.auto_mode,
    );
    ensure_hud_button(
        world,
        &mut entities.skip,
        panel,
        "vn.demo.skip",
        "Skip",
        [262.0, 10.0],
        playback.skip_mode,
    );
    ensure_hud_button(
        world,
        &mut entities.save,
        panel,
        "vn.demo.save",
        "Save",
        [334.0, 10.0],
        false,
    );
    ensure_hud_button(
        world,
        &mut entities.load,
        panel,
        "vn.demo.load",
        "Load",
        [406.0, 10.0],
        false,
    );
    ensure_hud_notice(world, &mut entities, panel);
    set_hud_visible(world, &entities, visible);
    world.insert_resource(entities);
}

fn ensure_hud_panel(
    world: &mut World,
    entities: &mut DemoHudEntities,
    x: f32,
    width: f32,
) -> EntityId {
    let entity = live_entity(world, entities.panel).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(width, 54.0).at(x, 18.0).z(130),
            UiPanel::new(Color::rgba8(10, 14, 22, 206)),
        ));
        entities.panel = Some(entity);
        entity
    });
    if let Some(node) = world.get_mut::<UiNode>(entity) {
        node.position = [x, 18.0];
        node.size = [UiLength::Px(width), UiLength::Px(54.0)];
    }
    entity
}

fn ensure_hud_text(
    world: &mut World,
    entities: &mut DemoHudEntities,
    parent: EntityId,
    playback: &VnPlaybackState,
) {
    let text = if playback.skip_mode {
        "SKIP"
    } else if playback.auto_mode {
        "AUTO"
    } else {
        "READ"
    };
    let entity = live_entity(world, entities.status).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(164.0, 30.0)
                .child_of(parent)
                .at(16.0, 13.0)
                .z(131)
                .input_transparent(),
            UiText::new("")
                .size(17.0)
                .color(Color::rgba8(220, 232, 244, 255)),
        ));
        entities.status = Some(entity);
        entity
    });
    if let Some(label) = world.get_mut::<UiText>(entity) {
        label.text = format!("{text}  Space");
    }
}

fn ensure_hud_notice(world: &mut World, entities: &mut DemoHudEntities, parent: EntityId) {
    let message = if entities.notice_timer > 0.0 {
        "Saved"
    } else {
        ""
    };
    let entity = live_entity(world, entities.notice).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(96.0, 24.0)
                .child_of(parent)
                .at(382.0, 54.0)
                .z(131)
                .input_transparent(),
            UiText::new("")
                .size(15.0)
                .color(Color::rgba8(210, 232, 244, 255)),
        ));
        entities.notice = Some(entity);
        entity
    });
    if let Some(text) = world.get_mut::<UiText>(entity) {
        text.text = message.to_owned();
    }
}

fn ensure_hud_button(
    world: &mut World,
    slot: &mut Option<EntityId>,
    parent: EntityId,
    id: &'static str,
    label: &'static str,
    position: [f32; 2],
    active: bool,
) {
    let entity = live_entity(world, *slot).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(62.0, 34.0)
                .id(UiId::new(id))
                .child_of(parent)
                .at(position[0], position[1])
                .z(131),
            UiButton::new(label),
        ));
        *slot = Some(entity);
        entity
    });
    if let Some(node) = world.get_mut::<UiNode>(entity) {
        node.position = position;
    }
    if let Some(button) = world.get_mut::<UiButton>(entity) {
        button.label = label.to_owned();
        button.normal_color = if active {
            Color::rgba8(90, 140, 190, 242)
        } else {
            Color::rgba8(34, 44, 60, 232)
        };
        button.hover_color = Color::rgba8(64, 88, 116, 244);
        button.pressed_color = Color::rgba8(22, 30, 42, 246);
        button.text_color = Color::rgba8(244, 248, 252, 255);
    }
}

fn set_hud_visible(world: &mut World, entities: &DemoHudEntities, visible: bool) {
    for entity in [
        entities.panel,
        entities.status,
        entities.auto,
        entities.skip,
        entities.save,
        entities.load,
        entities.backlog,
        entities.hide,
        entities.notice,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(node) = world.get_mut::<UiNode>(entity) {
            node.visible = visible;
            node.enabled = visible;
        }
    }
}

fn apply_demo_hud_events(world: &mut World) {
    let dt = world.time.delta;
    if let Some(mut entities) = world.remove_resource::<DemoHudEntities>() {
        entities.notice_timer = (entities.notice_timer - dt).max(0.0);
        world.insert_resource(entities);
    }

    let Some(events) = world.get_resource_mut::<UiEvents>() else {
        return;
    };
    let mut actions = Vec::new();
    let mut demo_actions = Vec::new();
    let mut retained = Vec::new();
    for event in events.drain() {
        let (vn_action, demo_action) = if event.kind == UiEventKind::Clicked {
            event
                .id
                .as_ref()
                .and_then(|id| match id.as_str() {
                    "vn.demo.auto" => Some((Some(VnAction::Auto), None)),
                    "vn.demo.skip" => Some((Some(VnAction::Skip), None)),
                    "vn.demo.save" => Some((None, Some(DemoSaveAction::Save))),
                    "vn.demo.load" => Some((None, Some(DemoSaveAction::Load))),
                    "vn.demo.backlog" => Some((Some(VnAction::Backlog), None)),
                    "vn.demo.hide" => Some((Some(VnAction::HideUi), None)),
                    _ => None,
                })
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        if let Some(action) = vn_action {
            actions.push(action);
        } else if let Some(action) = demo_action {
            demo_actions.push(action);
        } else {
            retained.push(event);
        }
    }
    for event in retained {
        events.push(event);
    }
    for action in actions {
        push_vn_action(world, action);
    }
    for action in demo_actions {
        match action {
            DemoSaveAction::Save => {
                if save_quick_slot(world).is_ok() {
                    flash_hud_notice(world);
                }
            }
            DemoSaveAction::Load => {
                if load_quick_slot(world).is_ok() {
                    flash_hud_notice(world);
                }
            }
        }
    }
}

fn live_entity(world: &World, entity: Option<EntityId>) -> Option<EntityId> {
    entity.filter(|entity| world.contains(*entity))
}

fn push_vn_action(world: &mut World, action: VnAction) {
    if let Some(vn) = world.get_resource_mut::<VnResource>() {
        vn.push_action(action);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoSaveAction {
    Save,
    Load,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DemoTitleAction {
    #[default]
    None,
    Quit,
}

fn sync_title_menu(world: &mut World) {
    let surface = world
        .get_resource::<UiState>()
        .map(UiState::surface_size)
        .filter(|size| size[0] > 0.0 && size[1] > 0.0)
        .unwrap_or([1280.0, 720.0]);
    let visible = world
        .get_resource::<VnResource>()
        .is_some_and(|vn| vn.ui().mode == VnUiMode::Title);
    let has_save = world
        .get_resource::<VnResource>()
        .is_some_and(|vn| vn.saves().get(QUICK_SLOT).is_some());

    let mut entities = world
        .remove_resource::<DemoTitleEntities>()
        .unwrap_or_default();
    let root = ensure_title_root(world, &mut entities, surface);
    ensure_title_text(world, &mut entities, root, surface);
    ensure_title_button(
        world,
        &mut entities.start,
        root,
        "vn.title.start",
        "Start",
        surface,
        0.0,
        true,
    );
    ensure_title_button(
        world,
        &mut entities.continue_button,
        root,
        "vn.title.continue",
        if has_save { "Continue" } else { "No Save" },
        surface,
        54.0,
        has_save,
    );
    ensure_title_button(
        world,
        &mut entities.quit,
        root,
        "vn.title.quit",
        "Quit",
        surface,
        108.0,
        true,
    );
    set_title_visible(world, &entities, visible);
    world.insert_resource(entities);
}

fn ensure_title_root(
    world: &mut World,
    entities: &mut DemoTitleEntities,
    surface: [f32; 2],
) -> EntityId {
    let entity = live_entity(world, entities.root).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(surface[0], surface[1]).at(0.0, 0.0).z(200),
            UiPanel::new(Color::rgba8(6, 10, 18, 232)),
        ));
        entities.root = Some(entity);
        entity
    });
    if let Some(node) = world.get_mut::<UiNode>(entity) {
        node.size = [UiLength::Px(surface[0]), UiLength::Px(surface[1])];
    }
    entity
}

fn ensure_title_text(
    world: &mut World,
    entities: &mut DemoTitleEntities,
    parent: EntityId,
    surface: [f32; 2],
) {
    let title_x = (surface[0] * 0.12).max(52.0);
    let title_y = (surface[1] * 0.22).max(96.0);
    let title = live_entity(world, entities.title).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(surface[0] * 0.72, 64.0)
                .child_of(parent)
                .at(title_x, title_y)
                .z(201)
                .input_transparent(),
            UiText::new("After School Promise")
                .size(42.0)
                .color(Color::rgba8(245, 248, 252, 255)),
        ));
        entities.title = Some(entity);
        entity
    });
    if let Some(node) = world.get_mut::<UiNode>(title) {
        node.position = [title_x, title_y];
        node.size = [UiLength::Px(surface[0] * 0.72), UiLength::Px(64.0)];
    }

    let subtitle = live_entity(world, entities.subtitle).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(surface[0] * 0.64, 32.0)
                .child_of(parent)
                .at(title_x + 4.0, title_y + 68.0)
                .z(201)
                .input_transparent(),
            UiText::new("SkyEngine Galgame Demo")
                .size(21.0)
                .color(Color::rgba8(178, 204, 226, 255)),
        ));
        entities.subtitle = Some(entity);
        entity
    });
    if let Some(node) = world.get_mut::<UiNode>(subtitle) {
        node.position = [title_x + 4.0, title_y + 68.0];
        node.size = [UiLength::Px(surface[0] * 0.64), UiLength::Px(32.0)];
    }

    let status_text = save_status_text(world);
    let status = live_entity(world, entities.status).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(surface[0] * 0.64, 32.0)
                .child_of(parent)
                .at(title_x + 4.0, title_y + 112.0)
                .z(201)
                .input_transparent(),
            UiText::new("")
                .size(17.0)
                .color(Color::rgba8(210, 224, 238, 255)),
        ));
        entities.status = Some(entity);
        entity
    });
    if let Some(text) = world.get_mut::<UiText>(status) {
        text.text = status_text;
    }
}

fn ensure_title_button(
    world: &mut World,
    slot: &mut Option<EntityId>,
    parent: EntityId,
    id: &'static str,
    label: &'static str,
    surface: [f32; 2],
    y_offset: f32,
    enabled: bool,
) {
    let width = 240.0_f32.min((surface[0] - 80.0).max(180.0));
    let x = (surface[0] * 0.12).max(52.0);
    let y = (surface[1] * 0.52).max(260.0) + y_offset;
    let entity = live_entity(world, *slot).unwrap_or_else(|| {
        let entity = world.spawn((
            UiNode::panel(width, 42.0)
                .id(UiId::new(id))
                .child_of(parent)
                .at(x, y)
                .z(201),
            UiButton::new(label),
        ));
        *slot = Some(entity);
        entity
    });
    if let Some(node) = world.get_mut::<UiNode>(entity) {
        node.position = [x, y];
        node.size = [UiLength::Px(width), UiLength::Px(42.0)];
    }
    if let Some(button) = world.get_mut::<UiButton>(entity) {
        button.label = label.to_owned();
        button.normal_color = if enabled {
            Color::rgba8(34, 48, 66, 236)
        } else {
            Color::rgba8(28, 32, 38, 196)
        };
        button.hover_color = Color::rgba8(66, 92, 122, 246);
        button.pressed_color = Color::rgba8(20, 28, 40, 250);
        button.text_color = if enabled {
            Color::rgba8(246, 249, 252, 255)
        } else {
            Color::rgba8(150, 160, 172, 255)
        };
    }
}

fn set_title_visible(world: &mut World, entities: &DemoTitleEntities, visible: bool) {
    for entity in [
        entities.root,
        entities.title,
        entities.subtitle,
        entities.status,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(node) = world.get_mut::<UiNode>(entity) {
            node.visible = visible;
            node.enabled = visible;
        }
    }

    for entity in [entities.start, entities.continue_button, entities.quit]
        .into_iter()
        .flatten()
    {
        let disabled_empty_save = world
            .get::<UiButton>(entity)
            .is_some_and(|button| button.label == "No Save");
        if let Some(node) = world.get_mut::<UiNode>(entity) {
            node.visible = visible;
            node.enabled = visible && !disabled_empty_save;
        }
    }
}

fn apply_title_menu_events(world: &mut World) -> DemoTitleAction {
    let Some(events) = world.get_resource_mut::<UiEvents>() else {
        return DemoTitleAction::None;
    };
    let mut title_actions = Vec::new();
    let mut retained = Vec::new();
    for event in events.drain() {
        let action = if event.kind == UiEventKind::Clicked {
            event.id.as_ref().and_then(|id| match id.as_str() {
                "vn.title.start" => Some("start"),
                "vn.title.continue" => Some("continue"),
                "vn.title.quit" => Some("quit"),
                _ => None,
            })
        } else {
            None
        };
        if let Some(action) = action {
            title_actions.push(action);
        } else {
            retained.push(event);
        }
    }
    for event in retained {
        events.push(event);
    }

    let mut output = DemoTitleAction::None;
    for action in title_actions {
        match action {
            "start" => {
                reset_runtime_to_start(world);
                enter_reading_mode(world);
            }
            "continue" => {
                if load_quick_slot(world).is_ok() {
                    enter_reading_mode(world);
                }
            }
            "quit" => output = DemoTitleAction::Quit,
            _ => {}
        }
    }
    output
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

fn flash_hud_notice(world: &mut World) {
    let mut entities = world
        .remove_resource::<DemoHudEntities>()
        .unwrap_or_default();
    entities.notice_timer = 1.4;
    world.insert_resource(entities);
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

fn main() {
    let project = demo_project();
    let initial_window_size = project.manifest.resolution;

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: sky_engine::render::Color::rgb(0.02, 0.024, 0.032),
        ..Default::default()
    });
    VnPlugin::default()
        .install(&mut world)
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

    App::new(
        AppConfig::new(
            "SkyEngine Galgame - After School Promise",
            initial_window_size[0],
            initial_window_size[1],
        )
        .with_auto_tick(false),
        world,
    )
    .with_render_pipeline(
        RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build(),
    )
    .run(VnUiDemo::new());
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
旁白: 我把退社申请塞进书包最里层，正打算从走廊尽头溜走。 #line:start.narrator.0002
Alice: 找到了。你果然会选这条没人经过的路。 #line:start.alice.0001
我: 如果我说只是路过，你会相信吗？ #line:start.player.0001
Alice: 不信。你心虚的时候，会把书包带绕在手指上。 #line:start.alice.0002
<<wait 0.2>>
Alice: 今天社团要决定文化祭主题。你不来，我们的 Galgame 就只剩标题了。 #line:start.alice.0003
旁白: 她说“我们的”时，窗外的夕光正好落在她肩上。 #line:start.narrator.0003
-> 说出退社的事
    <<set $route = "honest">>
    <<jump HonestRoute>>
-> 假装只是忘了时间
    <<set $route = "gentle">>
    <<jump GentleRoute>>
-> 提议先去看美术稿
    <<set $route = "art">>
    <<jump ArtRoute>>
===

title: HonestRoute
---
我: 其实我今天是来交退社申请的。 #line:honest.player.0001
<<move alice to="center">>
Alice: 嗯。我猜到了。 #line:honest.alice.0001
我: 那你还来堵我？ #line:honest.player.0002
Alice: 因为猜到和听你亲口说，是两件事。 #line:honest.alice.0002
Alice: 你不用一个人把企划、程序、剧本和大家的期待全背起来。 #line:honest.alice.0003
旁白: 她没有伸手抢走那张纸，只是站近了一点。 #line:honest.narrator.0001
-> 把申请书递给她
    <<set $ending = "leave">>
    Alice: 如果这是你认真想过的决定，我会替你好好收下。 #line:honest.leave.alice.0001
    <<jump Ending>>
-> 把申请书揉成一团
    <<set $ending = "stay">>
    Alice: 那就从最小的一幕开始。今晚只写两句台词，也算继续。 #line:honest.stay.alice.0001
    <<jump Ending>>
===

title: GentleRoute
---
我: 抱歉，刚刚在楼下买饮料，忘了时间。 #line:gentle.player.0001
Alice: 你连瓶子都没拿。 #line:gentle.alice.0001
我: 店员说今天卖完了。 #line:gentle.player.0002
Alice: 那我们去买新的。边走边聊。 #line:gentle.alice.0002
<<move alice to="center">>
旁白: 她没有戳破我，只把退路变成了一段同行的路。 #line:gentle.narrator.0001
Alice: 文化祭版本不需要很大。只要有一个让人想点下去的瞬间。 #line:gentle.alice.0003
-> 问她想写什么
    <<set $ending = "promise">>
    Alice: 写一个差点放弃的人，被另一个人拉回来的故事。 #line:gentle.promise.alice.0001
    <<jump Ending>>
-> 说自己可能做不到
    <<set $ending = "small_step">>
    Alice: 做不到完整的，就做今晚这一幕。你看，现在已经有开头了。 #line:gentle.small.alice.0001
    <<jump Ending>>
===

title: ArtRoute
---
我: 先去社办吧。我想看看新的美术稿。 #line:art.player.0001
Alice: 你每次逃跑前，都会先确认素材有没有备份。 #line:art.alice.0001
<<cg "{ROOM_EDIT_CG}" layer=40>>
旁白: 社办电脑还亮着。屏幕上的合成图，把陌生的房间照成了另一个世界。 #line:art.narrator.0001
Alice: 这张可以当回忆 CG。角色站进去以后，故事就有了重量。 #line:art.alice.0002
我: 也可能只是看起来像真的。 #line:art.player.0002
Alice: Galgame 本来就是这样吧。假的画面，装着真的心情。 #line:art.alice.0003
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
Alice: 明天我还是会把测试版发给你。不是催你，只是想让你看到我们做到了哪里。 #line:end.leave.alice.0001
<<elseif $ending == "stay">>
旁白: 被揉皱的纸团落进垃圾桶。故事没有突然变好，但它继续往下一行走。 #line:end.stay.narrator.0001
Alice: 欢迎回来，主程序。第一件事，把“点击继续”做得更像真正的 Galgame。 #line:end.stay.alice.0001
<<elseif $ending == "promise">>
旁白: 自动贩卖机吐出两罐温热的柠檬茶。她把其中一罐贴在我的掌心。 #line:end.promise.narrator.0001
Alice: 约好了。文化祭前，我们把这个故事做完。 #line:end.promise.alice.0001
<<elseif $ending == "small_step">>
旁白: 我们在走廊长椅坐下，把庞大的企划拆成今晚能完成的三件小事。 #line:end.small.narrator.0001
Alice: 先写标题画面，再写第一句台词。能做到这里，就已经不是零了。 #line:end.small.alice.0001
<<elseif $ending == "cg">>
旁白: 她站到画面中央。那一瞬间，我忽然明白所谓完成度，就是有人愿意相信它。 #line:end.cg.narrator.0001
Alice: 截图留好。以后回看，会知道我们是从这一幕开始认真起来的。 #line:end.cg.alice.0001
<<else>>
旁白: 走廊的灯一盏盏亮起，我们把社办门锁好，像给今天的剧情打上句号。 #line:end.corridor.narrator.0001
Alice: 明天见。下一次，不许在选择支前面存档逃跑。 #line:end.corridor.alice.0001
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
