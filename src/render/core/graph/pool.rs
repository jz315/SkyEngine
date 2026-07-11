//! Transient resource pools for the render graph.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::gpu::{RenderTarget, RenderTargetDescriptor};

use super::types::TextureFormat;

const UNUSED_FRAME_RETENTION: u64 = 1;

struct PoolBucket<T> {
    resources: Vec<T>,
    last_used_frame: u64,
}

impl<T> PoolBucket<T> {
    fn new(last_used_frame: u64) -> Self {
        Self {
            resources: Vec::new(),
            last_used_frame,
        }
    }
}

// ── Texture pool ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PoolKey {
    pub format: TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub width: u32,
    pub height: u32,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub array_layer_count: u32,
}

pub(crate) struct TransientPool {
    pool: FxHashMap<PoolKey, PoolBucket<RenderTarget>>,
    frame_index: u64,
}

impl TransientPool {
    pub fn new() -> Self {
        Self {
            pool: FxHashMap::default(),
            frame_index: 0,
        }
    }

    /// Advance pool age and release sizes/formats that have not been reused
    /// recently. This prevents resize or dynamic-pipeline churn from retaining
    /// one full GPU allocation for every historical descriptor forever.
    pub fn begin_frame(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1);
        let frame_index = self.frame_index;
        self.pool.retain(|_, bucket| {
            frame_index.wrapping_sub(bucket.last_used_frame) <= UNUSED_FRAME_RETENTION
        });
    }

    pub fn acquire(
        &mut self,
        ctx: &GpuContext,
        key: PoolKey,
        label: Cow<'static, str>,
    ) -> RenderTarget {
        self.acquire_with_label(ctx, key, || label)
    }

    pub fn acquire_with_label(
        &mut self,
        ctx: &GpuContext,
        key: PoolKey,
        label: impl FnOnce() -> Cow<'static, str>,
    ) -> RenderTarget {
        if let Some(bucket) = self.pool.get_mut(&key) {
            bucket.last_used_frame = self.frame_index;
            if let Some(target) = bucket.resources.pop() {
                return target;
            }
        }
        RenderTarget::from_descriptor(
            ctx,
            RenderTargetDescriptor::new(key.width, key.height, key.format)
                .usage(key.usage)
                .sample_count(key.sample_count)
                .mip_level_count(key.mip_level_count)
                .array_layer_count(key.array_layer_count)
                .label(label()),
        )
    }

    pub fn release(&mut self, key: PoolKey, target: RenderTarget) {
        let bucket = self
            .pool
            .entry(key)
            .or_insert_with(|| PoolBucket::new(self.frame_index));
        bucket.last_used_frame = self.frame_index;
        bucket.resources.push(target);
    }

    pub fn destroy_all(&mut self) {
        self.pool.clear();
    }

    #[cfg(test)]
    pub fn first_texture_ptr(&self) -> Option<usize> {
        self.pool
            .values()
            .flat_map(|bucket| bucket.resources.iter())
            .next()
            .map(|target| std::ptr::from_ref(target.texture()) as usize)
    }
}

// ── Buffer pool ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BufferPoolKey {
    pub size_bytes: u64,
    pub usage: wgpu::BufferUsages,
}

pub(crate) struct TransientBufferPool {
    pool: FxHashMap<BufferPoolKey, PoolBucket<wgpu::Buffer>>,
    frame_index: u64,
}

impl TransientBufferPool {
    pub fn new() -> Self {
        Self {
            pool: FxHashMap::default(),
            frame_index: 0,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1);
        let frame_index = self.frame_index;
        self.pool.retain(|_, bucket| {
            frame_index.wrapping_sub(bucket.last_used_frame) <= UNUSED_FRAME_RETENTION
        });
    }

    pub fn acquire(
        &mut self,
        ctx: &GpuContext,
        key: BufferPoolKey,
        label: Cow<'static, str>,
    ) -> wgpu::Buffer {
        if let Some(bucket) = self.pool.get_mut(&key) {
            bucket.last_used_frame = self.frame_index;
            if let Some(buffer) = bucket.resources.pop() {
                return buffer;
            }
        }
        ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: key.size_bytes,
            usage: key.usage,
            mapped_at_creation: false,
        })
    }

    pub fn release(&mut self, key: BufferPoolKey, buffer: wgpu::Buffer) {
        let bucket = self
            .pool
            .entry(key)
            .or_insert_with(|| PoolBucket::new(self.frame_index));
        bucket.last_used_frame = self.frame_index;
        bucket.resources.push(buffer);
    }

    pub fn destroy_all(&mut self) {
        self.pool.clear();
    }
}
