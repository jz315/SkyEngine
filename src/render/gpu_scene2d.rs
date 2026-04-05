//! GPU-resident authoritative scene data for the 2D renderer.

use crate::gpu::GpuContext;
use crate::render::light::Light2D;
use crate::render::pipeline::prepared::{PreparedRenderWorld2D, PreparedView2D, SpriteDrawSpan};
use crate::render::pipeline::SceneCache2D;
use crate::render::Texture;

const INITIAL_SPRITE_SLOT_CAPACITY: usize = 1024;
const INITIAL_LIGHT_SLOT_CAPACITY: usize = 256;
const INITIAL_VISIBLE_SPRITE_CAPACITY: usize = 1024;
const INITIAL_VISIBLE_LIGHT_CAPACITY: usize = 256;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuSpriteRecord {
    pub transform: [f32; 4],
    pub rotation: [f32; 4],
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuLightRecord {
    pub pos_radius: [f32; 4],
    pub color: [f32; 4],
    pub falloff: [f32; 4],
}

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

    pub(crate) fn upload_scene_frame(
        &mut self,
        ctx: &GpuContext,
        scene: &mut SceneCache2D,
        prepared: &PreparedRenderWorld2D,
    ) {
        let rewrite_sprite_table =
            self.ensure_sprite_table_capacity(ctx, scene.sprite_slot_capacity());
        let rewrite_light_table =
            self.ensure_light_table_capacity(ctx, scene.light_slot_capacity());

        self.dirty_sprite_slot_uploads = self.upload_sprite_table(ctx, scene, rewrite_sprite_table);
        self.dirty_light_slot_uploads = self.upload_light_table(ctx, scene, rewrite_light_table);

        self.ensure_visible_sprite_capacity(ctx, prepared.visible_sprite_slots.len());
        self.ensure_visible_light_capacity(ctx, prepared.visible_light_slots.len());

        if !prepared.visible_sprite_slots.is_empty() {
            ctx.queue().write_buffer(
                &self.visible_sprite_index_buffer,
                0,
                bytemuck::cast_slice(&prepared.visible_sprite_slots),
            );
        }
        if !prepared.visible_light_slots.is_empty() {
            ctx.queue().write_buffer(
                &self.visible_light_index_buffer,
                0,
                bytemuck::cast_slice(&prepared.visible_light_slots),
            );
        }

        self.views.clone_from(&prepared.views);
        self.draw_spans.clone_from(&prepared.draw_spans);
        self.textures.clone_from(&prepared.textures);
        self.sprite_count = prepared.sprite_count;
        self.light_count = prepared.light_count;
        self.visible_sprite_upload_count = prepared.visible_sprite_slots.len();
        self.visible_light_upload_count = prepared.visible_light_slots.len();

        scene.clear_dirty_tracking();
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

    fn ensure_sprite_table_capacity(&mut self, ctx: &GpuContext, required: usize) -> bool {
        if required <= self.sprite_table_capacity {
            return false;
        }
        self.sprite_table_capacity = required
            .next_power_of_two()
            .max(INITIAL_SPRITE_SLOT_CAPACITY);
        self.sprite_table_buffer =
            create_sprite_table_buffer(ctx, self.sprite_table_capacity, "gpu_scene2d_sprite_table");
        self.sprite_table_version = self.sprite_table_version.wrapping_add(1);
        true
    }

    fn ensure_light_table_capacity(&mut self, ctx: &GpuContext, required: usize) -> bool {
        if required <= self.light_table_capacity {
            return false;
        }
        self.light_table_capacity = required
            .next_power_of_two()
            .max(INITIAL_LIGHT_SLOT_CAPACITY);
        self.light_table_buffer =
            create_light_table_buffer(ctx, self.light_table_capacity, "gpu_scene2d_light_table");
        self.light_table_version = self.light_table_version.wrapping_add(1);
        true
    }

    fn ensure_visible_sprite_capacity(&mut self, ctx: &GpuContext, required: usize) {
        if required <= self.visible_sprite_index_capacity {
            return;
        }
        self.visible_sprite_index_capacity = required
            .next_power_of_two()
            .max(INITIAL_VISIBLE_SPRITE_CAPACITY);
        self.visible_sprite_index_buffer = create_visible_index_buffer(
            ctx,
            self.visible_sprite_index_capacity,
            "gpu_scene2d_visible_sprite_indices",
        );
    }

    fn ensure_visible_light_capacity(&mut self, ctx: &GpuContext, required: usize) {
        if required <= self.visible_light_index_capacity {
            return;
        }
        self.visible_light_index_capacity = required
            .next_power_of_two()
            .max(INITIAL_VISIBLE_LIGHT_CAPACITY);
        self.visible_light_index_buffer = create_visible_index_buffer(
            ctx,
            self.visible_light_index_capacity,
            "gpu_scene2d_visible_light_indices",
        );
    }

    fn upload_sprite_table(
        &mut self,
        ctx: &GpuContext,
        scene: &SceneCache2D,
        rewrite_full_table: bool,
    ) -> usize {
        if rewrite_full_table {
            self.sprite_upload_scratch.clear();
            self.sprite_upload_scratch.extend(
                (0..scene.sprite_slot_capacity()).map(|slot| sprite_record_for_slot(scene, slot)),
            );
            if !self.sprite_upload_scratch.is_empty() {
                ctx.queue().write_buffer(
                    &self.sprite_table_buffer,
                    0,
                    bytemuck::cast_slice(&self.sprite_upload_scratch),
                );
            }
            return self.sprite_upload_scratch.len();
        }

        if scene.dirty_sprite_slots().is_empty() {
            return 0;
        }

        self.sprite_dirty_scratch.clear();
        self.sprite_dirty_scratch
            .extend(scene.dirty_sprite_slots().iter().map(|&slot| slot as usize));
        self.sprite_dirty_scratch.sort_unstable();

        let mut uploaded = 0usize;
        let record_size = std::mem::size_of::<GpuSpriteRecord>() as u64;
        let mut run_start = 0usize;
        while run_start < self.sprite_dirty_scratch.len() {
            let first_slot = self.sprite_dirty_scratch[run_start];
            let mut run_end = run_start + 1;
            while run_end < self.sprite_dirty_scratch.len()
                && self.sprite_dirty_scratch[run_end] == self.sprite_dirty_scratch[run_end - 1] + 1
            {
                run_end += 1;
            }

            self.sprite_upload_scratch.clear();
            self.sprite_upload_scratch.extend(
                self.sprite_dirty_scratch[run_start..run_end]
                    .iter()
                    .map(|&slot| sprite_record_for_slot(scene, slot)),
            );
            ctx.queue().write_buffer(
                &self.sprite_table_buffer,
                first_slot as u64 * record_size,
                bytemuck::cast_slice(&self.sprite_upload_scratch),
            );
            uploaded += run_end - run_start;
            run_start = run_end;
        }

        uploaded
    }

    fn upload_light_table(
        &mut self,
        ctx: &GpuContext,
        scene: &SceneCache2D,
        rewrite_full_table: bool,
    ) -> usize {
        if rewrite_full_table {
            self.light_upload_scratch.clear();
            self.light_upload_scratch.extend(
                (0..scene.light_slot_capacity()).map(|slot| light_record_for_slot(scene, slot)),
            );
            if !self.light_upload_scratch.is_empty() {
                ctx.queue().write_buffer(
                    &self.light_table_buffer,
                    0,
                    bytemuck::cast_slice(&self.light_upload_scratch),
                );
            }
            return self.light_upload_scratch.len();
        }

        if scene.dirty_light_slots().is_empty() {
            return 0;
        }

        self.light_dirty_scratch.clear();
        self.light_dirty_scratch
            .extend(scene.dirty_light_slots().iter().map(|&slot| slot as usize));
        self.light_dirty_scratch.sort_unstable();

        let mut uploaded = 0usize;
        let record_size = std::mem::size_of::<GpuLightRecord>() as u64;
        let mut run_start = 0usize;
        while run_start < self.light_dirty_scratch.len() {
            let first_slot = self.light_dirty_scratch[run_start];
            let mut run_end = run_start + 1;
            while run_end < self.light_dirty_scratch.len()
                && self.light_dirty_scratch[run_end] == self.light_dirty_scratch[run_end - 1] + 1
            {
                run_end += 1;
            }

            self.light_upload_scratch.clear();
            self.light_upload_scratch.extend(
                self.light_dirty_scratch[run_start..run_end]
                    .iter()
                    .map(|&slot| light_record_for_slot(scene, slot)),
            );
            ctx.queue().write_buffer(
                &self.light_table_buffer,
                first_slot as u64 * record_size,
                bytemuck::cast_slice(&self.light_upload_scratch),
            );
            uploaded += run_end - run_start;
            run_start = run_end;
        }

        uploaded
    }
}

fn sprite_record_for_slot(scene: &SceneCache2D, slot: usize) -> GpuSpriteRecord {
    let Some(item) = scene.sprite_item(slot) else {
        return GpuSpriteRecord::default();
    };
    let (sin_a, cos_a) = item.transform.rotation.sin_cos();
    GpuSpriteRecord {
        transform: [
            item.transform.x,
            item.transform.y,
            item.sprite.width * item.transform.scale_x,
            item.sprite.height * item.transform.scale_y,
        ],
        rotation: [sin_a, cos_a, 0.0, 0.0],
        color: item.sprite.color.to_array(),
        uv_rect: item.sprite.uv,
    }
}

fn light_record_for_slot(scene: &SceneCache2D, slot: usize) -> GpuLightRecord {
    let Some(item) = scene.light_item(slot) else {
        return GpuLightRecord::default();
    };
    let light = Light2D::new(item.transform.x, item.transform.y, item.light.radius)
        .intensity(item.light.intensity)
        .color(item.light.color)
        .temperature(item.light.temperature)
        .falloff(item.light.falloff);
    GpuLightRecord {
        pos_radius: [light.position[0], light.position[1], light.radius, 0.0],
        color: light.effective_color(),
        falloff: [light.falloff.max(0.001), 50.0, 0.0, 0.0],
    }
}

fn create_sprite_table_buffer(
    ctx: &GpuContext,
    capacity: usize,
    label: &'static str,
) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity.max(1) * std::mem::size_of::<GpuSpriteRecord>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_light_table_buffer(
    ctx: &GpuContext,
    capacity: usize,
    label: &'static str,
) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity.max(1) * std::mem::size_of::<GpuLightRecord>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_visible_index_buffer(
    ctx: &GpuContext,
    capacity: usize,
    label: &'static str,
) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity.max(1) * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
