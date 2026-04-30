use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnSceneLayer {
    pub name: String,
    pub order: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnBackground {
    pub asset: String,
    pub layer: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnActorSprite {
    pub actor_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    pub layer: i32,
    pub z: i32,
    pub opacity: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnLive2DActor {
    pub actor_id: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnDialogueUi;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnChoiceUi;
