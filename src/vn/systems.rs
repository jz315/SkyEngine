use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::ecs::World;
use crate::vn::action::{VnAction, VnInputState, VnPlaybackState};
use crate::vn::loader::{VnLoadRequest, VnLoader};
#[cfg(feature = "app")]
use crate::vn::presentation::VnSpriteTextureMap;
use crate::vn::runtime::{VnRuntime, VnRuntimeEvent, VnRuntimeResult, VnStatus};
use crate::vn::script::{
    VnCommandArg, VnValue, YarnCommand, YarnInstruction, YarnProject, YarnScript,
};
use crate::vn::ui::{VnConfirmKind, VnUiMode, VnUiState};

#[cfg(feature = "app")]
use crate::asset::{AssetConfig, AssetServer};

#[cfg(feature = "app")]
use crate::input::{Input, InputActions, KeyCode, MouseButton};

#[cfg(feature = "app")]
const ACTION_ADVANCE: &str = "vn.advance";
#[cfg(feature = "app")]
const ACTION_CANCEL: &str = "vn.cancel";
#[cfg(feature = "app")]
const ACTION_UP: &str = "vn.up";
#[cfg(feature = "app")]
const ACTION_DOWN: &str = "vn.down";
#[cfg(feature = "app")]
const ACTION_MENU: &str = "vn.menu";
#[cfg(feature = "app")]
const ACTION_BACKLOG: &str = "vn.backlog";
#[cfg(feature = "app")]
const ACTION_AUTO: &str = "vn.auto";
#[cfg(feature = "app")]
const ACTION_SKIP: &str = "vn.skip";
#[cfg(feature = "app")]
const ACTION_HIDE_UI: &str = "vn.hide_ui";
#[cfg(feature = "app")]
const ACTION_QUICK_SAVE: &str = "vn.quick_save";
#[cfg(feature = "app")]
const ACTION_QUICK_LOAD: &str = "vn.quick_load";

#[derive(Clone, Debug, PartialEq)]
pub struct VnSystemConfig {
    pub auto_drain_runtime: bool,
    pub max_runtime_steps_per_tick: usize,
    pub reveal_chars_per_second: f32,
    pub skip_completes_lines: bool,
}

impl Default for VnSystemConfig {
    fn default() -> Self {
        Self {
            auto_drain_runtime: true,
            max_runtime_steps_per_tick: 24,
            reveal_chars_per_second: 48.0,
            skip_completes_lines: true,
        }
    }
}

pub fn vn_load_system(world: &mut World) {
    let asset_root = world
        .get_resource::<VnLoader>()
        .and_then(|loader| loader.asset_root().map(PathBuf::from));
    let pending = world
        .get_resource_mut::<VnLoader>()
        .and_then(VnLoader::take_pending);
    let Some(pending) = pending else {
        return;
    };

    match load_pending_vn(world, pending, asset_root) {
        Ok(image_count) => {
            if let Some(loader) = world.get_resource_mut::<VnLoader>() {
                loader.mark_loaded(image_count);
            }
        }
        Err(message) => {
            eprintln!("[SkyEngine][VN] load failed: {message}");
            if let Some(loader) = world.get_resource_mut::<VnLoader>() {
                loader.mark_failed(message);
            }
        }
    }
}

pub fn vn_input_system(world: &mut World) {
    #[cfg(feature = "app")]
    queue_vn_actions_from_app_input(world);

    let mut actions = Vec::new();
    if let Some(input) = world.get_resource_mut::<VnInputState>() {
        actions.extend(input.drain());
    }
    if actions.is_empty() {
        return;
    }

    apply_vn_actions(world, actions);
}

fn load_pending_vn(
    world: &mut World,
    pending: VnLoadRequest,
    asset_root: Option<PathBuf>,
) -> Result<usize, String> {
    let project = match pending {
        VnLoadRequest::Project(project) => project,
        VnLoadRequest::Script { script, start_node } => {
            YarnProject::new(start_node, script).map_err(|error| error.to_string())?
        }
    };
    let runtime = VnRuntime::from_project(project.clone()).map_err(|error| error.to_string())?;
    let root = asset_root.unwrap_or_else(|| project.root.clone());
    let image_count = prepare_vn_images(world, &project.script, root)?;

    world.insert_resource(project);
    world.insert_resource(runtime);
    Ok(image_count)
}

#[cfg(feature = "app")]
fn prepare_vn_images(
    world: &mut World,
    script: &YarnScript,
    asset_root: PathBuf,
) -> Result<usize, String> {
    let image_assets = collect_image_assets(script);
    if image_assets.is_empty() {
        world.insert_resource(VnSpriteTextureMap::default());
        return Ok(0);
    }

    let asset_server = match world.get_resource::<AssetServer>() {
        Some(assets) => assets.clone(),
        None => {
            let assets = AssetServer::with_empty_manifest(AssetConfig::default());
            world.insert_resource(assets.clone());
            assets
        }
    };

    let mut texture_map = VnSpriteTextureMap::default();
    for asset in &image_assets {
        texture_map
            .load_image_file(&asset_server, asset.clone(), asset_root.join(asset))
            .map_err(|error| error.to_string())?;
    }
    let image_count = image_assets.len();
    world.insert_resource(texture_map);
    Ok(image_count)
}

#[cfg(not(feature = "app"))]
fn prepare_vn_images(
    _world: &mut World,
    script: &YarnScript,
    _asset_root: PathBuf,
) -> Result<usize, String> {
    Ok(collect_image_assets(script).len())
}

fn collect_image_assets(script: &YarnScript) -> BTreeSet<String> {
    let mut assets = BTreeSet::new();
    for node in &script.nodes {
        collect_image_assets_from_instructions(&node.body, &mut assets);
    }
    assets
}

fn collect_image_assets_from_instructions(
    instructions: &[YarnInstruction],
    assets: &mut BTreeSet<String>,
) {
    for instruction in instructions {
        match instruction {
            YarnInstruction::Command(command) => collect_image_asset_from_command(command, assets),
            YarnInstruction::Choice(choice) => {
                collect_image_assets_from_instructions(&choice.body, assets);
            }
            YarnInstruction::Line(_) => {}
        }
    }
}

fn collect_image_asset_from_command(command: &YarnCommand, assets: &mut BTreeSet<String>) {
    match command.name.as_str() {
        "scene" | "bg" | "cg" => {
            if let Some(asset) = command.positional_args().next().and_then(image_arg_value) {
                assets.insert(asset.to_owned());
            }
        }
        "show" => {
            if let Some(asset) = command.named_arg("asset").and_then(image_arg_value) {
                assets.insert(asset.to_owned());
                return;
            }
            if let Some(asset) = command.positional_args().nth(1).and_then(image_arg_value) {
                assets.insert(asset.to_owned());
            }
        }
        _ => {}
    }
}

fn image_arg_value(arg: &VnCommandArg) -> Option<&str> {
    match &arg.value {
        VnValue::String(value) if !value.trim().is_empty() => Some(value.as_str()),
        _ => None,
    }
}

pub fn vn_script_system(world: &mut World) {
    let config = world
        .get_resource::<VnSystemConfig>()
        .cloned()
        .unwrap_or_default();
    let dt = world.time.delta;

    let Some(runtime) = world.get_resource_mut::<VnRuntime>() else {
        return;
    };

    runtime.tick(dt);
    if config.auto_drain_runtime {
        let _ = drain_runtime(runtime, config.max_runtime_steps_per_tick);
    }
    if runtime.status() == &VnStatus::Line && config.reveal_chars_per_second > 0.0 {
        let chars = (dt * config.reveal_chars_per_second).ceil().max(1.0) as usize;
        runtime.dialogue_mut().advance_reveal(chars);
    }
}

pub fn vn_ui_system(world: &mut World) {
    #[cfg(feature = "vn-ui")]
    {
        if let Err(error) = crate::vn::ui_binding::apply_vn_ui_events(world) {
            eprintln!("[SkyEngine][VN] UI event failed: {error}");
        }
    }

    #[cfg(all(feature = "app", not(feature = "vn-ui")))]
    crate::vn::presentation::sync_runtime_scene_to_world(world);

    #[cfg(not(any(feature = "app", feature = "vn-ui")))]
    let _ = world;
}

pub fn drain_runtime(
    runtime: &mut VnRuntime,
    max_steps: usize,
) -> VnRuntimeResult<Vec<VnRuntimeEvent>> {
    let mut events = Vec::new();
    for _ in 0..max_steps {
        match runtime.status() {
            VnStatus::Ready => match runtime.apply_action(VnAction::Advance)? {
                Some(VnRuntimeEvent::Command(command)) => {
                    events.push(VnRuntimeEvent::Command(command));
                    continue;
                }
                Some(VnRuntimeEvent::Wait(seconds)) => {
                    events.push(VnRuntimeEvent::Wait(seconds));
                    runtime.complete_wait();
                    continue;
                }
                Some(event) => {
                    events.push(event);
                    break;
                }
                None => break,
            },
            VnStatus::Waiting => {
                runtime.complete_wait();
            }
            VnStatus::Line | VnStatus::Choice | VnStatus::Ended => break,
        }
    }
    Ok(events)
}

fn apply_vn_actions(world: &mut World, actions: Vec<VnAction>) {
    for action in actions {
        match action {
            VnAction::Auto => {
                let mut playback = world
                    .remove_resource::<VnPlaybackState>()
                    .unwrap_or_default();
                playback.auto_mode = !playback.auto_mode;
                if playback.auto_mode {
                    playback.skip_mode = false;
                }
                world.insert_resource(playback);
            }
            VnAction::Skip => {
                let mut playback = world
                    .remove_resource::<VnPlaybackState>()
                    .unwrap_or_default();
                playback.skip_mode = !playback.skip_mode;
                if playback.skip_mode {
                    playback.auto_mode = false;
                }
                world.insert_resource(playback);
            }
            VnAction::HideUi => {
                toggle_ui_mode(world, VnUiMode::Hidden);
            }
            VnAction::Menu => {
                toggle_ui_mode(world, VnUiMode::Menu);
            }
            VnAction::Backlog => {
                toggle_ui_mode(world, VnUiMode::Backlog);
            }
            VnAction::QuickLoad => {
                enter_ui_mode(world, VnUiMode::Confirm(VnConfirmKind::QuickLoad));
            }
            VnAction::Cancel => {
                cancel_ui_mode(world);
            }
            VnAction::QuickSave => {
                // Full save persistence is wired through VnSaveStore. This action
                // is intentionally reserved here so bindings and UI can target it.
            }
            VnAction::Advance | VnAction::Confirm | VnAction::Up | VnAction::Down => {
                if let Some(runtime) = world.get_resource_mut::<VnRuntime>() {
                    if let Err(error) = runtime.apply_action(action) {
                        eprintln!("[SkyEngine][VN] action failed: {error}");
                    }
                }
            }
        }
    }
}

fn toggle_ui_mode(world: &mut World, mode: VnUiMode) {
    let mut ui = world.remove_resource::<VnUiState>().unwrap_or_default();
    if ui.mode == mode {
        ui.back();
    } else {
        ui.enter(mode);
    }
    world.insert_resource(ui);
}

fn enter_ui_mode(world: &mut World, mode: VnUiMode) {
    let mut ui = world.remove_resource::<VnUiState>().unwrap_or_default();
    ui.enter(mode);
    world.insert_resource(ui);
}

fn cancel_ui_mode(world: &mut World) {
    let mut ui = world.remove_resource::<VnUiState>().unwrap_or_default();
    if ui.mode == VnUiMode::Reading {
        ui.enter(VnUiMode::Menu);
    } else {
        ui.back();
    }
    world.insert_resource(ui);
}

#[cfg(feature = "app")]
fn queue_vn_actions_from_app_input(world: &mut World) {
    let actions = collect_vn_actions(world);
    if actions.is_empty() {
        return;
    }
    let Some(input) = world.get_resource_mut::<VnInputState>() else {
        return;
    };
    for action in actions {
        input.push(action);
    }
}

#[cfg(feature = "app")]
fn collect_vn_actions(world: &World) -> Vec<VnAction> {
    let mut actions = Vec::new();
    if let Some(input_actions) = world.get_resource::<InputActions>() {
        push_action_binding(
            &mut actions,
            input_actions,
            ACTION_ADVANCE,
            VnAction::Advance,
        );
        push_action_binding(&mut actions, input_actions, ACTION_CANCEL, VnAction::Cancel);
        push_action_binding(&mut actions, input_actions, ACTION_UP, VnAction::Up);
        push_action_binding(&mut actions, input_actions, ACTION_DOWN, VnAction::Down);
        push_action_binding(&mut actions, input_actions, ACTION_MENU, VnAction::Menu);
        push_action_binding(
            &mut actions,
            input_actions,
            ACTION_BACKLOG,
            VnAction::Backlog,
        );
        push_action_binding(&mut actions, input_actions, ACTION_AUTO, VnAction::Auto);
        push_action_binding(&mut actions, input_actions, ACTION_SKIP, VnAction::Skip);
        push_action_binding(
            &mut actions,
            input_actions,
            ACTION_HIDE_UI,
            VnAction::HideUi,
        );
        push_action_binding(
            &mut actions,
            input_actions,
            ACTION_QUICK_SAVE,
            VnAction::QuickSave,
        );
        push_action_binding(
            &mut actions,
            input_actions,
            ACTION_QUICK_LOAD,
            VnAction::QuickLoad,
        );
        if !actions.is_empty() {
            return actions;
        }
    }

    if let Some(input) = world.get_resource::<Input>() {
        if input.key_pressed(KeyCode::Enter)
            || input.key_pressed(KeyCode::Space)
            || input.mouse_button_pressed(MouseButton::Left)
        {
            actions.push(VnAction::Advance);
        }
        if input.key_pressed(KeyCode::ArrowUp) {
            actions.push(VnAction::Up);
        }
        if input.key_pressed(KeyCode::ArrowDown) {
            actions.push(VnAction::Down);
        }
        if input.key_pressed(KeyCode::Escape) || input.mouse_button_pressed(MouseButton::Right) {
            actions.push(VnAction::Cancel);
        }
        if input.key_pressed(KeyCode::Tab) {
            actions.push(VnAction::Skip);
        }
        if input.key_pressed(KeyCode::KeyA) {
            actions.push(VnAction::Auto);
        }
        if input.key_pressed(KeyCode::KeyH) {
            actions.push(VnAction::HideUi);
        }
        if input.key_pressed(KeyCode::KeyB) {
            actions.push(VnAction::Backlog);
        }
        if input.key_pressed(KeyCode::KeyS) {
            actions.push(VnAction::QuickSave);
        }
        if input.key_pressed(KeyCode::KeyL) {
            actions.push(VnAction::QuickLoad);
        }
    }
    actions
}

#[cfg(feature = "app")]
fn push_action_binding(
    actions: &mut Vec<VnAction>,
    input_actions: &InputActions,
    binding: &str,
    action: VnAction,
) {
    if input_actions.action_pressed(binding) {
        actions.push(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vn::script::YarnScript;
    use crate::vn::{VnLoaderStatus, VnPlugin};

    #[test]
    fn script_system_drains_commands_and_reveals_line() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<set $ready = true>>
Hello. #line:start.1
===
"#,
        )
        .unwrap();
        let mut world = World::new();
        world.insert_resource(VnRuntime::from_script(script, "Start").unwrap());
        world.insert_resource(VnSystemConfig {
            reveal_chars_per_second: 10.0,
            ..Default::default()
        });
        world.group("vn/script").add(vn_script_system);

        world.tick_with_delta(0.1);
        let runtime = world.get_resource::<VnRuntime>().unwrap();
        assert_eq!(runtime.status(), &VnStatus::Line);
        assert_eq!(runtime.dialogue().visible_text(), "H");
    }

    #[test]
    fn input_system_applies_pending_advance() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
Hello. #line:start.1
===
"#,
        )
        .unwrap();
        let mut world = World::new();
        world.insert_resource(VnRuntime::from_script(script, "Start").unwrap());
        world.insert_resource(VnInputState::default());
        world
            .get_resource_mut::<VnInputState>()
            .unwrap()
            .push(VnAction::Advance);

        vn_input_system(&mut world);

        assert_eq!(
            world.get_resource::<VnRuntime>().unwrap().status(),
            &VnStatus::Line
        );
    }

    #[test]
    fn ui_mode_actions_toggle_shell_state() {
        let mut world = World::new();
        world.insert_resource(VnInputState::default());
        world
            .get_resource_mut::<VnInputState>()
            .unwrap()
            .push(VnAction::Backlog);

        vn_input_system(&mut world);
        assert_eq!(
            world.get_resource::<VnUiState>().unwrap().mode,
            VnUiMode::Backlog
        );

        world
            .get_resource_mut::<VnInputState>()
            .unwrap()
            .push(VnAction::Cancel);
        vn_input_system(&mut world);
        assert_eq!(
            world.get_resource::<VnUiState>().unwrap().mode,
            VnUiMode::Reading
        );
    }

    #[test]
    fn load_system_builds_runtime_from_loader_request() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
Hello. #line:start.1
===
"#,
        )
        .unwrap();
        let mut world = World::new();
        VnPlugin::default().install(&mut world).unwrap();
        world
            .get_resource_mut::<VnLoader>()
            .unwrap()
            .load_script(script, "Start");

        world.tick_with_delta(0.016);

        assert!(world.contains_resource::<VnRuntime>());
        assert_eq!(
            world.get_resource::<VnLoader>().unwrap().status(),
            &VnLoaderStatus::Loaded { image_count: 0 }
        );
    }

    #[test]
    fn load_system_keeps_current_runtime_when_request_fails() {
        let original = YarnScript::parse_str(
            r#"
title: Start
---
Original. #line:start.1
===
"#,
        )
        .unwrap();
        let replacement = YarnScript::parse_str(
            r#"
title: Start
---
Replacement. #line:start.1
===
"#,
        )
        .unwrap();
        let mut world = World::new();
        VnPlugin::default().install(&mut world).unwrap();
        world.insert_resource(VnRuntime::from_script(original, "Start").unwrap());
        world
            .get_resource_mut::<VnLoader>()
            .unwrap()
            .load_script(replacement, "Missing");

        world.tick_with_delta(0.016);

        let runtime = world.get_resource::<VnRuntime>().unwrap();
        assert_eq!(
            runtime.dialogue().current_line.as_ref().unwrap().text,
            "Original."
        );
        assert!(matches!(
            world.get_resource::<VnLoader>().unwrap().status(),
            VnLoaderStatus::Failed { .. }
        ));
    }

    #[cfg(feature = "app")]
    #[test]
    fn load_system_prepares_images_from_yarn_commands() {
        let temp = tempfile::tempdir().unwrap();
        image::save_buffer(
            temp.path().join("white.png"),
            &[255, 255, 255, 255],
            1,
            1,
            image::ColorType::Rgba8,
        )
        .unwrap();
        image::save_buffer(
            temp.path().join("alice.png"),
            &[255, 0, 255, 255],
            1,
            1,
            image::ColorType::Rgba8,
        )
        .unwrap();
        image::save_buffer(
            temp.path().join("cg.png"),
            &[0, 0, 255, 255],
            1,
            1,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<scene "white.png">>
<<show alice asset="alice.png" expression="smile">>
-> CG
    <<cg "cg.png">>
    Done. #line:done.1
===
"#,
        )
        .unwrap();
        let mut world = World::new();
        VnPlugin::default().install(&mut world).unwrap();
        world
            .get_resource_mut::<VnLoader>()
            .unwrap()
            .set_asset_root(temp.path())
            .load_script(script, "Start");

        world.tick_with_delta(0.016);

        assert_eq!(
            world.get_resource::<VnLoader>().unwrap().status(),
            &VnLoaderStatus::Loaded { image_count: 3 }
        );
        let textures = world.get_resource::<VnSpriteTextureMap>().unwrap();
        assert_eq!(textures.size("white.png"), Some([1, 1]));
        assert_eq!(textures.size("alice.png"), Some([1, 1]));
        assert_eq!(textures.size("cg.png"), Some([1, 1]));
        assert!(textures.get("smile").is_none());
    }
}
