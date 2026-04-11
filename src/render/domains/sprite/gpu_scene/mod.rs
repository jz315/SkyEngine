//! GPU-resident authoritative scene data for the 2D renderer.

mod buffers;
mod records;
mod upload;

use crate::gpu::GpuContext;
use crate::render::Texture;

use self::buffers::{
    create_light_table_buffer, create_sprite_table_buffer, create_visible_index_buffer,
    INITIAL_LIGHT_SLOT_CAPACITY, INITIAL_SPRITE_SLOT_CAPACITY, INITIAL_VISIBLE_LIGHT_CAPACITY,
    INITIAL_VISIBLE_SPRITE_CAPACITY,
};
use self::records::{GpuLightRecord, GpuSpriteRecord};
use super::prepared::{PreparedView2D, SpriteDrawSpan};

pub struct GpuScene2D {
    sprite_table_buffer: wgpu::Buffer,
    sprite_table_capacity: usize,
    sprite_table_version: u64,
    light_table_buffer: wgpu::Buffer,
    light_table_capacity: usize,
    light_table_version: u64,
    visible_sprite_index_buffer: wgpu::Buffer,
    visible_sprite_index_capacity: usize,
    visible_light_index_buffer: wgpu::Buffer,
    visible_light_index_capacity: usize,
    views: Vec<PreparedView2D>,
    draw_spans: Vec<SpriteDrawSpan>,
    textures: Vec<Texture>,
    sprite_count: usize,
    light_count: usize,
    dirty_sprite_slot_uploads: usize,
    dirty_light_slot_uploads: usize,
    visible_sprite_upload_count: usize,
    visible_light_upload_count: usize,
    sprite_dirty_scratch: Vec<usize>,
    light_dirty_scratch: Vec<usize>,
    sprite_upload_scratch: Vec<GpuSpriteRecord>,
    light_upload_scratch: Vec<GpuLightRecord>,
}

impl GpuScene2D {
    pub fn new(ctx: &GpuContext) -> Self {
        Self {
            sprite_table_buffer: create_sprite_table_buffer(
                ctx,
                INITIAL_SPRITE_SLOT_CAPACITY,
                "gpu_scene2d_sprite_table",
            ),
            sprite_table_capacity: INITIAL_SPRITE_SLOT_CAPACITY,
            sprite_table_version: 1,
            light_table_buffer: create_light_table_buffer(
                ctx,
                INITIAL_LIGHT_SLOT_CAPACITY,
                "gpu_scene2d_light_table",
            ),
            light_table_capacity: INITIAL_LIGHT_SLOT_CAPACITY,
            light_table_version: 1,
            visible_sprite_index_buffer: create_visible_index_buffer(
                ctx,
                INITIAL_VISIBLE_SPRITE_CAPACITY,
                "gpu_scene2d_visible_sprite_indices",
            ),
            visible_sprite_index_capacity: INITIAL_VISIBLE_SPRITE_CAPACITY,
            visible_light_index_buffer: create_visible_index_buffer(
                ctx,
                INITIAL_VISIBLE_LIGHT_CAPACITY,
                "gpu_scene2d_visible_light_indices",
            ),
            visible_light_index_capacity: INITIAL_VISIBLE_LIGHT_CAPACITY,
            views: Vec::with_capacity(4),
            draw_spans: Vec::with_capacity(128),
            textures: Vec::with_capacity(32),
            sprite_count: 0,
            light_count: 0,
            dirty_sprite_slot_uploads: 0,
            dirty_light_slot_uploads: 0,
            visible_sprite_upload_count: 0,
            visible_light_upload_count: 0,
            sprite_dirty_scratch: Vec::with_capacity(64),
            light_dirty_scratch: Vec::with_capacity(32),
            sprite_upload_scratch: Vec::with_capacity(128),
            light_upload_scratch: Vec::with_capacity(64),
        }
    }

    #[inline]
    pub(crate) fn sprite_table_buffer(&self) -> &wgpu::Buffer {
        &self.sprite_table_buffer
    }

    #[inline]
    pub(crate) fn light_table_buffer(&self) -> &wgpu::Buffer {
        &self.light_table_buffer
    }

    #[inline]
    pub(crate) fn sprite_table_version(&self) -> u64 {
        self.sprite_table_version
    }

    #[inline]
    pub(crate) fn light_table_version(&self) -> u64 {
        self.light_table_version
    }

    #[inline]
    pub(crate) fn visible_sprite_index_buffer(&self) -> &wgpu::Buffer {
        &self.visible_sprite_index_buffer
    }

    #[inline]
    pub(crate) fn visible_light_index_buffer(&self) -> &wgpu::Buffer {
        &self.visible_light_index_buffer
    }

    #[inline]
    pub(crate) fn views(&self) -> &[PreparedView2D] {
        &self.views
    }

    #[inline]
    pub(crate) fn draw_spans_for_view(&self, view: PreparedView2D) -> &[SpriteDrawSpan] {
        &self.draw_spans[view.draw_span_range()]
    }

    #[inline]
    pub(crate) fn textures(&self) -> &[Texture] {
        &self.textures
    }

    #[inline]
    pub fn sprite_count(&self) -> usize {
        self.sprite_count
    }

    #[inline]
    pub fn light_count(&self) -> usize {
        self.light_count
    }

    #[inline]
    pub fn dirty_sprite_slot_uploads(&self) -> usize {
        self.dirty_sprite_slot_uploads
    }

    #[inline]
    pub fn dirty_light_slot_uploads(&self) -> usize {
        self.dirty_light_slot_uploads
    }

    #[inline]
    pub fn visible_sprite_upload_count(&self) -> usize {
        self.visible_sprite_upload_count
    }

    #[inline]
    pub fn visible_light_upload_count(&self) -> usize {
        self.visible_light_upload_count
    }
}
