use std::path::Path;

use crate::gpu::GpuContext;
use crate::render::core::texture::{Texture, TextureUploadDesc};
use crate::render::live2d::model::{Live2DLayout, Live2DModel};

use super::*;

impl Live2DModelResource {
    pub(super) fn load_texture(ctx: &GpuContext, path: &Path) -> Result<Texture, Live2DLoadError> {
        // Use the image crate to decode PNG → RGBA8
        let mut img = image::open(path)
            .map_err(|e| {
                Live2DLoadError::Texture(format!("failed to load {}: {e}", path.display()))
            })?
            .to_rgba8();
        premultiply_rgba8(img.as_mut());

        let (w, h) = img.dimensions();
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "live2d_texture".into());

        // Live2D textures are sampled in linear space.
        Ok(Texture::from_upload_desc(
            ctx,
            TextureUploadDesc::new(w, h, &img)
                .format(wgpu::TextureFormat::Rgba8Unorm)
                .label(label),
        ))
    }

    pub(super) fn parse_layout(json: &serde_json::Value) -> Option<Live2DLayout> {
        let layout_obj = json.get("Layout")?.as_object()?;
        let mut layout = Live2DLayout::default();

        for (key, value) in layout_obj {
            let Some(value) = value.as_f64().map(|v| v as f32) else {
                continue;
            };

            match normalize_layout_key(key).as_str() {
                "width" => layout.width = Some(value),
                "height" => layout.height = Some(value),
                "x" => layout.x = Some(value),
                "y" => layout.y = Some(value),
                "centerx" | "center_x" => layout.center_x = Some(value),
                "centery" | "center_y" => layout.center_y = Some(value),
                "top" => layout.top = Some(value),
                "bottom" => layout.bottom = Some(value),
                "left" => layout.left = Some(value),
                "right" => layout.right = Some(value),
                _ => {}
            }
        }

        if layout == Live2DLayout::default() {
            None
        } else {
            Some(layout)
        }
    }

    pub(super) fn parse_hit_areas(
        json: &serde_json::Value,
        model: &Live2DModel,
    ) -> Vec<Live2DHitArea> {
        json.get("HitAreas")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let drawable_id = entry.get("Id")?.as_str()?;
                let name = entry.get("Name")?.as_str()?;
                let drawable_index = model.find_drawable(drawable_id)?;
                Some(Live2DHitArea {
                    name: name.to_string(),
                    drawable_id: drawable_id.to_string(),
                    drawable_index,
                })
            })
            .collect()
    }

    pub(super) fn parse_user_data(
        text: &str,
        model: &Live2DModel,
    ) -> Result<Vec<Live2DUserDataEntry>, String> {
        Self::parse_user_data_with_resolver(text, |id| model.find_drawable(id))
    }

    pub(super) fn parse_user_data_with_resolver(
        text: &str,
        mut resolve_drawable: impl FnMut(&str) -> Option<usize>,
    ) -> Result<Vec<Live2DUserDataEntry>, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("userdata JSON parse error: {e}"))?;
        let Some(entries) = json.get("UserData").and_then(|value| value.as_array()) else {
            return Ok(Vec::new());
        };

        let mut parsed = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let target = entry
                .get("Target")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("userdata entry {index} missing Target"))?;
            let id = entry
                .get("Id")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("userdata entry {index} missing Id"))?;
            let value = entry
                .get("Value")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("userdata entry {index} missing Value"))?;
            parsed.push(Live2DUserDataEntry {
                target: target.to_string(),
                id: id.to_string(),
                value: value.to_string(),
                drawable_index: (target == "ArtMesh")
                    .then(|| resolve_drawable(id))
                    .flatten(),
            });
        }

        Ok(parsed)
    }

    pub(super) fn parse_display_info(text: &str) -> Result<Live2DDisplayInfo, String> {
        let json: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| format!("display info JSON parse error: {e}"))?;
        Ok(Live2DDisplayInfo {
            parameters: Self::parse_display_named_entries(&json, "Parameters")?,
            parameter_groups: Self::parse_display_named_entries(&json, "ParameterGroups")?,
            parts: Self::parse_display_named_entries(&json, "Parts")?,
        })
    }

    fn parse_display_named_entries(
        json: &serde_json::Value,
        field: &str,
    ) -> Result<Vec<Live2DDisplayNamedEntry>, String> {
        let Some(entries) = json.get(field).and_then(|value| value.as_array()) else {
            return Ok(Vec::new());
        };

        let mut parsed = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let id = entry
                .get("Id")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("{field} entry {index} missing Id"))?;
            let name = entry
                .get("Name")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("{field} entry {index} missing Name"))?;
            let group_id = entry
                .get("GroupId")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            parsed.push(Live2DDisplayNamedEntry {
                id: id.to_string(),
                group_id: group_id.to_string(),
                name: name.to_string(),
            });
        }
        Ok(parsed)
    }
}

fn normalize_layout_key(key: &str) -> String {
    key.chars()
        .filter(|c| *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub(super) fn premultiply_rgba8(bytes: &mut [u8]) {
    for pixel in bytes.chunks_exact_mut(4) {
        let alpha = pixel[3] as u16;
        pixel[0] = ((pixel[0] as u16 * alpha + 127) / 255) as u8;
        pixel[1] = ((pixel[1] as u16 * alpha + 127) / 255) as u8;
        pixel[2] = ((pixel[2] as u16 * alpha + 127) / 255) as u8;
    }
}
