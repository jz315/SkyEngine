use std::collections::BTreeSet;
use std::path::PathBuf;

#[cfg(test)]
use crate::ecs::Update;
use crate::ecs::World;
use crate::vn::action::VnAction;
use crate::vn::loader::VnLoadRequest;
#[cfg(feature = "app")]
use crate::vn::presentation::VnSpriteTextureMap;
use crate::vn::resource::VnResource;
use crate::vn::runtime::{VnRuntime, VnRuntimeEvent, VnRuntimeResult, VnStatus};
use crate::vn::script::{
    VnCommandArg, VnValue, YarnCommand, YarnInstruction, YarnProject, YarnScript,
};
use crate::vn::ui::{VnConfirmKind, VnUiMode};

#[cfg(feature = "app")]
use crate::asset::Assets;

#[cfg(feature = "app")]
use crate::input::{Input, InputActions, InteractionContext, KeyCode, MouseButton};

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
    #[cfg(feature = "app")]
    refresh_vn_texture_metadata(world);

    let (asset_root, pending) = {
        let Some(vn) = world.get_resource_mut::<VnResource>() else {
            return;
        };
        (
            vn.loader.asset_root().map(PathBuf::from),
            vn.take_pending_load(),
        )
    };
    let Some(pending) = pending else {
        return;
    };

    match load_pending_vn(world, pending, asset_root) {
        Ok(loaded) => {
            if let Some(vn) = world.get_resource_mut::<VnResource>() {
                #[cfg(feature = "app")]
                vn.mark_loaded(
                    loaded.project,
                    loaded.runtime,
                    loaded.image_count,
                    loaded.textures,
                );
                #[cfg(not(feature = "app"))]
                vn.mark_loaded(loaded.project, loaded.runtime, loaded.image_count);
            }
        }
        Err(message) => {
            eprintln!("[SkyEngine][VN] load failed: {message}");
            if let Some(vn) = world.get_resource_mut::<VnResource>() {
                vn.mark_failed(message);
            }
        }
    }
}

pub fn vn_input_system(world: &mut World) {
    let mut actions = Vec::new();

    #[cfg(feature = "app")]
    actions.extend(collect_vn_actions(world));

    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return;
    };
    actions.extend(vn.input.drain());
    vn.actions.extend(actions);
    apply_queued_vn_actions(vn);
}

fn load_pending_vn(
    world: &mut World,
    pending: VnLoadRequest,
    asset_root: Option<PathBuf>,
) -> Result<LoadedVn, String> {
    let project = match pending {
        VnLoadRequest::Project(project) => project,
        VnLoadRequest::Script { script, start_node } => {
            YarnProject::new(start_node, script).map_err(|error| error.to_string())?
        }
    };
    let runtime = VnRuntime::from_project(project.clone()).map_err(|error| error.to_string())?;
    let root = asset_root.unwrap_or_else(|| project.root.clone());
    let prepared = prepare_vn_images(world, &project.script, root)?;

    Ok(LoadedVn {
        project,
        runtime,
        image_count: prepared.image_count,
        #[cfg(feature = "app")]
        textures: prepared.textures,
    })
}

struct LoadedVn {
    project: YarnProject,
    runtime: VnRuntime,
    image_count: usize,
    #[cfg(feature = "app")]
    textures: VnSpriteTextureMap,
}

struct PreparedVnImages {
    image_count: usize,
    #[cfg(feature = "app")]
    textures: VnSpriteTextureMap,
}

#[cfg(feature = "app")]
fn prepare_vn_images(
    world: &mut World,
    script: &YarnScript,
    asset_root: PathBuf,
) -> Result<PreparedVnImages, String> {
    let image_assets = collect_image_assets(script);
    if image_assets.is_empty() {
        return Ok(PreparedVnImages {
            image_count: 0,
            textures: VnSpriteTextureMap::default(),
        });
    }

    let assets = world
        .get_resource::<Assets>()
        .cloned()
        .ok_or_else(|| "VN image preparation requires an Assets resource".to_string())?;

    let mut texture_map = VnSpriteTextureMap::default();
    for asset in &image_assets {
        texture_map
            .request_image_file(&assets, asset.clone(), asset_root.join(asset))
            .map_err(|error| error.to_string())?;
    }
    let image_count = image_assets.len();
    Ok(PreparedVnImages {
        image_count,
        textures: texture_map,
    })
}

#[cfg(feature = "app")]
fn refresh_vn_texture_metadata(world: &mut World) {
    let Some(asset_server) = world.get_resource::<Assets>().cloned() else {
        return;
    };
    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return;
    };
    vn.sprite_textures.refresh_metadata(&asset_server);
}

#[cfg(not(feature = "app"))]
fn prepare_vn_images(
    _world: &mut World,
    script: &YarnScript,
    _asset_root: PathBuf,
) -> Result<PreparedVnImages, String> {
    Ok(PreparedVnImages {
        image_count: collect_image_assets(script).len(),
    })
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
    let dt = world.time.delta;

    let Some(vn) = world.get_resource_mut::<VnResource>() else {
        return;
    };
    let config = vn.system_config.clone();
    let Some(runtime) = vn.runtime_mut() else {
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
    #[cfg(feature = "app")]
    crate::vn::presentation::sync_runtime_scene_to_world(world);

    #[cfg(not(feature = "app"))]
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

fn apply_vn_actions(vn: &mut VnResource, actions: Vec<VnAction>) {
    for action in actions {
        match action {
            VnAction::Auto => {
                let playback = &mut vn.playback;
                playback.auto_mode = !playback.auto_mode;
                if playback.auto_mode {
                    playback.skip_mode = false;
                }
            }
            VnAction::Skip => {
                let playback = &mut vn.playback;
                playback.skip_mode = !playback.skip_mode;
                if playback.skip_mode {
                    playback.auto_mode = false;
                }
            }
            VnAction::HideUi => {
                toggle_ui_mode(vn, VnUiMode::Hidden);
            }
            VnAction::Menu => {
                toggle_ui_mode(vn, VnUiMode::Menu);
            }
            VnAction::Backlog => {
                toggle_ui_mode(vn, VnUiMode::Backlog);
            }
            VnAction::QuickLoad => {
                enter_ui_mode(vn, VnUiMode::Confirm(VnConfirmKind::QuickLoad));
            }
            VnAction::Cancel => {
                cancel_ui_mode(vn);
            }
            VnAction::QuickSave => {
                if let Err(error) = vn.save_slot("quick") {
                    eprintln!("[SkyEngine][VN] quick save failed: {error}");
                }
            }
            VnAction::Advance
            | VnAction::Choice(_)
            | VnAction::Confirm
            | VnAction::Up
            | VnAction::Down => {
                if let Some(runtime) = vn.runtime_mut() {
                    match runtime.apply_action(action) {
                        Ok(Some(VnRuntimeEvent::Command(_))) => {
                            let mut events = Vec::new();
                            if let Err(error) = drain_runtime_commands(runtime, &mut events) {
                                eprintln!("[SkyEngine][VN] command drain failed: {error}");
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!("[SkyEngine][VN] action failed: {error}");
                        }
                    }
                }
            }
        }
    }
}

fn drain_runtime_commands(
    runtime: &mut VnRuntime,
    output: &mut Vec<VnRuntimeEvent>,
) -> VnRuntimeResult<()> {
    for _ in 0..16 {
        let event = runtime.advance()?;
        let keep_draining = matches!(event, VnRuntimeEvent::Command(_));
        output.push(event);
        if !keep_draining {
            break;
        }
    }
    Ok(())
}

fn apply_queued_vn_actions(vn: &mut VnResource) {
    let actions: Vec<_> = vn.actions.drain().collect();
    if !actions.is_empty() {
        apply_vn_actions(vn, actions);
    }
}

fn toggle_ui_mode(vn: &mut VnResource, mode: VnUiMode) {
    let ui = &mut vn.ui;
    if ui.mode == mode {
        ui.back();
    } else {
        ui.enter(mode);
    }
}

fn enter_ui_mode(vn: &mut VnResource, mode: VnUiMode) {
    vn.ui.enter(mode);
}

fn cancel_ui_mode(vn: &mut VnResource) {
    let ui = &mut vn.ui;
    if ui.mode == VnUiMode::Reading {
        ui.enter(VnUiMode::Menu);
    } else {
        ui.back();
    }
}

#[cfg(feature = "app")]
fn input_consumed(world: &World, button: MouseButton) -> bool {
    world
        .get_resource::<InteractionContext>()
        .is_some_and(|interaction| interaction.pointer_consumed(button))
}

#[cfg(feature = "app")]
fn key_consumed(world: &World, key: KeyCode) -> bool {
    world
        .get_resource::<InteractionContext>()
        .is_some_and(|interaction| interaction.key_consumed(key))
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
        let keyboard_advance = (input.key_pressed(KeyCode::Enter)
            && !key_consumed(world, KeyCode::Enter))
            || (input.key_pressed(KeyCode::Space) && !key_consumed(world, KeyCode::Space));
        let pointer_advance = input.mouse_button_pressed(MouseButton::Left)
            && !input_consumed(world, MouseButton::Left);
        if keyboard_advance || pointer_advance {
            actions.push(VnAction::Advance);
        }
        if input.key_pressed(KeyCode::ArrowUp) && !key_consumed(world, KeyCode::ArrowUp) {
            actions.push(VnAction::Up);
        }
        if input.key_pressed(KeyCode::ArrowDown) && !key_consumed(world, KeyCode::ArrowDown) {
            actions.push(VnAction::Down);
        }
        if (input.key_pressed(KeyCode::Escape) && !key_consumed(world, KeyCode::Escape))
            || (input.mouse_button_pressed(MouseButton::Right)
                && !input_consumed(world, MouseButton::Right))
        {
            actions.push(VnAction::Cancel);
        }
        if input.key_pressed(KeyCode::Tab) && !key_consumed(world, KeyCode::Tab) {
            actions.push(VnAction::Skip);
        }
        if input.key_pressed(KeyCode::KeyA) && !key_consumed(world, KeyCode::KeyA) {
            actions.push(VnAction::Auto);
        }
        if input.key_pressed(KeyCode::KeyH) && !key_consumed(world, KeyCode::KeyH) {
            actions.push(VnAction::HideUi);
        }
        if input.key_pressed(KeyCode::KeyB) && !key_consumed(world, KeyCode::KeyB) {
            actions.push(VnAction::Backlog);
        }
        if input.key_pressed(KeyCode::KeyS) && !key_consumed(world, KeyCode::KeyS) {
            actions.push(VnAction::QuickSave);
        }
        if input.key_pressed(KeyCode::KeyL) && !key_consumed(world, KeyCode::KeyL) {
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
    use crate::plugin::Plugin;
    use crate::vn::script::YarnScript;
    use crate::vn::{VnLoaderStatus, VnPlugin, VnResource};

    fn insert_vn_runtime(world: &mut World, runtime: VnRuntime) {
        let vn = VnResource {
            runtime: Some(runtime),
            ..Default::default()
        };
        world.insert_resource(vn);
    }

    fn vn_resource(world: &World) -> &VnResource {
        world.get_resource::<VnResource>().unwrap()
    }

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
        let vn = VnResource {
            runtime: Some(VnRuntime::from_script(script, "Start").unwrap()),
            system_config: VnSystemConfig {
                reveal_chars_per_second: 10.0,
                ..Default::default()
            },
            ..Default::default()
        };
        world.insert_resource(vn);
        world.stage(Update).add_exclusive(vn_script_system);

        world.tick_with_delta(0.1).unwrap();
        let runtime = vn_resource(&world).runtime().unwrap();
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
        insert_vn_runtime(&mut world, VnRuntime::from_script(script, "Start").unwrap());
        world
            .get_resource_mut::<VnResource>()
            .unwrap()
            .push_action(VnAction::Advance);

        vn_input_system(&mut world);

        assert_eq!(
            vn_resource(&world).runtime().unwrap().status(),
            &VnStatus::Line
        );
    }

    #[test]
    fn ui_mode_actions_toggle_shell_state() {
        let mut world = World::new();
        world.insert_resource(VnResource::default());
        world
            .get_resource_mut::<VnResource>()
            .unwrap()
            .push_action(VnAction::Backlog);

        vn_input_system(&mut world);
        assert_eq!(vn_resource(&world).ui().mode, VnUiMode::Backlog);

        world
            .get_resource_mut::<VnResource>()
            .unwrap()
            .push_action(VnAction::Cancel);
        vn_input_system(&mut world);
        assert_eq!(vn_resource(&world).ui().mode, VnUiMode::Reading);
    }

    #[cfg(feature = "app")]
    #[test]
    fn consumed_mouse_left_does_not_become_vn_advance() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
Hello. #line:start.1
-> A
    <<set $route = "a">>
    <<jump Ending>>
-> B
    <<set $route = "b">>
    <<jump Ending>>
===

title: Ending
---
Done. #line:end.1
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();
        runtime.advance().unwrap();
        runtime.dialogue_mut().complete_line();
        runtime.advance().unwrap();

        let mut input = Input::new();
        input.set_mouse_position(100.0, 100.0);
        input.mouse_button_down(MouseButton::Left.index());

        let mut world = World::new();
        insert_vn_runtime(&mut world, runtime);
        world.insert_resource(input);
        let mut interaction = InteractionContext::default();
        interaction.consume_pointer(MouseButton::Left);
        world.insert_resource(interaction);

        vn_input_system(&mut world);

        let runtime = vn_resource(&world).runtime().unwrap();
        assert_eq!(runtime.status(), &VnStatus::Choice);
        assert!(runtime.variable("route").is_none());
    }

    #[cfg(feature = "app")]
    #[test]
    fn unconsumed_mouse_left_can_advance_vn() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
Hello. #line:start.1
-> A
    <<set $route = "a">>
    <<jump Ending>>
-> B
    <<set $route = "b">>
    <<jump Ending>>
===

title: Ending
---
Done. #line:end.1
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();
        runtime.advance().unwrap();
        runtime.dialogue_mut().complete_line();
        runtime.advance().unwrap();

        let mut input = Input::new();
        input.set_mouse_position(100.0, 100.0);
        input.mouse_button_down(MouseButton::Left.index());

        let mut world = World::new();
        insert_vn_runtime(&mut world, runtime);
        world.insert_resource(input);

        vn_input_system(&mut world);

        assert_eq!(
            vn_resource(&world).runtime().unwrap().variable("route"),
            Some(&VnValue::String("a".to_owned()))
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
            .get_resource_mut::<VnResource>()
            .unwrap()
            .load_script(script, "Start")
            .unwrap();

        world.tick_with_delta(0.016).unwrap();

        assert!(vn_resource(&world).runtime().is_some());
        assert_eq!(
            vn_resource(&world).load_status(),
            &VnLoaderStatus::Loaded { image_count: 0 }
        );
    }

    #[test]
    fn load_script_rejects_missing_start_without_replacing_runtime() {
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
        world.get_resource_mut::<VnResource>().unwrap().runtime =
            Some(VnRuntime::from_script(original, "Start").unwrap());
        let result = world
            .get_resource_mut::<VnResource>()
            .unwrap()
            .load_script(replacement, "Missing");
        assert!(result.is_err());

        world.tick_with_delta(0.016).unwrap();

        let runtime = vn_resource(&world).runtime().unwrap();
        assert_eq!(
            runtime.dialogue().current_line.as_ref().unwrap().text,
            "Original."
        );
        assert_eq!(vn_resource(&world).load_status(), &VnLoaderStatus::Idle);
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
            .get_resource_mut::<VnResource>()
            .unwrap()
            .set_asset_root(temp.path())
            .load_script(script, "Start")
            .unwrap();

        world.tick_with_delta(0.016).unwrap();

        assert_eq!(
            vn_resource(&world).load_status(),
            &VnLoaderStatus::Loaded { image_count: 3 }
        );
        let textures = vn_resource(&world).sprite_textures();
        assert!(textures.get("white.png").is_some());
        assert!(textures.get("alice.png").is_some());
        assert!(textures.get("cg.png").is_some());
        assert_eq!(textures.size("white.png"), None);
        assert!(textures.get("smile").is_none());

        let assets = world.get_resource::<Assets>().unwrap().clone();
        for _ in 0..64 {
            assets.update().unwrap();
            world.tick_with_delta(0.016).unwrap();
            let textures = vn_resource(&world).sprite_textures();
            if textures.size("white.png").is_some()
                && textures.size("alice.png").is_some()
                && textures.size("cg.png").is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let textures = vn_resource(&world).sprite_textures();
        assert_eq!(textures.size("white.png"), Some([1, 1]));
        assert_eq!(textures.size("alice.png"), Some([1, 1]));
        assert_eq!(textures.size("cg.png"), Some([1, 1]));
    }
}
