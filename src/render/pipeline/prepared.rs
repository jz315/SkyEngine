//! Prepared multi-view 2D render data.

use rustc_hash::FxHashMap;

use crate::render::core::camera::Camera2D;
use crate::render::ecs::{RenderSettings2D, ViewportRect};
use crate::render::pipeline::{SceneCache2D, SceneLightItem, SceneSpriteItem};
use crate::render::Texture;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpriteDrawSpan {
    pub first_instance: u32,
    pub instance_count: u32,
    pub texture_index: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PreparedView2D {
    pub order: i32,
    pub viewport: ViewportRect,
    pub layer_mask: u32,
    pub camera: Camera2D,
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
    pub settings: RenderSettings2D,
    pub views: Vec<PreparedView2D>,
    pub visible_sprite_slots: Vec<u32>,
    pub visible_light_slots: Vec<u32>,
    pub draw_spans: Vec<SpriteDrawSpan>,
    pub textures: Vec<Texture>,
    pub sprite_count: usize,
    pub light_count: usize,
    visible_sprite_scratch: Vec<u32>,
}

impl PreparedRenderWorld2D {
    pub fn new() -> Self {
        Self {
            settings: RenderSettings2D::default(),
            views: Vec::with_capacity(4),
            visible_sprite_slots: Vec::with_capacity(1024),
            visible_light_slots: Vec::with_capacity(256),
            draw_spans: Vec::with_capacity(128),
            textures: Vec::with_capacity(32),
            sprite_count: 0,
            light_count: 0,
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
        self.visible_sprite_scratch.clear();
    }

    pub fn prepare_scene(&mut self, scene: &mut SceneCache2D, surface_size: [u32; 2]) {
        self.reset();
        self.settings = scene.settings;
        scene.ensure_sorted_sprites();
        let sorted_sprite_slots = scene.sorted_sprite_slots();

        let mut views = scene.views.clone();
        views.sort_by(|lhs, rhs| lhs.render.order.cmp(&rhs.render.order));

        let mut texture_map = FxHashMap::<u64, usize>::default();
        for scene_view in views {
            let viewport = scene_view.render.viewport.clamp_to_surface(surface_size);
            let mut camera = scene_view.camera;
            camera.set_viewport(viewport.width as f32, viewport.height as f32);

            let sprite_offset = self.visible_sprite_slots.len() as u32;
            let draw_span_offset = self.draw_spans.len();
            prepare_view_sprites(
                &mut self.visible_sprite_scratch,
                &mut self.visible_sprite_slots,
                &mut self.draw_spans,
                &mut self.textures,
                &mut texture_map,
                &camera,
                scene_view.render.layer_mask,
                sorted_sprite_slots,
                scene,
            );
            let sprite_count = self.visible_sprite_slots.len() as u32 - sprite_offset;
            let draw_span_count = self.draw_spans.len() - draw_span_offset;

            let light_offset = self.visible_light_slots.len() as u32;
            prepare_view_lights(
                &mut self.visible_light_slots,
                &camera,
                scene_view.render.layer_mask,
                scene,
            );
            let light_count = self.visible_light_slots.len() as u32 - light_offset;

            self.views.push(PreparedView2D {
                order: scene_view.render.order,
                viewport,
                layer_mask: scene_view.render.layer_mask,
                camera,
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
    camera: &Camera2D,
    layer_mask: u32,
    sorted_sprite_slots: &[usize],
    scene: &SceneCache2D,
) {
    visible_slots_scratch.clear();
    visible_slots_scratch.extend(sorted_sprite_slots.iter().copied().filter_map(|slot| {
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

    let mut current_span_texture = None;
    let mut current_span_start = out_visible_slots.len() as u32;
    let mut current_span_count = 0u32;

    for &slot in visible_slots_scratch.iter() {
        let item = scene
            .sprite_item(slot as usize)
            .expect("visible sprite slots must refer to occupied entries");
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
    camera: &Camera2D,
    layer_mask: u32,
    scene: &SceneCache2D,
) {
    for &slot in scene.active_light_slots() {
        let Some(item) = scene.light_item(slot) else {
            continue;
        };
        if !item.light.visible || (item.light.layer_mask & layer_mask) == 0 {
            continue;
        }
        if !light_visible_in_camera(camera, item) {
            continue;
        }

        out_visible_slots.push(slot as u32);
    }
}

fn sprite_visible_in_camera(camera: &Camera2D, item: &SceneSpriteItem) -> bool {
    let (left, right, bottom, top) = camera_world_bounds(camera);
    let half_w = item.sprite.width.abs() * item.transform.scale_x.abs() * 0.5;
    let half_h = item.sprite.height.abs() * item.transform.scale_y.abs() * 0.5;
    let sprite_left = item.transform.x - half_w;
    let sprite_right = item.transform.x + half_w;
    let sprite_bottom = item.transform.y - half_h;
    let sprite_top = item.transform.y + half_h;
    sprite_right >= left && sprite_left <= right && sprite_top >= bottom && sprite_bottom <= top
}

fn light_visible_in_camera(camera: &Camera2D, item: &SceneLightItem) -> bool {
    let (left, right, bottom, top) = camera_world_bounds(camera);
    let radius = item.light.radius.abs();
    let light_left = item.transform.x - radius;
    let light_right = item.transform.x + radius;
    let light_bottom = item.transform.y - radius;
    let light_top = item.transform.y + radius;
    light_right >= left && light_left <= right && light_top >= bottom && light_bottom <= top
}

fn camera_world_bounds(camera: &Camera2D) -> (f32, f32, f32, f32) {
    let hw = camera.viewport_width().max(f32::EPSILON) * 0.5 / camera.zoom.max(f32::EPSILON);
    let hh = camera.viewport_height().max(f32::EPSILON) * 0.5 / camera.zoom.max(f32::EPSILON);
    (
        camera.position[0] - hw,
        camera.position[0] + hw,
        camera.position[1] - hh,
        camera.position[1] + hh,
    )
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
