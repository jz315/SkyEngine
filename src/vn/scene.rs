use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::vn::script::{VnValue, YarnCommand};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VnSceneState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<VnImageLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cg: Option<VnImageLayer>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub actors: BTreeMap<String, VnActor>,
    #[serde(default)]
    pub camera: VnCameraState,
}

impl VnSceneState {
    pub fn apply_command(&mut self, command: &YarnCommand) -> Option<VnSceneChange> {
        match command.name.as_str() {
            "scene" | "bg" => {
                let asset = command.first_positional_raw()?.to_owned();
                let layer = VnImageLayer {
                    asset,
                    layer: named_i32(command, "layer").unwrap_or(0),
                    transition: transition_from_command(command),
                };
                self.background = Some(layer.clone());
                Some(VnSceneChange::Background(layer))
            }
            "cg" => {
                let asset = command.first_positional_raw()?.to_owned();
                let layer = VnImageLayer {
                    asset,
                    layer: named_i32(command, "layer").unwrap_or(40),
                    transition: transition_from_command(command),
                };
                self.cg = Some(layer.clone());
                Some(VnSceneChange::Cg(layer))
            }
            "hide_cg" | "clear_cg" => self.cg.take().map(VnSceneChange::CgHidden),
            "show" => {
                let mut positional = command.positional_args();
                let id = positional.next()?.raw.clone();
                let asset = command
                    .named_arg("asset")
                    .map(|arg| arg.raw.clone())
                    .or_else(|| positional.next().map(|arg| arg.raw.clone()));

                let actor = self.actors.entry(id.clone()).or_insert_with(|| VnActor {
                    id: id.clone(),
                    ..VnActor::default()
                });
                actor.visible = true;
                actor.asset = asset.or_else(|| actor.asset.clone());
                actor.expression = command
                    .named_arg("expression")
                    .map(|arg| arg.raw.clone())
                    .or_else(|| actor.expression.clone());
                actor.position = command
                    .named_arg("at")
                    .map(|arg| arg.raw.clone())
                    .or_else(|| actor.position.clone());
                actor.layer = named_i32(command, "layer").unwrap_or(actor.layer);
                actor.z = named_i32(command, "z").unwrap_or(actor.z);
                actor.opacity = named_f32(command, "opacity").unwrap_or(actor.opacity);
                actor.transition = transition_from_command(command);

                Some(VnSceneChange::ActorShown(actor.clone()))
            }
            "hide" => {
                let id = command.first_positional_raw()?.to_owned();
                let mut actor = self.actors.remove(&id).unwrap_or_else(|| VnActor {
                    id,
                    visible: false,
                    transition: transition_from_command(command),
                    ..VnActor::default()
                });
                actor.visible = false;
                actor.transition = transition_from_command(command);
                Some(VnSceneChange::ActorHidden(actor))
            }
            "move" => {
                let mut positional = command.positional_args();
                let id = positional.next()?.raw.clone();
                let actor = self.actors.get_mut(&id)?;
                actor.position = command
                    .named_arg("to")
                    .or_else(|| command.named_arg("at"))
                    .map(|arg| arg.raw.clone())
                    .or_else(|| positional.next().map(|arg| arg.raw.clone()))
                    .or_else(|| actor.position.clone());
                actor.transition = transition_from_command(command);
                Some(VnSceneChange::ActorMoved(actor.clone()))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum VnSceneChange {
    Background(VnImageLayer),
    Cg(VnImageLayer),
    CgHidden(VnImageLayer),
    ActorShown(VnActor),
    ActorHidden(VnActor),
    ActorMoved(VnActor),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnImageLayer {
    pub asset: String,
    pub layer: i32,
    pub transition: VnTransition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnActor {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    pub layer: i32,
    pub z: i32,
    pub opacity: f32,
    pub visible: bool,
    pub transition: VnTransition,
}

impl Default for VnActor {
    fn default() -> Self {
        Self {
            id: String::new(),
            asset: None,
            expression: None,
            position: None,
            layer: 20,
            z: 0,
            opacity: 1.0,
            visible: true,
            transition: VnTransition::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnCameraState {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub rotation: f32,
}

impl Default for VnCameraState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnTransition {
    pub kind: VnTransitionKind,
    pub duration: f32,
}

impl Default for VnTransition {
    fn default() -> Self {
        Self {
            kind: VnTransitionKind::None,
            duration: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnTransitionKind {
    None,
    Fade,
    Dissolve,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Move,
    Custom(String),
}

fn transition_from_command(command: &YarnCommand) -> VnTransition {
    let raw = command
        .named_arg("transition")
        .map(|arg| arg.raw.as_str())
        .unwrap_or("none");
    VnTransition {
        kind: match raw {
            "none" => VnTransitionKind::None,
            "fade" => VnTransitionKind::Fade,
            "dissolve" => VnTransitionKind::Dissolve,
            "slide_left" => VnTransitionKind::SlideLeft,
            "slide_right" => VnTransitionKind::SlideRight,
            "slide_up" => VnTransitionKind::SlideUp,
            "slide_down" => VnTransitionKind::SlideDown,
            "move" => VnTransitionKind::Move,
            other => VnTransitionKind::Custom(other.to_owned()),
        },
        duration: named_f32(command, "duration").unwrap_or(0.0),
    }
}

fn named_f32(command: &YarnCommand, name: &str) -> Option<f32> {
    match &command.named_arg(name)?.value {
        VnValue::Number(value) => Some(*value as f32),
        VnValue::String(value) => value.parse().ok(),
        VnValue::Bool(_) => None,
    }
}

fn named_i32(command: &YarnCommand, name: &str) -> Option<i32> {
    match &command.named_arg(name)?.value {
        VnValue::Number(value) => Some(*value as i32),
        VnValue::String(value) => value.parse().ok(),
        VnValue::Bool(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::vn::script::{YarnInstruction, YarnScript};

    use super::*;

    #[test]
    fn scene_state_applies_background_and_actor_commands() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<scene "bg/classroom.png" transition="fade" duration=0.4>>
<<show alice "chars/alice/smile.png" expression="smile" at="right" z=10>>
<<hide alice>>
===
"#,
        )
        .unwrap();
        let node = script.node("Start").unwrap();
        let mut scene = VnSceneState::default();

        for instruction in &node.body {
            let YarnInstruction::Command(command) = instruction else {
                continue;
            };
            scene.apply_command(command);
        }

        assert_eq!(
            scene.background.as_ref().map(|layer| layer.asset.as_str()),
            Some("bg/classroom.png")
        );
        assert!(scene.actors.is_empty());
    }

    #[test]
    fn scene_state_hides_cg_layer() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<cg "cg/notebook.png" layer=40>>
<<hide_cg>>
===
"#,
        )
        .unwrap();
        let node = script.node("Start").unwrap();
        let mut scene = VnSceneState::default();

        for instruction in &node.body {
            let YarnInstruction::Command(command) = instruction else {
                continue;
            };
            scene.apply_command(command);
        }

        assert_eq!(scene.cg, None);
    }
}
