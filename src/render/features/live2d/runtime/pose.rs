//! Live2D pose (`pose3.json`) runtime support.
//!
//! This is a Rust port of the essential `CubismPose` behavior from the
//! Cubism Framework, implemented on top of the Core-only `cubism-sys`
//! bindings available in this repo.

use crate::render::live2d::model::Live2DModel;

const EPSILON: f32 = 0.001;
const DEFAULT_FADE_IN_SECONDS: f32 = 0.5;
const PHI: f32 = 0.5;
const BACK_OPACITY_THRESHOLD: f32 = 0.15;

#[derive(Debug, Clone)]
struct PoseLink {
    id: String,
    part_index: Option<usize>,
}

#[derive(Debug, Clone)]
struct PosePart {
    id: String,
    parameter_index: Option<usize>,
    part_index: Option<usize>,
    links: Vec<PoseLink>,
}

/// Runtime pose state loaded from a `pose3.json` file.
#[derive(Debug, Clone)]
pub struct Live2DPose {
    fade_time_seconds: f32,
    parts: Vec<PosePart>,
    group_counts: Vec<usize>,
    last_model_ptr: Option<usize>,
}

impl Live2DPose {
    /// Parse a pose definition from a `pose3.json` string.
    pub fn from_json_str(text: &str) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("pose JSON parse error: {e}"))?;

        let fade_time_seconds = json
            .get("FadeInTime")
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
            .filter(|value| *value >= 0.0)
            .unwrap_or(DEFAULT_FADE_IN_SECONDS);

        let groups = json
            .get("Groups")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "pose JSON missing Groups array".to_string())?;

        let mut parts = Vec::new();
        let mut group_counts = Vec::with_capacity(groups.len());

        for (group_index, group) in groups.iter().enumerate() {
            let entries = group
                .as_array()
                .ok_or_else(|| format!("pose group {group_index} is not an array"))?;

            group_counts.push(entries.len());

            for (entry_index, entry) in entries.iter().enumerate() {
                let id = entry
                    .get("Id")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        format!("pose group {group_index} entry {entry_index} missing Id")
                    })?;

                let links = entry
                    .get("Link")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|value| value.as_str())
                            .map(|id| PoseLink {
                                id: id.to_string(),
                                part_index: None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                parts.push(PosePart {
                    id: id.to_string(),
                    parameter_index: None,
                    part_index: None,
                    links,
                });
            }
        }

        Ok(Self {
            fade_time_seconds,
            parts,
            group_counts,
            last_model_ptr: None,
        })
    }

    /// Apply pose state for the current frame.
    pub fn update_parameters(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) {
        let model_ptr = model.raw_model_ptr() as usize;
        if self.last_model_ptr != Some(model_ptr) {
            self.reset(model);
            self.last_model_ptr = Some(model_ptr);
        }

        let delta_time_seconds = delta_time_seconds.max(0.0);
        let mut begin_index = 0usize;

        for &group_count in &self.group_counts {
            self.do_fade(model, delta_time_seconds, begin_index, group_count);
            begin_index += group_count;
        }

        self.copy_part_opacities(model);
    }

    pub(crate) fn reset_state(&mut self, model: &mut Live2DModel) {
        self.reset(model);
        self.last_model_ptr = Some(model.raw_model_ptr() as usize);
    }

    fn reset(&mut self, model: &mut Live2DModel) {
        let mut begin_index = 0usize;

        for &group_count in &self.group_counts {
            for local_index in 0..group_count {
                let index = begin_index + local_index;
                let is_visible = local_index == 0;
                let part = &mut self.parts[index];

                part.parameter_index = model.find_parameter(&part.id);
                part.part_index = model.find_part(&part.id);

                if let Some(parameter_index) = part.parameter_index {
                    model.set_parameter_by_index(
                        parameter_index,
                        if is_visible { 1.0 } else { 0.0 },
                    );
                }

                if let Some(part_index) = part.part_index {
                    model.set_part_opacity(part_index, if is_visible { 1.0 } else { 0.0 });
                }

                for link in &mut part.links {
                    link.part_index = model.find_part(&link.id);
                }
            }

            begin_index += group_count;
        }
    }

    fn copy_part_opacities(&self, model: &mut Live2DModel) {
        for part in &self.parts {
            let Some(part_index) = part.part_index else {
                continue;
            };
            if part.links.is_empty() {
                continue;
            }

            let opacity = model.part_opacity(part_index);
            for link in &part.links {
                if let Some(link_part_index) = link.part_index {
                    model.set_part_opacity(link_part_index, opacity);
                }
            }
        }
    }

    fn do_fade(
        &self,
        model: &mut Live2DModel,
        delta_time_seconds: f32,
        begin_index: usize,
        group_count: usize,
    ) {
        if group_count == 0 {
            return;
        }

        let mut visible_part_index = None;
        let mut new_opacity = 1.0f32;

        for index in begin_index..begin_index + group_count {
            let part = &self.parts[index];
            let (Some(parameter_index), Some(part_index)) = (part.parameter_index, part.part_index)
            else {
                continue;
            };

            if model.parameter_value(parameter_index) > EPSILON {
                if visible_part_index.is_some() {
                    break;
                }

                visible_part_index = Some(index);
                new_opacity = model.part_opacity(part_index);
                new_opacity = if self.fade_time_seconds <= EPSILON {
                    1.0
                } else {
                    (new_opacity + delta_time_seconds / self.fade_time_seconds).min(1.0)
                };
            }
        }

        let visible_part_index = visible_part_index.unwrap_or(begin_index);

        for index in begin_index..begin_index + group_count {
            let Some(part_index) = self.parts[index].part_index else {
                continue;
            };

            if index == visible_part_index {
                model.set_part_opacity(part_index, new_opacity);
                continue;
            }

            let opacity = model.part_opacity(part_index);
            let mut target_opacity = if new_opacity < PHI {
                new_opacity * (PHI - 1.0) / PHI + 1.0
            } else {
                (1.0 - new_opacity) * PHI / (1.0 - PHI)
            };

            let back_opacity = (1.0 - target_opacity) * (1.0 - new_opacity);
            if back_opacity > BACK_OPACITY_THRESHOLD && (1.0 - new_opacity) > EPSILON {
                target_opacity = 1.0 - BACK_OPACITY_THRESHOLD / (1.0 - new_opacity);
            }

            model.set_part_opacity(part_index, opacity.min(target_opacity));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pose_groups_and_links() {
        let pose = Live2DPose::from_json_str(
            r#"{
                "FadeInTime": 0.25,
                "Groups": [
                    [
                        { "Id": "PartA", "Link": ["LinkA", "LinkB"] },
                        { "Id": "PartB", "Link": [] }
                    ],
                    [
                        { "Id": "PartC" }
                    ]
                ]
            }"#,
        )
        .expect("pose should parse");

        assert_eq!(pose.fade_time_seconds, 0.25);
        assert_eq!(pose.group_counts, vec![2, 1]);
        assert_eq!(pose.parts.len(), 3);
        assert_eq!(pose.parts[0].id, "PartA");
        assert_eq!(pose.parts[0].links.len(), 2);
        assert_eq!(pose.parts[2].id, "PartC");
    }

    #[test]
    fn negative_fade_uses_default() {
        let pose = Live2DPose::from_json_str(
            r#"{
                "FadeInTime": -1.0,
                "Groups": [[{ "Id": "PartA" }]]
            }"#,
        )
        .expect("pose should parse");

        assert_eq!(pose.fade_time_seconds, DEFAULT_FADE_IN_SECONDS);
    }

    #[test]
    fn reset_state_updates_cached_model_pointer() {
        let mut pose = Live2DPose::from_json_str(
            r#"{
                "Groups": [[{ "Id": "PartA" }]]
            }"#,
        )
        .expect("pose should parse");

        let moc_bytes = std::fs::read(
            "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.moc3",
        )
        .expect("sample moc3 should exist");
        let mut model = Live2DModel::from_moc3_bytes(&moc_bytes).expect("sample moc3 should load");

        pose.reset_state(&mut model);

        assert_eq!(pose.last_model_ptr, Some(model.raw_model_ptr() as usize));
    }
}
