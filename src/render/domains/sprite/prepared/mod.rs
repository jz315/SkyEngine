//! Prepared multi-view 2D render data.

mod cull;
mod sort;
#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;

use crate::render::core::camera::{Camera2D, ViewUniform};
use crate::render::ecs::{RenderSettings, ViewportRect};
use crate::render::scene::{RenderQueueSort, SceneView};
use crate::render::Texture;

use self::cull::{light_visible_in_camera, sprite_visible_in_camera};
use self::sort::sort_visible_sprite_slots_for_view;
use super::scene_cache::SceneCache2D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpriteDrawSpan {
    pub first_instance: u32,
    pub instance_count: u32,
    pub texture_index: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct PreparedView2D {
    pub order: i32,
    pub viewport: ViewportRect,
    pub layer_mask: u32,
    pub view_uniform: ViewUniform,
    pub cull_camera_2d: Option<Camera2D>,
    pub sprite_offset: u32,
    pub sprite_count: u32,
    pub draw_span_offset: usize,
    pub draw_span_count: usize,
    pub light_offset: u32,
    pub light_count: u32,
}

impl PreparedView2D {
    #[inline]
    pub fn sprite_range(self) -> std::ops::Range<u32> {
        self.sprite_offset..(self.sprite_offset + self.sprite_count)
    }

    #[inline]
    pub fn light_range(self) -> std::ops::Range<u32> {
        self.light_offset..(self.light_offset + self.light_count)
    }

    #[inline]
    pub fn draw_span_range(self) -> std::ops::Range<usize> {
        self.draw_span_offset..(self.draw_span_offset + self.draw_span_count)
    }
}

/// Fully prepared CPU-side render world for the current frame.
pub(crate) struct PreparedRenderWorld2D {
    pub settings: RenderSettings,
    pub views: Vec<PreparedView2D>,
    pub visible_sprite_slots: Vec<u32>,
    pub visible_light_slots: Vec<u32>,
    pub draw_spans: Vec<SpriteDrawSpan>,
    pub textures: Vec<Texture>,
    pub sprite_count: usize,
    pub light_count: usize,
    sorted_view_indices: Vec<usize>,
    visible_sprite_scratch: Vec<u32>,
}

impl PreparedRenderWorld2D {
    pub fn new() -> Self {
        Self {
            settings: RenderSettings::default(),
            views: Vec::with_capacity(4),
            visible_sprite_slots: Vec::with_capacity(1024),
            visible_light_slots: Vec::with_capacity(256),
            draw_spans: Vec::with_capacity(128),
            textures: Vec::with_capacity(32),
            sprite_count: 0,
            light_count: 0,
            sorted_view_indices: Vec::with_capacity(4),
            visible_sprite_scratch: Vec::with_capacity(1024),
        }
    }

    pub fn reset(&mut self) {
        self.views.clear();
        self.visible_sprite_slots.clear();
        self.visible_light_slots.clear();
        self.draw_spans.clear();
        self.textures.clear();
        self.sprite_count = 0;
        self.light_count = 0;
        self.sorted_view_indices.clear();
        self.visible_sprite_scratch.clear();
    }

    pub fn prepare_scene(
        &mut self,
        scene: &mut SceneCache2D,
        views: &[SceneView],
        surface_size: [u32; 2],
    ) {
        self.reset();
        self.settings = scene.settings;
        let active_sprite_slots = scene.active_sprite_slots();
        let sprite_sort_policy = scene.sprite_sort_policy();

        let mut texture_map = FxHashMap::<u64, usize>::default();
        self.sorted_view_indices.extend(0..views.len());
        self.sorted_view_indices
            .sort_by_key(|&index| views[index].order);
        for &view_index in &self.sorted_view_indices {
            let scene_view = &views[view_index];
            let viewport = scene_view.viewport.clamp_to_surface(surface_size);

            let sprite_offset = self.visible_sprite_slots.len() as u32;
            let draw_span_offset = self.draw_spans.len();
            prepare_view_sprites(
                &mut self.visible_sprite_scratch,
                &mut self.visible_sprite_slots,
                &mut self.draw_spans,
                &mut self.textures,
                &mut texture_map,
                scene_view,
                sprite_sort_policy,
                scene_view.cull_camera_2d.as_ref(),
                scene_view.layer_mask,
                active_sprite_slots,
                scene,
            );
            let sprite_count = self.visible_sprite_slots.len() as u32 - sprite_offset;
            let draw_span_count = self.draw_spans.len() - draw_span_offset;

            let light_offset = self.visible_light_slots.len() as u32;
            prepare_view_lights(
                &mut self.visible_light_slots,
                scene_view.cull_camera_2d.as_ref(),
                scene_view.layer_mask,
                scene,
            );
            let light_count = self.visible_light_slots.len() as u32 - light_offset;

            self.views.push(PreparedView2D {
                order: scene_view.order,
                viewport,
                layer_mask: scene_view.layer_mask,
                view_uniform: scene_view.view_uniform,
                cull_camera_2d: scene_view.cull_camera_2d,
                sprite_offset,
                sprite_count,
                draw_span_offset,
                draw_span_count,
                light_offset,
                light_count,
            });
        }

        self.sprite_count = self.visible_sprite_slots.len();
        self.light_count = self.visible_light_slots.len();
    }
}

impl Default for PreparedRenderWorld2D {
    fn default() -> Self {
        Self::new()
    }
}

fn prepare_view_sprites(
    visible_slots_scratch: &mut Vec<u32>,
    out_visible_slots: &mut Vec<u32>,
    out_draw_spans: &mut Vec<SpriteDrawSpan>,
    textures: &mut Vec<Texture>,
    texture_map: &mut FxHashMap<u64, usize>,
    scene_view: &SceneView,
    sort_policy: RenderQueueSort,
    camera: Option<&Camera2D>,
    layer_mask: u32,
    active_sprite_slots: &[usize],
    scene: &SceneCache2D,
) {
    visible_slots_scratch.clear();
    visible_slots_scratch.extend(active_sprite_slots.iter().copied().filter_map(|slot| {
        let item = scene.sprite_item(slot)?;
        if item.sprite.visible
            && (item.sprite.layer_mask & layer_mask) != 0
            && sprite_visible_in_camera(camera, item)
        {
            Some(slot as u32)
        } else {
            None
        }
    }));
    sort_visible_sprite_slots_for_view(visible_slots_scratch, sort_policy, scene_view, scene);

    let mut current_span_texture = None;
    let mut current_span_start = out_visible_slots.len() as u32;
    let mut current_span_count = 0u32;

    for &slot in visible_slots_scratch.iter() {
        let item = scene.require_sprite_item(slot as usize);
        let texture_index = texture_index_for(&item.sprite.texture, textures, texture_map);
        if current_span_count == 0 {
            current_span_texture = texture_index;
            current_span_start = out_visible_slots.len() as u32;
        } else if current_span_texture != texture_index {
            out_draw_spans.push(SpriteDrawSpan {
                first_instance: current_span_start,
                instance_count: current_span_count,
                texture_index: current_span_texture,
            });
            current_span_texture = texture_index;
            current_span_start = out_visible_slots.len() as u32;
            current_span_count = 0;
        }

        out_visible_slots.push(slot);
        current_span_count += 1;
    }

    if current_span_count > 0 {
        out_draw_spans.push(SpriteDrawSpan {
            first_instance: current_span_start,
            instance_count: current_span_count,
            texture_index: current_span_texture,
        });
    }
}

fn prepare_view_lights(
    out_visible_slots: &mut Vec<u32>,
    camera: Option<&Camera2D>,
    layer_mask: u32,
    scene: &SceneCache2D,
) {
    for &slot in scene.active_light_slots() {
        let item = scene.require_light_item(slot);
        if !item.light.visible || (item.light.layer_mask & layer_mask) == 0 {
            continue;
        }
        if !light_visible_in_camera(camera, item) {
            continue;
        }

        out_visible_slots.push(slot as u32);
    }
}

fn texture_index_for(
    texture: &Option<Texture>,
    textures: &mut Vec<Texture>,
    texture_map: &mut FxHashMap<u64, usize>,
) -> Option<usize> {
    let texture = texture.as_ref()?;
    let key = texture.texture() as *const wgpu::Texture as usize as u64;
    if let Some(index) = texture_map.get(&key).copied() {
        return Some(index);
    }
    let index = textures.len();
    textures.push(texture.clone());
    texture_map.insert(key, index);
    Some(index)
}
