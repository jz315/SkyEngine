use std::path::Path;

use sky_engine::ecs::EntityId;
use sky_engine::render::expert::live2d::Live2DUserModel;

/// One loaded model slot.
pub struct ModelSlot {
    pub entity: EntityId,
    pub name: String,
    pub motion_groups: Vec<MotionGroupUi>,
    pub expression_names: Vec<String>,
}

impl ModelSlot {
    pub fn from_user_model(entity: EntityId, path: &Path, user_model: &Live2DUserModel) -> Self {
        Self {
            entity,
            name: model_display_name(path),
            motion_groups: MotionGroupUi::from_user_model(user_model),
            expression_names: expression_names_from_model(user_model),
        }
    }
}

pub struct MotionGroupUi {
    pub name: String,
    pub motions: Vec<MotionUi>,
}

impl MotionGroupUi {
    fn from_user_model(user_model: &Live2DUserModel) -> Vec<Self> {
        user_model
            .motion_player()
            .map(|player| {
                let mut groups = Vec::<Self>::new();
                for entry in player.motion_entries() {
                    if groups
                        .last()
                        .is_none_or(|group| group.name != entry.group_name)
                    {
                        groups.push(Self {
                            name: entry.group_name.to_string(),
                            motions: Vec::new(),
                        });
                    }
                    groups
                        .last_mut()
                        .expect("motion group should exist")
                        .motions
                        .push(MotionUi::new(entry.index_in_group, entry.motion_name));
                }
                groups
            })
            .unwrap_or_default()
    }
}

pub struct MotionUi {
    pub index_in_group: usize,
    pub name: String,
}

impl MotionUi {
    fn new(index_in_group: usize, name: &str) -> Self {
        Self {
            index_in_group,
            name: name.to_string(),
        }
    }
}

fn expression_names_from_model(user_model: &Live2DUserModel) -> Vec<String> {
    user_model
        .expression_player()
        .map(|player| {
            player
                .expression_names()
                .map(|name| name.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn model_display_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Unknown")
        .trim_end_matches(".model3")
        .to_string()
}
