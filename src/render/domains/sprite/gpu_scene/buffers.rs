use crate::gpu::GpuContext;

use super::records::{GpuLightRecord, GpuSpriteRecord};
use super::GpuScene2D;

pub(super) const INITIAL_SPRITE_SLOT_CAPACITY: usize = 1024;
pub(super) const INITIAL_LIGHT_SLOT_CAPACITY: usize = 256;
pub(super) const INITIAL_VISIBLE_SPRITE_CAPACITY: usize = 1024;
pub(super) const INITIAL_VISIBLE_LIGHT_CAPACITY: usize = 256;

impl GpuScene2D {
    pub(super) fn ensure_sprite_table_capacity(
        &mut self,
        ctx: &GpuContext,
        required: usize,
    ) -> bool {
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

    pub(super) fn ensure_light_table_capacity(
        &mut self,
        ctx: &GpuContext,
        required: usize,
    ) -> bool {
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

    pub(super) fn ensure_visible_sprite_capacity(&mut self, ctx: &GpuContext, required: usize) {
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

    pub(super) fn ensure_visible_light_capacity(&mut self, ctx: &GpuContext, required: usize) {
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
}

pub(super) fn create_sprite_table_buffer(
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

pub(super) fn create_light_table_buffer(
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

pub(super) fn create_visible_index_buffer(
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
